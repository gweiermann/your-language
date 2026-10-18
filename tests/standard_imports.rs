//! Standard module IDs are built in, not relative to a checkout or project root.
use std::{fs, process::Command};
use your_language::{
    compile_language, load_compiled_language,
    semantics::{queries::QueryKey, SemanticEngine, SemanticValue},
};

const LANGUAGE: &str = r#"import { separatedBy } from "std/parser"
import { createLexicalScope } from "std/references"
import { annotateWithType, typeOf } from "std/types"
import { additionType } from "std/types/arithmetic"
import { numberType, stringType, isIdenticalTo } from "std/types/structural"
import { number } from "std/backend/integer"
import { contribute } from "core/query"

node Root = values: /[0-9]+/+ |> separatedBy(",") {
    semantics {
        createLexicalScope(self)
        annotateWithType(self, numberType())
        contribute("observed", isIdenticalTo(
            additionType(typeOf(self), numberType(), numberType(), stringType()),
            numberType()
        ))
    }
    backend { number(values) }
}
entry Root
"#;

#[test]
fn external_project_resolves_all_standard_modules_and_reloads_without_sources() {
    let directory = std::env::temp_dir().join(format!(
        "yl-standard-imports-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let entry = directory.join("language.yl");
    let artifact = directory.join("language.ylc");
    let input = directory.join("program.txt");
    fs::write(&entry, LANGUAGE).unwrap();
    fs::write(&input, "1,2").unwrap();

    let compiled = compile_language(&entry).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .current_dir(&directory)
        .args(["compile", "language.yl", "-o", "language.ylc"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    // No standard-library directory or alias configuration exists in this project.
    // Delete even the entry source before loading and checking the artifact.
    fs::remove_file(&entry).unwrap();
    let loaded = load_compiled_language(&fs::read(&artifact).unwrap()).unwrap();
    assert!(loaded
        .required_hooks()
        .iter()
        .all(|hook| hook.id.starts_with("std/backend/integer#")));
    assert!(loaded
        .required_hooks()
        .iter()
        .any(|hook| hook.id == "std/backend/integer#number"));
    let engine = SemanticEngine::default();
    let result = engine.analyze(&loaded, "program.txt", "1,2");
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result, engine.analyze(&compiled, "program.txt", "1,2"));
    assert!(result.graph.query_entries.iter().any(|entry| {
        entry.key == QueryKey::Text("observed".into()) && entry.value == SemanticValue::Bool(true)
    }));
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .current_dir(&directory)
        .args(["language", "language.ylc", "check", "program.txt", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "[]");
    assert!(!engine
        .analyze(&loaded, "invalid.txt", "1,wrong")
        .diagnostics
        .is_empty());
    fs::remove_file(artifact).unwrap();
    fs::remove_file(input).unwrap();
    fs::remove_dir(directory).unwrap();
}
