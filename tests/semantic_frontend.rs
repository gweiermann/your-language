use std::collections::BTreeMap;
use your_language::{
    frontend::parse_yl, load_compiled_language, semantics::SemanticEngine, syntax::DeclKind,
};

#[path = "../examples/semantics/definition.rs"]
mod definition;

fn sources(source: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("language.yl".into(), source.into())])
}

#[test]
fn meanings_compile_through_real_source_imports_and_pattern_boundaries() {
    let source = r#"
import { lexicalScope, declare, use } from "std/variables"
node Name = /[a-z]+/
node Declaration = "let" name: Name ";" {
    meanings { group Register { declare(name, reassignable=false) } }
}
node Reference = name: Name ";" { meanings { use(name) } }
pattern Contents = (Declaration | Reference)* { meanings { lexicalScope() } }
node Program = Contents
entry Program
"#;
    let ast = parse_yl("language.yl", source).unwrap();
    assert!(ast.declarations.iter().any(|declaration| matches!(
        &declaration.kind,
        DeclKind::Pattern {
            meanings: Some(_),
            ..
        }
    )));
    let engine = SemanticEngine::default();
    let language = engine.compile("language.yl", &sources(source)).unwrap();
    let bytes = language.to_bytes().unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["version"],
        4
    );
    let language = load_compiled_language(&bytes).unwrap();
    let result = engine.analyze(&language, "program", "letx;x;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.ast.is_some());
    assert!(!result.graph.query_entries.is_empty());
}

#[test]
fn source_meanings_validate_calls_captures_groups_and_selectors() {
    let engine = SemanticEngine::default();
    let cases = [
        ("use(missing)", "yl.artifact"),
        ("use()", "yl.invalid_argument"),
        ("use(name, name=name)", "yl.invalid_argument"),
        (
            "group Register {} group Register {}",
            "yl.duplicate_meaning_group",
        ),
        (
            "precedence { Program::meanings::Missing > Program }",
            "yl.unknown_meaning_group",
        ),
        ("precedence { Program > _ }", "yl.invalid_meaning_selector"),
    ];
    for (body, expected) in cases {
        let source = format!("import {{ use, declare }} from \"std/variables\" node Program = name: /[a-z]+/ {{ meanings {{ {body} }} }} entry Program");
        let errors = engine
            .compile("language.yl", &sources(&source))
            .unwrap_err();
        assert_eq!(errors[0].code, expected, "{body}: {errors:?}");
    }
}

#[test]
fn source_precedence_uses_resolved_definition_and_named_group_identity() {
    let engine = SemanticEngine::default();
    let source = r#"
import { lexicalScope, declare, use } from "std/variables"
node Declaration = "let" name: /[a-z]+/ ";" { meanings { group Register { declare(name) } } }
node Reference = name: /[a-z]+/ ";" { meanings { use(name) } }
node Program = Reference Declaration {
    meanings {
        lexicalScope()
        precedence { Declaration::meanings::Register > Reference }
    }
}
entry Program
"#;
    let language = engine.compile("language.yl", &sources(source)).unwrap();
    let result = engine.analyze(&language, "program", "x;letx;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn mixed_named_and_ungrouped_operations_preserve_source_order() {
    use your_language::{
        diagnostic::{Diagnostic, Span},
        semantics::{
            NativeOperation, OperationContext, OperationParameter, OperationRegistry,
            OperationSignature, ParameterKind, SemanticValue,
        },
    };
    struct Record;
    impl NativeOperation for Record {
        fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            let Some(SemanticValue::Text(value)) = context.argument("value").cloned() else {
                panic!("Expected typed text argument")
            };
            context.emit(Diagnostic::error("test.record", value, Span::default()));
            Ok(())
        }
    }
    let mut registry = OperationRegistry::new();
    registry
        .register(
            "test",
            "record",
            OperationSignature {
                id: "test#record".into(),
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
    let source = r#"import { record } from "test"
node Program = "x" { meanings {
    record("a")
    group G { record("b") }
    record("c")
} }
entry Program"#;
    let language = engine.compile("language.yl", &sources(source)).unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&language, "program", "x");
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[test]
fn checked_in_semantic_language_resolves_functions_and_reports_negative_inputs() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let valid = engine.analyze(
        &language,
        "valid.txt",
        include_str!("../examples/semantics/programs/program.txt"),
    );
    assert!(valid.diagnostics.is_empty(), "{:?}", valid.diagnostics);
    assert!(valid.ast.is_some());
    let unresolved = engine.analyze(
        &language,
        "unresolved.txt",
        include_str!("../examples/semantics/programs/unresolved.txt"),
    );
    assert_eq!(
        unresolved.diagnostics.len(),
        1,
        "{:?}",
        unresolved.diagnostics
    );
    assert_eq!(unresolved.diagnostics[0].code, "query.diagnostic");
    assert_eq!(unresolved.diagnostics[0].primary.file, "unresolved.txt");
    let duplicate = engine.analyze(
        &language,
        "duplicate.txt",
        include_str!("../examples/semantics/programs/duplicate.txt"),
    );
    assert!(duplicate
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.starts_with("Duplicate declaration")));
}

#[test]
fn semantic_operation_and_group_aliases_resolve_across_source_modules() {
    let engine = SemanticEngine::default();
    let sources = BTreeMap::from([
        (
            "main.yl".into(),
            r#"
import { Binder as Binding } from "./bindings"
import { lexicalScope as scope, use as lookup } from "std/variables"
node Reference = name: /[a-z]+/ ";" { meanings { lookup(name) } }
pattern Scoped(item) = item* { meanings { scope() } }
node Program = Scoped(Binding | Reference) {
    meanings { precedence { Binding::meanings::Register > Reference } }
}
entry Program
"#
            .into(),
        ),
        (
            "bindings.yl".into(),
            r#"
import { declare as register } from "std/variables"
export node Binder = "let" name: /[a-z]+/ ";" {
    meanings { group Register { register(name) } }
}
"#
            .into(),
        ),
    ]);
    let language = engine.compile("main.yl", &sources).unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&language, "program", "x;letx;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let artifact: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    assert_eq!(
        artifact["meanings"]["main.yl#Program"]["precedence"][0]["selectors"][0]["definition"],
        "bindings.yl#Binder"
    );
}

#[test]
fn selected_unannotated_patterns_keep_their_occurrence_boundary() {
    let engine = SemanticEngine::default();
    let source = r#"
import { lexicalScope, declare, use } from "std/variables"
node Declaration = "let" name: /[a-z]+/ ";" { meanings { declare(name) } }
node Reference = name: /[a-z]+/ ";" { meanings { use(name) } }
pattern Uses = Reference*
node Program = Declaration Uses {
    meanings { lexicalScope() precedence { Declaration > Uses } }
}
entry Program
"#;
    let language = engine.compile("language.yl", &sources(source)).unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&language, "program", "letx;x;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.occurrences[0]
        .children
        .iter()
        .any(|occurrence| occurrence.definition == "language.yl#Uses"));
}
