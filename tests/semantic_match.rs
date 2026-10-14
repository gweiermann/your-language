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
                contribution: false,
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
                contribution: false,
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
                contribution: false,
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

// This service deliberately uses no std/types names. Query distribution belongs
// to the engine protocol, while fact storage and completion belong to libraries.
mod distributed {
    use super::*;
    use std::any::Any;
    use your_language::semantics::AnalysisService;

    #[derive(Default)]
    struct Facts {
        values: Vec<String>,
        expanded: bool,
        settled: bool,
    }
    impl AnalysisService for Facts {
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn offers_alternatives(&self) -> bool {
            true
        }
        fn resolve_value(
            &mut self,
            key: &SemanticValue,
            _: &mut OperationContext<'_>,
        ) -> Result<Evaluation, Diagnostic> {
            let SemanticValue::Text(side) = key else {
                panic!("test key")
            };
            let values = if side == "empty" && self.values.is_empty() {
                return Ok(Evaluation::Pending);
            } else if side == "left" {
                let mut values = vec![
                    SemanticValue::Text("a".into()),
                    SemanticValue::Text("a".into()),
                    SemanticValue::Text("b".into()),
                ];
                if self.settled {
                    values.push(SemanticValue::Text("unknown".into()));
                }
                values
            } else if side == "empty" {
                vec![SemanticValue::Text("ready".into())]
            } else {
                let mut values = vec![SemanticValue::Text("c".into())];
                if self.expanded {
                    values.push(SemanticValue::Text("d".into()));
                }
                values
            };
            Ok(Evaluation::Ready(SemanticValue::Alternatives {
                service: "custom.facts".into(),
                values,
            }))
        }
        fn progress(&mut self, _: &mut OperationContext<'_>) -> Result<bool, Diagnostic> {
            if !self.expanded && !self.values.is_empty() {
                self.expanded = true;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        fn settle(&mut self, _: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.settled = true;
            Ok(())
        }
        fn finish(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            context.attach(
                "custom#attempts",
                SemanticValue::List(
                    self.values
                        .iter()
                        .cloned()
                        .map(SemanticValue::Text)
                        .collect(),
                ),
            );
            Ok(())
        }
    }
    struct Read;
    impl NativeOperation for Read {
        fn evaluate(
            &mut self,
            context: &mut OperationContext<'_>,
        ) -> Result<Evaluation, Diagnostic> {
            Ok(Evaluation::Ready(SemanticValue::Deferred {
                service: "custom.facts".into(),
                key: Box::new(context.argument("value").unwrap().clone()),
                span: context.occurrence.span.clone(),
            }))
        }
    }
    struct RawList;
    impl NativeOperation for RawList {
        fn evaluate(&mut self, _: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
            Ok(Evaluation::Ready(SemanticValue::List(vec![
                SemanticValue::Text("a".into()),
                SemanticValue::Text("b".into()),
            ])))
        }
    }
    struct Collect;
    impl NativeOperation for Collect {
        fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            let SemanticValue::Text(value) = context.argument("value").unwrap() else {
                panic!("text")
            };
            let value = value.clone();
            context.with_service::<Facts, _>("custom.facts", |facts, _| facts.values.push(value));
            Ok(())
        }
    }
    struct Audit;
    impl NativeOperation for Audit {
        fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            let message = context
                .with_service::<Facts, _>("custom.facts", |facts, _| {
                    format!("audit {} {}", facts.values.len(), facts.settled)
                })
                .unwrap();
            context.emit(Diagnostic::error(
                "test.audit",
                message,
                context.occurrence.span.clone(),
            ));
            Ok(())
        }
    }
    fn engine() -> SemanticEngine {
        let mut registry = OperationRegistry::new();
        registry
            .register_service("custom.facts", || Box::<Facts>::default())
            .unwrap();
        for (name, contribution, parameters, factory) in [
            (
                "read",
                false,
                true,
                (|| Box::new(Read) as Box<dyn NativeOperation>) as fn() -> Box<dyn NativeOperation>,
            ),
            ("collect", true, true, || {
                Box::new(Collect) as Box<dyn NativeOperation>
            }),
            ("echo", false, true, || {
                Box::new(Echo) as Box<dyn NativeOperation>
            }),
            ("raw", false, false, || {
                Box::new(RawList) as Box<dyn NativeOperation>
            }),
            ("audit", false, false, || {
                Box::new(Audit) as Box<dyn NativeOperation>
            }),
        ] {
            registry
                .register(
                    "native",
                    name,
                    OperationSignature {
                        id: format!("native#{name}"),
                        provides_context: false,
                        finalization_only: name == "collect",
                        contribution,
                        parameters: if parameters {
                            vec![OperationParameter {
                                name: "value".into(),
                                kind: ParameterKind::Value,
                                default: None,
                            }]
                        } else {
                            vec![]
                        },
                    },
                    factory,
                )
                .unwrap();
        }
        SemanticEngine::new(registry)
    }
    fn analyze(body: &str) -> your_language::semantics::AnalysisResult {
        let engine = engine();
        let sources = BTreeMap::from([(
            "main.yl".into(),
            format!(
                r#"
import {{ read, collect, echo, raw, audit }} from "native"
node Program = name: "x" {{ meanings {{ {body} }} }}
entry Program
"#
            ),
        )]);
        let language = engine.compile("main.yl", &sources).unwrap();
        let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
        engine.analyze(&language, "program", "x")
    }
    fn attempts(result: &your_language::semantics::AnalysisResult) -> Vec<String> {
        let value = &result
            .graph
            .attachments
            .iter()
            .find(|value| value.key == "custom#attempts")
            .unwrap()
            .value;
        let SemanticValue::List(values) = value else {
            panic!("list")
        };
        values
            .iter()
            .map(|value| {
                let SemanticValue::Text(value) = value else {
                    panic!("text")
                };
                value.clone()
            })
            .collect()
    }
    #[test]
    fn late_cartesian_members_and_unknown_do_not_repeat_prior_tuples() {
        let result = analyze(
            r#"
match (read("left"), read("right")) {
    "a", "c" => collect("ac"),
    "a", "d" => collect("ad"),
    "b", _ => { collect("b") warn("warning for this pair ${name}") },
    "unknown", _ => collect("unknown"),
    _ => error("unexpected"),
}
"#,
        );
        assert_eq!(
            attempts(&result),
            ["ac", "b", "ad", "b", "unknown", "unknown"]
        );
        // (b,c) and later (b,d) each warn once; repeated scans and duplicate
        // input members must not create further copies of either warning.
        assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
        assert!(result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message == "warning for this pair x"));
    }
    #[test]
    fn two_unsupported_pairs_each_emit_once_without_canceling_supported_contributions() {
        let result = analyze(
            r#"
match (read("left"), read("right")) {
    "a", _ => collect("supported"),
    "unknown", _ => {},
    _ => error("unsupported ${name}"),
}
"#,
        );
        assert_eq!(attempts(&result), ["supported", "supported"]);
        // (b,c) and (b,d) are distinct unsupported pairs selecting the same
        // wildcard case. Its error aborts each pair, never the supported pairs.
        assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
        assert!(result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message == "unsupported x"));
    }
    #[test]
    fn reactive_pending_reader_releases_later_collector() {
        let engine = engine();
        let sources = BTreeMap::from([(
            "main.yl".into(),
            r#"
import { read, collect } from "native"
node A = "x" { meanings { match (read("empty")) { "ready" => collect("observed"), _ => {} } } }
node B = "y" { meanings { collect("seed") } }
node Program = A B { meanings { precedence { A > B } } }
entry Program
"#
            .into(),
        )]);
        let language = engine.compile("main.yl", &sources).unwrap();
        let result = engine.analyze(&language, "program", "xy");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(attempts(&result), ["seed", "observed"]);
    }
    #[test]
    fn raw_lists_are_positional_values_not_alternatives() {
        let result = analyze(r#"match (raw()) { _ => echo("one") }"#);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].message, "one");
    }
    #[test]
    fn ordinary_successor_runs_after_reactive_fixed_point_and_settlement() {
        let engine = engine();
        let sources = BTreeMap::from([(
            "main.yl".into(),
            r#"
import { read, collect, echo, audit } from "native"
node A = "x" { meanings { match (read("left"), read("right")) { _ => collect("pair") } } }
node B = "y" { meanings { match (echo("scalar")) { _ => audit() } } }
node Program = A B { meanings { precedence { A > B } } }
entry Program
"#
            .into(),
        )]);
        let language = engine.compile("main.yl", &sources).unwrap();
        let result = engine.analyze(&language, "program", "xy");
        assert_eq!(attempts(&result).len(), 6);
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        assert_eq!(result.diagnostics[0].message, "audit 6 true");
    }
    #[test]
    fn arbitrary_effect_and_alternative_reread_are_parked_before_execution() {
        for (body, code) in [
            (
                r#"match (read("left")) { _ => echo("never") }"#,
                "semantic.distributed_effect",
            ),
            (
                r#"match (read("left")) { _ => collect(read("left")) }"#,
                "semantic.distributed_argument",
            ),
        ] {
            let result = analyze(body);
            assert!(attempts(&result).is_empty());
            assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
            assert_eq!(result.diagnostics[0].code, code);
        }
    }
}

#[test]
fn contributions_cannot_supply_contexts_in_registry_or_reload() {
    let mut registry = OperationRegistry::new();
    assert!(registry
        .register(
            "native",
            "context",
            OperationSignature {
                id: "native#context".into(),
                parameters: vec![],
                provides_context: true,
                finalization_only: false,
                contribution: true,
            },
            || Box::new(Echo)
        )
        .is_err());
    let language = engine()
        .compile("main.yl", &source(r#"echo("value")"#))
        .unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    let contract = &mut json["meanings"]["main.yl#Program"]["groups"][0]["calls"][0]["contract"];
    contract["contribution"] = true.into();
    contract["provides_context"] = true.into();
    assert!(load_compiled_language(&serde_json::to_vec(&json).unwrap()).is_err());
}
