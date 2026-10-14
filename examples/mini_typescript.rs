//! Mini TypeScript: authored reference and type queries through the public API.
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
        "generics" => include_str!("mini_typescript/programs/generics.txt"),
        "generics-errors" => include_str!("mini_typescript/programs/generics-errors.txt"),
        "higher-order" => include_str!("mini_typescript/programs/higher-order.txt"),
        "union" => include_str!("mini_typescript/programs/union.txt"),
        "union-errors" => include_str!("mini_typescript/programs/union-errors.txt"),
        "addition" => include_str!("mini_typescript/programs/addition.txt"),
        "addition-errors" => include_str!("mini_typescript/programs/addition-errors.txt"),
        _ => return Err(
            "Choose program, errors, generics, generics-errors, higher-order, union, union-errors, addition or addition-errors, optionally --json"
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
        for (name, ty) in inference::bindings(&result) {
            println!("{name}: {ty}");
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
