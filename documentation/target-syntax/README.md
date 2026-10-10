# MiniJS target syntax

This directory is the syntax-v0 validation language.

- `lexical.yl` defines trivia, names, and the language-local `keyword(...)` helper.
- `minijs.yl` defines the AST grammar and entrypoint.

The example intentionally uses imports, abstract nodes, precedence, patterns, pipes, trivia, keyword boundaries, and AST captures without semantic relations/scopes.

MiniJS explicitly imports `Whitespace` and `Comment`, then selects them in
`entry Program { trivia Whitespace, Comment }`. Importing lexical helpers alone
does not activate trivia.
