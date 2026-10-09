use std::{env, fs, path::Path, process::ExitCode};
use your_language::{
    compile_language,
    diagnostic::{Diagnostic, Span},
    load_compiled_language, parse_named, ParseResult,
};

fn run(args: &[String]) -> Result<bool, Vec<Diagnostic>> {
    let usage = || {
        vec![Diagnostic::error("yl.cli","Usage: yl check <entry.yl> | yl compile <entry.yl> -o <language.ylc> | yl parse <language.ylc> <source> [--json]",Span::default())]
    };
    let io = |path: &str, e: std::io::Error| {
        vec![Diagnostic::error(
            "yl.io",
            e.to_string(),
            Span::new(path, 0, 0),
        )]
    };
    match args.first().map(String::as_str) {
        Some("check") if args.len() == 2 => {
            compile_language(&args[1])?;
            println!("Language definition is valid");
            Ok(true)
        }
        Some("compile") if args.len() == 4 && args[2] == "-o" => {
            let language = compile_language(&args[1])?;
            fs::write(Path::new(&args[3]), language.to_bytes()?).map_err(|e| io(&args[3], e))?;
            println!("Compiled {}", args[3]);
            Ok(true)
        }
        Some("parse") if args.len() == 3 || (args.len() == 4 && args[3] == "--json") => {
            let bytes = fs::read(&args[1]).map_err(|e| io(&args[1], e))?;
            let language = load_compiled_language(&bytes).map_err(|mut errors| {
                for error in &mut errors {
                    if error.primary.file.is_empty() {
                        error.primary.file = args[1].clone();
                    }
                }
                errors
            })?;
            let source = fs::read_to_string(&args[2]).map_err(|e| io(&args[2], e))?;
            let result = parse_named(&language, &args[2], &source);
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|e| vec![Diagnostic::error(
                    "yl.json",
                    e.to_string(),
                    Span::default()
                )])?
            );
            Ok(!result.has_errors())
        }
        _ => Err(usage()),
    }
}
fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let machine =
        args.first().is_some_and(|a| a == "parse") && args.get(3).is_some_and(|a| a == "--json");
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(diagnostics) => {
            let serialized = if machine {
                serde_json::to_string_pretty(&ParseResult {
                    ast: None,
                    diagnostics,
                })
            } else {
                serde_json::to_string_pretty(&diagnostics)
            };
            match serialized {
                Ok(json) if machine => println!("{json}"),
                Ok(json) => eprintln!("{json}"),
                Err(_) => eprintln!("Unable to serialize diagnostics"),
            }
            ExitCode::FAILURE
        }
    }
}
