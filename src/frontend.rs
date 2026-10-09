//! Token-based recursive descent for declarations, Pratt-style grammar precedence.
use crate::diagnostic::{CompileResult, Diagnostic, Span};
use crate::lexer::{lex, Token, TokenKind};
use crate::syntax::*;

pub fn parse_yl(file: &str, source: &str) -> CompileResult<Module> {
    let tokens = lex(file, source).map_err(|e| vec![e])?;
    let mut parser = Parser {
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
type Result<T> = std::result::Result<T, Diagnostic>;
struct Parser {
    tokens: Vec<Token>,
    position: usize,
    depth: usize,
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
        ["node", "trivia", "pattern", "pipe", "enum", "constraint"]
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
            self.expect("{")?;
            let mut names = vec![];
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
            self.expect("from")?;
            let t = self.bump();
            let TokenKind::String(path) = t.kind else {
                return Err(Diagnostic::error(
                    "yl.unexpected_token",
                    "Expected import path string",
                    t.span,
                ));
            };
            DeclKind::Import { names, path }
        } else if self.take("entry") {
            DeclKind::Entry(self.path()?)
        } else if self.take("extend") {
            self.expect("node")?;
            let name = self.path()?;
            self.expect("{")?;
            let mut constraints = vec![];
            while !self.is("}") {
                if !self.take("constraints") {
                    return Err(self.error("yl.extension_conflict","An extension may only add constraints in syntax-v0; metadata syntax is PARKED"));
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
            if self.take("=") {
                grammar = Some(self.expression()?);
            }
            if self.take("{") {
                while !self.is("}") {
                    if self.take("constraints") {
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
            }
        } else if self.take("pattern") {
            let name = self.word()?;
            let parameters = self.parameters()?;
            self.expect("=")?;
            let grammar = self.expression()?;
            DeclKind::Pattern {
                name,
                parameters,
                grammar,
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
                let body = if self.take("=>") {
                    RewriteBody::Direct(self.expression()?)
                } else {
                    self.expect("{")?;
                    let mut cases = vec![];
                    while !self.is("}") {
                        let variant = self.atom()?;
                        self.expect("=>")?;
                        let value = self.expression()?;
                        cases.push((variant, value));
                    }
                    self.expect("}")?;
                    RewriteBody::Cases(cases)
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
                "Expected declaration (when is only valid inside constraints)",
            ));
        };
        self.depth -= 1;
        Ok(Declaration {
            export,
            kind,
            span: self.span_from(&start),
        })
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
        if self.boundary() || self.is("rewrite") || self.is("precedence") || self.is("constraints")
        {
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
            let mut value = self.expression()?;
            self.expect(")")?;
            value.span = self.span_from(&start);
            return Ok(value);
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
                    if self.is("(") {
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
    fn constraint_block(&mut self) -> Result<Vec<Constraint>> {
        self.expect("{")?;
        let mut body = vec![];
        while !self.is("}") {
            let span = self.token().span.clone();
            let kind = if self.take("when") {
                let condition = self.atom()?;
                let body = self.constraint_block()?;
                ConstraintKind::When(condition, body)
            } else {
                let name = self.path()?;
                let arguments = self.arguments()?;
                ConstraintKind::Call(name, arguments.into_iter().map(|a| a.value).collect())
            };
            body.push(Constraint {
                kind,
                span: self.span_from(&span),
            });
        }
        self.expect("}")?;
        Ok(body)
    }
}
