use std::{env, fs, io::IsTerminal, path::Path, process::ExitCode};
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
            if args
                .first()
                .is_some_and(|command| command == "check" || command == "compile")
            {
                render_definition_diagnostics(
                    &diagnostics,
                    args.get(1).map(String::as_str).unwrap_or("."),
                );
                return ExitCode::FAILURE;
            }
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

fn print_definition_diagnostics(language: &your_language::CompiledLanguage, entry: &str) {
    render_definition_diagnostics(language.diagnostics(), entry);
}

struct DiagnosticLine {
    file: String,
    line: usize,
    text: String,
    ranges: Vec<(usize, usize)>,
}
fn render_definition_diagnostics(diagnostics: &[Diagnostic], entry: &str) {
    use your_language::diagnostic::Severity;
    let root = Path::new(entry).parent().unwrap_or(Path::new("."));
    for diagnostic in diagnostics {
        eprintln!(
            "{:?}[{}]: {}",
            diagnostic.severity, diagnostic.code, diagnostic.message
        );
        let mut lines: Vec<DiagnosticLine> = vec![];
        for span in std::iter::once(diagnostic.primary.as_ref()).chain(&diagnostic.secondary) {
            let path = root.join(&span.file);
            let Ok(source) = fs::read_to_string(path) else {
                continue;
            };
            if span.start > span.end
                || span.end > source.len()
                || !source.is_char_boundary(span.start)
                || !source.is_char_boundary(span.end)
            {
                continue;
            }
            let line_start = source[..span.start].rfind('\n').map_or(0, |p| p + 1);
            let line_end = source[span.start..]
                .find('\n')
                .map_or(source.len(), |p| span.start + p);
            let line = source[..line_start].bytes().filter(|b| *b == b'\n').count() + 1;
            let column = source[line_start..span.start].chars().count();
            let length = source[span.start..span.end.min(line_end)]
                .chars()
                .count()
                .max(1);
            if let Some(record) = lines
                .iter_mut()
                .find(|record| record.file == span.file && record.line == line)
            {
                record.ranges.push((column, length));
            } else {
                lines.push(DiagnosticLine {
                    file: span.file.clone(),
                    line,
                    text: source[line_start..line_end].into(),
                    ranges: vec![(column, length)],
                });
            }
        }
        for DiagnosticLine {
            file,
            line,
            text,
            ranges: spans,
        } in lines
        {
            eprintln!("  --> {file}:{line}");
            eprintln!("  {line} | {text}");
            let mut markers = vec![' '; text.chars().count() + 1];
            for (start, length) in spans {
                for column in start..start.saturating_add(length).min(markers.len()) {
                    markers[column] = '^';
                }
            }
            let underline: String = markers.into_iter().collect();
            if std::io::stderr().is_terminal() {
                let color = if diagnostic.severity == Severity::Error {
                    31
                } else {
                    33
                };
                eprintln!("    | \x1b[{color}m{}\x1b[0m", underline.trim_end());
            } else {
                eprintln!("    | {}", underline.trim_end());
            }
        }
        if let Some(help) = &diagnostic.help {
            eprintln!("  help: {help}");
        }
    }
}
