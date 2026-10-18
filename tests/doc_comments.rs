use your_language::{frontend::parse_yl, syntax::DeclKind};

#[test]
fn documentation_attaches_to_declarations_with_original_spans() {
    let source = "// ordinary\r\n/// A café.\r\n///\r\n/// Second paragraph.\r\nexport node Root {\r\n    /// A number.\r\n    node Number = /[0-9]+/\r\n}\r\n/// A helper.\r\noperation helper(value) = value\r\n/// Identity key.\r\nexport symbol key\r\n/// Runtime callback.\r\nhook run(value)\r\n/// Choices.\r\nenum Choice { yes no }\r\n/// Reusable matcher.\r\npattern Item = Root\r\n";
    let module = parse_yl("docs.yl", source).unwrap();
    let root = &module.declarations[0];
    let docs = root.documentation.as_ref().unwrap();
    assert_eq!(docs.text, "A café.\n\nSecond paragraph.");
    assert_eq!(
        &source[docs.span.start..docs.span.end],
        "/// A café.\r\n///\r\n/// Second paragraph."
    );
    assert_eq!(docs.span.file, "docs.yl");
    assert!(source[root.span.start..].starts_with("export node"));
    let DeclKind::Node { members, .. } = &root.kind else {
        panic!()
    };
    assert_eq!(members[0].documentation.as_ref().unwrap().text, "A number.");
    assert!(module
        .declarations
        .iter()
        .all(|declaration| declaration.documentation.is_some()));
    let json = serde_json::to_string(&module).unwrap();
    let reloaded: your_language::syntax::Module = serde_json::from_str(&json).unwrap();
    assert_eq!(module, reloaded);
}

#[test]
fn blank_lines_ordinary_comments_inline_and_dangling_docs_do_not_attach() {
    for source in [
        "/// Detached.\n\nnode Root = \"a\"",
        "/// Detached.\n// Ordinary.\nnode Root = \"a\"",
        "pattern P = \"p\" /// trailing\nnode Root = \"a\"",
        "node Root = \"a\"\n/// Dangling.",
        "node Root = \"a\" { /// Not on its own line.\n node Child = \"b\" }",
    ] {
        let formatted = your_language::format_yl("docs.yl", source).unwrap();
        assert_eq!(
            formatted,
            your_language::format_yl("docs.yl", &formatted).unwrap()
        );
        let module = parse_yl("docs.yl", &formatted).unwrap();
        assert!(module
            .declarations
            .iter()
            .all(|declaration| declaration.documentation.is_none()));
    }
}

#[test]
fn formatter_preserves_documentation_attachment_and_is_idempotent() {
    let source = "/// Root docs.\nexport node Root {\n/// Child docs.\nnode Child=\"a\"\n}\n/// A pattern.\npattern P=Root\n";
    let formatted = your_language::format_yl("docs.yl", source).unwrap();
    assert!(formatted.contains("/// Root docs.\nexport node Root"));
    assert!(formatted.contains("    /// Child docs.\n    node Child"));
    assert_eq!(
        formatted,
        your_language::format_yl("docs.yl", &formatted).unwrap()
    );
    let module = parse_yl("docs.yl", &formatted).unwrap();
    assert_eq!(
        module.declarations[1].documentation.as_ref().unwrap().text,
        "A pattern."
    );
}

#[test]
fn doc_comments_do_not_change_compiled_language_or_target_ast() {
    let compile = |source: &str| {
        your_language::compile_sources(
            "test.yl",
            &std::collections::BTreeMap::from([("test.yl".into(), source.into())]),
        )
        .unwrap()
    };
    let plain = compile("node Root = value: /[0-9]+/ entry Root");
    let documented = compile("/// A number.\nnode Root = value: /[0-9]+/ entry Root");
    // Definition spans differ, but authoring docs must not become target fields.
    let plain_ast = your_language::parse_named(&plain, "input", "42");
    let docs_ast = your_language::parse_named(
        &your_language::load_compiled_language(&documented.to_bytes().unwrap()).unwrap(),
        "input",
        "42",
    );
    assert_eq!(plain_ast, docs_ast);
}
