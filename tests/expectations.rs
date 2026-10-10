use std::collections::BTreeMap;
use your_language::{compile_sources, load_compiled_language, parse, CompiledLanguage};

fn language(source: &str) -> CompiledLanguage {
    let language = compile_sources(
        "test.yl",
        &BTreeMap::from([("test.yl".into(), source.into())]),
    )
    .unwrap();
    load_compiled_language(&language.to_bytes().unwrap()).unwrap()
}

fn expression() -> CompiledLanguage {
    language(
        r#"
trivia Space = /\s+/
node E {
    node Number = value: /[0-9]+/
    node Name = value: /[a-z]+/
    node Group = "(" value: E ")"
    node Call = callee: E "(" arguments: (E ("," E)*)? ")"
    node Sum = left: E "+" right: E
    precedence { Call > Sum }
}
entry E { trivia Space }
"#,
    )
}

#[test]
fn eof_call_reports_required_closer_instead_of_optional_continuations() {
    let result = parse(&expression(), "foo(1");
    assert!(result.has_errors());
    assert_eq!(result.diagnostics[0].message, "Expected \")\"");
    assert_eq!(result.diagnostics[0].primary.start, 5);
}

#[test]
fn consumed_operator_or_separator_keeps_required_item_expectations() {
    let language = expression();
    for source in ["foo(1 +", "foo(1,", "foo(1, ", "foo(1 + \n"] {
        let result = parse(&language, source);
        assert!(result.has_errors());
        assert_eq!(
            result.diagnostics[0].message, "Expected \"(\" or /[0-9]+/ or /[a-z]+/",
            "{source:?}"
        );
        assert_eq!(result.diagnostics[0].primary.start, source.len());
    }
}

#[test]
fn nested_and_empty_calls_skip_only_unstarted_optional_occurrences() {
    let language = expression();
    for source in ["foo(", "foo(1 \n", "foo(bar(1", "foo(bar(1)"] {
        let result = parse(&language, source);
        assert_eq!(result.diagnostics[0].message, "Expected \")\"", "{source}");
        assert_eq!(result.diagnostics[0].primary.start, source.len());
    }
    for source in ["foo()", "foo(1)", "foo(bar(1))", "foo(1 + 2, 3)", "1"] {
        assert!(parse(&language, source).ast.is_some(), "{source}");
    }
}

#[test]
fn required_alternatives_survive_nullable_grammar_paths() {
    let language = language(r#"node End = ("a"? "b"* ")") | ("c"? "d"* "]") entry End"#);
    assert_eq!(
        parse(&language, "").diagnostics[0].message,
        "Expected \")\" or \"]\""
    );
    assert!(parse(&language, ")").ast.is_some());
    assert!(parse(&language, "]").ast.is_some());
}

#[test]
fn plus_requires_first_item_but_can_finish_after_one() {
    let language = language(r#"node Items = /[a-z]/+ "]" entry Items"#);
    assert_eq!(
        parse(&language, "").diagnostics[0].message,
        "Expected /[a-z]/"
    );
    assert_eq!(
        parse(&language, "a").diagnostics[0].message,
        "Expected \"]\""
    );
    assert!(parse(&language, "a]").ast.is_some());
}

#[test]
fn optional_sequence_that_has_started_retains_its_inner_requirement() {
    let language = language(r#"node Wrapped = "(" ("x" "y")? ")" entry Wrapped"#);
    assert_eq!(
        parse(&language, "(").diagnostics[0].message,
        "Expected \")\""
    );
    assert_eq!(
        parse(&language, "(x").diagnostics[0].message,
        "Expected \"y\""
    );
    assert_eq!(parse(&language, "(x").diagnostics[0].primary.start, 2);
    assert!(parse(&language, "(xy)").ast.is_some());
}

#[test]
fn non_eof_retains_optional_and_operator_alternatives() {
    let result = parse(&expression(), "foo(1 @");
    assert_eq!(
        result.diagnostics[0].message,
        "Expected \"(\" or \")\" or \"+\" or \",\""
    );
    assert_eq!(result.diagnostics[0].primary.start, 6);
}

#[test]
fn entirely_optional_grammar_accepts_eof_without_a_missing_token() {
    let language = language(r#"node Empty = "a"? "b"* entry Empty"#);
    for source in ["", "a", "abb"] {
        let result = parse(&language, source);
        assert!(result.ast.is_some());
        assert!(result.diagnostics.is_empty());
    }
}

#[test]
fn cli_human_and_json_checks_report_the_same_required_eof_token() {
    use std::{fs, path::Path, process::Command};

    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/cli-expectations");
    fs::create_dir_all(&directory).unwrap();
    let artifact = directory.join("language.ylc");
    let source = directory.join("call.txt");
    fs::write(&artifact, expression().to_bytes().unwrap()).unwrap();
    fs::write(&source, "foo(1").unwrap();
    let executable = env!("CARGO_BIN_EXE_yl");
    let human = Command::new(executable)
        .args(["language"])
        .arg(&artifact)
        .arg("check")
        .arg(&source)
        .output()
        .unwrap();
    assert!(!human.status.success());
    assert!(human.stdout.is_empty());
    let rendered = String::from_utf8(human.stderr).unwrap();
    assert!(rendered.contains("Expected \")\""), "{rendered}");
    assert!(!rendered.contains(" or "), "{rendered}");
    assert!(rendered.contains("foo(1"));
    assert!(rendered.contains('^'));

    let json = Command::new(executable)
        .arg("language")
        .arg(&artifact)
        .arg("check")
        .arg(&source)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!json.status.success());
    assert!(json.stderr.is_empty());
    let diagnostics: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(diagnostics[0]["message"], "Expected \")\"");
    assert_eq!(diagnostics[0]["primary"]["start"], 5);
    assert_eq!(diagnostics[0]["primary"]["end"], 5);
}
