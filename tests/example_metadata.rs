use std::collections::BTreeMap;
use your_language::{compile_sources, load_compiled_language, parse};
fn language(source: &str) -> your_language::CompiledLanguage {
    compile_sources(
        "entry.yl",
        &BTreeMap::from([("entry.yl".into(), source.into())]),
    )
    .unwrap()
}
fn preview(source: &str, name: &str) -> Option<String> {
    language(source)
        .example_previews()
        .into_iter()
        .find(|p| p.definition == format!("entry.yl#{name}"))
        .unwrap()
        .text
}
#[test]
fn explicit_examples_override_and_compose_nodes_and_patterns() {
    let source = r#"pattern Name = /[a-z]+/ { example "hello" }
node Number = /[0-9]+/ { example "42" }
node Program = Name "=" Number
entry Program"#;
    assert_eq!(preview(source, "Program").as_deref(), Some("hello=42"));
    let language = language(source);
    let loaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    assert_eq!(language.example_previews(), loaded.example_previews());
    assert_eq!(parse(&language, "hello=42"), parse(&loaded, "hello=42"));
}
#[test]
fn invalid_explicit_examples_are_spanned_errors() {
    let errors = compile_sources(
        "entry.yl",
        &BTreeMap::from([(
            "entry.yl".into(),
            "node Number = /[0-9]+/ { example \"abc\" } entry Number".into(),
        )]),
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "yl.invalid_example");
    assert!(errors[0].primary.end > errors[0].primary.start);
}
#[test]
fn shortest_choices_ties_optional_and_one_repetition() {
    assert_eq!(
        preview(
            "node Program = (\"long\" | \"b\" | \"a\") entry Program",
            "Program"
        )
        .as_deref(),
        Some("b")
    );
    assert_eq!(
        preview(
            "node Program = \"a\"? \"b\"* \"c\"+ entry Program",
            "Program"
        )
        .as_deref(),
        Some("bc")
    );
}
#[test]
fn regexes_and_recursive_terminating_paths_are_bounded() {
    assert_eq!(
        preview("node Program = /[0-9]{2}/ entry Program", "Program").as_deref(),
        Some("00")
    );
    assert_eq!(
        preview(
            "node Program = (\"(\" Program \")\" | \"x\") entry Program",
            "Program"
        )
        .as_deref(),
        Some("x")
    );
    assert_eq!(
        preview(
            "node Program = \"(\" Program \")\" entry Program",
            "Program"
        ),
        None
    );
    assert_eq!(
        preview("node Program = /a{10000}/ entry Program", "Program"),
        None
    );
}
#[test]
fn pipe_star_keeps_one_original_item_and_no_separator_tail() {
    let source = r#"import { separatedBy } from "std/parser"
node Item = "x"
node Program = Item* |> separatedBy(",")
entry Program"#;
    assert_eq!(preview(source, "Program").as_deref(), Some("x"));
}
#[test]
fn imports_trivia_and_constraints_are_parser_validated() {
    let sources = BTreeMap::from([
        ("entry.yl".into(), "import { Item } from \"./item\" trivia Space = / +/ node Program = \"let\" Item entry Program { trivia Space }".into()),
        ("item.yl".into(), "export node Item = /[a-z]+/ { example \"name\" }".into()),
    ]);
    let language = compile_sources("entry.yl", &sources).unwrap();
    let text = language
        .example_previews()
        .into_iter()
        .find(|p| p.definition == "entry.yl#Program")
        .unwrap()
        .text
        .unwrap();
    assert!(!parse(&language, &text).has_errors());
    let source = r#"node Program = value: "x" { constraints { when true { error("Invalid") } } } entry Program"#;
    assert_eq!(preview(source, "Program"), None);
}
#[test]
fn formatting_metadata_is_idempotent() {
    let source = "node Program = \"x\" { example \"x\" } entry Program";
    let once = your_language::format_yl("entry.yl", source).unwrap();
    assert_eq!(once, your_language::format_yl("entry.yl", &once).unwrap());
    assert!(once.contains("example \"x\""));
}

#[test]
fn metadata_has_no_effect_on_target_ast_or_syntax_diagnostics() {
    let plain = language("pattern Item = /[0-9]+/ node Program = Item entry Program");
    let metadata =
        language(r#"pattern Item = /[0-9]+/ { example "42" } node Program = Item entry Program"#);
    assert_eq!(parse(&plain, "7"), parse(&metadata, "7"));
    assert_eq!(parse(&plain, "bad"), parse(&metadata, "bad"));
}
