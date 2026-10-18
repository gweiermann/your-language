//! Unevaluated backend arguments and semantic expressions.

use super::*;

impl Parser {
    pub(super) fn backend(&mut self) -> Result<Backend> {
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
    pub(super) fn selector(&mut self) -> Result<Expr> {
        let start = self.token().span.clone();
        let name = self.path()?;
        Ok(Expr {
            kind: ExprKind::Ref(name),
            span: self.span_from(&start),
        })
    }
    pub(super) fn semantic_parameters(&mut self) -> Result<Vec<Parameter>> {
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
    pub(super) fn semantic_arguments(&mut self) -> Result<Vec<Argument>> {
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
    pub(super) fn semantic_value(&mut self) -> Result<Expr> {
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
}
