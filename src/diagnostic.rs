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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub primary: Span,
    pub secondary: Vec<Span>,
    pub help: Option<String>,
}
impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>, primary: Span) -> Self {
        Self {
            severity: Severity::Error,
            code: code.into(),
            message: message.into(),
            primary,
            secondary: vec![],
            help: None,
        }
    }
}
pub type CompileResult<T> = Result<T, Vec<Diagnostic>>;
