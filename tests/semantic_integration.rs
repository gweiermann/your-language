use std::collections::BTreeMap;
use your_language::semantics::SemanticEngine;
#[path = "../examples/06-references/hosts/definition.rs"]
mod definition;
#[test]
fn authored_consumer_compiles_and_survives_reload_without_native_adapters() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let source = include_str!("../examples/06-references/programs/program.txt");
    let direct = engine.analyze(&language, "source", source);
    assert!(direct.diagnostics.is_empty(), "{:?}", direct.diagnostics);
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    assert_eq!(direct, engine.analyze(&loaded, "source", source));
    assert!(!direct.graph.query_entries.is_empty());
    assert!(direct.graph.records.is_empty());
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
fn authored_analyses_are_isolated_across_repeated_and_parallel_calls() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let first = engine.analyze(&language, "source", "let x = 1; let result = x;");
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    let missing = engine.analyze(&language, "source", "let result = x;");
    assert!(missing
        .diagnostics
        .iter()
        .any(|error| error.message == "Unknown name 'x'"));
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
fn authored_example_diagnostics_keep_exact_name_spans() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    for (source, message, name) in [
        ("let result = missing;", "Unknown name 'missing'", "missing"),
        ("let x = 1; let x = 2;", "Duplicate declaration 'x'", "x"),
    ] {
        let result = engine.analyze(&language, "input", source);
        let error = result
            .diagnostics
            .iter()
            .find(|error| error.message == message)
            .unwrap();
        assert_eq!(&source[error.primary.start..error.primary.end], name);
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
                finalization_only: false,
                contribution: false,
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
node First = "a" { semantics { record("first") } }
node Middle = "b" { semantics { record("middle") } }
node Last = "c" { semantics { record("last") } }
node Program = First Middle? Last { semantics { precedence { Last > Middle > First } } }
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
