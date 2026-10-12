use std::collections::BTreeMap;
use your_language::semantics::{
    AnalysisResult, Occurrence, SemanticEngine, SemanticIdentity, SemanticKey, SemanticValue,
};

fn compile(engine: &SemanticEngine, source: &str) -> your_language::CompiledLanguage {
    let sources = BTreeMap::from([("language.yl".into(), source.into())]);
    let language = engine.compile("language.yl", &sources).unwrap();
    engine.load(&language.to_bytes().unwrap()).unwrap()
}
fn analyze(source: &str, input: &str) -> AnalysisResult {
    let engine = SemanticEngine::default();
    let language = compile(&engine, source);
    engine.analyze(&language, "program", input)
}
fn identity(roots: &[Occurrence], name: &str) -> SemanticValue {
    fn find(roots: &[Occurrence], name: &str) -> Option<usize> {
        for root in roots {
            if root.definition.ends_with(&format!("#{name}")) {
                return Some(root.id);
            }
            if let Some(id) = find(&root.children, name) {
                return Some(id);
            }
        }
        None
    }
    SemanticValue::Identity(SemanticIdentity::Occurrence(find(roots, name).unwrap()))
}
fn members(result: &AnalysisResult, name: &str, slot: Option<&str>) -> Vec<SemanticValue> {
    let id = identity(&result.occurrences, name);
    let key = if let Some(slot) = slot {
        SemanticValue::Tuple(vec![
            id,
            SemanticValue::Symbol(format!("language.yl#{slot}")),
        ])
    } else {
        id
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
    if let SemanticValue::Alternatives { values, .. } = value {
        values.clone()
    } else {
        vec![value.clone()]
    }
}
fn atom(name: &str) -> SemanticValue {
    SemanticValue::Symbol(format!("language.yl#{name}"))
}
fn clean(result: &AnalysisResult) {
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
const IMPORTS: &str = r#"import * as types from "std/types"
import * as syntax from "std/syntax"
symbol Quartz
symbol Orchid
symbol slot
"#;

#[test]
fn contributions_accumulate_distinct_members_without_equality_conflicts() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{
    types::annotate(syntax::identity(), Quartz)
    types::annotate(syntax::identity(), Orchid)
    types::annotate(syntax::identity(), Quartz)
}} }}
entry Root"#
        ),
        "x",
    );
    clean(&result);
    assert_eq!(
        members(&result, "Root", None),
        [atom("Orchid"), atom("Quartz")]
    );
}

#[test]
fn restrictions_require_every_known_member_with_precise_fact_and_bound_spans() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as syntax from "std/syntax"
symbol number
symbol string
node Value = /"[a-z]+"/ { meanings { types::annotate(syntax::identity(), string) } }
node Annotation = "number" { meanings { types::annotate(syntax::identity(), number) } }
node Root = value: Value annotation: Annotation { meanings {
    types::annotate(syntax::identity(), types::typeof(syntax::identityOf(value)))
    types::annotate(syntax::identity(), types::typeof(syntax::identityOf(annotation)))
    types::restrictTo(syntax::identity(), types::typeof(syntax::identityOf(annotation)))
} }
entry Root
"#,
        "\"wrong\"number",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "types.mismatch");
    assert_eq!(
        (
            &diagnostic.primary.file,
            diagnostic.primary.start,
            diagnostic.primary.end
        ),
        (&"program".to_owned(), 0, 7)
    );
    assert_eq!(diagnostic.secondary.len(), 1);
    assert_eq!(
        (diagnostic.secondary[0].start, diagnostic.secondary[0].end),
        (7, 13)
    );
    assert_eq!(
        members(&result, "Root", None),
        [atom("number"), atom("string")]
    );
}

#[test]
fn explicit_unknown_does_not_hide_an_incompatible_known_member() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{
    types::annotate(syntax::identity(), types::unknown)
    types::annotate(syntax::identity(), Orchid)
    types::restrictTo(syntax::identity(), Quartz)
}} }}
entry Root"#
        ),
        "x",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "types.mismatch");
    assert_eq!(
        members(&result, "Root", None),
        [
            atom("Orchid"),
            SemanticValue::Symbol("std/types#unknown".into())
        ]
    );
}

#[test]
fn unobserved_key_uses_allowed_fallback_without_publishing_a_fact() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{
    types::restrictTo(syntax::identity(), Quartz)
    match (types::typeof(syntax::identity())) {{
        Quartz => {{}},
        _ => error("restriction fallback was lost"),
    }}
}} }}
entry Root"#
        ),
        "x",
    );
    clean(&result);
    assert_eq!(members(&result, "Root", None), [atom("Quartz")]);
}

#[test]
fn missing_evidence_is_pending_until_unknown_settlement_and_then_can_refine() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{
    match (types::typeof(syntax::identity())) {{
        types::unknown => types::annotate(syntax::identity(), Quartz),
        Quartz => {{}},
        _ => error("unexpected fact"),
    }}
}} }}
entry Root"#
        ),
        "x",
    );
    clean(&result);
    assert_eq!(members(&result, "Root", None), [atom("Quartz")]);
}

#[test]
fn matches_register_late_facts_without_losing_existing_facts() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{
    types::annotate(syntax::identity(), Quartz)
    match (types::typeof(syntax::identity())) {{
        Quartz => types::annotate(syntax::identity(), Orchid),
        Orchid => types::annotate((syntax::identity(), slot), Orchid),
        _ => error("unexpected fact"),
    }}
}} }}
entry Root"#
        ),
        "x",
    );
    clean(&result);
    assert_eq!(
        members(&result, "Root", None),
        [atom("Orchid"), atom("Quartz")]
    );
    assert_eq!(members(&result, "Root", Some("slot")), [atom("Orchid")]);
}

#[test]
fn primitive_facts_flow_one_way_without_constraining_the_source() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Source = "x" {{ meanings {{ types::annotate(syntax::identity(), Quartz) }} }}
node Target = "y" {{ meanings {{ types::annotate(syntax::identity(), Orchid) }} }}
node Root = source: Source target: Target {{ meanings {{
    types::annotate(syntax::identityOf(target), types::typeof(syntax::identityOf(source)))
}} }}
entry Root"#
        ),
        "xy",
    );
    clean(&result);
    assert_eq!(members(&result, "Source", None), [atom("Quartz")]);
    assert_eq!(
        members(&result, "Target", None),
        [atom("Orchid"), atom("Quartz")]
    );
}

#[test]
fn read_cycles_without_evidence_settle_unknown_and_seeded_cycles_propagate() {
    for seeded in [false, true] {
        let seed = if seeded {
            "types::annotate(syntax::identityOf(left), Quartz)"
        } else {
            ""
        };
        let result = analyze(
            &format!(
                r#"{IMPORTS}
node Left = "x"
node Right = "y"
node Root = left: Left right: Right {{ meanings {{
    {seed}
    types::annotate(syntax::identityOf(left), types::typeof(syntax::identityOf(right)))
    types::annotate(syntax::identityOf(right), types::typeof(syntax::identityOf(left)))
}} }}
entry Root"#
            ),
            "xy",
        );
        clean(&result);
        let expected = if seeded {
            atom("Quartz")
        } else {
            SemanticValue::Symbol("std/types#unknown".into())
        };
        assert_eq!(members(&result, "Left", None), [expected.clone()]);
        assert_eq!(members(&result, "Right", None), [expected]);
    }
}

#[test]
fn recursively_growing_list_shape_is_diagnosed_before_expansion() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{
    types::annotate(syntax::identity(), [types::typeof(syntax::identity())])
}} }}
entry Root"#
        ),
        "x",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "types.mismatch");
    assert!(result.diagnostics[0]
        .message
        .to_lowercase()
        .contains("recursive"));
}

#[test]
fn correlated_lists_remain_two_facts_instead_of_four_products() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Left = "x" {{ meanings {{ types::annotate(syntax::identity(), [Quartz, Orchid]) }} }}
node Right = "y" {{ meanings {{ types::annotate(syntax::identity(), [Orchid, Quartz]) }} }}
node Root = left: Left right: Right {{ meanings {{
    types::annotate(syntax::identity(), types::typeof(syntax::identityOf(left)))
    types::annotate(syntax::identity(), types::typeof(syntax::identityOf(right)))
}} }}
entry Root"#
        ),
        "xy",
    );
    clean(&result);
    assert_eq!(
        members(&result, "Root", None),
        [
            SemanticValue::List(vec![atom("Orchid"), atom("Quartz")]),
            SemanticValue::List(vec![atom("Quartz"), atom("Orchid")])
        ]
    );
}

#[test]
fn positional_key_lists_receive_elements_and_reject_wrong_arity() {
    for (value, error) in [("[Quartz, Orchid]", false), ("[Quartz]", true)] {
        let result = analyze(
            &format!(
                r#"{IMPORTS}
node Left = "x"
node Right = "y"
node Root = left: Left right: Right {{ meanings {{
    types::annotate([syntax::identityOf(left), syntax::identityOf(right)], {value})
}} }}
entry Root"#
            ),
            "xy",
        );
        if error {
            assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
            assert_eq!(result.diagnostics[0].code, "types.mismatch");
        } else {
            clean(&result);
            assert_eq!(members(&result, "Left", None), [atom("Quartz")]);
            assert_eq!(members(&result, "Right", None), [atom("Orchid")]);
        }
    }
}

#[test]
fn native_kernel_and_syntax_identity_work_without_variable_library_and_are_isolated() {
    use your_language::semantics::{syntax, types, OperationRegistry};
    let mut registry = OperationRegistry::new();
    syntax::register(&mut registry).unwrap();
    types::register(&mut registry).unwrap();
    assert!(registry.operation_id("std/variables", "identity").is_none());
    let engine = SemanticEngine::new(registry);
    let language = compile(
        &engine,
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{ types::annotate(syntax::identity(), Quartz) }} }}
entry Root"#
        ),
    );
    std::thread::scope(|scope| {
        let first = scope.spawn(|| engine.analyze(&language, "program", "x"));
        let second = scope.spawn(|| engine.analyze(&language, "program", "x"));
        let first = first.join().unwrap();
        let second = second.join().unwrap();
        clean(&first);
        clean(&second);
        assert_eq!(first.graph, second.graph);
        assert_eq!(members(&first, "Root", None), [atom("Quartz")]);
    });
}

#[test]
fn late_slot_contribution_through_alias_reaches_original_structure_handle() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Original = "x" {{ meanings {{ types::annotate(syntax::identity(), Quartz) }} }}
node Alias = "y"
node Root = original: Original alias: Alias {{ meanings {{
    types::annotate(syntax::identityOf(alias), types::typeof(syntax::identityOf(original)))
    match (types::typeof(syntax::identityOf(alias))) {{
        Quartz => types::annotate((syntax::identityOf(alias), slot), Orchid),
        _ => error("alias lost its value"),
    }}
}} }}
entry Root"#
        ),
        "xy",
    );
    clean(&result);
    for name in ["Original", "Alias"] {
        assert_eq!(members(&result, name, None), [atom("Quartz")]);
        assert_eq!(members(&result, name, Some("slot")), [atom("Orchid")]);
    }
}

#[test]
fn contribution_capability_is_serialized_and_plain_query_false_remains_omitted() {
    let engine = SemanticEngine::default();
    let language = compile(
        &engine,
        &format!(
            r#"{IMPORTS}
operation annotateWithType(value) {{ types::annotate(syntax::identity(), value) }}
node Root = "x" {{ meanings {{
    annotateWithType(Quartz)
    types::restrictTo(syntax::identity(), Quartz)
    match (types::typeof(syntax::identity())) {{ Quartz => {{}}, _ => {{}} }}
}} }}
entry Root"#
        ),
    );
    let mut json: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    fn contracts(value: &serde_json::Value, output: &mut BTreeMap<String, Option<bool>>) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(contract) = object.get("contract") {
                    output.insert(
                        contract["id"].as_str().unwrap().into(),
                        contract.get("contribution").map(|v| v.as_bool().unwrap()),
                    );
                }
                for value in object.values() {
                    contracts(value, output);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    contracts(value, output);
                }
            }
            _ => {}
        }
    }
    let mut values = BTreeMap::new();
    contracts(&json, &mut values);
    assert_eq!(values["std/types#annotate"], Some(true));
    assert_eq!(values["std/types#restrictTo"], Some(true));
    assert_eq!(values["std/types#typeof"], None);
    assert_eq!(values["std/syntax#identity"], None);
    clean(&engine.analyze(&language, "program", "x"));
    fn remove_capability(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(contract) = object.get_mut("contract") {
                    if contract["id"] == "std/types#annotate" {
                        contract.as_object_mut().unwrap().remove("contribution");
                    }
                }
                for value in object.values_mut() {
                    remove_capability(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    remove_capability(value);
                }
            }
            _ => {}
        }
    }
    remove_capability(&mut json);
    let errors = engine
        .load(&serde_json::to_vec(&json).unwrap())
        .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.code == "semantic.contract_mismatch"),
        "{errors:?}"
    );
}

#[test]
fn positional_restrictions_keep_joint_list_correlation() {
    let source = r#"
import * as types from "std/types"
import * as syntax from "std/syntax"
symbol Red
symbol Blue
node First = "a" { meanings { types::annotate(syntax::identity(), Red) } }
node Second = "b" { meanings { types::annotate(syntax::identity(), Red) } }
node Allowed = "c" {
 meanings {
  types::annotate(syntax::identity(), [Red, Blue])
  types::annotate(syntax::identity(), [Blue, Red])
 }
}
node Root = a: First b: Second allowed: Allowed {
 meanings {
  types::restrictTo([syntax::identityOf(a), syntax::identityOf(b)], types::typeof(syntax::identityOf(allowed)))
 }
}
entry Root
"#;
    let result = analyze(source, "abc");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "types.mismatch");
    assert!(result.diagnostics[0]
        .message
        .starts_with("Type [Red, Red] does not match"));
}

#[test]
fn unobserved_positional_query_keeps_joint_allowed_contract() {
    use your_language::semantics::{
        Evaluation, NativeOperation, OperationContext, OperationParameter, OperationSignature,
        ParameterKind,
    };
    struct Check;
    impl NativeOperation for Check {
        fn evaluate(
            &mut self,
            context: &mut OperationContext<'_>,
        ) -> Result<Evaluation, your_language::diagnostic::Diagnostic> {
            let value = context.argument("value").unwrap().clone();
            let Evaluation::Ready(SemanticValue::Alternatives { values, .. }) =
                context.resolve(&value)?
            else {
                return Ok(Evaluation::Pending);
            };
            if values.len() < 2 {
                return Ok(Evaluation::Pending);
            }
            let red = SemanticValue::Symbol("language.yl#Red".into());
            let blue = SemanticValue::Symbol("language.yl#Blue".into());
            Ok(Evaluation::Ready(SemanticValue::Bool(
                values.len() == 2
                    && values.contains(&SemanticValue::List(vec![red.clone(), blue.clone()]))
                    && values.contains(&SemanticValue::List(vec![blue, red])),
            )))
        }
    }
    let mut engine = SemanticEngine::default();
    engine
        .registry_mut()
        .register(
            "test/pairs",
            "correlated",
            OperationSignature {
                id: "test/pairs#correlated".into(),
                provides_context: false,
                finalization_only: false,
                contribution: false,
                parameters: vec![OperationParameter {
                    name: "value".into(),
                    kind: ParameterKind::Value,
                    default: None,
                }],
            },
            || Box::new(Check),
        )
        .unwrap();
    let source = r#"
import { correlated } from "test/pairs"
import * as types from "std/types"
import * as syntax from "std/syntax"
symbol Red
symbol Blue
symbol Good
node First = "a"
node Second = "b"
node Allowed = "c" {
 meanings {
  types::annotate(syntax::identity(), [Red, Blue])
  types::annotate(syntax::identity(), [Blue, Red])
 }
}
node Root = a: First b: Second allowed: Allowed {
 meanings {
  types::restrictTo([syntax::identityOf(a), syntax::identityOf(b)], types::typeof(syntax::identityOf(allowed)))
  match (correlated(types::typeof([syntax::identityOf(a), syntax::identityOf(b)]))) {
   true => types::annotate(syntax::identity(), Good),
   _ => error("Invented an unallowed joint possibility")
  }
 }
}
entry Root
"#;
    let language = compile(&engine, source);
    let result = engine.analyze(&language, "program", "abc");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        members(&result, "Root", None),
        vec![SemanticValue::Symbol("language.yl#Good".into())]
    );
}

#[test]
fn identical_restriction_contributions_are_idempotent_across_pairs() {
    let result = analyze(
        r#"
import * as types from "std/types"
import * as syntax from "std/syntax"
symbol Red
symbol Blue
node Root = "x" {
 meanings {
  types::annotate(syntax::identity(), Red)
  types::annotate(syntax::identity(), Blue)
  match (types::typeof(syntax::identity())) {
   _ => types::restrictTo(syntax::identity(), Red)
  }
 }
}
entry Root
"#,
        "x",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(
        result.diagnostics[0].message,
        "Type Blue does not match Red"
    );
}

#[test]
fn forwarded_structure_slot_cannot_grow_recursively_through_its_alias() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Leaf = "x" {{ meanings {{ types::annotate(syntax::identity(), Quartz) }} }}
node Root = leaf: Leaf {{ meanings {{
    types::annotate(syntax::identity(), types::typeof(syntax::identityOf(leaf)))
    types::annotate((syntax::identity(), slot), [types::typeof((syntax::identityOf(leaf), slot))])
}} }}
entry Root"#
        ),
        "x",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "types.mismatch");
    assert!(
        result.diagnostics[0]
            .message
            .to_lowercase()
            .contains("recursive"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn contradictory_unobserved_restrictions_do_not_become_a_query_union() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{
    types::restrictTo(syntax::identity(), Quartz)
    types::restrictTo(syntax::identity(), Orchid)
    match (types::typeof(syntax::identity())) {{
        Quartz => warn("incorrect Quartz fallback"),
        Orchid => warn("incorrect Orchid fallback"),
        _ => {{}},
    }}
}} }}
entry Root"#
        ),
        "x",
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "types.mismatch"),
        "{:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "semantic.meaning_case"),
        "Contradictory requirements became query alternatives: {:?}",
        result.diagnostics
    );
}

#[test]
fn structural_unobserved_restrictions_intersect_positionally() {
    let result = analyze(
        &format!(
            r#"{IMPORTS}
node Root = "x" {{ meanings {{
    types::restrictTo(syntax::identity(), [types::unknown, Quartz])
    types::restrictTo(syntax::identity(), [Orchid, types::unknown])
}} }}
entry Root"#
        ),
        "x",
    );
    clean(&result);
    assert_eq!(
        members(&result, "Root", None),
        [SemanticValue::List(vec![atom("Orchid"), atom("Quartz")])]
    );
}
