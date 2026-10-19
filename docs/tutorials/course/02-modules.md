# 2. Compose modules and expressions

Keep the project configuration and programs. Replace the entry definition and
add lexical/expression modules. Imports are relative to the importing file;
`std/...` names are bundled and need no path configuration.

```yl file=definition/lexical.yl
import { boundedBy } from "std/parser"

pattern IdentifierPart = /[A-Za-z0-9_]/

export pattern keyword(value) = value |> boundedBy(IdentifierPart)

export node Name = value: /[A-Za-z_][A-Za-z0-9_]*/

export trivia Whitespace = /\s+/
```

```yl file=definition/expression.yl
export node Number = value: /[0-9]+/

node String = value: /"[^"\n]*"/

node Addition = left: Expression "+" right: Expression

export node Expression {
    node Addition

    node Number

    node String
}
```

```yl file=definition/language.yl
import { Name, Whitespace, keyword } from "./lexical"
import { Expression } from "./expression"

node LetDeclaration =
    keyword("let")
    name: Name
    "="
    initializer: Expression
    ";"

node Program = statements: LetDeclaration*

entry Program { trivia Whitespace }
```

```text file=programs/program.txt
let count = 40 + 2;
let greeting = "hello" + " world";
```

`yl project check .` accepts both declarations. The grammar recognizes addition;
it does not calculate `42` or decide whether strings may be added. Those are
later semantic and backend choices.

One recursive operator can use the default left-associative level. Multiple
competing recursive operators require explicit [precedence](/reference/precedence).
`keyword("let")` applies the language's identifier boundary without embedding a
trailing space into its literal. `boundedBy` is an authored pipe built on core
lookaround; whitespace still comes from the entry's trivia selection.

From the repository root, `cargo run --example 03-mini-js` explores a larger
syntax-only checkpoint with calls, functions, comments, lists, and precedence.

Previous: [Syntax](./01-syntax).

Next: [Diagnostics](./03-diagnostics).
