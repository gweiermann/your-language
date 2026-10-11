use std::{env, fs, path::Path, process::ExitCode};
use your_language::diagnostic_render::print_diagnostics;
use your_language::{
    compile_language,
    diagnostic::{Diagnostic, Span},
    load_compiled_language, parse_named,
};

fn run(args: &[String]) -> Result<bool, Vec<Diagnostic>> {
    let usage = || {
        vec![Diagnostic::error("yl.cli","Usage: yl fmt [--check] <files...> | yl check <entry.yl> | yl compile <entry.yl> -o <language.ylc> | yl language <language.ylc> ast <source> | yl language <language.ylc> check <source> [--json]",Span::default())]
    };
    let io = |path: &str, e: std::io::Error| {
        vec![Diagnostic::error(
            "yl.io",
            e.to_string(),
            Span::new(path, 0, 0),
        )]
    };
    match args.first().map(String::as_str) {
        Some("examples") if args.len() == 2 => {
            let language = if Path::new(&args[1]).extension().is_some_and(|e| e == "ylc") {
                load_compiled_language(&fs::read(&args[1]).map_err(|e| io(&args[1], e))?)?
            } else {
                compile_language(&args[1])?
            };
            println!("{}", json(&language.example_previews())?);
            Ok(true)
        }
        Some("fmt") => format_files(&args[1..]),
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

fn format_files(args: &[String]) -> Result<bool, Vec<Diagnostic>> {
    use std::collections::BTreeSet;
    let io = |path: &str, error: std::io::Error| {
        vec![Diagnostic::error(
            "yl.io",
            error.to_string(),
            Span::new(path, 0, 0),
        )]
    };
    let mut check = false;
    let mut options = true;
    let mut files = vec![];
    for arg in args {
        if options && arg == "--" {
            options = false;
        } else if options && arg == "--check" {
            check = true;
        } else if options && arg.starts_with('-') {
            return Err(vec![Diagnostic::error(
                "yl.cli",
                "Usage: yl fmt [--check] <files...>",
                Span::default(),
            )]);
        } else {
            files.push(arg);
        }
    }
    if files.is_empty() {
        return Err(vec![Diagnostic::error(
            "yl.cli",
            "Usage: yl fmt [--check] <files...>",
            Span::default(),
        )]);
    }
    let mut seen = BTreeSet::new();
    let mut pending = vec![];
    // Validate the whole batch before modifying any source files.
    for label in files {
        let path = fs::canonicalize(label).map_err(|error| io(label, error))?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|error| io(label, error))?;
        let formatted = your_language::format_yl(label, &source)?;
        if formatted != source {
            if !check
                && fs::metadata(&path)
                    .map_err(|error| io(label, error))?
                    .permissions()
                    .readonly()
            {
                return Err(io(
                    label,
                    std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Cannot format a read-only file",
                    ),
                ));
            }
            pending.push((label, path, source, formatted));
        }
    }
    let clean = pending.is_empty();
    for (label, path, source, formatted) in pending {
        if check {
            println!("Would format {label}");
        } else {
            replace_formatted_file(&path, &source, &formatted).map_err(|error| io(label, error))?;
            println!("Formatted {label}");
        }
    }
    Ok(!check || clean)
}

fn replace_formatted_file(path: &Path, original: &str, formatted: &str) -> std::io::Result<()> {
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let metadata = fs::metadata(path)?;
    if metadata.permissions().readonly() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Cannot format a read-only file",
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Missing parent directory")
    })?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let (temp, mut file) = loop {
        let temp = parent.join(format!(
            ".{name}.yl-fmt-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => break (temp, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        file.write_all(formatted.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::set_permissions(&temp, metadata.permissions())?;
        if fs::read_to_string(path)? != original {
            return Err(std::io::Error::other(
                "File changed while formatting; run the formatter again",
            ));
        }
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
