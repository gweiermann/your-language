use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use your_language::{format_yl, format_yl_with_options, frontend::parse_yl, FormatOptions};
fn tree(source: &str) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                fields.remove("span");
                for value in fields.values_mut() {
                    strip(value);
                }
            }
            serde_json::Value::Array(items) => {
                for value in items {
                    strip(value);
                }
            }
            _ => {}
        }
    }
    let mut result = serde_json::to_value(parse_yl("source", source).unwrap()).unwrap();
    strip(&mut result);
    result
}
#[test]
fn formats_semantic_blocks_without_changing_syntax() {
    let source = "import{scope}from\"lib\"node Program=name:/[a-z]+/ {semantics{boundary scope() group Register{scope()} precedence{Program>Child}}}entry Program";
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(tree(source), tree(&formatted));
    assert!(formatted.contains("import { scope } from \"lib\""));
    assert!(
        formatted.contains(
            "node Program = name: /[a-z]+/ {\n    semantics {\n        boundary\n        scope()\n"
        ),
        "{formatted}"
    );
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}
#[test]
fn preserves_comments_and_raw_string_regex_spellings() {
    let source = r#"// module
node A="line\n\t\"literal" // trailing A
// docs B
node B=/a\/b[{}]*/ { /* inner */ semantics{boundary} }
/* multiline
   exact contents */
entry B"#;
    let formatted = format_yl("source", source).unwrap();
    for spelling in [
        "// module",
        "// trailing A",
        "// docs B",
        "/* inner */",
        "/* multiline\n   exact contents */",
        r#""line\n\t\"literal""#,
        r#"/a\/b[{}]*/"#,
    ] {
        assert!(
            formatted.contains(spelling),
            "Lost {spelling}:\n{formatted}"
        );
    }
    assert!(formatted.contains("// docs B\nnode B"), "{formatted}");
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}
#[test]
fn every_valid_checked_in_definition_is_parse_equivalent_and_idempotent() {
    fn visit(directory: &Path, paths: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, paths);
            } else if path.extension().is_some_and(|ext| ext == "yl") {
                paths.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = vec![];
    visit(&root.join("examples"), &mut paths);
    visit(&root.join("tests/fixtures"), &mut paths);
    let mut checked = 0;
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        if parse_yl("source", &source).is_err() {
            continue;
        }
        let formatted = format_yl("source", &source)
            .unwrap_or_else(|errors| panic!("{}: {errors:?}", path.display()));
        assert_eq!(tree(&source), tree(&formatted), "{}", path.display());
        assert_eq!(
            format_yl("source", &formatted).unwrap(),
            formatted,
            "{}",
            path.display()
        );
        let aligned = aligned(&source);
        assert_eq!(tree(&source), tree(&aligned), "{}", path.display());
        assert_eq!(self::aligned(&aligned), aligned, "{}", path.display());
        checked += 1;
    }
    assert!(checked >= 10, "Only {checked} fixtures exercised");
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path())
            && self
                .0
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("yl-fmt-test-"))
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn scratch() -> Scratch {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "yl-fmt-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    Scratch(path)
}
#[test]
fn cli_check_is_read_only_and_invalid_batches_are_not_modified() {
    let root = scratch();
    let valid = root.0.join("valid.yl");
    let invalid = root.0.join("invalid.yl");
    let source = "node A=\"a\" entry A";
    fs::write(&valid, source).unwrap();
    fs::write(&invalid, "node =").unwrap();
    let run = |args: &[&str], files: &[&Path]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yl"));
        command.args(args);
        for file in files {
            command.arg(file);
        }
        command.output().unwrap()
    };
    let check = run(&["fmt", "--check"], &[&valid]);
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stderr).contains("yl.format_required"));
    assert_eq!(fs::read_to_string(&valid).unwrap(), source);
    assert!(String::from_utf8(check.stdout)
        .unwrap()
        .contains("Would format"));
    let failure = run(&["fmt"], &[&valid, &invalid]);
    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8(failure.stderr)
        .unwrap()
        .contains("yl.unexpected_token"));
    assert_eq!(fs::read_to_string(&valid).unwrap(), source);
    assert_eq!(fs::read_to_string(&invalid).unwrap(), "node =");
    let write = run(&["fmt"], &[&valid]);
    assert!(
        write.status.success(),
        "{}",
        String::from_utf8_lossy(&write.stderr)
    );
    let formatted = fs::read_to_string(&valid).unwrap();
    assert_eq!(formatted, format_yl("source", source).unwrap());
    let clean = run(&["fmt", "--check"], &[&valid]);
    assert!(clean.status.success());
    assert!(clean.stdout.is_empty());
    assert_eq!(
        fs::read_dir(&root.0).unwrap().count(),
        2,
        "Temporary formatter files leaked"
    );
}
#[test]
fn invalid_source_returns_structured_diagnostics() {
    let errors = format_yl("broken.yl", "node X = \"unterminated").unwrap_err();
    assert_eq!(errors[0].code, "yl.unterminated_literal");
    assert_eq!(errors[0].primary.file, "broken.yl");
}

#[test]
fn preserves_comment_trailing_spaces_and_literal_line_endings() {
    let source = "// comment with spaces   \r\nnode A=\"line\r\ncontents\" // tail   \r\nentry A";
    let formatted = format_yl("source", source).unwrap();
    assert!(
        formatted.contains("// comment with spaces   \n"),
        "{formatted:?}"
    );
    assert!(formatted.contains("// tail   \n"), "{formatted:?}");
    assert!(formatted.contains("\"line\r\ncontents\""), "{formatted:?}");
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn supports_enums_constraints_extensions_and_tuple_rewrite_cases() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"

        enum Mode{a,b}
        constraint named(value: Node){when value.name?.matches(/a\/b/)&&value.isPresent{warn("check")}}
        node Name=/[a-z]+/ node Family{node Name precedence{right Name} checks{named(this)}}
        extend node Family{checks{when this.isPresent{hint("extended")}}}
        pipe wrap(first: Mode=.a,second: Mode=.b){rewrite value* match(first,second){(.a,.a)=>"(" value ")" (.a,.b)=>{warn("pair") value}(_,_)=>{error("unsupported")}}}
        pattern Many(item: Node)=item* |> wrap(.a,.b)
        entry Family{trivia Name}
    "#;
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
    assert!(
        formatted.contains("enum Mode {\n    a,\n    b\n}"),
        "{formatted}"
    );
    assert!(
        formatted.contains("rewrite value* match (first, second) {"),
        "{formatted}"
    );
}

#[test]
fn optional_pattern_and_contextual_variant_do_not_merge_into_optional_chaining() {
    let source = "pattern A = /a/? .b";
    let formatted = format_yl("source", source).unwrap();
    assert!(formatted.contains("/a/?\n    .b"), "{formatted}");
    assert_eq!(tree(source), tree(&formatted));
}

#[test]
fn cli_read_only_files_and_literal_dash_paths_are_safe() {
    let root = scratch();
    let path = root.0.join("-leading.yl");
    let earlier = root.0.join("earlier.yl");
    let source = "node A=\"a\" entry A";
    fs::write(&path, source).unwrap();
    fs::write(&earlier, source).unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--", "earlier.yl", "-leading.yl"])
        .current_dir(&root.0)
        .output()
        .unwrap();
    fs::set_permissions(&path, original).unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&earlier).unwrap(), source);
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert!(String::from_utf8(failed.stderr)
        .unwrap()
        .contains("read-only"));
    let written = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--", "-leading.yl"])
        .current_dir(&root.0)
        .output()
        .unwrap();
    assert!(written.status.success());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format_yl("source", source).unwrap()
    );
    let duplicate = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--check"])
        .arg(&path)
        .arg(&path)
        .output()
        .unwrap();
    assert!(duplicate.status.success());
    assert!(duplicate.stdout.is_empty());
}

#[test]
fn single_units_stay_inline_including_captures_postfix_and_pipes() {
    let source = r#"node Name=/[a-z]+/ pattern Names=names:Name* |> separatedBy(",") node Value=item:Name? { semantics { scope() } }"#;
    let expected = "node Name = /[a-z]+/\n\npattern Names = names: Name* |> separatedBy(\",\")\n\nnode Value = item: Name? {\n    semantics {\n        scope()\n    }\n}\n";
    assert_eq!(format_yl("source", source).unwrap(), expected);
    assert_eq!(format_yl("source", expected).unwrap(), expected);
}

#[test]
fn sequences_place_every_grammar_unit_on_its_own_line() {
    let source = r#"pattern FunctionContents="(" parameters:Parameter* |> separatedBy(",") ")" annotation:Annotation? body:FunctionBody {semantics{createLexicalScope(self) functionType(parameters,body,annotation)}}"#;
    let expected = r#"pattern FunctionContents =
    "("
    parameters: Parameter* |> separatedBy(",")
    ")"
    annotation: Annotation?
    body: FunctionBody
{
    semantics {
        createLexicalScope(self)
        functionType(parameters, body, annotation)
    }
}
"#;
    assert_eq!(format_yl("source", source).unwrap(), expected);
    assert_eq!(tree(source), tree(expected));
    assert_eq!(format_yl("source", expected).unwrap(), expected);
}

#[test]
fn grouped_units_and_pipelines_remain_intact() {
    let source =
        r#"node A = first:(Name | Number)+ |> wrap("[") |> close("]") second:("(" Name ")")?"#;
    let expected = r#"node A =
    first: (Name | Number)+ |> wrap("[") |> close("]")
    second: ("(" Name ")")?
"#;
    assert_eq!(format_yl("source", source).unwrap(), expected);
    assert_eq!(tree(source), tree(expected));
    assert_eq!(format_yl("source", expected).unwrap(), expected);
}

#[test]
fn choices_have_separate_branches_without_splitting_captured_units() {
    let source = r#"pattern A=left:Name "+" right:Name | Number | Text"#;
    let expected = r#"pattern A =
    left: Name
    "+"
    right: Name
    | Number
    | Text
"#;
    assert_eq!(format_yl("source", source).unwrap(), expected);
    assert_eq!(tree(source), tree(expected));
    assert_eq!(format_yl("source", expected).unwrap(), expected);
}

#[test]
fn sequence_comments_and_raw_literals_remain_parse_equivalent() {
    let source = "node A = first:/a\\/b/ // first\n/* separator */ \";\" second:Name? |> wrap(\"x\") {semantics{scope()}}";
    let formatted = format_yl("source", source).unwrap();
    assert!(
        formatted.contains("first: /a\\/b/ // first\n"),
        "{formatted}"
    );
    assert!(
        formatted.contains("    second: Name? |> wrap(\"x\")\n{"),
        "{formatted}"
    );
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn deferred_meaning_matches_are_formatted_and_keep_noop_branches() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"
node Add=left:Expression "+" right:Expression {semantics{match(typeOf(left),typeOf(right)){string,string|string,number|number,string=>type(string),number,number=>type(number),unknown,_|_,unknown=>{},_=>error("${left} + ${right} is not possible")}}}"#;
    let formatted = format_yl("source", source).unwrap();
    assert!(
        formatted.contains("match (typeOf(left), typeOf(right)) {\n"),
        "{formatted}"
    );
    assert!(
        formatted.contains("number, number                                   => type(number),\n"),
        "{formatted}"
    );
    assert!(
        formatted.contains("_                                                => error(\"${left} + ${right} is not possible\")"),
        "{formatted}"
    );
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn authored_symbols_have_one_declaration_per_paragraph() {
    let source =
        "export symbol number export symbol string node Number=/[0-9]+/{semantics{type(number)}}";
    let formatted = format_yl("source", source).unwrap();
    assert!(
        formatted.starts_with(
            "export symbol number\n\nexport symbol string\n\nnode Number = /[0-9]+/ {"
        ),
        "{formatted}"
    );
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn meaning_metadata_is_first_and_each_operation_has_its_own_line() {
    let expected = r#"node Program = Statements* {
    semantics {
        boundary
        types(unknown, functionTypeSymbol)
        createLexicalScope(self)
        precedence {
            FunctionDeclaration > LetDeclaration > FunctionBody
        }
    }
}
"#;
    for body in [
        "boundary types(unknown,functionTypeSymbol) createLexicalScope(self) precedence{FunctionDeclaration>LetDeclaration>FunctionBody}",
        "types(unknown,functionTypeSymbol) boundary createLexicalScope(self) precedence{FunctionDeclaration>LetDeclaration>FunctionBody}",
        "types(unknown,functionTypeSymbol) createLexicalScope(self) precedence{FunctionDeclaration>LetDeclaration>FunctionBody} boundary",
    ] {
        let source = format!("node Program=Statements*{{semantics{{{body}}}}}");
        let formatted = format_yl("source", &source).unwrap();
        assert_eq!(formatted, expected);
        assert_eq!(tree(&source), tree(&formatted));
        assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
    }
}

#[test]
fn meaning_layout_keeps_groups_matches_and_precedence_separate() {
    let source = r#"node Program=Statements*{semantics{first() group Register{register() check()} precedence{Register>Body} match(typeOf(value)){number=>type(number),_=>{}} boundary last()}}"#;
    let expected = r#"node Program = Statements* {
    semantics {
        boundary
        first()
        group Register {
            register()
            check()
        }
        precedence {
            Register > Body
        }
        match (typeOf(value)) {
            number => type(number),
            _      => {
            }
        }
        last()
    }
}
"#;
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(formatted, expected);
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn metadata_movement_preserves_comments_literals_and_operation_order() {
    let source = r#"node A=/a/{semantics{first("boundary", /boundary/) // first stays first
// metadata explanation
boundary // boundary note
second(value=boundary) precedence{A>B} third()}}"#;
    let formatted = format_yl("source", source).unwrap();
    assert!(formatted.contains("semantics {\n        boundary\n        first(\"boundary\", /boundary/) // first stays first\n"), "{formatted}");
    for comment in ["// metadata explanation", "// boundary note"] {
        assert!(formatted.contains(comment));
    }
    assert!(
        formatted.contains("second(value = boundary)\n        precedence {"),
        "{formatted}"
    );
    assert!(formatted.find("first(").unwrap() < formatted.find("second(").unwrap());
    assert!(formatted.find("second(").unwrap() < formatted.find("third(").unwrap());
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn meaning_boundary_does_not_move_other_boundary_words() {
    let source = "symbol boundary node A=/a/{semantics{first(value=boundary) group G{second(boundary)} boundary last(\"boundary\")}}";
    let formatted = format_yl("source", source).unwrap();
    assert!(formatted.starts_with("symbol boundary\n\nnode A = /a/ {\n    semantics {\n        boundary\n        first(value = boundary)"), "{formatted}");
    assert!(formatted.contains("second(boundary)"));
    assert!(formatted.contains("last(\"boundary\")"));
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn qualified_operations_named_like_metadata_are_not_moved_or_split() {
    let source =
        "node A=/a/{semantics{lib::boundary() lib::precedence() boundary other() precedence{A>B}}}";
    let formatted = format_yl("source", source).unwrap();
    assert!(formatted.contains("semantics {\n        boundary\n        lib::boundary()\n        lib::precedence()\n        other()\n        precedence {"), "{formatted}");
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn cli_json_check_reports_each_file_and_is_read_only() {
    let root = scratch();
    let dirty = root.0.join("dirty.yl");
    let clean = root.0.join("clean.yl");
    let original = "node A=\"a\" entry A";
    fs::write(&dirty, original).unwrap();
    fs::write(&clean, format_yl("source", original).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--json"])
        .arg(&dirty)
        .arg("--check")
        .arg(&clean)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["check"], true);
    assert_eq!(report["success"], false);
    assert_eq!(report["files"][0]["path"], dirty.to_str().unwrap());
    assert_eq!(report["files"][0]["status"], "would_change");
    assert_eq!(report["files"][0]["would_change"], true);
    assert_eq!(report["files"][0]["changed"], false);
    assert_eq!(report["files"][1]["status"], "clean");
    assert_eq!(report["files"][1]["would_change"], false);
    assert_eq!(report["diagnostics"][0]["code"], "yl.format_required");
    assert_eq!(report["diagnostics"][0]["severity"], "warning");
    assert_eq!(
        report["diagnostics"][0]["primary"]["file"],
        dirty.to_str().unwrap()
    );
    assert_eq!(fs::read_to_string(&dirty).unwrap(), original);
}

#[test]
fn cli_json_write_and_clean_check_emit_one_report_without_human_output() {
    let root = scratch();
    let file = root.0.join("source.yl");
    fs::write(&file, "node A=\"a\" entry A").unwrap();
    for (flags, expected) in [
        (vec!["fmt", "--json"], "changed"),
        (vec!["fmt", "--check", "--json"], "clean"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_yl"))
            .args(flags)
            .arg(&file)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["success"], true);
        assert_eq!(report["files"][0]["status"], expected);
        assert_eq!(report["files"][0]["changed"], expected == "changed");
        assert!(report["diagnostics"].as_array().unwrap().is_empty());
    }
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
}

#[test]
fn cli_json_errors_cover_invalid_and_missing_files_without_partial_writes() {
    let root = scratch();
    let valid = root.0.join("valid.yl");
    let invalid = root.0.join("invalid.yl");
    let missing = root.0.join("missing.yl");
    let original = "node A=\"a\" entry A";
    fs::write(&valid, original).unwrap();
    fs::write(&invalid, "node =").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--json"])
        .arg(&valid)
        .arg(&invalid)
        .arg(&missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["success"], false);
    assert_eq!(report["files"][0]["status"], "skipped");
    assert_eq!(report["files"][0]["would_change"], true);
    assert_eq!(report["files"][0]["changed"], false);
    assert_eq!(report["files"][1]["status"], "error");
    assert!(report["files"][1]["would_change"].is_null());
    assert_eq!(report["files"][2]["status"], "error");
    assert_eq!(report["diagnostics"][0]["code"], "yl.unexpected_token");
    assert_eq!(report["diagnostics"][1]["code"], "yl.io");
    assert_eq!(fs::read_to_string(&valid).unwrap(), original);
    assert_eq!(fs::read_to_string(&invalid).unwrap(), "node =");
}

#[test]
fn cli_human_check_underlines_layout_and_syntax_errors() {
    let root = scratch();
    let file = root.0.join("source.yl");
    let source = "node A = \"é\" entry A";
    fs::write(&file, source).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_yl"))
            .args(["fmt", "--check"])
            .arg(&file)
            .env("CLICOLOR_FORCE", "1")
            .env_remove("NO_COLOR")
            .output()
            .unwrap()
    };
    let layout = run();
    assert_eq!(layout.status.code(), Some(1));
    let diagnostic = String::from_utf8(layout.stderr).unwrap();
    assert!(
        diagnostic.contains("Warning[yl.format_required]"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains(source));
    assert!(diagnostic.contains('^'));
    assert!(diagnostic.contains("\x1b[33m"));
    assert!(diagnostic.contains("without --check"));
    fs::write(&file, "node =").unwrap();
    let syntax = run();
    assert_eq!(syntax.status.code(), Some(1));
    let diagnostic = String::from_utf8(syntax.stderr).unwrap();
    assert!(diagnostic.contains("yl.unexpected_token"));
    assert!(diagnostic.contains("node ="));
    assert!(diagnostic.contains("\x1b[31m"));
}

#[test]
fn cli_json_usage_and_literal_json_file_names_are_unambiguous() {
    let invalid = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--bogus", "--json"])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(1));
    assert!(invalid.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "yl.cli");
    assert!(report["files"].as_array().unwrap().is_empty());
    let root = scratch();
    fs::write(root.0.join("--json"), "node A=\"a\" entry A").unwrap();
    let literal = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--", "--json"])
        .current_dir(&root.0)
        .output()
        .unwrap();
    assert!(literal.status.success());
    assert!(String::from_utf8(literal.stdout)
        .unwrap()
        .contains("Formatted --json"));
}

#[test]
fn cli_json_readonly_batch_has_io_diagnostic_and_no_changes() {
    let root = scratch();
    let earlier = root.0.join("earlier.yl");
    let readonly = root.0.join("readonly.yl");
    let source = "node A=\"a\" entry A";
    fs::write(&earlier, source).unwrap();
    fs::write(&readonly, source).unwrap();
    let permissions = fs::metadata(&readonly).unwrap().permissions();
    let mut protected = permissions.clone();
    protected.set_readonly(true);
    fs::set_permissions(&readonly, protected).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--json"])
        .arg(&earlier)
        .arg(&readonly)
        .output()
        .unwrap();
    fs::set_permissions(&readonly, permissions).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["files"][0]["status"], "skipped");
    assert_eq!(report["files"][1]["status"], "error");
    assert_eq!(report["files"][1]["would_change"], true);
    assert_eq!(report["diagnostics"][0]["code"], "yl.io");
    assert_eq!(fs::read_to_string(&earlier).unwrap(), source);
    assert_eq!(fs::read_to_string(&readonly).unwrap(), source);
}

#[test]
fn declaration_containers_start_without_a_blank_line_and_separate_siblings() {
    let source = r#"export trivia Comment {

    trivia Line = /\/\/[^\n]*/

    trivia Block = /\/\*[\s\S]*?\*\//

}"#;
    let expected = r#"export trivia Comment {
    trivia Line = /\/\/[^\n]*/

    trivia Block = /\/\*[\s\S]*?\*\//
}
"#;
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(formatted, expected);
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn nested_containers_keep_leading_comments_and_comment_contents() {
    let source = r#"node Outer {

// first member
node Inner {

/* nested documentation

   preserved blank line */
node First = "a"

// second member
node Second = "b"

}

pattern Tail = "c"

}
enum Kind {

A

B

}"#;
    let expected = r#"node Outer {
    // first member
    node Inner {
        /* nested documentation

   preserved blank line */
        node First = "a"

        // second member
        node Second = "b"
    }

    pattern Tail = "c"
}

enum Kind {
    A
    B
}
"#;
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(formatted, expected);
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn namespace_import_star_is_spaced_without_changing_repetition() {
    let source = "import * as types from \"std/types\"\nnode Program = Item*\nentry Program";
    let formatted = format_yl("source", source).unwrap();
    assert!(
        formatted.starts_with("import * as types from"),
        "{formatted}"
    );
    assert!(formatted.contains("Item*"), "{formatted}");
    assert_eq!(formatted, format_yl("source", &formatted).unwrap());
}

#[test]
fn owner_meanings_are_separated_from_nested_definitions() {
    let source = r#"export node Type {
node NumberType = "number" { semantics { annotate(number) } }
node BooleanType = "boolean" { semantics { annotate(boolean) } }
semantics { inherit() }
}"#;
    let expected = r#"export node Type {
    node NumberType = "number" {
        semantics {
            annotate(number)
        }
    }

    node BooleanType = "boolean" {
        semantics {
            annotate(boolean)
        }
    }

    semantics {
        inherit()
    }
}
"#;
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(formatted, expected);
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn owner_meaning_spacing_keeps_comments_and_first_section_layout() {
    let source = r#"node Outer {
node Inner {
pattern Name = "x" { semantics { annotate(string) } }
// owner documentation
semantics { inherit() }
}
/* outer documentation */
semantics { inherit() }
}
node Only { // first section
semantics { inherit() }
}
node Early {
semantics { inherit() }
node Child = "x"
}"#;
    let formatted = format_yl("source", source).unwrap();
    assert!(
        formatted.contains("        }\n\n        // owner documentation\n        semantics {"),
        "{formatted}"
    );
    assert!(
        formatted.contains("    }\n\n    /* outer documentation */\n    semantics {"),
        "{formatted}"
    );
    assert!(
        formatted.contains("node Only {\n    // first section\n    semantics {"),
        "{formatted}"
    );
    assert!(
        formatted.contains("node Early {\n    semantics {"),
        "{formatted}"
    );
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn alias_members_are_separate_from_owner_precedence_and_meanings() {
    let source = r#"node Expression {
node Addition = left: Expression "+" right: Expression
node Reference
precedence { Addition }
semantics { inherit() }
}"#;
    let expected = r#"node Expression {
    node Addition =
        left: Expression
        "+"
        right: Expression

    node Reference

    precedence {
        Addition
    }

    semantics {
        inherit()
    }
}
"#;
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(formatted, expected);
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn owner_sections_preserve_comments_and_ignore_member_names() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"
node Parent {
node meanings
node precedence
node constraints
// owner operator ordering
precedence { precedence }
// owner checks
checks { when true { warn("owner") } }
semantics { inherit() }
}
node Empty { precedence { Other } checks { } semantics { inherit() } }"#;
    let formatted = format_yl("source", source).unwrap();
    assert!(formatted.contains("    node meanings\n\n    node precedence\n\n    node constraints\n\n    // owner operator ordering\n    precedence {"), "{formatted}");
    assert!(
        formatted.contains("    }\n\n    // owner checks\n    checks {"),
        "{formatted}"
    );
    assert!(
        formatted.contains("node Empty {\n    precedence {"),
        "{formatted}"
    );
    assert!(!formatted.contains("{\n\n"), "{formatted}");
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

fn aligned(source: &str) -> String {
    format_yl_with_options(
        "source",
        source,
        FormatOptions {
            align_match_arrows: true,
        },
    )
    .unwrap()
}
#[test]
fn meaning_match_alignment_is_default_and_aligns_tuple_alternatives() {
    let source = include_str!("../examples/formatting-alignment/match-arrows.yl");
    let canonical = format_yl("source", source).unwrap();
    assert_eq!(
        canonical,
        format_yl_with_options("source", source, FormatOptions::default()).unwrap()
    );
    assert!(canonical.contains("number, number                                   => type(number)"));
    let formatted = aligned(source);
    assert!(
        formatted.contains("number, number                                   => type(number)"),
        "{formatted}"
    );
    assert!(
        formatted.contains("_                                                => error("),
        "{formatted}"
    );
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(aligned(&formatted), formatted);
    assert_eq!(format_yl("source", &formatted).unwrap(), canonical);
}
#[test]
fn alignment_is_local_to_each_match_and_handles_unicode_and_blocks() {
    let source = "node Container { node Outer=\"x\"{semantics{match(value){\"\u{4f60}\u{597d}\",veryLong => { first() second() } _=>{}}}} node Inner=\"y\"{semantics{match(value){\"\u{1f600}\"=>go() \"ab\"=>go() \"e\u{0301}\"=>go() _=>go()}}} }";
    let formatted = aligned(source);
    let columns: Vec<_> = formatted
        .lines()
        .filter_map(|line| {
            line.find("=>")
                .map(|index| unicode_width::UnicodeWidthStr::width(&line[..index]))
        })
        .collect();
    assert_eq!(columns.len(), 6, "{formatted}");
    assert_eq!(columns[0], columns[1], "{formatted}");
    assert_eq!(columns[2], columns[3], "{formatted}");
    assert_eq!(columns[3], columns[4], "{formatted}");
    assert_eq!(columns[4], columns[5], "{formatted}");
    assert_ne!(columns[0], columns[2], "{formatted}");
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(aligned(&formatted), formatted);
}
#[test]
fn alignment_preserves_comments_multiline_headers_and_other_arrows() {
    let source = r#"pipe keep() { rewrite value => value }
node Root = /a=>b/ {
semantics {
match(value) {
"a=>b" => write("literal => stays")
_ /* same line */ => { first() second() }
// case comment
longer // split header
=> go()
}
}
}"#;
    let formatted = aligned(source);
    assert!(formatted.contains("rewrite value => value"), "{formatted}");
    for raw in [
        "/a=>b/",
        "\"a=>b\"",
        "\"literal => stays\"",
        "/* same line */",
        "// case comment",
        "longer // split header\n",
    ] {
        assert!(formatted.contains(raw), "lost {raw}: {formatted}");
    }
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(aligned(&formatted), formatted);
}
#[test]
fn alignment_padding_is_bounded_with_a_structured_error() {
    let source = format!(
        "node Root=\"x\"{{semantics{{match(value){{\"{}\"=>go() {} }}}}}}",
        "x".repeat(1000),
        "_=>go() ".repeat(1100)
    );
    let errors = format_yl_with_options(
        "source",
        &source,
        FormatOptions {
            align_match_arrows: true,
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "yl.format_alignment_limit");
    assert!(format_yl_with_options(
        "source",
        &source,
        FormatOptions {
            align_match_arrows: false
        }
    )
    .is_ok());
}
#[test]
fn cli_default_alignment_check_json_write_and_legacy_flag_agree() {
    let root = scratch();
    let path = root.0.join("match.yl");
    let source = format_yl_with_options(
        "source",
        include_str!("../examples/formatting-alignment/match-arrows.yl"),
        FormatOptions {
            align_match_arrows: false,
        },
    )
    .unwrap();
    fs::write(&path, &source).unwrap();
    let check = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--check", "--json"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(1));
    assert!(check.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["files"][0]["status"], "would_change");
    assert!(report["diagnostics"][0]["help"]
        .as_str()
        .unwrap()
        .contains("Run yl fmt without --check"));
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    let write = Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("fmt")
        .arg(&path)
        .output()
        .unwrap();
    assert!(write.status.success());
    assert_eq!(fs::read_to_string(&path).unwrap(), aligned(&source));
    let clean = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--json", "--check"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(clean.status.success());
    let report: serde_json::Value = serde_json::from_slice(&clean.stdout).unwrap();
    assert_eq!(report["files"][0]["status"], "clean");
    let canonical = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--check", "--align-match-arrows"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(canonical.status.code(), Some(0));
    let literal = root.0.join("--align-match-arrows");
    fs::write(&literal, "node A=\"a\"").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_yl"))
        .current_dir(&root.0)
        .args(["fmt", "--", "--align-match-arrows"])
        .output()
        .unwrap();
    assert!(result.status.success());
}

#[test]
fn default_options_enable_alignment_and_explicit_library_override_preserves_ast() {
    assert!(FormatOptions::default().align_match_arrows);
    let source = "node Root=\"x\"{semantics{match(value){short=>go() longerName=>go()}}}";
    let formatted = format_yl("source", source).unwrap();
    assert!(formatted.contains("short      => go()"), "{formatted}");
    let unaligned = format_yl_with_options(
        "source",
        &formatted,
        FormatOptions {
            align_match_arrows: false,
        },
    )
    .unwrap();
    assert!(unaligned.contains("short => go()"));
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(tree(source), tree(&unaligned));
    assert_eq!(format_yl("source", &unaligned).unwrap(), formatted);
}

#[test]
fn immutable_definitions_reactive_guards_and_operation_results_have_separate_lines() {
    let source = r#"import { error } from "core/diagnostics"
operation attach(target,value){ result:=value return result }
node Root="x" {semantics{ref:=attach(self,true) when ref {error("example")}}}
entry Root"#;
    let formatted = format_yl("source", source).unwrap();
    assert!(
        formatted.contains("    result := value\n    return result"),
        "{formatted}"
    );
    assert!(
        formatted.contains("        ref := attach(self, true)\n        when ref {"),
        "{formatted}"
    );
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}
