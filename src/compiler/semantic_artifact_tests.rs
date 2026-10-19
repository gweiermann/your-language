use super::*;
use crate::semantics::{MeaningCall, MeaningGroup, OperationParameter, OperationSignature};

fn artifact() -> CompiledLanguage {
    let sources = BTreeMap::from([(
        "test.yl".into(),
        "node Program = name: /[a-z]+/ entry Program".into(),
    )]);
    let mut language = compile_sources("test.yl", &sources).unwrap();
    language.version = 2;
    let span = Span::new("test.yl", 0, 1);
    language.meanings.insert(
        "test.yl#Program".into(),
        MeaningDefinition {
            boundary: false,
            groups: vec![MeaningGroup {
                matching: None,
                name: Some("Register".into()),
                span: span.clone(),
                calls: vec![MeaningCall {
                    operation: "native#register".into(),
                    contract: OperationSignature {
                        id: "native#register".into(),
                        finalization_only: false,
                        contribution: false,
                        provides_context: false,
                        parameters: vec![OperationParameter {
                            name: "name".into(),
                            kind: ParameterKind::Capture,
                            default: None,
                        }],
                    },
                    arguments: BTreeMap::from([(
                        "name".into(),
                        MeaningValue::Capture("name".into()),
                    )]),
                    span,
                }],
            }],
            precedence: vec![],
        },
    );
    language
}
fn wire(language: &CompiledLanguage) -> Vec<u8> {
    let mut image = language.executable_image().clone();
    image.semantics = language.meanings.clone();
    serde_json::to_vec(&image).unwrap()
}
#[test]
fn semantic_contracts_roundtrip_and_reject_mismatched_artifacts() {
    let mut language = artifact();
    crate::load_compiled_language(&wire(&language)).unwrap();
    language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0]
        .contract
        .id = "other".into();
    assert_eq!(
        crate::load_compiled_language(&wire(&language)).unwrap_err()[0].code,
        "yl.artifact"
    );
}
#[test]
fn artifacts_reject_retired_native_standard_operations_explicitly() {
    for id in [
        "std/variables#declare",
        "std/references#use",
        "std/types#annotate",
    ] {
        let mut language = artifact();
        let call = &mut language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0];
        call.operation = id.into();
        call.contract.id = id.into();
        let bytes = wire(&language);
        let error = crate::load_compiled_language(&bytes).unwrap_err();
        assert_eq!(error[0].code, "yl.artifact_retired_std");
        assert!(error[0].message.contains("recompile"));
        let analyzed =
            crate::semantics::SemanticEngine::default().analyze(&language, "input", "name");
        assert_eq!(analyzed.diagnostics[0].code, "yl.artifact_retired_std");
        assert!(analyzed.ast.is_none());
    }
}

#[test]
fn semantic_artifacts_reject_invalid_arguments_and_missing_captures() {
    let mut language = artifact();
    language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0]
        .arguments
        .insert("name".into(), MeaningValue::Capture("missing".into()));
    assert!(
        crate::load_compiled_language(&wire(&language)).unwrap_err()[0]
            .message
            .contains("unavailable capture")
    );
    language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0]
        .arguments
        .insert("name".into(), MeaningValue::Bool(true));
    assert!(crate::load_compiled_language(&wire(&language)).is_err());
}
#[test]
fn semantic_artifacts_reject_nested_missing_captures_and_illegal_lifecycles() {
    let mut language = artifact();
    let call = &mut language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0];
    call.contract.parameters[0].kind = ParameterKind::Value;
    call.arguments.insert(
        "name".into(),
        MeaningValue::Tuple(vec![MeaningValue::List(vec![MeaningValue::Capture(
            "missing".into(),
        )])]),
    );
    assert!(
        crate::load_compiled_language(&wire(&language)).unwrap_err()[0]
            .message
            .contains("unavailable capture")
    );
    let call = &mut language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0];
    call.arguments
        .insert("name".into(), MeaningValue::Symbol(String::new()));
    assert!(crate::load_compiled_language(&wire(&language)).is_err());
    let call = &mut language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0];
    call.arguments
        .insert("name".into(), MeaningValue::Capture("name".into()));
    call.contract.finalization_only = true;
    call.contract.provides_context = true;
    assert!(
        crate::load_compiled_language(&wire(&language)).unwrap_err()[0]
            .message
            .contains("Finalization-only")
    );
}
