use crate::diagnostic::Span;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExprKind {
    EnumConstant(String, String),
    Bool(bool),
    Absent,
    Not(Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Literal(String),
    Regex(String),
    Ref(String),
    Variant(String),
    Call(String, Vec<Argument>),
    Group(Box<Expr>),
    Sequence(Vec<Expr>),
    Choice(Vec<Expr>),
    Repeat(Box<Expr>, Quantifier),
    Capture(String, Box<Expr>),
    Pipe(Box<Expr>, String, Vec<Argument>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quantifier {
    Optional,
    Star,
    Plus,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Argument {
    pub name: Option<String>,
    pub value: Expr,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub ty: Option<String>,
    pub default: Option<Expr>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub declarations: Vec<Declaration>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Declaration {
    pub export: bool,
    pub kind: DeclKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DeclKind {
    Import {
        names: Vec<(String, String)>,
        path: String,
    },
    Node {
        name: String,
        trivia: bool,
        grammar: Option<Expr>,
        members: Vec<Declaration>,
        precedence: Vec<Level>,
        constraints: Vec<Constraint>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meanings: Option<Meanings>,
    },
    Pattern {
        name: String,
        parameters: Vec<Parameter>,
        grammar: Expr,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meanings: Option<Meanings>,
    },
    Pipe {
        name: String,
        parameters: Vec<Parameter>,
        arms: Vec<Rewrite>,
    },
    Enum {
        name: String,
        variants: Vec<String>,
    },
    Constraint {
        name: String,
        parameters: Vec<Parameter>,
        body: Vec<Constraint>,
    },
    Extend {
        name: String,
        constraints: Vec<Constraint>,
    },
    Entry {
        node: Expr,
        trivia: Vec<Expr>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meanings {
    pub groups: Vec<MeaningSyntaxGroup>,
    pub precedence: Vec<MeaningSyntaxChain>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningSyntaxGroup {
    pub name: Option<String>,
    pub calls: Vec<MeaningSyntaxCall>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningSyntaxCall {
    pub name: String,
    pub arguments: Vec<Argument>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningSyntaxChain {
    pub selectors: Vec<Expr>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub members: Vec<String>,
    pub associativity: Associativity,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Associativity {
    Left,
    Right,
    Nonassoc,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rewrite {
    pub binding: String,
    pub quantifier: Option<Quantifier>,
    pub ty: Option<String>,
    pub body: RewriteBody,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RewriteBody {
    Direct(Expr),
    Cases(Vec<(Expr, Expr)>),
    Match {
        selectors: Vec<Expr>,
        cases: Vec<RewriteCase>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RewriteCase {
    pub patterns: Vec<Option<Expr>>,
    pub replacement: Option<Expr>,
    pub diagnostics: Vec<Constraint>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub kind: ConstraintKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConstraintKind {
    Call(String, Vec<Argument>),
    When(Expr, Vec<Constraint>),
}
