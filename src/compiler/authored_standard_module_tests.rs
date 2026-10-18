use super::*;
use crate::semantics::SemanticEngine;

const LANGUAGE: &str = r#"
    import * as references from "std/references"
    import { declareName, connectReference } from "std/references"
    import { unknown } from "std/types"
    import { contribute } from "core/query"
    trivia Space = /\s+/
    node Declaration = "let" name: /[a-z]+/ ";" {
        semantics { declareName(self, name) }
    }
    node Reference = name: /[a-z]+/ ";" {
        semantics { connectReference(self, name) }
    }
    node Program = (Declaration | Reference)* {
        semantics {
            references::createLexicalScope(self)
            contribute((self, unknown), unknown)
        }
    }
    entry Program { trivia Space }
"#;

fn assert_authored(language: &CompiledLanguage) {
    assert!(!language.query_meanings.is_empty());
    assert!(language.meanings.values().all(|definition| definition
        .groups
        .iter()
        .all(|group| group.calls.is_empty() && group.matching.is_none())));
    let json = serde_json::to_string(language).unwrap();
    assert!(json.contains("std/types#unknown"));
    assert!(json.contains("std/references#scope"));
    assert!(!json.contains("std/variables#"));
    assert!(!json.contains("std/references#declare"));
    assert!(!json.contains("std/references#lexicalScope"));
    let engine = SemanticEngine::default();
    let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    assert!(engine
        .analyze(&reloaded, "program", "let x; x;")
        .diagnostics
        .is_empty());
    let errors = engine.analyze(&reloaded, "program", "missing;");
    assert!(errors
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Unknown name 'missing'")));
}

#[test]
fn canonical_authored_imports_expand_and_reload_without_native_adapters() {
    let sources = BTreeMap::from([
        ("nested/language.yl".into(), LANGUAGE.into()),
        // Caller-supplied source maps cannot replace reserved definitions.
        ("std/types".into(), "export symbol wrong".into()),
        ("std/references".into(), "export symbol wrong".into()),
        ("std/variables".into(), "export symbol wrong".into()),
    ]);
    assert_authored(&compile_sources("nested/language.yl", &sources).unwrap());
}

#[test]
fn authored_operation_arguments_keep_caller_capture_identity() {
    let source = r#"import * as types from "std/types"
        symbol Red
        node Root = first: /x/ { semantics {
            types::annotateWithType(first, Red)
            types::restrictTypeTo(first, Red)
        } }
        entry Root"#;
    let engine = SemanticEngine::default();
    let language = compile_sources(
        "language.yl",
        &BTreeMap::from([("language.yl".into(), source.into())]),
    )
    .unwrap();
    let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    assert!(engine.analyze(&loaded, "input", "x").diagnostics.is_empty());
}

#[test]
fn nested_wrappers_preserve_caller_captures_and_member_projections() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"
import * as types from "std/types"
        import { apply, read } from "./helpers"
        import { equal } from "core/query"
        symbol Red
        node Container = first: /x/
        node Root = box: Container { semantics {
            apply(box, Red)
            when !equal(read(box), Red) { error("Wrong caller capture") }
        } }
        entry Root"#;
    let helper = r#"import * as types from "std/types"
        operation nested(value, annotation) { types::annotateWithType(value.first, annotation) }
        export operation apply(value, annotation) { nested(value, annotation) }
        export operation read(value) = types::typeOf(value.first)"#;
    let engine = SemanticEngine::default();
    let language = compile_sources(
        "language.yl",
        &BTreeMap::from([
            ("language.yl".into(), source.into()),
            ("helpers.yl".into(), helper.into()),
        ]),
    )
    .unwrap();
    assert!(engine
        .analyze(&language, "input", "x")
        .diagnostics
        .is_empty());
}

#[test]
fn query_artifacts_reject_missing_and_ambiguous_scheduling_groups() {
    let source = r#"import { contribute } from "core/query"
        node Root = "x" { semantics { contribute("value", true) } } entry Root"#;
    let language = compile_sources(
        "language.yl",
        &BTreeMap::from([("language.yl".into(), source.into())]),
    )
    .unwrap();
    for mutation in 0..3 {
        let mut broken = language.clone();
        let groups = &mut broken
            .image
            .as_mut()
            .unwrap()
            .semantics
            .get_mut("language.yl#Root")
            .unwrap()
            .groups;
        match mutation {
            0 => groups.clear(),
            1 => groups[0].span.start += 1,
            _ => groups.push(groups[0].clone()),
        }
        let errors =
            crate::load_compiled_language(&serde_json::to_vec(&broken).unwrap()).unwrap_err();
        assert_eq!(errors[0].code, "yl.artifact");
        assert!(errors[0].message.contains("scheduling placeholder"));
    }
}

#[test]
fn filesystem_imports_use_the_same_canonical_modules() {
    let directory = std::env::temp_dir().join(format!(
        "yl-authored-std-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let entry = directory.join("language.yl");
    std::fs::write(&entry, LANGUAGE).unwrap();
    let compiled = compile_language(&entry);
    std::fs::remove_file(&entry).unwrap();
    std::fs::remove_dir(&directory).unwrap();
    assert_authored(&compiled.unwrap());
}

#[test]
fn supplied_native_registry_cannot_replace_authored_standard_modules() {
    for module in [
        "std/references",
        "std/variables",
        "std/types",
        "std/types/structural",
        "std/types/arithmetic",
        "std/backend/integer",
    ] {
        let mut registry = OperationRegistry::new();
        registry.export_symbol(module, "replacement").unwrap();
        let errors = compile_sources_with_registry(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), LANGUAGE.into())]),
            &registry,
        )
        .unwrap_err();
        assert_eq!(errors[0].code, "yl.reserved_module");
        assert!(errors[0].message.contains(module));
    }
}

#[test]
fn default_engine_registers_only_generic_query_primitives() {
    let engine = SemanticEngine::default();
    assert!(engine.registry().contains_module("core/query"));
    assert!(!engine.registry().contains_module("std/references"));
    assert!(!engine.registry().contains_module("std/variables"));
    assert!(!engine.registry().contains_module("std/types"));
    assert!(!engine.registry().contains_module("core/syntax"));
}
