use std::{fs, process::Command};

#[test]
fn cli_compiles_reloads_and_reports_authored_name_diagnostics_as_human_and_json() {
    let executable = env!("CARGO_BIN_EXE_yl");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = root
        .join("target")
        .join(format!("query-cli-{}.ylc", std::process::id()));
    let build = Command::new(executable)
        .current_dir(root)
        .args([
            "compile",
            "examples/declarative-queries/definition/language.yl",
            "-o",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let good = Command::new(executable)
        .current_dir(root)
        .arg("language")
        .arg(&output)
        .args([
            "check",
            "examples/declarative-queries/programs/shadowing.txt",
        ])
        .output()
        .unwrap();
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    let bad = Command::new(executable)
        .current_dir(root)
        .arg("language")
        .arg(&output)
        .args(["check", "examples/declarative-queries/programs/errors.txt"])
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1));
    let human = String::from_utf8(bad.stderr).unwrap();
    assert!(human.contains("\x1b[31m"));
    assert!(human.contains("Unknown name 'missing'"));
    assert!(human.contains("Duplicate declaration 'greeting'"));
    let json = Command::new(executable)
        .current_dir(root)
        .arg("language")
        .arg(&output)
        .args([
            "check",
            "examples/declarative-queries/programs/errors.txt",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(json.status.code(), Some(1));
    assert!(json.stderr.is_empty());
    let diagnostics: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(diagnostics.as_array().unwrap().len(), 3);
    let source = include_str!("../examples/declarative-queries/programs/errors.txt");
    for diagnostic in diagnostics.as_array().unwrap() {
        let start = diagnostic["primary"]["start"].as_u64().unwrap() as usize;
        let end = diagnostic["primary"]["end"].as_u64().unwrap() as usize;
        assert!(matches!(&source[start..end], "greeting" | "missing"));
    }
    fs::remove_file(output).unwrap();
}
