use std::collections::BTreeMap;
use your_language::{compile_sources, parse};

#[test]
fn importing_helpers_does_not_implicitly_activate_private_trivia() {
    let sources = BTreeMap::from([
        (
            "main.yl".into(),
            r#"import { N } from "./lexical" node P = N N entry P"#.into(),
        ),
        (
            "lexical.yl".into(),
            r#"trivia W = /\s+/ export node N = /x/"#.into(),
        ),
    ]);
    let language = compile_sources("main.yl", &sources).unwrap();
    assert!(!parse(&language, "xx").has_errors());
    assert!(parse(&language, "x x").has_errors());
}

#[test]
fn entry_selects_exported_concrete_and_abstract_trivia_through_reload() {
    use your_language::load_compiled_language;
    let sources=BTreeMap::from([
        ("main.yl".into(),r#"import { N, W as Space, Comment } from "./lexical" node P = N N entry P { trivia Space, Comment }"#.into()),
        ("lexical.yl".into(),r#"export trivia W = /\s+/ export trivia Comment { trivia Line = /\/\/[^\n]*/ trivia Block = "/*" /[^*]*/ "*/" } trivia Hidden = "~" export node N = /x/"#.into()),
    ]);
    let language = compile_sources("main.yl", &sources).unwrap();
    let language = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    assert!(!parse(&language, " /* hi */ x // comment\n x ").has_errors());
    assert!(parse(&language, "x~x").has_errors());
}

#[test]
fn imports_do_not_activate_unselected_exported_trivia() {
    let sources = BTreeMap::from([
        (
            "main.yl".into(),
            r#"import { N, W } from "./lexical" node P = N N entry P"#.into(),
        ),
        (
            "lexical.yl".into(),
            r#"export trivia W = /\s+/ export node N = /x/"#.into(),
        ),
    ]);
    assert!(parse(&compile_sources("main.yl", &sources).unwrap(), "x x").has_errors());
}

#[test]
fn entry_trivia_uses_normal_visibility_and_reports_selector_spans() {
    for (imports, selection, expected) in [
        ("N", "Private", "yl.unknown_reference"),
        ("N, Private", "Private", "yl.non_exported_import"),
        ("N", "N", "yl.entry_trivia"),
    ] {
        let main = format!(
            "import {{ {imports} }} from \"./lexical\" node P = N entry P {{ trivia {selection} }}"
        );
        let sources = BTreeMap::from([
            ("main.yl".into(), main.clone()),
            (
                "lexical.yl".into(),
                r#"trivia Private = /\s+/ export node N = /x/"#.into(),
            ),
        ]);
        let error = compile_sources("main.yl", &sources).unwrap_err().remove(0);
        assert_eq!(error.code, expected);
        if expected == "yl.entry_trivia" {
            assert_eq!(&main[error.primary.start..error.primary.end], "N");
        }
    }
}
