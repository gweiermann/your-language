# MiniJS

MiniJS is a compact programming-language grammar demonstrating modular YL definitions, abstract nodes, recursive expressions, precedence, reusable patterns, structural pipes, and explicit trivia selection.

- `definition/lexical.yl` exports names, whitespace, comments, and a keyword boundary pattern.
- `definition/minijs.yl` defines expression and statement families and selects `Program` as its entry node.
- `programs/program.txt` contains a valid sample program.
- `programs/invalid.txt` contains an unexpected source token for inspecting diagnostics.

```sh
yl check examples/03-mini-js/definition/minijs.yl
yl compile examples/03-mini-js/definition/minijs.yl -o minijs.ylc
yl language minijs.ylc ast examples/03-mini-js/programs/program.txt
yl language minijs.ylc check examples/03-mini-js/programs/invalid.txt
```

The grammar includes variables, functions, return statements, calls, member access, unary operators, and arithmetic. It illustrates a selected JavaScript-like syntax; it does not implement JavaScript semantics or the complete JavaScript grammar.

Read the [walkthrough](../../docs/tutorials/mini-js.md) for a guided explanation.

The common runner verifies valid and negative fixtures: `cargo run -- project check examples/03-mini-js`.
