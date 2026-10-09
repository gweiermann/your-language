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
fn minijs_acceptance_is_explicitly_parked_not_weakened() {
    let error = compile_language(fixture("documentation/target-syntax/minijs.yl")).unwrap_err();
    assert_eq!(error[0].code, "yl.parked_application");
    assert_eq!(error[0].primary.file, "std/parser");
    golden("minijs-parked", &error);
    let result = Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("check")
        .arg(fixture("documentation/target-syntax/minijs.yl"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("yl.parked_application"));
}
#[test]
fn all_separated_by_modes_are_parked_at_the_same_design_boundary() {
    for q in ["*", "+"] {
        for mode in ["none", "optional", "required"] {
            let source=format!("import {{ separatedBy }} from \"std/parser\" node Item = value: /x/ node P = items: Item{q} |> separatedBy(\",\", trailing=.{mode}) entry P");
            let sources = BTreeMap::from([("main.yl".into(), source)]);
            assert_eq!(
                compile_sources("main.yl", &sources).unwrap_err()[0].code,
                "yl.parked_application"
            );
        }
    }
}
