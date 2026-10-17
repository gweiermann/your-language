use std::collections::BTreeMap;
use your_language::{compile_sources, load_compiled_language, parse};
fn language(source: &str) -> your_language::CompiledLanguage {
    compile_sources(
        "entry.yl",
        &BTreeMap::from([("entry.yl".into(), source.into())]),
    )
    .unwrap()
}
fn preview(source: &str, name: &str) -> Option<String> {
    language(source)
        .example_previews()
        .into_iter()
        .find(|p| p.definition == format!("entry.yl#{name}"))
        .unwrap()
        .text
}
#[test]
fn explicit_examples_override_and_compose_nodes_and_patterns() {
    let source = r#"pattern Name = /[a-z]+/ { ide { example "hello" } }
node Number = /[0-9]+/ { ide { example "42" } }
node Program = Name "=" Number
entry Program"#;
    assert_eq!(preview(source, "Program").as_deref(), Some("hello=42"));
    let language = language(source);
    let loaded = load_compiled_language(&language.to_bytes().unwrap()).unwrap();
    assert_eq!(language.example_previews(), loaded.example_previews());
    assert_eq!(parse(&language, "hello=42"), parse(&loaded, "hello=42"));
}
#[test]
fn invalid_explicit_examples_are_spanned_errors() {
    let errors = compile_sources(
        "entry.yl",
        &BTreeMap::from([(
            "entry.yl".into(),
            "node Number = /[0-9]+/ { ide { example \"abc\" } } entry Number".into(),
        )]),
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "yl.invalid_example");
    assert!(errors[0].primary.end > errors[0].primary.start);
}
#[test]
fn shortest_choices_ties_optional_and_one_repetition() {
    assert_eq!(
        preview(
            "node Program = (\"long\" | \"b\" | \"a\") entry Program",
            "Program"
        )
        .as_deref(),
        Some("b")
    );
    assert_eq!(
        preview(
            "node Program = \"a\"? \"b\"* \"c\"+ entry Program",
            "Program"
        )
        .as_deref(),
        Some("bc")
    );
}
#[test]
fn regexes_and_recursive_terminating_paths_are_bounded() {
    assert_eq!(
        preview("node Program = /[0-9]{2}/ entry Program", "Program").as_deref(),
        Some("00")
    );
    assert_eq!(
        preview(
            "node Program = (\"(\" Program \")\" | \"x\") entry Program",
            "Program"
        )
        .as_deref(),
        Some("x")
    );
    assert_eq!(
        preview(
            "node Program = \"(\" Program \")\" entry Program",
            "Program"
        ),
        None
    );
    assert_eq!(
        preview("node Program = /a{10000}/ entry Program", "Program"),
        None
    );
}
#[test]
fn pipe_star_keeps_one_original_item_and_no_separator_tail() {
    let source = r#"import { separatedBy } from "std/parser"
node Item = "x"
node Program = Item* |> separatedBy(",")
entry Program"#;
    assert_eq!(preview(source, "Program").as_deref(), Some("x"));
}
#[test]
fn imports_trivia_and_constraints_are_parser_validated() {
    let sources = BTreeMap::from([
        ("entry.yl".into(), "import { Item } from \"./item\" trivia Space = / +/ node Program = \"let\" Item entry Program { trivia Space }".into()),
        ("item.yl".into(), "export node Item = /[a-z]+/ { ide { example \"name\" } }".into()),
    ]);
    let language = compile_sources("entry.yl", &sources).unwrap();
    let text = language
        .example_previews()
        .into_iter()
        .find(|p| p.definition == "entry.yl#Program")
        .unwrap()
        .text
        .unwrap();
    assert!(!parse(&language, &text).has_errors());
    let source = r#"import { error, warn, hint } from "core/diagnostics"
node Program = value: "x" { checks { when true { error("Invalid") } } } entry Program"#;
    assert_eq!(preview(source, "Program"), None);
}
#[test]
fn formatting_metadata_is_idempotent() {
    let source = "node Program = \"x\" { ide { example \"x\" } } entry Program";
    let once = your_language::format_yl("entry.yl", source).unwrap();
    assert_eq!(once, your_language::format_yl("entry.yl", &once).unwrap());
    assert!(once.contains("example \"x\""));
}

#[test]
fn metadata_has_no_effect_on_target_ast_or_syntax_diagnostics() {
    let plain = language("pattern Item = /[0-9]+/ node Program = Item entry Program");
    let metadata = language(
        r#"pattern Item = /[0-9]+/ { ide { example "42" } } node Program = Item entry Program"#,
    );
    assert_eq!(parse(&plain, "7"), parse(&metadata, "7"));
    assert_eq!(parse(&plain, "bad"), parse(&metadata, "bad"));
}

#[test]
fn pattern_examples_preserve_scalar_constraint_typing() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"
pattern Name = /[a-z]+/ { ide { example "x" } }
node Root = name: Name {
    checks { when name == "x" { warn("ok") } }
}
entry Root"#;
    let annotated = language(source);
    let metadata = r#" { ide { example "x" } }"#;
    let plain = language(&source.replace(metadata, &" ".repeat(metadata.len())));
    assert_eq!(parse(&annotated, "x"), parse(&plain, "x"));
    assert_eq!(parse(&annotated, "other"), parse(&plain, "other"));
    assert_eq!(
        annotated
            .example_previews()
            .iter()
            .find(|p| p.definition == "entry.yl#Root")
            .unwrap()
            .text
            .as_deref(),
        Some("x")
    );
}

#[test]
fn corrupt_tooling_projection_markers_are_rejected_on_reload() {
    let compiled = language(r#"pattern Preview = "x" node Root = "x" entry Root"#);
    let mut artifact: serde_json::Value =
        serde_json::from_slice(&compiled.to_bytes().unwrap()).unwrap();
    let index = artifact["parser"]["instructions"].as_array().unwrap().len();
    artifact["parser"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"Mark":{"id":123,"term":0}}));
    artifact["parser"]["preview_roots"]["entry.yl#Preview"] = serde_json::json!(index);
    let errors = load_compiled_language(&serde_json::to_vec(&artifact).unwrap()).unwrap_err();
    assert_eq!(errors[0].code, "yl.artifact");
}
#[test]
fn generated_literals_have_the_same_byte_budget_as_compositions() {
    let literal = "x".repeat(5000);
    let source = format!("node Root = \"{literal}\" entry Root");
    assert_eq!(preview(&source, "Root"), None);
}

#[test]
fn loaded_node_examples_cannot_substitute_the_nodes_actual_grammar() {
    let compiled = language(r#"node Root = "x" { ide { example "x" } } entry Root"#);
    let mut artifact: serde_json::Value =
        serde_json::from_slice(&compiled.to_bytes().unwrap()).unwrap();
    let index = artifact["parser"]["instructions"].as_array().unwrap().len();
    artifact["parser"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"Literal":"z"}));
    artifact["parser"]["preview_roots"]["entry.yl#Root"] = serde_json::json!(index);
    artifact["previews"][0]["text"] = "z".into();
    assert_eq!(
        load_compiled_language(&serde_json::to_vec(&artifact).unwrap()).unwrap_err()[0].code,
        "yl.artifact"
    );
}

#[test]
fn generic_pipe_wrappers_preserve_one_item_through_nested_optional_and_empty_choice() {
    for body in [
        r#""[" (value ("," value)*)? "]""#,
        r#"("[]" | "[" value ("," value)* "]")"#,
    ] {
        let source = format!(
            r#"import {{ error, warn, hint }} from "core/diagnostics"
pipe wrapped() {{ rewrite value* => {body} }} node Item = "x" node Root = Item* |> wrapped() entry Root"#
        );
        assert_eq!(preview(&source, "Root").as_deref(), Some("[x]"));
    }
}
#[test]
fn authored_large_examples_do_not_bypass_generated_parent_budget() {
    let example = "x".repeat(5000);
    let source = format!(
        "pattern Alias = Child node Child = /x+/ {{ ide {{ example \"{example}\" }} }} entry Child"
    );
    let language = language(&source);
    let previews = language.example_previews();
    assert_eq!(
        previews
            .iter()
            .find(|p| p.definition == "entry.yl#Child")
            .unwrap()
            .text
            .as_deref(),
        Some(example.as_str())
    );
    assert_eq!(
        previews
            .iter()
            .find(|p| p.definition == "entry.yl#Alias")
            .unwrap()
            .text,
        None
    );
}

#[test]
fn recursive_operator_examples_use_their_pratt_family_context() {
    let source = r#"node Expression {
        node Number = /[0-9]+/
        node Sum = left: Expression "+" right: Expression { ide { example "1+2" } }
        precedence { Sum }
    }
    entry Expression"#;
    assert_eq!(preview(source, "Expression::Sum").as_deref(), Some("1+2"));
    assert_eq!(
        preview(
            &source.replace(r#" { ide { example "1+2" } }"#, ""),
            "Expression::Sum"
        )
        .as_deref(),
        Some("0+0")
    );
    let errors = compile_sources(
        "entry.yl",
        &BTreeMap::from([("entry.yl".into(), source.replace("1+2", "1"))]),
    )
    .unwrap_err();
    assert_eq!(errors[0].code, "yl.invalid_example");
}

#[test]
fn nullable_star_does_not_silently_preview_zero_occurrences() {
    assert_eq!(
        preview(r#"node Empty = "" node Root = Empty* entry Root"#, "Root"),
        None
    );
}

#[test]
fn readable_spacing_uses_only_selected_single_space_trivia() {
    let spaced = r#"trivia Space = / +/
pattern Name = /[a-z]+/ { ide { example "name" } }
node Number = /[0-9]+/ { ide { example "42" } }
node Program = "let" Name "=" Number ";"
entry Program { trivia Space }"#;
    assert_eq!(
        preview(spaced, "Program").as_deref(),
        Some("let name = 42 ;")
    );
    let compiled = language(spaced);
    // Preview readability does not change accepted syntax or invent keyword boundaries.
    assert!(!parse(&compiled, "letname=42;").has_errors());
    let loaded = load_compiled_language(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(compiled.example_previews(), loaded.example_previews());
    assert_eq!(
        preview(r#"node Pair = "a" "b" entry Pair"#, "Pair").as_deref(),
        Some("ab")
    );
    assert_eq!(
        preview(
            r#"trivia Comment = /\/\*x\*\// node Pair = "a" "b" entry Pair { trivia Comment }"#,
            "Pair"
        )
        .as_deref(),
        Some("ab")
    );
    assert_eq!(
        preview(
            r#"trivia Space = / +/ node Pair = "a" "b" entry Pair"#,
            "Pair"
        )
        .as_deref(),
        Some("ab")
    );
    assert_eq!(
        preview(
            r#"trivia Space = / +/ node Word = /[a-z]{2}/ entry Word { trivia Space }"#,
            "Word"
        )
        .as_deref(),
        Some("aa")
    );
    assert_eq!(
        preview(r#"node Pair = "a" " " "b" entry Pair"#, "Pair").as_deref(),
        Some("a b")
    );
}

#[test]
fn spacing_retries_adjacent_candidates_under_the_same_budget() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"
trivia Space = / +/
node Pair = left: "a" right: "b" {
    checks { when trivia.between(left, right) { error("Adjacent only") } }
}
entry Pair { trivia Space }"#;
    assert_eq!(preview(source, "Pair").as_deref(), Some("ab"));
    let compiled = language(source);
    assert!(parse(&compiled, "a b").has_errors());
    assert!(!parse(&compiled, "ab").has_errors());
}

#[test]
fn checked_in_fixture_cli_and_reload_generate_readable_trivia_examples() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/04-ide-metadata/definition/language.yl");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yl"))
        .arg("examples")
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let previews: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        previews
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["definition"] == "language.yl#Program")
            .unwrap()["text"],
        "let name = 42 ;"
    );
    let compiled = your_language::compile_language(fixture).unwrap();
    let reloaded = load_compiled_language(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(compiled.example_previews(), reloaded.example_previews());
    assert!(!parse(&reloaded, "let name = 42 ;").has_errors());
}

#[test]
fn spacing_fallback_survives_a_full_concat_pruned_from_the_readability_beam() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"
trivia Space = / +/
node Chain = a:"a" b:"b" c:"c" d:"d" e:"e" f:"f" {
    checks {
        when trivia.between(a,b) || trivia.between(b,c) || trivia.between(c,d) || trivia.between(d,e) || trivia.between(e,f) {
            error("Adjacent only")
        }
    }
}
entry Chain { trivia Space }"#;
    assert_eq!(preview(source, "Chain").as_deref(), Some("abcdef"));
}

#[test]
fn imported_child_previews_share_the_entries_selected_trivia() {
    let sources=BTreeMap::from([
        ("main.yl".into(),r#"import { Let } from "./let" import { Whitespace } from "./lexical" node Program = Let entry Program { trivia Whitespace }"#.into()),
        ("let.yl".into(),r#"import { Name } from "./lexical" node Number = /[0-9]+/ { ide { example "42" } } export node Let = "let" Name "=" Number ";""#.into()),
        ("lexical.yl".into(),r#"export trivia Whitespace = / +/ export pattern Name = /[a-z]+/ { ide { example "name" } }"#.into()),
    ]);
    let compiled = compile_sources("main.yl", &sources).unwrap();
    for id in ["main.yl#Program", "let.yl#Let"] {
        let previews = compiled.example_previews();
        assert_eq!(
            previews
                .iter()
                .find(|p| p.definition == id)
                .unwrap()
                .text
                .as_deref(),
            Some("let name = 42 ;")
        );
    }
    let mut without_selection = sources;
    without_selection.get_mut("main.yl").unwrap().replace_range(..,r#"import { Let } from "./let" import { Whitespace } from "./lexical" node Program = Let entry Program"#);
    let compiled = compile_sources("main.yl", &without_selection).unwrap();
    assert_eq!(
        compiled
            .example_previews()
            .iter()
            .find(|p| p.definition == "let.yl#Let")
            .unwrap()
            .text
            .as_deref(),
        Some("letname=42;")
    );
}
