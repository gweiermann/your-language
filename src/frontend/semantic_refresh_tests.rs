use super::*;

#[test]
fn semantic_boolean_operators_have_standard_precedence() {
    let source = "operation q(a, b, c) = !a && b || c";
    let module = parse_yl("boolean.yl", source).unwrap();
    let DeclKind::Operation {
        body: OperationBody::Query(value),
        ..
    } = &module.declarations[0].kind
    else {
        panic!("Expected a query");
    };
    let ExprKind::Binary(or, left, right) = &value.kind else {
        panic!("Expected disjunction");
    };
    assert_eq!(or, "||");
    assert!(matches!(&right.kind, ExprKind::Ref(name) if name == "c"));
    let ExprKind::Binary(and, left, right) = &left.kind else {
        panic!("Expected conjunction");
    };
    assert_eq!(and, "&&");
    assert!(matches!(left.kind, ExprKind::Not(_)));
    assert!(matches!(&right.kind, ExprKind::Ref(name) if name == "b"));
}

#[test]
fn semantic_boolean_operator_chains_obey_ast_depth_limits() {
    let source = format!("operation q() = {}", vec!["true"; 140].join(" && "));
    assert!(parse_yl("deep.yl", &source)
        .unwrap_err()
        .iter()
        .any(|error| error.code == "yl.depth_limit"));
    assert!(parse_yl("bad.yl", "operation q() = true &&").is_err());
}

#[test]
fn expression_matches_keep_alternatives_values_and_spans() {
    let source = r#"operation compare(left, right) = match (left, right) {
        Type::number, Type::number | Type::string, Type::string => true,
        true, false => match (right) { _ => [left, right] }[0],
        _ => false
    }"#;
    let module = parse_yl("match.yl", source).unwrap();
    let DeclKind::Operation {
        body: OperationBody::Query(value),
        ..
    } = &module.declarations[0].kind
    else {
        panic!("Expected a query operation");
    };
    let ExprKind::Match { inputs, cases } = &value.kind else {
        panic!("Expected a value match");
    };
    assert_eq!(inputs.len(), 2);
    assert_eq!(cases[0].alternatives.len(), 2);
    assert_eq!(cases[2].alternatives, vec![vec![None, None]]);
    assert!(matches!(cases[1].value.kind, ExprKind::Index(_, 0)));
    assert_eq!(
        &source[cases[2].span.start..cases[2].span.end],
        "_ => false"
    );
}

#[test]
fn malformed_expression_matches_are_diagnostics() {
    for source in [
        "operation q() = match () { _ => true }",
        "operation q() = match (true) { _ => }",
        "operation q() = match (true) { true => true false => false }",
        "operation q() = match (true) { _ => { q() } }",
    ] {
        assert!(parse_yl("bad.yl", source).is_err(), "{source}");
    }
}

#[test]
fn bindings_returns_and_reactive_guards_preserve_source_spans() {
    let source = r#"operation connect(target, name) {
ref := lookup(name)
when isPresent(ref) { attach(target, ref) }
return ref
}
node Reference = name: /[a-z]+/ {
semantics {
    ref := connect(self, name)
    annotateWithType(self, typeOf(ref))
}
checks { when name.matches(/x/) { error("Invalid name") } }
}
entry Reference"#;
    let module = parse_yl("refresh.yl", source).unwrap();
    let DeclKind::Operation {
        body: OperationBody::Conditional(actions),
        ..
    } = &module.declarations[0].kind
    else {
        panic!("Expected an operation with declarative actions");
    };
    let MeaningSyntaxAction::Binding { name, span, .. } = &actions[0] else {
        panic!("Expected an immutable binding");
    };
    assert_eq!(name, "ref");
    assert_eq!(&source[span.start..span.end], "ref := lookup(name)");
    assert!(matches!(actions[1], MeaningSyntaxAction::If { .. }));
    assert!(matches!(actions[2], MeaningSyntaxAction::Return { .. }));
    let DeclKind::Node {
        meanings: Some(semantics),
        constraints,
        ..
    } = &module.declarations[1].kind
    else {
        panic!("Expected semantics and syntax checks");
    };
    assert!(matches!(
        semantics.groups[0].actions[0],
        MeaningSyntaxAction::Binding { .. }
    ));
    assert_eq!(constraints.len(), 1);
}

#[test]
fn obsolete_block_and_condition_spellings_are_rejected() {
    for source in [
        "node A = \"a\" { meanings { action() } }",
        "node A = \"a\" { constraints { error(\"bad\") } }",
        "operation action() { if true { action() } }",
        "operation action() { self := lookup() }",
        "operation action() { value := }",
    ] {
        assert!(parse_yl("bad.yl", source).is_err(), "{source}");
    }
}
