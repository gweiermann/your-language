use std::collections::BTreeMap;
use your_language::{
    diagnostic::Diagnostic,
    semantics::{
        Evaluation, NativeOperation, OperationContext, OperationParameter, OperationRegistry,
        OperationSignature, ParameterKind, SemanticEngine, SemanticValue,
    },
};

struct Value;
impl NativeOperation for Value {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        Ok(Evaluation::Ready(
            context.argument("value").cloned().unwrap(),
        ))
    }
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let value = context.argument("value").cloned().unwrap();
        context.attach("test#value", value);
        Ok(())
    }
}
fn engine() -> SemanticEngine {
    let mut registry = OperationRegistry::new();
    registry
        .register(
            "test",
            "value",
            OperationSignature {
                id: "test#value".into(),
                finalization_only: false,
                contribution: false,
                provides_context: false,
                parameters: vec![OperationParameter {
                    name: "value".into(),
                    kind: ParameterKind::Value,
                    default: None,
                }],
            },
            || Box::new(Value),
        )
        .unwrap();
    SemanticEngine::new(registry)
}
fn graph(source: &str, extra: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut sources = BTreeMap::from([("language.yl".into(), source.into())]);
    for (name, source) in extra {
        sources.insert((*name).into(), (*source).into());
    }
    sources
}
#[test]
fn wrappers_expand_hygienically_through_namespace_reexports_and_reload() {
    let sources = graph(
        r#"import * as lib from "bridge"
symbol Kind
node Program = "x" { semantics { lib.annotate() } }
entry Program"#
            .replace("lib.annotate", "lib::annotate")
            .as_str(),
        &[
            ("bridge.yl", r#"export import { annotate } from "library""#),
            (
                "library.yl",
                r#"import * as native from "test"
export symbol Kind
operation query(value = Kind) = native::value(value)
export operation annotate(value = Kind) { native::value(query(value)) }"#,
            ),
        ],
    );
    let engine = engine();
    let language = engine.compile("language.yl", &sources).unwrap();
    let serialized = language.to_bytes().unwrap();
    let language = engine.load(&serialized).unwrap();
    let result = engine.analyze(&language, "program", "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.graph.attachments[0].value,
        SemanticValue::Symbol("library.yl#Kind".into())
    );
    assert!(!String::from_utf8(serialized)
        .unwrap()
        .contains("library.yl#annotate"));
}
#[test]
fn wrappers_preserve_caller_captures_and_tuple_list_values() {
    let sources = graph(
        r#"import * as native from "test"
operation capture(value) { native::value(value) }
node Program = name: /[a-z]+/ { semantics { capture((name, ["text", true, absent])) } }
entry Program"#,
        &[],
    );
    let engine = engine();
    let language = engine.compile("language.yl", &sources).unwrap();
    let result = engine.analyze(&language, "program", "hello");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let SemanticValue::Tuple(values) = &result.graph.attachments[0].value else {
        panic!("tuple expected")
    };
    assert!(matches!(&values[0],SemanticValue::Source(source) if source.text=="hello"));
    assert_eq!(
        values[1],
        SemanticValue::List(vec![
            SemanticValue::Text("text".into()),
            SemanticValue::Bool(true),
            SemanticValue::Absent
        ])
    );
}
#[test]
fn wrapper_named_arguments_and_multiple_actions_preserve_order() {
    let sources = graph(
        r#"import * as native from "test"
operation annotate(first="first",second="second") { native::value(first) native::value(second) }
node Program="x" { semantics { annotate(second="last") } }
entry Program"#,
        &[],
    );
    let engine = engine();
    let language = engine.compile("language.yl", &sources).unwrap();
    let result = engine.analyze(&language, "program", "x");
    assert_eq!(
        result
            .graph
            .attachments
            .iter()
            .map(|a| a.value.clone())
            .collect::<Vec<_>>(),
        vec![
            SemanticValue::Text("first".into()),
            SemanticValue::Text("last".into())
        ]
    );
}
#[test]
fn invalid_wrapper_calls_and_private_namespace_members_are_diagnostics() {
    for (operation, call, code) in [
        ("operation a() { a() }", "a()", "yl.operation_cycle"),
        (
            "operation a() { native::value(missing) }",
            "a()",
            "yl.unknown_reference",
        ),
        (
            "operation a() = a()",
            "native::value(a())",
            "yl.query_native_effect",
        ),
        (
            "operation a(x) { native::value(x) }",
            "a()",
            "yl.invalid_argument",
        ),
        (
            "operation a(x) { native::value(x) }",
            "a(x=\"a\",x=\"b\")",
            "yl.invalid_argument",
        ),
        (
            "operation a() { native::value(\"a\") }",
            "native::value(a())",
            "yl.operation_result",
        ),
        (
            "operation a() = native::value(\"a\")",
            "a()",
            "yl.invalid_operation_action",
        ),
    ] {
        let source=format!("import * as native from \"test\"\n{operation}\nnode Program=\"x\" {{semantics{{{call}}}}}\nentry Program");
        let errors = engine()
            .compile("language.yl", &graph(&source, &[]))
            .unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == code),
            "{call}: {errors:?}"
        );
    }
    let source="import * as lib from \"library\"\nnode Program=\"x\" { semantics { lib::private() } }\nentry Program";
    let errors = engine()
        .compile(
            "language.yl",
            &graph(source, &[("library.yl", "operation private() {}")]),
        )
        .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.code == "yl.non_exported_import"));
}
#[test]
fn semantic_tuples_and_operations_format_idempotently() {
    let source="import * as native from \"test\"\noperation emit(value=absent){native::value((value,[true,\"a\"]))}\nnode Program=\"x\"{semantics{emit()}}\nentry Program";
    let formatted = your_language::format_yl("language.yl", source).unwrap();
    assert_eq!(
        formatted,
        your_language::format_yl("language.yl", &formatted).unwrap()
    );
    assert!(formatted.contains("operation emit"));
}

#[test]
fn unused_wrappers_reject_unresolved_references_and_cycles() {
    for operation in [
        "operation unused() { missing() }",
        "operation unused() { native::value(missing) }",
        "operation unused() { unused() }",
    ] {
        let source = format!(
            "import * as native from \"test\"\n{operation}\nnode Program=\"x\"\nentry Program"
        );
        assert!(
            engine()
                .compile("language.yl", &graph(&source, &[]))
                .is_err(),
            "{operation}"
        );
    }
}

#[test]
fn namespace_aliases_cannot_shadow_local_or_named_imports() {
    for conflict in [
        "symbol native",
        "import { value as native } from \"test\"",
        "import * as native from \"test\"",
    ] {
        let source = format!(
            "import * as native from \"test\"\n{conflict}\nnode Program=\"x\"\nentry Program"
        );
        let errors = engine()
            .compile("language.yl", &graph(&source, &[]))
            .unwrap_err();
        assert_eq!(errors[0].code, "yl.duplicate_declaration");
    }
}

#[test]
fn contextual_literal_capture_names_survive_wrapper_arguments() {
    let source = r#"import * as native from "test"
operation annotate(value = absent) { native::value(value) }
node Program = absent: /a/ true: /b/ false: /c/ {
    semantics { annotate(absent) annotate(true) annotate(false) annotate() }
}
entry Program"#;
    let engine = engine();
    let language = engine.compile("language.yl", &graph(source, &[])).unwrap();
    let result = engine.analyze(&language, "input", "abc");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let values: Vec<_> = result
        .graph
        .attachments
        .iter()
        .map(|attachment| &attachment.value)
        .collect();
    for (value, text) in values.iter().zip(["a", "b", "c"]) {
        assert!(matches!(value, SemanticValue::Source(source) if source.text == text));
    }
    assert_eq!(values[3], &SemanticValue::Absent);
}
