use std::collections::BTreeMap;
use your_language::{
    compile_sources, format_yl, frontend::parse_yl, load_compiled_language, parse_named,
};

fn compile(source: &str) -> your_language::CompiledLanguage {
    compile_sources(
        "language.yl",
        &BTreeMap::from([("language.yl".into(), source.into())]),
    )
    .unwrap()
}
fn tokens(language: &your_language::CompiledLanguage, source: &str) -> Vec<(String, String)> {
    language
        .highlight_named("program", source)
        .into_iter()
        .map(|token| {
            (
                source[token.span.start..token.span.end].into(),
                token.token_type,
            )
        })
        .collect()
}
#[test]
fn contextual_pattern_metadata_survives_expansion_and_reload() {
    let language = compile(
        r#"
        trivia Space = /\s+/
        pattern Name = /[a-z]+/ { ide { highlightAs "variable" } }
        pattern Keyword = "function" { ide { highlightAs "keyword" } }
        pattern FunctionName = Name { ide { example "hello" highlightAs "function" } }
        node Program = Keyword name:FunctionName "(" ")"
        entry Program { trivia Space }
    "#,
    );
    let source = "function hello()";
    assert_eq!(
        tokens(&language, source),
        vec![
            ("function".into(), "keyword".into()),
            ("hello".into(), "function".into())
        ]
    );
    let loaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    assert_eq!(tokens(&loaded, source), tokens(&language, source));
    assert_eq!(
        parse_named(&loaded, "program", source),
        parse_named(&language, "program", source)
    );
    assert!(loaded
        .example_previews()
        .iter()
        .any(|preview| preview.text.as_deref() == Some("hello")));
}
#[test]
fn expanded_parameterized_patterns_can_highlight_their_terminals() {
    let language = compile(
        r#"
        pattern Colored(item) = item { ide { highlightAs "number" } }
        pattern Unused(item) = item { ide { highlightAs "string" } }
        node Program = Colored(/[0-9]+/)
        entry Program
    "#,
    );
    assert_eq!(
        tokens(&language, "42"),
        vec![("42".into(), "number".into())]
    );
    let loaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    assert_eq!(tokens(&loaded, "42"), tokens(&language, "42"));
}

#[test]
fn reusable_keyword_policy_keeps_boundaries_and_contextual_roles_after_reload() {
    let language = compile(
        r#"
        import { boundedBy } from "std/parser"
        trivia Space = /\s+/
        pattern IdentifierPart = /[a-z]/
        pattern keyword(value) = value |> boundedBy(IdentifierPart) {
            ide { highlightAs "keyword" }
        }
        pattern Name = /[a-z]+/ { ide { highlightAs "variable" } }
        pattern FunctionName = Name { ide { highlightAs "function" } }
        node Program = keyword("let") Name ";" keyword("function") FunctionName "(" ")"
        entry Program { trivia Space }
    "#,
    );
    let loaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let source = "let value; function next()";
    for language in [&language, &loaded] {
        assert!(parse_named(language, "program", source)
            .diagnostics
            .is_empty());
        assert_eq!(
            tokens(language, source),
            vec![
                ("let".into(), "keyword".into()),
                ("value".into(), "variable".into()),
                ("function".into(), "keyword".into()),
                ("next".into(), "function".into())
            ]
        );
        for invalid in ["letvalue; function next()", "let value; functionnext()"] {
            assert!(!parse_named(language, "program", invalid)
                .diagnostics
                .is_empty());
        }
    }
}
#[test]
fn failed_choices_and_lookaround_do_not_leak_highlights() {
    let language = compile(
        r#"
        import { notAhead } from "core/parser"
        pattern Wrong = "x" "z" { ide { highlightAs "string" } }
        pattern Right = "x" { ide { highlightAs "number" } }
        node Program = Wrong | Right
        entry Program
    "#,
    );
    assert_eq!(tokens(&language, "x"), vec![("x".into(), "number".into())]);
}
#[test]
fn trivia_highlights_do_not_get_overridden_by_enclosing_node() {
    let language = compile(
        r#"
        trivia Space = /\s+/
        trivia Comment = /\/\/[^\n]*/ { ide { highlightAs "comment" } }
        node Program = "let" name:/[a-z]+/ { ide { highlightAs "variable" } }
        entry Program { trivia Space, Comment }
    "#,
    );
    let source = "let // comment\nname";
    assert_eq!(
        tokens(&language, source),
        vec![
            ("let".into(), "variable".into()),
            ("// comment".into(), "comment".into()),
            ("name".into(), "variable".into())
        ]
    );
}
#[test]
fn metadata_validation_and_migration_are_structured() {
    for (source, code) in [
        (
            r#"node Program = "x" { example "x" } entry Program"#,
            "yl.ide_migration",
        ),
        (
            r#"node Program = "x" { ide { highlightAs "pink" } } entry Program"#,
            "yl.highlight",
        ),
        (
            r#"node Program = "x" { ide { highlightAs "number" highlightAs "string" } } entry Program"#,
            "yl.duplicate_highlight",
        ),
        (
            r#"node Program = "x" { ide {} ide {} } entry Program"#,
            "yl.duplicate_member",
        ),
    ] {
        assert_eq!(parse_yl("language.yl", source).unwrap_err()[0].code, code);
    }
    let language = compile(r#"node Program = "x" { ide { highlightAs "number" } } entry Program"#);
    let mut artifact: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    artifact["parser"]["highlights"]["language.yl::Program"] = "invalid".into();
    assert!(load_compiled_language(&serde_json::to_vec(&artifact).unwrap()).is_err());
}
#[test]
fn formatter_gives_metadata_properties_and_sections_their_own_lines() {
    let source =
        r#"node Program="42" {ide {example "42" highlightAs "number"} semantics {}} entry Program"#;
    let formatted = format_yl("language.yl", source).unwrap();
    assert!(
        formatted.contains(
            "ide {\n        example \"42\"\n        highlightAs \"number\"\n    }\n    semantics {"
        ),
        "{formatted}"
    );
    assert_eq!(formatted, format_yl("language.yl", &formatted).unwrap());
}

#[test]
fn incomplete_program_retains_one_consistent_prefix_not_failed_branches() {
    let language = compile(
        r#"
        pattern Name = /[a-z]+/ { ide { highlightAs "variable" } }
        pattern Keyword = "function" { ide { highlightAs "keyword" } }
        pattern Bad = Keyword "z" { ide { highlightAs "number" } }
        pattern Good = Keyword Name "!"
        trivia Space = /\s+/
        node Program = Bad | Good
        entry Program { trivia Space }
    "#,
    );
    assert_eq!(
        tokens(&language, "function hello"),
        vec![
            ("function".into(), "keyword".into()),
            ("hello".into(), "variable".into())
        ]
    );
    assert!(parse_named(&language, "program", "function hello")
        .ast
        .is_none());
}
