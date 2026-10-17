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
