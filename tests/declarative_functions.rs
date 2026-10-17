use your_language::{
    compile_language, load_compiled_language,
    semantics::{queries::QueryKey, SemanticEngine, SemanticIdentity, SemanticValue},
};

fn analyze(source: &str) -> your_language::semantics::AnalysisResult {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let language =
        compile_language(root.join("examples/10-declarative-functions/definition/language.yl"))
            .unwrap();
    let reloaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    SemanticEngine::default().analyze(&reloaded, "program", source)
}

fn call_types(result: &your_language::semantics::AnalysisResult) -> Vec<(String, String)> {
    fn visit(owner: &your_language::semantics::Occurrence, calls: &mut Vec<usize>) {
        if owner.definition.ends_with("#Expression::Call") {
            calls.push(owner.id);
        }
        for child in &owner.children {
            visit(child, calls);
        }
    }
    let mut calls = vec![];
    for owner in &result.occurrences {
        visit(owner, &mut calls);
    }
    result
        .graph
        .query_entries
        .iter()
        .filter_map(|entry| {
            let QueryKey::Tuple(parts) = &entry.key else {
                return None;
            };
            let [QueryKey::Identity(SemanticIdentity::Occurrence(id)), QueryKey::Symbol(slot)] =
                parts.as_slice()
            else {
                return None;
            };
            if !calls.contains(id) || !slot.ends_with("#type") {
                return None;
            }
            let SemanticValue::Symbol(value) = &entry.value else {
                return None;
            };
            Some((
                entry.span.start.to_string(),
                value.rsplit('#').next().unwrap().to_string(),
            ))
        })
        .collect()
}

#[test]
fn authored_identity_function_has_independent_number_and_string_calls() {
    let result = analyze(include_str!(
        "../examples/10-declarative-functions/programs/identity.txt"
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let types = call_types(&result);
    assert_eq!(
        types.iter().map(|(_, ty)| ty.as_str()).collect::<Vec<_>>(),
        vec!["number", "string"]
    );
    assert!(result.graph.query_entries.iter().any(|entry| matches!(&entry.key,QueryKey::Tuple(parts) if matches!(&parts[1],QueryKey::Symbol(slot) if slot.ends_with("#returnType"))) && matches!(&entry.value,SemanticValue::Symbol(value) if value.ends_with("#unknown"))));
}

#[test]
fn each_parameter_is_addressed_by_name_and_nested_hypothetical_calls_remain_isolated() {
    let result = analyze(
        r#"
        function first(left, right) { return left; }
        function identity(value) { return value; }
        function nested(value) { return identity(value); }
        first(1, "text");
        first("text", 1);
        nested(1);
        nested("text");
    "#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let types = call_types(&result);
    assert_eq!(
        types.iter().map(|(_, ty)| ty.as_str()).collect::<Vec<_>>(),
        vec!["unknown", "number", "string", "number", "string"]
    );
}

#[test]
fn authored_function_arity_and_unresolved_callee_diagnostics_have_source_spans() {
    let result = analyze(include_str!(
        "../examples/10-declarative-functions/programs/errors.txt"
    ));
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message == "Wrong number of arguments"));
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message == "Unknown name 'missing'"));
    assert!(result
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.primary.file == "program"));
}
