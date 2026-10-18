//! Structural rewrite cases and imported diagnostic recognition.

use super::*;

impl Parser {
    pub(super) fn rewrite_cases(&mut self) -> Result<Vec<RewriteCase>> {
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
    pub(super) fn is_diagnostic_call(&self) -> bool {
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
    pub(super) fn case_label(&self) -> bool {
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
}
