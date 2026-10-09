use std::collections::BTreeMap;
use your_language::{compile_sources, load_compiled_language, parse, AstValue, CompiledLanguage};

fn compile(source: &str) -> CompiledLanguage {
    compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    )
    .unwrap()
}
fn code(source: &str) -> String {
    compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    )
    .unwrap_err()[0]
        .code
        .clone()
}

#[test]
fn imports_exports_forward_references_and_cycles() {
    let sources = BTreeMap::from([
        (
            "main.yl".into(),
            "import { N as Name } from \"./names\" export node P = name: Name entry P".into(),
        ),
        (
            "names.yl".into(),
            "import { P } from \"./main\" export node N = value: /[a-z]+/".into(),
        ),
    ]);
    let language = compile_sources("main.yl", &sources).unwrap();
    assert!(parse(&language, "hello").ast.is_some());
    let mut private = sources;
    private.insert("names.yl".into(), "node N = /x/".into());
    assert_eq!(
        compile_sources("main.yl", &private).unwrap_err()[0].code,
        "yl.non_exported_import"
    );
    assert_eq!(
        code("import { N } from \"./absent\" node P = N entry P"),
        "yl.unresolved_import"
    );
}
#[test]
fn qualified_aliases_preserve_canonical_identity() {
    let language=compile("node Leaf = value: /x/ node A { node Leaf } node B { node A } node P = value: B::A::Leaf entry P");
    let ast = parse(&language, "x").ast.unwrap();
    let AstValue::Node(leaf) = &ast.fields["value"] else {
        panic!()
    };
    assert_eq!(leaf.kind, "Leaf");
    let language = compile("node A { node Leaf = value: /x/ } node P = value: A entry P");
    let ast = parse(&language, "x").ast.unwrap();
    let AstValue::Node(leaf) = &ast.fields["value"] else {
        panic!()
    };
    assert_eq!(leaf.kind, "A::Leaf");
}
#[test]
fn patterns_are_transparent_and_typed_arguments_use_membership() {
    let language=compile("pattern wrap(v: A) = \"(\" v \")\" node A { node Leaf = value: /x/ } node P = value: wrap(A::Leaf) entry P");
    let ast = parse(&language, "(x)").ast.unwrap();
    let AstValue::List(wrapped) = &ast.fields["value"] else {
        panic!()
    };
    let AstValue::Node(leaf) = &wrapped[1] else {
        panic!()
    };
    assert_eq!(leaf.kind, "A::Leaf");
    assert_eq!(
        code("pattern p(v: A) = v node A = /x/ node B = /b/ node P = p(B) entry P"),
        "yl.invalid_typed_argument"
    );
}
#[test]
fn rewrites_enums_and_defaults() {
    let language=compile("enum Mode { plain wrapped } pipe p(mode: Mode = .plain) { rewrite item* { .plain => item* .wrapped => \"[\" item* \"]\" } } node Item = value: /x/ node P = items: Item* |> p(mode=Mode::wrapped) entry P");
    assert!(parse(&language, "[xx]").ast.is_some());
    assert!(parse(&language, "xx").has_errors());
    assert_eq!(
        code("pipe p() { rewrite item+ => item+ } node P = /x/* |> p() entry P"),
        "yl.invalid_rewrite"
    );
    assert_eq!(
        code("node P = .optional entry P"),
        "yl.ambiguous_enum_variant"
    );
    assert_eq!(
        code("enum E { x } pattern p(v: E) = /x/ node P = p(.y) entry P"),
        "yl.invalid_argument"
    );
}
#[test]
fn diagnostic_categories() {
    for (source, expected) in [
        ("node P = X entry P", "yl.unknown_reference"),
        (
            "node P = /x/ node P = /y/ entry P",
            "yl.duplicate_declaration",
        ),
        (
            "node P { node A = /x/ node A = /y/ } entry P",
            "yl.duplicate_member",
        ),
        ("node P = /x/", "yl.entrypoint"),
        ("node P = /x/ entry P entry P", "yl.entrypoint"),
        (
            "node P { node A = /x/ precedence { A } } entry P",
            "yl.invalid_precedence_member",
        ),
        (
            "node P = /x/ extend node P { node X = /x/ } entry P",
            "yl.extension_conflict",
        ),
        ("node P = /[/ entry P", "yl.invalid_regex"),
        (
            "pattern P = P node Main = P entry Main",
            "yl.expansion_cycle",
        ),
        (
            "node A { node B } node B { node A } entry A",
            "yl.membership_cycle",
        ),
    ] {
        assert_eq!(code(source), expected, "{source}");
    }
    assert_eq!(
        code("node P = a: /x/ a: /y/ entry P"),
        "yl.duplicate_member"
    );
    assert_eq!(
        code(
            "node P = /x/ { constraints { when absent.matches(/x/) { error(\"bad\") } } } entry P"
        ),
        "yl.unknown_capture"
    );
    assert_eq!(
        code("pattern unused = Unknown node P = /x/ entry P"),
        "yl.unknown_reference"
    );
    assert_eq!(
        code("node E { node N = /x/ precedence { Unknown } } entry E"),
        "yl.invalid_precedence_member"
    );
    assert_eq!(code("node P = value: /x/? { constraints { when value.matches(/x/) { error(\"x\") } } } entry P"),"yl.parked_constraint_optional");
}
#[test]
fn deterministic_artifacts_and_hostile_input() {
    let language = compile("node P = value: /x/ entry P");
    let bytes = language.to_bytes().unwrap();
    let loaded = load_compiled_language(&bytes).unwrap();
    assert_eq!(bytes, loaded.to_bytes().unwrap());
    assert_eq!(parse(&language, "x"), parse(&loaded, "x"));
    for source in [b"{}".as_slice(), b"not json", b"null", b"[]"] {
        assert!(load_compiled_language(source).is_err());
    }
    let json = String::from_utf8(bytes)
        .unwrap()
        .replace("\"version\": 1", "\"version\": 999");
    assert!(load_compiled_language(json.as_bytes()).is_err());
}
