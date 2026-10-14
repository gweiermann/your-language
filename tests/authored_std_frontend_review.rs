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
    let source = r#"
import {lexicalScope, declare, use, referenceOf} from "std/references"
import {equal} from "std/query"
node Declare = "d" name:/[a-z]/ ";" {
 meanings {
  group Late { if false { error("late") } }
  group Register { declare(name) }
 }
}
node Use = "u" name:/[a-z]/ ";" { meanings { use(name) } }
node Root = usage:Use declaration:Declare {
 meanings {
  lexicalScope()
  precedence { Declare::meanings::Register > Use > Declare::meanings::Late }
  if !equal(referenceOf(usage), referenceOf(declaration)) { error("Wrong identity") }
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
import {lexicalScope, declare, use} from "std/references"
node Declare = "d" name:/[a-z]/ ";" { meanings {declare(name)} }
node Use = "u" name:/[a-z]/ ";" { meanings {use(name)} }
node Block = "{" (Use | Declare)* "}" { meanings {boundary lexicalScope()} }
node Root = (Declare | Use | Block)* { meanings {boundary lexicalScope() precedence { Declare > Block }} }
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
    let source = r#"
import {scope, declarations, lexicalScope, declare, use, referenceOf} from "std/references"
import {type, annotate, typeof} from "std/types"
import {equal} from "std/query"
symbol Red
operation helper(type, scope, declarations) {
 annotate(type, Red)
 if !equal(typeof(type), Red) { error("Type lost") }
 if !equal(referenceOf(scope), referenceOf(declarations)) {error("Capture lost")}
}
node Declare = "d" name:/[a-z]/ ";" { meanings { declare(name) } }
node Use = "u" name:/[a-z]/ ";" { meanings { use(name) } }
node Root = scope:Declare declarations:Use type:/[a-z]/ {meanings {lexicalScope() helper(type,scope,declarations)}}
entry Root
"#;
    let result = analyze(source, "dx;ux;z");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
#[test]
fn malformed_query_scheduling_placeholders_are_rejected_before_execution() {
    let language = compile(
        r#"import {annotate} from "std/types" symbol Red node Root="x" {meanings {group Register {annotate(self,Red)}}} entry Root"#,
    );
    let wire: serde_json::Value = serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    for mutation in [0, 1, 2] {
        let mut value = wire.clone();
        let owner = "review.yl#Root";
        match mutation {
            0 => value["meanings"][owner]["groups"][0]["name"] = serde_json::json!("Wrong"),
            1 => value["meanings"][owner]["groups"] = serde_json::json!([]),
            _ => value["query_meanings"][owner][0]["span"]["start"] = serde_json::json!(999),
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        let errors = load_compiled_language(&bytes).unwrap_err();
        assert!(
            errors.iter().any(|diag| diag.code == "yl.artifact"),
            "{errors:?}"
        );
    }
}
