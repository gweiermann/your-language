#[path = "../examples/14-backend-hooks/hosts/mod.rs"]
mod hosts;
use std::{path::Path, process::Command};
use your_language::backend::Backend;

fn source(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/14-backend-hooks/programs")
            .join(name),
    )
    .unwrap()
}
fn run_node(arguments: &[&str]) -> String {
    let output = Command::new("node")
        .args(arguments)
        .output()
        .expect("Node.js is required to execute backend example artifacts");
    assert!(
        output.status.success(),
        "Node failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
#[test]
fn same_reloaded_language_interprets_transpiles_and_compiles_real_wasm() {
    let language = hosts::load().unwrap();
    for (name, expected) in [
        ("valid.txt", 42),
        ("control-flow.txt", 42),
        ("shadowing.txt", 42),
        ("initializer-shadowing.txt", 42),
        ("overflow.txt", i32::MIN),
    ] {
        let source = source(name);
        let program = language.prepare_backend(name, &source).unwrap();
        let mut interpreter = hosts::Interpreter::default();
        assert_eq!(program.run(&mut interpreter).unwrap(), expected, "{name}");
        if name == "control-flow.txt" {
            assert_eq!(interpreter.loop_iterations, 3);
        }
        let javascript = program.run(&mut hosts::JavaScript::default()).unwrap();
        if name == "control-flow.txt" {
            assert!(javascript.contains("else"));
            assert!(javascript.contains("999"));
        }
        assert_eq!(
            run_node(&["-e", &javascript]),
            expected.to_string(),
            "{name}"
        );
        let mut wasm = hosts::Wasm::default();
        let instructions = program.run(&mut wasm).unwrap();
        if name == "control-flow.txt" {
            assert!(instructions
                .iter()
                .any(|i| matches!(i, wasm_encoder::Instruction::Else)));
            assert!(instructions
                .iter()
                .any(|i| matches!(i, wasm_encoder::Instruction::Loop(_))));
        }
        let bytes = wasm.finish(&instructions);
        let path =
            std::env::temp_dir().join(format!("yl-hooks-{}-{name}.wasm", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let value = run_node(&["-e", "const fs=require('node:fs'); const b=fs.readFileSync(process.argv[1]); if(!WebAssembly.validate(b))throw Error('invalid Wasm'); console.log(new WebAssembly.Instance(new WebAssembly.Module(b)).exports.run());", path.to_str().unwrap()]);
        std::fs::remove_file(path).unwrap();
        assert_eq!(value, expected.to_string(), "{name}");
    }
}
#[test]
fn interpreter_does_not_dispatch_unchosen_branch() {
    let language = hosts::load().unwrap();
    let program = language.prepare_backend("untaken", "let answer = 42; if (0) { answer = missing; } else { answer = answer; } result answer;").unwrap();
    assert_eq!(program.run(&mut hosts::Interpreter::default()).unwrap(), 42);
}
#[test]
fn undefined_variable_has_source_location_and_hosts_validate_fixed_bindings() {
    let language = hosts::load().unwrap();
    let program = language
        .prepare_backend("undefined", "result missing;")
        .unwrap();
    let error = program.run(&mut hosts::Interpreter::default()).unwrap_err();
    assert_eq!(error.code, "example.undefined");
    assert_eq!(error.primary.start, 7);
    assert_eq!(error.primary.end, 14);
    assert_eq!(hosts::Interpreter::default().bindings().len(), 11);
}

#[test]
fn bounded_keywords_do_not_split_identifier_prefixes() {
    let language = hosts::load().unwrap();
    // This is an assignment to the identifier `letanswer`, not a declaration of `answer`.
    let invalid = language
        .prepare_backend("bad-keyword", "letanswer = 42; result answer;")
        .unwrap();
    assert!(invalid.run(&mut hosts::Interpreter::default()).is_err());
    let program = language
        .prepare_backend("identifier", "let resultValue = 42; result resultValue;")
        .unwrap();
    assert_eq!(program.run(&mut hosts::Interpreter::default()).unwrap(), 42);
}
