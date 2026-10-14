use std::collections::BTreeMap;
use your_language::{
    load_compiled_language,
    semantics::{queries::QueryKey, SemanticEngine, SemanticIdentity, SemanticValue},
};

const LANGUAGE: &str = r#"
import { get, getAll, contribute, ancestors, find, text, isPresent, equal, hasMultiple } from "std/query"
symbol scope
symbol declarations
symbol reference

operation lexicalScope() {
    contribute((self, scope), self)
}

operation nearestScope(value) = find(ancestors(value), ancestor => get((ancestor, scope)))

operation resolve(name) = find(ancestors(self), ancestor =>
    get((get((ancestor, scope)), declarations, text(name)))
)

operation declare(name) {
    contribute((nearestScope(self), declarations, text(name)), self)
    contribute((name, reference), self)
    if hasMultiple(getAll((nearestScope(self), declarations, text(name)))) {
        error("Duplicate declaration ${name}")
    }
}

operation use(name) {
    contribute((name, reference), resolve(name))
    if !isPresent(resolve(name)) {
        error("Unknown name ${name}")
    }
}

node Name = value: /[a-z]+/

node Statement {
    node Declare = "let" name: Name ";" {
        meanings { declare(name) }
    }
    node Use = name: Name ";" {
        meanings { use(name) }
    }
    node Block = "{" statements: Statement* "}" {
        meanings { lexicalScope() }
    }
}

node Program = statements: Statement* {
    meanings { lexicalScope() }
}
trivia Whitespace = /\s+/
entry Program { trivia Whitespace }
"#;

fn analyze(grammar: &str, program: &str) -> your_language::semantics::AnalysisResult {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), grammar.into())]),
        )
        .unwrap();
    let reloaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    engine.analyze(&reloaded, "program", program)
}

#[test]
fn automatically_discovered_authored_scopes_resolve_forward_and_shadowed_names() {
    let result = analyze(LANGUAGE, "let x; { x; let x; } x;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let declarations = result
        .graph
        .query_entries
        .iter()
        .filter(|entry| matches!(&entry.key,QueryKey::Tuple(parts) if parts.len()==3))
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 2);
    let refs=result.graph.query_entries.iter().filter(|entry|matches!(&entry.key,QueryKey::Tuple(parts) if parts.len()==2 && matches!(&parts[1], QueryKey::Symbol(symbol) if symbol.ends_with("#reference")))).collect::<Vec<_>>();
    assert_eq!(refs.len(), 4);
    let outer = declarations
        .iter()
        .find(|entry| entry.span.start == 0)
        .unwrap();
    let inner = declarations
        .iter()
        .find(|entry| entry.span.start > 0)
        .unwrap();
    assert_eq!(refs[1].value, inner.value);
    assert_eq!(refs[3].value, outer.value);
    assert!(result
        .graph
        .query_dependencies
        .iter()
        .all(|dependency| dependency.state != "pending"));
}

#[test]
fn authored_missing_and_duplicate_diagnostics_are_not_native_reference_policy() {
    let missing = analyze(LANGUAGE, "absent;");
    assert_eq!(missing.diagnostics.len(), 1, "{:?}", missing.diagnostics);
    assert_eq!(missing.diagnostics[0].message, "Unknown name absent");
    let duplicate = analyze(LANGUAGE, "let x; let x;");
    assert_eq!(
        duplicate.diagnostics.len(),
        2,
        "{:?}",
        duplicate.diagnostics
    );
    assert!(duplicate
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.message == "Duplicate declaration x"));
}

#[test]
fn nested_structural_keys_keep_identity_symbol_and_text_distinct() {
    let grammar = r#"
        import { contribute, get } from "std/query"
        symbol slot
        node Program = "x" {
            meanings {
                contribute(((self, slot), "slot"), "value")
                contribute((self, "result"), get(((self, slot), "slot")))
            }
        }
        entry Program
    "#;
    let result = analyze(grammar, "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.graph.query_entries.len(), 2);
    assert!(result.graph.query_entries.iter().any(|entry| entry.key
        == QueryKey::Tuple(vec![
            QueryKey::Identity(SemanticIdentity::Occurrence(0)),
            QueryKey::Text("result".into())
        ])
        && entry.value == SemanticValue::Text("value".into())));
}
