# Walk through MiniJS

MiniJS demonstrates how YL features combine into a programming-language grammar. Its source lives in `examples/mini-js/` and is exercised through compilation, artifact reload, AST generation, and negative source fixtures.

## Lexical definitions

`lexical.yl` exports a `Name` node, a keyword pattern, whitespace, and an abstract comment family. The entry module imports these and explicitly selects `Whitespace` and `Comment`. Comment alternatives recognize line and block comments without creating AST fields.

## Expression identity

`Expression` is abstract. Number, string, name, group, member, call, unary, and binary nodes are members. Parsing an expression returns the concrete matching node rather than an `Expression` wrapper.

```yl
node Expression {
    node Number = value: /\d+(?:\.\d+)?/
    node Group = "(" value: Expression ")"
    node Sum = left: Expression operator: ("+" | "-") right: Expression
}
```

This excerpt illustrates identity and recursion; the complete grammar adds a precedence block and additional alternatives.

## Precedence as a declaration

The full grammar declares:

```yl
precedence {
    Member, Call >
    Unary >
    right Power >
    Product >
    Sum
}
```

Member access and calls bind most tightly. Product binds tighter than sum. Power associates to the right; binary levels otherwise associate to the left. Parenthesized expressions remain ordinary base alternatives.

## Statements and lists

The statement family includes variable declarations, return statements, functions, and expression membership. Function parameters and call arguments use `separatedBy` with optional trailing commas. The runtime returns lists of items, preserving AST identities and discarding separator values.

## Execute the complete flow

```sh
yl check examples/mini-js/minijs.yl
yl compile examples/mini-js/minijs.yl -o minijs.ylc
yl language minijs.ylc ast examples/mini-js/program.js
yl language minijs.ylc check examples/mini-js/invalid.js
yl language minijs.ylc check examples/mini-js/invalid.js --json
```

The valid sample produces a `Program` node with four statements. The invalid source demonstrates an unexpected-token diagnostic. `.ylc` contains the normalized grammar required to repeat these operations without the original definition files.

MiniJS deliberately covers a selected JavaScript-like syntax. It does not perform JavaScript name resolution, type checking, execution, or full JavaScript parsing. For embedding, continue with [the Rust tutorial](/tutorials/embedding).
