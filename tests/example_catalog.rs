use std::path::Path;
use your_language::project::Project;

#[test]
fn every_numbered_example_is_a_complete_project_with_text_programs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let projects = Project::list(&root).unwrap();
    assert_eq!(projects.len(), 14);
    for (index, project) in projects.iter().enumerate() {
        let directory = project.config_path.parent().unwrap();
        let name = directory.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with(&format!("{:02}-", index + 1)), "{name}");
        assert!(!name.contains('_'), "{name}");
        assert!(project.entry.is_file(), "{}", project.entry.display());
        assert!(directory.join("definition").is_dir(), "{name}");
        assert!(directory.join("programs").is_dir(), "{name}");
        assert!(directory.join("README.md").is_file(), "{name}");
        let sources = project.sources().unwrap();
        assert!(!sources.is_empty(), "{name}");
        for source in sources {
            assert_eq!(source.extension().unwrap(), "txt", "{}", source.display());
            assert!(source.starts_with(directory.join("programs")));
        }
    }
}

#[test]
fn every_project_has_both_numbered_and_friendly_cargo_entrypoints() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "your-language")
        .unwrap();
    let targets = package["targets"].as_array().unwrap();
    for project in Project::list(&root.join("examples")).unwrap() {
        let name = project
            .config_path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        for alias in [name, &name[3..]] {
            let target = targets
                .iter()
                .find(|target| target["name"] == alias)
                .unwrap_or_else(|| panic!("Missing cargo run --example {alias}"));
            assert_eq!(target["kind"][0], "example");
            let source = std::fs::read_to_string(target["src_path"].as_str().unwrap()).unwrap();
            assert!(
                source.contains(&format!("/examples/{name}/yl.json")),
                "{alias}"
            );
            assert!(
                source.contains("your_language::project::run_example"),
                "{alias}"
            );
        }
    }
}
