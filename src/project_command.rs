use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use your_language::{
    diagnostic::{Diagnostic, Span},
    diagnostic_render::print_diagnostics,
    project::Project,
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
fn error(path: &Path, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        "yl.project",
        message,
        Span::new(&path.to_string_lossy(), 0, 0),
    )]
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
    if !batch {
        let language = project.compile()?;
        let result = project.analyze_file(&language, path, args[0] == "check")?;
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
    let report = project.check()?;
    if json {
        println!("{}", super::json(&report)?);
    } else {
        report.print();
    }
    Ok(report.success)
}
