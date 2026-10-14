use std::collections::BTreeMap;
use your_language::semantics::{
    AnalysisResult, SemanticEngine, SemanticIdentity, SemanticKey, SemanticValue,
};
fn analyze(source: &str, input: &str) -> AnalysisResult {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source.into())]),
        )
        .unwrap();
    let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&language, "program", input);
    assert_eq!(result, engine.analyze(&reloaded, "program", input));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    result
}
fn type_at(result: &AnalysisResult, key: SemanticValue) -> SemanticValue {
    result
        .graph
        .records
        .iter()
        .find(|record| {
            record
                .entries
                .iter()
                .any(|(k, v)| *k == SemanticKey::Symbol("std/types#key".into()) && *v == key)
        })
        .unwrap()
        .entries
        .iter()
        .find(|(k, _)| *k == SemanticKey::Symbol("std/types#type".into()))
        .unwrap()
        .1
        .clone()
}
fn occurrence(id: usize) -> SemanticValue {
    SemanticValue::Identity(SemanticIdentity::Occurrence(id))
}
fn atom(name: &str) -> SemanticValue {
    SemanticValue::Symbol(format!("language.yl#{name}"))
}
#[test]
fn self_inside_authored_operation_is_the_callers_original_pattern_occurrence() {
    let result = analyze(
        r#"
import * as types from "std/types"
symbol number
operation annotate(value) { types::annotate(self, value) }
pattern Item = "x" { meanings { annotate(number) } }
node Program = item: Item { meanings { types::annotate(self, types::typeof(item)) } }
entry Program
"#,
        "x",
    );
    let root = &result.occurrences[0];
    assert_eq!(type_at(&result, occurrence(root.id)), atom("number"));
    assert_eq!(
        type_at(&result, occurrence(root.captures["item"].occurrences[0])),
        atom("number")
    );
}
#[test]
fn direct_repeated_optional_and_dotted_pattern_captures_are_addresses() {
    let result = analyze(
        r#"
import * as types from "std/types"
import { separatedBy } from "std/parser"
symbol number
symbol parameters
pattern Item = "x" { meanings { types::annotate(self, number) } }
pattern Contents = "(" parameters: Item* |> separatedBy(",") ")"
node Root = contents: Contents absentItem: Item? {
    meanings {
        types::annotate((self, parameters), types::typeof(contents.parameters))
        types::annotate(self, types::typeof(absentItem))
    }
}
entry Root
"#,
        "(x,x)",
    );
    let root = &result.occurrences[0];
    assert_eq!(
        type_at(
            &result,
            SemanticValue::Tuple(vec![occurrence(root.id), atom("parameters")])
        ),
        SemanticValue::List(vec![atom("number"), atom("number")])
    );
}
#[test]
fn declaration_and_use_record_the_exact_name_capture_with_shadowing_and_dollar_prefix() {
    let result = analyze(
        r#"
import { lexicalScope, declare, use, referenceOf } from "std/references"
import * as types from "std/types"
symbol outer
symbol inner
pattern Name = /[a-z]+/
node DeclareOuter = "$" name: Name { meanings { declare(name) types::annotate(referenceOf(name), outer) } }
node DeclareInner = "$" name: Name { meanings { declare(name) types::annotate(referenceOf(name), inner) } }
node Reference = name: Name { meanings { use(name) types::annotate(self, types::typeof(referenceOf(name))) } }
node Block = "{" declaration: DeclareInner ";" value: Reference "}" { meanings { lexicalScope() } }
node Root = declaration: DeclareOuter ";" block: Block ";" value: Reference { meanings { lexicalScope() } }
entry Root
"#,
        "$x;{$x;x};x",
    );
    let root = &result.occurrences[0];
    let outer_id = root.captures["value"].occurrences[0];
    let block = root
        .children
        .iter()
        .find(|node| node.definition.ends_with("#Block"))
        .unwrap();
    let inner_id = block.captures["value"].occurrences[0];
    assert_eq!(type_at(&result, occurrence(outer_id)), atom("outer"));
    assert_eq!(type_at(&result, occurrence(inner_id)), atom("inner"));
}

#[test]
fn hypothetical_parameter_assumptions_produce_independent_returns_without_mutation() {
    let result = analyze(
        r#"
import * as types from "std/types"
symbol number
symbol string
symbol result
pattern Parameter = "p"
pattern Body = "b" source: Parameter {
    meanings { types::annotate((self, result), types::typeof(source)) }
}
node Root = body: Body {
    meanings {
        types::annotate(self, types::resolveWith((body, result), [(body.source, number)]))
        types::annotate((self, result), types::resolveWith((body, result), [(body.source, string)]))
    }
}
entry Root
"#,
        "bp",
    );
    let root = &result.occurrences[0];
    assert_eq!(type_at(&result, occurrence(root.id)), atom("number"));
    assert_eq!(
        type_at(
            &result,
            SemanticValue::Tuple(vec![occurrence(root.id), atom("result")])
        ),
        atom("string")
    );
    let parameter = &root.children[0].captures["source"];
    assert_eq!(
        type_at(&result, occurrence(parameter.occurrences[0])),
        SemanticValue::Symbol("std/types#unknown".into())
    );
}

#[test]
fn hypothetical_resolution_replays_declarative_type_matches_and_preserves_outer_values() {
    let result = analyze(
        r#"
import * as types from "std/types"
symbol number
symbol string
symbol output
pattern Parameter = "p"
pattern Outer = "o" { meanings { types::annotate(self, number) } }
pattern Addition = left: Parameter right: Outer {
    meanings {
        match (types::typeof(left), types::typeof(right)) {
            number, number => types::annotate((self, output), number),
            string, number => types::annotate((self, output), string),
            _ => {}
        }
    }
}
node Root = addition: Addition {
    meanings {
        types::annotate(self, types::resolveWith((addition, output), [(addition.left, string)]))
    }
}
entry Root
"#,
        "po",
    );
    let root = &result.occurrences[0];
    assert_eq!(type_at(&result, occurrence(root.id)), atom("string"));
    assert_eq!(
        type_at(
            &result,
            occurrence(root.children[0].captures["right"].occurrences[0])
        ),
        atom("number")
    );
}

#[path = "../examples/mini_typescript/definition.rs"]
mod definition;
#[path = "../examples/mini_typescript/inference.rs"]
mod inference;
fn mini(source: &str) -> AnalysisResult {
    let engine = inference::engine();
    let language = engine
        .compile(definition::ENTRY, &definition::sources())
        .unwrap();
    let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&reloaded, "program", source);
    assert_eq!(result, engine.analyze(&language, "program", source));
    result
}
fn binding(result: &AnalysisResult, name: &str) -> String {
    result
        .graph
        .records
        .iter()
        .find_map(|record| {
            if record.entries.contains(&(
                SemanticKey::Symbol("std/variables#nameSymbol".into()),
                SemanticValue::Text(name.into()),
            )) {
                inference::display_type(record)
            } else {
                None
            }
        })
        .unwrap()
}
#[test]
fn nested_calls_preserve_per_use_types_and_captured_outer_binding() {
    let result = mini(
        r#"
let outside = 10;
function identity(value) { return value; }
function outer(value) { return identity(value) + outside; }
let first = outer(1);
let second = outer("hello");
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(binding(&result, "first"), "number");
    assert_eq!(binding(&result, "second"), "string");
    assert_eq!(binding(&result, "outside"), "number");
    assert_eq!(binding(&result, "identity"), "(unknown) => unknown");
    assert_eq!(binding(&result, "outer"), "(unknown) => unknown");
}
#[test]
fn invalid_type_case_has_call_argument_provenance_and_does_not_disable_other_checks() {
    let source = r#"function add(value) { return value + 1; } let bad = add(true); let other: number = "wrong";"#;
    let result = mini(source);
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
    let invalid = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "semantic.meaning_case")
        .unwrap_or_else(|| panic!("{:?}", result.diagnostics));
    assert_eq!(&source[invalid.primary.start..invalid.primary.end], "true");
    let mismatch = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "types.mismatch")
        .unwrap();
    assert_eq!(
        &source[mismatch.primary.start..mismatch.primary.end],
        "\"wrong\""
    );
}
#[test]
fn duplicate_hypothetical_assumptions_are_structured_diagnostics() {
    let engine = SemanticEngine::default();
    let language=engine.compile("language.yl",&BTreeMap::from([("language.yl".into(),r#"
import * as types from "std/types"
symbol number
node Root = "x" { meanings { types::annotate(self,types::resolveWith(self,[(self,number),(self,number)])) } }
entry Root
"#.into())])).unwrap();
    let result = engine.analyze(&language, "program", "x");
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "types.assumptions"));
}

#[test]
fn hypothetical_queries_never_replay_arbitrary_native_effects() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use your_language::{
        diagnostic::Diagnostic,
        semantics::{NativeOperation, OperationContext, OperationSignature},
    };
    struct Effect(Arc<AtomicUsize>);
    impl NativeOperation for Effect {
        fn before(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let mut engine = SemanticEngine::default();
    let counter = count.clone();
    engine
        .registry_mut()
        .register(
            "host",
            "effect",
            OperationSignature {
                id: "host#effect".into(),
                parameters: vec![],
                provides_context: false,
                finalization_only: false,
                contribution: false,
            },
            move || Box::new(Effect(counter.clone())),
        )
        .unwrap();
    let language=engine.compile("language.yl",&BTreeMap::from([("language.yl".into(),r#"
import * as types from "std/types"
import { unknown } from "std/types"
import { effect } from "host"
symbol number
symbol result
pattern Parameter = "p"
pattern Body = parameter: Parameter {
    meanings {
        match (types::typeof(parameter)) {
            number => { types::annotate((self,result),number) effect() },
            _ => {}
        }
    }
}
node Root = body: Body { meanings { types::annotate(self,types::resolveWith((body,result),[(body.parameter,number)])) } }
entry Root
"#.into())])).unwrap();
    let result = engine.analyze(&language, "program", "p");
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "types.hypothetical_effect"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn projections_through_repeated_parents_preserve_nested_list_groups() {
    let result = analyze(
        r#"
import * as types from "std/types"
import { separatedBy } from "std/parser"
symbol number
symbol groups
pattern Item = "x" { meanings { types::annotate(self,number) } }
pattern Contents = "(" parameters: Item* |> separatedBy(",") ")"
node Root = contents: Contents* {
    meanings { types::annotate((self,groups),types::typeof(contents.parameters)) }
}
entry Root
"#,
        "(x,x)(x)",
    );
    let root = &result.occurrences[0];
    assert_eq!(
        type_at(
            &result,
            SemanticValue::Tuple(vec![occurrence(root.id), atom("groups")])
        ),
        SemanticValue::List(vec![
            SemanticValue::List(vec![atom("number"), atom("number")]),
            SemanticValue::List(vec![atom("number")])
        ])
    );
}

#[test]
fn hypothetical_union_keeps_supported_results_alongside_error_diagnostics() {
    let result = mini(
        r#"
function add(value) { return value + 1; }
let mixed: number = true;
let result = add(mixed);
"#,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "semantic.meaning_case"),
        "{:?}",
        result.diagnostics
    );
    assert_eq!(binding(&result, "result"), "number");
}

#[test]
fn direct_hypothetical_match_queries_report_errors_and_keep_valid_combinations() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([(
                "language.yl".into(),
                r#"
import * as types from "std/types"
import { unknown } from "std/types"
symbol number
symbol boolean
symbol result
pattern Parameter = "p"
pattern Actual = "a" { meanings { types::annotate(self,number) types::annotate(self,boolean) } }
pattern Body = parameter: Parameter {
    meanings {
        match (types::typeof(parameter)) {
            number => types::annotate((self,result),number),
            unknown => {},
            _ => error("Unsupported parameter")
        }
    }
}
node Root = body: Body actual: Actual {
    meanings {
        match (types::resolveWith((body,result),[(body.parameter,types::typeof(actual))])) {
            number => types::annotate(self,number),
            _ => {}
        }
    }
}
entry Root
"#
                .into(),
            )]),
        )
        .unwrap();
    let result = engine.analyze(&language, "program", "pa");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "Unsupported parameter"),
        "{:?}",
        result.diagnostics
    );
    assert_eq!(
        type_at(&result, occurrence(result.occurrences[0].id)),
        atom("number")
    );
}

#[test]
fn rejected_direct_hypothetical_query_has_no_internal_unsettled_diagnostic() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([(
                "language.yl".into(),
                r#"
import * as types from "std/types"
import { unknown } from "std/types"
symbol number
symbol boolean
symbol result
pattern Parameter = "p"
pattern Body = parameter: Parameter {
    meanings {
        match (types::typeof(parameter)) {
            number => types::annotate((self,result),number),
            unknown => {},
            _ => error("Unsupported parameter")
        }
    }
}
node Root = body: Body {
    meanings {
        match (types::resolveWith((body,result),[(body.parameter,boolean)])) {
            number => types::annotate(self,number),
            _ => {}
        }
    }
}
entry Root
"#
                .into(),
            )]),
        )
        .unwrap();
    let result = engine.analyze(&language, "program", "p");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].message, "Unsupported parameter");
}

#[test]
fn hypothetical_warning_contributions_preserve_the_result_without_effect_replay() {
    let engine = SemanticEngine::default();
    let language=engine.compile("language.yl",&BTreeMap::from([("language.yl".into(),r#"
import * as types from "std/types"
import { unknown } from "std/types"
symbol number
symbol boolean
symbol result
pattern Parameter = "p"
pattern Body = parameter: Parameter {
    meanings {
        match (types::typeof(parameter)) {
            number => types::annotate((self,result),number),
            unknown => {},
            _ => { types::annotate((self,result),number) warn("Fallback used") }
        }
    }
}
node Root = body: Body { meanings { types::annotate(self,types::resolveWith((body,result),[(body.parameter,boolean)])) } }
entry Root
"#.into())])).unwrap();
    let result = engine.analyze(&language, "program", "p");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(
        result.diagnostics[0].severity,
        your_language::diagnostic::Severity::Warning
    );
    assert_eq!(result.diagnostics[0].message, "Fallback used");
    assert_eq!(
        type_at(&result, occurrence(result.occurrences[0].id)),
        atom("number")
    );
}
