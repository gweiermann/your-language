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
        import { lexicalScope, use } from "std/semantics"
        pipe wrapped() { rewrite value => "(" value ")" }
        pattern Name = /[a-z]+/
        node Root = name: Name |> wrapped() { meanings { lexicalScope() use(name) } }
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
    let executable = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/debug/examples")
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
    assert_eq!(
        String::from_utf8(success.stdout).unwrap().trim(),
        "Analysis succeeded (16 semantic records)."
    );
    assert!(success.stderr.is_empty());
    let error = run(&["duplicate"], false, false);
    assert_eq!(error.status.code(), Some(1));
    assert!(error.stdout.is_empty());
    let plain = String::from_utf8(error.stderr).unwrap();
    assert!(plain.contains("semantic.duplicate_declaration"));
    assert_eq!(plain.matches("-->").count(), 2);
    assert!(plain.contains('^'));
    assert!(!plain.contains('\x1b'));
    let forced = run(&["duplicate"], true, false);
    assert!(String::from_utf8(forced.stderr)
        .unwrap()
        .contains("\x1b[31m"));
    let disabled = run(&["duplicate"], true, true);
    assert_eq!(String::from_utf8(disabled.stderr).unwrap(), plain);
    for args in [["initializer", "--json"], ["--json", "program"]] {
        let json = run(&args, true, false);
        assert!(json.stderr.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
        assert!(value.get("ast").is_some());
        assert!(value.get("graph").is_some());
        assert_eq!(json.status.success(), args[1] == "program");
        if !json.status.success() {
            assert_eq!(value["diagnostics"][0]["primary"]["start"], 11);
        }
    }
}
