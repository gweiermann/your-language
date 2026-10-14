use std::collections::BTreeMap;
use your_language::{load_compiled_language, semantics::SemanticEngine};

fn compile(source: &str) -> your_language::CompiledLanguage {
    SemanticEngine::default()
        .compile(
            "review.yl",
            &BTreeMap::from([("review.yl".into(), source.into())]),
        )
        .unwrap()
}
fn analyze(source: &str, input: &str) -> your_language::semantics::AnalysisResult {
    let engine = SemanticEngine::default();
    let language = compile(source);
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "input", input);
    assert_eq!(result, engine.analyze(&language, "input", input));
    result
}
#[test]
fn authored_named_group_precedence_controls_reference_visibility() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"

import {createLexicalScope, declareName, connectReference, referenceOf} from "std/references"
import {equal} from "core/query"
node Declare = "d" name:/[a-z]/ ";" {
 semantics {
  group Late { when false { error("late") } }
  group Register { declareName(self, name) }
 }
}
node Use = "u" name:/[a-z]/ ";" { semantics { connectReference(self, name) } }
node Root = usage:Use declaration:Declare {
 semantics {
  createLexicalScope(self)
  precedence { Declare::semantics::Register > Use > Declare::semantics::Late }
  when !equal(referenceOf(usage), referenceOf(declaration)) { error("Wrong identity") }
 }
}
entry Root
"#;
    let result = analyze(source, "ux;dx;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
#[test]
fn outer_whole_region_deferral_and_inner_boundary_preserve_order() {
    let source = r#"
import {createLexicalScope, declareName, connectReference} from "std/references"
node Declare = "d" name:/[a-z]/ ";" { semantics {declareName(self, name)} }
node Use = "u" name:/[a-z]/ ";" { semantics {connectReference(self, name)} }
node Block = "{" (Use | Declare)* "}" { semantics {boundary createLexicalScope(self)} }
node Root = (Declare | Use | Block)* { semantics {boundary createLexicalScope(self) precedence { Declare > Block }} }
entry Root
"#;
    let result = analyze(source, "{ux;}dx;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let result = analyze(source, "{uy;dy;}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.message == "Unknown name 'y'"),
        "{:?}",
        result.diagnostics
    );
}
#[test]
fn imported_symbols_never_replace_caller_captures_inside_nested_operations() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"

import {scope, declarations, createLexicalScope, declareName, connectReference, referenceOf} from "std/references"
import {type, annotateWithType, typeOf} from "std/types"
import {equal} from "core/query"
symbol Red
operation helper(type, scope, declarations) {
 annotateWithType(type, Red)
 when !equal(typeOf(type), Red) { error("Type lost") }
 when !equal(referenceOf(scope), referenceOf(declarations)) {error("Capture lost")}
}
node Declare = "d" name:/[a-z]/ ";" { semantics { declareName(self, name) } }
node Use = "u" name:/[a-z]/ ";" { semantics { connectReference(self, name) } }
node Root = scope:Declare declarations:Use type:/[a-z]/ {semantics {createLexicalScope(self) helper(type,scope,declarations)}}
entry Root
"#;
    let result = analyze(source, "dx;ux;z");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
#[test]
fn malformed_query_scheduling_placeholders_are_rejected_before_execution() {
    let language = compile(
        r#"import {annotateWithType} from "std/types" symbol Red node Root="x" {semantics {group Register {annotateWithType(self,Red)}}} entry Root"#,
    );
    let wire: serde_json::Value = serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    for mutation in [0, 1, 2] {
        let mut value = wire.clone();
        let owner = "review.yl#Root";
        match mutation {
            0 => value["semantics"][owner]["groups"][0]["name"] = serde_json::json!("Wrong"),
            1 => value["semantics"][owner]["groups"] = serde_json::json!([]),
            _ => value["queries"]["groups"][owner][0]["span"]["start"] = serde_json::json!(999),
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        let errors = load_compiled_language(&bytes).unwrap_err();
        assert!(
            errors.iter().any(|diag| diag.code == "yl.artifact"),
            "{errors:?}"
        );
    }
}
