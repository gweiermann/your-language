use super::*;
#[test]
fn expression_callbacks_and_nested_conditional_actions_roundtrip() {
    let source = r#"import { get, find, ancestors, contribute, isPresent } from "core/query"
export symbol scope
operation resolve(target, name) = find(ancestors(target), ancestor => get((get((ancestor, scope)), name)))
operation declare(target, name) {
when isPresent(name) {
    match (name) {
        true => { when true { contribute((target, scope), name) } },
        _ => warn("Missing ${name}")
    }
} else { error("No name") }
}
node Program = name: /[a-z]+/ { semantics { when !isPresent(resolve(self, name)) { declare(self, name) } } }
entry Program
"#;
    let parsed = parse_yl("query.yl", source).unwrap();
    assert!(parsed.declarations.iter().any(|decl| matches!(
        &decl.kind,
        DeclKind::Operation {
            body: OperationBody::Conditional(_),
            ..
        }
    )));
    let formatted = crate::format_yl("query.yl", source).unwrap();
    assert_eq!(crate::format_yl("query.yl", &formatted).unwrap(), formatted);
    let reparsed = parse_yl("query.yl", &formatted).unwrap();
    assert_eq!(parsed.declarations.len(), reparsed.declarations.len());
    assert!(formatted.contains("ancestor =>"));
}
#[test]
fn malformed_callbacks_and_conditionals_are_diagnostics() {
    for source in [
        "operation q() = find([], item =>)",
        "operation q() { when true missing() }",
        "operation q() { when true { q() } else q() }",
    ] {
        assert!(parse_yl("bad.yl", source).is_err(), "{source}");
    }
}
