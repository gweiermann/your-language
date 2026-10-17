use serde::{Deserialize, Serialize};

/// All offsets are UTF-8 bytes; ranges are half open.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub file: String,
    pub start: usize,
    pub end: usize,
}
impl Span {
    pub fn new(file: &str, start: usize, end: usize) -> Self {
        Self {
            file: file.into(),
            start,
            end,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Help,
}
/// A source highlight belonging to one diagnostic, optionally carrying its own label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticAnnotation {
    pub span: Span,
    pub label: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub primary: Box<Span>,
    pub secondary: Vec<Span>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    // Keep the frequently returned Diagnostic below Rust's large-error threshold.
    #[allow(clippy::box_collection)]
    pub annotations: Box<Vec<DiagnosticAnnotation>>,
    pub help: Option<String>,
}
impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>, primary: Span) -> Self {
        Self {
            severity: Severity::Error,
            code: code.into(),
            message: message.into(),
            primary: Box::new(primary),
            secondary: vec![],
            annotations: Box::default(),
            help: None,
        }
    }
}
pub type CompileResult<T> = Result<T, Vec<Diagnostic>>;
