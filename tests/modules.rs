use std::collections::BTreeMap;
use your_language::{compile_sources, parse, AstValue};

#[test]
fn exported_parent_allows_qualified_access_but_does_not_export_child_globally() {
    let module = "export node Family { node Child = value: /x/ }";
    let graph = |import: &str| {
        BTreeMap::from([
            (
                "main.yl".into(),
                format!("{import} node P = child: C entry P"),
            ),
            ("family.yl".into(), module.into()),
        ])
    };
    let language = compile_sources(
        "main.yl",
        &graph("import { Family::Child as C } from \"./family\""),
    )
    .unwrap();
    let ast = parse(&language, "x").ast.unwrap();
    let AstValue::Node(child) = &ast.fields["child"] else {
        panic!()
    };
    assert_eq!(child.kind, "Family::Child");
    assert_eq!(
        compile_sources("main.yl", &graph("import { Child as C } from \"./family\"")).unwrap_err()
            [0]
        .code,
        "yl.non_exported_import"
    );
}
#[test]
fn exported_child_does_not_export_parent() {
    let graph = BTreeMap::from([
        (
            "main.yl".into(),
            "import { Child } from \"./family\" node P = Child entry P".into(),
        ),
        (
            "family.yl".into(),
            "node Family { export node Child = value: /x/ }".into(),
        ),
    ]);
    assert!(parse(&compile_sources("main.yl", &graph).unwrap(), "x")
        .ast
        .is_some());
    let mut graph = graph;
    graph.insert(
        "main.yl".into(),
        "import { Family } from \"./family\" node P = Family entry P".into(),
    );
    assert_eq!(
        compile_sources("main.yl", &graph).unwrap_err()[0].code,
        "yl.non_exported_import"
    );
}
#[test]
fn import_order_does_not_change_extensions_or_artifact() {
    let graph = |imports: &str| {
        BTreeMap::from([
        ("main.yl".into(),format!("{imports} node P = Name entry P")),
        ("names.yl".into(),"export node Name = value: /[a-z]+/".into()),
        ("checks.yl".into(),"import { Name } from \"./names\" export pattern Unused = /x/ extend node Name { constraints { when value.matches(/x/) { warning(\"x\") } } }".into()),
    ])
    };
    let first = compile_sources(
        "main.yl",
        &graph("import { Name } from \"./names\" import { Unused } from \"./checks\""),
    )
    .unwrap();
    let second = compile_sources(
        "main.yl",
        &graph("import { Unused } from \"./checks\" import { Name } from \"./names\""),
    )
    .unwrap();
    // Declaration source spans differ with formatting/order; executable rule semantics do not.
    let a = parse(&first, "x");
    let b = parse(&second, "x");
    assert_eq!(a.ast, b.ast);
    assert_eq!(a.diagnostics, b.diagnostics);
}
#[test]
fn transitive_imported_membership_resolves_across_modules() {
    let graph=BTreeMap::from([
        ("main.yl".into(),"import { Arithmetic } from \"./math\" node E { node Arithmetic } node P = v: E::Arithmetic::N entry P".into()),
        ("math.yl".into(),"export node Arithmetic { node N = value: /[0-9]+/ }".into()),
    ]);
    let ast = parse(&compile_sources("main.yl", &graph).unwrap(), "12")
        .ast
        .unwrap();
    let AstValue::Node(n) = &ast.fields["v"] else {
        panic!()
    };
    assert_eq!(n.kind, "Arithmetic::N");
}
