use super::*;

#[test]
fn rewrite_diagnostics_accept_namespace_alias_and_forward_imports() {
    for (call, import) in [
        (
            "diagnostics::warn",
            "import * as diagnostics from \"core/diagnostics\"",
        ),
        ("note", "import { hint as note } from \"core/diagnostics\""),
    ] {
        let source = format!("enum Mode {{ a }} pipe wrap(mode: Mode) {{ rewrite value match mode {{ .a => {{ {call}(\"Notice\") value }} }} }} {import}");
        let module = parse_yl("rewrite.yl", &source).unwrap();
        let DeclKind::Pipe { arms, .. } = &module.declarations[1].kind else {
            panic!("Expected pipe");
        };
        let RewriteBody::Match { cases, .. } = &arms[0].body else {
            panic!("Expected rewrite match");
        };
        assert_eq!(cases[0].diagnostics.len(), 1);
        assert!(cases[0].replacement.is_some());
        assert!(
            matches!(&cases[0].diagnostics[0].kind, ConstraintKind::Call(name, _) if name == call)
        );
    }
}

#[test]
fn user_authored_error_callable_is_not_implicitly_a_diagnostic() {
    let source = r#"pattern error(value) = value
    enum Mode { a }
    pipe wrap(mode: Mode) { rewrite value match mode { .a => { error(value) } } }"#;
    let module = parse_yl("rewrite.yl", source).unwrap();
    let DeclKind::Pipe { arms, .. } = &module.declarations[2].kind else {
        panic!("Expected pipe");
    };
    let RewriteBody::Match { cases, .. } = &arms[0].body else {
        panic!("Expected rewrite match");
    };
    assert!(cases[0].diagnostics.is_empty());
    assert!(cases[0].replacement.is_some());
}

#[test]
fn hooks_and_backend_bindings_preserve_unevaluated_capture_expressions() {
    let source = "export hook add(left, right)\nnode Sum = left: Number right: Number { backend { runtime::add(left, right) } }\npattern Wrapper = Number { backend { wrap(self) } }";
    let module = parse_yl("hooks.yl", source).unwrap();
    let DeclKind::Hook { name, parameters } = &module.declarations[0].kind else {
        panic!("Expected hook declaration");
    };
    assert!(module.declarations[0].export);
    assert_eq!(name, "add");
    assert_eq!(
        parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        ["left", "right"]
    );
    let DeclKind::Node {
        backend: Some(backend),
        ..
    } = &module.declarations[1].kind
    else {
        panic!("Expected backend binding");
    };
    assert_eq!(backend.hook, "runtime::add");
    assert!(matches!(&backend.arguments[0].kind, ExprKind::Ref(name) if name == "left"));
    assert_eq!(
        &source[backend.span.start..backend.span.end],
        "backend { runtime::add(left, right) }"
    );
    assert!(matches!(
        &module.declarations[2].kind,
        DeclKind::Pattern {
            backend: Some(_),
            ..
        }
    ));
}

#[test]
fn malformed_hook_contracts_and_backend_blocks_have_diagnostics() {
    for (source, code) in [
        ("hook add(left = true)", "yl.hook_parameter"),
        ("hook add(left: Value)", "yl.hook_parameter"),
        ("hook add(left, left)", "yl.hook_parameter"),
        ("node N = \"n\" { backend {} }", "yl.backend_call"),
        (
            "node N = \"n\" { backend { return add(self) } }",
            "yl.backend_call",
        ),
        (
            "node N = \"n\" { backend { add(self) add(self) } }",
            "yl.backend_call",
        ),
        (
            "node N = \"n\" { backend { add(value=self) } }",
            "yl.backend_call",
        ),
        (
            "node N = \"n\" { backend { add(self) } backend { add(self) } }",
            "yl.duplicate_member",
        ),
        (
            "pattern N = \"n\" { backend { add(self) } backend { add(self) } }",
            "yl.duplicate_member",
        ),
    ] {
        let errors = parse_yl("invalid.yl", source).unwrap_err();
        assert_eq!(errors[0].code, code, "{source}");
    }
}
