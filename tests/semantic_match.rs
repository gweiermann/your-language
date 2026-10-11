use std::collections::BTreeMap;
use your_language::{
    diagnostic::Diagnostic,
    load_compiled_language,
    semantics::{
        Evaluation, NativeOperation, OperationContext, OperationParameter, OperationRegistry,
        OperationSignature, ParameterKind, SemanticEngine, SemanticValue,
    },
};
struct Echo;
impl NativeOperation for Echo {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        Ok(Evaluation::Ready(
            context.argument("value").unwrap().clone(),
        ))
    }
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let SemanticValue::Text(text) = context.argument("value").unwrap() else {
            panic!("typed value")
        };
        context.emit(Diagnostic::error(
            "test.effect",
            text.clone(),
            context.occurrence.span.clone(),
        ));
        Ok(())
    }
}
fn engine() -> SemanticEngine {
    let mut registry = OperationRegistry::new();
    registry
        .register(
            "native",
            "echo",
            OperationSignature {
                id: "native#echo".into(),
                finalization_only: false,
                provides_context: false,
                parameters: vec![OperationParameter {
                    name: "value".into(),
                    kind: ParameterKind::Text,
                    default: None,
                }],
            },
            || Box::new(Echo),
        )
        .unwrap();
    SemanticEngine::new(registry)
}
fn source(body: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(
        "main.yl".into(),
        format!(
            r#"import {{ echo }} from "native"
node Program = name: /[a-z]+/ {{ meanings {{ {body} }} }}
entry Program"#
        ),
    )])
}
#[test]
fn nested_values_and_tuple_alternatives_round_trip_and_execute_once() {
    let engine = engine();
    let language = engine
        .compile(
            "main.yl",
            &source(
                r#"
        echo(echo("nested"))
        match (echo("a"), echo("b")) {
            "a","a" | "a","b" => echo("selected")
            _ => error("unexpected ${name}")
        }
        match (echo("x")) { _ => {} }
    "#,
            ),
        )
        .unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    for _ in 0..2 {
        let result = engine.analyze(&language, "program", "hello");
        let messages: Vec<_> = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(messages, ["nested", "selected"]);
    }
}
#[test]
fn diagnostic_interpolation_targets_captured_source() {
    let engine = engine();
    let language = engine
        .compile(
            "main.yl",
            &source(r#"match (echo("x")) { _ => error("invalid ${name}") }"#),
        )
        .unwrap();
    let result = engine.analyze(&language, "program", "hello");
    assert_eq!(result.diagnostics.len(), 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.message, "invalid hello");
    assert_eq!(diagnostic.primary.file, "program");
    assert_eq!(diagnostic.primary.start, 0);
    assert_eq!(diagnostic.primary.end, 5);
    assert_eq!(diagnostic.secondary[0].file, "main.yl");
}
#[test]
fn comma_separated_match_cases_and_action_blocks_round_trip() {
    let engine = engine();
    let language = engine
        .compile(
            "main.yl",
            &source(
                r#"match (echo("a"), echo("b")) {
                "a", "b" => { echo("first") echo("second") },
                "b", _ | _, "a" => {},
                _ => error("unexpected"),
            }"#,
            ),
        )
        .unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&language, "program", "hello");
    let messages: Vec<_> = result
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(messages, ["first", "second"]);
}
#[test]
fn malformed_matches_and_nested_captures_are_rejected() {
    for (body, code) in [
        (
            r#"match (echo("a")) { "a" => {} }"#,
            "yl.non_exhaustive_meaning_match",
        ),
        (
            r#"match (echo("a"), echo("b")) { "a" => {} _ => {} }"#,
            "yl.invalid_meaning_match",
        ),
        (
            r#"match (echo("a")) { _ => {} "a" => {} }"#,
            "yl.invalid_meaning_match",
        ),
        (
            r#"match (echo("a")) { _ => error("${missing}") }"#,
            "yl.unknown_capture",
        ),
    ] {
        let errors = engine().compile("main.yl", &source(body)).unwrap_err();
        assert_eq!(errors[0].code, code, "{body}: {errors:?}");
    }
}
#[test]
fn reload_rejects_nonconstant_patterns() {
    let engine = engine();
    let language = engine
        .compile("main.yl", &source(r#"match (echo("x")) { _ => {} }"#))
        .unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    json["meanings"]["main.yl#Program"]["groups"][0]["matching"]["cases"][0]["alternatives"][0]
        [0] = serde_json::json!({"Capture": "name"});
    assert!(load_compiled_language(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn pending_values_wait_for_settlement_without_repeating_effects() {
    use std::any::Any;
    use your_language::semantics::AnalysisService;
    struct Service(bool);
    impl AnalysisService for Service {
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn settle(&mut self, _: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.0 = true;
            Ok(())
        }
    }
    struct Waiting;
    impl NativeOperation for Waiting {
        fn evaluate(
            &mut self,
            context: &mut OperationContext<'_>,
        ) -> Result<Evaluation, Diagnostic> {
            Ok(context
                .with_service::<Service, _>("test.waiting", |service, _| {
                    if service.0 {
                        Evaluation::Ready(SemanticValue::Text("ready".into()))
                    } else {
                        Evaluation::Pending
                    }
                })
                .unwrap())
        }
    }
    let mut registry = OperationRegistry::new();
    registry
        .register_service("test.waiting", || Box::new(Service(false)))
        .unwrap();
    registry
        .register(
            "native",
            "waiting",
            OperationSignature {
                id: "native#waiting".into(),
                parameters: vec![],
                finalization_only: false,
                provides_context: false,
            },
            || Box::new(Waiting),
        )
        .unwrap();
    registry
        .register(
            "native",
            "echo",
            OperationSignature {
                id: "native#echo".into(),
                finalization_only: false,
                provides_context: false,
                parameters: vec![OperationParameter {
                    name: "value".into(),
                    kind: ParameterKind::Text,
                    default: None,
                }],
            },
            || Box::new(Echo),
        )
        .unwrap();
    let engine = SemanticEngine::new(registry);
    let sources = BTreeMap::from([(
        "main.yl".into(),
        r#"
import { waiting, echo } from "native"
node Program = "x" { meanings {
    echo(waiting())
    match (waiting()) { "ready" => echo("once") _ => error("not ready") }
} }
entry Program
"#
        .into(),
    )]);
    let language = engine.compile("main.yl", &sources).unwrap();
    for _ in 0..2 {
        let result = engine.analyze(&language, "program", "x");
        let messages: Vec<_> = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(messages, ["ready", "once"]);
    }
}

#[test]
fn deferred_groups_preserve_explicit_definition_precedence() {
    let engine = engine();
    let sources = BTreeMap::from([(
        "main.yl".into(),
        r#"
import { echo } from "native"
node A = "a" { meanings { match (echo("ready")) { _ => echo("A") } } }
node B = "b" { meanings { match (echo("ready")) { _ => echo("B") } } }
node Program = B A { meanings { precedence { A > B } } }
entry Program
"#
        .into(),
    )]);
    let language = engine.compile("main.yl", &sources).unwrap();
    let result = engine.analyze(&language, "program", "ba");
    let messages: Vec<_> = result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert_eq!(messages, ["A", "B"]);
}

#[test]
fn deferred_before_ordinary_precedence_is_explicitly_rejected() {
    let engine = engine();
    let sources = BTreeMap::from([(
        "main.yl".into(),
        r#"
import { echo } from "native"
node A = "a" { meanings { match (echo("ready")) { _ => echo("A") } } }
node B = "b" { meanings { echo("B") } }
node Program = B A { meanings { precedence { A > B } } }
entry Program
"#
        .into(),
    )]);
    let language = engine.compile("main.yl", &sources).unwrap();
    let result = engine.analyze(&language, "program", "ba");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, "semantic.deferred_order");
    assert_eq!(result.diagnostics[0].primary.file, "main.yl");
}
