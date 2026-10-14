use std::collections::BTreeMap;
use your_language::semantics::{SemanticEngine, SemanticValue};

fn analyze(grammar: &str, source: &str) -> your_language::semantics::AnalysisResult {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "review.yl",
            &BTreeMap::from([("review.yl".into(), grammar.into())]),
        )
        .unwrap();
    engine.analyze(&language, "source", source)
}

#[test]
fn mixed_repeated_nodes_and_terminals_keep_individual_text() {
    let result = analyze(
        r#"import { contribute, map, text } from "core/query"
 node Name = /[a-z]+/
 node Program = values: (Name | /[0-9]+/)* { semantics { contribute("texts", map(values, item => text(item))) } }
 trivia Whitespace = /\s+/
 trivia Comment = /\/\/[^\n]*/
 entry Program { trivia Whitespace, Comment }"#,
        "foo // 12 comment\n12 bar 34",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::List(
            vec!["foo", "12", "bar", "34"]
                .into_iter()
                .map(|s| SemanticValue::Text(s.into()))
                .collect()
        )
    );
}

#[test]
fn nested_hypothetical_memo_includes_inherited_and_shadowed_overrides() {
    let result = analyze(
        r#"import { contribute, get, hypothetical, concat } from "core/query"
 node Program = "x" { semantics {
   contribute("a", "original-a")
   contribute("b", "original-b")
   contribute("return", concat([get("a"), get("b")]))
   contribute("inner", hypothetical("return", [("b", "local-b")]))
   contribute("first", hypothetical("inner", [("a", "first-a")]))
   contribute("second", hypothetical("inner", [("a", "second-a")]))
   contribute("shadow", hypothetical("inner", [("b", "outer-b")]))
 } } entry Program"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    for (key, expected) in [
        ("first", "first-alocal-b"),
        ("second", "second-alocal-b"),
        ("shadow", "original-alocal-b"),
    ] {
        let entry = result
            .graph
            .query_entries
            .iter()
            .find(|e| e.key == your_language::semantics::queries::QueryKey::Text(key.into()))
            .unwrap();
        assert_eq!(entry.value, SemanticValue::Text(expected.into()));
    }
}

#[test]
fn projected_repeated_terminal_fields_keep_individual_spans() {
    let result = analyze(
        r#"import { contribute, map, text } from "core/query"
 node Name = value: /[a-z]+/
 node Program = names: Name* { semantics { contribute("texts", map(names.value, item => text(item))) } }
 trivia Whitespace = /\s+/
 trivia Comment = /\/\/[^\n]*/
 entry Program { trivia Whitespace, Comment }"#,
        "one // two\ntwo",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::List(vec![
            SemanticValue::Text("one".into()),
            SemanticValue::Text("two".into())
        ])
    );
}

#[test]
fn facts_deduplicate_capture_and_explicit_identity_of_same_occurrence() {
    let result = analyze(
        r#"import { contribute, facts } from "core/query"
 node Name = /[a-z]+/ { semantics { contribute("fact", self) } }
 node Program = name: Name { semantics {
    contribute("fact", name)
    contribute("result", facts("fact"))
 } } entry Program"#,
        "name",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let entry = result
        .graph
        .query_entries
        .iter()
        .find(|e| e.key == your_language::semantics::queries::QueryKey::Text("result".into()))
        .unwrap();
    let SemanticValue::List(values) = &entry.value else {
        panic!("expected facts list")
    };
    assert_eq!(values.len(), 1, "{:?}", values);
}

#[test]
fn nested_repeated_projections_keep_empty_groups_and_terminal_text() {
    let result = analyze(
        r#"import { contribute, map, text } from "core/query"
 node Name = value: /[a-z]+/
 node Group = "{" names: Name* "}"
 node Program = groups: Group* { semantics {
   contribute("texts", map(groups.names.value, group => map(group, item => text(item))))
 } }
 trivia Whitespace = /\s+/
 trivia Comment = /\/\/[^\n]*/
 entry Program { trivia Whitespace, Comment }"#,
        "{one // two\ntwo} {} {three}",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::List(vec![
            SemanticValue::List(vec![
                SemanticValue::Text("one".into()),
                SemanticValue::Text("two".into())
            ]),
            SemanticValue::List(vec![]),
            SemanticValue::List(vec![SemanticValue::Text("three".into())]),
        ])
    );
}

#[test]
fn mixed_repeated_pipe_projections_keep_each_original_text() {
    let result = analyze(
        r#"import { contribute, map, text } from "core/query"
 import { separatedBy } from "std/parser"
 node Name = /[a-z]+/
 node Program = values: (Name | /[0-9]+/)* |> separatedBy(",") {
   semantics { contribute("texts", map(values, item => text(item))) }
 }
 trivia Whitespace = /\s+/
 trivia Comment = /\/\/[^\n]*/
 entry Program { trivia Whitespace, Comment }"#,
        "foo, // 12 comment\n12, bar, 34",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::List(
            vec!["foo", "12", "bar", "34"]
                .into_iter()
                .map(|s| SemanticValue::Text(s.into()))
                .collect()
        )
    );
}
#[test]
fn nested_terminal_lists_skip_empty_group_span_slots() {
    let result = analyze(
        r#"import { contribute, map, text } from "core/query"
node Group = "{" names: /[a-z]+/* "}"
node Program = groups: Group* { semantics {
    contribute("texts", map(groups.names, group => map(group, item => text(item))))
} }
trivia Whitespace = /\s+/
entry Program { trivia Whitespace }"#,
        "{one} {} {three}",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::List(vec![
            SemanticValue::List(vec![SemanticValue::Text("one".into())]),
            SemanticValue::List(vec![]),
            SemanticValue::List(vec![SemanticValue::Text("three".into())]),
        ])
    );
}
