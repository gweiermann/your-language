use std::{any::Any, collections::BTreeMap};
use your_language::{
    diagnostic::Diagnostic,
    semantics::{
        AnalysisService, NativeOperation, OperationContext, OperationRegistry, OperationSignature,
        SemanticEngine, SemanticValue,
    },
};
#[derive(Default)]
struct AuditService {
    collected: usize,
    progressed: usize,
    settles: usize,
}
impl AnalysisService for AuditService {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn progress(&mut self, _ctx: &mut OperationContext<'_>) -> Result<bool, Diagnostic> {
        if self.progressed == self.collected {
            return Ok(false);
        }
        self.progressed = self.collected;
        Ok(true)
    }
    fn settle(&mut self, _ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.settles += 1;
        Ok(())
    }
    fn finish(&mut self, ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        assert_eq!(self.settles, 1);
        assert_eq!(self.collected, self.progressed);
        ctx.attach(
            "test/service#count",
            SemanticValue::Text(self.collected.to_string()),
        );
        Ok(())
    }
}
struct Collect;
impl NativeOperation for Collect {
    fn before(&mut self, ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        ctx.with_service::<AuditService, _>("test/service", |state, _ctx| state.collected += 1)
            .unwrap();
        Ok(())
    }
}
fn engine() -> SemanticEngine {
    let mut registry = OperationRegistry::new();
    registry
        .register_service("test/service", || Box::<AuditService>::default())
        .unwrap();
    assert!(registry
        .register_service("test/service", || Box::<AuditService>::default())
        .is_err());
    registry
        .register(
            "test/service",
            "collect",
            OperationSignature {
                id: "test/service#collect".into(),
                parameters: vec![],
                provides_context: false,
            },
            || Box::new(Collect),
        )
        .unwrap();
    SemanticEngine::new(registry)
}
#[test]
fn analysis_services_are_automatic_isolated_and_skip_syntax_errors() {
    let engine = engine();
    let source = r#"import { collect } from "test/service" node Item = "a" { meanings { collect() } } node Root = Item* entry Root"#;
    let language = engine
        .compile(
            "service.yl",
            &BTreeMap::from([("service.yl".into(), source.into())]),
        )
        .unwrap();
    for (input, count) in [("aa", "2"), ("a", "1"), ("", "0")] {
        let result = engine.analyze(&language, "input", input);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert!(result
            .graph
            .attachments
            .iter()
            .any(|attachment| attachment.key == "test/service#count"
                && attachment.value == SemanticValue::Text(count.into())));
    }
    let error = engine.analyze(&language, "input", "b");
    assert!(error.graph.attachments.is_empty());
    std::thread::scope(|scope| {
        let a = scope.spawn(|| engine.analyze(&language, "input", "aa"));
        let b = scope.spawn(|| engine.analyze(&language, "input", "aa"));
        assert_eq!(a.join().unwrap(), b.join().unwrap());
    });
}
#[test]
fn failed_service_is_not_polled_again_and_still_finishes_once() {
    #[derive(Default)]
    struct Failing {
        polls: usize,
        finishes: usize,
    }
    impl AnalysisService for Failing {
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn progress(&mut self, ctx: &mut OperationContext<'_>) -> Result<bool, Diagnostic> {
            self.polls += 1;
            Err(Diagnostic::error(
                "test.failure",
                "Service failed",
                ctx.occurrence.span.clone(),
            ))
        }
        fn settle(&mut self, _ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            panic!("Failed service must not settle")
        }
        fn finish(&mut self, ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.finishes += 1;
            assert_eq!(self.polls, 1);
            assert_eq!(self.finishes, 1);
            ctx.attach("test/failure#finished", SemanticValue::Bool(true));
            Ok(())
        }
    }
    let mut engine = engine();
    engine
        .registry_mut()
        .register_service("test/failure", || Box::<Failing>::default())
        .unwrap();
    let source = r#"import { collect } from "test/service" node Root = "a" { meanings { collect() } } entry Root"#;
    let result = engine
        .compile_and_analyze(
            "service.yl",
            &BTreeMap::from([("service.yl".into(), source.into())]),
            "input",
            "a",
        )
        .unwrap();
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "test.failure");
    assert!(result
        .graph
        .attachments
        .iter()
        .any(|attachment| attachment.key == "test/failure#finished"));
}
