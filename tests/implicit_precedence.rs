use std::collections::BTreeMap;
use your_language::{
    compile_sources, load_compiled_language, parse, AstNode, AstValue, CompiledLanguage,
};

fn compile(source: &str) -> CompiledLanguage {
    compile_sources(
        "test.yl",
        &BTreeMap::from([("test.yl".into(), source.into())]),
    )
    .unwrap()
}

fn node(value: &AstValue) -> &AstNode {
    let AstValue::Node(node) = value else {
        panic!("Expected node")
    };
    node
}

#[test]
fn implicit_infix_matches_explicit_left_and_survives_reload() {
    let source = r#"node E {
        node N = value: /[0-9]+/
        node Name = value: /[a-z]+/
        node Group = "(" value: E ")"
        node Add = left: E "+" right: E
        PRECEDENCE
    } entry E"#;
    let implicit = compile(&source.replace("PRECEDENCE", ""));
    let explicit = compile(&source.replace("PRECEDENCE", "precedence { Add }"));
    let reloaded = load_compiled_language(&implicit.to_bytes().unwrap()).unwrap();
    let implicit_ir = serde_json::to_value(&implicit).unwrap();
    let explicit_ir = serde_json::to_value(&explicit).unwrap();
    assert_eq!(
        implicit_ir["rules"]["test.yl#E"]["body"],
        explicit_ir["rules"]["test.yl#E"]["body"]
    );
    for input in ["1+2+3", "a+b+c", "(1+2)+3", "1+(2+3)"] {
        let actual = parse(&implicit, input);
        assert!(!actual.has_errors(), "{actual:?}");
        assert_eq!(actual.ast, parse(&explicit, input).ast);
        assert_eq!(actual.ast, parse(&reloaded, input).ast);
    }
    let ast = parse(&implicit, "1+2+3").ast.unwrap();
    assert_eq!(node(&ast.fields["left"]).kind, "E::Add");
}

#[test]
fn implicit_prefix_and_postfix_chain() {
    for (grammar, input, field) in [
        (r#""!" operand: E"#, "!!!x", "operand"),
        (r#"operand: E "!""#, "x!!!", "operand"),
    ] {
        let language = compile(&format!(
            "node E {{ node N = /x/ node Op = {grammar} }} entry E"
        ));
        let result = parse(&language, input);
        assert!(!result.has_errors(), "{result:?}");
        let ast = result.ast.unwrap();
        assert_eq!(node(&ast.fields[field]).kind, "E::Op");
    }
}

#[test]
fn token_enclosed_recursion_is_not_an_operator() {
    let language = compile(r#"node E { node N = /x/ node Group = "(" inner: E ")" } entry E"#);
    assert!(!parse(&language, "((x))").has_errors());
}

#[test]
fn explicit_right_and_nonassoc_remain_authoritative() {
    let source =
        r#"node E { node N = /x/ node Op = left: E "+" right: E precedence { ASSOC Op } } entry E"#;
    let right = compile(&source.replace("ASSOC", "right"));
    let ast = parse(&right, "x+x+x").ast.unwrap();
    assert_eq!(node(&ast.fields["right"]).kind, "E::Op");
    let nonassoc = compile(&source.replace("ASSOC", "nonassoc"));
    assert!(!parse(&nonassoc, "x+x").has_errors());
    assert!(parse(&nonassoc, "x+x+x").has_errors());
}

#[test]
fn multiple_operators_need_explicit_precedence() {
    let source = r#"node E { node N = /x/ node Add = E "+" E node Mul = E "*" E } entry E"#;
    let errors = compile_sources(
        "test.yl",
        &BTreeMap::from([("test.yl".into(), source.into())]),
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "yl.missing_precedence");
    assert_eq!(errors[0].secondary.len(), 2);
    assert!(errors[0]
        .help
        .as_ref()
        .unwrap()
        .contains("precedence block"));
}

#[test]
fn imported_member_alias_is_classified_by_canonical_rule() {
    let language = compile_sources("test.yl", &BTreeMap::from([
        ("test.yl".into(), r#"import { Add as Plus } from "ops" export node E { node N = /x/ node Plus } entry E"#.into()),
        ("ops.yl".into(), r#"import { E } from "test" export node Add = left: E "+" right: E"#.into()),
    ])).unwrap();
    assert!(!parse(&language, "x+x+x").has_errors());
}

#[test]
fn nested_abstract_members_can_supply_the_single_operator() {
    let language = compile(
        r#"node E { node Atoms { node N = /x/ } node Ops { node Add = left: E "+" right: E } } entry E"#,
    );
    let result = parse(&language, "x+x+x");
    assert!(!result.has_errors(), "{result:?}");
    assert_eq!(result.ast.unwrap().kind, "E::Ops::Add");
}
