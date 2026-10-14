use std::collections::BTreeMap;
use your_language::semantics::{queries::QueryKey, AnalysisResult, SemanticEngine, SemanticValue};

fn analyze(grammar: &str, source: &str) -> AnalysisResult {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "review.yl",
            &BTreeMap::from([("review.yl".into(), grammar.into())]),
        )
        .unwrap();
    engine.analyze(
        &engine.load(&language.to_bytes().unwrap()).unwrap(),
        "program",
        source,
    )
}

#[test]
fn inner_assumption_without_origin_does_not_reuse_outer_origin() {
    let result = analyze(
        r#"import { contribute, get, equal, hypotheticalChecked } from "std/query"
 node Program = a: /a/ b: /b/ { meanings {
   contribute("parameter", false)
   contribute("result", get("parameter"))
   if equal(get("parameter"), true) { error("bad", b) }
   contribute("inner", hypotheticalChecked("result", [("parameter", true, absent)]))
   contribute("outer", hypotheticalChecked("inner", [("parameter", false, a)]))
 } } entry Program"#,
        "ab",
    );
    assert!(!result.diagnostics.is_empty());
    assert!(
        result.diagnostics.iter().all(|d| d.primary.end != 1),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn selected_alternative_branch_preserves_its_matched_value() {
    let result = analyze(
        r#"import * as types from "std/types"
 symbol Red
 symbol Blue
 symbol result
 node Program = "x" { meanings {
   types::annotate(self, Red)
   types::annotate(self, Blue)
   match (types::typeof(self)) {
      Red => types::annotate((self, result), types::typeof(self)),
      _ => {}
   }
 } } entry Program"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let values=result.graph.query_entries.iter().filter(|e|matches!(&e.key,QueryKey::Tuple(parts) if matches!(&parts[0],QueryKey::Tuple(inner) if matches!(&inner[1],QueryKey::Symbol(s) if s.ends_with("#result"))))).map(|e|e.value.clone()).collect::<Vec<_>>();
    assert_eq!(values, vec![SemanticValue::Symbol("review.yl#Red".into())]);
}

fn matched(body: &str) -> AnalysisResult {
    analyze(
        &format!(
            r#"import * as types from "std/types"
 import {{ contribute, equal }} from "std/query"
 symbol Red
 symbol Blue
 symbol left
 symbol right
 node Program = "x" {{ meanings {{
   types::annotate((self,left), Red)
   types::annotate((self,left), Blue)
   types::annotate((self,right), Red)
   types::annotate((self,right), Blue)
   {body}
 }} }} entry Program"#
        ),
        "x",
    )
}

#[test]
fn repeated_match_input_is_correlated_instead_of_cartesian_with_itself() {
    let result = matched(
        r#"match (types::typeof((self,left)), types::typeof((self,left))) {
       Red, Red | Blue, Blue => contribute("pairs", (types::typeof((self,left)), types::typeof((self,left)))),
       _ => error("Impossible mixed pair")
    }"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let pairs = result
        .graph
        .query_entries
        .iter()
        .filter(|entry| entry.key == QueryKey::Text("pairs".into()))
        .collect::<Vec<_>>();
    assert_eq!(pairs.len(), 2);
    for entry in pairs {
        let SemanticValue::Tuple(items) = &entry.value else {
            panic!("expected pair")
        };
        assert_eq!(items[0], items[1]);
    }
}

#[test]
fn independent_inputs_produce_every_correlated_pair_from_one_case() {
    let result = matched(
        r#"match (types::typeof((self,left)), types::typeof((self,right))) {
       _, _ => contribute("pairs", (types::typeof((self,left)), types::typeof((self,right))))
    }"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let pairs = result
        .graph
        .query_entries
        .iter()
        .filter(|entry| entry.key == QueryKey::Text("pairs".into()))
        .collect::<Vec<_>>();
    assert_eq!(pairs.len(), 4);
    assert!(pairs.iter().all(|entry|matches!(&entry.value, SemanticValue::Tuple(items) if items.iter().all(|v|matches!(v,SemanticValue::Symbol(_))))));
}

#[test]
fn nested_match_and_if_keep_outer_case_selection() {
    let result = matched(
        r#"match (types::typeof((self,left))) {
       Red => match (types::typeof((self,left))) {
         Red => if equal(types::typeof((self,left)), Red) { contribute("selected", types::typeof((self,left))) },
         _ => error("Lost outer branch")
       },
       _ => {}
    }"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let values = result
        .graph
        .query_entries
        .iter()
        .filter(|entry| entry.key == QueryKey::Text("selected".into()))
        .map(|entry| entry.value.clone())
        .collect::<Vec<_>>();
    assert_eq!(values, vec![SemanticValue::Symbol("review.yl#Red".into())]);
}

#[test]
fn branch_local_values_can_address_distinct_keys() {
    let result = matched(
        r#"match (types::typeof((self,left))) {
      _ => contribute(("selected",types::typeof((self,left))), types::typeof((self,left)))
    }"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let pairs=result.graph.query_entries.iter().filter(|entry|matches!(&entry.key,QueryKey::Tuple(items) if items[0] == QueryKey::Text("selected".into()))).collect::<Vec<_>>();
    assert_eq!(pairs.len(), 2);
    for entry in pairs {
        let QueryKey::Tuple(parts) = &entry.key else {
            unreachable!()
        };
        let QueryKey::Symbol(key) = &parts[1] else {
            panic!("expected symbol")
        };
        assert_eq!(entry.value, SemanticValue::Symbol(key.clone()));
    }
}

#[test]
fn branch_local_diagnostics_do_not_report_unmatched_candidates() {
    let result = matched(
        r#"match (types::typeof((self,left))) {
      Red => {},
      _ => error("Bad type ${types::typeof((self,left))}")
    }"#,
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].message.contains("Blue"));
    assert!(!result.diagnostics[0].message.contains("Red"));
}
