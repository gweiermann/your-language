//! Loading, hook contracts, and command-line diagnostics shared by the examples.
use std::path::Path;
use your_language::{
    backend::{HookBinding, HookContract},
    diagnostic::{Diagnostic, Span},
    CompiledLanguage,
};

pub fn load() -> Result<CompiledLanguage, String> {
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/backend-hooks/yl.json");
    let config_source = std::fs::read_to_string(&config_path).map_err(|error| error.to_string())?;
    let project = your_language::project::Project::parse(&config_path, &config_source)
        .map_err(|errors| format!("{errors:?}"))?;
    let language =
        your_language::compile_language(project.entry).map_err(|errors| format!("{errors:?}"))?;
    your_language::load_compiled_language(&language.to_bytes().map_err(|e| format!("{e:?}"))?)
        .map_err(|e| format!("{e:?}"))
}
pub(super) fn signatures() -> Vec<HookBinding> {
    [
        ("number", &["value"][..]),
        ("reference", &["name"][..]),
        ("add", &["left", "right"][..]),
        ("multiply", &["left", "right"][..]),
        ("less", &["left", "right"][..]),
        ("declare", &["name", "initializer"][..]),
        ("assign", &["name", "value"][..]),
        ("block", &["statements"][..]),
        ("conditional", &["condition", "yes", "no"][..]),
        ("repeat", &["condition", "body"][..]),
        ("program", &["statements", "result"][..]),
    ]
    .into_iter()
    .map(|(name, args)| HookBinding::new(format!("hooks.yl#{name}"), args))
    .collect()
}
pub(super) fn backend_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("example.backend", message, Span::default())
}
pub(super) fn operation(hook: &HookContract) -> &str {
    hook.id.rsplit('#').next().unwrap_or("")
}

pub fn source_argument() -> Result<(String, String), String> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/backend-hooks/programs/control-flow.txt".into());
    let source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok((path, source))
}

pub fn render_errors(errors: &[Diagnostic], path: &str, source: &str) -> String {
    your_language::diagnostic_render::render_diagnostics(
        errors,
        |span| (span.file == path).then(|| source.to_owned()),
        your_language::diagnostic_render::stderr_color_enabled(),
    )
}
