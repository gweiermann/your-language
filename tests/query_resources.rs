use std::collections::BTreeMap;
use your_language::semantics::{queries::QueryKey, SemanticEngine, SemanticValue};

fn language(grammar: &str) -> (SemanticEngine, your_language::CompiledLanguage) {
    let engine = SemanticEngine::default();
    let compiled = engine
        .compile(
            "limit.yl",
            &BTreeMap::from([("limit.yl".into(), grammar.into())]),
        )
        .unwrap();
    let reloaded = engine.load(&compiled.to_bytes().unwrap()).unwrap();
    (engine, reloaded)
}

#[test]
fn recursively_constructed_facts_are_bounded_before_exponential_growth() {
    let (engine, language) = language(
        r#"import { contribute, contributeAll, map, facts } from "std/query"
node Program = "x" { meanings {
    contribute("k", "seed")
    contributeAll("k", map(facts("k"), item => (item, item)))
    contribute("independent", true)
} }
entry Program"#,
    );
    let result = engine.analyze(&language, "source", "x");
    assert!(
        result.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.code.as_str(),
            "query.resource" | "query.nonconvergent"
        )),
        "{:?}",
        result.diagnostics
    );
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.key == QueryKey::Text("independent".into())
            && entry.value == SemanticValue::Bool(true)));
}

#[test]
fn callback_builders_reject_oversized_results_before_collecting_them() {
    let grammar = format!(
        r#"import {{ contribute, map }} from "std/query"
node Program = tokens: /x/* {{ meanings {{ contribute("large", map(tokens, token => "{}")) }} }}
entry Program"#,
        "z".repeat(1024)
    );
    let (engine, language) = language(&grammar);
    let result = engine.analyze(&language, "source", &"x".repeat(1500));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "query.resource"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn excessive_wire_depth_and_trailing_json_are_structured_artifact_errors() {
    let engine = SemanticEngine::default();
    let bytes = format!("{}null{}", "[".repeat(513), "]".repeat(513));
    let errors = engine.load(bytes.as_bytes()).unwrap_err();
    assert!(errors[0].message.contains("nesting limit"));
    let (_, language) = language("node Program = \"x\" entry Program");
    let mut bytes = language.to_bytes().unwrap();
    bytes.extend_from_slice(b" true");
    assert_eq!(engine.load(&bytes).unwrap_err()[0].code, "yl.artifact");
}
#[test]
fn expanded_branches_share_one_key_and_diagnostic_content_budget() {
    let choices = (0..1024)
        .map(|index| format!("\"choice{index}\""))
        .collect::<Vec<_>>()
        .join(",");
    for action in [
        format!("error(\"{} ${{get(\\\"selector\\\")}}\")", "z".repeat(2048)),
        format!(
            "contribute((concat([\"{}\", get(\"selector\")]), \"branch\"), true)",
            "z".repeat(2048)
        ),
    ] {
        let grammar = format!(
            r#"import {{ contribute, alternatives, get, concat }} from "std/query"
node Program = "x" {{ meanings {{
    contribute("selector", alternatives([{}]))
    match (get("selector")) {{ _ => {} }}
    contribute("independent", true)
}} }}
entry Program"#,
            choices, action
        );
        let (engine, language) = language(&grammar);
        let result = engine.analyze(&language, "source", "x");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "query.resource"),
            "{:?}",
            result.diagnostics
        );
        assert!(result
            .graph
            .query_entries
            .iter()
            .any(|entry| entry.key == QueryKey::Text("independent".into())));
    }
}

#[test]
fn new_primitives_and_related_locations_cannot_be_mislabeled_as_old_artifacts() {
    for grammar in [
        r#"import { contribute, tail } from "std/query"
node Program = "x" { meanings { contribute("result",tail([true,false])) } } entry Program"#,
        r#"node Program = "x" { meanings { if true { error("Message", self, self) } } } entry Program"#,
        r#"import { contribute,getEarlier } from "std/query"
node Program = "x" { meanings { contribute("result",getEarlier("other")) } } entry Program"#,
    ] {
        let (engine, language) = language(grammar);
        let mut json = serde_json::to_value(language).unwrap();
        json["version"] = serde_json::json!(3);
        let errors = engine
            .load(&serde_json::to_vec(&json).unwrap())
            .unwrap_err();
        assert!(errors[0].message.contains("version 4"), "{:?}", errors);
    }
}
