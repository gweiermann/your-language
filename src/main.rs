use std::{env, fs, path::Path, process::ExitCode};
use your_language::{
    compile_language,
    diagnostic::{Diagnostic, Span},
    load_compiled_language, parse_named,
};

fn run() -> Result<bool, Vec<Diagnostic>> {
    let args: Vec<_> = env::args().skip(1).collect();
    let usage = || {
        vec![Diagnostic::error("yl.cli","Usage: yl check <entry.yl> | yl compile <entry.yl> -o <language.ylc> | yl parse <language.ylc> <source> [--json]",Span::default())]
    };
    let io = |e: std::io::Error| vec![Diagnostic::error("yl.io", e.to_string(), Span::default())];
    match args.first().map(String::as_str) {
        Some("check") if args.len() == 2 => {
            compile_language(&args[1])?;
            println!("Language definition is valid");
            Ok(true)
        }
        Some("compile") if args.len() == 4 && args[2] == "-o" => {
            let language = compile_language(&args[1])?;
            fs::write(Path::new(&args[3]), language.to_bytes()?).map_err(io)?;
            println!("Compiled {}", args[3]);
            Ok(true)
        }
        Some("parse") if args.len() == 3 || (args.len() == 4 && args[3] == "--json") => {
            let language = load_compiled_language(&fs::read(&args[1]).map_err(io)?)?;
            let source = fs::read_to_string(&args[2]).map_err(io)?;
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
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(diagnostics) => {
            match serde_json::to_string_pretty(&diagnostics) {
                Ok(json) => eprintln!("{json}"),
                Err(_) => eprintln!("Unable to serialize diagnostics"),
            }
            ExitCode::FAILURE
        }
    }
}
