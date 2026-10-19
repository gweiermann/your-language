//! Reactive semantic blocks, matches, groups, and tooling metadata.

use super::*;

impl Parser {
    pub(super) fn meaning_block(&mut self) -> Result<Vec<MeaningSyntaxAction>> {
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
    pub(super) fn match_patterns(&mut self, arity: usize) -> Result<Vec<Vec<Option<Expr>>>> {
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
    pub(super) fn ide(&mut self) -> Result<(Ide, Option<Example>)> {
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
    pub(super) fn meanings(&mut self) -> Result<Meanings> {
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
}
