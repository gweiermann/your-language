use std::collections::BTreeMap;
use your_language::semantics::{queries::QueryKey, SemanticEngine, SemanticValue};
fn analyze(body: &str) -> your_language::semantics::AnalysisResult {
    let source = format!(
        r#"import * as types from "std/types"
import {{ contribute, equal, isPresent, get }} from "std/query"
symbol Red
symbol Blue
symbol color
symbol other
node Root = "x" {{ meanings {{ {body} }} }} entry Root"#
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source)]),
        )
        .unwrap();
    engine.analyze(
        &engine.load(&language.to_bytes().unwrap()).unwrap(),
        "input",
        "x",
    )
}
#[test]
fn authored_type_exports_keep_concrete_types_language_owned() {
    let engine = SemanticEngine::default();
    for export in [
        "unknown",
        "type",
        "annotate",
        "restrictTo",
        "typeof",
        "candidates",
        "resolveWith",
    ] {
        let source =
            format!(r#"import {{ {export} }} from "std/types" node Root = "x" entry Root"#);
        engine
            .compile(
                "language.yl",
                &BTreeMap::from([("language.yl".into(), source)]),
            )
            .unwrap();
    }
    for removed in [
        "number",
        "string",
        "boolean",
        "functionType",
        "callType",
        "ensure",
        "configureTypes",
        "template",
        "instantiate",
    ] {
        let source =
            format!(r#"import {{ {removed} }} from "std/types" node Root = "x" entry Root"#);
        assert!(
            engine
                .compile(
                    "language.yl",
                    &BTreeMap::from([("language.yl".into(), source)])
                )
                .is_err(),
            "{removed}"
        );
    }
}
#[test]
fn nested_tuple_slots_are_independent() {
    let result = analyze(
        r#"
 types::annotate((self, color), Red)
 types::annotate((self, other), Blue)
 types::annotate(((self, color), other), Blue)
 if !equal(types::typeof((self,color)), Red) { error("Wrong color") }
 if !equal(types::typeof((self,other)), Blue) { error("Wrong other") }
 if !equal(types::typeof(((self,color),other)), Blue) { error("Wrong nested slot") }
 if !equal(types::typeof(self), types::unknown) { error("Unexpected direct type") }
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
#[test]
fn restriction_checks_evidence_without_annotating_missing_evidence() {
    let result = analyze(
        r#"
 types::restrictTo(self, Red)
 if !equal(types::typeof(self), types::unknown) { error("Restriction became evidence") }
 contribute("observed", types::candidates(self))
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.key == QueryKey::Text("observed".into())
            && entry.value == SemanticValue::List(vec![])));
}
#[test]
fn restrictions_target_the_supplied_syntax_span() {
    let result = analyze("types::annotate(self, Blue) types::restrictTo(self, Red)");
    let error = result
        .diagnostics
        .iter()
        .find(|error| error.message.contains("does not match"))
        .unwrap();
    assert_eq!(error.primary.file, "input");
    assert_eq!(error.primary.start, 0);
    assert_eq!(error.primary.end, 1);
}
#[test]
fn invalid_scalar_keys_are_structured_diagnostics() {
    let result = analyze("types::annotate([self], Red)");
    assert!(result
        .diagnostics
        .iter()
        .any(|error| error.code.starts_with("query.")));
}
