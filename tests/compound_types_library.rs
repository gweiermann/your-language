use std::collections::BTreeMap;
use your_language::semantics::{queries::QueryKey, AnalysisResult, SemanticEngine, SemanticValue};

fn evaluate(actions: &str) -> AnalysisResult {
    let source = format!(
        r#"
import {{ contribute, isPresent, equal }} from "core/query"
import {{ numberType, stringType, tupleType, listType, unionType, structType,
          parameterType, genericType, instantiateType, isIdenticalTo, isAssignableTo, typeOf, evidenceOf, unknownType }} from "./typing"
symbol first
symbol second
node Program = "x" {{ semantics {{ {actions} }} }}
entry Program
"#
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([
                ("language.yl".into(), source),
                (
                    "typing.yl".into(),
                    include_str!("../examples/compound-types/definition/typing.yl").into(),
                ),
            ]),
        )
        .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "program", "x");
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    result
}

fn boolean(result: &AnalysisResult, key: &str) -> bool {
    match result
        .graph
        .query_entries
        .iter()
        .find(|entry| entry.key == QueryKey::Text(key.into()))
        .map(|entry| &entry.value)
    {
        Some(SemanticValue::Bool(value)) => *value,
        value => panic!("Expected boolean result for {key}, got {value:?}"),
    }
}

#[test]
fn authored_comparison_policy_handles_structural_identity_invariant_lists_and_unions() {
    let result = evaluate(
        r#"
        small := structType([("name", stringType())])
        large := structType([("name", stringType()), ("age", numberType())])
        reversed := structType([("age", numberType()), ("name", stringType())])
        mixed := unionType([numberType(), stringType()])
        contribute("extraFieldsAssignable", isAssignableTo(large, small))
        contribute("extraFieldsIdentical", isIdenticalTo(large, small))
        contribute("fieldOrder", isIdenticalTo(large, reversed))
        contribute("tupleOrder", isIdenticalTo(tupleType([numberType(), stringType()]), tupleType([stringType(), numberType()])))
        contribute("listInvariant", isAssignableTo(listType(numberType()), listType(mixed)))
        contribute("unionOrder", isIdenticalTo(mixed, unionType([stringType(), numberType()])))
        contribute("unionDedupe", isIdenticalTo(unionType([numberType(), numberType()]), numberType()))
        contribute("unionMember", isAssignableTo(numberType(), mixed))
        contribute("unionAllMembers", isAssignableTo(mixed, numberType()))
    "#,
    );
    for (key, expected) in [
        ("extraFieldsAssignable", true),
        ("extraFieldsIdentical", false),
        ("fieldOrder", true),
        ("tupleOrder", false),
        ("listInvariant", false),
        ("unionOrder", true),
        ("unionDedupe", true),
        ("unionMember", true),
        ("unionAllMembers", false),
    ] {
        assert_eq!(boolean(&result, key), expected, "{key}");
    }
}

#[test]
fn generic_substitution_is_nested_explicit_and_keeps_template_parameters_independent() {
    let result = evaluate(
        r#"
        box := genericType([first], structType([("value", parameterType(first))]))
        envelope := genericType([second], structType([("box", instantiateType(box, [parameterType(second)]))]))
        numberEnvelope := instantiateType(envelope, [numberType()])
        stringEnvelope := instantiateType(envelope, [stringType()])
        contribute("numberInstance", isIdenticalTo(numberEnvelope, structType([("box", structType([("value", numberType())]))])))
        contribute("stringInstance", isIdenticalTo(stringEnvelope, structType([("box", structType([("value", stringType())]))])))
        contribute("differentInstances", isIdenticalTo(numberEnvelope, stringEnvelope))
        contribute("originalTemplate", isIdenticalTo(instantiateType(box, [parameterType(first)]), structType([("value", parameterType(first))])))
        contribute("wrongArity", isPresent(instantiateType(box, [])))
    "#,
    );
    for (key, expected) in [
        ("numberInstance", true),
        ("stringInstance", true),
        ("differentInstances", false),
        ("originalTemplate", true),
        ("wrongArity", false),
    ] {
        assert_eq!(boolean(&result, key), expected, "{key}");
    }
}

#[test]
fn absent_targets_end_queries_but_present_untyped_targets_remain_unknown() {
    let result = evaluate(
        r#"
        contribute("absentType", isPresent(typeOf(absent)))
        contribute("absentEvidence", isPresent(evidenceOf(absent)))
        contribute("presentUnknown", equal(typeOf(self), unknownType()))
    "#,
    );
    assert!(!boolean(&result, "absentType"));
    assert!(!boolean(&result, "absentEvidence"));
    assert!(boolean(&result, "presentUnknown"));
}
