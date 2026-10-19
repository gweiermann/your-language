use super::*;

#[test]
fn nested_when_checks_accept_imported_diagnostic_namespaces() {
    let source = r#"import * as diagnostics from "core/diagnostics"
node Name = value: /[a-z]+/ {
checks {
    when value.isPresent {
        when value.matches(/bad/) {
            diagnostics::error("Invalid name")
        }
    }
}
}
entry Name"#;
    let module = parse_yl("checks.yl", source).unwrap();
    let DeclKind::Node { constraints, .. } = &module.declarations[1].kind else {
        panic!("Expected checked node");
    };
    let ConstraintKind::When(_, outer) = &constraints[0].kind else {
        panic!("Expected outer when");
    };
    let ConstraintKind::When(_, inner) = &outer[0].kind else {
        panic!("Expected nested when");
    };
    let ConstraintKind::Call(name, _) = &inner[0].kind else {
        panic!("Expected diagnostic call");
    };
    assert_eq!(name, "diagnostics::error");
}
