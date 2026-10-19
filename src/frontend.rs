//! Token-based recursive descent for declarations, Pratt-style grammar precedence.
use crate::diagnostic::{CompileResult, Diagnostic, Span};
use crate::lexer::{documentation_text, lex, lex_with_comments, Token, TokenKind};
use crate::syntax::*;
use std::collections::BTreeSet;

mod checks;
mod declarations;
mod diagnostic_imports;
mod documentation;
mod grammar;
mod rewrites;
mod semantic_actions;
mod semantic_values;
mod tokens;
use diagnostic_imports::imported_diagnostic_paths;
use documentation::attach_documentation;

/// Parse a complete source file into a spanned declaration graph.
///
/// `file` is the diagnostic source identity; `source` is UTF-8 source text.
/// Comments are partitioned once and documentation is attached after grammar parsing.
pub fn parse_yl(file: &str, source: &str) -> CompileResult<Module> {
    let (comments, tokens): (Vec<Token>, Vec<Token>) = lex_with_comments(file, source)
        .map_err(|error| vec![error])?
        .into_iter()
        .partition(|token| matches!(token.kind, TokenKind::Comment));
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
    attach_documentation(&mut declarations, &comments, source);
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
#[cfg(test)]
mod nested_check_refresh_tests;
#[cfg(test)]
mod query_syntax_tests;
#[cfg(test)]
mod rewrite_diagnostic_import_tests;
#[cfg(test)]
mod semantic_refresh_tests;
