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
            "yl-runner-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn project(&self, name: &str) -> PathBuf {
        let dir = self.0.join(name);
        fs::create_dir_all(dir.join("programs")).unwrap();
        fs::write(dir.join("yl.json"), r#"{"entry":"language.yl","files":["programs/**/*.txt"],"expectErrors":["programs/bad.txt"]}"#).unwrap();
        fs::write(dir.join("language.yl"), "node Root = /[0-9]+/ entry Root").unwrap();
        fs::write(dir.join("programs/good.txt"), "42").unwrap();
        fs::write(dir.join("programs/bad.txt"), "wrong").unwrap();
        dir
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(command: &str, path: impl AsRef<std::ffi::OsStr>) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["project", command])
        .arg(path)
        .arg("--json")
        .output()
        .unwrap()
}
#[test]
fn projects_are_sorted_and_excluded_build_directories_are_not_discovered() {
    let f = Fixture::new();
    f.project("02-second");
    f.project("01-first");
    fs::create_dir_all(f.0.join("target/hidden")).unwrap();
    fs::write(f.0.join("target/hidden/yl.json"), "not json").unwrap();
    let output = run("list", &f.0);
    assert!(output.status.success(), "{:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 2);
    assert!(json[0]["config"].as_str().unwrap().contains("01-first"));
    assert_eq!(json[0]["sources"][0]["expectErrors"], true);
    assert_eq!(json[0]["sources"][1]["expectErrors"], false);
}
#[test]
fn expected_errors_pass_and_unexpected_errors_and_success_fail() {
    let f = Fixture::new();
    let dir = f.project("sample");
    let output = run("check", &dir);
    assert!(output.status.success(), "{:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["sources"][0]["diagnostics"][0]["severity"], "error");
    fs::write(dir.join("programs/bad.txt"), "123").unwrap();
    let output = run("check", dir.join("yl.json"));
    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["sources"][0]["success"], false);
    assert_eq!(json["sources"][0]["diagnostics"][0]["code"], "yl.project");
    fs::write(dir.join("programs/bad.txt"), "wrong").unwrap();
    fs::write(dir.join("programs/good.txt"), "wrong").unwrap();
    assert!(!run("check", &dir).status.success());
}
#[test]
fn nearest_invalid_config_is_not_ignored_by_batch_or_list() {
    let f = Fixture::new();
    let dir = f.project("sample");
    fs::write(dir.join("programs/yl.json"), "broken").unwrap();
    for command in ["check", "list"] {
        let output = run(command, &dir);
        assert!(!output.status.success());
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json[0]["code"], "yl.project");
    }
}
#[test]
fn expectation_globs_use_the_same_validation_and_nested_projects_own_their_sources() {
    for pattern in ["../bad.txt", "[ab].txt", "C:/bad.txt"] {
        let source = serde_json::json!({"entry":"language.yl", "files":["**/*.txt"], "expectErrors":[pattern]}).to_string();
        assert!(Project::parse(std::path::Path::new("yl.json"), &source).is_err());
    }
    let f = Fixture::new();
    let dir = f.project("sample");
    fs::create_dir_all(dir.join("programs/nested")).unwrap();
    fs::write(
        dir.join("programs/nested/yl.json"),
        r#"{"entry":"language.yl","files":["*.txt"]}"#,
    )
    .unwrap();
    fs::write(dir.join("programs/nested/other.txt"), "x").unwrap();
    assert_eq!(Project::load(&dir).unwrap().sources().unwrap().len(), 2);
}
