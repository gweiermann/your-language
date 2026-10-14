use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use your_language::{
    diagnostic::Diagnostic,
    semantics::{
        NativeOperation, OperationContext, OperationRegistry, OperationSignature, SemanticEngine,
    },
};

type Events = Arc<Mutex<Vec<String>>>;

struct Recording {
    events: Events,
    name: &'static str,
    enters: usize,
    fail_enter: Option<usize>,
}

impl NativeOperation for Recording {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:before", self.name));
        context.attach(
            "test/native#visit",
            your_language::semantics::SemanticValue::Text(context.occurrence.definition.clone()),
        );
        Ok(())
    }
    fn enter(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.enters += 1;
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:enter", self.name));
        if self.fail_enter == Some(self.enters) {
            return Err(Diagnostic::error(
                "test.failed_enter",
                "Activation failed",
                context.occurrence.span.clone(),
            ));
        }
        Ok(())
    }
    fn leave(&mut self, _: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:leave", self.name));
        Ok(())
    }
    fn after(&mut self, _: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:after", self.name));
        Ok(())
    }
}

fn recording_engine(events: &Events, fail_enter: Option<usize>) -> SemanticEngine {
    let mut registry = OperationRegistry::new();
    for (name, provides_context) in [("scope", true), ("action", false)] {
        let events = events.clone();
        registry
            .register(
                "test/native",
                name,
                OperationSignature {
                    id: format!("test/native#{name}"),
                    parameters: vec![],
                    provides_context,
                    finalization_only: false,
                    contribution: false,
                },
                move || {
                    Box::new(Recording {
                        events: events.clone(),
                        name,
                        enters: 0,
                        fail_enter: if provides_context { fail_enter } else { None },
                    })
                },
            )
            .unwrap();
    }
    SemanticEngine::new(registry)
}

fn sources(grammar: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("language.yl".into(), grammar.into())])
}

fn count(events: &[String], event: &str) -> usize {
    events
        .iter()
        .filter(|actual| actual.as_str() == event)
        .count()
}

#[test]
fn ordinary_native_operations_receive_balanced_own_activation_hooks() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        import { action } from "test/native"
        node Root = "x" { semantics { action() } }
        entry Root
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let events = events.lock().unwrap();
    assert_eq!(count(&events, "action:before"), 1);
    assert_eq!(count(&events, "action:after"), 1);
    assert!(count(&events, "action:enter") > 0);
    assert_eq!(
        count(&events, "action:enter"),
        count(&events, "action:leave")
    );
}

#[test]
fn failed_cleanup_activation_still_finalizes_every_started_operation_once() {
    let events = Events::default();
    let engine = recording_engine(&events, Some(3));
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        import { scope, action } from "test/native"
        node Child = "c" { semantics { action() } }
        node Root = "{" child: Child "}" { semantics { scope() } }
        entry Root
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "{c}");
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "test.failed_enter"));
    let events = events.lock().unwrap();
    assert_eq!(count(&events, "action:before"), 1);
    assert_eq!(count(&events, "action:after"), 1);
    assert_eq!(count(&events, "scope:before"), 1);
    assert_eq!(count(&events, "scope:after"), 1);
}

#[test]
fn rejected_syntax_does_not_invoke_semantics_but_warnings_continue() {
    for (severity, expected_calls) in [("error", 0), ("warn", 1), ("hint", 1)] {
        let events = Events::default();
        let engine = recording_engine(&events, None);
        let grammar = format!(
            r#"
            import {{ error, warn, hint }} from "core/diagnostics"
            import {{ action }} from "test/native"
            node Root = "x" {{
                checks {{ when true {{ {severity}("Local validation") }} }}
                semantics {{ action() }}
            }}
            entry Root
        "#
        );
        let language = engine.compile("language.yl", &sources(&grammar)).unwrap();
        let result = engine.analyze(&language, "source", "x");
        assert!(result.ast.is_some());
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "parse.constraint");
        assert_eq!(
            count(&events.lock().unwrap(), "action:before"),
            expected_calls
        );
        if severity == "error" {
            assert!(result.graph.records.is_empty());
            assert!(result.graph.attachments.is_empty());
        }
        events.lock().unwrap().clear();
        let rejected = engine.analyze(&language, "source", "@");
        assert!(rejected.ast.is_none());
        assert!(rejected.graph.records.is_empty());
        assert!(rejected.graph.attachments.is_empty());
        assert!(events.lock().unwrap().is_empty());
    }
}

#[test]
fn committed_original_node_and_pattern_actions_survive_pipe_projection_once() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        import { separatedBy } from "std/parser"
        import { notAhead } from "core/parser"
        node Failed = "a" "!" { semantics { action() } }
        node Accepted = "a" { semantics { action() } }
        pattern Item = Failed | Accepted { semantics { action() } }
        node Separator = "," { semantics { action() } }
        node Root = notAhead(Failed) items: Item* |> separatedBy(Separator)
        entry Root
    "#;
    let language = engine.compile("language.yl", &sources(grammar)).unwrap();
    let result = engine.analyze(&language, "source", "a,a");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(count(&events.lock().unwrap(), "action:before"), 4);
    let items = &result.occurrences[0].children;
    assert_eq!(items.len(), 2);
    for (item, offset) in items.iter().zip([0, 2]) {
        assert_eq!(item.definition, "language.yl#Item");
        assert_eq!(item.span.start, offset);
        assert_eq!(item.span.end, offset + 1);
        assert_eq!(item.children.len(), 1);
        assert_eq!(item.children[0].definition, "language.yl#Accepted");
    }

    events.lock().unwrap().clear();
    let duplicated = grammar.replace(
        "node Root = notAhead(Failed) items: Item* |> separatedBy(Separator)",
        "pipe twice() { rewrite item* => item item } node Root = items: Item* |> twice()",
    );
    let language = engine
        .compile("language.yl", &sources(&duplicated))
        .unwrap();
    let result = engine.analyze(&language, "source", "aa");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(count(&events.lock().unwrap(), "action:before"), 4);
    assert_eq!(result.occurrences[0].children.len(), 2);
    assert_eq!(result.occurrences[0].children[0].span.start, 0);
    assert_eq!(result.occurrences[0].children[1].span.start, 1);

    // Pipes keep the original input's meaning. A separator introduced by a pipe
    // contributes no actions, even if its declaration has its own meanings.
    // The same node used directly remains an ordinary semantic occurrence.
    events.lock().unwrap().clear();
    let direct = grammar.replace(
        "node Root = notAhead(Failed) items: Item* |> separatedBy(Separator)",
        "node Root = notAhead(Failed) items: ((Item) (Separator Item)*)",
    );
    let language = engine.compile("language.yl", &sources(&direct)).unwrap();
    let result = engine.analyze(&language, "source", "a,a");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(count(&events.lock().unwrap(), "action:before"), 5);
    assert_eq!(
        result.occurrences[0].children[1].definition,
        "language.yl#Separator"
    );
}

#[test]
fn pratt_occurrences_do_not_duplicate_left_hand_actions() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        import { action } from "test/native"
        node E {
            node Atom = "a" { semantics { action() } }
            node Sum = left: E "+" right: E { semantics { action() } }
            precedence { Sum }
        }
        entry E
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "a+a+a");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(count(&events.lock().unwrap(), "action:before"), 5);
    assert_eq!(count(&events.lock().unwrap(), "action:after"), 5);
    assert_eq!(result.ast.as_ref().unwrap().kind, "E::Sum");
    assert_eq!(result.occurrences[0].definition, "language.yl#E");
    assert_eq!(
        result.occurrences[0].children[0].definition,
        "language.yl#E::Sum"
    );
}

#[test]
fn aliases_preserve_canonical_meaning_identity_and_abstract_boundaries() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let modules = BTreeMap::from([
        (
            "names.yl".into(),
            r#"
            import { action } from "test/native"
            export node Name = "x" { semantics { action() } }
        "#
            .into(),
        ),
        (
            "language.yl".into(),
            r#"
            import { action } from "test/native"
            import { Name as Renamed } from "./names"
            node Family { node Renamed semantics { action() } }
            node Root = direct: Renamed family: Family member: Family::Renamed {
                semantics { action() }
            }
            entry Root
        "#
            .into(),
        ),
    ]);
    let language = engine.compile("language.yl", &modules).unwrap();
    let result = engine.analyze(&language, "source", "xxx");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(count(&events.lock().unwrap(), "action:before"), 5);
    let children = &result.occurrences[0].children;
    assert_eq!(children[0].definition, "names.yl#Name");
    assert_eq!(children[0].span.start, 0);
    assert_eq!(children[1].definition, "language.yl#Family");
    assert_eq!(children[1].children[0].definition, "names.yl#Name");
    assert_eq!(children[1].span.start, 1);
    assert_eq!(children[2].definition, "names.yl#Name");
    assert_eq!(children[2].span.start, 2);
    let ast = result.ast.unwrap();
    for field in ["direct", "family", "member"] {
        let your_language::AstValue::Node(value) = &ast.fields[field] else {
            panic!("Expected concrete node")
        };
        assert_eq!(value.kind, "Name");
    }
}

#[test]
fn compatible_and_conflicting_nested_rules_accumulate_over_deeper_occurrences() {
    for (inner_chain, cyclic) in [
        ("Declaration > Reference", false),
        ("Reference > Declaration", true),
    ] {
        let events = Events::default();
        let engine = recording_engine(&events, None);
        let grammar = format!(
            r#"
            import {{ error, warn, hint }} from "core/diagnostics"
            import {{ action }} from "test/native"
            node Declaration = "d" {{ semantics {{ action() }} }}
            node Reference = "r" {{ semantics {{ action() }} }}
            pattern Inner = Reference Declaration {{ semantics {{ precedence {{ {inner_chain} }} }} }}
            node Root = Inner {{ semantics {{ precedence {{ Declaration > Reference }} }} }}
            entry Root
        "#
        );
        let language = engine.compile("language.yl", &sources(&grammar)).unwrap();
        let result = engine.analyze(&language, "source", "rd");
        if cyclic {
            assert_eq!(result.diagnostics[0].code, "semantic.precedence_cycle");
            assert!(events.lock().unwrap().is_empty());
            assert!(result.graph.attachments.is_empty());
        } else {
            assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
            let order: Vec<_> = result
                .graph
                .attachments
                .iter()
                .filter_map(|a| {
                    if let your_language::semantics::SemanticValue::Text(definition) = &a.value {
                        Some(definition.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(order, ["language.yl#Declaration", "language.yl#Reference"]);
        }
    }
}

#[test]
fn equally_named_authored_symbols_from_distinct_modules_keep_distinct_identity() {
    let engine = SemanticEngine::default();
    let source = r#"import { error, warn, hint } from "core/diagnostics"

        import * as types from "std/types"
        import * as variables from "std/references"
        import { category as First } from "first"
        import { category as Second } from "second"
        node A = "a" { semantics { types::annotateWithType(self, First) } }
        node B = "b" { semantics { types::annotateWithType(self, Second) } }
        node Root = a: A b: B {
            semantics {
                match (types::typeOf(a), types::typeOf(b)) {
                    First, Second => {},
                    _ => error("Authored symbol identities were lost")
                }
            }
        }
        entry Root
    "#;
    let modules = BTreeMap::from([
        ("language.yl".into(), source.into()),
        ("first.yl".into(), "export symbol category".into()),
        ("second.yl".into(), "export symbol category".into()),
    ]);
    let language = engine.compile("language.yl", &modules).unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "source", "ab");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let normalized = String::from_utf8(loaded.to_bytes().unwrap()).unwrap();
    assert!(normalized.contains("first.yl#category"));
    assert!(normalized.contains("second.yl#category"));
}

#[test]
fn traversal_cycle_without_native_operations_is_a_structured_diagnostic() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        node A = "a"
        node Root = A { semantics { precedence { A > A } } }
        entry Root
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "a");
    assert!(result.ast.is_some());
    assert_eq!(result.diagnostics[0].code, "semantic.precedence_cycle");
    assert_eq!(result.diagnostics[0].primary.file, "language.yl");
    assert!(result.graph.records.is_empty());
    assert!(result.graph.attachments.is_empty());
}

#[test]
fn bare_unannotated_node_and_pattern_are_real_traversal_boundaries() {
    for construct in ["node", "pattern"] {
        let events = Events::default();
        let engine = recording_engine(&events, None);
        let grammar = format!(
            r#"
            import {{ error, warn, hint }} from "core/diagnostics"
            import {{ action }} from "test/native"
            node Child = "c" {{ semantics {{ action() }} }}
            {construct} Deferred = Child
            node Early = "e" {{ semantics {{ action() }} }}
            node Root = Deferred Early {{ semantics {{ precedence {{ Early > Deferred }} }} }}
            entry Root
        "#
        );
        let language = engine.compile("language.yl", &sources(&grammar)).unwrap();
        let result = engine.analyze(&language, "source", "ce");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let order: Vec<_> = result
            .graph
            .attachments
            .iter()
            .filter_map(|a| {
                if let your_language::semantics::SemanticValue::Text(definition) = &a.value {
                    Some(definition.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            order,
            ["language.yl#Early", "language.yl#Child"],
            "{construct}"
        );
    }
}

fn visit_order(result: &your_language::semantics::AnalysisResult) -> Vec<&str> {
    result
        .graph
        .attachments
        .iter()
        .filter_map(|a| {
            if let your_language::semantics::SemanticValue::Text(definition) = &a.value {
                Some(definition.as_str())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn transitive_earlier_construct_preserves_its_entire_initializer_subtree() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        node Child = "x" { semantics { action() } }
        node A = Child { semantics { action() } }
        node C = A { semantics { action() } }
        node B = "b" { semantics { action() } }
        node Root = C B { semantics { precedence { A > B > C } } }
        entry Root
    "#;
    for chain in ["A > B > C", "A > B\nB > C"] {
        let grammar = grammar.replace("A > B > C", chain);
        let language = engine.compile("language.yl", &sources(&grammar)).unwrap();
        let result = engine.analyze(&language, "source", "xb");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(
            visit_order(&result),
            [
                "language.yl#Child",
                "language.yl#A",
                "language.yl#B",
                "language.yl#C"
            ],
            "{chain}"
        );
    }
}

#[test]
fn independent_local_predecessor_is_exempt_from_ancestor_body_deferral() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        import { action } from "test/native"
        node Reference = "r" { semantics { action() } }
        node Body = Reference {
            semantics { action() precedence { Reference > Body } }
        }
        node Outer = "o" { semantics { action() } }
        node Root = Body Outer { semantics { precedence { Outer > Body } } }
        entry Root
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "ro");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        visit_order(&result),
        [
            "language.yl#Reference",
            "language.yl#Outer",
            "language.yl#Body"
        ]
    );
}

#[test]
fn absent_pipe_input_keeps_an_empty_original_source_span() {
    use your_language::semantics::{OperationParameter, ParameterKind, SemanticValue};
    struct Observe;
    impl NativeOperation for Observe {
        fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            context.attach("observed", context.argument("value").unwrap().clone());
            Ok(())
        }
    }
    let mut registry = OperationRegistry::new();
    registry
        .register(
            "test/provenance",
            "observe",
            OperationSignature {
                id: "test/provenance#observe".into(),
                finalization_only: false,
                contribution: false,
                provides_context: false,
                parameters: vec![OperationParameter {
                    name: "value".into(),
                    kind: ParameterKind::Capture,
                    default: None,
                }],
            },
            || Box::new(Observe),
        )
        .unwrap();
    let engine = SemanticEngine::new(registry);
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        import { observe } from "test/provenance"
        pipe wrapped() { rewrite value => "(" value ")" }
        node Root = value: /[a-z]+/? |> wrapped() { semantics { observe(value) } }
        entry Root
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "()");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let SemanticValue::Source(value) = &result.graph.attachments[0].value else {
        panic!("Expected source capture")
    };
    assert_eq!(value.value, your_language::AstValue::None);
    assert_eq!(value.text, "");
    assert_eq!(value.span.start, 1);
    assert_eq!(value.span.end, 1);
}

#[test]
fn region_selection_reaches_matches_at_every_wrapper_depth() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        node A = "a" { semantics { action() } }
        node Wrapped = A
        node B = "b" { semantics { action() } }
        node Root = Wrapped first: A B last: A { semantics { precedence { B > A } } }
        entry Root
    "#;
    let language = engine.compile("language.yl", &sources(grammar)).unwrap();
    let result = engine.analyze(&language, "source", "aaba");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    // All A matches in the current region wait for B, including the wrapped A.
    assert_eq!(
        visit_order(&result),
        [
            "language.yl#B",
            "language.yl#A",
            "language.yl#A",
            "language.yl#A"
        ]
    );
}

#[test]
fn explicit_pattern_groups_differently_wrapped_declarations_as_peers() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        node A = "a" { semantics { action() } }
        node Wrapped = "(" A ")"
        pattern Declaration = A | Wrapped
        node B = "b" { semantics { action() } }
        node Root = Declaration Declaration B { semantics { precedence { B > Declaration } } }
        entry Root
    "#;
    let language = engine.compile("language.yl", &sources(grammar)).unwrap();
    let result = engine.analyze(&language, "source", "(a)ab");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        visit_order(&result),
        ["language.yl#B", "language.yl#A", "language.yl#A"]
    );
    assert_eq!(
        result.occurrences[0].children[0].definition,
        "language.yl#Declaration"
    );
    assert_eq!(
        result.occurrences[0].children[1].definition,
        "language.yl#Declaration"
    );
}

#[test]
fn owning_node_is_selectable_without_hiding_its_initializer_selector() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        node Initializer = "i" { semantics { action() } }
        node Declaration = Initializer {
            semantics { action() precedence { Initializer > Declaration } }
        }
        node Root = Declaration
        entry Root
    "#;
    let language = engine.compile("language.yl", &sources(grammar)).unwrap();
    let result = engine.analyze(&language, "source", "i");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        visit_order(&result),
        ["language.yl#Initializer", "language.yl#Declaration"]
    );
}

#[test]
fn plain_pattern_wrappers_do_not_create_false_region_boundaries() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        node A = "a" { semantics { action() } }
        pattern Wrapped = A
        node B = "b" { semantics { action() } }
        node Root = Wrapped first: A B last: A { semantics { precedence { B > A } } }
        entry Root
    "#;
    let language = engine.compile("language.yl", &sources(grammar)).unwrap();
    let result = engine.analyze(&language, "source", "aaba");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    // All A matches in the current region wait for B, including the wrapped A.
    assert_eq!(
        visit_order(&result),
        [
            "language.yl#B",
            "language.yl#A",
            "language.yl#A",
            "language.yl#A"
        ]
    );
}

#[test]
fn nested_named_functions_are_not_outer_function_declaration_peers() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        node FunctionDeclaration = "f" "{" body: FunctionBody "}" { semantics { action() } }
        node FunctionBody = FunctionDeclaration* "r" { semantics { boundary action() precedence { FunctionDeclaration > FunctionBody } } }
        node Later = "l" { semantics { action() } }
        node Root = FunctionDeclaration Later {
            semantics { precedence { FunctionDeclaration > Later > FunctionBody } }
        }
        entry Root
    "#;
    let language = engine.compile("language.yl", &sources(grammar)).unwrap();
    let result = engine.analyze(&language, "source", "f{f{r}r}l");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        visit_order(&result),
        [
            "language.yl#FunctionDeclaration",
            "language.yl#Later",
            "language.yl#FunctionDeclaration",
            "language.yl#FunctionBody",
            "language.yl#FunctionBody"
        ]
    );
}

#[test]
fn qualified_named_groups_search_the_region_and_order_only_the_selected_group() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        node A = "a" { semantics { group Early { action() } group Late { action() } } }
        node Wrapped = A
        node B = "b" { semantics { action() } }
        node Root = Wrapped A B { semantics { precedence { B > A::semantics::Late } } }
        entry Root
    "#;
    let language = engine.compile("language.yl", &sources(grammar)).unwrap();
    let result = engine.analyze(&language, "source", "aab");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        visit_order(&result),
        [
            "language.yl#A",
            "language.yl#A",
            "language.yl#B",
            "language.yl#A",
            "language.yl#A"
        ]
    );
}

#[test]
fn imported_aliases_share_canonical_identity_in_region_group_selection() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let modules = BTreeMap::from([
        (
            "names.yl".into(),
            r#"
            import { action } from "test/native"
            export node Item = "x" { semantics { group Emit { action() } } }
        "#
            .into(),
        ),
        (
            "language.yl".into(),
            r#"
            import { action } from "test/native"
            import { Item as First, Item as Second } from "./names"
            node Wrapped = First
            node B = "b" { semantics { action() } }
            node Root = Wrapped First B Second {
                semantics { precedence { B > First::semantics::Emit } }
            }
            entry Root
        "#
            .into(),
        ),
    ]);
    let language = engine.compile("language.yl", &modules).unwrap();
    let result = engine.analyze(&language, "source", "xxbx");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        visit_order(&result),
        [
            "language.yl#B",
            "names.yl#Item",
            "names.yl#Item",
            "names.yl#Item"
        ]
    );
}

#[test]
fn node_and_pattern_boundaries_are_selectable_but_hide_their_contents() {
    for construct in ["node", "pattern"] {
        let events = Events::default();
        let engine = recording_engine(&events, None);
        let grammar = format!(
            r#"
            import {{ error, warn, hint }} from "core/diagnostics"
            import {{ action }} from "test/native"
            node A = "a" {{ semantics {{ action() }} }}
            {construct} Region = A {{ semantics {{ boundary }} }}
            node B = "b" {{ semantics {{ action() }} }}
            node Root = Region A B {{ semantics {{ boundary precedence {{ B > A }} }} }}
            entry Root
        "#
        );
        let language = engine.compile("language.yl", &sources(&grammar)).unwrap();
        let result = engine.analyze(&language, "source", "aab");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        // The owner boundary does not hide its interior. Nested Region DOES hide
        // its A, so only the exterior A waits for B in this clause.
        assert_eq!(
            visit_order(&result),
            ["language.yl#A", "language.yl#B", "language.yl#A"]
        );
        let grammar = grammar.replace("B > A", "B > Region");
        let language = engine.compile("language.yl", &sources(&grammar)).unwrap();
        let result = engine.analyze(&language, "source", "aab");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        // The boundary occurrence itself can be selected, deferring ALL its work.
        assert_eq!(
            visit_order(&result),
            ["language.yl#A", "language.yl#B", "language.yl#A"]
        );
        let boundary = &result.occurrences[0].children[0];
        let exterior = &result.occurrences[0].children[1];
        let owners: Vec<_> = result
            .graph
            .attachments
            .iter()
            .map(|a| a.occurrence)
            .collect();
        assert_eq!(owners[0], exterior.id);
        assert_eq!(owners[2], boundary.children[0].id);
        let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap()["meanings"]["language.yl#Region"]["boundary"],
            true
        );
        assert_eq!(result, engine.analyze(&loaded, "source", "aab"));
    }
}

#[test]
fn boundary_entry_waits_for_outer_predecessors_before_local_ordering() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        node Reference = "r" { semantics { action() } }
        node Body = Reference { semantics { boundary action() precedence { Reference > Body } } }
        node Outer = "o" { semantics { action() } }
        node Root = Body Outer { semantics { precedence { Outer > Body } } }
        entry Root
    "#;
    let language = engine.compile("language.yl", &sources(grammar)).unwrap();
    let result = engine.analyze(&language, "source", "ro");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        visit_order(&result),
        [
            "language.yl#Outer",
            "language.yl#Reference",
            "language.yl#Body"
        ]
    );
}

#[test]
fn repeated_boundary_marker_is_a_structured_definition_error() {
    let engine = SemanticEngine::default();
    let diagnostics = engine
        .compile(
            "language.yl",
            &sources("node Root = \"r\" { semantics { boundary boundary } } entry Root"),
        )
        .unwrap_err();
    assert_eq!(diagnostics[0].code, "yl.duplicate_boundary");
}
