use your_language::{
    frontend::parse_yl,
    syntax::{DeclKind, ExprKind},
};

#[test]
fn checked_in_definitions_parse() {
    for (file, source) in [
        (
            "lexical.yl",
            include_str!("../documentation/target-syntax/lexical.yl"),
        ),
        (
            "minijs.yl",
            include_str!("../documentation/target-syntax/minijs.yl"),
        ),
    ] {
        parse_yl(file, source).unwrap();
    }
}
fn grammar(source: &str) -> ExprKind {
    let module = parse_yl("test", &format!("pattern P = {source}")).unwrap();
    match module.declarations.into_iter().next().unwrap().kind {
        DeclKind::Pattern { grammar, .. } => grammar.kind,
        _ => panic!(),
    }
}
#[test]
fn operators_bind_as_specified() {
    assert!(
        matches!(grammar("A | B |> f()"), ExprKind::Choice(ref v) if matches!(v[1].kind,ExprKind::Pipe(..)))
    );
    assert!(
        matches!(grammar("A B |> f()"), ExprKind::Sequence(ref v) if matches!(v[1].kind,ExprKind::Pipe(..)))
    );
    assert!(
        matches!(grammar("x: A |> f()"), ExprKind::Capture(_,ref v) if matches!(v.kind,ExprKind::Pipe(..)))
    );
    assert!(
        matches!(grammar("A* |> f()"), ExprKind::Pipe(ref v,_,_) if matches!(v.kind,ExprKind::Repeat(..)))
    );
    assert!(
        matches!(grammar("A |> f() |> g()"), ExprKind::Pipe(ref v,_,_) if matches!(v.kind,ExprKind::Pipe(..)))
    );
    assert!(
        matches!(grammar("(A B) |> f()"), ExprKind::Pipe(ref v,_,_) if matches!(v.kind,ExprKind::Group(..)))
    );
}
#[test]
fn contextual_keywords_and_formatting() {
    let a = parse_yl(
        "test",
        "node Example = node: Name node Name = value: /x/ entry Example",
    )
    .unwrap();
    let b = parse_yl(
        "test",
        "node Example =\n node: Name\nnode Name =\nvalue: /x/\nentry Example",
    )
    .unwrap();
    assert_eq!(a.declarations.len(), b.declarations.len());
    assert!(
        matches!(a.declarations[0].kind,DeclKind::Node { grammar:Some(ref g),.. } if matches!(g.kind,ExprKind::Capture(..)))
    );
}
#[test]
fn errors_and_unicode_spans() {
    let error = parse_yl("unicode", "node A = \"é\" ;").unwrap_err();
    assert_eq!(error[0].primary.start, 14);
    for source in [
        "node",
        "node A =",
        "node A = \"x",
        "when x {}",
        "node A = /x",
        "enum E {",
        "node A = (((",
    ] {
        assert!(parse_yl("test", source).is_err());
    }
}
