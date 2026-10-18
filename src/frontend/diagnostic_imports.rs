//! Recognize diagnostic imports before declarations, including forward imports and aliases.

use super::*;

pub(super) fn imported_diagnostic_paths(tokens: &[Token]) -> BTreeSet<String> {
    fn word(token: Option<&Token>, value: &str) -> bool {
        matches!(token.map(|token| &token.kind), Some(TokenKind::Word(name)) if name == value)
    }
    let mut paths = BTreeSet::new();
    for index in 0..tokens.len() {
        if !word(tokens.get(index), "import") {
            continue;
        }
        let Some(from) = (index + 1..tokens.len())
            .find(|&next| word(tokens.get(next), "from") || word(tokens.get(next), "import"))
        else {
            continue;
        };
        if !word(tokens.get(from), "from")
            || !matches!(tokens.get(from + 1).map(|token| &token.kind), Some(TokenKind::String(path)) if path == "core/diagnostics")
        {
            continue;
        }
        if matches!(tokens.get(index + 1).map(|token| &token.kind), Some(TokenKind::Symbol(symbol)) if symbol == "*")
        {
            if let Some(Token {
                kind: TokenKind::Word(namespace),
                ..
            }) = tokens.get(index + 3)
            {
                for name in ["error", "warn", "hint"] {
                    paths.insert(format!("{namespace}::{name}"));
                }
            }
        } else {
            let mut item = index + 2;
            while item < from {
                if let TokenKind::Word(name) = &tokens[item].kind {
                    if matches!(name.as_str(), "error" | "warn" | "hint") {
                        let alias = if word(tokens.get(item + 1), "as") {
                            match tokens.get(item + 2).map(|token| &token.kind) {
                                Some(TokenKind::Word(alias)) => alias,
                                _ => name,
                            }
                        } else {
                            name
                        };
                        paths.insert(alias.clone());
                    }
                }
                item += 1;
            }
        }
    }
    paths
}
