#[path = "../examples/mini_typescript/definition.rs"]
mod definition;
#[path = "../examples/mini_typescript/inference.rs"]
mod inference;
use your_language::semantics::AnalysisResult;

fn analyze_with(source: &str, edit: impl FnOnce(&mut String)) -> AnalysisResult {
    let mut sources = definition::sources();
    edit(sources.get_mut("definition/expression.yl").unwrap());
    let engine = inference::engine();
    let compiled = engine.compile(definition::ENTRY, &sources).unwrap();
    let loaded = engine.load(&compiled.to_bytes().unwrap()).unwrap();
    engine.analyze(&loaded, "input.ts", source)
}
fn analyze(source: &str) -> AnalysisResult {
    analyze_with(source, |_| {})
}
fn binding(result: &AnalysisResult, name: &str) -> String {
    inference::binding_types(result, name)
        .first()
        .cloned()
        .expect("Missing inferred binding")
}
fn replace_action(grammar: &mut String, pattern: &str, action: &str) {
    let original = grammar
        .lines()
        .find(|line| {
            line.split_once("=>")
                .is_some_and(|(head, _)| head.trim() == pattern)
        })
        .expect("Fixture policy case")
        .to_owned();
    *grammar = grammar.replace(
        &original,
        &format!("{}=> {action}", original.split_once("=>").unwrap().0),
    );
}

#[test]
fn addition_policy_lives_in_authored_meanings() {
    let result = analyze(include_str!(
        "../examples/mini_typescript/programs/addition.txt"
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    for (name, expected) in [
        ("numberSum", "Type::number"),
        ("stringSum", "Type::string"),
        ("mixedLeft", "Type::string"),
        ("mixedRight", "Type::string"),
    ] {
        assert_eq!(binding(&result, name), expected);
    }
    let changed = analyze_with("let result = 1 + 2;", |grammar| {
        replace_action(
            grammar,
            "Type::number, Type::number",
            "annotateWithType(self, Type::string),",
        )
    });
    assert!(changed.diagnostics.is_empty(), "{:?}", changed.diagnostics);
    assert_eq!(binding(&changed, "result"), "Type::string");
}

#[test]
fn unsupported_additions_emit_authored_diagnostics() {
    let result = analyze(include_str!(
        "../examples/mini_typescript/programs/addition-errors.txt"
    ));
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
    assert!(result
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.message.contains("is not possible")));
}

#[test]
fn pending_call_operands_recompute_under_hypothetical_parameters() {
    let result = analyze("function add(value) { return value + 1; } let result: number = add(41);");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "add"), "(unknown) => unknown");
    assert_eq!(binding(&result, "result"), "Type::number");
}

#[test]
fn void_branch_produces_no_type_facts() {
    let result = analyze_with("let result = 1 + 2;", |grammar| {
        replace_action(grammar, "Type::number, Type::number", "{},")
    });
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), "unknown");
}

#[test]
fn explicit_unknown_propagates_to_the_next_match() {
    let result = analyze_with("let first = 1 + 2; let second = first + 3;", |grammar| {
        replace_action(
            grammar,
            "Type::number, Type::number",
            "annotateWithType(self, unknown),",
        )
    });
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "first"), "unknown");
    assert_eq!(binding(&result, "second"), "unknown");
}

#[test]
fn warning_and_annotation_actions_can_share_a_branch() {
    let result = analyze_with(
        "let first = true + false; let second = first + 3;",
        |grammar| {
            grammar.insert_str(0, "import { warn } from \"core/diagnostics\"\n");
            replace_action(
                grammar,
                "_",
                "{ warn(\"${left} + ${right} is experimental\") annotateWithType(self, unknown) }",
            )
        },
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(
        result.diagnostics[0].severity,
        your_language::diagnostic::Severity::Warning
    );
    assert_eq!(binding(&result, "second"), "unknown");
}

#[test]
fn target_type_symbols_can_be_renamed_without_rust_changes() {
    let mut sources = definition::sources();
    for source in sources.values_mut() {
        // Target type names are independent from standard editor categories.
        *source = source
            .replace("Type::number", "Type::decimal")
            .replace("\n    number\n", "\n    decimal\n")
            .replace("= \"number\"", "= \"decimal\"");
    }
    let engine = inference::engine();
    let compiled = engine.compile(definition::ENTRY, &sources).unwrap();
    let loaded = engine.load(&compiled.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "input.ts", "let result: decimal = 1;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), "Type::decimal");
}

#[test]
fn function_slot_names_can_be_changed_in_the_language() {
    let mut sources = definition::sources();
    for source in sources.values_mut() {
        *source = source
            .replace("parameters", "inputs")
            .replace("returnType", "output");
    }
    let engine = inference::engine();
    let compiled = engine.compile(definition::ENTRY, &sources).unwrap();
    let loaded = engine.load(&compiled.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(
        &loaded,
        "input.ts",
        "function id(value) { return value; } let result = id(1);",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    // The renderer knows only the demonstration's original slot names. A plain
    // result type needs no slot knowledge, and the solver uses the renamed keys.
    assert_eq!(binding(&result, "result"), "Type::number");
}

#[test]
fn union_candidates_keep_each_valid_addition_result() {
    let result = analyze_with("let value = 1; let result = value + 1;", |grammar| {
        *grammar = grammar.replacen(
            "annotateWithType(self, Type::number)",
            "annotateWithType(self, Type::number)\n        annotateWithType(self, Type::string)",
            1,
        );
    });
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), "Type::number | Type::string");
}
