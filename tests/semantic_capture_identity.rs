use std::collections::BTreeMap;
use your_language::{
    diagnostic::Diagnostic,
    semantics::{
        NativeOperation, OperationContext, OperationParameter, OperationRegistry,
        OperationSignature, ParameterKind, SemanticEngine, SemanticValue,
    },
};
struct Inspect;
impl NativeOperation for Inspect {
    fn before(&mut self, ctx: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        ctx.attach(
            "test/identity#capture",
            ctx.argument("value").unwrap().clone(),
        );
        Ok(())
    }
}
fn engine() -> SemanticEngine {
    let mut registry = OperationRegistry::new();
    registry
        .register(
            "test/identity",
            "inspect",
            OperationSignature {
                id: "test/identity#inspect".into(),
                provides_context: false,
                parameters: vec![OperationParameter {
                    name: "value".into(),
                    kind: ParameterKind::Capture,
                    default: None,
                }],
            },
            || Box::new(Inspect),
        )
        .unwrap();
    SemanticEngine::new(registry)
}
#[test]
fn capture_handles_select_original_alias_pattern_and_pipe_occurrences() {
    for construct in ["node", "pattern"] {
        let sources = BTreeMap::from([
            (
                "items.yl".into(),
                format!("export {construct} Item = /[a-z]/"),
            ),
            (
                "main.yl".into(),
                r#"
                import { Item as Alias } from "./items"
                import { inspect } from "test/identity"
                import { separatedBy } from "std/parser"
                pipe wrapped() { rewrite value => "(" value ")" }
                node Root = value: Alias |> wrapped() ";" items: Alias* |> separatedBy(",") {
                    meanings { inspect(value) inspect(items) }
                }
                entry Root
            "#
                .into(),
            ),
        ]);
        let engine = engine();
        let language = engine.compile("main.yl", &sources).unwrap();
        let result = engine.analyze(&language, "input", "(a);b,c");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(
            result,
            engine.analyze(
                &engine.load(&language.to_bytes().unwrap()).unwrap(),
                "input",
                "(a);b,c"
            )
        );
        let root = &result.occurrences[0];
        let capture = &root.captures["value"];
        assert_eq!(capture.text, "a");
        assert_eq!(capture.occurrences.len(), 1);
        let original = root
            .children
            .iter()
            .find(|child| child.id == capture.occurrences[0])
            .unwrap();
        assert_eq!(original.definition, "items.yl#Item");
        assert_eq!(original.span.start, 1);
        assert_eq!(original.span.end, 2);
        let list = &root.captures["items"];
        assert_eq!(list.occurrences.len(), 2);
        let texts: Vec<_> = list
            .occurrences
            .iter()
            .map(|id| {
                let node = root.children.iter().find(|child| child.id == *id).unwrap();
                &"(a);b,c"[node.span.start..node.span.end]
            })
            .collect();
        assert_eq!(texts, ["b", "c"]);
        for attachment in &result.graph.attachments {
            let SemanticValue::Source(source) = &attachment.value else {
                panic!("Expected source capture")
            };
            assert!(!source.occurrences.is_empty());
        }
    }
}
#[test]
fn terminals_and_absent_captures_have_no_occurrence_handles() {
    let source = r#"import { inspect } from "test/identity" node Root = value: "a" absent: "b"? { meanings { inspect(value) inspect(absent) } } entry Root"#;
    let result = engine()
        .compile_and_analyze(
            "main.yl",
            &BTreeMap::from([("main.yl".into(), source.into())]),
            "input",
            "a",
        )
        .unwrap();
    assert!(result.diagnostics.is_empty());
    assert!(result.occurrences[0]
        .captures
        .values()
        .all(|capture| capture.occurrences.is_empty()));
}

#[test]
fn pratt_capture_handles_refer_to_committed_subexpressions_not_equal_spans() {
    let source = r#"
        import { inspect } from "test/identity"
        node Number = /[0-9]+/
        node Add = left: Expression "+" right: Expression {
            meanings { inspect(left) inspect(right) }
        }
        node Expression {
            node Add
            node Number
            precedence { Add }
        }
        node Root = value: Expression "!" {
            meanings { inspect(value) }
        }
        entry Root
    "#;
    let engine = engine();
    let language = engine
        .compile(
            "main.yl",
            &BTreeMap::from([("main.yl".into(), source.into())]),
        )
        .unwrap();
    let result = engine.analyze(&language, "input", "1+2+3!");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    fn collect<'a>(
        occurrence: &'a your_language::semantics::Occurrence,
        all: &mut Vec<&'a your_language::semantics::Occurrence>,
    ) {
        all.push(occurrence);
        for child in &occurrence.children {
            collect(child, all);
        }
    }
    let mut all = vec![];
    collect(&result.occurrences[0], &mut all);
    for occurrence in &all {
        for capture in occurrence.captures.values() {
            for id in &capture.occurrences {
                let target = all.iter().find(|candidate| candidate.id == *id).unwrap();
                assert_eq!(target.span, capture.span);
                assert_ne!(target.id, occurrence.id);
            }
        }
    }
    let add = all
        .iter()
        .find(|occurrence| occurrence.definition == "main.yl#Add" && occurrence.span.end == 5)
        .unwrap();
    assert_eq!(add.captures["left"].text, "1+2");
    assert_eq!(add.captures["right"].text, "3");
    assert_ne!(
        add.captures["left"].occurrences,
        add.captures["right"].occurrences
    );
    assert_eq!(
        result,
        engine.analyze(
            &engine.load(&language.to_bytes().unwrap()).unwrap(),
            "input",
            "1+2+3!"
        )
    );
}
