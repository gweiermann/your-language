use std::collections::BTreeMap;
use your_language::{
    diagnostic::Diagnostic,
    semantics::{
        NativeOperation, OperationContext, OperationParameter, OperationSignature, ParameterKind,
        SemanticEngine, SemanticIdentity, SemanticValue,
    },
};

struct Inspect;
impl NativeOperation for Inspect {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        context.attach(
            "test/bindings#value",
            context.argument("value").unwrap().clone(),
        );
        Ok(())
    }
}
fn engine() -> SemanticEngine {
    let mut engine = SemanticEngine::default();
    engine
        .registry_mut()
        .register(
            "test/bindings",
            "inspect",
            OperationSignature {
                id: "test/bindings#inspect".into(),
                provides_context: false,
                finalization_only: false,
                parameters: vec![OperationParameter {
                    name: "value".into(),
                    kind: ParameterKind::Value,
                    default: None,
                }],
            },
            || Box::new(Inspect),
        )
        .unwrap();
    engine
}
#[test]
fn binding_contracts_are_unambiguous_and_removed_categories_are_unavailable() {
    let engine = engine();
    for bad in [
        "v::identity(name)",
        "v::identityOf()",
        "v::declareVariable(name)",
        "v::declareFunction(name)",
        "v::useVariable(name)",
        "v::declare(v::variable,name)",
    ] {
        let sources = BTreeMap::from([(
            "language.yl".into(),
            format!(
                r#"import * as v from "std/variables"
node Root = name: /[a-z]+/ {{ meanings {{ {bad} }} }} entry Root"#
            ),
        )]);
        assert!(engine.compile("language.yl", &sources).is_err(), "{bad}");
    }
    for removed in ["variable", "function", "kindSymbol"] {
        let sources = BTreeMap::from([(
            "language.yl".into(),
            format!(r#"import {{ {removed} }} from "std/variables" node Root = "x" entry Root"#),
        )]);
        assert!(
            engine.compile("language.yl", &sources).is_err(),
            "{removed}"
        );
    }
}
#[test]
fn explicit_identity_of_preserves_absence_and_binding_identity_through_reload() {
    let sources = BTreeMap::from([("language.yl".into(), r#"
import * as v from "std/variables"
import { inspect } from "test/bindings"
node Declaration = "let " name: /[a-z]+/ ";" { meanings { v::declare(name, reassignable=false) inspect(v::identity()) } }
node Reference = name: /[a-z]+/ ";" { meanings { v::use(name) inspect(v::identity()) } }
node Root = declaration: Declaration reference: Reference {
 meanings {
  v::lexicalScope()
  inspect((v::identity(), v::identityOf(declaration), v::identityOf(reference), v::identityOf(absent)))
 }
}
entry Root
"#.into())]);
    let engine = engine();
    let language = engine.compile("language.yl", &sources).unwrap();
    let result = engine.analyze(&language, "input", "let x;x;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result,
        engine.analyze(
            &engine.load(&language.to_bytes().unwrap()).unwrap(),
            "input",
            "let x;x;"
        )
    );
    let values: Vec<_> = result
        .graph
        .attachments
        .iter()
        .filter(|a| a.key == "test/bindings#value")
        .map(|a| a.value.clone())
        .collect();
    assert_eq!(values[0], values[1]);
    assert!(matches!(
        values[0],
        SemanticValue::Identity(SemanticIdentity::Binding(_))
    ));
    let SemanticValue::Tuple(tuple) = &values[2] else {
        panic!("{values:?}")
    };
    assert!(matches!(
        tuple[0],
        SemanticValue::Identity(SemanticIdentity::Occurrence(_))
    ));
    assert_eq!(tuple[1], values[0]);
    assert_eq!(tuple[2], values[1]);
    assert_eq!(tuple[3], SemanticValue::Absent);
    assert!(!serde_json::to_string(&result.graph)
        .unwrap()
        .contains("kindSymbol"));
    assert!(serde_json::to_string(&result.graph)
        .unwrap()
        .contains("reassignableSymbol"));
}
