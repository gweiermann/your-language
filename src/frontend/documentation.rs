//! Attach declaration documentation after token parsing, without affecting grammar boundaries.

use super::*;

// Attach after parsing so documentation remains legal wherever ordinary comments
// are legal, and never becomes a grammar expression or declaration boundary.
pub(super) fn attach_documentation(
    declarations: &mut [Declaration],
    tokens: &[Token],
    source: &str,
) {
    for declaration in declarations {
        let mut end = declaration.span.start;
        let mut lines = Vec::new();
        let mut first = end;
        let preceding = tokens.partition_point(|token| token.span.end <= end);
        for token in tokens[..preceding].iter().rev() {
            let raw = &source[token.span.start..token.span.end];
            let gap = &source[token.span.end..end];
            let line_start = source[..token.span.start].rfind('\n').map_or(0, |i| i + 1);
            if !matches!(token.kind, TokenKind::Comment)
                || !(raw.starts_with("///") || raw.starts_with("/**"))
                || !gap.chars().all(char::is_whitespace)
                || gap.bytes().filter(|&byte| byte == b'\n').count() > 1
                || !source[line_start..token.span.start].trim().is_empty()
            {
                break;
            }
            lines.push(documentation_text(raw).unwrap_or_default());
            first = token.span.start;
            end = first;
        }
        if !lines.is_empty() {
            lines.reverse();
            let last_end = tokens[preceding - 1].span.end;
            declaration.documentation = Some(Documentation {
                text: lines.join("\n"),
                span: Span::new(&declaration.span.file, first, last_end),
            });
        }
        if let DeclKind::Node { members, .. } = &mut declaration.kind {
            attach_documentation(members, tokens, source);
        }
    }
}
