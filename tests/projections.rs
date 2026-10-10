use std::collections::BTreeMap;
use your_language::{compile_sources, load_compiled_language, parse, AstValue, CompiledLanguage};
fn compile(source: &str) -> CompiledLanguage {
    compile_sources("p.yl", &BTreeMap::from([("p.yl".into(), source.into())])).unwrap()
}

#[test]
fn scalar_pipes_preserve_input_values_through_arbitrary_wrappers() {
    let language = compile(
        r#"
        pipe wrapped(open,close) { rewrite value => open value close }
        node N = value: /x/
        node P = item: N |> wrapped("(", ")") |> wrapped("[", "]")
        entry P
    "#,
    );
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    let ast = parse(&language, "[(x)]").ast.unwrap();
    let AstValue::Node(n) = &ast.fields["item"] else {
        panic!("{:?}", ast.fields)
    };
    assert_eq!(n.kind, "N");
    assert_eq!(n.fields["value"], AstValue::Text("x".into()));
}
#[test]
fn repetition_rewrites_collect_only_bound_item_values() {
    for q in ["*", "+"] {
        let language = compile(&format!(
            r#"
            pipe terminated(separator) {{ rewrite item{q} => (item separator){q} }}
            node Item = value: /x/
            node P = items: Item{q} |> terminated(",")
            entry P
        "#
        ));
        let ast = parse(&language, "x,x,").ast.unwrap();
        let AstValue::List(items) = &ast.fields["items"] else {
            panic!()
        };
        assert_eq!(items.len(), 2);
        assert!(items
            .iter()
            .all(|i| matches!(i,AstValue::Node(n) if n.kind=="Item")));
        if q == "*" {
            assert_eq!(
                parse(&language, "").ast.unwrap().fields["items"],
                AstValue::List(vec![])
            );
        } else {
            assert!(parse(&language, "").has_errors());
        }
    }
}
#[test]
fn tuple_valued_items_are_collected_without_flattening_their_own_values() {
    let language = compile(
        r#"pattern Pair = A B pipe p(separator) { rewrite item* => (item separator)* } node A = value: /a/ node B = value: /b/ node P = pairs: Pair* |> p(",") entry P"#,
    );
    let ast = parse(&language, "ab,ab,").ast.unwrap();
    let AstValue::List(items) = &ast.fields["pairs"] else {
        panic!()
    };
    assert_eq!(items.len(), 2);
    assert!(items
        .iter()
        .all(|v| matches!(v,AstValue::List(pair) if pair.len()==2)));
}
#[test]
fn input_captures_survive_and_wrapper_captures_do_not_leak() {
    let language = compile(
        r#"pipe wrap(open,close) { rewrite value => ignored: open value close } node N = value: /x/ node P = (item: N) |> wrap("(",")") entry P"#,
    );
    let ast = parse(&language, "(x)").ast.unwrap();
    assert_eq!(ast.fields.len(), 1);
    assert!(matches!(ast.fields["item"], AstValue::Node(_)));
}
#[test]
fn typed_arguments_see_the_preserved_pipe_result() {
    let language = compile(
        r#"pipe wrap() { rewrite value => "(" value ")" } pattern accept(v: N) = v node N = value: /x/ node P = item: accept(N |> wrap()) entry P"#,
    );
    assert!(matches!(
        parse(&language, "(x)").ast.unwrap().fields["item"],
        AstValue::Node(_)
    ));
}
#[test]
fn scalar_duplication_is_rejected_instead_of_selecting_an_arbitrary_value() {
    let result=compile_sources("p.yl",&BTreeMap::from([("p.yl".into(),"pipe twice() { rewrite value => value value } node N = /x/ node P = N |> twice() entry P".into())]));
    assert_eq!(result.unwrap_err()[0].code, "yl.projection_cardinality");
}
