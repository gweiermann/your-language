mod format_command;

use std::{env, fs, path::Path, process::ExitCode};
use your_language::diagnostic_render::print_diagnostics;
use your_language::{
    compile_language,
    diagnostic::{Diagnostic, Span},
    load_compiled_language, parse_named,
};

fn run(args: &[String]) -> Result<bool, Vec<Diagnostic>> {
    let usage = || {
        vec![Diagnostic::error("yl.cli","Usage: yl project check|ast <source> [--json] | yl lsp | yl examples <entry.yl|language.ylc> | yl fmt [--check] [--json] [--align-match-arrows] <files...> | yl check <entry.yl> | yl compile <entry.yl> -o <language.ylc> | yl language <language.ylc> ast <source> | yl language <language.ylc> check <source> [--json]",Span::default())]
    };
    let io = |path: &str, e: std::io::Error| {
        vec![Diagnostic::error(
            "yl.io",
            e.to_string(),
            Span::new(path, 0, 0),
        )]
    };
    match args.first().map(String::as_str) {
        Some("project")
            if (args.len() == 3
                || args.len() == 4 && args[3] == "--json" && args[1] == "check")
                && matches!(args[1].as_str(), "check" | "ast") =>
        {
            let project = your_language::project::Project::discover(Path::new(&args[2]))?
                .filter(|project| project.matches(Path::new(&args[2])))
                .ok_or_else(|| {
                    vec![Diagnostic::error(
                        "yl.project",
                        "Source does not match a nearest yl.json project",
                        Span::new(&args[2], 0, 0),
                    )]
                })?;
            let language = compile_language(&project.entry).map_err(|mut errors| {
                let root = project.entry.parent().unwrap_or(Path::new("."));
                project_diagnostic_paths(&mut errors, root, None);
                errors
            })?;
            let source = fs::read_to_string(&args[2]).map_err(|e| io(&args[2], e))?;
            let mut result = if args[1] == "check" && language.has_declarative_meanings() {
                let analysis = your_language::semantics::SemanticEngine::default()
                    .analyze(&language, &args[2], &source);
                your_language::ParseResult {
                    ast: analysis.ast,
                    diagnostics: analysis.diagnostics,
                }
            } else {
                parse_named(&language, &args[2], &source)
            };
            let mut diagnostics = language.diagnostics().to_vec();
            diagnostics.append(&mut result.diagnostics);
            result.diagnostics = diagnostics;
            project_diagnostic_paths(
                &mut result.diagnostics,
                project.entry.parent().unwrap_or(Path::new(".")),
                Some(&args[2]),
            );
            if args[1] == "ast" {
                println!("{}", json(&result.ast)?);
                if !result.diagnostics.is_empty() {
                    eprintln!("{}", json(&result.diagnostics)?);
                }
            } else if args.len() == 4 {
                println!("{}", json(&result.diagnostics)?);
            } else {
                print_diagnostics(&result.diagnostics, |span| {
                    fs::read_to_string(&span.file).ok()
                });
            }
            Ok(!result.has_errors())
        }
        Some("examples") if args.len() == 2 => {
            let language = if Path::new(&args[1]).extension().is_some_and(|e| e == "ylc") {
                load_compiled_language(&fs::read(&args[1]).map_err(|e| io(&args[1], e))?)?
            } else {
                compile_language(&args[1])?
            };
            println!("{}", json(&language.example_previews())?);
            Ok(true)
        }
        Some("fmt") => format_command::run(&args[1..]),
        Some("check") if args.len() == 2 => {
            let language = compile_language(&args[1])?;
            print_definition_diagnostics(&language, &args[1]);
            println!("Language definition is valid");
            Ok(true)
        }
        Some("compile") if args.len() == 4 && args[2] == "-o" => {
            let language = compile_language(&args[1])?;
            print_definition_diagnostics(&language, &args[1]);
            fs::write(Path::new(&args[3]), language.to_bytes()?).map_err(|e| io(&args[3], e))?;
            println!("Compiled {}", args[3]);
            Ok(true)
        }
        Some("language")
            if args.len() == 4 && matches!(args[2].as_str(), "ast" | "check")
                || args.len() == 5 && args[2] == "check" && args[4] == "--json" =>
        {
            let bytes = fs::read(&args[1]).map_err(|e| io(&args[1], e))?;
            let language = load_compiled_language(&bytes).map_err(|mut errors| {
                for error in &mut errors {
                    if error.primary.file.is_empty() {
                        error.primary.file = args[1].clone();
                    }
                }
                errors
            })?;
            let source = fs::read_to_string(&args[3]).map_err(|e| io(&args[3], e))?;
            let result = if args[2] == "check" && language.has_declarative_meanings() {
                let analysis = your_language::semantics::SemanticEngine::default()
                    .analyze(&language, &args[3], &source);
                your_language::ParseResult {
                    ast: analysis.ast,
                    diagnostics: analysis.diagnostics,
                }
            } else {
                parse_named(&language, &args[3], &source)
            };
            if args[2] == "ast" {
                println!("{}", json(&result.ast)?);
                if !result.diagnostics.is_empty() {
                    eprintln!("{}", json(&result.diagnostics)?);
                }
            } else if args.get(4).is_some_and(|flag| flag == "--json") {
                println!("{}", json(&result.diagnostics)?);
            } else {
                print_diagnostics(&result.diagnostics, |span| {
                    if span.file == args[3] {
                        Some(source.clone())
                    } else {
                        fs::read_to_string(&span.file).ok()
                    }
                });
            }
            Ok(!result.has_errors())
        }
        _ => Err(usage()),
    }
}
fn project_diagnostic_paths(diagnostics: &mut [Diagnostic], root: &Path, source: Option<&str>) {
    for diagnostic in diagnostics {
        for span in std::iter::once(diagnostic.primary.as_mut())
            .chain(diagnostic.secondary.iter_mut())
            .chain(
                diagnostic
                    .annotations
                    .iter_mut()
                    .map(|annotation| &mut annotation.span),
            )
        {
            if source != Some(span.file.as_str()) && Path::new(&span.file).is_relative() {
                span.file = root.join(&span.file).to_string_lossy().into();
            }
        }
    }
}
fn json(value: &impl serde::Serialize) -> Result<String, Vec<Diagnostic>> {
    serde_json::to_string_pretty(value)
        .map_err(|e| vec![Diagnostic::error("yl.json", e.to_string(), Span::default())])
}
fn main() -> ExitCode {
    if env::args().skip(1).eq(["lsp"]) {
        return match your_language::language_server::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("YL language server: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let args: Vec<_> = env::args().skip(1).collect();
    let diagnostic_json = (args.first().is_some_and(|a| a == "language")
        && args.get(2).is_some_and(|a| a == "check")
        && args.get(4).is_some_and(|a| a == "--json"))
        || (args.first().is_some_and(|a| a == "project")
            && args.get(1).is_some_and(|a| a == "check")
            && args.get(3).is_some_and(|a| a == "--json"));
    let ast_json = args.first().is_some_and(|a| a == "language")
        && args.get(2).is_some_and(|a| a == "ast")
        || args.first().is_some_and(|a| a == "project") && args.get(1).is_some_and(|a| a == "ast");
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(diagnostics) => {
            if diagnostic_json || ast_json {
                if ast_json {
                    println!("null");
                }
                match json(&diagnostics) {
                    Ok(output) if diagnostic_json => println!("{output}"),
                    Ok(output) => eprintln!("{output}"),
                    Err(_) => eprintln!("Unable to serialize diagnostics"),
                }
            } else if args.first().is_some_and(|command| {
                command == "check" || command == "compile" || command == "examples"
            }) {
                render_definition_diagnostics(
                    &diagnostics,
                    args.get(1).map(String::as_str).unwrap_or("."),
                );
            } else {
                print_diagnostics(&diagnostics, |span| fs::read_to_string(&span.file).ok());
            }
            ExitCode::FAILURE
        }
    }
}

fn print_definition_diagnostics(language: &your_language::CompiledLanguage, entry: &str) {
    render_definition_diagnostics(language.diagnostics(), entry);
}

fn render_definition_diagnostics(diagnostics: &[Diagnostic], entry: &str) {
    let root = Path::new(entry).parent().unwrap_or(Path::new("."));
    print_diagnostics(diagnostics, |span| {
        fs::read_to_string(root.join(&span.file)).ok()
    });
}
