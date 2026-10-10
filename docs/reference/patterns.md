# Patterns

A pattern gives a reusable grammar fragment a name. Unlike a node, it does not introduce AST identity. Its value is the value produced by its underlying grammar.

## Named patterns

```yl
pattern IdentifierText = /[_a-zA-Z][_a-zA-Z0-9]*/

node Name = value: IdentifierText
```

The `Name` node stores the matched text under `value`. There is no `IdentifierText` node in the tree.

Patterns can compose node references, regexes, literals, groups, choices, repetitions, and pipes:

```yl
import { separatedBy } from "std/parser"

node Name = value: /[a-z]+/
pattern Names = Name* |> separatedBy(",")
node ParameterList = "(" names: Names ")"
```

`names` receives a list of `Name` nodes. The pipe preserves that list value while matching commas between items.

## Grammar parameters

Parameterized patterns accept grammar expressions:

```yl
pattern wrapped(value) = "(" value ")"

node Number = value: /[0-9]+/
node Group = contents: wrapped(Number)
```

The argument `Number` supplies a grammar matcher. It is not a runtime AST argument or a string naming a rule. The compiler resolves and expands the pattern when compiling the language.

Because literals produce values, `wrapped(Number)` returns a tuple containing the opening delimiter, the number node, and the closing delimiter. A pattern does not automatically discard delimiters. For a wrapper that preserves only the input value, use a [pipe](./pipes#wrapping-a-value).

## Named arguments and defaults

Parameters may provide defaults, and calls may use named arguments:

```yl
pattern surrounded(value, open="(", close=")") =
    open value close

node Number = value: /[0-9]+/
node Bracketed = contents: surrounded(Number, open="[", close="]")
```

Parameter declarations use `=` for defaults. Named call arguments also use `=`. Supplied arguments are bound before expansion; missing required parameters, unknown names, duplicate arguments, and incompatible arguments produce definition diagnostics.

Place positional arguments before named arguments.

## Typed grammar parameters

Untyped parameters accept grammar expressions without a declared node restriction. A typed parameter restricts the grammar's result type:

```yl
node Expression {
    node Number = value: /[0-9]+/
    node NameExpression = value: /[a-z]+/
}

pattern expressionInParens(value: Expression) =
    "(" value ")"
```

The pattern accepts `Expression` and its concrete members. An unrelated node or text-producing regex does not satisfy that node type. Types on grammar parameters describe grammar results, rather than target-language variable types.

```yl
node NumericGroup = contents: expressionInParens(Expression::Number)
```

## Visibility and reuse

Export a pattern when it forms part of a module's public grammar interface:

```yl
export pattern IdentifierText = /[_a-zA-Z][_a-zA-Z0-9]*/
```

Other modules import it through the ordinary [module rules](./modules). Patterns may refer to declarations defined later in the same resolved module graph.

## Pattern or node?

Use a node when the AST should identify a construct and expose named fields. Use a pattern for grammar reuse when the underlying value is sufficient. Use a pipe when transforming how an existing grammar is matched while preserving its output type.

For example, a `Parameter` node represents a parameter in the tree, a `Parameters` pattern names a reusable list grammar, and `separatedBy` adjusts that list grammar to consume separators without placing separators in the result.
