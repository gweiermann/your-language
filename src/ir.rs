//! Serializable, surface-sugar-free interpreter input. IDs are deterministic strings.
use crate::{
    diagnostic::Span,
    syntax::{Associativity, Quantifier},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompiledLanguage {
    pub(crate) version: u32,
    pub(crate) entry: String,
    pub(crate) rules: BTreeMap<String, Rule>,
    pub(crate) trivia: Vec<String>,
    #[serde(skip)]
    pub(crate) diagnostics: Vec<crate::diagnostic::Diagnostic>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub trivia: bool,
    pub body: RuleBody,
    pub constraints: Vec<Check>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RuleBody {
    Concrete(Term),
    Abstract {
        bases: Vec<String>,
        operators: Vec<Operator>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Operator {
    pub rule: String,
    pub binding_power: usize,
    pub associativity: Associativity,
    pub left: bool,
    pub right: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Term {
    Literal(String),
    Regex(String),
    Ref(String),
    Sequence(Vec<Term>),
    Choice(Vec<Term>),
    Repeat(Box<Term>, Quantifier),
    Capture(String, Box<Term>),
    NotAhead(Box<Term>),
    NotBehind(Box<Term>),
    /// Marks values of the original grammar, independent of wrapper values.
    Mark {
        id: u32,
        term: Box<Term>,
    },
    /// Normalized value projection; runtime does not know any YL pipe names.
    Project {
        id: u32,
        term: Box<Term>,
        quantifier: Option<Quantifier>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        collect_lists: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub condition: Condition,
    pub emissions: Vec<Emission>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    Bool(bool),
    Text(String),
    Absent,
    CaptureValue(String),
    EnumValue {
        ty: String,
        variant: String,
    },
    Present(String),
    CaptureType {
        capture: String,
        rule: String,
    },
    OptionalMatches {
        capture: String,
        regex: String,
    },
    Not(Box<Condition>),
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Equal(Box<Condition>, Box<Condition>),
    Between {
        trivia: Option<String>,
        left: String,
        right: String,
    },
    Matches {
        capture: String,
        regex: String,
    },
    Always,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Emission {
    pub severity: crate::diagnostic::Severity,
    pub message: String,
    pub span: Span,
}
