use std::collections::BTreeMap;
use your_language::{
    diagnostic_render::render_diagnostics, load_compiled_language, semantics::SemanticEngine,
};

fn analyze(body: &str) -> your_language::semantics::AnalysisResult {
    let source = format!(
        r#"
import {{ error, warn, hint }} from "core/diagnostics"
import {{ get, contribute, concat }} from "core/query"
node Program = first: /[a-z]+/ "," second: /[a-z]+/ {{
    semantics {{ {body} }}
}}
entry Program
"#
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source)]),
        )
        .unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    engine.analyze(&language, "program", "first,second")
}

#[test]
fn grouped_annotations_keep_exact_ranges_labels_and_one_json_diagnostic() {
    let result =
        analyze(r#"error("Different types", [(second, "incompatible value"), (first, absent)])"#);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let diagnostic = &result.diagnostics[0];
    assert_eq!((diagnostic.primary.start, diagnostic.primary.end), (6, 12));
    assert_eq!(diagnostic.annotations.len(), 2);
    assert_eq!(
        diagnostic.annotations[0].label.as_deref(),
        Some("incompatible value")
    );
    assert_eq!(diagnostic.annotations[1].label, None);
    assert_eq!(
        (
            diagnostic.annotations[1].span.start,
            diagnostic.annotations[1].span.end
        ),
        (0, 5)
    );
    let json = serde_json::to_value(&result.diagnostics).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["annotations"][1]["label"], serde_json::Value::Null);
    let rendered = render_diagnostics(
        &result.diagnostics,
        |span| (span.file == "program").then(|| "first,second".into()),
        false,
    );
    assert_eq!(rendered.matches("Error[").count(), 1);
    assert_eq!(rendered.matches('^').count(), 11);
    assert!(rendered.contains("incompatible value"));
    assert!(!rendered.contains("language.yl"));
}

#[test]
fn warn_and_hint_have_groups_and_empty_or_absent_locations_dead_end() {
    let result = analyze(
        r#"warn("warning", [(first, absent)]) hint("hint", [(second, "look here")]) error("empty", []) error("absent", [(absent, absent)])"#,
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
    assert!(result
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.annotations.len() == 1));
}

#[test]
fn malformed_annotation_is_a_structured_error() {
    for annotation in [
        "[first]",
        "[(first, true)]",
        "[(first, absent, absent)]",
        "[(true, absent)]",
    ] {
        let result = analyze(&format!("error(\"bad\", {annotation})"));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "query.diagnostic_target"),
            "{:?}",
            result.diagnostics
        );
    }
}

#[test]
fn scattered_evidence_renders_one_group_across_files_and_old_json_stays_stable() {
    use your_language::diagnostic::{Diagnostic, DiagnosticAnnotation, Span};
    let mut diagnostic = Diagnostic::error(
        "types.mismatch",
        "number is not assignable to string",
        Span::new("value", 12, 17),
    );
    let old_json = serde_json::to_value(&diagnostic).unwrap();
    assert!(old_json.get("annotations").is_none());
    assert_eq!(
        serde_json::from_value::<Diagnostic>(old_json).unwrap(),
        diagnostic
    );
    diagnostic.annotations = Box::new(vec![
        DiagnosticAnnotation {
            span: Span::new("value", 12, 17),
            label: Some("number originates here".into()),
        },
        DiagnosticAnnotation {
            span: Span::new("assignment", 24, 31),
            label: Some("checked here".into()),
        },
    ]);
    let sources = BTreeMap::from([
        ("value", "let count = 12345;"),
        ("assignment", "let result: Container = profile;"),
    ]);
    let output = render_diagnostics(
        &[diagnostic],
        |span| sources.get(span.file.as_str()).map(|text| (*text).into()),
        false,
    );
    assert_eq!(output.matches("Error[").count(), 1);
    assert!(output.contains("--> value:1"));
    assert!(output.contains("--> assignment:1"));
    assert!(output.contains("number originates here"));
    assert!(output.contains("checked here"));
}

#[test]
fn computed_messages_and_pending_labels_settle_before_emission() {
    let result = analyze(
        r#"
        label := get((self, "label"))
        error(concat(["Computed ", label]), [(second, label)])
        contribute((self, "label"), "message")
    "#,
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].message, "Computed message");
    assert_eq!(
        result.diagnostics[0].annotations[0].label.as_deref(),
        Some("message")
    );
}

#[test]
fn annotation_count_is_bounded_before_editor_amplification() {
    let locations = vec!["(first, absent)"; 129].join(",");
    let result = analyze(&format!("error(\"large\", [{locations}])"));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "query.resource"),
        "{:?}",
        result.diagnostics
    );
}
