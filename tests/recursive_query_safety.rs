use std::collections::BTreeMap;
use your_language::{
    load_compiled_language,
    semantics::{AnalysisResult, SemanticEngine, SemanticValue},
};

fn analyze(operations: &str, actions: &str) -> AnalysisResult {
    let source = format!(
        r#"
import {{ contribute, get, getEarlier, equal, isPresent, tail, concat, map, hypothetical, alternatives }} from "core/query"
import {{ error }} from "core/diagnostics"
{operations}
node Program = value: /[a-z]+/ {{ semantics {{ {actions} }} }}
entry Program
"#
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile("safety.yl", &BTreeMap::from([("safety.yl".into(), source)]))
        .unwrap();
    let loaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    engine.analyze(&loaded, "program", "source")
}

fn has_value(result: &AnalysisResult, value: SemanticValue) -> bool {
    result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == value)
}

#[test]
fn direct_and_mutual_nonprogressing_recursion_report_structured_cycles() {
    for operations in [
        "operation loop(value) = loop(value)",
        "operation loop(value) = next(value) operation next(value) = loop(value)",
    ] {
        let result = analyze(operations, "contribute(\"result\", loop(true))");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "query.recursion"),
            "{:?}",
            result.diagnostics
        );
    }
}

#[test]
fn progressing_but_excessively_deep_recursion_reports_resource_limit() {
    let items = vec!["true"; 270].join(",");
    let result = analyze(
        r#"operation visit(items) = match (isPresent(items[0])) {
        false => "done", true => visit(tail(items))
    }"#,
        &format!("contribute(\"result\", visit([{items}]))"),
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "query.resource"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn recursion_retains_pending_dependencies_until_later_contributions_settle() {
    let result = analyze(
        r#"operation join(items) = match (isPresent(items[0])) {
        false => "", true => concat([items[0], join(tail(items))])
    }"#,
        r#"contribute("result", join(get("input"))) contribute("input", ["a", "b"] )"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(has_value(&result, SemanticValue::Text("ab".into())));
}

#[test]
fn hypothetical_recursion_keeps_overrides_isolated_between_calls_and_real_state() {
    let result = analyze(
        r#"operation read(items) = match (isPresent(items[0])) {
        false => get("input"), true => read(tail(items))
    }"#,
        r#"
        contribute("input", "base")
        contribute("derived", read([true]))
        contribute("left", hypothetical("derived", [("input", "left")]))
        contribute("right", hypothetical("derived", [("input", "right")]))
    "#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    for value in ["base", "left", "right"] {
        assert!(
            has_value(&result, SemanticValue::Text(value.into())),
            "{value}: {:?}",
            result.graph.query_entries
        );
    }
}

#[test]
fn recursive_callback_values_preserve_source_evidence_for_annotations() {
    let result = analyze(
        r#"operation lastValue(items) = match (isPresent(items[1])) {
        true => lastValue(tail(items)), false => items[0]
    }"#,
        r#"error("source retained", map([lastValue([value])], item => (item, absent)))"#,
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].annotations.len(), 1);
    assert_eq!(result.diagnostics[0].primary.file, "program");
    assert_eq!(
        (
            result.diagnostics[0].primary.start,
            result.diagnostics[0].primary.end
        ),
        (0, 6)
    );
}

#[test]
fn malformed_function_images_are_rejected_without_panicking() {
    let source = r#"import { contribute, isPresent, tail } from "core/query"
        operation walk(items) = match (isPresent(items[0])) {
            false => true, true => walk(tail(items))
        }
        node Program = "x" { semantics { contribute("result", walk([true])) } }
        entry Program"#;
    let language = SemanticEngine::default()
        .compile(
            "image.yl",
            &BTreeMap::from([("image.yl".into(), source.into())]),
        )
        .unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    let functions = original["queries"]["functions"].as_object().unwrap();
    let name = functions
        .keys()
        .find(|name| name.contains("walk"))
        .unwrap()
        .clone();
    let body = original["queries"]["functions"][&name]["body"]
        .as_u64()
        .unwrap() as usize;
    let invoke = original["queries"]["expressions"]
        .as_array()
        .unwrap()
        .iter()
        .position(|expression| expression.get("Invoke").is_some())
        .unwrap();
    for mutation in 0..11 {
        let mut image = original.clone();
        match mutation {
            0 => image["queries"]["functions"][&name]["body"] = u32::MAX.into(),
            1 => {
                image["queries"]["functions"][&name]["parameters"] =
                    serde_json::json!(["item", "item"])
            }
            2 => {
                image["queries"]["expressions"][invoke]["Invoke"]["name"] =
                    "missing-function".into()
            }
            3 => image["queries"]["expressions"][invoke]["Invoke"]["args"] = serde_json::json!([]),
            4 => image["queries"]["expressions"][body] = "SelfValue".into(),
            5 => {
                image["queries"]["expressions"][body] = serde_json::json!({"Capture":"undeclared"})
            }
            6 => image["queries"]["expressions"][body] = serde_json::json!({"Local":"unbound"}),
            7 => {
                image["queries"]["expressions"][body]["Match"]["cases"][0]["alternatives"] =
                    serde_json::json!([[]])
            }
            8 => {
                image["queries"]["expressions"][body]["Match"]["cases"][0]["value"] =
                    u32::MAX.into()
            }
            9 => {
                image["queries"]["expressions"][body]["Match"]["cases"][0]["alternatives"][0][0] =
                    (invoke as u32).into()
            }
            _ => {
                image["queries"]["expressions"][body]["Match"]["cases"][0]["alternatives"][0][0] =
                    serde_json::Value::Null
            }
        }
        let bytes = serde_json::to_vec(&image).unwrap();
        let loaded = std::panic::catch_unwind(|| load_compiled_language(&bytes));
        assert!(loaded.is_ok(), "image mutation {mutation} panicked");
        assert!(
            loaded.unwrap().is_err(),
            "image mutation {mutation} was accepted"
        );
    }
}

#[test]
fn recursive_alternative_matches_do_not_override_the_next_parameter_frame() {
    let result = analyze(
        r#"operation visit(items) = match (items[0]) {
        true => visit(tail(items)), false => "stopped", _ => "empty"
    }"#,
        r#"contribute("result", visit([alternatives([true, false]), false]))"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.graph.query_entries.iter().any(|entry| matches!(&entry.value,
        SemanticValue::Alternatives { values, .. } if values == &[SemanticValue::Text("stopped".into())]
    )), "{:?}", result.graph.query_entries);
}

#[test]
fn recursive_calls_keep_closed_query_narrowing_from_the_selected_match_branch() {
    let result = analyze(
        r#"operation read(items) = match (isPresent(items[0])) {
        true => read(tail(items)), false => get("switch")
    }
    operation choose() = match (get("switch")) {
        true => read([true]), false => false
    }"#,
        r#"contribute("switch", alternatives([true, false])) contribute("result", choose())"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.graph.query_entries.iter().any(|entry| matches!(&entry.value,
        SemanticValue::Alternatives { values, .. } if values == &[SemanticValue::Bool(true), SemanticValue::Bool(false)]
    )), "{:?}", result.graph.query_entries);
    assert!(!result.graph.query_entries.iter().any(|entry| matches!(&entry.value,
        SemanticValue::Alternatives { values, .. } if values.iter().any(|value| matches!(value, SemanticValue::Alternatives { .. }))
    )), "Nested alternatives leaked from a closed-query branch: {:?}", result.graph.query_entries);
}

#[test]
fn unused_bindings_cannot_hide_effects_in_recursive_functions() {
    let source = r#"import { contribute, isPresent, tail } from "core/query"
        operation walk(items) {
            unused := contribute("hidden", true)
            return match (isPresent(items[0])) { false => true, true => walk(tail(items)) }
        }
        node Program = "x" { semantics { contribute("result", walk([true])) } }
        entry Program"#;
    let result = SemanticEngine::default().compile(
        "effects.yl",
        &BTreeMap::from([("effects.yl".into(), source.into())]),
    );
    assert!(
        result.is_err(),
        "An unused binding silently erased a forbidden recursive effect"
    );
}

#[test]
fn recursive_active_argument_copies_are_bounded_before_unwinding() {
    let item = format!("\"{}\"", "x".repeat(4096));
    let items = vec![item; 80].join(",");
    let result = analyze(
        r#"operation visit(items) = match (isPresent(items[0])) {
        false => true, true => visit(tail(items))
    }"#,
        &format!("contribute(\"result\", visit([{items}]))"),
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "query.resource"
                && diagnostic.message.contains("memory budget")),
        "{:?}",
        result.diagnostics
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("work/depth")),
        "Allocation should be bounded while fewer than 80 frames are active: {:?}",
        result.diagnostics
    );
}

#[test]
fn closed_query_narrowing_survives_locally_bound_callback_parameters() {
    let result = analyze(
        r#"operation read(items) = match (isPresent(items[0])) {
        true => read(tail(items)), false => map([get("switch")], item => item)[0]
    }
    operation choose() = match (map([get("switch")], item => item)[0]) {
        true => read([true]), false => false
    }"#,
        r#"contribute("switch", alternatives([true, false])) contribute("result", choose())"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(!result.graph.query_entries.iter().any(|entry| matches!(&entry.value,
        SemanticValue::Alternatives { values, .. } if values.iter().any(|value| matches!(value, SemanticValue::Alternatives { .. }))
    )), "A locally bound callback parameter is not a free variable: {:?}", result.graph.query_entries);
}
