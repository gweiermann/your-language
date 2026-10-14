//! Authored hypothetical type relationships through the public Rust API.
use your_language::{
    compile_language,
    diagnostic::Severity,
    diagnostic_render::print_diagnostics,
    semantics::{queries::QueryKey, SemanticEngine},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let errors = args.iter().any(|arg| arg == "errors");
    let json = args.iter().any(|arg| arg == "--json");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let engine = SemanticEngine::default();
    let language = compile_language(root.join("examples/type_instantiation/definition/main.yl"))
        .map_err(|errors| format!("{errors:?}"))?;
    let loaded = engine
        .load(
            &language
                .to_bytes()
                .map_err(|errors| format!("{errors:?}"))?,
        )
        .map_err(|errors| format!("{errors:?}"))?;
    let file = if errors { "errors.txt" } else { "program.txt" };
    let source =
        std::fs::read_to_string(root.join("examples/type_instantiation/programs").join(file))?;
    let result = engine.analyze(&loaded, file, &source);
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_diagnostics(&result.diagnostics, |span| {
            if span.file == file {
                Some(source.clone())
            } else if span.file == "std/types" {
                Some(include_str!("../stdlib/types.yl").into())
            } else {
                None
            }
        });
        for entry in &result.graph.query_entries {
            if let QueryKey::Text(name) = &entry.key {
                println!("{name}: {}", serde_json::to_string(&entry.value)?);
            }
        }
    }
    if result
        .diagnostics
        .iter()
        .any(|error| error.severity == Severity::Error)
    {
        std::process::exit(1);
    }
    Ok(())
}
