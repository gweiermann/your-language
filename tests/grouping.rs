use std::collections::BTreeMap;
use your_language::{compile_sources, parse, AstValue, CompiledLanguage};
fn compile(source: &str) -> CompiledLanguage {
    compile_sources(
        "group.yl",
        &BTreeMap::from([("group.yl".into(), source.into())]),
    )
    .unwrap()
}

#[test]
fn node_reference_then_group_preserves_capture_and_postfix_attachment() {
    let language = compile(
        r#"node Name = value: /[a-z]+/ node Number = value: /[0-9]+/ node P = name: Name ("=" initializer: Number)? entry P"#,
    );
    let ast = parse(&language, "x=1").ast.unwrap();
    assert_eq!(ast.fields.len(), 2);
    assert!(matches!(ast.fields["name"],AstValue::Node(ref n) if n.kind=="Name"));
    assert!(matches!(ast.fields["initializer"],AstValue::Node(ref n) if n.kind=="Number"));
    let ast = parse(&language, "x").ast.unwrap();
    assert_eq!(ast.fields.len(), 1);
    assert!(matches!(ast.fields["name"], AstValue::Node(_)));
}
#[test]
fn pipes_apply_to_the_group_and_explicit_grouping_applies_to_the_whole_sequence() {
    let declarations =
        r#"pipe wrap() { rewrite value => "[" value "]" } node A = value: /a/ node B = value: /b/"#;
    let language = compile(&format!(
        r#"{declarations} node P = x: A (B "|")? |> wrap() entry P"#
    ));
    for source in ["a[b|]", "a[]"] {
        let ast = parse(&language, source).ast.unwrap();
        assert_eq!(ast.fields.len(), 1);
        assert!(matches!(ast.fields["x"],AstValue::Node(ref n) if n.kind=="A"));
    }
    assert!(parse(&language, "[ab|]").has_errors());
    let language = compile(&format!(
        r#"{declarations} node P = x: (A (B "|")?) |> wrap() entry P"#
    ));
    let ast = parse(&language, "[ab|]").ast.unwrap();
    assert!(matches!(ast.fields["x"],AstValue::List(ref v) if v.len()==2));
}
#[test]
fn qualified_imported_nodes_group_and_forward_parameterized_patterns_call() {
    let sources=BTreeMap::from([
        ("group.yl".into(),r#"import { A as Alias } from "./a" node P = first: Alias (second: Alias)? third: f(Alias) pattern f(v) = v entry P"#.into()),
        ("a.yl".into(),"export node A = value: /a/".into()),
    ]);
    let language = compile_sources("group.yl", &sources).unwrap();
    let ast = parse(&language, "aaa").ast.unwrap();
    assert_eq!(ast.fields.len(), 3);
    assert!(ast
        .fields
        .values()
        .all(|v| matches!(v,AstValue::Node(n) if n.kind=="A")));
}
#[test]
fn std_lists_can_discard_node_valued_separators_without_runtime_special_cases() {
    let language = compile(
        r#"import { separatedBy } from "std/parser" node Item = value: /x/ node Separator = value: "," node P = items: Item+ |> separatedBy(Separator) entry P"#,
    );
    let ast = parse(&language, "x,x,x").ast.unwrap();
    assert_eq!(ast.fields.len(), 1);
    let AstValue::List(items) = &ast.fields["items"] else {
        panic!()
    };
    assert_eq!(items.len(), 3);
    assert!(items
        .iter()
        .all(|v| matches!(v,AstValue::Node(n) if n.kind=="Item")));
}
