use crate::diagnostic::Span;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExprKind {
    Match {
        inputs: Vec<Expr>,
        cases: Vec<ValueMatchCase>,
    },
    Lambda {
        parameter: String,
        body: Box<Expr>,
    },
    Local(String),
    /// Compiler-only caller capture; never re-resolved inside an authored operation.
    SemanticCapture(String),
    /// Compiler-only dependency-preserving result of an authored action operation.
    SemanticBinding {
        invocation: String,
        result: Box<Expr>,
    },
    SemanticBlock {
        invocation: String,
        actions: Vec<crate::semantics::queries::QueryAction>,
        result: Box<Expr>,
    },
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
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    /// Zero-based positional access in semantic expressions.
    Index(Box<Expr>, usize),
    Sequence(Vec<Expr>),
    Choice(Vec<Expr>),
    Repeat(Box<Expr>, Quantifier),
    Capture(String, Box<Expr>),
    Pipe(Box<Expr>, String, Vec<Argument>),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueMatchCase {
    pub alternatives: Vec<Vec<Option<Expr>>>,
    pub value: Expr,
    pub span: Span,
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
    Symbol {
        name: String,
    },
    Import {
        names: Vec<(String, String)>,
        path: String,
    },
    NamespaceImport {
        name: String,
        path: String,
    },
    Operation {
        name: String,
        parameters: Vec<Parameter>,
        body: OperationBody,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        example: Option<Example>,
    },
    Pattern {
        name: String,
        parameters: Vec<Parameter>,
        grammar: Expr,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meanings: Option<Meanings>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        example: Option<Example>,
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
    #[serde(default)]
    pub boundary: bool,
    pub groups: Vec<MeaningSyntaxGroup>,
    pub precedence: Vec<MeaningSyntaxChain>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningSyntaxGroup {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<MeaningSyntaxAction>,
    pub name: Option<String>,
    pub calls: Vec<MeaningSyntaxCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matching: Option<MeaningSyntaxMatch>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningSyntaxMatch {
    pub inputs: Vec<Expr>,
    pub cases: Vec<MeaningSyntaxCase>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningSyntaxCase {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nested_actions: Vec<MeaningSyntaxAction>,
    pub alternatives: Vec<Vec<Option<Expr>>>,
    pub actions: Vec<MeaningSyntaxCall>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Example {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OperationBody {
    Conditional(Vec<MeaningSyntaxAction>),
    Actions(Vec<MeaningSyntaxCall>),
    Query(Expr),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MeaningSyntaxAction {
    Binding {
        name: String,
        value: Expr,
        span: Span,
    },
    Return {
        value: Expr,
        span: Span,
    },
    Call(MeaningSyntaxCall),
    If {
        condition: Expr,
        actions: Vec<MeaningSyntaxAction>,
        otherwise: Vec<MeaningSyntaxAction>,
        span: Span,
    },
    Match(MeaningSyntaxMatch),
}
