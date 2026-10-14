#[path = "../examples/mini_typescript/definition.rs"]
mod definition;
#[path = "../examples/mini_typescript/inference.rs"]
mod inference;
use your_language::semantics::AnalysisResult;

fn analyze(source: &str) -> AnalysisResult {
    let engine = inference::engine();
    let compiled = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    assert!(serde_json::to_value(&compiled).unwrap()["query_meanings"]
        .as_object()
        .is_some_and(|groups| !groups.is_empty()));
    let loaded = engine.load(&compiled.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "test.ts", source);
    assert_eq!(result, engine.analyze(&compiled, "test.ts", source));
    result
}

fn binding(result: &AnalysisResult, name: &str) -> Vec<String> {
    inference::binding_types(result, name)
}

#[test]
fn literals_annotations_and_calls() {
    let result = analyze(include_str!(
        "../examples/mini_typescript/programs/program.txt"
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "identity"), ["(unknown) => unknown"]);
    assert_eq!(binding(&result, "count"), ["Type::number"]);
    assert_eq!(binding(&result, "again"), ["Type::number"]);
    assert_eq!(binding(&result, "title"), ["Type::string"]);
    assert_eq!(binding(&result, "enabled"), ["Type::boolean"]);
}

#[test]
fn multiple_parameters_nested_calls_and_zero_arguments() {
    let result = analyze("function pair(a: number, b: string) { return b; } function zero() { return 1; } let result = pair(zero(), \"yes\");");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        binding(&result, "pair"),
        ["(Type::number, Type::string) => Type::string"]
    );
    assert_eq!(binding(&result, "result"), ["Type::string"]);
}

#[test]
fn declaration_and_reference_types_respect_shadowing() {
    let result =
        analyze("let value = 1; function inner(value: string) { return value; } let copy = value;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "value"), ["Type::number", "Type::string"]);
    assert_eq!(binding(&result, "copy"), ["Type::number"]);
}

#[test]
fn mixed_calls_do_not_contaminate_the_declaration_signature() {
    let result = analyze(
        "function echo(value) { return value; } let one = echo(1); let two = echo(\"two\");",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "value"), ["unknown"]);
    assert_eq!(binding(&result, "echo"), ["(unknown) => unknown"]);
    assert_eq!(binding(&result, "one"), ["Type::number"]);
    assert_eq!(binding(&result, "two"), ["Type::string"]);
}

#[test]
fn rejected_annotation_does_not_become_an_inferred_generic_call_type() {
    let source = "function identity(value) { return value; } let count: string = identity(42); let title: string = identity(\"hello\"); let copy = count;";
    let result = analyze(source);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(
        diagnostic.message,
        "Type Type::number does not match Type::string"
    );
    assert_eq!(
        &source[diagnostic.primary.start..diagnostic.primary.end],
        "identity(42)"
    );
    assert!(diagnostic
        .secondary
        .iter()
        .any(|span| span.file == "test.ts" && &source[span.start..span.end] == "string"));
    assert_eq!(binding(&result, "count"), ["Type::number"]);
    assert_eq!(binding(&result, "copy"), ["Type::number"]);
    assert_eq!(binding(&result, "title"), ["Type::string"]);
    assert_eq!(binding(&result, "value"), ["unknown"]);
}

#[test]
fn aliases_keep_the_function_binding_identity() {
    let result =
        analyze("function id(value) { return value; } let alias = id; let result = alias(1);");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "id"), binding(&result, "alias"));
    assert_eq!(binding(&result, "result"), ["Type::number"]);
}

#[test]
fn function_arguments_retain_signatures_through_parameter_overrides() {
    let result = analyze(include_str!(
        "../examples/mini_typescript/programs/higher-order.txt"
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "identity"), ["(unknown) => unknown"]);
    assert_eq!(binding(&result, "result"), ["Type::number"]);
}

#[test]
fn nested_hypotheticals_keep_captured_outer_values_shared() {
    let result = analyze(include_str!(
        "../examples/mini_typescript/programs/generics.txt"
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    for (name, expected) in [
        ("outside", "Type::number"),
        ("count", "Type::number"),
        ("title", "Type::string"),
        ("numberResult", "Type::number"),
        ("stringResult", "Type::string"),
    ] {
        assert_eq!(binding(&result, name), [expected]);
    }
}

#[test]
fn generic_error_fixture_keeps_nested_returns_and_outer_types_independent() {
    let source = include_str!("../examples/mini_typescript/programs/generics-errors.txt");
    let result = analyze(source);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(
        diagnostic.message,
        "Type Type::number does not match Type::string"
    );
    assert_eq!(
        &source[diagnostic.primary.start..diagnostic.primary.end],
        "identity(42)"
    );
    for (name, expected) in [
        ("outside", "Type::number"),
        ("count", "Type::number"),
        ("title", "Type::string"),
        ("numberResult", "Type::number"),
        ("stringResult", "Type::string"),
    ] {
        assert_eq!(binding(&result, name), [expected]);
    }
}

#[test]
fn arity_and_non_callable_values_have_authored_diagnostics() {
    for (source, message) in [
        (
            "function echo(value) { return value; } let bad = echo();",
            "Wrong number of arguments",
        ),
        (
            "let count = 1; let bad = count(1);",
            "Expression is not callable",
        ),
    ] {
        let result = analyze(source);
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        assert_eq!(result.diagnostics[0].message, message);
    }
}

#[test]
fn annotation_errors_underline_the_actual_expression() {
    for (source, expected) in [
        ("let count: number = \"wrong\";", "\"wrong\""),
        (
            "function id(value: number) { return value; } let result = id(\"wrong\");",
            "\"wrong\"",
        ),
        ("function bad(): number { return \"wrong\"; }", "\"wrong\""),
    ] {
        let result = analyze(source);
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        let diagnostic = &result.diagnostics[0];
        assert_eq!(
            &source[diagnostic.primary.start..diagnostic.primary.end],
            expected
        );
        assert!(diagnostic.message.contains("does not match"));
        assert!(
            diagnostic.secondary.iter().any(|span| {
                span.file == "test.ts" && &source[span.start..span.end] == "number"
            }),
            "Missing exact related annotation: {diagnostic:?}"
        );
    }
}

#[test]
fn independent_checks_survive_a_bad_call() {
    let result = analyze(include_str!(
        "../examples/mini_typescript/programs/errors.txt"
    ));
    assert!(result.diagnostics.len() >= 3, "{:?}", result.diagnostics);
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message == "Expression is not callable"));
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Type::string")
            && diagnostic.message.contains("Type::number")));
}

#[test]
fn repeated_concurrent_analysis_is_isolated() {
    let engine = inference::engine();
    let compiled = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let loaded = engine.load(&compiled.to_bytes().unwrap()).unwrap();
    std::thread::scope(|scope| {
        let one = scope.spawn(|| engine.analyze(&loaded, "same", "let value = 1;"));
        let two = scope.spawn(|| engine.analyze(&loaded, "other", "let value = false;"));
        assert_eq!(binding(&one.join().unwrap(), "value"), ["Type::number"]);
        assert_eq!(binding(&two.join().unwrap(), "value"), ["Type::boolean"]);
    });
}

#[test]
fn syntax_and_reference_failures_are_structured() {
    let missing = analyze("let value = missing;");
    assert!(missing
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Unknown name")));
    let broken = analyze("let broken = ;");
    assert!(!broken.diagnostics.is_empty());
    assert!(broken.graph.query_entries.is_empty());
}

#[test]
fn hypothetical_case_error_keeps_actual_argument_origin() {
    let source = "function add(value) { return value + 1; } let bad = add(true); let other: number = \"wrong\";";
    let result = analyze(source);
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
    let invalid = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("is not possible"))
        .unwrap();
    assert_eq!(&source[invalid.primary.start..invalid.primary.end], "true");
}

#[test]
fn parameter_overrides_are_independent_and_positional() {
    let result = analyze("function choose(first, second) { return second; } let numberResult = choose(\"ignored\", 42); let stringResult = choose(true, \"yes\");");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "numberResult"), ["Type::number"]);
    assert_eq!(binding(&result, "stringResult"), ["Type::string"]);
    assert_eq!(
        binding(&result, "choose"),
        ["(unknown, unknown) => unknown"]
    );
}

#[test]
fn union_function_identities_resolve_each_signature() {
    let mut sources = definition::sources();
    let typing = sources.get_mut("definition/typing.yl").unwrap();
    *typing = typing.replace("referenceOf, enclosingScope", "referenceOf, enclosingScope, declarations").replacen(
        "annotateWithType(referenceOf(name), typeOf(value))",
        "annotateWithType(referenceOf(name), typeOf(value))\n    when equal(text(name), \"choice\") {\n        annotateWithType(referenceOf(name), get((enclosingScope(name), declarations, \"second\")))\n    }",
        1,
    );
    let engine = inference::engine();
    let compiled = engine.compile(definition::ENTRY, &sources).unwrap();
    let loaded = engine.load(&compiled.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "test.ts", "function first(value) { return 1; } function second(value) { return \"yes\"; } let choice = first; let result = choice(true);");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), ["Type::number | Type::string"]);
}

#[test]
fn second_parameter_type_error_points_to_second_argument() {
    let source = "function pick(first: number, second: string) { return second; } let result = pick(42, true);";
    let result = analyze(source);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let error = &result.diagnostics[0];
    assert_eq!(&source[error.primary.start..error.primary.end], "true");
    assert!(error
        .secondary
        .iter()
        .any(|span| span.file == "test.ts" && &source[span.start..span.end] == "string"));
}

#[test]
fn let_initializers_do_not_see_their_own_or_later_declarations() {
    for source in ["let value = value;", "let earlier = later; let later = 1;"] {
        let result = analyze(source);
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Unknown name"));
    }
}

#[test]
fn hoisted_functions_and_deferred_bodies_use_scheduled_visibility() {
    let result = analyze("let result = read(); let later = 42; function read() { return later; }");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), ["Type::number"]);
    assert_eq!(binding(&result, "read"), ["() => Type::number"]);
}
