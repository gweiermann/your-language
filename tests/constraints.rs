use std::collections::BTreeMap;
use your_language::{compile_sources, parse};

#[test]
fn reusable_regex_constraint_substitutes_receiver_and_regex_parameters() {
    let sources=BTreeMap::from([
        ("main.yl".into(),r#"import { starts } from "./checks" node P = name: /x/ { constraints { starts(name, /x/) } } entry P"#.into()),
        ("checks.yl".into(),r#"export constraint starts(input, pattern) { when input.matches(pattern) { warning("matched") } }"#.into()),
    ]);
    let language = compile_sources("main.yl", &sources).unwrap();
    let result = parse(&language, "x");
    assert!(result.ast.is_some());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].message, "matched");
}

#[test]
fn reusable_constraint_named_arguments_bind_by_name() {
    let sources = BTreeMap::from([(
        "main.yl".into(),
        r#"
        trivia W = /\s+/
        constraint tight(left, right) { when trivia.between(left, right) { error("not tight") } }
        node P = first: "a" second: "b" { constraints { tight(right=second, left=first) } }
        entry P
    "#
        .into(),
    )]);
    let language = compile_sources("main.yl", &sources).unwrap();
    assert!(!parse(&language, "ab").has_errors());
    assert!(parse(&language, "a b").has_errors());
}

#[test]
fn constraint_forwarding_defaults_grouping_and_message_arguments() {
    let sources = BTreeMap::from([(
        "main.yl".into(),
        r#"
        constraint check(value, pattern=/x/, message="matched") {
            when (value.matches(pattern)) { warning(message) }
        }
        constraint forward(input) { check(message="forwarded",value=(input)) }
        node P = name: /x/ { constraints { forward(name) } }
        entry P
    "#
        .into(),
    )]);
    let result = parse(&compile_sources("main.yl", &sources).unwrap(), "x");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].message, "forwarded");
}

#[test]
fn invalid_constraint_arguments_are_compile_diagnostics() {
    for arguments in [
        "unknown=name",
        "value=name,value=name",
        "",
        "name,name",
        "value=name,name",
    ] {
        let source = format!(
            r#"constraint c(value) {{ when value.matches(/x/) {{ warning("x") }} }} node P = name: /x/ {{ constraints {{ c({arguments}) }} }} entry P"#
        );
        let result = compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), source)]));
        assert_eq!(
            result.unwrap_err()[0].code,
            "yl.invalid_argument",
            "{arguments}"
        );
    }
}
