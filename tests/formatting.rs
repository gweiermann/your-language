use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use your_language::{format_yl, frontend::parse_yl};
fn tree(source: &str) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                fields.remove("span");
                for value in fields.values_mut() {
                    strip(value);
                }
            }
            serde_json::Value::Array(items) => {
                for value in items {
                    strip(value);
                }
            }
            _ => {}
        }
    }
    let mut result = serde_json::to_value(parse_yl("source", source).unwrap()).unwrap();
    strip(&mut result);
    result
}
#[test]
fn formats_semantic_blocks_without_changing_syntax() {
    let source = "import{scope}from\"lib\"node Program=name:/[a-z]+/ {meanings{boundary scope() group Register{scope()} precedence{Program>Child}}}entry Program";
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(tree(source), tree(&formatted));
    assert!(formatted.contains("import { scope } from \"lib\""));
    assert!(formatted.contains("node Program =\n    name: /[a-z]+/\n{\n    meanings {\n        boundary\n        scope()\n"), "{formatted}");
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}
#[test]
fn preserves_comments_and_raw_string_regex_spellings() {
    let source = r#"// module
node A="line\n\t\"literal" // trailing A
// docs B
node B=/a\/b[{}]*/ { /* inner */ meanings{boundary} }
/* multiline
   exact contents */
entry B"#;
    let formatted = format_yl("source", source).unwrap();
    for spelling in [
        "// module",
        "// trailing A",
        "// docs B",
        "/* inner */",
        "/* multiline\n   exact contents */",
        r#""line\n\t\"literal""#,
        r#"/a\/b[{}]*/"#,
    ] {
        assert!(
            formatted.contains(spelling),
            "Lost {spelling}:\n{formatted}"
        );
    }
    assert!(formatted.contains("// docs B\nnode B"), "{formatted}");
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}
#[test]
fn every_valid_checked_in_definition_is_parse_equivalent_and_idempotent() {
    fn visit(directory: &Path, paths: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, paths);
            } else if path.extension().is_some_and(|ext| ext == "yl") {
                paths.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = vec![];
    visit(&root.join("examples"), &mut paths);
    visit(&root.join("tests/fixtures"), &mut paths);
    let mut checked = 0;
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        if parse_yl("source", &source).is_err() {
            continue;
        }
        let formatted = format_yl("source", &source)
            .unwrap_or_else(|errors| panic!("{}: {errors:?}", path.display()));
        assert_eq!(tree(&source), tree(&formatted), "{}", path.display());
        assert_eq!(
            format_yl("source", &formatted).unwrap(),
            formatted,
            "{}",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 10, "Only {checked} fixtures exercised");
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path())
            && self
                .0
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("yl-fmt-test-"))
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn scratch() -> Scratch {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "yl-fmt-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    Scratch(path)
}
#[test]
fn cli_check_is_read_only_and_invalid_batches_are_not_modified() {
    let root = scratch();
    let valid = root.0.join("valid.yl");
    let invalid = root.0.join("invalid.yl");
    let source = "node A=\"a\" entry A";
    fs::write(&valid, source).unwrap();
    fs::write(&invalid, "node =").unwrap();
    let run = |args: &[&str], files: &[&Path]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yl"));
        command.args(args);
        for file in files {
            command.arg(file);
        }
        command.output().unwrap()
    };
    let check = run(&["fmt", "--check"], &[&valid]);
    assert_eq!(check.status.code(), Some(1));
    assert!(check.stderr.is_empty());
    assert_eq!(fs::read_to_string(&valid).unwrap(), source);
    assert!(String::from_utf8(check.stdout)
        .unwrap()
        .contains("Would format"));
    let failure = run(&["fmt"], &[&valid, &invalid]);
    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8(failure.stderr)
        .unwrap()
        .contains("yl.unexpected_token"));
    assert_eq!(fs::read_to_string(&valid).unwrap(), source);
    assert_eq!(fs::read_to_string(&invalid).unwrap(), "node =");
    let write = run(&["fmt"], &[&valid]);
    assert!(
        write.status.success(),
        "{}",
        String::from_utf8_lossy(&write.stderr)
    );
    let formatted = fs::read_to_string(&valid).unwrap();
    assert_eq!(formatted, format_yl("source", source).unwrap());
    let clean = run(&["fmt", "--check"], &[&valid]);
    assert!(clean.status.success());
    assert!(clean.stdout.is_empty());
    assert_eq!(
        fs::read_dir(&root.0).unwrap().count(),
        2,
        "Temporary formatter files leaked"
    );
}
#[test]
fn invalid_source_returns_structured_diagnostics() {
    let errors = format_yl("broken.yl", "node X = \"unterminated").unwrap_err();
    assert_eq!(errors[0].code, "yl.unterminated_literal");
    assert_eq!(errors[0].primary.file, "broken.yl");
}

#[test]
fn preserves_comment_trailing_spaces_and_literal_line_endings() {
    let source = "// comment with spaces   \r\nnode A=\"line\r\ncontents\" // tail   \r\nentry A";
    let formatted = format_yl("source", source).unwrap();
    assert!(
        formatted.contains("// comment with spaces   \n"),
        "{formatted:?}"
    );
    assert!(formatted.contains("// tail   \n"), "{formatted:?}");
    assert!(formatted.contains("\"line\r\ncontents\""), "{formatted:?}");
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
}

#[test]
fn supports_enums_constraints_extensions_and_tuple_rewrite_cases() {
    let source = r#"
        enum Mode{a,b}
        constraint named(value: Node){when value.name?.matches(/a\/b/)&&value.isPresent{warning("check")}}
        node Name=/[a-z]+/ node Family{node Name precedence{right Name} constraints{named(this)}}
        extend node Family{constraints{when this.isPresent{help("extended")}}}
        pipe wrap(first: Mode=.a,second: Mode=.b){rewrite value* match(first,second){(.a,.a)=>"(" value ")" (.a,.b)=>{warning("pair") value}(_,_)=>{error("unsupported")}}}
        pattern Many(item: Node)=item* |> wrap(.a,.b)
        entry Family{trivia Name}
    "#;
    let formatted = format_yl("source", source).unwrap();
    assert_eq!(tree(source), tree(&formatted));
    assert_eq!(format_yl("source", &formatted).unwrap(), formatted);
    assert!(
        formatted.contains("enum Mode {\n    a,\n    b\n}"),
        "{formatted}"
    );
    assert!(
        formatted.contains("rewrite value* match (first, second) {"),
        "{formatted}"
    );
}

#[test]
fn optional_pattern_and_contextual_variant_do_not_merge_into_optional_chaining() {
    let source = "pattern A = /a/? .b";
    let formatted = format_yl("source", source).unwrap();
    assert!(formatted.contains("/a/? .b"), "{formatted}");
    assert_eq!(tree(source), tree(&formatted));
}

#[test]
fn cli_read_only_files_and_literal_dash_paths_are_safe() {
    let root = scratch();
    let path = root.0.join("-leading.yl");
    let earlier = root.0.join("earlier.yl");
    let source = "node A=\"a\" entry A";
    fs::write(&path, source).unwrap();
    fs::write(&earlier, source).unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--", "earlier.yl", "-leading.yl"])
        .current_dir(&root.0)
        .output()
        .unwrap();
    fs::set_permissions(&path, original).unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&earlier).unwrap(), source);
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert!(String::from_utf8(failed.stderr)
        .unwrap()
        .contains("read-only"));
    let written = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--", "-leading.yl"])
        .current_dir(&root.0)
        .output()
        .unwrap();
    assert!(written.status.success());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format_yl("source", source).unwrap()
    );
    let duplicate = Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["fmt", "--check"])
        .arg(&path)
        .arg(&path)
        .output()
        .unwrap();
    assert!(duplicate.status.success());
    assert!(duplicate.stdout.is_empty());
}
