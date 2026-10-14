use std::collections::BTreeMap;
use your_language::semantics::{AnalysisResult, SemanticEngine, SemanticIdentity, SemanticValue};
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
    use your_language::semantics::queries::QueryKey;
    fn convert(value: SemanticValue) -> QueryKey {
        match value {
            SemanticValue::Identity(id) => QueryKey::Identity(id),
            SemanticValue::Symbol(id) => QueryKey::Symbol(id),
            SemanticValue::Tuple(items) => {
                QueryKey::Tuple(items.into_iter().map(convert).collect())
            }
            _ => panic!("unexpected key"),
        }
    }
    let key = QueryKey::Tuple(vec![
        convert(key),
        QueryKey::Symbol("std/types#type".into()),
    ]);
    let mut values = vec![];
    for entry in &result.graph.query_entries {
        if entry.key == key && !values.contains(&entry.value) {
            values.push(entry.value.clone());
        }
    }
    match values.len() {
        0 => SemanticValue::Symbol("std/types#unknown".into()),
        1 => values.remove(0),
        _ => SemanticValue::Alternatives {
            service: "core/query".into(),
            values,
        },
    }
}
fn occurrence(id: usize) -> SemanticValue {
    SemanticValue::Identity(SemanticIdentity::Occurrence(id))
}
fn atom(name: &str) -> SemanticValue {
    SemanticValue::Symbol(format!("language.yl#{name}"))
}
#[test]
fn explicit_target_inside_authored_operation_is_the_original_pattern_occurrence() {
    let result = analyze(
        r#"
import * as types from "std/types"
import { map } from "core/query"
symbol number
operation annotate(target, value) { types::annotateWithType(target, value) }
pattern Item = "x" { semantics { annotate(self, number) } }
node Program = item: Item { semantics { types::annotateWithType(self, types::typeOf(item)) } }
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
import { map } from "core/query"
import { separatedBy } from "std/parser"
symbol number
symbol parameters
pattern Item = "x" { semantics { types::annotateWithType(self, number) } }
pattern Contents = "(" parameters: Item* |> separatedBy(",") ")"
node Root = contents: Contents absentItem: Item? {
    semantics {
        types::annotateWithType((self, parameters), map(contents.parameters, parameter => types::typeOf(parameter)))
        types::annotateWithType(self, types::typeOf(absentItem))
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
import { createLexicalScope, declareName, connectReference, referenceOf } from "std/references"
import * as types from "std/types"
import { map } from "core/query"
symbol outer
symbol inner
pattern Name = /[a-z]+/
node DeclareOuter = "$" name: Name { semantics { declareName(self, name) types::annotateWithType(referenceOf(name), outer) } }
node DeclareInner = "$" name: Name { semantics { declareName(self, name) types::annotateWithType(referenceOf(name), inner) } }
node Reference = name: Name { semantics { connectReference(self, name) types::annotateWithType(self, types::typeOf(referenceOf(name))) } }
node Block = "{" declaration: DeclareInner ";" value: Reference "}" { semantics { createLexicalScope(self) } }
node Root = declaration: DeclareOuter ";" block: Block ";" value: Reference { semantics { createLexicalScope(self) } }
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
import { map } from "core/query"
symbol number
symbol string
symbol result
pattern Parameter = "p"
pattern Body = "b" source: Parameter {
    semantics { types::annotateWithType((self, result), types::typeOf(source)) }
}
node Root = body: Body {
    semantics {
        types::annotateWithType(self, types::resolveTypeWith((body, result), [(body.source, number)]))
        types::annotateWithType((self, result), types::resolveTypeWith((body, result), [(body.source, string)]))
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
import { map } from "core/query"
symbol number
symbol string
symbol output
pattern Parameter = "p"
pattern Outer = "o" { semantics { types::annotateWithType(self, number) } }
pattern Addition = left: Parameter right: Outer {
    semantics {
        match (types::typeOf(left), types::typeOf(right)) {
            number, number => types::annotateWithType((self, output), number),
            string, number => types::annotateWithType((self, output), string),
            _ => {}
        }
    }
}
node Root = addition: Addition {
    semantics {
        types::annotateWithType(self, types::resolveTypeWith((addition, output), [(addition.left, string)]))
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
    inference::binding_types(result, name).join(" | ")
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
    assert_eq!(binding(&result, "first"), "Type::number");
    assert_eq!(binding(&result, "second"), "Type::string");
    assert_eq!(binding(&result, "outside"), "Type::number");
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
        .find(|diagnostic| diagnostic.message.contains("is not possible"))
        .unwrap_or_else(|| panic!("{:?}", result.diagnostics));
    assert_eq!(&source[invalid.primary.start..invalid.primary.end], "true");
    let mismatch = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("does not match"))
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
import { map } from "core/query"
symbol number
node Root = "x" { semantics { types::annotateWithType(self,types::resolveTypeWith(self,[(self,number),(self,number)])) } }
entry Root
"#.into())])).unwrap();
    let result = engine.analyze(&language, "program", "x");
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "query.assumption"));
}

#[test]
fn hypothetical_queries_never_replay_arbitrary_native_effects() {
    use your_language::semantics::{NativeOperation, OperationSignature};
    struct Effect;
    impl NativeOperation for Effect {}
    let mut engine = SemanticEngine::default();
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
            || Box::new(Effect),
        )
        .unwrap();
    let errors = engine
        .compile(
            "language.yl",
            &BTreeMap::from([(
                "language.yl".into(),
                r#"import { effect } from "host"
import * as types from "std/types"
symbol number
node Root = "x" { semantics { match (types::typeOf(self)) { number => effect(), _ => {} } } }
entry Root"#
                    .into(),
            )]),
        )
        .unwrap_err();
    assert!(errors
        .iter()
        .any(|diagnostic| diagnostic.code == "yl.query_native_effect"));
}

#[test]
fn projections_through_repeated_parents_preserve_nested_list_groups() {
    let result = analyze(
        r#"
import * as types from "std/types"
import { map } from "core/query"
import { separatedBy } from "std/parser"
symbol number
symbol groups
pattern Item = "x" { semantics { types::annotateWithType(self,number) } }
pattern Contents = "(" parameters: Item* |> separatedBy(",") ")"
node Root = contents: Contents* {
    semantics { types::annotateWithType((self,groups),map(contents.parameters, parameters => map(parameters, parameter => types::typeOf(parameter)))) }
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
    let mut sources = definition::sources();
    // Supply a genuine union contribution; a declared annotation is a
    // restriction and must not manufacture an extra observed type.
    let grammar = sources.get_mut("definition/expression.yl").unwrap();
    *grammar = grammar.replacen(
        "annotateWithType(self, Type::boolean)",
        "annotateWithType(self, Type::boolean)\n        annotateWithType(self, Type::number)",
        1,
    );
    let engine = SemanticEngine::default();
    let language = engine.compile(definition::ENTRY, &sources).unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let source = r#"
function add(value) { return value + 1; }
let mixed = true;
let result = add(mixed);
"#;
    let result = engine.analyze(&loaded, "test.ts", source);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("is not possible")),
        "{:?}",
        result.diagnostics
    );
    assert_eq!(binding(&result, "result"), "Type::number");
}

#[test]
fn direct_hypothetical_match_queries_report_errors_and_keep_valid_combinations() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([(
                "language.yl".into(),
                r#"import { error, warn, hint } from "core/diagnostics"

import * as types from "std/types"
import { map } from "core/query"
import { unknown } from "std/types"
symbol number
symbol boolean
symbol result
pattern Parameter = "p"
pattern Actual = "a" { semantics { types::annotateWithType(self,number) types::annotateWithType(self,boolean) } }
pattern Body = parameter: Parameter {
    semantics {
        match (types::typeOf(parameter)) {
            number => types::annotateWithType((self,result),number),
            unknown => {},
            _ => error("Unsupported parameter")
        }
    }
}
node Root = body: Body actual: Actual {
    semantics {
        match (types::resolveTypeWith((body,result),[(body.parameter,types::typeOf(actual))])) {
            number => types::annotateWithType(self,number),
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
                r#"import { error, warn, hint } from "core/diagnostics"

import * as types from "std/types"
import { map } from "core/query"
import { unknown } from "std/types"
symbol number
symbol boolean
symbol result
pattern Parameter = "p"
pattern Body = parameter: Parameter {
    semantics {
        match (types::typeOf(parameter)) {
            number => types::annotateWithType((self,result),number),
            unknown => {},
            _ => error("Unsupported parameter")
        }
    }
}
node Root = body: Body {
    semantics {
        match (types::resolveTypeWith((body,result),[(body.parameter,boolean)])) {
            number => types::annotateWithType(self,number),
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
    let language=engine.compile("language.yl",&BTreeMap::from([("language.yl".into(),r#"import { error, warn, hint } from "core/diagnostics"

import * as types from "std/types"
import { map } from "core/query"
import { unknown } from "std/types"
symbol number
symbol boolean
symbol result
pattern Parameter = "p"
pattern Body = parameter: Parameter {
    semantics {
        match (types::typeOf(parameter)) {
            number => types::annotateWithType((self,result),number),
            unknown => {},
            _ => { types::annotateWithType((self,result),number) warn("Fallback used") }
        }
    }
}
node Root = body: Body { semantics { types::annotateWithType(self,types::resolveTypeWith((body,result),[(body.parameter,boolean)])) } }
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

#[test]
fn terminal_captures_are_stable_scalar_list_and_optional_type_addresses() {
    let result = analyze(
        r#"
import * as types from "std/types"
import { map } from "core/query"
import { separatedBy } from "std/parser"
symbol number
symbol items
pipe wrapped() { rewrite value => "(" value ")" }
pattern Letter = /[a-z]/ { semantics { types::annotateWithType(self, number) } }
pattern Contents = values: Letter* |> separatedBy(",")
node Root = name: /[a-z]/ |> wrapped() ";" contents: Contents missing: /[0-9]/? {
    semantics {
        types::annotateWithType(name, number)
        types::annotateWithType(self, types::typeOf(name))
        types::annotateWithType((self, items), map(contents.values, value => types::typeOf(value)))
    }
}
entry Root
"#,
        "(x);a,b",
    );
    let root = &result.occurrences[0];
    assert_eq!(type_at(&result, occurrence(root.id)), atom("number"));
    assert_eq!(
        type_at(
            &result,
            SemanticValue::Tuple(vec![occurrence(root.id), atom("items")])
        ),
        SemanticValue::List(vec![atom("number"), atom("number")])
    );
    let name = &root.captures["name"];
    assert!(name.occurrences.is_empty());
    assert_eq!(name.text, "x");
    assert_eq!((name.span.start, name.span.end), (1, 2));
    assert!(matches!(
        name.occurrence_selection(),
        Some(your_language::semantics::OccurrenceSelection::Terminal(_))
    ));
    assert_eq!(
        root.captures["missing"].occurrence_selection(),
        Some(your_language::semantics::OccurrenceSelection::Absent)
    );
    let contents = root
        .children
        .iter()
        .find(|child| child.definition.ends_with("#Contents"))
        .unwrap();
    assert_eq!(
        root.captures["values"].occurrence_selection(),
        contents.captures["values"].occurrence_selection()
    );
    let selection = contents.captures["values"].occurrence_selection().unwrap();
    let your_language::semantics::OccurrenceSelection::List(values) = selection else {
        panic!("Expected a repeated terminal selection")
    };
    assert_eq!(values.len(), 2);
    assert_ne!(values[0], values[1]);
}
