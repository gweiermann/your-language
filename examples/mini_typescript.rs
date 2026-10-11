//! Throwaway Mini TypeScript inference through public native semantic APIs.
#[path = "mini_typescript/definition.rs"]
mod definition;
#[path = "mini_typescript/inference.rs"]
mod inference;
use your_language::{diagnostic::Severity, diagnostic_render::print_diagnostics};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let json = args.iter().any(|arg| arg == "--json");
    let choice = args
        .iter()
        .find(|arg| arg.as_str() != "--json")
        .map(String::as_str)
        .unwrap_or("program");
    if choice == "examples" {
        let engine = inference::engine();
        let language = engine
            .compile(definition::ENTRY, &definition::sources())
            .map_err(|errors| format!("{errors:?}"))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&language.example_previews())?
        );
        return Ok(());
    }
    let source = match choice {
        "program" => include_str!("mini_typescript/programs/program.txt"),
        "errors" => include_str!("mini_typescript/programs/errors.txt"),
        "higher-order" => include_str!("mini_typescript/programs/higher-order.txt"),
        "addition" => include_str!("mini_typescript/programs/addition.txt"),
        "addition-errors" => include_str!("mini_typescript/programs/addition-errors.txt"),
        _ => return Err(
            "Choose program, errors, higher-order, addition or addition-errors, optionally --json"
                .into(),
        ),
    };
    let sources = definition::sources();
    let engine = inference::engine();
    let result = match engine.compile_and_analyze(definition::ENTRY, &sources, choice, source) {
        Ok(result) => result,
        Err(diagnostics) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&diagnostics)?);
            } else {
                print_diagnostics(&diagnostics, |span| sources.get(&span.file).cloned());
            }
            std::process::exit(1);
        }
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_diagnostics(&result.diagnostics, |span| {
            if span.file == choice {
                Some(source.into())
            } else {
                sources.get(&span.file).cloned()
            }
        });
        for record in &result.graph.records {
            let name = record
                .entries
                .iter()
                .find_map(|(key, value)| match (key, value) {
                    (
                        your_language::semantics::SemanticKey::Symbol(key),
                        your_language::semantics::SemanticValue::Text(value),
                    ) if key == "std/variables#nameSymbol" => Some(value),
                    _ => None,
                });
            let ty = inference::display_type(record);
            if let (Some(name), Some(ty)) = (name, ty) {
                println!("{name}: {ty}");
            }
        }
    }
    if result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        std::process::exit(1);
    }
    Ok(())
}
