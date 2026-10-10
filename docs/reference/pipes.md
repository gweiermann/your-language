# Pipes and structural rewrite

A pipe transforms the structure of a grammar during language compilation. It receives the grammar expression on its left and produces a new matcher with the same value type and cardinality.

Pipes operate on grammar, not target source text. Their calls are expanded before the compiled language is executed.

## Wrapping a value

```yl
pipe wrapped(open, close) {
    rewrite value => open value close
}

node Name = text: /[a-z]+/
node Group = name: Name |> wrapped("(", ")")
```

`wrapped` matches a delimiter before and after the original `Name` grammar. The capture `name` still contains only the `Name` node. Delimiter values and any wrapper captures do not become part of the projected result.

This differs from a pattern containing `"(" Name ")"`, whose underlying sequence produces a tuple. A pipe preserves the input's value contract.

## Application and chaining

Apply a pipe with `|>`:

```yl
Name |> wrapped("(", ")")
```

Pipe calls accept positional and named parameters, defaults, and declared parameter types. They use the same [argument syntax](./enums#parameters-defaults-and-call-syntax) as parameterized patterns.

Pipes chain left to right. Group a sequence or choice when the entire fragment should be rewritten:

```yl
(A | B) |> first() |> second()
```

Without parentheses, a pipe binds more tightly than sequence or choice. See [Grammar operators](./grammar-operators#pipe-application).

## Rewrite bindings

The identifier after `rewrite` binds the original grammar fragment:

```yl
pipe bracketed() {
    rewrite value => "[" value "]"
}
```

For structural repetition arms, the binding names an item and the quantifier selects the shape of the input grammar:

```yl
pipe commaSeparated() {
    rewrite item* => (item ("," item)*)?
    rewrite item+ => item ("," item)*
}
```

`item*` matches zero-or-more input grammar; `item+` matches one-or-more input grammar. The replacement uses the bound item grammar in a new structure. A pipe application with no compatible structural arm is a definition error.

A rewrite can restrict the accepted result type:

```yl
pipe parenthesizedExpression() {
    rewrite value: Expression => "(" value ")"
}
```

The restriction accepts that node type and its members. It does not introduce target-language type checking.

## Preserving values

Only occurrences of the bound original grammar contribute to a pipe's result. Replacement punctuation, separators, and other surrounding grammar are matched and discarded from that result.

For list rewrites, the runtime collects bound item values in source order. An item that itself produces a tuple or a list remains one element; the pipe does not flatten the item's internal structure.

```yl
import { separatedBy } from "std/parser"

node Name = text: /[a-z]+/
node Names = values: Name* |> separatedBy(",")
```

The resulting `values` field is a flat list of `Name` nodes, not a tuple containing commas and nested repetition arrays.

## Cardinality

Every successful replacement path must preserve the input cardinality:

| Input result | Permitted number of original values | Omission |
| --- | --- | --- |
| Required scalar | Exactly one | Rejected |
| Optional scalar | Zero or one | Produces `absent` |
| Zero-or-more list | Zero or more | Produces `[]` |
| One-or-more list | One or more | Rejected |

Duplicating a required scalar or an optional scalar so both occurrences can contribute values is incompatible with its result type. A list rewrite can collect multiple item occurrences. Invalid cardinality is reported at the pipe application, where the actual input grammar is known.

## Enum-selected rewrites

An explicit `match` selects an enum parameter:

```yl
enum Delimiter { round square }

pipe delimit(kind: Delimiter) {
    rewrite value match kind {
        .round => "(" value ")"
        .square => "[" value "]"
    }
}
```

With exactly one enum parameter, `rewrite value { ... }` is shorthand for selecting it. With multiple enum parameters, write the selector explicitly.

To match combinations, select a tuple:

```yl
enum Mode { strict permissive }

pipe configured(first: Mode, second: Mode) {
    rewrite value match (first, second) {
        (.strict, .strict) => "(" value ")"
        (.strict, .permissive) => "[" value "]"
        (.permissive, _) => value
    }
}
```

Tuple cases have one component per selector. `_` can match a component or the complete tuple. Cases are examined in source order; the first matching case wins. The compiler requires exhaustive coverage and rejects cases completely shadowed by earlier cases.

## Case diagnostics

Use a block when a case emits a diagnostic:

```yl
rewrite value match (first, second) {
    (.strict, .strict) => "(" value ")"
    (.strict, .permissive) => {
        error("This combination is not supported.")
    }
    _ => {
        warning("No delimiter was applied for this configuration.")
        value
    }
}
```

Diagnostic statements precede the replacement grammar. `error(...)` rejects the pipe application and does not require a replacement. `warning(...)` and `help(...)` continue with the replacement. Returning the bound grammar unchanged is an identity transformation.

These diagnostics are emitted while compiling the language, when enum arguments select a case. They are not checks executed against every target-source occurrence.

The primary span points to the selected enum argument at the pipe caller. Other selected arguments and the diagnostic declaration supply secondary spans. Forwarded arguments retain their original caller locations. Defaulted arguments point to the pipe application, with help identifying the default.

## Standard helpers are ordinary pipes

`boundedBy` and `separatedBy` are implemented in YL in `std/parser`. Custom pipes use the same binding, selection, expansion, and projection mechanisms. Neither the runtime nor the AST builder depends on the name of a particular helper.
