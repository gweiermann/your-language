#[path = "../examples/mini_typescript/definition.rs"]
mod definition;
#[path = "../examples/mini_typescript/inference.rs"]
mod inference;
use your_language::semantics::{AnalysisResult, SemanticKey, SemanticValue};
fn analyze(source: &str) -> AnalysisResult {
    inference::engine()
        .compile_and_analyze(definition::ENTRY, &definition::sources(), "test.ts", source)
        .unwrap()
}
fn types(result: &AnalysisResult, name: &str) -> Vec<String> {
    result
        .graph
        .records
        .iter()
        .filter_map(|record| {
            let get = |key: &str| {
                record
                    .entries
                    .iter()
                    .find(|(k, _)| *k == SemanticKey::Symbol(key.into()))
                    .map(|(_, v)| v)
            };
            if get("std/variables#nameSymbol") != Some(&SemanticValue::Text(name.into())) {
                return None;
            }
            inference::display_type(record)
        })
        .collect()
}
#[test]
fn literals_annotations_and_calls() {
    let result = analyze(include_str!(
        "../examples/mini_typescript/programs/program.txt"
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(types(&result, "identity"), ["(number) => number"]);
    assert_eq!(types(&result, "again"), ["number"]);
    assert_eq!(types(&result, "title"), ["string"]);
    assert_eq!(types(&result, "enabled"), ["boolean"]);
}
#[test]
fn multiple_parameters_nested_calls_and_zero_arguments() {
    let result = analyze("function pair(a: number, b: string) { return b; } function zero() { return 1; } let result = pair(zero(), \"yes\");");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(types(&result, "pair"), ["(number, string) => string"]);
    assert_eq!(types(&result, "result"), ["string"]);
}
#[test]
fn annotation_and_call_errors_have_original_spans() {
    let source = include_str!("../examples/mini_typescript/programs/errors.txt");
    let result = analyze(source);
    assert_eq!(result.diagnostics.len(), 4, "{:?}", result.diagnostics);
    assert!(result.diagnostics.iter().all(|d| d.code == "types.mismatch"
        && d.primary.file == "test.ts"
        && d.primary.end <= source.len()));
    assert_eq!(
        &source[result.diagnostics[0].primary.start..result.diagnostics[0].primary.end],
        ": number"
    );
}
#[test]
fn declarations_references_keep_identity_across_shadowing() {
    let result =
        analyze("let value = 1; function inner(value: string) { return value; } let copy = value;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(types(&result, "value"), ["string", "number"]);
    assert_eq!(types(&result, "copy"), ["number"]);
    let declarations: Vec<_> = result
        .graph
        .attachments
        .iter()
        .filter(|a| a.key == "std/variables#declaration")
        .map(|a| &a.value)
        .collect();
    for reference in result
        .graph
        .attachments
        .iter()
        .filter(|a| a.key == "std/variables#reference")
    {
        assert!(declarations.contains(&&reference.value));
    }
}
#[test]
fn unanchored_mutual_recursion_remains_unknown() {
    let result = analyze("function first(value) { return second(value); } function second(value) { return first(value); }");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(types(&result, "first"), ["(unknown) => unknown"]);
    assert_eq!(types(&result, "second"), ["(unknown) => unknown"]);
}
#[test]
fn infinite_types_are_diagnosed() {
    let result = analyze("function apply(value) { return value(value); }");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("infinite type")),
        "{:?}",
        result.diagnostics
    );
}
#[test]
fn arity_mismatch_is_diagnosed() {
    let result = analyze("function echo(value) { return value; } let bad = echo();");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
}
#[test]
fn inference_is_monomorphic() {
    let result = analyze(
        "function echo(value) { return value; } let one = echo(1); let two = echo(\"two\");",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
}
#[test]
fn lexical_errors_and_syntax_errors_remain_distinct() {
    let result = analyze("let self = self;");
    assert_eq!(result.diagnostics[0].code, "semantic.unresolved_reference");
    let result = analyze("let broken = ;");
    assert!(!result.diagnostics.is_empty());
    assert!(result.graph.records.is_empty());
}
#[test]
fn repeated_concurrent_and_reload_analysis_is_isolated() {
    let engine = inference::engine();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let first = engine.analyze(&language, "same", "let value = 1;");
    assert_eq!(first, engine.analyze(&reloaded, "same", "let value = 1;"));
    assert_eq!(
        types(
            &engine.analyze(&language, "other", "let value = \"yes\";"),
            "value"
        ),
        ["string"]
    );
    std::thread::scope(|scope| {
        let one = scope.spawn(|| engine.analyze(&language, "same", "let value = 1;"));
        let two = scope.spawn(|| engine.analyze(&language, "other", "let value = false;"));
        assert_eq!(one.join().unwrap(), first);
        assert_eq!(types(&two.join().unwrap(), "value"), ["boolean"]);
    });
}
#[test]
fn attachment_reader_supports_another_native_library() {
    use your_language::{
        diagnostic::Diagnostic,
        semantics::{NativeOperation, OperationContext, OperationSignature},
    };
    struct Audit;
    impl NativeOperation for Audit {
        fn after(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            assert!(context
                .attachment(context.occurrence.id, "missing")
                .is_none());
            let declaration = &context.occurrence.children[0];
            let Some(SemanticValue::Record(id)) =
                context.attachment(declaration.id, "std/variables#declaration")
            else {
                panic!("Missing declaration identity")
            };
            let id = *id;
            context.set(
                id,
                SemanticKey::Symbol("test/audit#checked".into()),
                SemanticValue::Bool(true),
            )?;
            Ok(())
        }
    }
    let mut engine = inference::engine();
    engine
        .registry_mut()
        .register(
            "test/audit",
            "audit",
            OperationSignature {
                id: "test/audit#audit".into(),
                parameters: vec![],
                finalization_only: false,
                provides_context: false,
            },
            || Box::new(Audit),
        )
        .unwrap();
    let mut sources = definition::sources();
    let source = sources.get_mut(definition::ENTRY).unwrap();
    *source = format!(
        "import {{ audit }} from \"test/audit\"\n{}",
        source.replace("precedence {", "audit() precedence {")
    );
    let result = engine
        .compile_and_analyze(definition::ENTRY, &sources, "audit", "let value = 1;")
        .unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result
        .graph
        .records
        .iter()
        .any(|record| record.entries.contains(&(
            SemanticKey::Symbol("test/audit#checked".into()),
            SemanticValue::Bool(true)
        ))));
}

#[test]
fn function_aliases_share_parameter_and_return_slots() {
    let result =
        analyze("function id(value) { return value; } let alias = id; let result = alias(1);");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(types(&result, "id"), ["(number) => number"]);
    assert_eq!(types(&result, "alias"), ["(number) => number"]);
    assert_eq!(types(&result, "result"), ["number"]);
}

#[test]
fn function_arguments_propagate_signature_slots_through_parameters() {
    let result = analyze("function identity(value) { return value; } function apply(f, value) { return f(value); } let result = apply(identity, 1);");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(types(&result, "identity"), ["(number) => number"]);
    assert_eq!(types(&result, "result"), ["number"]);
}
