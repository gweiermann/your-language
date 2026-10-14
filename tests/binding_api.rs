use std::collections::BTreeMap;
use your_language::semantics::SemanticEngine;

fn analyze(source: &str, input: &str) -> your_language::semantics::AnalysisResult {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source.into())]),
        )
        .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "input", input);
    assert_eq!(result, engine.analyze(&language, "input", input));
    result
}

#[test]
fn reference_contracts_reject_invalid_arguments_and_removed_categories() {
    let engine = SemanticEngine::default();
    for bad in [
        "v::identity(name)",
        "v::identityOf()",
        "v::declareVariable(name)",
        "v::declareFunction(name)",
        "v::useVariable(name)",
        "v::declare(v::variable,name)",
        "v::declare(name, reassignable=true, reassignable=false)",
        "v::declare(name, nope=true)",
    ] {
        let source = format!(
            r#"import * as v from "std/references"
node Root = name: /[a-z]+/ {{ meanings {{ {bad} }} }} entry Root"#
        );
        assert!(
            engine
                .compile(
                    "language.yl",
                    &BTreeMap::from([("language.yl".into(), source)])
                )
                .is_err(),
            "{bad}"
        );
    }
}

#[test]
fn syntax_and_resolved_reference_identities_remain_distinct_through_reload() {
    let result = analyze(
        r#"
import * as r from "std/references"
import { equal, isPresent, get } from "std/query"
node Declaration = "let " name: /[a-z]+/ ";" { meanings { r::declare(name, reassignable=false) } }
node Reference = name: /[a-z]+/ ";" { meanings { r::use(name) } }
node Root = declaration: Declaration reference: Reference {
 meanings {
  r::lexicalScope()
  if !equal(r::referenceOf(declaration), r::referenceOf(reference)) { error("Different references") }
  if equal(declaration, reference) { error("Different syntax must retain distinct identity") }
  if isPresent(r::identityOf(absent)) { error("Absent became an identity") }
  if !equal(get((r::referenceOf(reference), r::reassignableSymbol)), false) { error("Mutability lost") }
 }
}
entry Root
"#,
        "let x;x;",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(!result.graph.query_entries.is_empty());
}

#[test]
fn scope_search_handles_shadowing_and_sibling_isolation() {
    let grammar = r#"
import { lexicalScope, declare, use } from "std/references"
node Declaration = "let " name: /[a-z]+/ ";" { meanings { declare(name) } }
node Reference = name: /[a-z]+/ ";" { meanings { use(name) } }
node Block = "{" (Declaration | Reference | Block)* "}" { meanings { lexicalScope() } }
node Program = (Declaration | Reference | Block)* { meanings { lexicalScope() } }
entry Program
"#;
    for input in ["let x;x;", "let x;{x;let x;}x;", "{let x;x;}{let x;x;}"] {
        let result = analyze(grammar, input);
        assert!(
            result.diagnostics.is_empty(),
            "{input}: {:?}",
            result.diagnostics
        );
    }
    let forward = analyze(grammar, "x;let x;");
    assert!(forward
        .diagnostics
        .iter()
        .any(|error| error.message == "Unknown name 'x'"));
    let result = analyze(grammar, "{let x;}x;");
    let error = result
        .diagnostics
        .iter()
        .find(|error| error.message == "Unknown name 'x'")
        .unwrap();
    assert_eq!(error.primary.start, 8);
    assert_eq!(error.primary.end, 9);
    let duplicate = analyze(grammar, "let x;let x;");
    assert!(duplicate
        .diagnostics
        .iter()
        .any(|error| error.message == "Duplicate declaration 'x'"));
}

#[test]
fn node_and_pattern_names_keep_unwrapped_spans_through_pipes() {
    for construct in ["node", "pattern"] {
        let source = format!(
            r#"
import {{ lexicalScope, declare, use, referenceOf }} from "std/references"
import {{ equal }} from "std/query"
pipe wrapped() {{ rewrite value => "(" value ")" }}
{construct} Name = /[a-z]+/
node Declaration = "let" name: Name |> wrapped() ";" {{ meanings {{ declare(name) }} }}
node Reference = name: Name ";" {{ meanings {{ use(name) }} }}
node Program = declaration: Declaration reference: Reference {{ meanings {{
 lexicalScope()
 if !equal(referenceOf(declaration), referenceOf(reference)) {{ error("Pipe lost binding") }}
}} }}
entry Program
"#
        );
        let result = analyze(&source, "let(x);x;");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let declaration = result.occurrences[0]
            .children
            .iter()
            .find(|occurrence| occurrence.definition.ends_with("#Declaration"))
            .unwrap();
        assert_eq!(declaration.captures["name"].span.start, 4);
        assert_eq!(declaration.captures["name"].span.end, 5);
    }
}

#[test]
fn nested_functions_find_mutual_names_later_bindings_and_outer_captures() {
    let grammar = r#"
        import { lexicalScope, declare, use } from "std/references"
        node Name = /[a-z]+/
        trivia Spaces = /\s+/
        node Reference = name: Name { meanings { use(name) } }
        node Expression { node FunctionExpression node Reference node Number = /[0-9]+/ }
        node ExpressionDeclaration = Expression
        node LetDeclaration = "let" name: Name "=" ExpressionDeclaration ";" {
            meanings { declare(name) precedence { ExpressionDeclaration > LetDeclaration > FunctionBody } }
        }
        node FunctionBody = "{" (FunctionDeclaration | LetDeclaration)* "return" Reference ";" "}" {
            meanings { boundary precedence { FunctionDeclaration > LetDeclaration > FunctionBody } }
        }
        pattern FunctionContents = "(" ")" FunctionBody { meanings { lexicalScope() } }
        node FunctionExpression = "function" FunctionContents
        node FunctionDeclaration = "function" name: Name FunctionContents { meanings { declare(name) } }
        node Program = (FunctionDeclaration | LetDeclaration)* {
            meanings { boundary lexicalScope() precedence {
                FunctionDeclaration > ExpressionDeclaration
                FunctionDeclaration > LetDeclaration > FunctionBody
            } }
        }
        entry Program { trivia Spaces }
    "#;
    let source = "function outer() { function first() { return second; } function second() { return first; } function read() { return later; } let later = 1; return read; } let f = function() { let inner = later; return inner; }; let later = 1;";
    let result = analyze(grammar, source);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let self_reference = analyze(grammar, "let x = x;");
    assert!(self_reference
        .diagnostics
        .iter()
        .any(|error| error.message == "Unknown name 'x'"));
}

#[test]
fn declaration_and_use_in_the_same_group_keep_action_order() {
    let result = analyze(
        r#"
import { lexicalScope, declare, use } from "std/references"
node Declaration = name: /[a-z]+/ { meanings { declare(name) use(name) } }
node Root = Declaration { meanings { lexicalScope() } }
entry Root
"#,
        "x",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn named_registration_can_precede_reference_while_analysis_runs_later() {
    let result = analyze(
        r#"
import { lexicalScope, declare, use } from "std/references"
import { contribute } from "std/query"
node Declaration = "let " name: /[a-z]+/ ";" { meanings {
 group Late { contribute("late", true) }
 group Register { declare(name) }
} }
node Reference = name: /[a-z]+/ ";" { meanings { use(name) } }
node Program = Reference Declaration { meanings {
 lexicalScope()
 precedence { Declaration::meanings::Register > Reference > Declaration::meanings::Late }
} }
entry Program
"#,
        "x;let x;",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn separate_unnamed_groups_keep_their_own_schedule_position() {
    for (actions, unresolved) in [
        ("declare(name) group Divider {} use(name)", false),
        ("use(name) group Divider {} declare(name)", true),
    ] {
        let source = format!(
            r#"
import {{ lexicalScope, declare, use }} from "std/references"
node Declaration = name: /[a-z]+/ {{ meanings {{ {actions} }} }}
node Root = Declaration {{ meanings {{ lexicalScope() }} }}
entry Root
"#
        );
        let result = analyze(&source, "x");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .any(|error| error.message == "Unknown name 'x'"),
            unresolved,
            "{actions}: {:?}",
            result.diagnostics
        );
    }
}
