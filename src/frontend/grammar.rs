//! Grammar parameters and operator-precedence parsing.

use super::*;

impl Parser {
    pub(super) fn parameters(&mut self) -> Result<Vec<Parameter>> {
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
    pub(super) fn arguments(&mut self) -> Result<Vec<Argument>> {
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
    pub(super) fn expression(&mut self) -> Result<Expr> {
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
    pub(super) fn atom(&mut self) -> Result<Expr> {
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
}
