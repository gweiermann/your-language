//! In-process project checks shared by the CLI and the Cargo examples.
use super::Project;
use crate::{
    compile_language,
    diagnostic::{CompileResult, Diagnostic, Severity, Span},
    diagnostic_render::print_diagnostics,
    parse_named, CompiledLanguage, ParseResult,
};
use serde::Serialize;
use std::{fs, path::Path, path::PathBuf, process::ExitCode};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReport {
    pub path: PathBuf,
    pub expect_errors: bool,
    pub success: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize)]
pub struct ProjectReport {
    pub config: PathBuf,
    pub success: bool,
    pub sources: Vec<SourceReport>,
}

fn error(path: &Path, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        "yl.project",
        message,
        Span::new(&path.to_string_lossy(), 0, 0),
    )]
}

fn diagnostic_paths(diagnostics: &mut [Diagnostic], root: &Path, source: Option<&str>) {
    for diagnostic in diagnostics {
        for span in std::iter::once(diagnostic.primary.as_mut())
            .chain(diagnostic.secondary.iter_mut())
            .chain(diagnostic.annotations.iter_mut().map(|a| &mut a.span))
        {
            if source != Some(span.file.as_str()) && Path::new(&span.file).is_relative() {
                span.file = root.join(&span.file).to_string_lossy().into();
            }
        }
    }
}

impl Project {
    /// Compile this project's configured language without executing backend hooks.
    pub fn compile(&self) -> CompileResult<CompiledLanguage> {
        compile_language(&self.entry).map_err(|mut diagnostics| {
            diagnostic_paths(
                &mut diagnostics,
                self.entry.parent().unwrap_or(Path::new(".")),
                None,
            );
            diagnostics
        })
    }

    /// Parse one source using a previously compiled language; optionally run static analysis.
    pub fn analyze_file(
        &self,
        language: &CompiledLanguage,
        path: &Path,
        check: bool,
    ) -> CompileResult<ParseResult> {
        let source = fs::read_to_string(path).map_err(|e| error(path, e.to_string()))?;
        let name = path.to_string_lossy();
        let mut result = if check && language.has_declarative_meanings() {
            let analysis =
                crate::semantics::SemanticEngine::default().analyze(language, &name, &source);
            ParseResult {
                ast: analysis.ast,
                diagnostics: analysis.diagnostics,
            }
        } else {
            parse_named(language, &name, &source)
        };
        let mut diagnostics = language.diagnostics().to_vec();
        diagnostics.append(&mut result.diagnostics);
        diagnostic_paths(
            &mut diagnostics,
            self.entry.parent().unwrap_or(Path::new(".")),
            Some(&name),
        );
        result.diagnostics = diagnostics;
        Ok(result)
    }

    /// Check every owned source, applying `expectErrors` only to the aggregate result.
    pub fn check(&self) -> CompileResult<ProjectReport> {
        let language = self.compile()?;
        let sources = self.sources()?;
        if sources.is_empty() {
            return Err(error(
                &self.config_path,
                "No source files match this project",
            ));
        }
        let mut reports = Vec::new();
        for path in sources {
            let result = self.analyze_file(&language, &path, true);
            let expect_errors = self.expects_errors(&path);
            let (mut diagnostics, read_failure) = match result {
                Ok(r) => (r.diagnostics, false),
                Err(d) => (d, true),
            };
            let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
            let success = !read_failure && has_errors == expect_errors;
            if expect_errors && !has_errors {
                diagnostics.extend(error(
                    &path,
                    "Expected errors, but this source passed analysis",
                ));
            }
            reports.push(SourceReport {
                path,
                expect_errors,
                success,
                diagnostics,
            });
        }
        Ok(ProjectReport {
            config: self.config_path.clone(),
            success: reports.iter().all(|r| r.success),
            sources: reports,
        })
    }
}

impl ProjectReport {
    /// Show the original diagnostics and whether each fixture met its expectation.
    pub fn print(&self) {
        for source in &self.sources {
            render(&source.diagnostics);
            println!(
                "{} {}{}",
                if source.success { "PASS" } else { "FAIL" },
                source.path.display(),
                if source.expect_errors {
                    " (expected errors)"
                } else {
                    ""
                }
            );
        }
    }
}

fn render(diagnostics: &[Diagnostic]) {
    print_diagnostics(diagnostics, |span| fs::read_to_string(&span.file).ok());
}

/// Run a Cargo example directly against its `yl.json`, using the CLI's check pipeline.
pub fn run_example(config: impl AsRef<Path>) -> ExitCode {
    let config = config.as_ref();
    let result = Project::load(config).and_then(|project| project.check());
    match result {
        Ok(report) => {
            println!("Project: {}", report.config.display());
            report.print();
            println!(
                "{} {} program(s), including {} expected-error fixture(s).",
                if report.success { "Checked" } else { "Failed" },
                report.sources.len(),
                report.sources.iter().filter(|s| s.expect_errors).count()
            );
            if report.success {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(diagnostics) => {
            render(&diagnostics);
            ExitCode::FAILURE
        }
    }
}
