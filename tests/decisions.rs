use std::collections::BTreeMap;
use your_language::{compile_sources, load_compiled_language, parse};

#[test]
fn optional_boolean_conditions_preserve_absence_through_reload() {
    let source = r#"
    node P = name: /[A-Za-z]+/? {
        constraints {
            when name?.matches(/^[A-Z]/) { warning("capital") }
            when !name?.matches(/^[A-Z]/) { warning("lowercase") }
            when name?.matches(/^[A-Z]/) == absent { help("missing") }
            when name.isPresent() && name.matches(/^X/) { warning("X") }
            when name?.matches(/^X/) || true { help("fallback") }
        }
    }
    entry P
    "#;
    let language = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    )
    .unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    for (input, expected) in [
        ("", vec!["missing", "fallback"]),
        ("Alice", vec!["capital", "fallback"]),
        ("bob", vec!["lowercase", "fallback"]),
        ("X", vec!["capital", "X", "fallback"]),
    ] {
        let result = parse(&language, input);
        assert!(!result.has_errors(), "{result:?}");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn boolean_precedence_string_equality_and_definition_errors() {
    let source = r#"node P = name: /[a-z]+/ {
        constraints {
            when false || true && name == "x" { warning("x") }
            when !(name != "x") { help("equal") }
            when "a" != "b" && true { help("always") }
        }
    } entry P"#;
    let language = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    )
    .unwrap();
    assert_eq!(parse(&language, "x").diagnostics.len(), 3);
    assert_eq!(parse(&language, "y").diagnostics.len(), 1);
    for (condition, code) in [
        ("name.matches(/x/)", "yl.optional_capture"),
        (
            "name.isPresent() || name.matches(/x/)",
            "yl.optional_capture",
        ),
        ("true == \"x\"", "yl.condition_type"),
        ("\"x\"", "yl.condition_type"),
    ] {
        let source = format!("node P = name: /x/? {{ constraints {{ when {condition} {{ error(\"bad\") }} }} }} entry P");
        assert_eq!(
            compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), source)])).unwrap_err()
                [0]
            .code,
            code
        );
    }
}

#[test]
fn tuple_rewrite_matches_are_exhaustive_and_support_wildcard_fallbacks() {
    let source = r#"
        enum Mode { a b }
        pipe wrap(first: Mode, second: Mode) {
            rewrite value match (first, second) {
                (.a, .a) => "(" value ")"
                (.a, .b) => { error("This combination is not allowed.") }
                (_, _) => { warning("Pipe not applied.") value }
            }
        }
        node P = item: /x/ |> wrap(.a, .a)
        entry P
    "#;
    let language = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    )
    .unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    assert!(!parse(&language, "(x)").has_errors());
    let invalid = source.replace("wrap(.a, .a)", "wrap(.a, .b)");
    let errors = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), invalid.clone())]),
    )
    .unwrap_err();
    assert_eq!(errors[0].message, "This combination is not allowed.");
    assert_eq!(
        &invalid[errors[0].primary.start..errors[0].primary.end],
        ".a"
    );
    assert!(errors[0]
        .secondary
        .iter()
        .any(|s| &invalid[s.start..s.end] == ".b"));
}

#[test]
fn rewrite_omission_and_duplication_follow_input_cardinality() {
    use your_language::AstValue;
    for (input, replacement, text, expected) in [
        ("/x/?", "\",\"", ",", AstValue::None),
        ("/x/*", "\",\"", ",", AstValue::List(vec![])),
        (
            "/x/*",
            "value \",\" value",
            "xx,x",
            AstValue::List(vec![AstValue::Text("x".into()); 3]),
        ),
        (
            "/x/+",
            "value \",\" value",
            "xx,x",
            AstValue::List(vec![AstValue::Text("x".into()); 3]),
        ),
    ] {
        let source = format!(
            "pipe p() {{ rewrite value => {replacement} }} node P = items: {input} |> p() entry P"
        );
        let language =
            compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), source)])).unwrap();
        let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
        let result = parse(&language, text);
        assert!(!result.has_errors(), "{result:?}");
        assert_eq!(result.ast.unwrap().fields["items"], expected);
    }
    for (input, replacement) in [
        ("/x/", "\",\""),
        ("/x/+", "\",\""),
        ("/x/?", "value value"),
        ("/x/", "value value"),
    ] {
        let source = format!(
            "pipe p() {{ rewrite value => {replacement} }} node P = {input} |> p() entry P"
        );
        assert_eq!(
            compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), source)])).unwrap_err()
                [0]
            .code,
            "yl.projection_cardinality"
        );
    }
}

#[test]
fn typed_reusable_constraints_validate_node_and_enum_arguments() {
    let source = r#"
        enum Mode { a b }
        node N = text: /x/
        node Other = /y/
        constraint c(input: N, mode: Mode=.a) {
            when mode == .a && input.matches(/x/) { warning("typed") }
        }
        node P = name: N { constraints { c(name) } }
        entry P
    "#;
    let language = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    )
    .unwrap();
    assert_eq!(parse(&language, "x").diagnostics[0].message, "typed");
    let source = source.replace("name: N", "name: Other");
    assert_eq!(
        compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), source)])).unwrap_err()[0]
            .code,
        "yl.invalid_typed_argument"
    );
}

#[test]
fn wildcard_warnings_defaults_and_forwarded_enum_spans() {
    let source = r#"
        enum Mode { a b }
        pipe choice(mode: Mode=.b) {
            rewrite value {
                .a => { error("no a") }
                _ => { warning("identity") value }
            }
        }
        pattern forwarded(mode: Mode) = /x/ |> choice(mode)
        node P = forwarded(.b)
        entry P
    "#;
    let language = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    )
    .unwrap();
    assert_eq!(language.diagnostics().len(), 1);
    assert_eq!(language.diagnostics()[0].message, "identity");
    let span = &language.diagnostics()[0].primary;
    assert_eq!(&source[span.start..span.end], ".b");
    assert!(!parse(&language, "x").has_errors());
    let invalid = source.replace("forwarded(.b)", "forwarded(.a)");
    let errors = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), invalid.clone())]),
    )
    .unwrap_err();
    let span = &errors[0].primary;
    assert_eq!(&invalid[span.start..span.end], ".a");
    let defaulted = source.replace("node P = forwarded(.b)", "node P = /x/ |> choice()");
    let language =
        compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), defaulted)])).unwrap();
    assert!(language.diagnostics()[0]
        .help
        .as_ref()
        .unwrap()
        .contains("defaults"));
}

#[test]
fn all_match_combinations_must_be_covered_and_cases_reachable() {
    for (cases, code) in [
        ("(.a, .a) => value", "yl.nonexhaustive_match"),
        ("(_, _) => value (.a, .b) => value", "yl.unreachable_case"),
        (
            "(.a, _) => value (.b, _) => value (_, _) => value",
            "yl.unreachable_case",
        ),
        (".a => value", "yl.enum_case"),
        (
            "(_, _) => { warning(\"no replacement\") }",
            "yl.invalid_rewrite",
        ),
        (
            "(.a,.a) => value (_,_) => { error(message=\"bad\") }",
            "yl.invalid_argument",
        ),
    ] {
        let source=format!("enum M {{ a b }} pipe unused(first:M,second:M) {{ rewrite value match (first,second) {{ {cases} }} }} node P = /x/ entry P");
        assert_eq!(
            compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), source)])).unwrap_err()
                [0]
            .code,
            code,
            "{cases}"
        );
    }
}

#[test]
fn nested_guards_reusable_booleans_and_absence_literals() {
    let source = r#"
        constraint enabled(flag=true) { when flag { help("enabled") } }
        node P = name: /x/? {
            constraints {
                enabled()
                when name.isPresent() {
                    help("outer")
                    when name.matches(/x/) { warning("inner") }
                    help("last")
                }
                when !absent { error("never") }
                when absent && false { error("never") }
                when absent || true { help("fallback") }
                when name != absent && name.matches(/x/) { help("guarded") }
            }
        }
        entry P
    "#;
    let language = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    )
    .unwrap();
    assert_eq!(
        parse(&language, "")
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>(),
        vec!["enabled", "fallback"]
    );
    assert_eq!(
        parse(&language, "x")
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>(),
        vec!["enabled", "outer", "inner", "last", "fallback", "guarded"]
    );
}

#[test]
fn new_condition_paths_reject_malformed_input_without_panics() {
    let oversized = std::iter::repeat_n("true", 260)
        .collect::<Vec<_>>()
        .join(" && ");
    let source = format!(
        "node P = /x/ {{ constraints {{ when {oversized} {{ error(\"bad\") }} }} }} entry P"
    );
    assert_eq!(
        compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), source)])).unwrap_err()[0]
            .code,
        "yl.depth_limit"
    );
    let source = r#"import { notAhead } from "core/parser" constraint c(v: notAhead) { when true { warning("bad") } } node P = name: /x/ { constraints { c(name) } } entry P"#;
    assert!(compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())])
    )
    .is_err());
}

#[test]
fn imported_constraint_enum_arguments_keep_their_caller_context() {
    let sources=BTreeMap::from([
        ("main.yl".into(),r#"import { M as Mode, check } from "./checks" node P = name: /x/ { constraints { check(name,Mode::b) } } entry P"#.into()),
        ("checks.yl".into(),r#"export enum M { a b } export constraint check(input,mode) { when mode != .a && input.matches(/x/) { warning("b") } }"#.into()),
    ]);
    assert_eq!(
        parse(&compile_sources("main.yl", &sources).unwrap(), "x").diagnostics[0].message,
        "b"
    );
}
