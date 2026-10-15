//! Hypothetical resolution replaces native template ownership and instance allocation.
use std::collections::BTreeMap;
use your_language::semantics::{queries::QueryKey, SemanticEngine, SemanticValue};
fn language() -> (SemanticEngine, your_language::CompiledLanguage) {
    let source = r#"
import * as types from "std/types"
import { contribute, equal } from "core/query"
symbol number
symbol string
symbol parameter
symbol result
symbol external
node Root = "x" { semantics {
 types::annotateWithType((self,parameter), string)
 types::annotateWithType((self,external), number)
 types::annotateWithType((self,result), types::typeOf((self,parameter)))
 contribute("first", types::resolveTypeWith((self,result), [((self,parameter), number)]))
 contribute("second", types::resolveTypeWith((self,result), [((self,parameter), string)]))
 contribute("original", types::typeOf((self,parameter)))
 contribute("outside", types::resolveTypeWith((self,external), [((self,parameter), string)]))
} }
entry Root
"#;
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source.into())]),
        )
        .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    (engine, loaded)
}
fn observed(result: &your_language::semantics::AnalysisResult, name: &str) -> SemanticValue {
    result
        .graph
        .query_entries
        .iter()
        .find(|entry| entry.key == QueryKey::Text(name.into()))
        .unwrap()
        .value
        .clone()
}
#[test]
fn explicit_parameter_assumptions_recompute_result_and_keep_instances_independent() {
    let (engine, language) = language();
    let result = engine.analyze(&language, "input", "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        observed(&result, "first"),
        SemanticValue::Symbol("language.yl#number".into())
    );
    assert_eq!(
        observed(&result, "second"),
        SemanticValue::Symbol("language.yl#string".into())
    );
    assert_eq!(
        observed(&result, "original"),
        SemanticValue::Symbol("language.yl#string".into())
    );
}
#[test]
fn hypothetical_parameter_overrides_preserve_external_capture() {
    let (engine, language) = language();
    let result = engine.analyze(&language, "input", "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        observed(&result, "outside"),
        SemanticValue::Symbol("language.yl#number".into())
    );
}
#[test]
fn hypothetical_queries_are_analysis_local_and_parallel_safe() {
    let (engine, language) = language();
    let expected = engine.analyze(&language, "input", "x");
    std::thread::scope(|scope| {
        let calls: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| engine.analyze(&language, "input", "x")))
            .collect();
        for call in calls {
            assert_eq!(call.join().unwrap(), expected);
        }
    });
}
#[test]
fn canonical_std_contains_no_native_template_ownership_contracts() {
    let (_, language) = language();
    let normalized = String::from_utf8(language.to_bytes().unwrap()).unwrap();
    assert!(!normalized.contains("register_template"));
    assert!(!normalized.contains("std/types#instantiate"));
    assert!(normalized.contains("\"HypotheticalChecked\""));
}

#[test]
fn authored_hypothetical_example_cli_preserves_human_and_json_underlines() {
    use std::process::Command;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let artifact = root
        .join("target")
        .join(format!("query-type-example-{}.ylc", std::process::id()));
    let cli = env!("CARGO_BIN_EXE_yl");
    let compile = Command::new(cli)
        .arg("compile")
        .arg(root.join("examples/type_instantiation/definition/main.yl"))
        .arg("-o")
        .arg(&artifact)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let program = root.join("examples/type_instantiation/programs/program.txt");
    let success = Command::new(cli)
        .arg("language")
        .arg(&artifact)
        .arg("check")
        .arg(&program)
        .output()
        .unwrap();
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    let errors = root.join("examples/type_instantiation/programs/errors.txt");
    let human = Command::new(cli)
        .arg("language")
        .arg(&artifact)
        .arg("check")
        .arg(&errors)
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();
    assert_eq!(human.status.code(), Some(1));
    let rendered = String::from_utf8(human.stderr).unwrap();
    assert!(rendered.contains("\x1b[31m"));
    assert!(rendered.contains("^^^^^^^"));
    let json = Command::new(cli)
        .arg("language")
        .arg(&artifact)
        .arg("check")
        .arg(&errors)
        .arg("--json")
        .output()
        .unwrap();
    std::fs::remove_file(&artifact).unwrap();
    assert_eq!(json.status.code(), Some(1));
    assert!(json.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let diagnostic = &json[0];
    let primary = &diagnostic["primary"];
    let source = std::fs::read_to_string(errors).unwrap();
    assert_eq!(
        &source[primary["start"].as_u64().unwrap() as usize
            ..primary["end"].as_u64().unwrap() as usize],
        "\"wrong\""
    );
}
