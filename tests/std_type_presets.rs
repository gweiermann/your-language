use std::{collections::BTreeMap, path::PathBuf};
use your_language::{
    compile_language, load_compiled_language,
    semantics::{queries::QueryKey, AnalysisResult, SemanticEngine, SemanticValue},
};

fn structural(body: &str) -> AnalysisResult {
    let source = format!(
        r#"import * as types from "std/types/structural"
import {{ contribute }} from "core/query"
node Root = "x" {{ semantics {{ {body} }} }}
entry Root"#
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source)]),
        )
        .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "input", "x");
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result, engine.analyze(&language, "input", "x"));
    result
}

fn observed(result: &AnalysisResult) -> &SemanticValue {
    &result
        .graph
        .query_entries
        .iter()
        .find(|entry| entry.key == QueryKey::Text("observed".into()))
        .expect("Expected the authored operation to publish a result")
        .value
}

#[test]
fn structural_assignment_and_identity_have_explicit_distinct_policies() {
    let result = structural(
        r#"actual := types::structType([("name", types::stringType()), ("age", types::numberType())])
expected := types::structType([("name", types::stringType())])
contribute("observed", [types::isAssignableTo(actual, expected), types::isIdenticalTo(actual, expected), types::isIdenticalTo(actual, types::structType([("age", types::numberType()), ("name", types::stringType())]))])"#,
    );
    assert_eq!(
        observed(&result),
        &SemanticValue::List(vec![
            SemanticValue::Bool(true),
            SemanticValue::Bool(false),
            SemanticValue::Bool(true)
        ])
    );
}

#[test]
fn nested_generic_substitution_preserves_shared_parameter_identity() {
    let result = structural(
        r#"generic := types::genericType([self], types::structType([("items", types::listType(types::parameterType(self))), ("pair", types::tupleType([types::parameterType(self), types::stringType()]))]))
instantiated := types::instantiateType(generic, [types::numberType()])
expected := types::structType([("items", types::listType(types::numberType())), ("pair", types::tupleType([types::numberType(), types::stringType()]))])
contribute("observed", types::isIdenticalTo(instantiated, expected))"#,
    );
    assert_eq!(observed(&result), &SemanticValue::Bool(true));
}

#[test]
fn mutable_lists_are_invariant_even_when_element_assignment_would_succeed() {
    let result = structural(
        r#"numeric := types::listType(types::numberType())
mixed := types::listType(types::unionType([types::numberType(), types::stringType()]))
contribute("observed", [types::isAssignableTo(numeric, mixed), types::isAssignableTo(mixed, numeric), types::isAssignableTo(numeric, numeric)])"#,
    );
    assert_eq!(
        observed(&result),
        &SemanticValue::List(vec![
            SemanticValue::Bool(false),
            SemanticValue::Bool(false),
            SemanticValue::Bool(true)
        ])
    );
}

#[test]
fn arithmetic_accepts_caller_owned_type_symbols_and_reports_unsupported_operands() {
    let source = r#"import { checkAddition } from "std/types/arithmetic"
import { annotateWithType, typeOf, unknown } from "std/types"
import { contribute } from "core/query"
enum Mine { integer text boolean }
node Root = "x" {
    semantics {
        annotateWithType("left", Mine::integer)
        annotateWithType("right", Mine::text)
        checkAddition("sum", "left", "right", Mine::integer, Mine::text)
        checkAddition("pending", "untyped", "right", Mine::integer, Mine::text)
        annotateWithType("bad", Mine::boolean)
        checkAddition(self, "bad", "right", Mine::integer, Mine::text)
        contribute("observed", [typeOf("sum"), typeOf("pending")])
    }
}
entry Root"#;
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source.into())]),
        )
        .unwrap();
    let language = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&language, "input", "x");
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].message, "bad + right is not possible");
    assert_eq!(
        observed(&result),
        &SemanticValue::List(vec![
            SemanticValue::Symbol("language.yl#Mine::text".into()),
            SemanticValue::Symbol("std/types#unknown".into())
        ])
    );
}

#[test]
fn diagnostic_origins_reuses_preset_without_losing_precise_negative_spans() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/12-diagnostic-origins");
    let language = compile_language(root.join("definition/language.yl")).unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let engine = SemanticEngine::default();
    for entry in std::fs::read_dir(root.join("programs")).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("invalid-")
        {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let result = engine.analyze(&language, &path.to_string_lossy(), &source);
        assert!(
            !result.diagnostics.is_empty(),
            "{} unexpectedly passed",
            path.display()
        );
        for diagnostic in &result.diagnostics {
            assert!(diagnostic.primary.start < diagnostic.primary.end);
            assert!(diagnostic.primary.end <= source.len());
        }
    }
}

#[test]
fn declarative_functions_reuses_addition_policy_without_changing_local_type_storage() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/10-declarative-functions");
    let language = compile_language(root.join("definition/language.yl")).unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let engine = SemanticEngine::default();
    let source = std::fs::read_to_string(root.join("programs/addition.txt")).unwrap();
    let result = engine.analyze(&language, "addition.txt", &source);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    fn additions(items: &[your_language::semantics::Occurrence], ids: &mut Vec<usize>) {
        for item in items {
            if item.definition.ends_with("::Addition") {
                ids.push(item.id);
            }
            additions(&item.children, ids);
        }
    }
    let mut ids = vec![];
    additions(&result.occurrences, &mut ids);
    assert_eq!(ids.len(), 3);
    let inferred = ids.iter().map(|id| {
        &result.graph.query_entries.iter().find(|entry| {
            matches!(&entry.key, QueryKey::Tuple(parts) if matches!(parts.as_slice(), [QueryKey::Identity(your_language::semantics::SemanticIdentity::Occurrence(found)), QueryKey::Symbol(slot)] if found == id && slot.ends_with("#type")))
        }).expect("Addition must annotate its own syntax").value
    }).collect::<Vec<_>>();
    assert_eq!(
        inferred,
        vec![
            &SemanticValue::Symbol("types.yl#number".into()),
            &SemanticValue::Symbol("types.yl#string".into()),
            &SemanticValue::Symbol("types.yl#string".into())
        ]
    );
    let source = std::fs::read_to_string(root.join("programs/invalid-addition.txt")).unwrap();
    let result = engine.analyze(&language, "invalid-addition.txt", &source);
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    let span = &result.diagnostics[0].primary;
    assert_eq!(&source[span.start..span.end], "identity + 1");
}

#[test]
fn arithmetic_distributes_union_candidates_before_comparing_caller_types() {
    let source = r#"import { checkAddition } from "std/types/arithmetic"
import { annotateWithType, candidates } from "std/types"
import { contribute } from "core/query"
enum Mine { integer text boolean }
node Root = "x" {
    semantics {
        annotateWithType("left", Mine::integer)
        annotateWithType("left", Mine::text)
        annotateWithType("right", Mine::integer)
        checkAddition("sum", "left", "right", Mine::integer, Mine::text)
        annotateWithType("mixed", Mine::integer)
        annotateWithType("mixed", Mine::boolean)
        checkAddition("partiallyInvalid", "mixed", "right", Mine::integer, Mine::text)
        contribute("observed", [candidates("sum"), candidates("partiallyInvalid")])
    }
}
entry Root"#;
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source.into())]),
        )
        .unwrap();
    let language = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&language, "input", "x");
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    let SemanticValue::List(results) = observed(&result) else {
        panic!("Expected result lists")
    };
    let SemanticValue::List(sum) = &results[0] else {
        panic!("Expected sum candidates")
    };
    assert_eq!(sum.len(), 2);
    assert!(sum.contains(&SemanticValue::Symbol("language.yl#Mine::integer".into())));
    assert!(sum.contains(&SemanticValue::Symbol("language.yl#Mine::text".into())));
    assert_eq!(
        results[1],
        SemanticValue::List(vec![SemanticValue::Symbol(
            "language.yl#Mine::integer".into()
        )])
    );
}

#[test]
fn generic_function_recomputes_addition_for_each_hypothetical_call() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/10-declarative-functions");
    let language = compile_language(root.join("definition/language.yl")).unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let source = r#"increment(41); increment("answer: ");
function increment(value) { return value + 1; }"#;
    let result = SemanticEngine::default().analyze(&language, "generic-addition.txt", source);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    fn calls(items: &[your_language::semantics::Occurrence], ids: &mut Vec<usize>) {
        for item in items {
            if item.definition.ends_with("::Call") {
                ids.push(item.id);
            }
            calls(&item.children, ids);
        }
    }
    let mut ids = vec![];
    calls(&result.occurrences, &mut ids);
    assert_eq!(ids.len(), 2);
    for (id, expected) in ids.iter().zip(["number", "string"]) {
        let entry = result.graph.query_entries.iter().find(|entry| {
            matches!(&entry.key, QueryKey::Tuple(parts) if matches!(parts.as_slice(), [QueryKey::Identity(your_language::semantics::SemanticIdentity::Occurrence(found)), QueryKey::Symbol(slot)] if found == id && slot.ends_with("#type")))
        }).expect("Call should have a recomputed return type");
        assert_eq!(
            entry.value,
            SemanticValue::Symbol(format!("types.yl#{expected}"))
        );
    }
}
