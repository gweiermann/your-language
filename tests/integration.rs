use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use your_language::{
    compile_language, compile_sources, frontend::parse_yl, load_compiled_language, parse_named,
};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}
fn golden(name: &str, value: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(value).unwrap() + "\n";
    let path = fixture(&format!("tests/golden/{name}.json"));
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &json).unwrap();
    }
    assert_eq!(json, fs::read_to_string(path).unwrap(), "Golden {name}");
}
#[test]
fn yl_ast_goldens_preserve_the_entire_acceptance_language() {
    for name in ["lexical", "minijs"] {
        let relative = format!("documentation/target-syntax/{name}.yl");
        golden(
            &format!("{name}.yl-ast"),
            &parse_yl(&relative, &fs::read_to_string(fixture(&relative)).unwrap()).unwrap(),
        );
    }
}
#[test]
fn public_api_compile_serialize_reload_parse_exact_ast_and_diagnostics() {
    let language = compile_language(fixture("tests/fixtures/syntax-v0/language.yl")).unwrap();
    let bytes = language.to_bytes().unwrap();
    golden("normalized-language", &language);
    let language = load_compiled_language(&bytes).unwrap();
    assert_eq!(bytes, language.to_bytes().unwrap());
    let source = fs::read_to_string(fixture("tests/fixtures/syntax-v0/program.txt")).unwrap();
    let result = parse_named(&language, "program.txt", &source);
    assert!(!result.has_errors());
    assert!(result.ast.is_some());
    golden("syntax-v0-ast", &result);
    let invalid = parse_named(&language, "invalid.txt", "let x = 1 + @");
    assert!(invalid.has_errors());
    golden("syntax-v0-diagnostics", &invalid);
}
#[test]
fn cli_check_compile_parse_and_error_exit_codes() {
    let entry = fixture("tests/fixtures/syntax-v0/language.yl");
    let output = fixture("target/cli-integration.ylc");
    let executable = env!("CARGO_BIN_EXE_yl");
    assert!(Command::new(executable)
        .arg("check")
        .arg(&entry)
        .output()
        .unwrap()
        .status
        .success());
    assert!(Command::new(executable)
        .arg("compile")
        .arg(&entry)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap()
        .status
        .success());
    let parsed = Command::new(executable)
        .arg("parse")
        .arg(&output)
        .arg(fixture("tests/fixtures/syntax-v0/program.txt"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(parsed.status.success());
    let json: serde_json::Value = serde_json::from_slice(&parsed.stdout).unwrap();
    assert_eq!(json["ast"]["type"], "Program");
    assert_eq!(json["diagnostics"], serde_json::json!([]));
    let failed = Command::new(executable)
        .arg("parse")
        .arg(&output)
        .arg(fixture("tests/fixtures/minijs/unexpected-token.js"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    let json: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert!(json["ast"].is_null());
    assert_eq!(json["diagnostics"][0]["code"], "parse.unexpected_token");
}
#[test]
fn cli_json_errors_have_ast_diagnostics_and_file_context() {
    let executable = env!("CARGO_BIN_EXE_yl");
    let program = fixture("tests/fixtures/syntax-v0/program.txt");
    let corrupt = fixture("target/cli-corrupt.ylc");
    fs::write(&corrupt, "{invalid").unwrap();
    let valid = fixture("target/cli-errors.ylc");
    fs::write(
        &valid,
        compile_language(fixture("tests/fixtures/syntax-v0/language.yl"))
            .unwrap()
            .to_bytes()
            .unwrap(),
    )
    .unwrap();
    let absent_artifact = fixture("tests/fixtures/syntax-v0/absent.ylc");
    let absent_source = fixture("tests/fixtures/syntax-v0/absent-source.txt");
    for (artifact, source, code, file) in [
        (&corrupt, &program, "yl.artifact", &corrupt),
        (&absent_artifact, &program, "yl.io", &absent_artifact),
        (&valid, &absent_source, "yl.io", &absent_source),
    ] {
        let failed = Command::new(executable)
            .arg("parse")
            .arg(artifact)
            .arg(source)
            .arg("--json")
            .output()
            .unwrap();
        assert!(!failed.status.success());
        assert!(failed.stderr.is_empty());
        let json: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
        assert!(json["ast"].is_null());
        assert_eq!(json["diagnostics"][0]["code"], code);
        assert_eq!(
            json["diagnostics"][0]["primary"]["file"],
            file.to_string_lossy().as_ref()
        );
    }
}
#[test]
fn oversized_left_growing_ast_returns_json_error_instead_of_crashing() {
    let artifact = fixture("target/cli-depth-limit.ylc");
    fs::write(
        &artifact,
        compile_language(fixture("documentation/target-syntax/minijs.yl"))
            .unwrap()
            .to_bytes()
            .unwrap(),
    )
    .unwrap();
    let source = fixture("target/cli-depth-limit.js");
    fs::write(&source, format!("let x = 1{} +", " + 1".repeat(1200))).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("parse")
        .arg(artifact)
        .arg(source)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(json["ast"].is_null());
    assert_eq!(json["diagnostics"][0]["code"], "parse.resource_limit");
}
#[test]
fn minijs_compiles_reloads_and_parses_exact_ast_and_negative_diagnostics() {
    let language = compile_language(fixture("documentation/target-syntax/minijs.yl")).unwrap();
    golden("minijs.normalized", &language);
    let bytes = language.to_bytes().unwrap();
    let loaded = load_compiled_language(&bytes).unwrap();
    assert_eq!(bytes, loaded.to_bytes().unwrap());
    let program = fs::read_to_string(fixture("tests/fixtures/minijs/program.js")).unwrap();
    let result = parse_named(&loaded, "program.js", &program);
    assert!(result.diagnostics.is_empty());
    let ast = serde_json::to_value(&result).unwrap();
    assert_eq!(ast["ast"]["type"], "Program");
    assert_eq!(
        ast["ast"]["fields"]["statements"].as_array().unwrap().len(),
        4
    );
    let statements = &ast["ast"]["fields"]["statements"];
    assert_eq!(statements[0]["type"], "Statement::VariableDeclaration");
    assert_eq!(
        statements[0]["fields"]["initializer"]["type"],
        "Expression::Sum"
    );
    assert_eq!(
        statements[0]["fields"]["initializer"]["fields"]["right"]["type"],
        "Expression::Product"
    );
    assert_eq!(
        statements[1]["fields"]["initializer"]["fields"]["callee"]["type"],
        "Expression::Member"
    );
    assert_eq!(
        statements[1]["fields"]["initializer"]["fields"]["arguments"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        statements[2]["fields"]["parameters"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        statements[3]["fields"]["initializer"]["fields"]["right"]["type"],
        "Expression::Power"
    );
    golden("minijs.ast", &result);
    let source = fs::read_to_string(fixture("tests/fixtures/minijs/features.js")).unwrap();
    let features = parse_named(&loaded, "features.js", &source);
    assert!(features.diagnostics.is_empty());
    golden("minijs.features.ast", &features);
    let json = serde_json::to_value(&features).unwrap();
    let statements = &json["ast"]["fields"]["statements"];
    assert_eq!(
        statements[0]["fields"]["initializer"]["type"],
        "Expression::String"
    );
    assert_eq!(
        statements[1]["fields"]["initializer"]["type"],
        "Expression::Member"
    );
    assert_eq!(
        statements[1]["fields"]["initializer"]["fields"]["object"]["type"],
        "Expression::Call"
    );
    assert_eq!(
        statements[2]["fields"]["initializer"]["fields"]["argument"]["fields"]["argument"]["type"],
        "Expression::Unary"
    );
    assert_eq!(
        statements[3]["fields"]["parameters"][0]["fields"]["default"]["type"],
        "Expression::Number"
    );
    assert!(
        statements[3]["fields"]["body"]["fields"]["statements"][0]["fields"]["value"].is_null()
    );
    assert!(parse_named(&loaded, "boundary.js", "letter\noutlet")
        .diagnostics
        .is_empty());
    for name in [
        "malformed-expression",
        "missing-delimiter",
        "invalid-separator",
        "keyword-boundary",
        "unexpected-token",
    ] {
        let source =
            fs::read_to_string(fixture(&format!("tests/fixtures/minijs/{name}.js"))).unwrap();
        let result = parse_named(&loaded, &format!("{name}.js"), &source);
        assert!(result.has_errors());
        assert!(result.ast.is_none());
        golden(&format!("minijs.{name}.diagnostics"), &result);
    }
    let output = fixture("target/cli-minijs.ylc");
    let cli = env!("CARGO_BIN_EXE_yl");
    let entry = fixture("documentation/target-syntax/minijs.yl");
    assert!(Command::new(cli)
        .arg("check")
        .arg(&entry)
        .output()
        .unwrap()
        .status
        .success());
    assert!(Command::new(cli)
        .arg("compile")
        .arg(&entry)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap()
        .status
        .success());
    let result = Command::new(cli)
        .arg("parse")
        .arg(&output)
        .arg(fixture("tests/fixtures/minijs/program.js"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(result.status.success());
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["diagnostics"], serde_json::json!([]));
}
#[test]
fn all_separated_by_modes_match_the_documented_language_and_flat_list_values() {
    for q in ["*", "+"] {
        for mode in ["none", "optional", "required"] {
            let source=format!("import {{ separatedBy }} from \"std/parser\" node Item = value: /x/ node P = items: Item{q} |> separatedBy(\",\", trailing=.{mode}) entry P");
            let sources = BTreeMap::from([("main.yl".into(), source)]);
            let language = compile_sources("main.yl", &sources).unwrap();
            for (source, length) in [
                ("", 0),
                ("x", 1),
                ("x,", 1),
                ("x,x", 2),
                ("x,x,", 2),
                ("x,,x", 2),
                (",x", 1),
            ] {
                let valid = if source.is_empty() {
                    q == "*"
                } else if source.contains(",,") || source.starts_with(',') {
                    false
                } else {
                    match mode {
                        "none" => !source.ends_with(','),
                        "required" => source.ends_with(','),
                        _ => true,
                    }
                };
                let result = your_language::parse(&language, source);
                assert_eq!(result.ast.is_some(), valid, "{q} {mode} {source:?}");
                if valid {
                    let ast = result.ast.unwrap();
                    let your_language::AstValue::List(items) = &ast.fields["items"] else {
                        panic!()
                    };
                    assert_eq!(items.len(), length);
                    assert!(items
                        .iter()
                        .all(|v| matches!(v,your_language::AstValue::Node(n) if n.kind=="Item")));
                } else {
                    assert!(result.has_errors());
                }
            }
        }
    }
}

#[test]
fn agreed_decisions_have_frontend_normalized_runtime_and_diagnostic_goldens() {
    let relative = "tests/fixtures/decisions/language.yl";
    let source = fs::read_to_string(fixture(relative)).unwrap();
    golden("decisions.yl-ast", &parse_yl(relative, &source).unwrap());
    let language = compile_language(fixture(relative)).unwrap();
    golden("decisions.normalized", &language);
    golden("decisions.compile-diagnostics", &language.diagnostics());
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    golden(
        "decisions.ast",
        &parse_named(&language, "decisions.txt", "Alice:xxx"),
    );
    golden(
        "decisions.absent",
        &parse_named(&language, "decisions.txt", ":"),
    );
    let invalid = source.replace("wrap(.b, .a)", "wrap(.a, .b)");
    golden(
        "decisions.error",
        &compile_sources(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), invalid)]),
        )
        .unwrap_err(),
    );
}

#[test]
fn cli_pipe_diagnostics_underline_caller_arguments_before_definition() {
    let entry = fixture("tests/fixtures/decisions/invalid-combination.yl");
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("check")
        .arg(entry)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains("This combination is not allowed."));
    assert!(text.contains("^^  ^^"));
    let caller = text.find("node Program").unwrap();
    let definition = text.find("error(\"This combination").unwrap();
    assert!(caller < definition, "{text}");
}

#[test]
fn cli_source_underlines_align_with_the_marked_columns() {
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("check")
        .arg(fixture("tests/fixtures/decisions/invalid-combination.yl"))
        .output()
        .unwrap();
    let text = String::from_utf8(output.stderr).unwrap();
    let lines: Vec<_> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.contains("node Program") {
            let underline = lines[index + 1];
            assert_eq!(underline.find('^'), line.find(".a"));
            assert_eq!(
                underline.rfind('^'),
                line.find(".b").map(|column| column + 1)
            );
        }
        if line.contains("error(\"This combination") {
            assert_eq!(lines[index + 1].find('^'), line.find("error("));
        }
    }
}

#[test]
fn cli_parse_renders_source_errors_without_json_and_preserves_machine_mode() {
    let artifact = fixture("target/cli-human-parse.ylc");
    fs::write(
        &artifact,
        compile_language(fixture("documentation/target-syntax/minijs.yl"))
            .unwrap()
            .to_bytes()
            .unwrap(),
    )
    .unwrap();
    let input = fixture("tests/fixtures/minijs/unexpected-token.js");
    let human = Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("parse")
        .arg(&artifact)
        .arg(&input)
        .output()
        .unwrap();
    assert!(!human.status.success());
    let diagnostic = String::from_utf8(human.stderr).unwrap();
    assert!(
        diagnostic.contains("Error[parse.unexpected_token]"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains('@'), "{diagnostic}");
    assert!(diagnostic.contains('^'), "{diagnostic}");
    assert!(human.stdout.is_empty());
    let machine = Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("parse")
        .arg(artifact)
        .arg(input)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!machine.status.success());
    assert!(machine.stderr.is_empty());
    let result: serde_json::Value = serde_json::from_slice(&machine.stdout).unwrap();
    assert_eq!(result["diagnostics"][0]["code"], "parse.unexpected_token");
}

#[test]
fn cli_parse_handles_relative_paths_eof_and_retains_ast_for_constraint_errors() {
    let artifact = fixture("target/cli-human-relative.ylc");
    fs::write(
        &artifact,
        compile_language(fixture("documentation/target-syntax/minijs.yl"))
            .unwrap()
            .to_bytes()
            .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "parse",
            "target/cli-human-relative.ylc",
            "tests/fixtures/minijs/malformed-expression.js",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(
        diagnostic.contains("malformed-expression.js:2"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains('^'));
    let constrained=compile_sources("checks.yl",&BTreeMap::from([("checks.yl".into(),r#"node P = value: /x/ { constraints { when true { error("Rejected value") } } } entry P"#.into())])).unwrap();
    fs::write(&artifact, constrained.to_bytes().unwrap()).unwrap();
    let input = fixture("target/cli-human-constrained.txt");
    fs::write(&input, "x").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("parse")
        .arg(artifact)
        .arg(input)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("Rejected value"));
    let ast: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(ast["type"], "P");
}
