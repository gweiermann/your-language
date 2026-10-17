use std::collections::BTreeMap;
use your_language::{
    load_compiled_language,
    semantics::{AnalysisResult, SemanticEngine, SemanticValue},
};

fn analyze(operations: &str, expression: &str) -> AnalysisResult {
    let source = format!(
        r#"import {{ contribute, equal, isPresent, tail, concat, map, get }} from "core/query"
        {operations}
        node Program = "x" {{ semantics {{ contribute((self, "result"), {expression}) }} }}
        entry Program"#
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source)]),
        )
        .unwrap();
    let loaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    engine.analyze(&loaded, "program", "x")
}

#[test]
fn pure_recursive_operations_execute_after_artifact_reload() {
    let result = analyze(
        r#"operation join(parts) = match (isPresent(parts[0])) {
            false => "",
            true => concat([parts[0], join(tail(parts))])
        }"#,
        r#"join(["a", "b", "c"])"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == SemanticValue::Text("abc".into())));
}

#[test]
fn mutual_recursion_and_nested_callback_value_matches_keep_locals() {
    let result = analyze(
        r#"operation even(parts) = match (isPresent(parts[0])) {
            false => true,
            true => odd(tail(parts))
        }
        operation odd(parts) = match (isPresent(parts[0])) {
            false => false,
            true => even(tail(parts))
        }"#,
        r#"map([["a", "b"], ["a"]], item => match (even(item)) { true => "even", false => "odd" })"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.graph.query_entries.iter().any(|entry| entry.value
        == SemanticValue::List(vec![
            SemanticValue::Text("even".into()),
            SemanticValue::Text("odd".into())
        ])));
}

#[test]
fn value_match_only_evaluates_the_selected_arm() {
    let result = analyze(
        r#"operation result() = match (true, false) {
            true, false => "selected",
            _ => get("unavailable")
        }"#,
        "result()",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == SemanticValue::Text("selected".into())));
}

#[test]
fn boolean_operators_short_circuit_without_importing_choose() {
    let result = analyze(
        r#"operation result() = [false && get("missing"), true || get("missing"), !false && true || false]"#,
        "result()",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.graph.query_entries.iter().any(|entry| entry.value
        == SemanticValue::List(vec![
            SemanticValue::Bool(false),
            SemanticValue::Bool(true),
            SemanticValue::Bool(true)
        ])));
}

#[test]
fn value_matches_require_exhaustive_constant_patterns_and_correct_tuple_arity() {
    for body in [
        "match (true, false) { true => true, _ => false }",
        "match (true) { true => true }",
        "match (true) { _ => true, false => false }",
        "match (true) { equal(true, true) => true, _ => false }",
    ] {
        let source = format!("import {{ equal }} from \"core/query\" operation bad() = {body} node Program = \"x\" entry Program");
        assert!(
            SemanticEngine::default()
                .compile(
                    "language.yl",
                    &BTreeMap::from([("language.yl".into(), source)])
                )
                .is_err(),
            "{body}"
        );
    }
}

#[test]
fn complete_enum_patterns_do_not_need_a_wildcard() {
    let result = analyze(
        r#"enum Choice { first second }
        operation choose(value) = match (value) {
            Choice::first => "one",
            Choice::second => "two"
        }"#,
        "choose(Choice::second)",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == SemanticValue::Text("two".into())));
}

#[test]
fn block_helpers_keep_compile_time_capture_projections() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "projection.yl",
            &BTreeMap::from([(
                "projection.yl".into(),
                r#"
        import { contribute } from "core/query"
        operation field(value) { selected := value.name return selected }
        operation outer(value) { return field(value) }
        node Box = name: /x/
        node Program = item: Box { semantics { contribute("result", outer(item)) } }
        entry Program
    "#
                .into(),
            )]),
        )
        .unwrap();
    let result = engine.analyze(
        &load_compiled_language(&language.to_bytes().unwrap()).unwrap(),
        "program",
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.graph.query_entries.iter().any(
        |entry| matches!(&entry.value, SemanticValue::Source(capture) if capture.text == "x")
    ));
}

#[test]
fn compiled_pure_block_defaults_keep_each_callers_effect_identity() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "defaults.yl",
            &BTreeMap::from([(
                "defaults.yl".into(),
                r#"
        import { contribute } from "core/query"
        operation marked() { contribute("markers", true) return true }
        operation pure(value = marked()) { return value }
        node Program = "x" {
            semantics {
                contribute("one", pure())
                contribute("two", pure())
            }
        }
        entry Program
    "#
                .into(),
            )]),
        )
        .unwrap();
    let result = engine.analyze(
        &load_compiled_language(&language.to_bytes().unwrap()).unwrap(),
        "program",
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .graph
            .query_entries
            .iter()
            .filter(|entry| entry.key
                == your_language::semantics::queries::QueryKey::Text("markers".into()))
            .count(),
        2,
        "{:?}",
        result.graph.query_entries
    );
}
