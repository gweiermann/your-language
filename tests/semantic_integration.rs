use std::collections::BTreeMap;
use your_language::{diagnostic::Severity, semantics::SemanticEngine};

#[path = "../examples/semantics/definition.rs"]
mod definition;

#[test]
fn native_consumer_compiles_and_analyzes_in_memory_and_survives_optional_reload() {
    let engine = SemanticEngine::default();
    let source = include_str!("../examples/semantics/programs/program.txt");
    let direct = engine
        .compile_and_analyze(
            definition::ENTRY,
            &definition::sources(),
            "programs/program.txt",
            source,
        )
        .unwrap();
    assert!(direct.diagnostics.is_empty(), "{:?}", direct.diagnostics);
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    assert_eq!(
        direct,
        engine.analyze(&loaded, "programs/program.txt", source)
    );
    let empty = SemanticEngine::new(Default::default());
    assert_eq!(
        empty.load(&language.to_bytes().unwrap()).unwrap_err()[0].code,
        "semantic.missing_operation"
    );
    let graph = serde_json::to_string_pretty(&direct.graph).unwrap() + "\n";
    let normalized = String::from_utf8(language.to_bytes().unwrap()).unwrap() + "\n";
    if std::env::var_os("UPDATE_SEMANTIC_GOLDENS").is_some() {
        std::fs::write("tests/golden/semantic.graph.json", &graph).unwrap();
        std::fs::write("tests/golden/semantic.normalized.json", &normalized).unwrap();
    } else {
        assert_eq!(
            graph,
            std::fs::read_to_string("tests/golden/semantic.graph.json").unwrap()
        );
        assert_eq!(
            normalized,
            std::fs::read_to_string("tests/golden/semantic.normalized.json").unwrap()
        );
    }
}

#[test]
fn native_analyses_are_isolated_across_repeated_and_parallel_calls() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let first = engine.analyze(&language, "source", "let x = 1; let result = x;");
    assert!(first.diagnostics.is_empty());
    let second = engine.analyze(&language, "source", "let result = x;");
    assert_eq!(second.diagnostics[0].code, "semantic.unresolved_reference");
    std::thread::scope(|scope| {
        let calls: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| engine.analyze(&language, "source", "let x = 1; let result = x;"))
            })
            .collect();
        for call in calls {
            assert_eq!(call.join().unwrap(), first);
        }
    });
}

#[test]
fn semantic_examples_have_specific_source_diagnostics_and_declaration_origins() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    for (file, source, code) in [
        (
            "programs/unresolved.txt",
            include_str!("../examples/semantics/programs/unresolved.txt"),
            "semantic.unresolved_reference",
        ),
        (
            "programs/duplicate.txt",
            include_str!("../examples/semantics/programs/duplicate.txt"),
            "semantic.duplicate_declaration",
        ),
    ] {
        let result = engine.analyze(&language, file, source);
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == code)
            .unwrap();
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.primary.file, file);
        assert!(!source[diagnostic.primary.start..diagnostic.primary.end].is_empty());
        if code == "semantic.duplicate_declaration" {
            assert_eq!(diagnostic.secondary.len(), 1);
        }
    }
}

#[test]
fn precedence_remains_transitive_when_optional_middle_level_is_absent() {
    use your_language::{
        diagnostic::Diagnostic,
        semantics::{
            NativeOperation, OperationContext, OperationParameter, OperationRegistry,
            OperationSignature, ParameterKind, SemanticValue,
        },
    };
    struct Record;
    impl NativeOperation for Record {
        fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            let value = context.argument("value").unwrap().clone();
            context.attach("test/order", value);
            Ok(())
        }
    }
    let mut registry = OperationRegistry::new();
    registry
        .register(
            "test/order",
            "record",
            OperationSignature {
                id: "test/order#record".into(),
                provides_context: false,
                parameters: vec![OperationParameter {
                    name: "value".into(),
                    kind: ParameterKind::Text,
                    default: None,
                }],
            },
            || Box::new(Record),
        )
        .unwrap();
    let engine = SemanticEngine::new(registry);
    let source = r#"import { record } from "test/order"
node First = "a" { meanings { record("first") } }
node Middle = "b" { meanings { record("middle") } }
node Last = "c" { meanings { record("last") } }
node Program = First Middle? Last { meanings { precedence { Last > Middle > First } } }
entry Program"#;
    let result = engine
        .compile_and_analyze(
            "order.yl",
            &BTreeMap::from([("order.yl".into(), source.into())]),
            "program",
            "ac",
        )
        .unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .graph
            .attachments
            .iter()
            .map(|attachment| attachment.value.clone())
            .collect::<Vec<_>>(),
        vec![
            SemanticValue::Text("last".into()),
            SemanticValue::Text("first".into())
        ]
    );
}
