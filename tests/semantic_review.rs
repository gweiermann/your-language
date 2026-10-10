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
        node Root = "x" { meanings { action() } }
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
        node Child = "c" { meanings { action() } }
        node Root = "{" child: Child "}" { meanings { scope() } }
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
    for (severity, expected_calls) in [("error", 0), ("warning", 1), ("help", 1)] {
        let events = Events::default();
        let engine = recording_engine(&events, None);
        let grammar = format!(
            r#"
            import {{ action }} from "test/native"
            node Root = "x" {{
                constraints {{ when true {{ {severity}("Local validation") }} }}
                meanings {{ action() }}
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
fn descendant_ordering_diagnoses_scope_provider_cycles() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        import { lexicalScope, declareVariable } from "std/semantics"
        node LetDeclaration = "let" name: /[a-z]+/ ";" {
            meanings { declareVariable(name) }
        }
        node FunctionBody = "{" children: LetDeclaration* "}" {
            meanings { lexicalScope() }
        }
        node Program = body: FunctionBody {
            meanings {
                lexicalScope()
                precedence { LetDeclaration > FunctionBody }
            }
        }
        entry Program
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "{letinner;}");
    assert!(result.ast.is_some());
    assert_eq!(result.diagnostics[0].code, "semantic.precedence_cycle");
    assert!(!result.diagnostics[0].secondary.is_empty());
    assert!(result.graph.records.is_empty());
    assert!(result.graph.attachments.is_empty());
}

#[test]
fn enclosing_pattern_scope_is_independent_of_deferred_body_operations() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        import { lexicalScope, declareVariable, use } from "std/semantics"
        node LetDeclaration = "let" name: /[a-z]+/ ";" {
            meanings { declareVariable(name) }
        }
        node FunctionBody = "{" children: LetDeclaration* "return" returnName: /[a-z]+/ ";}" {
            meanings { use(returnName) }
        }
        pattern FunctionContents = FunctionBody { meanings { lexicalScope() } }
        node Program = body: FunctionContents {
            meanings {
                lexicalScope()
                precedence { LetDeclaration > FunctionBody }
            }
        }
        entry Program
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "{letinner;returninner;}");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let declaration = result
        .graph
        .attachments
        .iter()
        .find(|a| a.key == "std/semantics#declaration")
        .unwrap();
    let reference = result
        .graph
        .attachments
        .iter()
        .find(|a| a.key == "std/semantics#reference")
        .unwrap();
    assert_eq!(declaration.value, reference.value);
    assert_ne!(declaration.occurrence, reference.occurrence);
    let again = engine.analyze(&language, "source", "{letinner;returninner;}");
    assert_eq!(result, again);
}

#[test]
fn committed_original_node_and_pattern_actions_survive_pipe_projection_once() {
    let events = Events::default();
    let engine = recording_engine(&events, None);
    let grammar = r#"
        import { action } from "test/native"
        import { separatedBy } from "std/parser"
        import { notAhead } from "core/parser"
        node Failed = "a" "!" { meanings { action() } }
        node Accepted = "a" { meanings { action() } }
        pattern Item = Failed | Accepted { meanings { action() } }
        node Separator = "," { meanings { action() } }
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
            node Atom = "a" { meanings { action() } }
            node Sum = left: E "+" right: E { meanings { action() } }
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
            export node Name = "x" { meanings { action() } }
        "#
            .into(),
        ),
        (
            "language.yl".into(),
            r#"
            import { action } from "test/native"
            import { Name as Renamed } from "./names"
            node Family { node Renamed meanings { action() } }
            node Root = direct: Renamed family: Family member: Family::Renamed {
                meanings { action() }
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

fn generic_references() -> (SemanticEngine, your_language::CompiledLanguage) {
    generic_references_with_top_level_role(false)
}

fn generic_references_with_top_level_role(
    top_level_role: bool,
) -> (SemanticEngine, your_language::CompiledLanguage) {
    let events = Events::default();
    let mut registry = your_language::semantics::lexical::registry();
    let captured = events.clone();
    registry
        .register(
            "test/native",
            "action",
            OperationSignature {
                id: "test/native#action".into(),
                parameters: vec![],
                provides_context: false,
            },
            move || {
                Box::new(Recording {
                    events: captured.clone(),
                    name: "body",
                    enters: 0,
                    fail_enter: None,
                })
            },
        )
        .unwrap();
    let engine = SemanticEngine::new(registry);
    let grammar = r#"
        import { lexicalScope, declareVariable, use } from "std/semantics"
        import { action } from "test/native"
        node Name = /[a-z]+/
        node Reference = name: Name { meanings { use(name) } }
        node FunctionBody = "{" locals: LetDeclaration* "return" value: Reference ";}" { meanings { action() } }
        pattern FunctionContents = FunctionBody { meanings { lexicalScope() } }
        node FunctionExpression = "function()" body: FunctionContents
        node Expression { node FunctionExpression node Reference node Number = /[0-9]+/ }
        node LetDeclaration = "let" name: Name "=" value: Expression ";" {
            meanings { declareVariable(name) }
        }
        node Program = declarations: LetDeclaration* {
            meanings { lexicalScope() precedence { LetDeclaration > FunctionBody } }
        }
        entry Program
    "#;
    let grammar = if top_level_role {
        grammar.replace("node Program = declarations: LetDeclaration*", "pattern TopLevelDeclaration = LetDeclaration\nnode Program = declarations: TopLevelDeclaration*")
            .replace("precedence { LetDeclaration > FunctionBody }", "precedence { TopLevelDeclaration > FunctionBody }")
    } else {
        grammar.into()
    };
    let language = engine.compile("language.yl", &sources(&grammar)).unwrap();
    (engine, language)
}

#[test]
fn generic_reference_can_bind_to_the_enclosing_function_variable() {
    let (engine, language) = generic_references();
    // Self-reference alone does not prove deferral. The separately
    // later-declaration test also verifies body deferral.
    let result = engine.analyze(&language, "source", "letf=function(){returnf;};");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let declaration = result
        .graph
        .attachments
        .iter()
        .find(|a| a.key == "std/semantics#declaration")
        .unwrap();
    let reference = result
        .graph
        .attachments
        .iter()
        .find(|a| a.key == "std/semantics#reference")
        .unwrap();
    assert_eq!(declaration.value, reference.value);
}

#[test]
// Regression for the confirmed whole-construct traversal rule. The earlier
// own-group-only implementation reported unresolved_reference at 22..27.
fn deferred_body_delays_generic_reference_operations_until_outer_declarations() {
    let (engine, language) = generic_references();
    let result = engine.analyze(
        &language,
        "source",
        "letf=function(){returnlater;};letlater=1;",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
// Paired with the body-deferral case. The earlier preorder priority registered
// `a` before its initializer Reference. Desired traversal must visit the ordinary
// initializer first, while allowing deferred function-body references later.
fn ordinary_initializer_reference_does_not_bind_to_its_own_declaration() {
    let (engine, language) = generic_references();
    let result = engine.analyze(&language, "source", "leta=a;");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, "semantic.unresolved_reference");
    assert_eq!(result.diagnostics[0].primary.start, 5);
    assert_eq!(result.diagnostics[0].primary.end, 6);
    assert!(!result
        .graph
        .attachments
        .iter()
        .any(|a| a.key == "std/semantics#reference"));
}

#[test]
fn whole_node_and_pattern_deferral_delay_their_generic_children() {
    for construct in ["node", "pattern"] {
        let events = Events::default();
        let engine = recording_engine(&events, None);
        let grammar = format!(
            r#"
            import {{ action }} from "test/native"
            node Child = "c" {{ meanings {{ action() }} }}
            {construct} Deferred = Child {{ meanings {{ action() }} }}
            node Early = "e" {{ meanings {{ action() }} }}
            node Root = Deferred Early {{ meanings {{ precedence {{ Early > Deferred }} }} }}
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
            .filter_map(
                |attachment| match (&attachment.key[..], &attachment.value) {
                    (
                        "test/native#visit",
                        your_language::semantics::SemanticValue::Text(definition),
                    ) => Some(definition.as_str()),
                    _ => None,
                },
            )
            .collect();
        assert_eq!(
            order,
            [
                "language.yl#Early",
                "language.yl#Child",
                "language.yl#Deferred"
            ],
            "{construct}"
        );
    }
}

#[test]
fn deferred_generic_reference_sees_later_outer_bindings_and_local_shadowing() {
    let (engine, language) = generic_references();
    for source in [
        "letf=function(){letinner=1;returnlater;};letlater=1;",
        "letf=function(){letf=1;returnf;};",
    ] {
        let result = engine.analyze(&language, "source", source);
        assert!(
            result.diagnostics.is_empty(),
            "{source}: {:?}",
            result.diagnostics
        );
        let reference = result
            .graph
            .attachments
            .iter()
            .find(|a| a.key == "std/semantics#reference")
            .unwrap();
        let your_language::semantics::SemanticValue::Record(binding) = reference.value else {
            panic!("Expected reference to binding")
        };
        let record = &result.graph.records[binding];
        let origin = record
            .entries
            .iter()
            .find_map(|(key, value)| match (key, value) {
                (
                    your_language::semantics::SemanticKey::Symbol(key),
                    your_language::semantics::SemanticValue::Source(origin),
                ) if key == "std/semantics#originSymbol" => Some(origin),
                _ => None,
            })
            .unwrap();
        let expected = if source.contains("returnlater") {
            source.find("letlater").unwrap() + 3
        } else {
            source.rfind("letf").unwrap() + 3
        };
        assert_eq!(origin.span.start, expected, "{source}");
    }
}

#[test]
fn explicit_earlier_local_declaration_preserves_ordinary_initializer_order() {
    let (engine, language) = generic_references();
    let source = "letf=function(){leta=a;returnf;};";
    let result = engine.analyze(&language, "source", source);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "semantic.unresolved_reference");
    let position = source.find("leta=a").unwrap() + 5;
    assert_eq!(result.diagnostics[0].primary.start, position);
    assert_eq!(result.diagnostics[0].primary.end, position + 1);
    assert_eq!(
        result
            .graph
            .attachments
            .iter()
            .filter(|a| a.key == "std/semantics#reference")
            .count(),
        1
    );
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
            import {{ action }} from "test/native"
            node Declaration = "d" {{ meanings {{ action() }} }}
            node Reference = "r" {{ meanings {{ action() }} }}
            pattern Inner = Reference Declaration {{ meanings {{ precedence {{ {inner_chain} }} }} }}
            node Root = Inner {{ meanings {{ precedence {{ Declaration > Reference }} }} }}
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
fn equally_named_categories_from_distinct_libraries_keep_distinct_identity() {
    let mut engine = SemanticEngine::default();
    let first = engine
        .registry_mut()
        .export_symbol("native/first", "category")
        .unwrap();
    let second = engine
        .registry_mut()
        .export_symbol("native/second", "category")
        .unwrap();
    let language = engine
        .compile(
            "language.yl",
            &sources(
                r#"
        import { lexicalScope, declare } from "std/semantics"
        import { category as First } from "native/first"
        import { category as Second } from "native/second"
        node A = name: "a" { meanings { declare(First, name) } }
        node B = name: "b" { meanings { declare(Second, name) } }
        node Root = A B { meanings { lexicalScope() } }
        entry Root
    "#,
            ),
        )
        .unwrap();
    let result = engine.analyze(&language, "source", "ab");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let mut categories: Vec<_> = result
        .graph
        .records
        .iter()
        .flat_map(|record| &record.entries)
        .filter_map(|(key, value)| match (key, value) {
            (
                your_language::semantics::SemanticKey::Symbol(key),
                your_language::semantics::SemanticValue::Symbol(category),
            ) if key == "std/semantics#kindSymbol" => Some(category.clone()),
            _ => None,
        })
        .collect();
    categories.sort();
    assert_ne!(first, second);
    assert_eq!(categories, [first, second]);
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
        node Root = A { meanings { precedence { A > A } } }
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
            import {{ action }} from "test/native"
            node Child = "c" {{ meanings {{ action() }} }}
            {construct} Deferred = Child
            node Early = "e" {{ meanings {{ action() }} }}
            node Root = Deferred Early {{ meanings {{ precedence {{ Early > Deferred }} }} }}
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
        node Child = "x" { meanings { action() } }
        node A = Child { meanings { action() } }
        node C = A { meanings { action() } }
        node B = "b" { meanings { action() } }
        node Root = C B { meanings { precedence { A > B > C } } }
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
        node Reference = "r" { meanings { action() } }
        node Body = Reference {
            meanings { action() precedence { Reference > Body } }
        }
        node Outer = "o" { meanings { action() } }
        node Root = Body Outer { meanings { precedence { Outer > Body } } }
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
fn lexical_name_arguments_observe_preserved_pipe_values() {
    for construct in ["node", "pattern"] {
        let engine = SemanticEngine::default();
        let grammar = format!(
            r#"
            import {{ lexicalScope, declareVariable, use }} from "std/semantics"
            pipe wrapped() {{ rewrite value => "(" value ")" }}
            {construct} Name = value: /[a-z]+/
            node Declaration = "let" name: Name |> wrapped() ";" {{ meanings {{ declareVariable(name) }} }}
            node Reference = name: Name ";" {{ meanings {{ use(name) }} }}
            node Root = Declaration Reference {{ meanings {{ lexicalScope() }} }}
            entry Root
        "#
        );
        let language = engine.compile("language.yl", &sources(&grammar)).unwrap();
        let result = engine.analyze(&language, "source", "let(x);x;");
        assert!(
            result.diagnostics.is_empty(),
            "{construct}: {:?}",
            result.diagnostics
        );
        let declaration = result
            .graph
            .attachments
            .iter()
            .find(|a| a.key == "std/semantics#declaration")
            .unwrap();
        let reference = result
            .graph
            .attachments
            .iter()
            .find(|a| a.key == "std/semantics#reference")
            .unwrap();
        assert_eq!(declaration.value, reference.value);
        let your_language::semantics::SemanticValue::Record(binding) = declaration.value else {
            panic!("Expected declaration binding")
        };
        let origin = result.graph.records[binding]
            .entries
            .iter()
            .find_map(|(key, value)| match (key, value) {
                (
                    your_language::semantics::SemanticKey::Symbol(key),
                    your_language::semantics::SemanticValue::Source(origin),
                ) if key == "std/semantics#originSymbol" => Some(origin),
                _ => None,
            })
            .unwrap();
        assert_eq!(origin.text, "x", "{construct}");
        assert_eq!(origin.span.start, 4, "{construct}");
        assert_eq!(origin.span.end, 5, "{construct}");
    }
}

#[test]
fn sequential_ordinary_initializers_can_use_earlier_declarations() {
    let (engine, language) = generic_references();
    let result = engine.analyze(&language, "source", "letx=1;lety=x;");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let declaration = result
        .graph
        .attachments
        .iter()
        .find(|a| a.key == "std/semantics#declaration")
        .unwrap();
    let reference = result
        .graph
        .attachments
        .iter()
        .find(|a| a.key == "std/semantics#reference")
        .unwrap();
    assert_eq!(declaration.value, reference.value);
}

#[test]
fn closure_local_initializer_can_use_a_later_outer_declaration() {
    let (engine, language) = generic_references_with_top_level_role(true);
    let source = "letf=function(){letinner=later;returninner;};letlater=1;";
    let result = engine.analyze(&language, "source", source);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .graph
            .attachments
            .iter()
            .filter(|a| a.key == "std/semantics#reference")
            .count(),
        2
    );
}

#[test]
fn broad_descendant_precedence_explicitly_exempts_the_local_initializer() {
    let (engine, language) = generic_references();
    let source = "letf=function(){letinner=later;returninner;};letlater=1;";
    let result = engine.analyze(&language, "source", source);
    // Broad LetDeclaration > FunctionBody selects the inner let too, explicitly
    // pulling its initializer before deferred body traversal. A top-level role
    // pattern above expresses the policy that leaves body-local lets deferred.
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "semantic.unresolved_reference");
    assert_eq!(result.diagnostics[0].primary.start, 25);
    assert_eq!(result.diagnostics[0].primary.end, 30);
}
