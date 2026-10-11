//! Human-readable source diagnostics shared by command-line consumers.
use crate::diagnostic::{Diagnostic, Span};
use std::io::IsTerminal;

/// Select terminal color. NO_COLOR takes precedence over forced color.
pub fn stderr_color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some_and(|value| !value.is_empty() && value != "0") {
        return true;
    }
    std::io::stderr().is_terminal()
}

/// Render to stderr using automatic terminal color selection.
pub fn print_diagnostics(diagnostics: &[Diagnostic], source_for: impl Fn(&Span) -> Option<String>) {
    eprint!(
        "{}",
        render_diagnostics(diagnostics, source_for, stderr_color_enabled())
    );
}

struct DiagnosticLine {
    file: String,
    line: usize,
    text: String,
    ranges: Vec<(usize, usize)>,
}
/// Format diagnostics with UTF-8 byte spans converted to source columns.
pub fn render_diagnostics(
    diagnostics: &[Diagnostic],
    source_for: impl Fn(&Span) -> Option<String>,
    color: bool,
) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    use crate::diagnostic::Severity;
    for diagnostic in diagnostics {
        writeln!(
            output,
            "{:?}[{}]: {}",
            diagnostic.severity, diagnostic.code, diagnostic.message
        )
        .expect("writing to String cannot fail");
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
            writeln!(output, "  --> {}", diagnostic.primary.file)
                .expect("writing to String cannot fail");
        }
        for DiagnosticLine {
            file,
            line,
            text,
            ranges: spans,
        } in lines
        {
            writeln!(output, "  --> {file}:{line}").expect("writing to String cannot fail");
            writeln!(output, "  {line} | {text}").expect("writing to String cannot fail");
            let mut markers = vec![' '; text.chars().count() + 1];
            for (start, length) in spans {
                for column in start..start.saturating_add(length).min(markers.len()) {
                    markers[column] = '^';
                }
            }
            let underline: String = markers.into_iter().collect();
            let gutter = " ".repeat(2 + line.to_string().len());
            if color {
                let color = if diagnostic.severity == Severity::Error {
                    31
                } else {
                    33
                };
                writeln!(
                    output,
                    "{gutter} | \x1b[{color}m{}\x1b[0m",
                    underline.trim_end()
                )
                .expect("writing to String cannot fail");
            } else {
                writeln!(output, "{gutter} | {}", underline.trim_end())
                    .expect("writing to String cannot fail");
            }
        }
        if let Some(help) = &diagnostic.help {
            writeln!(output, "  help: {help}").expect("writing to String cannot fail");
        }
    }
    output
}
