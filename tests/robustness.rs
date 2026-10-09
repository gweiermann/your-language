use std::collections::BTreeMap;
use your_language::{compile_sources, frontend::parse_yl, load_compiled_language, parse};

#[test]
fn malformed_frontend_input_never_panics() {
    // Deterministic corpus of token fragments, Unicode, incomplete strings/regexes,
    // declarations and deeply nested groups. This supplements focused error fixtures.
    let fragments = [
        "node ", "P ", "= ", "( ", ") ", "{ ", "} ", "\"x\" ", "/x/ ", "| ", "|> ", "? ", "* ",
        "entry ", "import ", "when ", ". ", ":: ", "; ", "é ", "\" ", "/ ", "\\ ",
    ];
    let mut seed = 17u64;
    for _ in 0..1000 {
        let mut source = String::new();
        for _ in 0..20 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            source.push_str(fragments[(seed as usize) % fragments.len()]);
        }
        assert!(
            std::panic::catch_unwind(|| parse_yl("fuzz.yl", &source)).is_ok(),
            "{source:?}"
        );
    }
    let source = format!(
        "node P = {} /x/ {} entry P",
        "(".repeat(150),
        ")".repeat(150)
    );
    assert_eq!(
        parse_yl("deep.yl", &source).unwrap_err()[0].code,
        "yl.depth_limit"
    );
    let source = format!(
        "node P = /x/ {{ constraints {{ {} error(\"x\") {} }} }} entry P",
        "when true { ".repeat(150),
        "} ".repeat(150)
    );
    assert_eq!(
        parse_yl("deep.yl", &source).unwrap_err()[0].code,
        "yl.depth_limit"
    );
}
#[test]
fn corrupted_artifacts_are_rejected_before_execution() {
    let language = compile_sources(
        "a.yl",
        &BTreeMap::from([("a.yl".into(), "node P = value: /x/ entry P".into())]),
    )
    .unwrap();
    let bytes = language.to_bytes().unwrap();
    for offset in (0..bytes.len()).step_by(7) {
        assert!(load_compiled_language(&bytes[..offset]).is_err());
    }
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["rules"]["a.yl#P"]["body"]["Concrete"] = serde_json::json!({"Ref":"missing"});
    assert!(load_compiled_language(&serde_json::to_vec(&json).unwrap()).is_err());
    json["rules"]["a.yl#P"]["body"]["Concrete"] = serde_json::json!({"Regex":"["});
    assert!(load_compiled_language(&serde_json::to_vec(&json).unwrap()).is_err());
}
#[test]
fn malformed_target_input_and_recursive_artifacts_do_not_panic() {
    let language = compile_sources(
        "a.yl",
        &BTreeMap::from([("a.yl".into(), "node P = value: /[a-z]+/ entry P".into())]),
    )
    .unwrap();
    for text in ["", "\0", "é", "😃", "a\0", "\r\n", "aé"] {
        assert!(std::panic::catch_unwind(|| parse(&language, text)).is_ok());
    }
    let language = compile_sources(
        "a.yl",
        &BTreeMap::from([("a.yl".into(), "node P = P entry P".into())]),
    )
    .unwrap();
    assert!(parse(&language, "").has_errors());
}

#[test]
fn invalid_precedence_plans_are_rejected_and_never_overflow() {
    let language=compile_sources("a.yl",&BTreeMap::from([("a.yl".into(),"node E { node N = value: /[0-9]+/ node Sum = left: E \"+\" right: E precedence { Sum } } entry E".into())])).unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    for power in [0, usize::MAX, 1000] {
        let mut corrupt = original.clone();
        corrupt["rules"]["a.yl#E"]["body"]["Abstract"]["operators"][0]["binding_power"] =
            serde_json::json!(power);
        let bytes = serde_json::to_vec(&corrupt).unwrap();
        assert!(
            load_compiled_language(&bytes).is_err(),
            "Corrupt binding power {power} was accepted"
        );
        // Even callers deserializing the public v0 IR directly cannot trigger arithmetic panics.
        let unchecked: your_language::CompiledLanguage = serde_json::from_slice(&bytes).unwrap();
        assert!(std::panic::catch_unwind(|| parse(&unchecked, "1+2")).is_ok());
    }
    for field in ["left", "right"] {
        let mut corrupt = original.clone();
        corrupt["rules"]["a.yl#E"]["body"]["Abstract"]["operators"][0][field] =
            serde_json::json!(false);
        assert!(load_compiled_language(&serde_json::to_vec(&corrupt).unwrap()).is_err());
    }
}
