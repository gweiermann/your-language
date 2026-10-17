//! Token-based recursive descent for declarations, Pratt-style grammar precedence.
use crate::diagnostic::{CompileResult, Diagnostic, Span};
use crate::lexer::{lex, Token, TokenKind};
use crate::syntax::*;
use std::collections::BTreeSet;

pub fn parse_yl(file: &str, source: &str) -> CompileResult<Module> {
    let tokens = lex(file, source).map_err(|e| vec![e])?;
    let diagnostic_paths = imported_diagnostic_paths(&tokens);
    let mut parser = Parser {
        diagnostic_paths,
        tokens,
        position: 0,
        depth: 0,
    };
    let mut declarations = vec![];
    while !matches!(parser.token().kind, TokenKind::End) {
        declarations.push(parser.declaration().map_err(|e| vec![e])?);
    }
    Ok(Module {
        declarations,
        span: Span::new(file, 0, source.len()),
    })
}
/// Parse an embedded semantic expression with the same grammar as operation arguments.
pub(crate) fn parse_semantic_expr(file: &str, source: &str) -> CompileResult<Expr> {
    let tokens = lex(file, source).map_err(|error| vec![error])?;
    let diagnostic_paths = imported_diagnostic_paths(&tokens);
    let mut parser = Parser {
        diagnostic_paths,
        tokens,
        position: 0,
        depth: 0,
    };
    let expr = parser.semantic_value().map_err(|error| vec![error])?;
    if !matches!(parser.token().kind, TokenKind::End) {
        return Err(vec![parser.error(
            "yl.unexpected_token",
            "Unexpected token after semantic expression",
        )]);
    }
    Ok(expr)
}

type Result<T> = std::result::Result<T, Diagnostic>;
struct Parser {
    diagnostic_paths: BTreeSet<String>,
    tokens: Vec<Token>,
    position: usize,
    depth: usize,
}
// Discover diagnostic imports independently of declaration order. Only explicitly
// imported core diagnostics receive the rewrite-case diagnostic prelude syntax.
fn imported_diagnostic_paths(tokens: &[Token]) -> BTreeSet<String> {
    fn word(token: Option<&Token>, value: &str) -> bool {
        matches!(token.map(|token| &token.kind), Some(TokenKind::Word(name)) if name == value)
    }
    let mut paths = BTreeSet::new();
    for index in 0..tokens.len() {
        if !word(tokens.get(index), "import") {
            continue;
        }
        let Some(from) = (index + 1..tokens.len())
            .find(|&next| word(tokens.get(next), "from") || word(tokens.get(next), "import"))
        else {
            continue;
        };
        if !word(tokens.get(from), "from")
            || !matches!(tokens.get(from + 1).map(|token| &token.kind), Some(TokenKind::String(path)) if path == "core/diagnostics")
        {
            continue;
        }
        if matches!(tokens.get(index + 1).map(|token| &token.kind), Some(TokenKind::Symbol(symbol)) if symbol == "*")
        {
            if let Some(Token {
                kind: TokenKind::Word(namespace),
                ..
            }) = tokens.get(index + 3)
            {
                for name in ["error", "warn", "hint"] {
                    paths.insert(format!("{namespace}::{name}"));
                }
            }
        } else {
            let mut item = index + 2;
            while item < from {
                if let TokenKind::Word(name) = &tokens[item].kind {
                    if matches!(name.as_str(), "error" | "warn" | "hint") {
                        let alias = if word(tokens.get(item + 1), "as") {
                            match tokens.get(item + 2).map(|token| &token.kind) {
                                Some(TokenKind::Word(alias)) => alias,
                                _ => name,
                            }
                        } else {
                            name
                        };
                        paths.insert(alias.clone());
                    }
                }
                item += 1;
            }
        }
    }
    paths
}

impl Parser {
    fn token(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }
    fn peek(&self, offset: usize) -> &TokenKind {
        &self.tokens[(self.position + offset).min(self.tokens.len() - 1)].kind
    }
    fn is(&self, s: &str) -> bool {
        matches!(&self.token().kind, TokenKind::Word(w) | TokenKind::Symbol(w) if w==s)
    }
    fn take(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn bump(&mut self) -> Token {
        let t = self.token().clone();
        if self.position + 1 < self.tokens.len() {
            self.position += 1;
        }
        t
    }
    fn error(&self, code: &str, message: &str) -> Diagnostic {
        Diagnostic::error(code, message, self.token().span.clone())
    }
    fn expect(&mut self, s: &str) -> Result<()> {
        if self.take(s) {
            Ok(())
        } else {
            Err(self.error("yl.unexpected_token", &format!("Expected {s}")))
        }
    }
    fn word(&mut self) -> Result<String> {
        match self.bump() {
            Token {
                kind: TokenKind::Word(w),
                ..
            } => Ok(w),
            t => Err(Diagnostic::error(
                "yl.unexpected_token",
                "Expected identifier",
                t.span,
            )),
        }
    }
    fn path(&mut self) -> Result<String> {
        let mut name = self.word()?;
        while self.take("::") {
            name.push_str("::");
            name.push_str(&self.word()?);
        }
        Ok(name)
    }
    fn span_from(&self, start: &Span) -> Span {
        Span::new(
            &start.file,
            start.start,
            self.tokens[self.position.saturating_sub(1)].span.end,
        )
    }
    fn boundary(&self) -> bool {
        if self.is("export") || self.is("import") || self.is("entry") || self.is("extend") {
            return true;
        }
        [
            "node",
            "trivia",
            "pattern",
            "pipe",
            "enum",
            "constraint",
            "symbol",
            "operation",
            "hook",
        ]
        .iter()
        .any(|s| self.is(s))
            && matches!(self.peek(1), TokenKind::Word(_))
    }
    fn declaration(&mut self) -> Result<Declaration> {
        let start = self.token().span.clone();
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error(
                "yl.depth_limit",
                "YL nesting exceeds implementation resource limit",
            ));
        }
        let export = self.take("export");
        let kind = if self.take("import") {
            let namespace = if self.take("*") {
                self.expect("as")?;
                Some(self.word()?)
            } else {
                None
            };
            let mut names = vec![];
            if namespace.is_none() {
                self.expect("{")?;
                while !self.is("}") {
                    let name = self.path()?;
                    let alias = if self.take("as") {
                        self.word()?
                    } else {
                        name.rsplit("::").next().unwrap_or(&name).into()
                    };
                    names.push((name, alias));
                    if !self.take(",") {
                        break;
                    }
                }
                self.expect("}")?;
            }
            self.expect("from")?;
            let t = self.bump();
            let TokenKind::String(path) = t.kind else {
                return Err(Diagnostic::error(
                    "yl.unexpected_token",
                    "Expected import path string",
                    t.span,
                ));
            };
            if let Some(name) = namespace {
                DeclKind::NamespaceImport { name, path }
            } else {
                DeclKind::Import { names, path }
            }
        } else if self.take("hook") {
            let name = self.word()?;
            self.expect("(")?;
            let mut parameters = vec![];
            let mut names = BTreeSet::new();
            while !self.is(")") {
                let span = self.token().span.clone();
                let name = self.word()?;
                if !names.insert(name.clone()) {
                    return Err(self.error("yl.hook_parameter", "Duplicate hook parameter"));
                }
                parameters.push(Parameter {
                    name,
                    ty: None,
                    default: None,
                    span: self.span_from(&span),
                });
                if self.is(":") || self.is("=") {
                    return Err(self.error(
                        "yl.hook_parameter",
                        "Hook parameters are names only; types and defaults are not supported",
                    ));
                }
                if !self.take(",") {
                    break;
                }
            }
            self.expect(")")?;
            DeclKind::Hook { name, parameters }
        } else if self.take("operation") {
            let name = self.word()?;
            let parameters = self.semantic_parameters()?;
            let body = if self.take("=") {
                OperationBody::Query(self.semantic_value()?)
            } else {
                self.expect("{")?;
                let actions = self.meaning_block()?;
                if actions
                    .iter()
                    .all(|action| matches!(action, MeaningSyntaxAction::Call(_)))
                {
                    OperationBody::Actions(
                        actions
                            .into_iter()
                            .map(|action| match action {
                                MeaningSyntaxAction::Call(call) => call,
                                _ => unreachable!(),
                            })
                            .collect(),
                    )
                } else {
                    OperationBody::Conditional(actions)
                }
            };
            DeclKind::Operation {
                name,
                parameters,
                body,
            }
        } else if self.take("symbol") {
            DeclKind::Symbol { name: self.word()? }
        } else if self.take("entry") {
            let node = self.selector()?;
            let mut trivia = vec![];
            if self.take("{") {
                if !self.is("}") {
                    self.expect("trivia")?;
                    trivia.push(self.selector()?);
                    while self.take(",") {
                        trivia.push(self.selector()?);
                    }
                }
                self.expect("}")?;
            }
            DeclKind::Entry { node, trivia }
        } else if self.take("extend") {
            self.expect("node")?;
            let name = self.path()?;
            self.expect("{")?;
            let mut constraints = vec![];
            while !self.is("}") {
                if !self.take("checks") {
                    return Err(self.error("yl.extension_conflict","An extension may only add checks in syntax-v0; metadata is deferred beyond syntax-v0"));
                }
                constraints.extend(self.constraint_block()?);
            }
            self.expect("}")?;
            DeclKind::Extend { name, constraints }
        } else if self.is("node") || self.is("trivia") {
            let trivia = self.take("trivia");
            if !trivia {
                self.expect("node")?;
            }
            let name = self.path()?;
            let mut grammar = None;
            let mut members = vec![];
            let mut precedence = vec![];
            let mut constraints = vec![];
            let mut meanings = None;
            let mut example = None;
            let mut ide = None;
            let mut backend = None;
            if self.take("=") {
                grammar = Some(self.expression()?);
            }
            if self.take("{") {
                while !self.is("}") {
                    if self.is("backend") {
                        if backend.is_some() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate backend section")
                            );
                        }
                        backend = Some(self.backend()?);
                    } else if self.take("ide") {
                        if ide.is_some() {
                            return Err(self.error("yl.duplicate_member", "Duplicate ide section"));
                        }
                        let (metadata, explicit) = self.ide()?;
                        ide = Some(metadata);
                        example = explicit;
                    } else if self.is("example") {
                        return Err(self.error(
                            "yl.ide_migration",
                            "Move example metadata into ide { example \"...\" }",
                        ));
                    } else if self.take("semantics") {
                        if meanings.is_some() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate semantics section")
                            );
                        }
                        meanings = Some(self.meanings()?);
                    } else if self.take("checks") {
                        constraints.extend(self.constraint_block()?);
                    } else if self.take("precedence") {
                        if !precedence.is_empty() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate precedence section")
                            );
                        }
                        self.expect("{")?;
                        loop {
                            let span = self.token().span.clone();
                            let associativity = if self.take("right") {
                                Associativity::Right
                            } else if self.take("nonassoc") {
                                Associativity::Nonassoc
                            } else {
                                Associativity::Left
                            };
                            let mut level = vec![self.path()?];
                            while self.take(",") {
                                level.push(self.path()?);
                            }
                            precedence.push(Level {
                                members: level,
                                associativity,
                                span: self.span_from(&span),
                            });
                            if !self.take(">") {
                                break;
                            }
                        }
                        self.expect("}")?;
                    } else {
                        members.push(self.declaration()?);
                    }
                }
                self.expect("}")?;
            }
            DeclKind::Node {
                name,
                trivia,
                grammar,
                members,
                precedence,
                constraints,
                meanings,
                example,
                ide,
                backend,
            }
        } else if self.take("pattern") {
            let name = self.word()?;
            let parameters = self.parameters()?;
            self.expect("=")?;
            let grammar = self.expression()?;
            let mut meanings = None;
            let mut example = None;
            let mut ide = None;
            let mut backend = None;
            if self.take("{") {
                while !self.is("}") {
                    if self.is("backend") {
                        if backend.is_some() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate backend section")
                            );
                        }
                        backend = Some(self.backend()?);
                    } else if self.take("ide") {
                        if ide.is_some() {
                            return Err(self.error("yl.duplicate_member", "Duplicate ide section"));
                        }
                        let (metadata, explicit) = self.ide()?;
                        ide = Some(metadata);
                        example = explicit;
                    } else if self.is("example") {
                        return Err(self.error(
                            "yl.ide_migration",
                            "Move example metadata into ide { example \"...\" }",
                        ));
                    } else {
                        self.expect("semantics")?;
                        if meanings.is_some() {
                            return Err(
                                self.error("yl.duplicate_member", "Duplicate semantics section")
                            );
                        }
                        meanings = Some(self.meanings()?);
                    }
                }
                self.expect("}")?;
            }
            DeclKind::Pattern {
                name,
                parameters,
                grammar,
                meanings,
                example,
                ide,
                backend,
            }
        } else if self.take("pipe") {
            let name = self.word()?;
            let parameters = self.parameters()?;
            self.expect("{")?;
            let mut arms = vec![];
            while !self.is("}") {
                let span = self.token().span.clone();
                self.expect("rewrite")?;
                let binding = self.word()?;
                let quantifier = if self.take("*") {
                    Some(Quantifier::Star)
                } else if self.take("?") {
                    Some(Quantifier::Optional)
                } else if self.take("+") {
                    Some(Quantifier::Plus)
                } else {
                    None
                };
                let ty = if self.take(":") {
                    Some(self.path()?)
                } else {
                    None
                };
                let selectors = if self.take("match") {
                    let tuple = self.take("(");
                    let mut selectors = vec![self.selector()?];
                    if tuple {
                        while self.take(",") {
                            selectors.push(self.selector()?);
                        }
                        self.expect(")")?;
                    }
                    Some(selectors)
                } else {
                    None
                };
                let body = if let Some(selectors) = selectors {
                    RewriteBody::Match {
                        selectors,
                        cases: self.rewrite_cases()?,
                    }
                } else if self.take("=>") {
                    RewriteBody::Direct(self.expression()?)
                } else {
                    let cases = self.rewrite_cases()?;
                    if cases.iter().all(|c| {
                        c.patterns.len() == 1
                            && c.patterns[0].is_some()
                            && c.replacement.is_some()
                            && c.diagnostics.is_empty()
                    }) {
                        RewriteBody::Cases(
                            cases
                                .into_iter()
                                .filter_map(|c| {
                                    c.patterns.into_iter().next().flatten().zip(c.replacement)
                                })
                                .collect(),
                        )
                    } else {
                        RewriteBody::Match {
                            selectors: vec![],
                            cases,
                        }
                    }
                };
                arms.push(Rewrite {
                    binding,
                    quantifier,
                    ty,
                    body,
                    span: self.span_from(&span),
                });
            }
            self.expect("}")?;
            DeclKind::Pipe {
                name,
                parameters,
                arms,
            }
        } else if self.take("enum") {
            let name = self.word()?;
            self.expect("{")?;
            let mut variants = vec![];
            while !self.is("}") {
                variants.push(self.word()?);
                self.take(",");
            }
            self.expect("}")?;
            DeclKind::Enum { name, variants }
        } else if self.take("constraint") {
            let name = self.word()?;
            let parameters = self.parameters()?;
            let body = self.constraint_block()?;
            DeclKind::Constraint {
                name,
                parameters,
                body,
            }
        } else {
            return Err(self.error(
                "yl.incomplete_declaration",
                "Expected declaration (when belongs inside checks, semantics, or an operation)",
            ));
        };
        self.depth -= 1;
        Ok(Declaration {
            export,
            kind,
            span: self.span_from(&start),
        })
    }
    fn backend(&mut self) -> Result<Backend> {
        let span = self.token().span.clone();
        self.expect("backend")?;
        self.expect("{")?;
        if self.is("return") || self.is("}") {
            return Err(self.error(
                "yl.backend_call",
                "A backend section must contain exactly one hook invocation",
            ));
        }
        let hook = self.path()?;
        let arguments = self.semantic_arguments()?;
        if arguments.iter().any(|argument| argument.name.is_some()) {
            return Err(self.error(
                "yl.backend_call",
                "Backend hook arguments must be positional",
            ));
        }
        if !self.is("}") {
            return Err(self.error(
                "yl.backend_call",
                "A backend section must contain exactly one hook invocation",
            ));
        }
        self.expect("}")?;
        Ok(Backend {
            hook,
            arguments: arguments
                .into_iter()
                .map(|argument| argument.value)
                .collect(),
            span: self.span_from(&span),
        })
    }
    fn selector(&mut self) -> Result<Expr> {
        let start = self.token().span.clone();
        let name = self.path()?;
        Ok(Expr {
            kind: ExprKind::Ref(name),
            span: self.span_from(&start),
        })
    }
    fn semantic_parameters(&mut self) -> Result<Vec<Parameter>> {
        self.expect("(")?;
        let mut result = vec![];
        while !self.is(")") {
            let span = self.token().span.clone();
            let name = self.word()?;
            let default = if self.take("=") {
                Some(self.semantic_value()?)
            } else {
                None
            };
            result.push(Parameter {
                name,
                ty: None,
                default,
                span: self.span_from(&span),
            });
            if !self.take(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(result)
    }
    fn semantic_arguments(&mut self) -> Result<Vec<Argument>> {
        self.expect("(")?;
        let mut result = vec![];
        while !self.is(")") {
            let span = self.token().span.clone();
            let name = if matches!(self.peek(0), TokenKind::Word(_))
                && matches!(self.peek(1), TokenKind::Symbol(s) if s == "=")
            {
                let name = self.word()?;
                self.expect("=")?;
                Some(name)
            } else {
                None
            };
            let value = self.semantic_value()?;
            result.push(Argument {
                name,
                value,
                span: self.span_from(&span),
            });
            if !self.take(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(result)
    }
    fn semantic_value(&mut self) -> Result<Expr> {
        self.semantic_binary(0)
    }
    fn semantic_binary(&mut self, minimum: u8) -> Result<Expr> {
        let mut left = self.semantic_atom()?;
        loop {
            let (operator, precedence) = if self.is("||") {
                ("||", 1)
            } else if self.is("&&") {
                ("&&", 2)
            } else {
                break;
            };
            if precedence < minimum {
                break;
            }
            self.bump();
            let right = self.semantic_binary(precedence + 1)?;
            let span = Span::new(&left.span.file, left.span.start, right.span.end);
            left = Expr {
                kind: ExprKind::Binary(operator.into(), Box::new(left), Box::new(right)),
                span,
            };
        }
        // Left-associative operator chains also contribute to the AST resource bound.
        self.semantic_indexes(left)
    }
    fn semantic_atom(&mut self) -> Result<Expr> {
        let start = self.token().span.clone();
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error(
                "yl.depth_limit",
                "Semantic value nesting exceeds resource limit",
            ));
        }
        let kind = if self.take("match") {
            self.expect("(")?;
            let mut inputs = vec![self.semantic_value()?];
            while self.take(",") {
                inputs.push(self.semantic_value()?);
            }
            self.expect(")")?;
            self.expect("{")?;
            let mut cases = vec![];
            while !self.is("}") {
                let case_start = self.token().span.clone();
                let alternatives = self.match_patterns(inputs.len())?;
                self.expect("=>")?;
                let value = self.semantic_value()?;
                cases.push(ValueMatchCase {
                    alternatives,
                    value,
                    span: self.span_from(&case_start),
                });
                if !self.take(",") && !self.is("}") {
                    return Err(self.error(
                        "yl.unexpected_token",
                        "Expected ',' between match value cases",
                    ));
                }
            }
            self.expect("}")?;
            ExprKind::Match { inputs, cases }
        } else if self.take("!") {
            ExprKind::Not(Box::new(self.semantic_atom()?))
        } else if matches!(self.token().kind, TokenKind::Word(_))
            && matches!(self.peek(1), TokenKind::Symbol(value) if value == "=>")
        {
            let parameter = self.word()?;
            if matches!(parameter.as_str(), "self" | "absent" | "true" | "false") {
                return Err(self.error(
                    "yl.invalid_callback_parameter",
                    "Callback parameter cannot shadow a core value",
                ));
            }
            self.expect("=>")?;
            ExprKind::Lambda {
                parameter,
                body: Box::new(self.semantic_value()?),
            }
        } else if self.is("(") || self.is("[") {
            let list = self.take("[");
            if !list {
                self.expect("(")?;
            }
            let end = if list { "]" } else { ")" };
            let mut values = vec![];
            while !self.is(end) {
                values.push(self.semantic_value()?);
                if !self.take(",") {
                    break;
                }
            }
            self.expect(end)?;
            if list {
                ExprKind::List(values)
            } else if values.len() == 1 {
                ExprKind::Group(Box::new(values.remove(0)))
            } else {
                ExprKind::Tuple(values)
            }
        } else {
            match self.token().kind.clone() {
                TokenKind::String(s) => {
                    self.bump();
                    ExprKind::Literal(s)
                }
                TokenKind::Regex(s) => {
                    self.bump();
                    ExprKind::Regex(s)
                }
                TokenKind::Word(_) => {
                    let mut name = self.path()?;
                    while self.take(".") {
                        name.push('.');
                        name.push_str(&self.word()?);
                    }
                    if self.is("(") {
                        ExprKind::Call(name, self.semantic_arguments()?)
                    } else {
                        ExprKind::Ref(name)
                    }
                }
                _ => return Err(self.error("yl.unexpected_token", "Expected semantic value")),
            }
        };
        let value = Expr {
            kind,
            span: self.span_from(&start),
        };
        let value = self.semantic_indexes(value)?;
        self.depth -= 1;
        Ok(value)
    }
    fn semantic_indexes(&mut self, mut value: Expr) -> Result<Expr> {
        // Postfix chains can be combined through groups without increasing parser
        // call depth. Bound the resulting tree before the compiler clones it.
        let mut pending = vec![(&value, 1usize)];
        let mut depth = 0;
        while let Some((expr, current)) = pending.pop() {
            depth = depth.max(current);
            match &expr.kind {
                ExprKind::Group(inner) | ExprKind::Not(inner) | ExprKind::Index(inner, _) => {
                    pending.push((inner, current + 1));
                }
                ExprKind::Tuple(values) | ExprKind::List(values) => {
                    pending.extend(values.iter().map(|inner| (inner, current + 1)));
                }
                ExprKind::Call(_, args) => {
                    pending.extend(args.iter().map(|arg| (&arg.value, current + 1)));
                }
                ExprKind::Lambda { body, .. } => pending.push((body, current + 1)),
                ExprKind::Binary(_, left, right) => {
                    pending.push((left, current + 1));
                    pending.push((right, current + 1));
                }
                ExprKind::Match { inputs, cases } => {
                    pending.extend(inputs.iter().map(|inner| (inner, current + 1)));
                    for case in cases {
                        pending.push((&case.value, current + 1));
                        pending.extend(
                            case.alternatives
                                .iter()
                                .flatten()
                                .flatten()
                                .map(|inner| (inner, current + 1)),
                        );
                    }
                }
                _ => {}
            }
        }
        if depth > 128 {
            return Err(self.error(
                "yl.depth_limit",
                "Semantic value nesting exceeds resource limit",
            ));
        }
        while self.take("[") {
            depth += 1;
            if depth > 128 {
                return Err(self.error(
                    "yl.depth_limit",
                    "Semantic value nesting exceeds resource limit",
                ));
            }
            let TokenKind::Index(digits) = &self.token().kind else {
                return Err(self.error(
                    "yl.invalid_index",
                    "Expected a nonnegative integer literal index",
                ));
            };
            // A fixed-width index keeps source behavior independent of the host's pointer size.
            let index = digits.parse::<u32>().map_err(|_| {
                self.error(
                    "yl.invalid_index",
                    "Index exceeds the maximum supported value (4294967295)",
                )
            })? as usize;
            self.bump();
            self.expect("]")?;
            let span = self.span_from(&value.span);
            value = Expr {
                kind: ExprKind::Index(Box::new(value), index),
                span,
            };
        }
        Ok(value)
    }
    fn meaning_block(&mut self) -> Result<Vec<MeaningSyntaxAction>> {
        let mut actions = vec![];
        while !self.is("}") {
            actions.push(self.meaning_action()?);
        }
        self.expect("}")?;
        Ok(actions)
    }
    fn meaning_action(&mut self) -> Result<MeaningSyntaxAction> {
        let start = self.token().span.clone();
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error(
                "yl.depth_limit",
                "Semantic action nesting exceeds resource limit",
            ));
        }
        let action = if self.take("return") {
            MeaningSyntaxAction::Return {
                value: self.semantic_value()?,
                span: self.span_from(&start),
            }
        } else if matches!(self.peek(0), TokenKind::Word(_))
            && matches!(self.peek(1), TokenKind::Symbol(value) if value == ":=")
        {
            let name = self.word()?;
            if matches!(name.as_str(), "self" | "absent" | "true" | "false") {
                return Err(self.error("yl.invalid_binding", "Binding cannot shadow a core value"));
            }
            self.expect(":=")?;
            MeaningSyntaxAction::Binding {
                name,
                value: self.semantic_value()?,
                span: self.span_from(&start),
            }
        } else if self.take("when") {
            let condition = self.semantic_value()?;
            self.expect("{")?;
            let actions = self.meaning_block()?;
            let otherwise = if self.take("else") {
                self.expect("{")?;
                self.meaning_block()?
            } else {
                vec![]
            };
            MeaningSyntaxAction::If {
                condition,
                actions,
                otherwise,
                span: self.span_from(&start),
            }
        } else if self.take("match") {
            MeaningSyntaxAction::Match(self.meaning_match(start)?)
        } else {
            MeaningSyntaxAction::Call(self.meaning_call()?)
        };
        self.depth -= 1;
        Ok(action)
    }
    fn meaning_call(&mut self) -> Result<MeaningSyntaxCall> {
        let start = self.token().span.clone();
        let name = self.path()?;
        let arguments = self.semantic_arguments()?;
        Ok(MeaningSyntaxCall {
            name,
            arguments,
            span: self.span_from(&start),
        })
    }
    fn meaning_match(&mut self, start: Span) -> Result<MeaningSyntaxMatch> {
        self.expect("(")?;
        let mut inputs = vec![self.semantic_value()?];
        while self.take(",") {
            inputs.push(self.semantic_value()?);
        }
        self.expect(")")?;
        self.expect("{")?;
        let mut cases = vec![];
        while !self.is("}") {
            let case_start = self.token().span.clone();
            let alternatives = self.match_patterns(inputs.len())?;
            self.expect("=>")?;
            let all_actions = if self.take("{") {
                self.meaning_block()?
            } else {
                vec![self.meaning_action()?]
            };
            let mut actions = vec![];
            let mut nested_actions = vec![];
            for action in all_actions {
                match action {
                    MeaningSyntaxAction::Call(call) => actions.push(call),
                    other => nested_actions.push(other),
                }
            }
            cases.push(MeaningSyntaxCase {
                alternatives,
                actions,
                nested_actions,
                span: self.span_from(&case_start),
            });
            self.take(",");
        }
        self.expect("}")?;
        Ok(MeaningSyntaxMatch {
            inputs,
            cases,
            span: self.span_from(&start),
        })
    }
    fn match_patterns(&mut self, arity: usize) -> Result<Vec<Vec<Option<Expr>>>> {
        let mut alternatives = vec![];
        loop {
            let mut patterns = vec![];
            loop {
                patterns.push(if self.take("_") {
                    None
                } else {
                    Some(self.atom()?)
                });
                if !self.take(",") {
                    break;
                }
            }
            if patterns.len() == 1 && patterns[0].is_none() {
                patterns.resize(arity, None);
            }
            alternatives.push(patterns);
            if !self.take("|") {
                break;
            }
        }
        Ok(alternatives)
    }
    fn ide(&mut self) -> Result<(Ide, Option<Example>)> {
        let start = self.tokens[self.position.saturating_sub(1)].span.clone();
        self.expect("{")?;
        let mut example = None;
        let mut highlight_as = None;
        while !self.is("}") {
            if self.take("example") {
                if example.is_some() {
                    return Err(self.error("yl.duplicate_example", "Only one example is allowed"));
                }
                example = Some(self.example()?);
            } else if self.take("highlightAs") {
                if highlight_as.is_some() {
                    return Err(
                        self.error("yl.duplicate_highlight", "Only one highlightAs is allowed")
                    );
                }
                let token = self.bump();
                let TokenKind::String(category) = token.kind else {
                    return Err(self.error(
                        "yl.highlight",
                        "Expected a standard highlighting category string",
                    ));
                };
                if !HIGHLIGHT_TYPES.contains(&category.as_str()) {
                    return Err(Diagnostic::error("yl.highlight", format!("Unknown highlighting category {category:?}; expected a standard LSP token type"), token.span));
                }
                highlight_as = Some(category);
            } else {
                return Err(self.error("yl.ide", "Expected example or highlightAs inside ide"));
            }
        }
        self.expect("}")?;
        Ok((
            Ide {
                highlight_as,
                span: self.span_from(&start),
            },
            example,
        ))
    }
    fn example(&mut self) -> Result<Example> {
        let start = self.tokens[self.position.saturating_sub(1)].span.clone();
        let token = self.bump();
        if let TokenKind::String(text) = token.kind {
            Ok(Example {
                text,
                span: self.span_from(&start),
            })
        } else {
            Err(self.error("yl.example", "Expected example string"))
        }
    }
    fn meanings(&mut self) -> Result<Meanings> {
        let start = self.token().span.clone();
        self.expect("{")?;
        let mut boundary = false;
        let mut groups = vec![];
        let mut precedence = vec![];
        while !self.is("}") {
            let start = self.token().span.clone();
            if self.take("boundary") {
                if boundary {
                    return Err(self.error(
                        "yl.duplicate_boundary",
                        "Boundary is already declared for this node or pattern",
                    ));
                }
                boundary = true;
            } else if self.take("group") {
                let name = self.word()?;
                self.expect("{")?;
                let all_actions = self.meaning_block()?;
                let mut calls = vec![];
                let mut actions = vec![];
                for action in all_actions {
                    match action {
                        MeaningSyntaxAction::Call(call) => calls.push(call),
                        other => actions.push(other),
                    }
                }
                groups.push(MeaningSyntaxGroup {
                    actions,
                    name: Some(name),
                    calls,
                    matching: None,
                    span: self.span_from(&start),
                });
            } else if self.take("match") {
                let matching = self.meaning_match(start.clone())?;
                groups.push(MeaningSyntaxGroup {
                    actions: vec![],
                    name: None,
                    calls: vec![],
                    matching: Some(matching),
                    span: self.span_from(&start),
                });
            } else if self.is("when")
                || self.is("return")
                || (matches!(self.peek(0), TokenKind::Word(_))
                    && matches!(self.peek(1), TokenKind::Symbol(value) if value == ":="))
            {
                let action = self.meaning_action()?;
                groups.push(MeaningSyntaxGroup {
                    name: None,
                    calls: vec![],
                    matching: None,
                    actions: vec![action],
                    span: self.span_from(&start),
                });
            } else if self.take("precedence") {
                self.expect("{")?;
                while !self.is("}") {
                    let start = self.token().span.clone();
                    let mut selectors = vec![self.selector()?];
                    self.expect(">")?;
                    selectors.push(self.selector()?);
                    while self.take(">") {
                        selectors.push(self.selector()?);
                    }
                    precedence.push(MeaningSyntaxChain {
                        selectors,
                        span: self.span_from(&start),
                    });
                }
                self.expect("}")?;
            } else {
                let call = self.meaning_call()?;
                groups.push(MeaningSyntaxGroup {
                    actions: vec![],
                    name: None,
                    span: call.span.clone(),
                    calls: vec![call],
                    matching: None,
                });
            }
        }
        self.expect("}")?;
        let span = self.span_from(&start);
        Ok(Meanings {
            boundary,
            groups,
            precedence,
            span,
        })
    }
    fn rewrite_cases(&mut self) -> Result<Vec<RewriteCase>> {
        self.expect("{")?;
        let mut cases = vec![];
        while !self.is("}") {
            let start = self.token().span.clone();
            let tuple = self.take("(");
            let mut patterns = vec![];
            loop {
                patterns.push(if self.take("_") {
                    None
                } else {
                    Some(self.atom()?)
                });
                if !tuple || !self.take(",") {
                    break;
                }
            }
            if tuple {
                self.expect(")")?;
            }
            self.expect("=>")?;
            let mut diagnostics = vec![];
            let replacement = if self.take("{") {
                while self.is_diagnostic_call() {
                    let span = self.token().span.clone();
                    let name = self.path()?;
                    let arguments = self.condition_arguments()?;
                    diagnostics.push(Constraint {
                        kind: ConstraintKind::Call(name, arguments),
                        span: self.span_from(&span),
                    });
                }
                let value = if self.is("}") {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect("}")?;
                value
            } else {
                Some(self.expression()?)
            };
            cases.push(RewriteCase {
                patterns,
                replacement,
                diagnostics,
                span: self.span_from(&start),
            });
            self.take(",");
        }
        self.expect("}")?;
        Ok(cases)
    }
    fn is_diagnostic_call(&self) -> bool {
        let mut position = self.position;
        let Some(Token {
            kind: TokenKind::Word(first),
            ..
        }) = self.tokens.get(position)
        else {
            return false;
        };
        let mut name = first.clone();
        position += 1;
        while matches!(self.tokens.get(position).map(|token| &token.kind), Some(TokenKind::Symbol(symbol)) if symbol == "::")
        {
            let Some(Token {
                kind: TokenKind::Word(part),
                ..
            }) = self.tokens.get(position + 1)
            else {
                return false;
            };
            name.push_str("::");
            name.push_str(part);
            position += 2;
        }
        self.diagnostic_paths.contains(&name)
            && matches!(self.tokens.get(position).map(|token| &token.kind), Some(TokenKind::Symbol(symbol)) if symbol == "(")
    }
    fn case_label(&self) -> bool {
        let mut position = self.position;
        let mut depth = 0;
        loop {
            let Some(token) = self.tokens.get(position) else {
                return false;
            };
            match &token.kind {
                TokenKind::Symbol(s) if s == "=>" => return depth == 0,
                TokenKind::Symbol(s) if s == "(" => depth += 1,
                TokenKind::Symbol(s) if s == ")" && depth > 0 => depth -= 1,
                TokenKind::Word(_) => {}
                TokenKind::Symbol(s) if matches!(s.as_str(), "." | "::" | ",") => {}
                _ => return false,
            }
            position += 1;
        }
    }
    fn parameters(&mut self) -> Result<Vec<Parameter>> {
        let mut parameters = vec![];
        if !self.take("(") {
            return Ok(parameters);
        }
        while !self.is(")") {
            let span = self.token().span.clone();
            let name = self.word()?;
            let ty = if self.take(":") {
                Some(self.path()?)
            } else {
                None
            };
            let default = if self.take("=") {
                Some(self.expression()?)
            } else {
                None
            };
            parameters.push(Parameter {
                name,
                ty,
                default,
                span: self.span_from(&span),
            });
            if !self.take(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(parameters)
    }
    fn arguments(&mut self) -> Result<Vec<Argument>> {
        self.expect("(")?;
        let mut arguments = vec![];
        while !self.is(")") {
            let span = self.token().span.clone();
            let name = if matches!(self.peek(0), TokenKind::Word(_))
                && matches!(self.peek(1),TokenKind::Symbol(s) if s=="=")
            {
                let name = self.word()?;
                self.expect("=")?;
                Some(name)
            } else {
                None
            };
            let value = self.expression()?;
            arguments.push(Argument {
                name,
                value,
                span: self.span_from(&span),
            });
            if !self.take(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(arguments)
    }
    fn expression(&mut self) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error(
                "yl.depth_limit",
                "YL nesting exceeds implementation resource limit",
            ));
        }
        let first = self.sequence()?;
        let start = first.span.clone();
        let mut branches = vec![first];
        while self.take("|") {
            branches.push(self.sequence()?);
        }
        self.depth -= 1;
        if branches.len() == 1 {
            Ok(branches.remove(0))
        } else {
            Ok(Expr {
                kind: ExprKind::Choice(branches),
                span: self.span_from(&start),
            })
        }
    }
    fn sequence(&mut self) -> Result<Expr> {
        let first = self.capture()?;
        let start = first.span.clone();
        let mut items = vec![first];
        while self.starts_expression() {
            items.push(self.capture()?);
        }
        if items.len() == 1 {
            Ok(items.remove(0))
        } else {
            Ok(Expr {
                kind: ExprKind::Sequence(items),
                span: self.span_from(&start),
            })
        }
    }
    fn starts_expression(&self) -> bool {
        if self.boundary() || self.is("rewrite") || self.is("precedence") || self.is("checks") {
            return false;
        }
        if self.case_label() {
            return false;
        }
        // A case label is a boundary regardless of layout.
        if self.is(".") && matches!(self.peek(2),TokenKind::Symbol(s) if s=="=>") {
            return false;
        }
        if matches!(self.peek(0), TokenKind::Word(_))
            && matches!(self.peek(1),TokenKind::Symbol(s) if s=="::")
            && matches!(self.peek(3),TokenKind::Symbol(s) if s=="=>")
        {
            return false;
        }
        matches!(
            self.token().kind,
            TokenKind::Word(_) | TokenKind::String(_) | TokenKind::Regex(_)
        ) || self.is("(")
            || self.is(".")
    }
    fn capture(&mut self) -> Result<Expr> {
        if matches!(self.peek(0), TokenKind::Word(_))
            && matches!(self.peek(1),TokenKind::Symbol(s) if s==":")
        {
            let start = self.token().span.clone();
            let name = self.word()?;
            self.expect(":")?;
            let value = self.pipeline()?;
            Ok(Expr {
                kind: ExprKind::Capture(name, Box::new(value)),
                span: self.span_from(&start),
            })
        } else {
            self.pipeline()
        }
    }
    fn pipeline(&mut self) -> Result<Expr> {
        let mut value = self.atom()?;
        let start = value.span.clone();
        loop {
            let q = if self.take("?") {
                Some(Quantifier::Optional)
            } else if self.take("*") {
                Some(Quantifier::Star)
            } else if self.take("+") {
                Some(Quantifier::Plus)
            } else {
                None
            };
            if let Some(q) = q {
                value = Expr {
                    kind: ExprKind::Repeat(Box::new(value), q),
                    span: self.span_from(&start),
                };
            } else {
                break;
            }
        }
        while self.take("|>") {
            let name = self.path()?;
            let args = self.arguments()?;
            value = Expr {
                kind: ExprKind::Pipe(Box::new(value), name, args),
                span: self.span_from(&start),
            };
        }
        Ok(value)
    }
    fn atom(&mut self) -> Result<Expr> {
        let start = self.token().span.clone();
        let kind = if self.take("(") {
            let value = self.expression()?;
            self.expect(")")?;
            ExprKind::Group(Box::new(value))
        } else if self.take(".") {
            ExprKind::Variant(self.word()?)
        } else {
            match self.token().kind.clone() {
                TokenKind::String(s) => {
                    self.bump();
                    ExprKind::Literal(s)
                }
                TokenKind::Regex(s) => {
                    self.bump();
                    ExprKind::Regex(s)
                }
                TokenKind::Word(_) => {
                    let mut name = self.path()?;
                    while self.take(".") {
                        name.push('.');
                        name.push_str(&self.word()?);
                    }
                    if self.is("(") && !self.case_label() {
                        ExprKind::Call(name, self.arguments()?)
                    } else {
                        ExprKind::Ref(name)
                    }
                }
                _ => return Err(self.error("yl.unexpected_token", "Expected grammar expression")),
            }
        };
        Ok(Expr {
            kind,
            span: self.span_from(&start),
        })
    }
    fn condition_expression(&mut self, minimum: u8) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error("yl.depth_limit", "Condition nesting exceeds resource limit"));
        }
        let start = self.token().span.clone();
        let kind = if self.take("!") {
            ExprKind::Not(Box::new(self.condition_expression(4)?))
        } else if self.take("(") {
            let inner = self.condition_expression(0)?;
            self.expect(")")?;
            ExprKind::Group(Box::new(inner))
        } else if self.is("true")
            && !matches!(self.peek(1),TokenKind::Symbol(s) if s == "." || s == "?.")
        {
            self.bump();
            ExprKind::Bool(true)
        } else if self.is("false")
            && !matches!(self.peek(1),TokenKind::Symbol(s) if s == "." || s == "?.")
        {
            self.bump();
            ExprKind::Bool(false)
        } else if self.is("absent")
            && !matches!(self.peek(1),TokenKind::Symbol(s) if s == "." || s == "?.")
        {
            self.bump();
            ExprKind::Absent
        } else if matches!(self.token().kind, TokenKind::Word(_)) {
            let mut name = self.path()?;
            while self.is(".") || self.is("?.") {
                name.push_str(if self.take("?.") {
                    "?."
                } else {
                    self.expect(".")?;
                    "."
                });
                name.push_str(&self.word()?);
            }
            if self.is("(") {
                ExprKind::Call(name, self.condition_arguments()?)
            } else {
                ExprKind::Ref(name)
            }
        } else {
            self.atom()?.kind
        };
        let mut left = Expr {
            kind,
            span: self.span_from(&start),
        };
        let mut operators = 0;
        loop {
            let (operator, power) = if self.is("||") {
                ("||", 1)
            } else if self.is("&&") {
                ("&&", 2)
            } else if self.is("==") {
                ("==", 3)
            } else if self.is("!=") {
                ("!=", 3)
            } else {
                break;
            };
            if power < minimum {
                break;
            }
            operators += 1;
            if operators > 128 {
                return Err(
                    self.error("yl.depth_limit", "Condition nesting exceeds resource limit")
                );
            }
            self.bump();
            let right = self.condition_expression(power + 1)?;
            left = Expr {
                kind: ExprKind::Binary(operator.into(), Box::new(left), Box::new(right)),
                span: self.span_from(&start),
            };
        }
        self.depth -= 1;
        Ok(left)
    }
    fn condition_arguments(&mut self) -> Result<Vec<Argument>> {
        self.expect("(")?;
        let mut arguments = vec![];
        while !self.is(")") {
            let start = self.token().span.clone();
            let name = if matches!(self.peek(0), TokenKind::Word(_))
                && matches!(self.peek(1), TokenKind::Symbol(s) if s == "=")
            {
                let name = self.word()?;
                self.expect("=")?;
                Some(name)
            } else {
                None
            };
            let value = self.condition_expression(0)?;
            arguments.push(Argument {
                name,
                value,
                span: self.span_from(&start),
            });
            if !self.take(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(arguments)
    }
    fn constraint_block(&mut self) -> Result<Vec<Constraint>> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error(
                "yl.depth_limit",
                "Syntax check nesting exceeds resource limit",
            ));
        }
        self.expect("{")?;
        let mut body = vec![];
        while !self.is("}") {
            let span = self.token().span.clone();
            let kind = if self.take("when") {
                let condition = self.condition_expression(0)?;
                let body = self.constraint_block()?;
                ConstraintKind::When(condition, body)
            } else {
                let name = self.path()?;
                let arguments = self.condition_arguments()?;
                ConstraintKind::Call(name, arguments)
            };
            body.push(Constraint {
                kind,
                span: self.span_from(&span),
            });
        }
        self.expect("}")?;
        self.depth -= 1;
        Ok(body)
    }
}

#[cfg(test)]
mod query_syntax_tests {
    use super::*;
    #[test]
    fn expression_callbacks_and_nested_conditional_actions_roundtrip() {
        let source = r#"import { get, find, ancestors, contribute, isPresent } from "core/query"
export symbol scope
operation resolve(target, name) = find(ancestors(target), ancestor => get((get((ancestor, scope)), name)))
operation declare(target, name) {
    when isPresent(name) {
        match (name) {
            true => { when true { contribute((target, scope), name) } },
            _ => warn("Missing ${name}")
        }
    } else { error("No name") }
}
node Program = name: /[a-z]+/ { semantics { when !isPresent(resolve(self, name)) { declare(self, name) } } }
entry Program
"#;
        let parsed = parse_yl("query.yl", source).unwrap();
        assert!(parsed.declarations.iter().any(|decl| matches!(
            &decl.kind,
            DeclKind::Operation {
                body: OperationBody::Conditional(_),
                ..
            }
        )));
        let formatted = crate::format_yl("query.yl", source).unwrap();
        assert_eq!(crate::format_yl("query.yl", &formatted).unwrap(), formatted);
        let reparsed = parse_yl("query.yl", &formatted).unwrap();
        assert_eq!(parsed.declarations.len(), reparsed.declarations.len());
        assert!(formatted.contains("ancestor =>"));
    }
    #[test]
    fn malformed_callbacks_and_conditionals_are_diagnostics() {
        for source in [
            "operation q() = find([], item =>)",
            "operation q() { when true missing() }",
            "operation q() { when true { q() } else q() }",
        ] {
            assert!(parse_yl("bad.yl", source).is_err(), "{source}");
        }
    }
}

#[cfg(test)]
mod semantic_refresh_tests {
    use super::*;

    #[test]
    fn semantic_boolean_operators_have_standard_precedence() {
        let source = "operation q(a, b, c) = !a && b || c";
        let module = parse_yl("boolean.yl", source).unwrap();
        let DeclKind::Operation {
            body: OperationBody::Query(value),
            ..
        } = &module.declarations[0].kind
        else {
            panic!("Expected a query");
        };
        let ExprKind::Binary(or, left, right) = &value.kind else {
            panic!("Expected disjunction");
        };
        assert_eq!(or, "||");
        assert!(matches!(&right.kind, ExprKind::Ref(name) if name == "c"));
        let ExprKind::Binary(and, left, right) = &left.kind else {
            panic!("Expected conjunction");
        };
        assert_eq!(and, "&&");
        assert!(matches!(left.kind, ExprKind::Not(_)));
        assert!(matches!(&right.kind, ExprKind::Ref(name) if name == "b"));
    }

    #[test]
    fn semantic_boolean_operator_chains_obey_ast_depth_limits() {
        let source = format!("operation q() = {}", vec!["true"; 140].join(" && "));
        assert!(parse_yl("deep.yl", &source)
            .unwrap_err()
            .iter()
            .any(|error| error.code == "yl.depth_limit"));
        assert!(parse_yl("bad.yl", "operation q() = true &&").is_err());
    }

    #[test]
    fn expression_matches_keep_alternatives_values_and_spans() {
        let source = r#"operation compare(left, right) = match (left, right) {
            Type::number, Type::number | Type::string, Type::string => true,
            true, false => match (right) { _ => [left, right] }[0],
            _ => false
        }"#;
        let module = parse_yl("match.yl", source).unwrap();
        let DeclKind::Operation {
            body: OperationBody::Query(value),
            ..
        } = &module.declarations[0].kind
        else {
            panic!("Expected a query operation");
        };
        let ExprKind::Match { inputs, cases } = &value.kind else {
            panic!("Expected a value match");
        };
        assert_eq!(inputs.len(), 2);
        assert_eq!(cases[0].alternatives.len(), 2);
        assert_eq!(cases[2].alternatives, vec![vec![None, None]]);
        assert!(matches!(cases[1].value.kind, ExprKind::Index(_, 0)));
        assert_eq!(
            &source[cases[2].span.start..cases[2].span.end],
            "_ => false"
        );
    }

    #[test]
    fn malformed_expression_matches_are_diagnostics() {
        for source in [
            "operation q() = match () { _ => true }",
            "operation q() = match (true) { _ => }",
            "operation q() = match (true) { true => true false => false }",
            "operation q() = match (true) { _ => { q() } }",
        ] {
            assert!(parse_yl("bad.yl", source).is_err(), "{source}");
        }
    }

    #[test]
    fn bindings_returns_and_reactive_guards_preserve_source_spans() {
        let source = r#"operation connect(target, name) {
    ref := lookup(name)
    when isPresent(ref) { attach(target, ref) }
    return ref
}
node Reference = name: /[a-z]+/ {
    semantics {
        ref := connect(self, name)
        annotateWithType(self, typeOf(ref))
    }
    checks { when name.matches(/x/) { error("Invalid name") } }
}
entry Reference"#;
        let module = parse_yl("refresh.yl", source).unwrap();
        let DeclKind::Operation {
            body: OperationBody::Conditional(actions),
            ..
        } = &module.declarations[0].kind
        else {
            panic!("Expected an operation with declarative actions");
        };
        let MeaningSyntaxAction::Binding { name, span, .. } = &actions[0] else {
            panic!("Expected an immutable binding");
        };
        assert_eq!(name, "ref");
        assert_eq!(&source[span.start..span.end], "ref := lookup(name)");
        assert!(matches!(actions[1], MeaningSyntaxAction::If { .. }));
        assert!(matches!(actions[2], MeaningSyntaxAction::Return { .. }));
        let DeclKind::Node {
            meanings: Some(semantics),
            constraints,
            ..
        } = &module.declarations[1].kind
        else {
            panic!("Expected semantics and syntax checks");
        };
        assert!(matches!(
            semantics.groups[0].actions[0],
            MeaningSyntaxAction::Binding { .. }
        ));
        assert_eq!(constraints.len(), 1);
    }

    #[test]
    fn obsolete_block_and_condition_spellings_are_rejected() {
        for source in [
            "node A = \"a\" { meanings { action() } }",
            "node A = \"a\" { constraints { error(\"bad\") } }",
            "operation action() { if true { action() } }",
            "operation action() { self := lookup() }",
            "operation action() { value := }",
        ] {
            assert!(parse_yl("bad.yl", source).is_err(), "{source}");
        }
    }
}

#[cfg(test)]
mod nested_check_refresh_tests {
    use super::*;

    #[test]
    fn nested_when_checks_accept_imported_diagnostic_namespaces() {
        let source = r#"import * as diagnostics from "core/diagnostics"
node Name = value: /[a-z]+/ {
    checks {
        when value.isPresent {
            when value.matches(/bad/) {
                diagnostics::error("Invalid name")
            }
        }
    }
}
entry Name"#;
        let module = parse_yl("checks.yl", source).unwrap();
        let DeclKind::Node { constraints, .. } = &module.declarations[1].kind else {
            panic!("Expected checked node");
        };
        let ConstraintKind::When(_, outer) = &constraints[0].kind else {
            panic!("Expected outer when");
        };
        let ConstraintKind::When(_, inner) = &outer[0].kind else {
            panic!("Expected nested when");
        };
        let ConstraintKind::Call(name, _) = &inner[0].kind else {
            panic!("Expected diagnostic call");
        };
        assert_eq!(name, "diagnostics::error");
    }
}

#[cfg(test)]
mod rewrite_diagnostic_import_tests {
    use super::*;

    #[test]
    fn rewrite_diagnostics_accept_namespace_alias_and_forward_imports() {
        for (call, import) in [
            (
                "diagnostics::warn",
                "import * as diagnostics from \"core/diagnostics\"",
            ),
            ("note", "import { hint as note } from \"core/diagnostics\""),
        ] {
            let source = format!("enum Mode {{ a }} pipe wrap(mode: Mode) {{ rewrite value match mode {{ .a => {{ {call}(\"Notice\") value }} }} }} {import}");
            let module = parse_yl("rewrite.yl", &source).unwrap();
            let DeclKind::Pipe { arms, .. } = &module.declarations[1].kind else {
                panic!("Expected pipe");
            };
            let RewriteBody::Match { cases, .. } = &arms[0].body else {
                panic!("Expected rewrite match");
            };
            assert_eq!(cases[0].diagnostics.len(), 1);
            assert!(cases[0].replacement.is_some());
            assert!(
                matches!(&cases[0].diagnostics[0].kind, ConstraintKind::Call(name, _) if name == call)
            );
        }
    }

    #[test]
    fn user_authored_error_callable_is_not_implicitly_a_diagnostic() {
        let source = r#"pattern error(value) = value
        enum Mode { a }
        pipe wrap(mode: Mode) { rewrite value match mode { .a => { error(value) } } }"#;
        let module = parse_yl("rewrite.yl", source).unwrap();
        let DeclKind::Pipe { arms, .. } = &module.declarations[2].kind else {
            panic!("Expected pipe");
        };
        let RewriteBody::Match { cases, .. } = &arms[0].body else {
            panic!("Expected rewrite match");
        };
        assert!(cases[0].diagnostics.is_empty());
        assert!(cases[0].replacement.is_some());
    }

    #[test]
    fn hooks_and_backend_bindings_preserve_unevaluated_capture_expressions() {
        let source = "export hook add(left, right)\nnode Sum = left: Number right: Number { backend { runtime::add(left, right) } }\npattern Wrapper = Number { backend { wrap(self) } }";
        let module = parse_yl("hooks.yl", source).unwrap();
        let DeclKind::Hook { name, parameters } = &module.declarations[0].kind else {
            panic!("Expected hook declaration");
        };
        assert!(module.declarations[0].export);
        assert_eq!(name, "add");
        assert_eq!(
            parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>(),
            ["left", "right"]
        );
        let DeclKind::Node {
            backend: Some(backend),
            ..
        } = &module.declarations[1].kind
        else {
            panic!("Expected backend binding");
        };
        assert_eq!(backend.hook, "runtime::add");
        assert!(matches!(&backend.arguments[0].kind, ExprKind::Ref(name) if name == "left"));
        assert_eq!(
            &source[backend.span.start..backend.span.end],
            "backend { runtime::add(left, right) }"
        );
        assert!(matches!(
            &module.declarations[2].kind,
            DeclKind::Pattern {
                backend: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn malformed_hook_contracts_and_backend_blocks_have_diagnostics() {
        for (source, code) in [
            ("hook add(left = true)", "yl.hook_parameter"),
            ("hook add(left: Value)", "yl.hook_parameter"),
            ("hook add(left, left)", "yl.hook_parameter"),
            ("node N = \"n\" { backend {} }", "yl.backend_call"),
            (
                "node N = \"n\" { backend { return add(self) } }",
                "yl.backend_call",
            ),
            (
                "node N = \"n\" { backend { add(self) add(self) } }",
                "yl.backend_call",
            ),
            (
                "node N = \"n\" { backend { add(value=self) } }",
                "yl.backend_call",
            ),
            (
                "node N = \"n\" { backend { add(self) } backend { add(self) } }",
                "yl.duplicate_member",
            ),
            (
                "pattern N = \"n\" { backend { add(self) } backend { add(self) } }",
                "yl.duplicate_member",
            ),
        ] {
            let errors = parse_yl("invalid.yl", source).unwrap_err();
            assert_eq!(errors[0].code, code, "{source}");
        }
    }
}
