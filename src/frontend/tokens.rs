//! Token cursor operations and declaration boundaries.

use super::*;

impl Parser {
    pub(super) fn token(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }
    pub(super) fn peek(&self, offset: usize) -> &TokenKind {
        &self.tokens[(self.position + offset).min(self.tokens.len() - 1)].kind
    }
    pub(super) fn is(&self, s: &str) -> bool {
        matches!(&self.token().kind, TokenKind::Word(w) | TokenKind::Symbol(w) if w==s)
    }
    pub(super) fn take(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.bump();
            true
        } else {
            false
        }
    }
    pub(super) fn bump(&mut self) -> Token {
        let t = self.token().clone();
        if self.position + 1 < self.tokens.len() {
            self.position += 1;
        }
        t
    }
    pub(super) fn error(&self, code: &str, message: &str) -> Diagnostic {
        Diagnostic::error(code, message, self.token().span.clone())
    }
    pub(super) fn expect(&mut self, s: &str) -> Result<()> {
        if self.take(s) {
            Ok(())
        } else {
            Err(self.error("yl.unexpected_token", &format!("Expected {s}")))
        }
    }
    pub(super) fn word(&mut self) -> Result<String> {
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
    pub(super) fn path(&mut self) -> Result<String> {
        let mut name = self.word()?;
        while self.take("::") {
            name.push_str("::");
            name.push_str(&self.word()?);
        }
        Ok(name)
    }
    pub(super) fn span_from(&self, start: &Span) -> Span {
        Span::new(
            &start.file,
            start.start,
            self.tokens[self.position.saturating_sub(1)].span.end,
        )
    }
    pub(super) fn boundary(&self) -> bool {
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
}
