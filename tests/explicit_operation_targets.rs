use std::collections::BTreeMap;
use your_language::{
    diagnostic::Diagnostic,
    semantics::{AnalysisResult, SemanticEngine},
};

fn compile(source: &str) -> Result<your_language::CompiledLanguage, Vec<Diagnostic>> {
    SemanticEngine::default().compile(
        "test.yl",
        &BTreeMap::from([("test.yl".into(), source.into())]),
    )
}
fn analyze(source: &str) -> AnalysisResult {
    let engine = SemanticEngine::default();
    let language = compile(source).unwrap();
    let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&language, "source", "x");
    assert_eq!(result, engine.analyze(&reloaded, "source", "x"));
    result
}
fn grammar(operations: &str, meanings: &str) -> String {
    format!(
        r#"import {{ error, warn, hint }} from "core/diagnostics"
import {{ get, contribute, contributeAll, facts, isPresent, equal, choose, text, map, first }} from "core/query"
import * as types from "std/types"
symbol number
symbol slot
{operations}
node Item = "y"
node Root = "x" optional: Item? {{ semantics {{ {meanings} }} }}
entry Root"#
    )
}

#[test]
fn unused_operations_cannot_capture_contextual_self() {
    for operation in [
        "operation invalid() = self",
        "operation invalid() { contribute((self, slot), number) }",
        "operation invalid() = map([], item => self)",
        "operation invalid(target = self) = target",
        "operation invalid() { when true { error(\"Hidden ${self}\") } }",
    ] {
        let errors = compile(&grammar(operation, "")).unwrap_err();
        assert_eq!(errors[0].code, "yl.operation_self", "{errors:?}");
        if !operation.contains("error(") {
            assert_eq!(
                &grammar(operation, "")[errors[0].primary.start..errors[0].primary.end],
                "self"
            );
        }
    }
}

#[test]
fn self_can_be_an_explicit_parameter_and_is_forwarded_through_wrappers() {
    let result = analyze(&grammar(
        "operation nested(self, value) { types::annotateWithType(self, value) } operation wrapper(target, value) { nested(target, value) }",
        "wrapper(self, number)",
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.graph.query_entries.len(), 1);
    assert!(
        result.graph.query_entries[0].value
            == your_language::semantics::SemanticValue::Symbol("test.yl#number".into())
    );
}

#[test]
fn absent_optional_argument_ends_only_dependent_actions() {
    let result = analyze(&grammar(
        r#"operation apply(target, annotation) {
            types::annotateWithType(target, types::typeOf(annotation))
            types::restrictTypeTo(target, types::typeOf(annotation))
            contribute((target, slot), "unaffected")
            warn("Still executed", target)
        }"#,
        "apply(self, optional)",
    ));
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].message, "Still executed");
    assert_eq!(result.graph.query_entries.len(), 1);
    assert_eq!(
        result.graph.query_entries[0].value,
        your_language::semantics::SemanticValue::Text("unaffected".into())
    );
}

#[test]
fn absent_queries_do_not_turn_missing_syntax_into_unknown() {
    let result = analyze(&grammar(
        "",
        r#"
        types::annotateWithType(self, types::typeOf(optional))
        contribute((self, slot), [optional, types::typeOf(optional), types::typeOf(self)])
        when !equal(types::typeOf(optional), absent) { error("Missing syntax became unknown") }
        when !equal(types::typeOf(self), types::unknown) { error("Untyped syntax lost unknown") }
        when text(optional) { error("Absent condition entered") } else { error("Absent condition entered else") }
    "#,
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.graph.query_entries.len(), 1);
    assert_eq!(
        result.graph.query_entries[0].value,
        your_language::semantics::SemanticValue::List(vec![
            your_language::semantics::SemanticValue::Absent,
            your_language::semantics::SemanticValue::Absent,
            your_language::semantics::SemanticValue::Symbol("std/types#unknown".into()),
        ])
    );
}

#[test]
fn absent_key_components_rule_out_pending_paths_in_either_order() {
    for key in ["(get((self, slot)), absent)", "(absent, get((self, slot)))"] {
        let result = analyze(&grammar(
            "",
            &format!(
                r#"
            contribute({key}, get((self, slot)))
            contribute((self, slot), choose(isPresent(get({key})), "wrong", "settled"))
        "#
            ),
        ));
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.graph.query_entries.len(), 1);
        assert_eq!(
            result.graph.query_entries[0].value,
            your_language::semantics::SemanticValue::Text("settled".into())
        );
    }
}

#[test]
fn pending_key_with_absent_value_is_not_a_possible_producer() {
    let result = analyze(&grammar(
        "",
        r#"
        contribute(get((self, slot)), absent)
        contribute((self, slot), choose(isPresent(get((self, "missing"))), "wrong", "settled"))
    "#,
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.graph.query_entries.len(), 1);
}

#[test]
fn strict_queries_with_pending_and_absent_arguments_settle_without_cycles() {
    for expression in [
        "contains(get((self, slot)), absent)",
        "contains(absent, get((self, slot)))",
    ] {
        let source = grammar(
            "",
            &format!(
                r#"
            contribute((self, slot), choose(isPresent({expression}), "wrong", "settled"))
        "#
            ),
        )
        .replace("first }", "first, contains }");
        let result = analyze(&source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.graph.query_entries.len(), 1);
        assert_eq!(
            result.graph.query_entries[0].value,
            your_language::semantics::SemanticValue::Text("settled".into())
        );
    }
}

#[test]
fn repeated_optional_types_keep_positions_but_absent_contributions_add_no_fact() {
    let result = analyze(&grammar(
        "",
        r#"
        contributeAll((self, slot), [absent, number])
        contribute((self, "shape"), map([optional, self], value => types::typeOf(value)))
    "#,
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.graph.query_entries.len(), 2);
    assert!(result.graph.query_entries.iter().any(|entry| entry.value
        == your_language::semantics::SemanticValue::List(vec![
            your_language::semantics::SemanticValue::Absent,
            your_language::semantics::SemanticValue::Symbol("std/types#unknown".into())
        ])));
    assert!(!result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == your_language::semantics::SemanticValue::Absent));
}
