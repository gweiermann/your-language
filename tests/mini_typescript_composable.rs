#[path = "../examples/mini_typescript/definition.rs"]
mod definition;
#[path = "../examples/mini_typescript/inference.rs"]
mod inference;
use your_language::{
    diagnostic::Severity,
    semantics::{AnalysisResult, SemanticKey, SemanticValue},
};
fn analyze(source: &str) -> AnalysisResult {
    analyze_with(source, |_| {})
}
fn analyze_with(source: &str, edit: impl FnOnce(&mut String)) -> AnalysisResult {
    let mut sources = definition::sources();
    edit(sources.get_mut(definition::ENTRY).unwrap());
    inference::engine()
        .compile_and_analyze(definition::ENTRY, &sources, "input.ts", source)
        .unwrap()
}
fn binding(result: &AnalysisResult, name: &str) -> String {
    for record in &result.graph.records {
        let get = |key: &str| {
            record
                .entries
                .iter()
                .find(|(k, _)| *k == SemanticKey::Symbol(key.into()))
                .map(|(_, v)| v)
        };
        if get("std/semantics#nameSymbol") == Some(&SemanticValue::Text(name.into())) {
            if let Some(SemanticValue::Text(ty)) = get("prototype/types#inferredType") {
                return ty.clone();
            }
        }
    }
    panic!("Missing inferred binding {name}")
}
#[test]
fn addition_policy_lives_in_local_meanings() {
    let result = analyze(include_str!(
        "../examples/mini_typescript/programs/addition.txt"
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    for (name, ty) in [
        ("numberSum", "number"),
        ("stringSum", "string"),
        ("mixedLeft", "string"),
        ("mixedRight", "string"),
    ] {
        assert_eq!(binding(&result, name), ty);
    }
    let changed = analyze_with("let result = 1 + 2;", |grammar| {
        *grammar = grammar.replace(
            "number, number => type(number)",
            "number, number => type(string)",
        )
    });
    assert!(changed.diagnostics.is_empty());
    assert_eq!(binding(&changed, "result"), "string");
}
#[test]
fn unsupported_types_emit_each_interpolated_diagnostic_once() {
    let source = include_str!("../examples/mini_typescript/programs/addition-errors.txt");
    let result = analyze(source);
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
    assert_eq!(
        result.diagnostics[0].message,
        "true + false is not possible"
    );
    assert_eq!(
        &source[result.diagnostics[0].primary.start..result.diagnostics[0].primary.end],
        "true"
    );
    assert_eq!(
        &source[result.diagnostics[0].secondary[0].start..result.diagnostics[0].secondary[0].end],
        "false"
    );
    assert!(result
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.severity == Severity::Error));
}
#[test]
fn pending_operands_wait_for_call_inference_instead_of_taking_unknown_arm() {
    let result = analyze("function add(value) { return value + 1; } let result: number = add(41);");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "add"), "(number) => number");
    assert_eq!(binding(&result, "result"), "number");
}
#[test]
fn unresolved_and_recursive_operands_settle_without_spurious_fallback_errors() {
    for source in [
        "function add(value) { return value + 1; }",
        "function first(value) { return second(value) + 1; } function second(value) { return first(value); } let result = first(1);",
    ] {
        let result=analyze(source); assert!(result.diagnostics.is_empty(),"{:?}",result.diagnostics);
        let expected = if source.contains("function add") { binding(&result,"add") } else { binding(&result,"result") };
        assert!(expected.ends_with("unknown"));
    }
}
#[test]
fn annotation_anchors_recursive_addition_constraints() {
    let result=analyze("function first(value: number): number { return second(value) + 1; } function second(value: number): number { return first(value); } let result = first(1);");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), "number");
}
#[test]
fn no_op_void_branch_does_not_implicitly_publish_a_type() {
    let result = analyze_with("let result = 1 + 2;", |grammar| {
        *grammar = grammar.replace("number, number => type(number)", "number, number => {}")
    });
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), "unknown");
}
#[test]
fn explicit_unknown_is_settled_and_propagates_to_next_match() {
    let result = analyze_with("let first = 1 + 2; let second = first + 3;", |grammar| {
        *grammar = grammar.replace(
            "number, number => type(number)",
            "number, number => type(unknown)",
        )
    });
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "first"), "unknown");
    assert_eq!(binding(&result, "second"), "unknown");
}
#[test]
fn warning_and_type_branch_effects_are_once_only() {
    let result = analyze_with(
        "let first = true + false; let second = first + 3;",
        |grammar| {
            *grammar = grammar.replace(
                "_ => error(\"${left} + ${right} is not possible\")",
                "_ => { warning(\"${left} + ${right} is experimental\") type(unknown) }",
            )
        },
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].severity, Severity::Warning);
    assert_eq!(binding(&result, "second"), "unknown");
}
#[test]
fn local_match_operations_survive_reload_and_isolated_concurrent_analysis() {
    let engine = inference::engine();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let source = "function add(value) { return value + 1; } let result = add(41);";
    let direct = engine.analyze(&language, "input.ts", source);
    assert_eq!(direct, engine.analyze(&loaded, "input.ts", source));
    assert!(direct.diagnostics.is_empty());
    std::thread::scope(|scope| {
        let number = scope.spawn(|| engine.analyze(&loaded, "input.ts", source));
        let string = scope.spawn(|| {
            engine.analyze(
                &loaded,
                "input.ts",
                "function add(value) { return value + 1; } let result = add(\"value\");",
            )
        });
        assert_eq!(number.join().unwrap(), direct);
        let string = string.join().unwrap();
        assert!(string.diagnostics.is_empty());
        assert_eq!(binding(&string, "result"), "string");
    });
}
