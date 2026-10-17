use std::collections::BTreeMap;
use your_language::{
    compile_sources, load_compiled_language, parse_named, semantics::SemanticEngine,
};

fn language() -> your_language::CompiledLanguage {
    compile_sources(
        "language.yl",
        &BTreeMap::from([(
            "language.yl".into(),
            r#"
        import { contribute, get, equal } from "core/query"
        import { error } from "core/diagnostics"
        symbol tag
        node Program = name:/[a-z]+/ {
            semantics {
                value := get((self, tag))
                contribute((self, tag), name)
                when equal(value, "bad") { error("Rejected name", name) }
            }
        }
        entry Program
    "#
            .into(),
        )]),
    )
    .unwrap()
}
#[test]
fn image_is_indexed_self_contained_and_deterministic() {
    let compiled = language();
    let bytes = compiled.to_bytes().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["version"], 9);
    assert!(json.get("rules").is_none());
    assert!(json.get("query_meanings").is_none());
    assert!(json["parser"]["rules"].is_array());
    assert!(json["parser"]["instructions"].is_array());
    assert!(!json["queries"]["expressions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(String::from_utf8(bytes.clone())
        .unwrap()
        .contains("\"Contribute\""));
    let loaded = load_compiled_language(&bytes).unwrap();
    assert_eq!(bytes, loaded.to_bytes().unwrap());
    assert_eq!(bytes, language().to_bytes().unwrap());
    let engine = SemanticEngine::default();
    for source in ["hello", "bad", "?", ""] {
        assert_eq!(
            parse_named(&compiled, "program", source),
            parse_named(&loaded, "program", source)
        );
        assert_eq!(
            engine.analyze(&compiled, "program", source),
            engine.analyze(&loaded, "program", source)
        );
    }
}
#[test]
fn image_rejects_unknown_capabilities_indices_and_opcodes() {
    let original: serde_json::Value =
        serde_json::from_slice(&language().to_bytes().unwrap()).unwrap();
    for mutation in 0..6 {
        let mut json = original.clone();
        match mutation {
            0 => json["capabilities"][0] = "unsupported-engine-feature".into(),
            1 => json["parser"]["entry"] = u32::MAX.into(),
            2 => json["parser"]["instructions"][0] = serde_json::json!({"NotAhead":0}),
            3 => json["queries"]["expressions"][0] = serde_json::json!({"Not":0}),
            4 => {
                json["queries"]["actions"][0] = serde_json::json!({"Call":{"expression":u32::MAX,"span":{"file":"bad","start":0,"end":0}}})
            }
            _ => {
                json["queries"]["expressions"][0] =
                    serde_json::json!({"Call":{"opcode":"UnknownInstruction","args":[]}})
            }
        }
        let bytes = serde_json::to_vec(&json).unwrap();
        let loaded = std::panic::catch_unwind(|| load_compiled_language(&bytes));
        assert!(loaded.is_ok(), "mutation {mutation} panicked");
        assert!(loaded.unwrap().is_err(), "mutation {mutation} was accepted");
    }
}
#[test]
fn retired_formats_require_recompilation() {
    let original: serde_json::Value =
        serde_json::from_slice(&language().to_bytes().unwrap()).unwrap();
    for version in [1, 2, 3, 4, 5, 6, 7, 8, 10] {
        let mut value = original.clone();
        value["version"] = version.into();
        let errors = load_compiled_language(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert_eq!(errors[0].code, "yl.artifact");
        assert!(errors[0].message.contains("recompile"));
    }
}
