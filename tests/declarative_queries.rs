use std::collections::BTreeMap;
use your_language::{
    load_compiled_language,
    semantics::{queries::QueryKey, SemanticEngine, SemanticIdentity, SemanticValue},
};

const LANGUAGE: &str = r#"import { error, warn, hint } from "core/diagnostics"

import { get, getAll, contribute, ancestors, find, text, isPresent, equal, hasMultiple } from "core/query"
symbol scope
symbol declarations
symbol reference

operation createLexicalScope(target) {
    contribute((target, scope), target)
}

operation nearestScope(value) = find(ancestors(value), ancestor => get((ancestor, scope)))

operation resolve(name) = find(ancestors(name), ancestor =>
    get((get((ancestor, scope)), declarations, text(name)))
)

operation declareName(target, name) {
    contribute((nearestScope(target), declarations, text(name)), target)
    contribute((name, reference), target)
    when hasMultiple(getAll((nearestScope(target), declarations, text(name)))) {
        error("Duplicate declaration ${name}")
    }
}

operation connectReference(target, name) {
    contribute((name, reference), resolve(name))
    when !isPresent(resolve(name)) {
        error("Unknown name ${name}")
    }
}

node Name = value: /[a-z]+/

node Statement {
    node Declare = "let" name: Name ";" {
        semantics { declareName(self, name) }
    }
    node Use = name: Name ";" {
        semantics { connectReference(self, name) }
    }
    node Block = "{" statements: Statement* "}" {
        semantics { createLexicalScope(self) }
    }
}

node Program = statements: Statement* {
    semantics { createLexicalScope(self) }
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
        import { contribute, get } from "core/query"
        symbol slot
        node Program = "x" {
            semantics {
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

#[test]
fn conditional_producers_are_discovered_before_guards_or_match_inputs_settle() {
    let grammar = r#"import { error, warn, hint } from "core/diagnostics"

        import { contribute, get, equal } from "core/query"
        symbol selection
        symbol result
        symbol selected
        node Program = "x" {
            semantics {
                when equal(get((self, selection)), true) {
                    contribute((self, result), "enabled")
                } else {
                    contribute((self, result), "disabled")
                }
                match (get((self, result))) {
                    "enabled" => contribute((self, selected), "yes"),
                    _ => error("Selected the wrong branch")
                }
                contribute((self, selection), true)
            }
        }
        entry Program
    "#;
    let result = analyze(grammar, "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == SemanticValue::Text("yes".into())));
    assert!(result
        .graph
        .query_dependencies
        .iter()
        .any(|dependency| dependency.state == "absent"));
}

#[test]
fn hypothetical_overrides_only_supplied_keys_and_recomputes_conditional_relationships() {
    let grammar = r#"import { error, warn, hint } from "core/diagnostics"

        import { contribute, get, equal, hypothetical } from "core/query"
        symbol parameter
        symbol outside
        symbol result
        symbol trial
        node Program = "x" {
            semantics {
                contribute((self, parameter), "number")
                contribute((self, outside), "number")
                when equal(get((self, parameter)), "string") {
                    contribute((self, result), "string")
                    warn("A hypothetical diagnostic must not leak")
                } else {
                    contribute((self, result), get((self, outside)))
                }
                contribute((self, trial), hypothetical((self, result), [((self, parameter), "string")]))
            }
        }
        entry Program
    "#;
    let result = analyze(grammar, "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let read = |name: &str| {
        result.graph.query_entries.iter().find(|entry|matches!(&entry.key,QueryKey::Tuple(parts) if matches!(&parts[1], QueryKey::Symbol(symbol) if symbol.ends_with(&format!("#{name}"))))).unwrap().value.clone()
    };
    assert_eq!(read("parameter"), SemanticValue::Text("number".into()));
    assert_eq!(read("outside"), SemanticValue::Text("number".into()));
    assert_eq!(read("result"), SemanticValue::Text("number".into()));
    assert_eq!(read("trial"), SemanticValue::Text("string".into()));
}

#[test]
fn positive_fact_cycles_reach_a_fixed_point_without_final_value_deadlock() {
    let grammar = r#"
        import { contribute, contributeAll, facts, getAll } from "core/query"
        symbol left
        symbol right
        symbol result
        node Program = "x" {
            semantics {
                contributeAll((self, left), facts((self, right)))
                contributeAll((self, right), facts((self, left)))
                contribute((self, left), "seed")
                contribute((self, result), getAll((self, right)))
            }
        }
        entry Program
    "#;
    let result = analyze(grammar, "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == SemanticValue::List(vec![SemanticValue::Text("seed".into())])));
}

#[test]
fn absence_dependent_cycles_diagnose_while_independent_footprints_still_settle() {
    let grammar = r#"
        import { contribute, get, isPresent } from "core/query"
        symbol cyclic
        symbol good
        node Program = "x" {
            semantics {
                when isPresent(get((self, cyclic))) {
                    contribute((self, cyclic), "cycle")
                }
                contribute((self, good), "ready")
            }
        }
        entry Program
    "#;
    let result = analyze(grammar, "x");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "query.cycle");
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == SemanticValue::Text("ready".into())));
}

#[test]
fn terminal_captures_are_direct_keys_and_can_traverse_their_parents() {
    let grammar = r#"
        import { contribute, ancestors } from "core/query"
        symbol parents
        node Program = token: /[a-z]+/ {
            semantics { contribute((token, parents), ancestors(token)) }
        }
        entry Program
    "#;
    let result = analyze(grammar, "hello");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::List(vec![SemanticValue::Identity(SemanticIdentity::Occurrence(
            0
        ))])
    );
}

#[test]
fn corrupted_query_artifacts_are_rejected_before_runtime_indexing() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), LANGUAGE.into())]),
        )
        .unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    json["queries"]["expressions"][0] =
        serde_json::json!({"Call":{"opcode":"Contribute","args":[]}});
    assert!(load_compiled_language(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn compound_data_and_authored_field_compatibility_use_generic_collection_queries() {
    let grammar = r#"import { error, warn, hint } from "core/diagnostics"

        import { contribute, get, keys, lookup, isSubset, every, contains, equal } from "core/query"
        symbol number
        symbol string
        symbol fields
        symbol selected
        operation compatible(actual, expected) =
            every(expected, field => contains(actual, field))
        node Program = "point" {
            semantics {
                contribute((self, fields), [("x", number), ("y", number)])
                when !isSubset(["x"], keys(get((self, fields)))) {
                    error("Missing required x field")
                }
                when !compatible(keys(get((self, fields))), ["x", "y"]) {
                    error("Incompatible fields")
                }
                contribute((self, selected), lookup(get((self, fields)), "x"))
                when !equal(lookup(get((self, fields)), "missing"), absent) {
                    error("Missing field should be absent")
                }
            }
        }
        entry Program
    "#;
    let result = analyze(grammar, "point");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.value == SemanticValue::Symbol("language.yl#number".into())));
}

#[test]
fn repeated_captures_are_direct_lists_and_preserve_terminal_match_spans() {
    let grammar = r#"
        import { contribute, map, text } from "core/query"
        symbol namesSlot
        node Program = names: /[a-z]+/* {
            semantics { contribute((self, namesSlot), map(names, name => text(name))) }
        }
        trivia Spaces = /\s+/
        trivia Comment = /\/\/[^\n]*/
        entry Program { trivia Spaces, Comment }
    "#;
    let result = analyze(grammar, "one // two inside trivia\ntwo");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::List(vec![
            SemanticValue::Text("one".into()),
            SemanticValue::Text("two".into())
        ])
    );
    assert_eq!(
        result.occurrences[0].captures["names"].element_spans[1].start,
        25
    );
}

#[test]
fn conditional_operations_compose_through_ordinary_authored_operation_wrappers() {
    let grammar = r#"
        import { contribute } from "core/query"
        operation inner(value) {
            when true { contribute("result", value) }
        }
        operation outer(value) { inner(value) }
        node Program = "x" { semantics { outer("works") } }
        entry Program
    "#;
    let result = analyze(grammar, "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.query_entries[0].value,
        SemanticValue::Text("works".into())
    );
}
