//! Syntax-guided layout over lossless token spellings. No filesystem policy lives here.
use crate::{
    diagnostic::{CompileResult, Diagnostic, Span},
    frontend::parse_yl,
    lexer::{lex_with_comments, Token, TokenKind},
    syntax::*,
};
use std::collections::{BTreeMap, BTreeSet};

/// Format one YL definition module. Comments and raw literals are preserved;
/// syntax errors return diagnostics instead of formatted output. Imports are not loaded.
pub fn format_yl(file: &str, source: &str) -> CompileResult<String> {
    let module = parse_yl(file, source)?;
    let tokens = lex_with_comments(file, source).map_err(|error| vec![error])?;
    let mut layout = Layout::default();
    for declaration in &module.declarations {
        layout.declaration(declaration, true);
    }
    let tokens = layout.prepare_meanings(&tokens);
    layout.attach_leading_comments(&tokens, source);
    let formatted = layout.render(&tokens, source);
    let output_tokens =
        lex_with_comments(file, &formatted).map_err(|_| vec![contract_error(file, source)])?;
    let raw_tokens = |tokens: &[Token], text: &str| {
        tokens
            .iter()
            .filter(|token| !matches!(token.kind, TokenKind::End))
            .map(|token| text[token.span.start..token.span.end].to_owned())
            .collect::<Vec<_>>()
    };
    if raw_tokens(&tokens, source) != raw_tokens(&output_tokens, &formatted) {
        return Err(vec![contract_error(file, source)]);
    }
    let reparsed = parse_yl(file, &formatted).map_err(|_| vec![contract_error(file, source)])?;
    if normalized(&module) != normalized(&reparsed) {
        return Err(vec![contract_error(file, source)]);
    }
    Ok(formatted)
}
fn contract_error(file: &str, source: &str) -> Diagnostic {
    Diagnostic::error(
        "yl.format_equivalence",
        "Formatting could not preserve this module's syntax; the source was left unchanged",
        Span::new(file, 0, source.len()),
    )
}
fn normalized(module: &Module) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                fields.remove("span");
                for value in fields.values_mut() {
                    strip(value);
                }
            }
            serde_json::Value::Array(items) => {
                for value in items {
                    strip(value);
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::to_value(module).expect("syntax serialization cannot fail");
    strip(&mut value);
    value
}
#[derive(Default)]
struct Layout {
    breaks: BTreeMap<usize, bool>,
    grammar: Vec<Span>,
    inline: Vec<Span>,
    enums: Vec<Span>,
    grammar_ends: BTreeSet<usize>,
    choice_separators: Vec<Span>,
    meanings: Vec<Meanings>,
}
impl Layout {
    fn line(&mut self, span: &Span, blank: bool) {
        self.breaks
            .entry(span.start)
            .and_modify(|current| *current |= blank)
            .or_insert(blank);
    }
    fn grammar(&mut self, expression: &Expr) {
        // Layout is determined by the grammar AST, not source whitespace or
        // token count. Captures, repetitions, groups and pipes are one unit.
        match &expression.kind {
            ExprKind::Sequence(items) => {
                for item in items {
                    self.line(&item.span, false);
                }
            }
            ExprKind::Choice(branches) => {
                self.line(&expression.span, false);
                for pair in branches.windows(2) {
                    self.choice_separators.push(Span::new(
                        &expression.span.file,
                        pair[0].span.end,
                        pair[1].span.start,
                    ));
                }
                for branch in branches {
                    if let ExprKind::Sequence(items) = &branch.kind {
                        for item in items.iter().skip(1) {
                            self.line(&item.span, false);
                        }
                    }
                }
            }
            _ => return,
        }
        self.grammar.push(expression.span.clone());
        self.grammar_ends.insert(expression.span.end);
    }
    fn declaration(&mut self, declaration: &Declaration, top: bool) {
        self.line(
            &declaration.span,
            !matches!(declaration.kind, DeclKind::Import { .. }),
        );
        match &declaration.kind {
            DeclKind::Import { .. } => self.inline.push(declaration.span.clone()),
            DeclKind::Enum { .. } => self.enums.push(declaration.span.clone()),
            DeclKind::Node {
                grammar,
                members,
                constraints,
                meanings,
                example,
                ..
            } => {
                if let Some(grammar) = grammar {
                    self.grammar(grammar);
                }
                if let Some(example) = example {
                    self.line(&example.span, false);
                }
                for member in members {
                    self.declaration(member, false);
                }
                for constraint in constraints {
                    self.constraint(constraint);
                }
                if let Some(meanings) = meanings {
                    self.meanings(meanings);
                }
            }
            DeclKind::Pattern {
                grammar,
                meanings,
                example,
                ..
            } => {
                self.grammar(grammar);
                if let Some(example) = example {
                    self.line(&example.span, false);
                }
                if let Some(meanings) = meanings {
                    self.meanings(meanings);
                }
            }
            DeclKind::Pipe { arms, .. } => {
                for arm in arms {
                    self.line(&arm.span, false);
                    match &arm.body {
                        RewriteBody::Match { cases, .. } => {
                            for case in cases {
                                self.line(&case.span, false);
                                for diagnostic in &case.diagnostics {
                                    self.constraint(diagnostic);
                                }
                            }
                        }
                        RewriteBody::Cases(cases) => {
                            for (pattern, _) in cases {
                                self.line(&pattern.span, false);
                            }
                        }
                        RewriteBody::Direct(_) => {}
                    }
                }
            }
            DeclKind::Constraint { body, .. }
            | DeclKind::Extend {
                constraints: body, ..
            } => {
                for constraint in body {
                    self.constraint(constraint);
                }
            }
            DeclKind::Entry { .. } | DeclKind::Symbol { .. } => {}
        }
        if !top {
            self.breaks.insert(declaration.span.start, true);
        }
    }
    fn meanings(&mut self, meanings: &Meanings) {
        self.meanings.push(meanings.clone());
        for group in &meanings.groups {
            self.line(&group.span, false);
            for call in &group.calls {
                self.line(&call.span, false);
            }
            if let Some(matching) = &group.matching {
                self.line(&matching.span, false);
                for case in &matching.cases {
                    self.line(&case.span, false);
                    for action in &case.actions {
                        // Single actions stay on the case's line. Blocks with
                        // several actions receive one line per call.
                        if case.actions.len() > 1 {
                            self.line(&action.span, false);
                        }
                    }
                }
            }
        }
        for chain in &meanings.precedence {
            self.line(&chain.span, false);
        }
    }
    fn constraint(&mut self, constraint: &Constraint) {
        self.line(&constraint.span, false);
        if let ConstraintKind::When(_, body) = &constraint.kind {
            for item in body {
                self.constraint(item);
            }
        }
    }
    fn prepare_meanings(&mut self, tokens: &[Token]) -> Vec<Token> {
        let mut tokens = tokens.to_vec();
        // Meanings stores boundary as a structural boolean, independent of its
        // source position. Move ONLY that marker; preserve every operation and
        // all other raw tokens in their original order. The token guard compares
        // against this canonical order and AST equivalence is still mandatory.
        for meanings in self.meanings.clone() {
            let span = &meanings.span;
            let Some(open) = tokens
                .iter()
                .position(|token| token.span.start == span.start)
            else {
                continue;
            };
            let mut braces = 0usize;
            let mut parentheses = 0usize;
            let mut boundary = None;
            for (index, token) in tokens.iter().enumerate().skip(open) {
                if token.span.end > span.end {
                    break;
                }
                match &token.kind {
                    TokenKind::Symbol(symbol) => match symbol.as_str() {
                        "{" => braces += 1,
                        "}" => braces = braces.saturating_sub(1),
                        "(" => parentheses += 1,
                        ")" => parentheses = parentheses.saturating_sub(1),
                        _ => {}
                    },
                    TokenKind::Word(word)
                        if braces == 1
                            && parentheses == 0
                            && !meanings.groups.iter().any(|group| {
                                group.span.start <= token.span.start
                                    && token.span.end <= group.span.end
                            }) =>
                    {
                        if word == "boundary" {
                            boundary = Some(index);
                            self.line(&token.span, false);
                        } else if word == "precedence" {
                            self.line(&token.span, false);
                        }
                    }
                    _ => {}
                }
                if braces == 0 {
                    break;
                }
            }
            if let Some(index) = boundary {
                let marker = tokens.remove(index);
                tokens.insert(open + 1, marker);
            }
        }
        tokens
    }
    fn attach_leading_comments(&mut self, tokens: &[Token], source: &str) {
        let breaks: Vec<_> = self
            .breaks
            .iter()
            .filter(|(_, blank)| **blank)
            .map(|(start, _)| *start)
            .collect();
        for start in breaks {
            let Some(index) = tokens.iter().position(|token| token.span.start == start) else {
                continue;
            };
            let mut first = index;
            while first > 0 && matches!(tokens[first - 1].kind, TokenKind::Comment) {
                let comment = &tokens[first - 1];
                let line = source[..comment.span.start]
                    .rfind('\n')
                    .map_or(0, |p| p + 1);
                if !source[line..comment.span.start].trim().is_empty() {
                    break;
                }
                first -= 1;
            }
            if first != index {
                self.breaks.insert(start, false);
                self.breaks.insert(tokens[first].span.start, true);
            }
        }
    }
    fn render(&self, tokens: &[Token], source: &str) -> String {
        let mut output = Writer::default();
        let mut stack: Vec<(bool, bool)> = vec![];
        let mut depth = 0;
        let mut previous: Option<&Token> = None;
        for token in tokens
            .iter()
            .filter(|token| !matches!(token.kind, TokenKind::End))
        {
            let raw = &source[token.span.start..token.span.end];
            let grammar = self
                .grammar
                .iter()
                .any(|span| span.start <= token.span.start && token.span.end <= span.end);
            let indent = depth + usize::from(grammar);
            if let Some(blank) = self.breaks.get(&token.span.start) {
                output.line(*blank);
            }
            if matches!(token.kind, TokenKind::Comment) {
                let line = source[..token.span.start].rfind('\n').map_or(0, |p| p + 1);
                let leading = source[line..token.span.start].trim().is_empty()
                    || output.text.is_empty()
                    || output.text.ends_with('\n');
                if leading {
                    output.line(false);
                } else {
                    output.space();
                }
                output.token(raw, indent);
                // Line comments must never consume a following source token.
                if raw.starts_with("//") || leading || raw.contains('\n') {
                    output.line(false);
                }
                continue;
            }
            if previous.is_some_and(|previous| {
                self.grammar_ends.contains(&previous.span.end)
                    && token.span.start >= previous.span.end
            }) {
                output.line(false);
            }
            let symbol = match &token.kind {
                TokenKind::Symbol(symbol) => Some(symbol.as_str()),
                _ => None,
            };
            if symbol == Some("|")
                && self
                    .choice_separators
                    .iter()
                    .any(|span| span.start <= token.span.start && token.span.end <= span.end)
            {
                output.line(false);
            }
            if symbol == Some("{") {
                let inline = self
                    .inline
                    .iter()
                    .any(|span| span.start <= token.span.start && token.span.end <= span.end);
                output.space();
                output.token(raw, indent);
                let enumeration = self
                    .enums
                    .iter()
                    .any(|span| span.start <= token.span.start && token.span.end <= span.end);
                stack.push((inline, enumeration));
                if !inline {
                    depth += 1;
                    output.line(false);
                }
            } else if symbol == Some("}") {
                let (inline, _) = stack.pop().unwrap_or_default();
                if inline {
                    output.space();
                    output.token(raw, indent);
                } else {
                    depth = depth.saturating_sub(1);
                    output.line(false);
                    output.token(raw, depth);
                }
            } else {
                if previous.is_some_and(|previous| {
                    matches!(&previous.kind, TokenKind::Symbol(symbol) if symbol == "}")
                        && !self.inline.iter().any(|span| {
                            span.start <= previous.span.start && previous.span.end <= span.end
                        })
                }) && !matches!(symbol, Some("," | ")"))
                {
                    output.line(false);
                }
                if stack.last().is_some_and(|(_, enumeration)| *enumeration)
                    && matches!(token.kind, TokenKind::Word(_))
                {
                    output.line(false);
                }
                if space_between(previous, token) {
                    output.space();
                }
                output.token(raw, indent);
            }
            previous = Some(token);
        }
        output.finish()
    }
}
fn space_between(previous: Option<&Token>, token: &Token) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    fn symbol(token: &Token) -> Option<&str> {
        match &token.kind {
            TokenKind::Symbol(value) => Some(value.as_str()),
            _ => None,
        }
    }
    let before = symbol(previous);
    let current = symbol(token);
    // Avoid coalescing two tokens into optional chaining when a postfix optional
    // pattern is followed by a contextual variant.
    if before == Some("?") && current == Some(".") {
        return true;
    }
    if matches!(
        current,
        Some(")" | "," | ":" | "::" | "?." | "?" | "*" | "+")
    ) {
        return false;
    }
    if current == Some(".") {
        return !matches!(before, Some("(" | "::" | "?." | "."))
            && !matches!(previous.kind, TokenKind::Word(_));
    }
    if matches!(before, Some("(" | "::" | "." | "?." | "!")) {
        return false;
    }
    if current == Some("(") {
        if before == Some(":") {
            return true;
        }
        return matches!(&previous.kind,TokenKind::Word(word) if word == "when" || word == "match");
    }
    true
}
#[derive(Default)]
struct Writer {
    text: String,
    protected: usize,
}
impl Writer {
    fn line(&mut self, blank: bool) {
        while self.text.len() > self.protected && self.text.ends_with(' ') {
            self.text.pop();
        }
        if self.text.is_empty() {
            return;
        }
        if !self.text.ends_with('\n') {
            self.text.push('\n');
        }
        if blank && !self.text.ends_with("\n\n") {
            self.text.push('\n');
        }
    }
    fn space(&mut self) {
        if !self.text.is_empty() && !self.text.ends_with([' ', '\n']) {
            self.text.push(' ');
        }
    }
    fn token(&mut self, raw: &str, indent: usize) {
        if self.text.is_empty() || self.text.ends_with('\n') {
            self.text.push_str(&"    ".repeat(indent));
        }
        self.text.push_str(raw);
        self.protected = self.text.len();
    }
    fn finish(mut self) -> String {
        while self.text.len() > self.protected && self.text.ends_with([' ', '\n']) {
            self.text.pop();
        }
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        self.text
    }
}
