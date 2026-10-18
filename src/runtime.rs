//! Backtracking grammar interpreter with Pratt execution for recursive abstract families.
//! Every attempt owns its captures; failed alternatives cannot leak AST fields/diagnostics.
use crate::{
    diagnostic::{Diagnostic, Severity, Span},
    image::parser::{
        CheckExpression, ImageBody, ImageOperator, ImageRule, Instruction, ParserImage,
    },
    ir::{CompiledLanguage, HighlightSpan},
    syntax::{Associativity, Quantifier},
};
use serde::{Deserialize, Serialize};
mod checks;
mod highlighting;
mod instructions;
mod matching;
mod occurrences;
mod precedence;
mod session;
pub(crate) use session::accepts_preview;
use session::parse_session;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AstNode {
    #[serde(rename = "type")]
    pub kind: String,
    pub fields: BTreeMap<String, AstValue>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AstValue {
    Node(Box<AstNode>),
    Text(String),
    List(Vec<AstValue>),
    None,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParseResult {
    pub ast: Option<AstNode>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct TerminalAddress {
    file: String,
    start: usize,
    end: usize,
    capture: String,
    position: usize,
}
/// A committed capture with per-element source origins and occurrence ownership.
#[derive(Clone)]
struct Captured {
    element_occurrences: Vec<Vec<usize>>,
    element_spans: Vec<Span>,
    repeated: bool,
    occurrences: Vec<usize>,
    value: AstValue,
    span: Span,
    origin: Span,
    depth: usize,
}
/// Branch-local parse result. Backtracking discards the entire value on failure,
/// including fields, diagnostics, projection markers, and semantic occurrences.
#[derive(Clone)]
struct Match {
    element_occurrences: Vec<Vec<usize>>,
    element_spans: Vec<Span>,
    end: usize,
    value: Option<AstValue>,
    origin: Option<Span>,
    fields: BTreeMap<String, Captured>,
    diagnostics: Vec<Diagnostic>,
    marked: BTreeMap<u32, Vec<MarkedValue>>,
    value_depth: usize,
    occurrences: Vec<crate::semantics::Occurrence>,
    highlights: Vec<HighlightSpan>,
}
#[derive(Clone)]
struct MarkedValue {
    element_occurrences: Vec<Vec<usize>>,
    element_spans: Vec<Span>,
    value: Option<AstValue>,
    origin: Option<Span>,
    fields: BTreeMap<String, Captured>,
    depth: usize,
    occurrences: Vec<crate::semantics::Occurrence>,
}
struct OperatorContext<'a> {
    left: Option<(u32, &'a Match)>,
    right: Option<u32>,
    family: u32,
    min_power: usize,
}
impl Match {
    fn empty(end: usize) -> Self {
        Self {
            element_occurrences: vec![],
            element_spans: vec![],
            end,
            value: None,
            origin: None,
            fields: BTreeMap::new(),
            diagnostics: vec![],
            marked: BTreeMap::new(),
            value_depth: 0,
            occurrences: vec![],
            highlights: vec![],
        }
    }
}
pub fn parse(language: &CompiledLanguage, source: &str) -> ParseResult {
    parse_named(language, "<source>", source)
}
/// Parse `source` using the loaded language; `file` supplies all returned source spans.
/// Each invocation owns its failure bookkeeping and capture/occurrence buffers.
pub fn parse_named(language: &CompiledLanguage, file: &str, source: &str) -> ParseResult {
    parse_occurrences(language, file, source).0
}
pub(crate) fn parse_occurrences(
    language: &CompiledLanguage,
    file: &str,
    source: &str,
) -> (ParseResult, Vec<crate::semantics::Occurrence>) {
    let (result, occurrences, _) = parse_session(language, file, source, false);
    (result, occurrences)
}
impl CompiledLanguage {
    /// Highlight successful target-language tokens. Failed alternatives, trivia and
    /// lookaround never leak tokens. Wrapper metadata overrides its inner grammar.
    pub fn highlight_named(&self, file: &str, source: &str) -> Vec<HighlightSpan> {
        parse_session(self, file, source, true).2
    }
}
/// Mutable state for one source parse; executable tables remain borrowed and immutable.
struct Runtime<'a> {
    image: &'a ParserImage,
    trace: bool,
    highlighting: bool,
    partial_highlights: Vec<HighlightSpan>,
    next_occurrence: usize,
    source: &'a str,
    file: &'a str,
    regexes: BTreeMap<String, regex::Regex>,
    active: BTreeSet<(u32, usize, bool, usize, usize)>,
    farthest: usize,
    expected: BTreeSet<String>,
    depth: usize,
    resource_limit: bool,
    steps: usize,
    /// Consumption bound for lookbehind; assertions still inspect the original source.
    limit: usize,
}
#[derive(PartialEq)]
enum ConditionValue {
    PresentCapture,
    Bool(bool),
    Text(String),
    Enum(String, String),
    Absent,
}
fn merge_marked(
    target: &mut BTreeMap<u32, Vec<MarkedValue>>,
    source: BTreeMap<u32, Vec<MarkedValue>>,
) {
    for (id, values) in source {
        target.entry(id).or_default().extend(values);
    }
}
fn collapse(mut values: Vec<AstValue>) -> Option<AstValue> {
    match values.len() {
        0 => None,
        1 => values.pop(),
        _ => Some(AstValue::List(values)),
    }
}
impl ParseResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

#[cfg(test)]
mod tests;
