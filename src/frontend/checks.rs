//! Syntax-local condition expressions and checks.

use super::*;

impl Parser {
    pub(super) fn condition_expression(&mut self, minimum: u8) -> Result<Expr> {
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
    pub(super) fn condition_arguments(&mut self) -> Result<Vec<Argument>> {
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
    pub(super) fn constraint_block(&mut self) -> Result<Vec<Constraint>> {
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
