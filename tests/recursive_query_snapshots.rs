use std::collections::BTreeMap;
use your_language::{frontend::parse_yl, load_compiled_language, semantics::SemanticEngine};

fn snapshot(name: &str, value: &impl serde::Serialize) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let actual = serde_json::to_string_pretty(value).unwrap() + "\n";
    if std::env::var_os("UPDATE_RECURSIVE_QUERY_GOLDENS").is_some() {
        std::fs::write(&path, &actual).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path)
            .unwrap()
            .replace("\r\n", "\n"),
        actual,
        "{name}"
    );
}

#[test]
fn recursive_value_match_ast_machine_and_result_have_stable_snapshots() {
    let source = include_str!("fixtures/recursive-query.yl");
    snapshot(
        "recursive-query.yl-ast.json",
        &parse_yl("recursive.yl", source).unwrap(),
    );
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "recursive.yl",
            &BTreeMap::from([("recursive.yl".into(), source.into())]),
        )
        .unwrap();
    snapshot("recursive-query.normalized.json", &language);
    let bytes = language.to_bytes().unwrap();
    let reloaded = load_compiled_language(&bytes).unwrap();
    assert_eq!(bytes, reloaded.to_bytes().unwrap());
    let result = engine.analyze(&reloaded, "program", "demo");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    snapshot("recursive-query.ast.json", &result.ast);
    snapshot("recursive-query.graph.json", &result.graph);
}
