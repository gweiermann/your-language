//! An in-memory consumer of the experimental native semantic interface.
use std::collections::BTreeMap;
use your_language::{
    diagnostic::Severity, diagnostic_render::print_diagnostics, semantics::SemanticEngine,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut choice = "program";
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut json = false;
    let mut selected = false;
    for arg in &args {
        if arg == "--json" {
            json = true;
        } else if !selected {
            choice = arg;
            selected = true;
        } else {
            return Err("Choose one example and optionally --json".into());
        }
    }
    let (file, source) = match choice {
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
    let analysis = match engine.compile_and_analyze("language.yl", &sources, file, source) {
        Ok(analysis) => analysis,
        Err(diagnostics) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&diagnostics)?);
            } else {
                print_diagnostics(&diagnostics, |span| sources.get(&span.file).cloned());
            }
            std::process::exit(1);
        }
    };
    let errors = analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    if json {
        println!("{}", serde_json::to_string_pretty(&analysis)?);
    } else if !errors {
        println!(
            "Analysis succeeded ({} semantic records).",
            analysis.graph.records.len()
        );
    }
    if !json {
        print_diagnostics(&analysis.diagnostics, |span| {
            if span.file == file {
                Some(source.into())
            } else {
                sources.get(&span.file).cloned()
            }
        });
    }
    if errors {
        std::process::exit(1);
    }
    Ok(())
}
