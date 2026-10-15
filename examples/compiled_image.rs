//! Inspect and exercise a prepared language image without keeping its source modules.
use std::path::PathBuf;
use std::time::Instant;
use your_language::{compile_language, load_compiled_language, semantics::SemanticEngine};

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let (entry, source) = match args.as_slice() {
        [] => (
            PathBuf::from("examples/mini-js/minijs.yl"),
            PathBuf::from("examples/mini-js/program.js"),
        ),
        [entry, source] => (entry.into(), source.into()),
        _ => return Err("Usage: cargo run --example compiled_image -- [entry.yl source]".into()),
    };
    let start = Instant::now();
    let language = compile_language(&entry).map_err(|errors| format!("{errors:?}"))?;
    let compiled_in = start.elapsed();
    let bytes = language
        .to_bytes()
        .map_err(|errors| format!("{errors:?}"))?;
    drop(language);
    let start = Instant::now();
    let loaded = load_compiled_language(&bytes).map_err(|errors| format!("{errors:?}"))?;
    let loaded_in = start.elapsed();
    let image = loaded.executable_image();
    println!("image version: {}", image.version);
    println!("encoded bytes: {}", bytes.len());
    println!(
        "parser rules / instructions / checks: {} / {} / {}",
        image.parser.rules.len(),
        image.parser.instructions.len(),
        image.parser.checks.len()
    );
    println!(
        "query expressions / actions: {} / {}",
        image.queries.expressions.len(),
        image.queries.actions.len()
    );
    println!("compile: {compiled_in:?}; checked load: {loaded_in:?}");
    let text = std::fs::read_to_string(&source).map_err(|error| error.to_string())?;
    let engine = SemanticEngine::default();
    let start = Instant::now();
    let result = engine.analyze(&loaded, &source.to_string_lossy(), &text);
    println!(
        "analyze: {:?}; AST: {}; diagnostics: {}",
        start.elapsed(),
        result.ast.is_some(),
        result.diagnostics.len()
    );
    if result
        .diagnostics
        .iter()
        .any(|error| error.severity == your_language::diagnostic::Severity::Error)
    {
        return Err(format!("{:?}", result.diagnostics));
    }
    Ok(())
}
