//! Batch policy and CLI reporting; the formatter itself stays a pure library API.
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use your_language::{
    diagnostic::{Diagnostic, Severity, Span},
    diagnostic_render::print_diagnostics,
    FormatOptions,
};

#[derive(Serialize)]
struct Report {
    check: bool,
    success: bool,
    files: Vec<FileReport>,
    diagnostics: Vec<Diagnostic>,
}
#[derive(Serialize)]
struct FileReport {
    path: String,
    status: Status,
    would_change: Option<bool>,
    changed: bool,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Clean,
    WouldChange,
    Changed,
    Error,
    Skipped,
}
struct Options {
    check: bool,
    json: bool,
    files: Vec<String>,
    format: FormatOptions,
}
impl Options {
    fn parse(args: &[String]) -> Result<Self, Diagnostic> {
        let mut result = Self {
            check: false,
            json: false,
            files: vec![],
            format: FormatOptions::default(),
        };
        let mut options = true;
        for argument in args {
            match argument.as_str() {
                "--" if options => options = false,
                "--check" if options => result.check = true,
                "--json" if options => result.json = true,
                "--align-match-arrows" if options => result.format.align_match_arrows = true,
                argument if options && argument.starts_with('-') => return Err(usage()),
                _ => result.files.push(argument.clone()),
            }
        }
        if result.files.is_empty() {
            return Err(usage());
        }
        Ok(result)
    }
}
fn usage() -> Diagnostic {
    Diagnostic::error(
        "yl.cli",
        "Usage: yl fmt [--check] [--json] [--align-match-arrows] [--] <files...>",
        Span::default(),
    )
}
fn flag(args: &[String], name: &str) -> bool {
    args.iter()
        .take_while(|argument| argument.as_str() != "--")
        .any(|argument| argument == name)
}
fn io(path: &str, error: std::io::Error) -> Diagnostic {
    Diagnostic::error("yl.io", error.to_string(), Span::new(path, 0, 0))
}
fn required(path: &str, source: &str, formatted: &str, options: FormatOptions) -> Diagnostic {
    let start = source
        .chars()
        .zip(formatted.chars())
        .take_while(|(left, right)| left == right)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let end = start + source[start..].chars().next().map_or(0, char::len_utf8);
    let mut diagnostic = Diagnostic::error(
        "yl.format_required",
        "This file needs formatting",
        Span::new(path, start, end),
    );
    diagnostic.severity = Severity::Warning;
    diagnostic.help = Some(if options.align_match_arrows {
        "Run yl fmt --align-match-arrows without --check to apply the requested layout".into()
    } else {
        "Run yl fmt without --check to apply the canonical layout".into()
    });
    diagnostic
}
struct Pending {
    index: usize,
    path: PathBuf,
    source: String,
    formatted: String,
}

pub(super) fn run(args: &[String]) -> Result<bool, Vec<Diagnostic>> {
    let parsed = Options::parse(args);
    let json = parsed
        .as_ref()
        .map_or_else(|_| flag(args, "--json"), |options| options.json);
    let check = parsed
        .as_ref()
        .map_or_else(|_| flag(args, "--check"), |options| options.check);
    let mut report = Report {
        check,
        success: true,
        files: vec![],
        diagnostics: vec![],
    };
    let mut sources = BTreeMap::new();
    match parsed {
        Ok(options) => process(&options, &mut report, &mut sources),
        Err(diagnostic) => {
            report.success = false;
            report.diagnostics.push(diagnostic);
        }
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| vec![Diagnostic::error(
                "yl.json",
                error.to_string(),
                Span::default()
            )])?
        );
    } else {
        for file in &report.files {
            match file.status {
                Status::WouldChange if check => println!("Would format {}", file.path),
                Status::Changed => println!("Formatted {}", file.path),
                _ => {}
            }
        }
        print_diagnostics(&report.diagnostics, |span| sources.get(&span.file).cloned());
    }
    Ok(report.success)
}
fn process(options: &Options, report: &mut Report, sources: &mut BTreeMap<String, String>) {
    let mut seen = BTreeSet::new();
    let mut pending = vec![];
    // Preflight every file before writing any file, so diagnostics cover the batch.
    for label in &options.files {
        let canonical = fs::canonicalize(label);
        if let Ok(path) = &canonical {
            if !seen.insert(path.clone()) {
                continue;
            }
        }
        let index = report.files.len();
        report.files.push(FileReport {
            path: label.clone(),
            status: Status::Error,
            would_change: None,
            changed: false,
        });
        let path = match canonical {
            Ok(path) => path,
            Err(error) => {
                report.diagnostics.push(io(label, error));
                report.success = false;
                continue;
            }
        };
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                report.diagnostics.push(io(label, error));
                report.success = false;
                continue;
            }
        };
        sources.insert(label.clone(), source.clone());
        let formatted = match your_language::format_yl_with_options(label, &source, options.format)
        {
            Ok(formatted) => formatted,
            Err(errors) => {
                report.diagnostics.extend(errors);
                report.success = false;
                continue;
            }
        };
        let changed = formatted != source;
        report.files[index].would_change = Some(changed);
        if !changed {
            report.files[index].status = Status::Clean;
            continue;
        }
        if !options.check {
            let writable = fs::metadata(&path).and_then(|metadata| {
                if metadata.permissions().readonly() {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Cannot format a read-only file",
                    ))
                } else {
                    Ok(())
                }
            });
            if let Err(error) = writable {
                report.diagnostics.push(io(label, error));
                report.success = false;
                continue;
            }
        } else {
            report
                .diagnostics
                .push(required(label, &source, &formatted, options.format));
        }
        report.files[index].status = Status::WouldChange;
        pending.push(Pending {
            index,
            path,
            source,
            formatted,
        });
    }
    if options.check {
        report.success &= pending.is_empty();
        return;
    }
    for file in pending {
        if !report.success {
            report.files[file.index].status = Status::Skipped;
            continue;
        }
        match replace_formatted_file(&file.path, &file.source, &file.formatted) {
            Ok(()) => {
                report.files[file.index].status = Status::Changed;
                report.files[file.index].changed = true;
            }
            Err(error) => {
                report.files[file.index].status = Status::Error;
                report
                    .diagnostics
                    .push(io(&report.files[file.index].path, error));
                report.success = false;
            }
        }
    }
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
