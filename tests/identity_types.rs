use std::collections::BTreeMap;
use your_language::semantics::SemanticEngine;
fn analyze(definition: &str, source: &str) -> your_language::semantics::AnalysisResult {
    let engine = SemanticEngine::default();
    let sources = BTreeMap::from([("language.yl".into(), definition.into())]);
    let language = engine.compile("language.yl", &sources).unwrap();
    let language = engine.load(&language.to_bytes().unwrap()).unwrap();
    engine.analyze(&language, "input", source)
}
#[test]
fn std_types_exports_two_operations_and_only_unknown() {
    let engine = SemanticEngine::default();
    let registry = engine.registry();
    assert!(registry.symbol_id("std/types", "unknown").is_some());
    for name in ["number", "string", "boolean", "function", "Function"] {
        assert!(registry.symbol_id("std/types", name).is_none());
    }
    for name in ["ensure", "typeof"] {
        assert!(registry.operation_id("std/types", name).is_some());
    }
    for name in [
        "types",
        "configureTypes",
        "type",
        "sameType",
        "inheritType",
        "functionType",
        "callType",
    ] {
        assert!(registry.operation_id("std/types", name).is_none());
    }
}
#[test]
fn arbitrary_slots_are_independent_and_matches_resolve_their_values() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as variables from "std/variables"
symbol Red
symbol Blue
symbol color
symbol other
node Value = "x" {
 meanings {
  types::ensure((variables::identity(), color), Red)
  types::ensure((variables::identity(), other), Blue)
  match (types::typeof((variables::identity(), color)), types::typeof((variables::identity(), other))) {
   Red, Blue => types::ensure(variables::identity(), Red),
   _ => error("Wrong slot values")
  }
 }
}
entry Value
"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
#[test]
fn equality_of_types_propagates_language_authored_slots() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as variables from "std/variables"
symbol Red
symbol Blue
symbol field
node Item = "x" { meanings { types::ensure((variables::identity(), field), Red) } }
node Root = item: Item {
 meanings {
  types::ensure(variables::identity(), types::typeof(variables::identity(item)))
  types::ensure((variables::identity(), field), Blue)
 }
}
entry Root
"#,
        "x",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "types.mismatch");
}
#[test]
fn invalid_keys_are_diagnostics_instead_of_panics() {
    for key in [
        "\"not an identity\"",
        "(variables::identity(), \"not a symbol\")",
        "(variables::identity(), Red, Red)",
    ] {
        let definition = format!(
            r#"
import * as types from "std/types"
import * as variables from "std/variables"
symbol Red
node Item = "x" {{ meanings {{ types::ensure({key}, Red) }} }}
entry Item
"#
        );
        let result = analyze(&definition, "x");
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        assert_eq!(result.diagnostics[0].code, "types.key");
    }
}
#[test]
fn list_equality_checks_arity_and_retains_elements() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as variables from "std/variables"
symbol Red
symbol Blue
node Item = "x" {
 meanings {
  types::ensure(variables::identity(), [Red, Blue])
  types::ensure(variables::identity(), [Red])
 }
}
entry Item
"#,
        "x",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "types.mismatch");
}

#[test]
fn a_live_context_cannot_claim_finalization_only_effects() {
    use your_language::semantics::{NativeOperation, OperationRegistry, OperationSignature};
    struct Operation;
    impl NativeOperation for Operation {}
    let mut registry = OperationRegistry::new();
    assert!(registry
        .register(
            "test",
            "invalid",
            OperationSignature {
                id: "test#invalid".into(),
                parameters: vec![],
                provides_context: true,
                finalization_only: true,
            },
            || Box::new(Operation)
        )
        .is_err());
}
