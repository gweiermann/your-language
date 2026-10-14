use std::collections::BTreeMap;
use your_language::semantics::{queries::QueryKey, SemanticEngine, SemanticValue};
fn analyze(body: &str) -> your_language::semantics::AnalysisResult {
    let source = format!(
        r#"import * as types from "std/types"
import {{ contribute, equal, contains, isPresent, alternatives }} from "std/query"
symbol Red
symbol Blue
symbol left
symbol right
node Root = "x" {{ meanings {{ {body} }} }} entry Root"#
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source)]),
        )
        .unwrap();
    let result = engine.analyze(&language, "input", "x");
    assert_eq!(
        result,
        engine.analyze(
            &engine.load(&language.to_bytes().unwrap()).unwrap(),
            "input",
            "x"
        )
    );
    result
}
fn clean(body: &str) -> your_language::semantics::AnalysisResult {
    let result = analyze(body);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    result
}
fn observed(result: &your_language::semantics::AnalysisResult) -> &SemanticValue {
    &result
        .graph
        .query_entries
        .iter()
        .find(|entry| entry.key == QueryKey::Text("observed".into()))
        .unwrap()
        .value
}
#[test]
fn repeated_annotations_accumulate_distinct_candidates_idempotently() {
    let result = clean(
        r#"types::annotate(self, Red) types::annotate(self, Blue) types::annotate(self, Red)
 contribute("observed", types::candidates(self))"#,
    );
    let SemanticValue::List(values) = observed(&result) else {
        panic!("Expected candidate list")
    };
    assert_eq!(values.len(), 2);
    assert!(values.contains(&SemanticValue::Symbol("language.yl#Red".into())));
    assert!(values.contains(&SemanticValue::Symbol("language.yl#Blue".into())));
}
#[test]
fn unknown_is_absence_of_evidence_and_does_not_mask_known_mismatch() {
    let result = analyze("types::annotate(self, types::unknown) types::annotate(self, Blue) types::restrictTo(self, Red)");
    assert!(result
        .diagnostics
        .iter()
        .any(|error| error.message.contains("Blue") && error.message.contains("Red")));
}
#[test]
fn match_distributes_over_union_without_losing_existing_candidates() {
    let result = clean(
        r#"types::annotate((self,left), Red) types::annotate((self,left), Blue)
 match (types::typeof((self,left))) {
  Red => types::annotate(self, Red),
  Blue => types::annotate(self, Blue),
  _ => error("Unknown candidate")
 }
 contribute("observed", types::candidates(self))"#,
    );
    let SemanticValue::List(values) = observed(&result) else {
        panic!("Expected list")
    };
    assert_eq!(values.len(), 2);
}
#[test]
fn forward_fact_transfer_reaches_fixed_point_without_constraining_source() {
    clean(
        r#"types::annotate((self,right), types::typeof((self,left)))
 types::annotate((self,left), Red)
 types::annotate((self,right), Blue)
 if !equal(types::typeof((self,left)), Red) { error("Transfer changed source") }
 if !contains(types::candidates((self,right)), Red) { error("Missing transferred type") }
 if !contains(types::candidates((self,right)), Blue) { error("Missing own type") }"#,
    );
}
#[test]
fn evidence_free_read_cycles_settle_unknown() {
    clean(
        r#"types::annotate((self,left), types::typeof((self,right)))
 types::annotate((self,right), types::typeof((self,left)))
 if !equal(types::typeof((self,left)), types::unknown) { error("Cycle invented evidence") }"#,
    );
}
#[test]
fn seeded_read_cycles_propagate_the_seed() {
    clean(
        r#"types::annotate((self,left), types::typeof((self,right)))
 types::annotate((self,right), types::typeof((self,left)))
 types::annotate((self,left), Red)
 if !equal(types::typeof((self,right)), Red) { error("Seed did not propagate") }"#,
    );
}
#[test]
fn union_restrictions_require_every_known_member() {
    let result = analyze(
        "types::annotate(self, Red) types::annotate(self, Blue) types::restrictTo(self, Red)",
    );
    assert!(result
        .diagnostics
        .iter()
        .any(|error| error.message.contains("does not match")));
    clean("types::annotate(self, Red) types::annotate(self, Blue) types::restrictTo(self, alternatives([Red, Blue]))");
}
#[test]
fn missing_evidence_allows_a_positive_unknown_branch_to_publish_a_fact() {
    clean(
        r#"match (types::typeof((self,left))) {
 types::unknown => types::annotate(self, Red),
 _ => {}
 }
 if !equal(types::typeof(self), Red) { error("Branch was lost") }"#,
    );
}
#[test]
fn type_queries_need_no_references_or_host_registration() {
    let result = clean("types::annotate(self, Red)");
    assert!(!result.graph.query_entries.is_empty());
    assert!(result.graph.records.is_empty());
}
