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
    edit(sources.get_mut("definition/expression.yl").unwrap());
    inference::engine()
        .compile_and_analyze(definition::ENTRY, &sources, "input.ts", source)
        .unwrap()
}
fn replace_action(grammar: &mut String, pattern: &str, action: &str) {
    let original = grammar
        .lines()
        .find(|line| {
            line.split_once("=>")
                .is_some_and(|(head, _)| head.trim() == pattern)
        })
        .expect("fixture must contain the policy case")
        .to_owned();
    let replacement = format!("{}=> {action}", original.split_once("=>").unwrap().0);
    *grammar = grammar.replace(&original, &replacement);
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
        if get("std/variables#nameSymbol") == Some(&SemanticValue::Text(name.into())) {
            if let Some(ty) = inference::display_type(record) {
                return ty;
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
        replace_action(grammar, "number, number", "annotateWithType(string),")
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
    assert_eq!(binding(&result, "add"), "(unknown) => unknown");
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
        replace_action(grammar, "number, number", "{},")
    });
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), "unknown");
}
#[test]
fn explicit_unknown_is_settled_and_propagates_to_next_match() {
    let result = analyze_with("let first = 1 + 2; let second = first + 3;", |grammar| {
        replace_action(grammar, "number, number", "annotateWithType(unknown),")
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
            replace_action(
                grammar,
                "_",
                "{ warning(\"${left} + ${right} is experimental\") annotateWithType(unknown) }",
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

#[test]
fn language_authored_type_symbols_can_be_renamed_and_reloaded() {
    let mut sources = definition::sources();
    for source in sources.values_mut() {
        *source = source.replace("number", "decimal");
    }
    let engine = inference::engine();
    let language = engine.compile(definition::ENTRY, &sources).unwrap();
    let bytes = language.to_bytes().unwrap();
    let language = engine.load(&bytes).unwrap();
    let result = engine.analyze(&language, "input.ts", "let result: decimal = 1;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), "decimal");
}

#[test]
fn authored_symbols_obey_export_visibility_and_duplicate_rules() {
    let engine = inference::engine();
    let mut sources = definition::sources();
    *sources.get_mut("definition/types.yl").unwrap() =
        sources["definition/types.yl"].replace("export symbol number", "symbol number");
    assert!(engine.compile(definition::ENTRY, &sources).is_err());
    let mut sources = definition::sources();
    sources
        .get_mut("definition/types.yl")
        .unwrap()
        .push_str("\nsymbol number\n");
    assert_eq!(
        engine.compile(definition::ENTRY, &sources).unwrap_err()[0].code,
        "yl.duplicate_declaration"
    );
}
#[test]
fn primitive_symbols_are_language_owned_and_unknown_is_library_owned() {
    let engine = inference::engine();
    for name in ["number", "string", "boolean", "Function"] {
        assert!(engine.registry().symbol_id("std/types", name).is_none());
    }
    assert_eq!(
        engine.registry().symbol_id("std/types", "unknown"),
        Some("std/types#unknown")
    );
    let result = analyze("let value = 1;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "value"), "number");
}

#[test]
fn arbitrary_function_slot_names_preserve_inference_through_reload() {
    let mut sources = definition::sources();
    for source in sources.values_mut() {
        *source = source
            .replace("parameters", "inputs")
            .replace("returnType", "output");
    }
    let engine = inference::engine();
    let language = engine.compile(definition::ENTRY, &sources).unwrap();
    let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(
        &reloaded,
        "input.ts",
        "function id(value) { return value; } let result = id(1);",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "result"), "number");
    let function = result
        .graph
        .records
        .iter()
        .find(|record| {
            record.entries.contains(&(
                SemanticKey::Symbol("std/variables#nameSymbol".into()),
                SemanticValue::Text("id".into()),
            ))
        })
        .unwrap();
    assert!(function.entries.contains(&(
        SemanticKey::Symbol("definition/types.yl#inputs".into()),
        SemanticValue::Text("[unknown]".into())
    )));
    assert!(function.entries.contains(&(
        SemanticKey::Symbol("definition/types.yl#output".into()),
        SemanticValue::Text("unknown".into())
    )));
}

#[test]
fn references_and_declarations_share_type_facts_without_retyping_initializer() {
    use your_language::semantics::{Occurrence, SemanticIdentity};
    let result = analyze_with("let original = 1; let copied = original;", |grammar| {
        assert!(grammar.contains("referenceType(name)"));
        *grammar = grammar.replace(
            "referenceType(name)",
            "referenceType(name)\n        annotateWithType(string)",
        );
    });
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "original"), "number | string");
    assert_eq!(binding(&result, "copied"), "number | string");
    fn number_id(occurrences: &[Occurrence]) -> Option<usize> {
        for occurrence in occurrences {
            if occurrence.definition.ends_with("#Number") {
                return Some(occurrence.id);
            }
            if let Some(id) = number_id(&occurrence.children) {
                return Some(id);
            }
        }
        None
    }
    let key = SemanticValue::Identity(SemanticIdentity::Occurrence(
        number_id(&result.occurrences).unwrap(),
    ));
    let record = result
        .graph
        .records
        .iter()
        .find(|record| {
            record
                .entries
                .iter()
                .any(|(k, v)| *k == SemanticKey::Symbol("std/types#key".into()) && *v == key)
        })
        .unwrap();
    assert!(record.entries.contains(&(
        SemanticKey::Symbol("std/types#type".into()),
        SemanticValue::Symbol("definition/types.yl#number".into())
    )));
}
