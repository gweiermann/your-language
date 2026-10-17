//! An in-memory consumer of authored reference analysis.
use your_language::{
    diagnostic::Severity, diagnostic_render::print_diagnostics, semantics::SemanticEngine,
};

#[path = "06-references/hosts/definition.rs"]
mod definition;

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
        "program" => (
            "programs/program.txt",
            include_str!("06-references/programs/program.txt"),
        ),
        "unresolved" => (
            "programs/unresolved.txt",
            include_str!("06-references/programs/unresolved.txt"),
        ),
        "duplicate" => (
            "programs/duplicate.txt",
            include_str!("06-references/programs/duplicate.txt"),
        ),
        "initializer" => (
            "programs/initializer-error.txt",
            include_str!("06-references/programs/initializer-error.txt"),
        ),
        _ => return Err("Choose program, unresolved, duplicate, or initializer".into()),
    };
    let engine = SemanticEngine::default();
    let sources = definition::sources();
    // No artifact files, reload, or repeated library registration are required.
    let analysis = match engine.compile_and_analyze(definition::ENTRY, &sources, file, source) {
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
            "Analysis succeeded ({} attachments).",
            analysis.graph.query_entries.len()
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
