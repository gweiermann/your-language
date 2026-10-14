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
fn std_types_exports_three_operations_and_only_unknown() {
    let engine = SemanticEngine::default();
    let registry = engine.registry();
    assert!(registry.symbol_id("std/types", "unknown").is_some());
    for name in ["number", "string", "boolean", "function", "Function"] {
        assert!(registry.symbol_id("std/types", name).is_none());
    }
    for name in ["annotate", "restrictTo", "typeof"] {
        assert!(registry.operation_id("std/types", name).is_some());
    }
    for name in [
        "ensure",
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
import * as variables from "std/syntax"
symbol Red
symbol Blue
symbol color
symbol other
node Value = "x" {
 meanings {
  types::annotate((variables::identity(), color), Red)
  types::annotate((variables::identity(), other), Blue)
  match (types::typeof((variables::identity(), color)), types::typeof((variables::identity(), other))) {
   Red, Blue => types::annotate(variables::identity(), Red),
   _ => error("Wrong slot values")
  }
 }
}
entry Value
"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    use your_language::semantics::{SemanticKey, SemanticValue};
    assert!(result
        .graph
        .records
        .iter()
        .any(|record| record.entries.iter().any(|(key, value)| *key
            == SemanticKey::Symbol("std/types#type".into())
            && *value == SemanticValue::Symbol("language.yl#Red".into()))));
    assert!(result
        .graph
        .records
        .iter()
        .any(|record| record.entries.iter().any(|(key, value)| *key
            == SemanticKey::Symbol("std/types#key".into())
            && matches!(value, SemanticValue::Tuple(_)))));
}
#[test]
fn forwarded_type_values_share_authored_slots_without_equality_conflicts() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as variables from "std/syntax"
symbol Red
symbol Blue
symbol field
node Item = "x" { meanings { types::annotate((variables::identity(), field), Red) } }
node Root = item: Item {
 meanings {
  types::annotate(variables::identity(), types::typeof(variables::identityOf(item)))
  types::annotate((variables::identity(), field), Blue)
 }
}
entry Root
"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    for name in ["Root", "Item"] {
        let id = occurrence(&result.occurrences, name);
        assert_eq!(
            types_at(&result, id, Some("language.yl#field")),
            ["language.yl#Blue", "language.yl#Red"]
        );
    }
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
import * as variables from "std/syntax"
symbol Red
node Item = "x" {{ meanings {{ types::annotate({key}, Red) }} }}
entry Item
"#
        );
        let result = analyze(&definition, "x");
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        assert_eq!(result.diagnostics[0].code, "types.key");
    }
}
#[test]
fn list_restrictions_check_arity_and_retain_facts() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as variables from "std/syntax"
symbol Red
symbol Blue
node Item = "x" {
 meanings {
  types::annotate(variables::identity(), [Red, Blue])
  types::restrictTo(variables::identity(), [Red])
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
                contribution: false,
            },
            || Box::new(Operation)
        )
        .is_err());
}

#[test]
fn directional_transfers_retain_unknown_and_known_members() {
    for (left, right) in [("first", "second"), ("second", "first")] {
        let definition = format!(
            r#"
import * as types from "std/types"
import * as variables from "std/syntax"
symbol Red
node First = "x" {{ meanings {{ types::annotate(variables::identity(), types::unknown) }} }}
node Second = "y" {{ meanings {{ types::annotate(variables::identity(), Red) }} }}
node Root = first: First second: Second {{
 meanings {{
  types::annotate(variables::identityOf({left}), types::typeof(variables::identityOf({right})))

 }}
}}
entry Root
"#
        );
        let result = analyze(&definition, "xy");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let destination = occurrence(
            &result.occurrences,
            if left == "first" { "First" } else { "Second" },
        );
        assert_eq!(
            types_at(&result, destination, None),
            ["language.yl#Red", "std/types#unknown"]
        );
    }
}
#[test]
fn a_branch_after_unknown_settlement_can_publish_a_positive_result() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as variables from "std/syntax"
symbol Red
node Root = "x" {
 meanings {
  match (types::typeof(variables::identity())) {
   types::unknown => types::annotate(variables::identity(), Red),
   _ => {}
  }
 }
}
entry Root
"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let id = occurrence(&result.occurrences, "Root");
    assert_eq!(types_at(&result, id, None), ["language.yl#Red"]);
}

#[test]
fn invalid_key_after_a_type_query_does_not_emit_internal_unsettled_errors() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as variables from "std/syntax"
node Root = "x" { meanings { types::annotate("invalid", types::typeof(variables::identity())) } }
entry Root
"#,
        "x",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "types.key");
}

fn occurrence(roots: &[your_language::semantics::Occurrence], name: &str) -> usize {
    for root in roots {
        if root.definition.ends_with(&format!("#{name}")) {
            return root.id;
        }
        if let Some(id) = find_occurrence(&root.children, name) {
            return id;
        }
    }
    panic!("No occurrence {name}")
}
fn find_occurrence(roots: &[your_language::semantics::Occurrence], name: &str) -> Option<usize> {
    for root in roots {
        if root.definition.ends_with(&format!("#{name}")) {
            return Some(root.id);
        }
        if let Some(id) = find_occurrence(&root.children, name) {
            return Some(id);
        }
    }
    None
}
fn types_at(
    result: &your_language::semantics::AnalysisResult,
    id: usize,
    slot: Option<&str>,
) -> Vec<String> {
    use your_language::semantics::{SemanticIdentity, SemanticKey, SemanticValue};
    let identity = SemanticValue::Identity(SemanticIdentity::Occurrence(id));
    let key = if let Some(slot) = slot {
        SemanticValue::Tuple(vec![identity, SemanticValue::Symbol(slot.into())])
    } else {
        identity
    };
    let record = result
        .graph
        .records
        .iter()
        .find(|record| {
            record
                .entries
                .iter()
                .any(|(k, v)| *k == SemanticKey::Symbol("std/types#key".into()) && *v == key)
        })
        .unwrap();
    let value = &record
        .entries
        .iter()
        .find(|(k, _)| *k == SemanticKey::Symbol("std/types#type".into()))
        .unwrap()
        .1;
    let values = if let SemanticValue::Alternatives { values, .. } = value {
        values.clone()
    } else {
        vec![value.clone()]
    };
    values
        .into_iter()
        .map(|value| {
            let SemanticValue::Symbol(value) = value else {
                panic!("symbol")
            };
            value
        })
        .collect()
}
