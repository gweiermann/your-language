use crate::diagnostic::{Diagnostic, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Word(String),
    /// Digits are accepted only by positional-access syntax, not as query values.
    Index(String),
    String(String),
    Regex(String),
    Symbol(String),
    Comment,
    End,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Classify source comments, not regex or string tokens containing these spellings.
pub(crate) fn is_documentation_comment(raw: &str) -> bool {
    raw.starts_with("///") || raw.starts_with("/**")
}

/// Normalize documentation for presentation while the token retains its raw span.
pub(crate) fn documentation_text(raw: &str) -> Option<String> {
    if let Some(line) = raw.strip_prefix("///") {
        return Some(line.strip_prefix(' ').unwrap_or(line).to_owned());
    }
    if !raw.starts_with("/**") || !raw.ends_with("*/") {
        return None;
    }
    // In /**/ the opening and closing markers overlap at the second star.
    if raw.len() <= 5 {
        return Some(String::new());
    }
    let lines: Vec<&str> = raw[3..raw.len() - 2]
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line = line.trim_end();
            if index == 0 {
                return line.trim_start();
            }
            let margin = line.trim_start();
            if let Some(content) = margin.strip_prefix('*') {
                content.strip_prefix(' ').unwrap_or(content)
            } else {
                line
            }
        })
        .collect();
    let start = lines
        .iter()
        .position(|line| !line.is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map_or(start, |index| index + 1);
    let lines = &lines[start..end];
    let indent = lines
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.chars().take_while(|ch| ch.is_whitespace()).count())
        .min()
        .unwrap_or(0);
    Some(
        lines
            .iter()
            .map(|line| {
                line.char_indices()
                    .nth(indent)
                    .map_or("", |(start, _)| &line[start..])
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn lex(file: &str, source: &str) -> Result<Vec<Token>, Diagnostic> {
    tokenize(file, source, false)
}

pub(crate) fn lex_with_comments(file: &str, source: &str) -> Result<Vec<Token>, Diagnostic> {
    tokenize(file, source, true)
}

fn tokenize(file: &str, source: &str, comments: bool) -> Result<Vec<Token>, Diagnostic> {
    let mut tokens = vec![];
    let mut i = 0;
    while i < source.len() {
        let start = i;
        let ch = source[i..].chars().next().unwrap_or('\0');
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        if source[i..].starts_with("//") {
            i += source[i..].find('\n').unwrap_or(source.len() - i);
            if comments {
                let end = if source.as_bytes().get(i.wrapping_sub(1)) == Some(&b'\r') {
                    i - 1
                } else {
                    i
                };
                tokens.push(Token {
                    kind: TokenKind::Comment,
                    span: Span::new(file, start, end),
                });
            }
            continue;
        }
        if source[i..].starts_with("/*") {
            if let Some(end) = source[i + 2..].find("*/") {
                i += end + 4;
                if comments {
                    tokens.push(Token {
                        kind: TokenKind::Comment,
                        span: Span::new(file, start, i),
                    });
                }
                continue;
            }
            return Err(Diagnostic::error(
                "yl.unterminated_comment",
                "Unterminated comment",
                Span::new(file, start, source.len()),
            ));
        }
        let kind = if ch == '"' || ch == '/' {
            let delimiter = ch;
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < source.len() {
                let c = source[i..].chars().next().unwrap_or('\0');
                i += c.len_utf8();
                if c == delimiter {
                    closed = true;
                    break;
                }
                if c == '\\' {
                    let Some(next) = source[i..].chars().next() else {
                        break;
                    };
                    i += next.len_utf8();
                    if delimiter == '/' {
                        if next != '/' {
                            value.push('\\');
                        }
                        value.push(next);
                    } else {
                        value.push(match next {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            '"' => '"',
                            '\\' => '\\',
                            _ => {
                                return Err(Diagnostic::error(
                                    "yl.invalid_escape",
                                    "Unknown string escape",
                                    Span::new(file, i - next.len_utf8() - 1, i),
                                ))
                            }
                        });
                    }
                } else {
                    value.push(c);
                }
            }
            if !closed {
                return Err(Diagnostic::error(
                    "yl.unterminated_literal",
                    "Unterminated literal",
                    Span::new(file, start, i),
                ));
            }
            if delimiter == '"' {
                TokenKind::String(value)
            } else {
                TokenKind::Regex(value)
            }
        } else if ch.is_ascii_digit() {
            i += 1;
            while source.as_bytes().get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            TokenKind::Index(source[start..i].into())
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            i += 1;
            while let Some(c) = source[i..].chars().next() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            TokenKind::Word(source[start..i].into())
        } else {
            let symbol = [":=", "::", "|>", "=>", "?.", "&&", "||", "==", "!="]
                .into_iter()
                .find(|s| source[i..].starts_with(s));
            if let Some(s) = symbol {
                i += s.len();
                TokenKind::Symbol(s.into())
            } else if "{}[](),:=?*+|>.!".contains(ch) {
                i += ch.len_utf8();
                TokenKind::Symbol(ch.to_string())
            } else {
                return Err(Diagnostic::error(
                    "yl.unexpected_token",
                    format!("Unexpected character {ch:?}"),
                    Span::new(file, start, start + ch.len_utf8()),
                ));
            }
        };
        tokens.push(Token {
            kind,
            span: Span::new(file, start, i),
        });
    }
    tokens.push(Token {
        kind: TokenKind::End,
        span: Span::new(file, i, i),
    });
    Ok(tokens)
}
