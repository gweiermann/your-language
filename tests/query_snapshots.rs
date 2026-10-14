use std::collections::BTreeMap;
use your_language::{frontend::parse_yl, load_compiled_language, semantics::SemanticEngine};

fn snapshot(name: &str, value: &impl serde::Serialize) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("tests/golden").join(name);
    let actual = serde_json::to_string_pretty(value).unwrap() + "\n";
    if std::env::var_os("UPDATE_QUERY_GOLDENS").is_some() {
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
fn declarative_ast_normalization_and_graph_have_stable_public_snapshots() {
    let source = include_str!("fixtures/declarative-query.yl");
    snapshot("query.yl-ast.json", &parse_yl("golden.yl", source).unwrap());
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "golden.yl",
            &BTreeMap::from([("golden.yl".into(), source.into())]),
        )
        .unwrap();
    snapshot("query.normalized.json", &language);
    let bytes = language.to_bytes().unwrap();
    let reloaded = load_compiled_language(&bytes).unwrap();
    assert_eq!(bytes, reloaded.to_bytes().unwrap());
    let result = engine.analyze(&reloaded, "program", "demo");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    snapshot("query.graph.json", &result.graph);
    snapshot("query.ast.json", &result.ast);
}
