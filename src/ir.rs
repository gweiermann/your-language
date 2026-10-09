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
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub condition: Condition,
    pub emissions: Vec<Emission>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Condition {
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
