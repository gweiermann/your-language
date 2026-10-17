use std::path::PathBuf;
use your_language::{compile_language, load_compiled_language, semantics::SemanticEngine};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/11-compound-types")
}

#[test]
fn compound_types_do_not_require_source_tracking() {
    let language = compile_language(root().join("definition/language.yl")).unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let engine = SemanticEngine::default();
    let source = std::fs::read_to_string(root().join("programs/valid.txt")).unwrap();
    let result = engine.analyze(&language, "valid.txt", &source);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert!(!result
        .graph
        .query_entries
        .iter()
        .any(|entry| format!("{:?}", entry.key).contains("typeSources")));
    for entry in std::fs::read_dir(root().join("programs")).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("invalid-")
        {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let result = engine.analyze(&language, &path.to_string_lossy(), &source);
        assert!(
            !result.diagnostics.is_empty(),
            "{} unexpectedly passed",
            path.display()
        );
    }
}

#[test]
fn inferred_types_and_declared_write_contracts_remain_separate() {
    let language = compile_language(root().join("definition/language.yl")).unwrap();
    let engine = SemanticEngine::default();
    for source in [
        "let value: number | string = 1; value = \"ok\";",
        "interface User { name: number | string; } let user: User = { name: 1 }; user.name = \"ok\";",
        "interface User { scores: (number | string)[]; } let user: User = { scores: [1, \"old\"] }; user.scores[0] = \"ok\";",
    ] {
        let result = engine.analyze(&language, "assignment", source);
        assert!(result.diagnostics.is_empty(), "{source}: {:#?}", result.diagnostics);
    }
    let source = "let value: number | string = 1; value = true;";
    let result = engine.analyze(&language, "assignment", source);
    assert_eq!(result.diagnostics.len(), 1, "{:#?}", result.diagnostics);
    let primary = &result.diagnostics[0].primary;
    assert_eq!(&source[primary.start..primary.end], "true");
}
