use std::{collections::BTreeMap, path::Path};
use your_language::{
    semantics::{queries::QueryKey, SemanticEngine},
    CompiledLanguage,
};

fn language() -> CompiledLanguage {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/07-declarative-queries/definition");
    let sources = [
        "language.yl",
        "lexical.yl",
        "statements.yl",
        "references.yl",
    ]
    .into_iter()
    .map(|name| {
        (
            name.to_owned(),
            std::fs::read_to_string(root.join(name)).unwrap(),
        )
    })
    .collect::<BTreeMap<_, _>>();
    SemanticEngine::default()
        .compile("language.yl", &sources)
        .unwrap()
}

#[test]
fn local_reference_helpers_settle_nearest_scope_and_keep_name_spans_after_reload() {
    let engine = SemanticEngine::default();
    let language = language();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    for language in [&language, &loaded] {
        // The inner declaration wins even though it occurs after the reference.
        let source = "let shared; { shared; let shared; } shared;";
        let result = engine.analyze(language, "source", source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let declarations = result
            .graph
            .query_entries
            .iter()
            .filter(|entry| matches!(&entry.key, QueryKey::Tuple(parts) if parts.len() == 3))
            .collect::<Vec<_>>();
        assert_eq!(declarations.len(), 2);
        let outer = declarations
            .iter()
            .find(|entry| entry.span.start == 0)
            .unwrap();
        let inner = declarations
            .iter()
            .find(|entry| entry.span.start > 0)
            .unwrap();
        let reference = |start| {
            result.graph.query_entries.iter().find(|entry| {
            entry.span.start == start
                && matches!(&entry.key, QueryKey::Tuple(parts) if parts.len() == 2
                    && matches!(&parts[1], QueryKey::Symbol(symbol) if symbol.ends_with("#reference")))
        }).unwrap()
        };
        assert_eq!(
            reference(source.find("{ shared").unwrap() + 2).value,
            inner.value
        );
        assert_eq!(
            reference(source.rfind("shared").unwrap()).value,
            outer.value
        );
        let source = "{ let hidden; } hidden;";
        let result = engine.analyze(language, "source", source);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = &result.diagnostics[0];
        assert_eq!(diagnostic.message, "Unknown name 'hidden'");
        assert_eq!(diagnostic.primary.start, source.rfind("hidden").unwrap());
        assert_eq!(
            &source[diagnostic.primary.start..diagnostic.primary.end],
            "hidden"
        );

        let source = "let repeated; let repeated;";
        let result = engine.analyze(language, "source", source);
        assert_eq!(result.diagnostics.len(), 2);
        for diagnostic in &result.diagnostics {
            assert_eq!(diagnostic.message, "Duplicate declaration 'repeated'");
            assert_eq!(
                &source[diagnostic.primary.start..diagnostic.primary.end],
                "repeated"
            );
        }
    }
}
