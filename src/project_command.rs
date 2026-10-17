use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use your_language::{
    compile_language,
    diagnostic::{Diagnostic, Severity, Span},
    diagnostic_render::print_diagnostics,
    parse_named,
    project::Project,
    CompiledLanguage, ParseResult,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceInfo {
    path: PathBuf,
    expect_errors: bool,
}
#[derive(Serialize)]
struct ProjectInfo {
    config: PathBuf,
    entry: PathBuf,
    sources: Vec<SourceInfo>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceReport {
    path: PathBuf,
    expect_errors: bool,
    success: bool,
    diagnostics: Vec<Diagnostic>,
}
#[derive(Serialize)]
struct Report {
    config: PathBuf,
    success: bool,
    sources: Vec<SourceReport>,
}

fn error(path: &Path, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        "yl.project",
        message,
        Span::new(&path.to_string_lossy(), 0, 0),
    )]
}
fn analyze(
    language: &CompiledLanguage,
    project: &Project,
    path: &Path,
    check: bool,
) -> Result<ParseResult, Vec<Diagnostic>> {
    let source = fs::read_to_string(path).map_err(|e| error(path, e.to_string()))?;
    let name = path.to_string_lossy();
    let mut result = if check && language.has_declarative_meanings() {
        let analysis =
            your_language::semantics::SemanticEngine::default().analyze(language, &name, &source);
        ParseResult {
            ast: analysis.ast,
            diagnostics: analysis.diagnostics,
        }
    } else {
        parse_named(language, &name, &source)
    };
    let mut diagnostics = language.diagnostics().to_vec();
    diagnostics.append(&mut result.diagnostics);
    super::project_diagnostic_paths(
        &mut diagnostics,
        project.entry.parent().unwrap_or(Path::new(".")),
        Some(&name),
    );
    result.diagnostics = diagnostics;
    Ok(result)
}
fn compile(project: &Project) -> Result<CompiledLanguage, Vec<Diagnostic>> {
    compile_language(&project.entry).map_err(|mut diagnostics| {
        super::project_diagnostic_paths(
            &mut diagnostics,
            project.entry.parent().unwrap_or(Path::new(".")),
            None,
        );
        diagnostics
    })
}
fn render(diagnostics: &[Diagnostic]) {
    print_diagnostics(diagnostics, |span| fs::read_to_string(&span.file).ok());
}
pub fn run(args: &[String]) -> Result<bool, Vec<Diagnostic>> {
    let json = args.get(2).is_some_and(|s| s == "--json");
    if args.len() < 2 || args.len() > 3 || (args.len() == 3 && !json) {
        return Err(error(Path::new(""), "Usage: yl project list <directory> [--json] | yl project check <source|yl.json|directory> [--json] | yl project ast <source>"));
    }
    let path = Path::new(&args[1]);
    if args[0] == "list" {
        let projects = Project::list(path)?;
        let mut infos = Vec::new();
        for project in projects {
            let sources = project
                .sources()?
                .into_iter()
                .map(|path| SourceInfo {
                    expect_errors: project.expects_errors(&path),
                    path,
                })
                .collect();
            infos.push(ProjectInfo {
                config: project.config_path,
                entry: project.entry,
                sources,
            });
        }
        if json {
            println!("{}", super::json(&infos)?);
        } else {
            for info in infos {
                println!("{}", info.config.display());
                for source in info.sources {
                    println!(
                        "  {}{}",
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
        return Ok(true);
    }
    if !matches!(args[0].as_str(), "check" | "ast") || args[0] == "ast" && json {
        return Err(error(
            path,
            "Expected project check or project ast (AST output is already JSON)",
        ));
    }
    let batch = path.is_dir() || path.file_name().is_some_and(|s| s == "yl.json");
    if batch && args[0] == "ast" {
        return Err(error(path, "Specify one source file for project ast"));
    }
    let project = if batch {
        Project::load(path)?
    } else {
        Project::discover(path)?
            .filter(|p| p.matches(path))
            .ok_or_else(|| error(path, "Source does not match a nearest yl.json project"))?
    };
    let language = compile(&project)?;
    if !batch {
        let result = analyze(&language, &project, path, args[0] == "check")?;
        if args[0] == "ast" {
            println!("{}", super::json(&result.ast)?);
            if !result.diagnostics.is_empty() {
                eprintln!("{}", super::json(&result.diagnostics)?);
            }
        } else if json {
            println!("{}", super::json(&result.diagnostics)?);
        } else {
            render(&result.diagnostics);
        }
        return Ok(!result.has_errors());
    }
    let sources = project.sources()?;
    if sources.is_empty() {
        return Err(error(
            &project.config_path,
            "No source files match this project",
        ));
    }
    let mut reports = Vec::new();
    for path in sources {
        let result = analyze(&language, &project, &path, true);
        let expect_errors = project.expects_errors(&path);
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
        if !json {
            render(&diagnostics);
            println!(
                "{} {}{}",
                if success { "PASS" } else { "FAIL" },
                path.display(),
                if expect_errors {
                    " (expected errors)"
                } else {
                    ""
                }
            );
        }
        reports.push(SourceReport {
            path,
            expect_errors,
            success,
            diagnostics,
        });
    }
    let success = reports.iter().all(|r| r.success);
    let report = Report {
        config: project.config_path,
        success,
        sources: reports,
    };
    if json {
        println!("{}", super::json(&report)?);
    }
    Ok(success)
}
