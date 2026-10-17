use std::path::PathBuf;
use your_language::{
    compile_language, load_compiled_language,
    semantics::{
        queries::QueryKey, AnalysisResult, Occurrence, SemanticEngine, SemanticIdentity,
        SemanticValue,
    },
};

fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/compound-types")
}

fn analyze(name: &str) -> (String, your_language::semantics::AnalysisResult) {
    let language = compile_language(directory().join("definition/language.yl")).unwrap();
    let reloaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let program = std::fs::read_to_string(directory().join("programs").join(name)).unwrap();
    let result = SemanticEngine::default().analyze(&reloaded, name, &program);
    (program, result)
}

fn tuple(value: &SemanticValue) -> &[SemanticValue] {
    let SemanticValue::Tuple(values) = value else {
        panic!("Expected tuple, got {value:?}")
    };
    values
}

fn list(value: &SemanticValue) -> &[SemanticValue] {
    let SemanticValue::List(values) = value else {
        panic!("Expected list, got {value:?}")
    };
    values
}

fn tag(value: &SemanticValue) -> &str {
    let SemanticValue::Symbol(name) = &tuple(value)[0] else {
        panic!("Expected type tag, got {value:?}")
    };
    name.rsplit("::").next().unwrap()
}

fn declaration_type<'a>(result: &'a AnalysisResult, name: &str) -> &'a SemanticValue {
    fn find<'a>(items: &'a [Occurrence], name: &str) -> Option<&'a Occurrence> {
        items.iter().find_map(|item| {
            if item.definition.ends_with("#LetDeclaration")
                && item
                    .captures
                    .get("name")
                    .is_some_and(|capture| capture.text == name)
            {
                Some(item)
            } else {
                find(&item.children, name)
            }
        })
    }
    let declaration =
        find(&result.occurrences, name).unwrap_or_else(|| panic!("Missing declaration {name}"));
    let evidence = result.graph.query_entries.iter().find(|entry| matches!(&entry.key, QueryKey::Tuple(values) if matches!(values.as_slice(), [QueryKey::Identity(SemanticIdentity::Occurrence(id)), QueryKey::Symbol(symbol)] if *id == declaration.id && symbol.ends_with("#typeEvidence"))))
        .unwrap_or_else(|| panic!("Missing type evidence for {name}"));
    &tuple(&evidence.value)[0]
}

fn field<'a>(descriptor: &'a SemanticValue, name: &str) -> &'a SemanticValue {
    list(&tuple(descriptor)[1])
        .iter()
        .find_map(|item| {
            let parts = tuple(item);
            (parts[0] == SemanticValue::Text(name.into())).then_some(&parts[1])
        })
        .unwrap_or_else(|| panic!("Missing field {name} in {descriptor:?}"))
}

#[test]
fn interfaces_tuples_lists_members_and_explicit_generics_compile_reload_and_check() {
    let (_, result) = analyze("valid.txt");
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(result.ast.is_some());
    assert_eq!(tag(declaration_type(&result, "name")), "string");
    assert_eq!(tag(declaration_type(&result, "score")), "number");
    assert_eq!(tag(declaration_type(&result, "label")), "string");
    let pair = declaration_type(&result, "pair");
    assert_eq!(tag(pair), "tuple");
    assert_eq!(
        list(&tuple(pair)[1]).iter().map(tag).collect::<Vec<_>>(),
        ["number", "string"]
    );
    let mixed = declaration_type(&result, "mixed");
    assert_eq!(tag(mixed), "list");
    let members = &tuple(mixed)[1];
    assert_eq!(tag(members), "union");
    assert_eq!(
        list(&tuple(members)[1]).iter().map(tag).collect::<Vec<_>>(),
        ["number", "string"]
    );
    let result_type = declaration_type(&result, "result");
    assert_eq!(
        tag(field(
            field(field(field(result_type, "payload"), "details"), "record"),
            "value"
        )),
        "number"
    );
}

#[test]
fn nested_generic_mismatch_preserves_scattered_literal_evidence() {
    let (source, result) = analyze("invalid-scattered.txt");
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    let diagnostic = &result.diagnostics[0];
    assert!(
        diagnostic
            .message
            .contains("string is not assignable to number"),
        "{diagnostic:?}"
    );
    assert!(
        diagnostic.message.contains("payload.details.record.value"),
        "{diagnostic:?}"
    );
    assert_eq!(
        &source[diagnostic.primary.start..diagnostic.primary.end],
        "\"wrong\""
    );
    assert!(diagnostic.annotations.len() >= 2, "{diagnostic:?}");
    let rendered = your_language::diagnostic_render::render_diagnostics(
        std::slice::from_ref(diagnostic),
        |span| (span.file == "invalid-scattered.txt").then(|| source.clone()),
        false,
    );
    assert!(rendered.contains("\"wrong\""), "{rendered}");
}

#[test]
fn missing_member_highlights_only_the_member_name() {
    let (source, result) = analyze("invalid-member.txt");
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    let diagnostic = &result.diagnostics[0];
    assert!(
        diagnostic.message.contains("does not exist"),
        "{diagnostic:?}"
    );
    assert_eq!(
        &source[diagnostic.primary.start..diagnostic.primary.end],
        "missing"
    );
}

#[test]
fn member_assignment_mismatch_highlights_the_right_hand_value() {
    let (source, result) = analyze("invalid-assignment.txt");
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    let diagnostic = &result.diagnostics[0];
    assert!(
        diagnostic
            .message
            .contains("number is not assignable to string"),
        "{diagnostic:?}"
    );
    assert_eq!(
        &source[diagnostic.primary.start..diagnostic.primary.end],
        "42"
    );
}

#[test]
fn missing_fields_tuple_lengths_list_elements_arity_and_index_errors_are_precise() {
    for (file, message, highlighted) in [
        (
            "invalid-missing-field.txt",
            "Missing required field scores",
            "{ name: \"Ada\" }",
        ),
        ("invalid-tuple-length.txt", "Tuple length differs", "(42)"),
        (
            "invalid-list.txt",
            "string is not assignable to number",
            "\"wrong\"",
        ),
        (
            "invalid-generic-arity.txt",
            "Wrong number of type arguments",
            "Box<number, string>",
        ),
        (
            "invalid-missing-type-arguments.txt",
            "Wrong number of type arguments",
            "Box",
        ),
        ("invalid-index.txt", "Only homogeneous list types", "0"),
    ] {
        let (source, result) = analyze(file);
        assert_eq!(
            result.diagnostics.len(),
            1,
            "{file}: {:#?}",
            result.diagnostics
        );
        let diagnostic = &result.diagnostics[0];
        assert!(
            diagnostic.message.contains(message),
            "{file}: {diagnostic:?}"
        );
        assert_eq!(
            &source[diagnostic.primary.start..diagnostic.primary.end],
            highlighted,
            "{file}"
        );
    }
}

#[test]
fn cli_compiles_reloads_and_emits_grouped_human_and_json_diagnostics() {
    use std::process::Command;
    let artifact =
        std::env::temp_dir().join(format!("yl-compound-example-{}.ylc", std::process::id()));
    let cli = env!("CARGO_BIN_EXE_yl");
    let compiled = Command::new(cli)
        .arg("compile")
        .arg(directory().join("definition/language.yl"))
        .arg("-o")
        .arg(&artifact)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let valid = Command::new(cli)
        .arg("language")
        .arg(&artifact)
        .arg("check")
        .arg(directory().join("programs/valid.txt"))
        .output()
        .unwrap();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let human = Command::new(cli)
        .env("NO_COLOR", "1")
        .arg("language")
        .arg(&artifact)
        .arg("check")
        .arg(directory().join("programs/invalid-scattered.txt"))
        .output()
        .unwrap();
    assert!(!human.status.success());
    let diagnostic = String::from_utf8(human.stderr).unwrap();
    assert!(
        diagnostic.contains("payload.details.record.value"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("\"wrong\""), "{diagnostic}");
    assert!(diagnostic.contains("^^^^^^^"), "{diagnostic}");
    let json = Command::new(cli)
        .arg("language")
        .arg(&artifact)
        .arg("check")
        .arg(directory().join("programs/invalid-scattered.txt"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(!json.status.success());
    let diagnostics: Vec<your_language::diagnostic::Diagnostic> =
        serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].annotations.len() >= 2);
    let source =
        std::fs::read_to_string(directory().join("programs/invalid-scattered.txt")).unwrap();
    assert_eq!(
        &source[diagnostics[0].primary.start..diagnostics[0].primary.end],
        "\"wrong\""
    );
    std::fs::remove_file(artifact).unwrap();
}

#[test]
fn duplicate_object_and_interface_fields_group_both_exact_names() {
    for file in [
        "invalid-duplicate-object-field.txt",
        "invalid-duplicate-interface-field.txt",
    ] {
        let (source, result) = analyze(file);
        assert_eq!(
            result.diagnostics.len(),
            1,
            "{file}: {:#?}",
            result.diagnostics
        );
        let diagnostic = &result.diagnostics[0];
        assert_eq!(diagnostic.message, "Duplicate field name");
        assert_eq!(diagnostic.annotations.len(), 2, "{diagnostic:?}");
        for annotation in diagnostic.annotations.iter() {
            assert_eq!(&source[annotation.span.start..annotation.span.end], "name");
        }
        assert_ne!(
            diagnostic.annotations[0].span,
            diagnostic.annotations[1].span
        );
    }
}

#[test]
fn recursive_interface_is_rejected_instead_of_silently_becoming_unknown() {
    let (source, result) = analyze("invalid-recursive-interface.txt");
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "query.diagnostic");
    assert!(
        diagnostic.message.contains("Unknown name 'Loop'"),
        "{diagnostic:?}"
    );
    assert_eq!(
        &source[diagnostic.primary.start..diagnostic.primary.end],
        "Loop"
    );
}
