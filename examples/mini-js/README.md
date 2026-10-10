# MiniJS

MiniJS is a compact programming-language grammar demonstrating modular YL definitions, abstract nodes, recursive expressions, precedence, reusable patterns, structural pipes, and explicit trivia selection.

- `lexical.yl` exports names, whitespace, comments, and a keyword boundary pattern.
- `minijs.yl` defines expression and statement families and selects `Program` as its entry node.
- `program.js` contains a valid sample program.
- `invalid.js` contains an unexpected source token for inspecting diagnostics.

```sh
yl check examples/mini-js/minijs.yl
yl compile examples/mini-js/minijs.yl -o minijs.ylc
yl language minijs.ylc ast examples/mini-js/program.js
yl language minijs.ylc check examples/mini-js/invalid.js
```

The grammar includes variables, functions, return statements, calls, member access, unary operators, and arithmetic. It illustrates a selected JavaScript-like syntax; it does not implement JavaScript semantics or the complete JavaScript grammar.

Read the [walkthrough](../../docs/tutorials/mini-js.md) for a guided explanation.
