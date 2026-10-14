use std::{collections::BTreeMap, process::Command};
use your_language::{
    diagnostic::{Diagnostic, Span},
    diagnostic_render::render_diagnostics,
    semantics::SemanticEngine,
};

#[test]
fn source_underlines_preserve_unicode_columns_secondary_locations_and_color() {
    let mut diagnostic = Diagnostic::error("example", "Bad combination", Span::new("source", 3, 5));
    diagnostic.secondary.push(Span::new("source", 7, 9));
    let source = "é .a, .b";
    let plain = render_diagnostics(&[diagnostic.clone()], |_| Some(source.into()), false);
    let lines: Vec<_> = plain.lines().collect();
    let source_line = lines.iter().position(|line| line.contains(source)).unwrap();
    let underline = lines[source_line + 1];
    assert_eq!(
        underline.find('^'),
        lines[source_line].find(".a").map(|byte| byte - 1)
    );
    assert_eq!(underline.matches('^').count(), 4);
    let colored = render_diagnostics(&[diagnostic], |_| Some(source.into()), true);
    assert!(colored.contains("\x1b[31m"));
    assert_eq!(
        colored.replace("\x1b[31m", "").replace("\x1b[0m", ""),
        plain
    );
    let eof = render_diagnostics(
        &[Diagnostic::error(
            "eof",
            "Expected token",
            Span::new("source", source.len(), source.len()),
        )],
        |_| Some(source.into()),
        false,
    );
    assert!(eof.lines().last().unwrap().ends_with('^'));
}

#[test]
fn semantic_pipe_origin_is_rendered_at_the_original_name() {
    let engine = SemanticEngine::default();
    let grammar = r#"
        import { createLexicalScope, connectReference } from "std/references"
        pipe wrapped() { rewrite value => "(" value ")" }
        pattern Name = /[a-z]+/
        node Root = name: Name |> wrapped() { semantics { createLexicalScope(self) connectReference(self, name) } }
        entry Root
    "#;
    let sources = BTreeMap::from([("language.yl".into(), grammar.into())]);
    let result = engine
        .compile_and_analyze("language.yl", &sources, "source", "(missing)")
        .unwrap();
    let output = render_diagnostics(&result.diagnostics, |_| Some("(missing)".into()), false);
    let lines: Vec<_> = output.lines().collect();
    let index = lines
        .iter()
        .position(|line| line.contains("(missing)"))
        .unwrap();
    assert_eq!(lines[index + 1].find('^'), lines[index].find("missing"));
    assert_eq!(lines[index + 1].matches('^').count(), 7);
}

#[test]
fn semantic_example_defaults_to_human_output_and_json_is_explicit() {
    let build = Command::new(env!("CARGO"))
        .args(["build", "--quiet", "--example", "semantic_analysis"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yl"))
        .parent()
        .unwrap()
        .join("examples")
        .join(format!("semantic_analysis{}", std::env::consts::EXE_SUFFIX));
    let run = |args: &[&str], force: bool, no_color: bool| {
        let mut command = Command::new(&executable);
        command
            .args(args)
            .env_remove("NO_COLOR")
            .env_remove("CLICOLOR_FORCE");
        if force {
            command.env("CLICOLOR_FORCE", "1");
        }
        if no_color {
            command.env("NO_COLOR", "1");
        }
        command.output().unwrap()
    };
    let success = run(&[], false, false);
    assert!(success.status.success());
    assert!(String::from_utf8(success.stdout)
        .unwrap()
        .trim()
        .starts_with("Analysis succeeded ("));
    assert!(success.stderr.is_empty());
    let error = run(&["duplicate"], false, false);
    assert_eq!(error.status.code(), Some(1));
    assert!(error.stdout.is_empty());
    let plain = String::from_utf8(error.stderr).unwrap();
    assert!(plain.contains("Duplicate declaration"));
    assert!(plain.matches("-->").count() >= 2);
    assert!(plain.contains('^'));
    assert!(!plain.contains('\x1b'));
    let forced = run(&["duplicate"], true, false);
    assert!(String::from_utf8(forced.stderr)
        .unwrap()
        .contains("\x1b[31m"));
    let disabled = run(&["duplicate"], true, true);
    assert_eq!(String::from_utf8(disabled.stderr).unwrap(), plain);
    for args in [["unresolved", "--json"], ["--json", "program"]] {
        let json = run(&args, true, false);
        assert!(json.stderr.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
        assert!(value.get("ast").is_some());
        assert!(value.get("graph").is_some());
        assert_eq!(json.status.success(), args[1] == "program");
        if !json.status.success() {
            assert!(value["diagnostics"][0]["message"]
                .as_str()
                .unwrap()
                .starts_with("Unknown name"));
        }
    }
}

#[test]
fn mini_typescript_consumer_renders_initializer_origin_and_json_uses_same_span() {
    let build = Command::new(env!("CARGO"))
        .args(["build", "--quiet", "--example", "mini_typescript"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yl"))
        .parent()
        .unwrap()
        .join("examples")
        .join(format!("mini_typescript{}", std::env::consts::EXE_SUFFIX));
    let human = Command::new(&executable)
        .arg("errors")
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();
    assert_eq!(human.status.code(), Some(1));
    let colored = String::from_utf8(human.stderr).unwrap();
    assert!(colored.contains("\x1b[31m"));
    let plain = colored.replace("\x1b[31m", "").replace("\x1b[0m", "");
    let lines: Vec<_> = plain.lines().collect();
    let row = lines
        .iter()
        .position(|line| line.contains("let count: number = \"wrong\";"))
        .unwrap();
    let type_column = lines[row].find("number").unwrap();
    assert_eq!(
        &lines[row + 1][type_column - 2..type_column + 7],
        "  ^^^^^^ ",
        "{plain}"
    );
    let column = lines[row].find("\"wrong\"").unwrap();
    assert_eq!(&lines[row + 1][column..column + 7], "^^^^^^^", "{plain}");
    let json = Command::new(&executable)
        .args(["errors", "--json"])
        .output()
        .unwrap();
    assert_eq!(json.status.code(), Some(1));
    assert!(json.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let source = include_str!("../examples/mini_typescript/programs/errors.txt");
    let diagnostics = value["diagnostics"].as_array().unwrap();
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            let primary = &diagnostic["primary"];
            let (start, end) = (
                primary["start"].as_u64().unwrap() as usize,
                primary["end"].as_u64().unwrap() as usize,
            );
            &source[start..end] == "\"wrong\""
        })
        .expect("Initializer is the primary JSON location");
    assert_eq!(diagnostic["code"], "query.diagnostic");
    assert!(diagnostic["message"]
        .as_str()
        .unwrap()
        .contains("does not match"));
    assert!(diagnostic["secondary"]
        .as_array()
        .unwrap()
        .iter()
        .any(|span| {
            let (start, end) = (
                span["start"].as_u64().unwrap() as usize,
                span["end"].as_u64().unwrap() as usize,
            );
            span["file"] == "errors" && &source[start..end] == "number"
        }));
}
