use std::collections::BTreeMap;
use your_language::{compile_sources, parse, parse_named, AstNode, AstValue, CompiledLanguage};
fn compile(source: &str) -> CompiledLanguage {
    compile_sources(
        "test.yl",
        &BTreeMap::from([("test.yl".into(), source.into())]),
    )
    .unwrap()
}
fn node(value: &AstValue) -> &AstNode {
    let AstValue::Node(n) = value else {
        panic!("Expected node: {value:?}")
    };
    n
}
fn expression() -> CompiledLanguage {
    compile(
        r#"
trivia W = /\s+/
node Name = value: /[a-z]+/
node E {
 node Number = value: /\d+/
 node NameExpression = name: Name
 node Group = "(" value: E ")"
 node Member = object: E "." property: Name
 node Call = callee: E "(" arguments: E* ")"
 node Unary = operator: "!" argument: E
 node Power = left: E operator: "**" right: E
 node Product = left: E operator: "*" right: E
 node Sum = left: E operator: "+" right: E
 precedence { Member, Call > Unary > right Power > Product > Sum }
}
entry E
"#,
    )
}
#[test]
fn precedence_associativity_postfix_prefix_and_grouping() {
    let language = expression();
    let ast = parse(&language, "a + b * c").ast.unwrap();
    assert_eq!(ast.kind, "E::Sum");
    assert_eq!(node(&ast.fields["right"]).kind, "E::Product");
    let ast = parse(&language, "2 ** 3 ** 2").ast.unwrap();
    assert_eq!(node(&ast.fields["right"]).kind, "E::Power");
    let ast = parse(&language, "a + b + c").ast.unwrap();
    assert_eq!(node(&ast.fields["left"]).kind, "E::Sum");
    let ast = parse(&language, "foo.bar(1).baz").ast.unwrap();
    assert_eq!(ast.kind, "E::Member");
    assert_eq!(node(&ast.fields["object"]).kind, "E::Call");
    let ast = parse(&language, "!!!x").ast.unwrap();
    assert_eq!(ast.kind, "E::Unary");
    assert_eq!(node(&ast.fields["argument"]).kind, "E::Unary");
    let ast = parse(&language, "(a + b) * c").ast.unwrap();
    assert_eq!(ast.kind, "E::Product");
    assert_eq!(node(&ast.fields["left"]).kind, "E::Group");
}
#[test]
fn nonassoc_rejects_chaining() {
    let language=compile("node E { node N = value: /[0-9]+/ node Compare = left: E \"<\" right: E precedence { nonassoc Compare } } entry E");
    assert!(parse(&language, "1<2").ast.is_some());
    assert!(parse(&language, "1<2<3").has_errors());
}
#[test]
fn nested_abstract_family_executes_its_own_precedence() {
    let language=compile("node Arithmetic { node N = value: /[0-9]+/ node Sum = left: Arithmetic \"+\" right: Arithmetic precedence { Sum } } node E { node Arithmetic } entry E");
    let ast = parse(&language, "1+2+3").ast.unwrap();
    assert_eq!(ast.kind, "Arithmetic::Sum");
    assert_eq!(node(&ast.fields["left"]).kind, "Arithmetic::Sum");
}
#[test]
fn abstract_constraints_do_not_add_wrappers() {
    let language=compile("node E { node N = value: /x/ constraints { when value.matches(/x/) { warning(\"x\") } } } entry E");
    let result = parse(&language, "x");
    assert_eq!(result.ast.unwrap().kind, "E::N");
    assert_eq!(result.diagnostics.len(), 1);
}
#[test]
fn trivia_is_normal_grammar_without_recursive_skip() {
    let language = compile(
        r#"trivia W = /\s+/ trivia Comment { trivia Line = /\/\/[^\n]*/ trivia Block = "/*" /[^*]*/ "*/" } node P = "a" "b" entry P"#,
    );
    assert!(parse(&language, " /* hi */ a // comment\n b ")
        .ast
        .is_some());
    assert!(parse(&language, "a /* unfinished b").has_errors());
}
#[test]
fn patterns_options_lists_tuples_and_capture_only_fields() {
    let language=compile("pattern Tuple = A B node A = value: /a/ node B = value: /b/ node P = \"!\" tuple: Tuple opt: A? list: B* plus: A+ entry P");
    let ast = parse(&language, "!abb aa".replace(' ', "").as_str())
        .ast
        .unwrap();
    assert_eq!(ast.fields.len(), 4);
    assert!(matches!(ast.fields["tuple"],AstValue::List(ref v) if v.len()==2));
    assert_eq!(ast.fields["opt"], AstValue::None);
    assert!(matches!(ast.fields["list"],AstValue::List(ref v) if v.len()==1));
    assert!(matches!(ast.fields["plus"],AstValue::List(ref v) if v.len()==2));
}
#[test]
fn core_requires_import_and_keyword_boundaries_are_language_local() {
    let sources=BTreeMap::from([("test.yl".into(),r#"import { boundedBy } from "std/parser" pattern Part = /[a-z]/ pattern keyword(v) = v |> boundedBy(Part) node P = keyword("let") entry P"#.into())]);
    let language = compile_sources("test.yl", &sources).unwrap();
    assert!(parse(&language, "let").ast.is_some());
    for text in ["letter", "outlet"] {
        assert!(parse(&language, text).has_errors());
    }
    let language = compile(
        r#"import { notBehind } from "core/parser" node P = "out" notBehind(/[a-z]/) "let" entry P"#,
    );
    assert!(parse(&language, "outlet").has_errors());
}
#[test]
fn constraints_and_extensions_emit_after_success_only() {
    let language = compile(
        r#"trivia W = /\s+/ constraint tight(left,right) { when trivia.between(left,right) { error("not tight") help("remove whitespace") } } node P = left: "a" right: "b" { constraints { tight(left,right) } } extend node P { constraints { when left.matches(/a/) { warning("a seen") } } } entry P"#,
    );
    let tight = parse(&language, "ab");
    assert_eq!(tight.diagnostics.len(), 1);
    assert!(!tight.has_errors());
    let loose = parse(&language, "a b");
    assert_eq!(loose.diagnostics.len(), 3);
    assert!(loose.has_errors());
    assert!(loose.ast.is_some());
    let invalid = parse(&language, "a c");
    assert_eq!(invalid.diagnostics.len(), 1);
    assert!(invalid.ast.is_none());
}
#[test]
fn invalid_source_is_structured_and_deterministic() {
    let language = expression();
    for source in ["a +", "(1", "foo(,)", "@", "é", "1 ** ** 2"] {
        let a = parse_named(&language, "bad.js", source);
        assert!(a.has_errors(), "{source}");
        assert_eq!(a, parse_named(&language, "bad.js", source));
        assert_eq!(a.diagnostics[0].primary.file, "bad.js");
    }
}
