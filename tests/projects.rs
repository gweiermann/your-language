use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use your_language::project::Project;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yl-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("definition")).unwrap();
        fs::create_dir_all(root.join("programs/nested")).unwrap();
        fs::write(
            root.join("yl.json"),
            r#"{"entry":"./definition/language.yl","files":["programs/**/*.ts"]}"#,
        )
        .unwrap();
        fs::write(
            root.join("definition/language.yl"),
            "node Root = /[0-9]+/ entry Root",
        )
        .unwrap();
        fs::write(root.join("programs/good.ts"), "42").unwrap();
        fs::write(root.join("programs/bad.ts"), "wrong").unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn nearest_project_and_globs_are_shared_with_cli() {
    let f = Fixture::new();
    let project = Project::discover(&f.0.join("programs/good.ts"))
        .unwrap()
        .unwrap();
    assert_eq!(project.entry, f.0.join("definition/language.yl"));
    assert!(project.matches(&f.0.join("programs/good.ts")));
    assert!(project.matches(&f.0.join("programs/nested/other.ts")));
    assert!(!project.matches(&f.0.join("definition/other.ts")));
    assert!(!project.matches(&f.0.join("programs/file.js")));
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["project", "check"])
        .arg(f.0.join("programs/good.ts"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "[]");
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["project", "check"])
        .arg(f.0.join("programs/bad.ts"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let errors: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(errors[0]["code"].as_str().unwrap().starts_with("parse."));
    fs::write(f.0.join("programs/yl.json"), "{}").unwrap();
    assert_eq!(
        Project::discover(&f.0.join("programs/good.ts")).unwrap_err()[0].code,
        "yl.project"
    );
}

#[test]
fn invalid_configuration_and_unsupported_globs_are_structured() {
    for source in [
        "not json",
        r#"{"entry":"x","files":[]}"#,
        r#"{"entry":"x","files":["../*.ts"]}"#,
        r#"{"entry":"x","files":["[ab].ts"]}"#,
        r#"{"entry":"x","files":["*.ts"],"highlighting":{}}"#,
    ] {
        assert_eq!(
            Project::parse(std::path::Path::new("yl.json"), source).unwrap_err()[0].code,
            "yl.project"
        );
    }
    let f = Fixture::new();
    fs::write(f.0.join("yl.json"), "broken").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["project", "check"])
        .arg(f.0.join("programs/good.ts"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let errors: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(errors[0]["code"], "yl.project");
}
