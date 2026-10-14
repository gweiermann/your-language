use std::collections::BTreeMap;
use your_language::semantics::{SemanticEngine, SemanticValue};

fn run(actions: &str) -> your_language::semantics::AnalysisResult {
    let grammar = format!(
        r#"import {{ contribute, contributeAll, get, facts, first, equal, hypothetical, find }} from "core/query"
 node Program = "x" {{ semantics {{ {actions} }} }} entry Program"#
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "review.yl",
            &BTreeMap::from([("review.yl".into(), grammar)]),
        )
        .unwrap();
    engine.analyze(&language, "program", "x")
}

#[test]
fn transient_failure_should_not_remain() {
    let result = run(
        r#"when first(facts("flag")) { contribute("result", "yes") } contribute("flag", true)"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn nested_callback_shadowing_uses_the_innermost_binding() {
    let result = run(r#"contribute("result", find(["a"], item => find(["b"], item => item)))"#);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::Text("b".into())
    );
}

#[test]
fn facts_negative_cycle_is_not_silently_absent() {
    let result =
        run(r#"when equal(first(facts("cycle")), absent) { contribute("cycle", "present") }"#);
    assert!(!result.diagnostics.is_empty(), "{:?}", result);
}

#[test]
fn hypothetical_convergence_bound_should_diagnose() {
    let result = run(
        r#"when equal(first(facts("cycle")), absent) { contribute("cycle", "present") } contribute("trial", hypothetical("cycle", []))"#,
    );
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "query.nonconvergent"));
}

#[test]
fn competing_contributions_get_conflict_diagnostic() {
    let result = run(
        r#"contribute("value", "a") contribute("value", "b") contribute("result", get("value"))"#,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "query.conflict"),
        "{:?}",
        result
    );
}

#[test]
fn isolated_nonconvergent_hypothetical_reports_error() {
    let result = run(r#"contribute("parameter", false)
 when equal(get("parameter"), true) {
    when equal(first(facts("cycle")), absent) { contribute("cycle", "present") }
 }
 contribute("trial", hypothetical("cycle", [("parameter", true)]))"#);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "query.nonconvergent"),
        "{:?}",
        result
    );
}

#[test]
fn nested_callbacks_can_read_both_lexical_bindings() {
    let result =
        run(r#"contribute("result", find(["a"], outer => find(["b"], inner => (outer, inner))))"#);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::Tuple(vec![
            SemanticValue::Text("a".into()),
            SemanticValue::Text("b".into())
        ])
    );
}

#[test]
fn late_dynamic_keys_settle_in_every_producer_order() {
    let actions = [
        r#"contribute(get("dispatch"), "answer")"#,
        r#"contribute("dispatch", "target")"#,
        r#"contribute("result", get("target"))"#,
    ];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let result = run(&order
            .iter()
            .map(|i| actions[*i])
            .collect::<Vec<_>>()
            .join("\n"));
        assert!(
            result.diagnostics.is_empty(),
            "{:?}: {:?}",
            order,
            result.diagnostics
        );
        assert!(result.graph.query_entries.iter().any(|entry| entry.key
            == your_language::semantics::queries::QueryKey::Text("result".into())
            && entry.value == SemanticValue::Text("answer".into())));
    }
}

#[test]
fn ruled_out_dynamic_key_does_not_block_unrelated_query() {
    let result = run(
        r#"when false { contribute(get("undefined"), "never") } contribute("result", get("missing")) contribute("independent", "answer")"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.graph.query_entries.len(), 1);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::Text("answer".into())
    );
}
