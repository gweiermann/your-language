use std::collections::BTreeMap;
use your_language::{
    semantics::{AnalysisResult, SemanticEngine, SemanticValue},
    CompiledLanguage,
};

fn compile(source: &str) -> your_language::diagnostic::CompileResult<CompiledLanguage> {
    SemanticEngine::default().compile(
        "definitions.yl",
        &BTreeMap::from([("definitions.yl".into(), source.into())]),
    )
}

fn analyze(source: &str) -> AnalysisResult {
    let engine = SemanticEngine::default();
    let language = compile(source).unwrap();
    let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&reloaded, "program", "x");
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == your_language::diagnostic::Severity::Error),
        "{:?}",
        result.diagnostics
    );
    result
}

#[test]
fn immutable_bindings_follow_forward_dependencies_instead_of_textual_order() {
    let result = analyze(
        r#"
import { contribute, get } from "core/query"
node Program = "x" {
    semantics {
        contribute("answer", earlier)
        earlier := later
        later := get("seed")
        contribute("seed", "ready")
    }
}
entry Program
"#,
    );
    assert!(result.graph.query_entries.iter().any(|entry| entry.key
        == your_language::semantics::queries::QueryKey::Text("answer".into())
        && entry.value == SemanticValue::Text("ready".into())));
}

#[test]
fn duplicate_definitions_and_definition_cycles_are_structured_errors() {
    for (actions, code) in [
        ("value := true value := false", "yl.duplicate_binding"),
        ("first := second second := first", "yl.binding_cycle"),
    ] {
        let source = format!("node Program = \"x\" {{ semantics {{ {actions} }} }} entry Program");
        let errors = compile(&source).unwrap_err();
        assert!(errors.iter().any(|error| error.code == code), "{errors:?}");
    }
}

#[test]
fn action_results_preserve_effects_with_repeated_reads_and_unused_results() {
    let result = analyze(
        r#"import { error, warn, hint } from "core/diagnostics"

import { contribute } from "core/query"
operation connect(target, message) {
    contribute((target, "connected"), true)
    warn("${message}", target)
    return target
}
node Program = "x" {
    semantics {
        ref := connect(self, "bound")
        contribute("first", ref)
        contribute("second", ref)
        connect(self, "unused")
    }
}
entry Program
"#,
    );
    // Each invocation owns one effect; reading ref twice cannot create another invocation.
    assert_eq!(result.diagnostics.len(), 2);
}

#[test]
fn enum_variants_are_symbol_values_and_keys_after_artifact_reload() {
    let result = analyze(
        r#"
import { contribute, get } from "core/query"
enum Type { number string }
node Program = "x" {
    semantics {
        contribute((self, Type::number), Type::string)
        contribute("result", get((self, Type::number)))
    }
}
entry Program
"#,
    );
    assert!(result.graph.query_entries.iter().any(|entry| matches!(&entry.value, SemanticValue::Symbol(value) if value.ends_with("Type::string"))));
}

#[test]
fn diagnostics_require_imports_and_support_namespace_imports() {
    let errors =
        compile(r#"node Program = "x" { semantics { error("missing import") } } entry Program"#)
            .unwrap_err();
    assert!(!errors.is_empty());
    let source = r#"import { error, warn, hint } from "core/diagnostics"
import * as diagnostics from "core/diagnostics"
node Program = "x" { semantics { diagnostics::hint("Useful hint", self) } }
entry Program"#;
    let engine = SemanticEngine::default();
    let language = compile(source).unwrap();
    let result = engine.analyze(&language, "program", "x");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].message, "Useful hint");
}

fn entries_at<'a>(
    result: &'a AnalysisResult,
    name: &str,
) -> Vec<&'a your_language::semantics::queries::QueryEntry> {
    result
        .graph
        .query_entries
        .iter()
        .filter(|entry| entry.key == your_language::semantics::queries::QueryKey::Text(name.into()))
        .collect()
}

#[test]
fn forward_binding_effects_are_unconditional_even_if_all_reads_are_guarded_out() {
    let result = analyze(
        r#"
import { contribute } from "core/query"
operation make() { contribute("effect", true) return "result" }
node Program = "x" {
    semantics {
        when false { contribute("unreachable", value) }
        value := make()
    }
}
entry Program"#,
    );
    assert_eq!(entries_at(&result, "effect").len(), 1);
    assert!(entries_at(&result, "unreachable").is_empty());
}

#[test]
fn unused_effectful_binding_runs_once() {
    let result = analyze(
        r#"
import { contribute } from "core/query"
operation make() { contribute("effect", true) return true }
node Program = "x" { semantics { unused := make() } }
entry Program"#,
    );
    assert_eq!(entries_at(&result, "effect").len(), 1);
}

#[test]
fn distinct_invocations_keep_distinct_producers_with_equal_arguments() {
    let result = analyze(
        r#"
import { contribute } from "core/query"
operation make(value) { contribute("effect", value) return value }
node Program = "x" {
    semantics {
        first := make(true)
        second := make(true)
        contribute("first", first)
        contribute("second", second)
    }
}
entry Program"#,
    );
    let effects = entries_at(&result, "effect");
    assert_eq!(effects.len(), 2);
    assert_ne!(effects[0].producer, effects[1].producer);
}

#[test]
fn return_defines_a_result_without_skipping_subsequent_actions() {
    let result = analyze(
        r#"
import { contribute } from "core/query"
operation make() { return "result" contribute("after", true) }
node Program = "x" { semantics { value := make() contribute("answer", value) } }
entry Program"#,
    );
    assert_eq!(entries_at(&result, "after").len(), 1);
    assert_eq!(
        entries_at(&result, "answer")[0].value,
        SemanticValue::Text("result".into())
    );
}

#[test]
fn nested_namespaced_wrappers_keep_invocation_identity_per_outer_call() {
    let engine = SemanticEngine::default();
    let language = engine
        .compile(
            "definitions.yl",
            &BTreeMap::from([
                (
                    "definitions.yl".into(),
                    r#"
import * as helpers from "./helpers"
import { contribute } from "core/query"
node Program = "x" {
    semantics {
        first := helpers::outer(true)
        second := helpers::outer(true)
        contribute("first", first)
        contribute("second", second)
    }
}
entry Program"#
                        .into(),
                ),
                (
                    "helpers.yl".into(),
                    r#"
import { contribute } from "core/query"
operation inner(value) { contribute("effect", value) return value }
export operation outer(value) { result := inner(value) return result }
"#
                    .into(),
                ),
            ]),
        )
        .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    let result = engine.analyze(&loaded, "program", "x");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let effects = entries_at(&result, "effect");
    assert_eq!(effects.len(), 2);
    assert_ne!(effects[0].producer, effects[1].producer);
}

#[test]
fn absent_binding_and_pending_dependency_do_not_disable_sibling_contributions() {
    let result = analyze(
        r#"
import { contribute, get } from "core/query"
node Program = "x" {
    semantics {
        missing := get("absent-key")
        delayed := get("seed")
        contribute("omitted", missing)
        contribute("answer", delayed)
        contribute("sibling", true)
        contribute("seed", "settled")
    }
}
entry Program"#,
    );
    assert!(entries_at(&result, "omitted").is_empty());
    assert_eq!(entries_at(&result, "sibling").len(), 1);
    assert_eq!(
        entries_at(&result, "answer")[0].value,
        SemanticValue::Text("settled".into())
    );
}

#[test]
fn operation_self_and_non_declarative_return_shapes_are_rejected() {
    for source in [
        r#"operation bad() { return self } node Program = "x" { semantics { bad() } } entry Program"#,
        r#"operation bad() { return true return false } node Program = "x" entry Program"#,
        r#"operation bad() { when true { return true } } node Program = "x" entry Program"#,
        r#"node Program = "x" { semantics { return self } } entry Program"#,
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}

#[test]
fn forward_projection_and_callback_shadowing_use_lexical_dependencies() {
    let result = analyze(
        r#"
import { contribute, map } from "core/query"
node Child = value: "x"
node Program = child: Child {
    semantics {
        projected := holder.value
        holder := child
        item := map(["ready"], item => item)
        contribute("projection", projected)
        contribute("callback", item)
    }
}
entry Program
"#,
    );
    assert!(result
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.key
            == your_language::semantics::queries::QueryKey::Text("projection".into())));
}

#[test]
fn compound_definition_expansion_is_bounded_before_large_copies() {
    let mut actions = String::from("v00 := true ");
    for index in 1..25 {
        actions.push_str(&format!(
            "v{index:02} := (v{previous:02}, v{previous:02}) ",
            previous = index - 1
        ));
    }
    let source = format!("node Program = \"x\" {{ semantics {{ {actions} }} }} entry Program");
    let errors = compile(&source).unwrap_err();
    assert_eq!(errors[0].code, "yl.expansion_limit");
}

#[test]
fn effectful_default_values_belong_to_each_outer_invocation() {
    let result = analyze(
        r#"
import { contribute } from "core/query"
operation make() { contribute("events", true) return true }
operation outer(value = make()) { return value }
node Program = "x" {
    semantics {
        first := outer()
        second := outer()
        outer()
    }
}
entry Program
"#,
    );
    let events = entries_at(&result, "events");
    assert_eq!(events.len(), 3);
    assert_ne!(events[0].producer, events[1].producer);
}

#[test]
fn ignored_explicit_and_default_arguments_keep_their_effects() {
    let result = analyze(
        r#"
import { contribute } from "core/query"
operation make() { contribute("events", true) return true }
operation ignore(value) { return true }
operation ignoreDefault(value = make()) { return true }
operation ignoreQuery(value) = true
node Program = "x" {
    semantics {
        first := ignore(make())
        second := ignoreDefault()
        third := ignoreQuery(make())
        ignore(make())
        ignoreDefault()
    }
}
entry Program
"#,
    );
    assert_eq!(entries_at(&result, "events").len(), 5);
}

#[test]
fn named_groups_read_surrounding_definitions_and_shadow_locally() {
    let result = analyze(
        r#"
import { contribute } from "core/query"
node Program = "x" {
    semantics {
        ref := "outer"
        group Register { contribute("outer", ref) }
        group Local { ref := "inner" contribute("inner", ref) }
    }
}
entry Program
"#,
    );
    assert_eq!(
        entries_at(&result, "outer")[0].value,
        SemanticValue::Text("outer".into())
    );
    assert_eq!(
        entries_at(&result, "inner")[0].value,
        SemanticValue::Text("inner".into())
    );
}

#[test]
fn callbacks_can_read_an_outer_effectful_definition_without_replaying_it() {
    let result = analyze(
        r#"
import { contribute, map } from "core/query"
operation make() { contribute("events", true) return "ready" }
node Program = "x" {
    semantics {
        ref := make()
        values := map([true, false], item => ref)
        contribute("result", values)
    }
}
entry Program
"#,
    );
    assert_eq!(entries_at(&result, "events").len(), 1);
    assert_eq!(
        entries_at(&result, "result")[0].value,
        SemanticValue::List(vec![
            SemanticValue::Text("ready".into()),
            SemanticValue::Text("ready".into())
        ])
    );
}

#[test]
fn a_binding_keeps_its_definition_rank_when_read_by_a_later_group() {
    let result = analyze(
        r#"
import { contribute, getEarlier, isPresent } from "core/query"
import { error } from "core/diagnostics"
node Program = "x" {
    semantics {
        cached := getEarlier("key")
        group Producer { contribute("key", true) }
        group Consumer {
            when isPresent(cached) { error("Binding resolved at reader rank") }
        }
    }
}
entry Program
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
