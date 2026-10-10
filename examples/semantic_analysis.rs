//! An in-memory consumer of the experimental native semantic interface.
use std::collections::BTreeMap;
use your_language::semantics::SemanticEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let choice = std::env::args().nth(1).unwrap_or_else(|| "program".into());
    let (file, source) = match choice.as_str() {
        "program" => ("program.txt", include_str!("semantics/program.txt")),
        "unresolved" => ("unresolved.txt", include_str!("semantics/unresolved.txt")),
        "duplicate" => ("duplicate.txt", include_str!("semantics/duplicate.txt")),
        "initializer" => (
            "initializer-error.txt",
            include_str!("semantics/initializer-error.txt"),
        ),
        _ => return Err("Choose program, unresolved, duplicate, or initializer".into()),
    };
    let engine = SemanticEngine::default();
    let sources = BTreeMap::from([(
        "language.yl".into(),
        include_str!("semantics/language.yl").into(),
    )]);
    // No artifact files, reload, or repeated library registration are required.
    let analysis = engine
        .compile_and_analyze("language.yl", &sources, file, source)
        .map_err(|diagnostics| format!("Language definition failed: {diagnostics:#?}"))?;
    println!("{}", serde_json::to_string_pretty(&analysis)?);
    if analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == your_language::diagnostic::Severity::Error)
    {
        std::process::exit(1);
    }
    Ok(())
}
