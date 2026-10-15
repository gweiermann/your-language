use std::collections::BTreeMap;
use your_language::{
    format_yl,
    frontend::parse_yl,
    semantics::{queries::QueryKey, AnalysisResult, SemanticEngine, SemanticValue},
    syntax::{DeclKind, ExprKind, OperationBody},
};

fn analyze(grammar: &str, input: &str) -> AnalysisResult {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), grammar.into())]),
        )
        .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "input", input);
    assert_eq!(result, engine.analyze(&language, "input", input));
    result
}

fn value(result: &AnalysisResult, name: &str) -> Option<SemanticValue> {
    result
        .graph
        .query_entries
        .iter()
        .find(|entry| entry.key == QueryKey::Text(name.into()))
        .map(|entry| entry.value.clone())
}

#[test]
fn positional_access_composes_with_operations_bindings_callbacks_and_reload() {
    let result = analyze(
        r#"
import { contribute, map } from "core/query"
operation second(pair) = pair[1]
node Root = "x" {
    semantics {
        pair := ("first", ("Ada", "Lovelace"))
        contribute("result", second(pair)[0])
        contribute("callback", map([pair], item => item[1][1]))
        contribute("literal", ["left", "right"][0])
    }
}
entry Root
"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        value(&result, "result"),
        Some(SemanticValue::Text("Ada".into()))
    );
    assert_eq!(
        value(&result, "callback"),
        Some(SemanticValue::List(vec![SemanticValue::Text(
            "Lovelace".into()
        )]))
    );
    assert_eq!(
        value(&result, "literal"),
        Some(SemanticValue::Text("left".into()))
    );
}

#[test]
fn capture_elements_keep_their_identity_and_diagnostic_source() {
    let result = analyze(
        r#"
import { contribute, equal, text } from "core/query"
import { error } from "core/diagnostics"
node Name = /[a-z]+/
node Root = names: Name+ {
    semantics {
        contribute("first", text(names[0]))
        when equal(text(names[1]), "wrong") {
            error("Unexpected ${text(names[1])}", names[1])
        }
    }
}
trivia Space = /\s+/
entry Root { trivia Space }
"#,
        "right wrong",
    );
    assert_eq!(
        value(&result, "first"),
        Some(SemanticValue::Text("right".into()))
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].message, "Unexpected wrong");
    assert_eq!(
        (
            result.diagnostics[0].primary.start,
            result.diagnostics[0].primary.end
        ),
        (6, 11)
    );
}

#[test]
fn pending_receivers_settle_and_missing_positions_are_absent() {
    let result = analyze(
        r#"
import { contribute, get, isPresent } from "core/query"
import { error } from "core/diagnostics"
node Root = "x" {
    semantics {
        contribute("copy", get("later")[1][0])
        contribute("later", ("first", ["ready"]))
        contribute("empty", [][0])
        contribute("missing", ("only", "other")[9])
        contribute("absent", absent[0])
        contribute("slot", (absent, "kept")[1])
        when isPresent([][0]) { error("Unexpected element") }
    }
}
entry Root
"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        value(&result, "copy"),
        Some(SemanticValue::Text("ready".into()))
    );
    assert_eq!(
        value(&result, "slot"),
        Some(SemanticValue::Text("kept".into()))
    );
    for key in ["empty", "missing", "absent"] {
        assert!(value(&result, key).is_none());
    }
}

#[test]
fn non_collection_receiver_reports_a_structured_diagnostic() {
    let result = analyze(
        r#"
import { contribute } from "core/query"
node Root = "x" { semantics { contribute("result", "text"[0]) } }
entry Root
"#,
        "x",
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].code.starts_with("query."));
    assert!(value(&result, "result").is_none());
}

#[test]
fn invalid_indexes_are_definition_errors_and_long_chains_are_bounded() {
    for index in [
        "",
        "-1",
        "1.5",
        "name",
        "4294967296",
        "99999999999999999999999999",
    ] {
        let source = format!("operation position(value) = value[{index}]");
        assert!(parse_yl("index.yl", &source).is_err(), "{source}");
    }
    let source = format!("operation position(value) = value{}", "[0]".repeat(130));
    let diagnostics = parse_yl("index.yl", &source).unwrap_err();
    assert_eq!(diagnostics[0].code, "yl.depth_limit");
    let mut grouped = "value".to_owned();
    for _ in 0..60 {
        grouped = format!("({grouped}){}", "[0]".repeat(60));
    }
    let source = format!("operation position(value) = {grouped}");
    let diagnostics = parse_yl("index.yl", &source).unwrap_err();
    assert_eq!(diagnostics[0].code, "yl.depth_limit");
    assert!(parse_yl("index.yl", "operation position(value) = value[4294967295]").is_ok());
}

#[test]
fn indexing_is_postfix_and_preserves_complete_source_spans() {
    let source = "operation selected(value) = !value[1][0]";
    let module = parse_yl("index.yl", source).unwrap();
    let DeclKind::Operation {
        body: OperationBody::Query(expr),
        ..
    } = &module.declarations[0].kind
    else {
        panic!("Expected operation");
    };
    let ExprKind::Not(inner) = &expr.kind else {
        panic!("Expected negation");
    };
    let ExprKind::Index(first, 0) = &inner.kind else {
        panic!("Expected outer index");
    };
    assert!(matches!(first.kind, ExprKind::Index(_, 1)));
    assert_eq!(&source[inner.span.start..inner.span.end], "value[1][0]");
}

#[test]
fn formatter_keeps_indexes_attached_and_list_literals_separate() {
    let source = "operation selected(value) = value [ 1 ] [0]\noperation values(value) = [value [0], value [1]]\n";
    let formatted = format_yl("index.yl", source).unwrap();
    assert!(formatted.contains("value[1][0]"), "{formatted}");
    assert!(formatted.contains("= [value[0], value[1]]"), "{formatted}");
    assert_eq!(formatted, format_yl("index.yl", &formatted).unwrap());
    let commented = format_yl(
        "index.yl",
        "operation selected(value) = value[/* index */1]",
    )
    .unwrap();
    assert!(commented.contains("value["), "{commented}");
    assert_eq!(commented, format_yl("index.yl", &commented).unwrap());
}
