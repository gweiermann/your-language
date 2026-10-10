use std::{env, fs, io::IsTerminal, path::Path, process::ExitCode};
use your_language::{
    compile_language,
    diagnostic::{Diagnostic, Span},
    load_compiled_language, parse_named,
};

fn run(args: &[String]) -> Result<bool, Vec<Diagnostic>> {
    let usage = || {
        vec![Diagnostic::error("yl.cli","Usage: yl check <entry.yl> | yl compile <entry.yl> -o <language.ylc> | yl language <language.ylc> ast <source> | yl language <language.ylc> check <source> [--json]",Span::default())]
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
            let result = parse_named(&language, &args[3], &source);
            if args[2] == "ast" {
                println!("{}", json(&result.ast)?);
                if !result.diagnostics.is_empty() {
                    eprintln!("{}", json(&result.diagnostics)?);
                }
            } else if args.get(4).is_some_and(|flag| flag == "--json") {
                println!("{}", json(&result.diagnostics)?);
            } else {
                render_diagnostics(&result.diagnostics, |span| {
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
fn json(value: &impl serde::Serialize) -> Result<String, Vec<Diagnostic>> {
    serde_json::to_string_pretty(value)
        .map_err(|e| vec![Diagnostic::error("yl.json", e.to_string(), Span::default())])
}
fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let diagnostic_json = args.first().is_some_and(|a| a == "language")
        && args.get(2).is_some_and(|a| a == "check")
        && args.get(4).is_some_and(|a| a == "--json");
    let ast_json =
        args.first().is_some_and(|a| a == "language") && args.get(2).is_some_and(|a| a == "ast");
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
            } else if args
                .first()
                .is_some_and(|command| command == "check" || command == "compile")
            {
                render_definition_diagnostics(
                    &diagnostics,
                    args.get(1).map(String::as_str).unwrap_or("."),
                );
            } else {
                render_diagnostics(&diagnostics, |span| fs::read_to_string(&span.file).ok());
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
    let root = Path::new(entry).parent().unwrap_or(Path::new("."));
    render_diagnostics(diagnostics, |span| {
        fs::read_to_string(root.join(&span.file)).ok()
    });
}

fn render_diagnostics(diagnostics: &[Diagnostic], source_for: impl Fn(&Span) -> Option<String>) {
    use your_language::diagnostic::Severity;
    for diagnostic in diagnostics {
        eprintln!(
            "{:?}[{}]: {}",
            diagnostic.severity, diagnostic.code, diagnostic.message
        );
        let mut lines: Vec<DiagnosticLine> = vec![];
        for span in std::iter::once(diagnostic.primary.as_ref()).chain(&diagnostic.secondary) {
            let Some(source) = source_for(span) else {
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
                    text: source[line_start..line_end].trim_end_matches('\r').into(),
                    ranges: vec![(column, length)],
                });
            }
        }
        if lines.is_empty() && !diagnostic.primary.file.is_empty() {
            eprintln!("  --> {}", diagnostic.primary.file);
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
            let gutter = " ".repeat(2 + line.to_string().len());
            if std::io::stderr().is_terminal() {
                let color = if diagnostic.severity == Severity::Error {
                    31
                } else {
                    33
                };
                eprintln!("{gutter} | \x1b[{color}m{}\x1b[0m", underline.trim_end());
            } else {
                eprintln!("{gutter} | {}", underline.trim_end());
            }
        }
        if let Some(help) = &diagnostic.help {
            eprintln!("  help: {help}");
        }
    }
}
