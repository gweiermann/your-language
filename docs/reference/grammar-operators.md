# Grammar operators

Grammar expressions compose matchers. Their operators determine both which source text is consumed and which value is produced.

## Operator precedence

From strongest to weakest:

| Level | Syntax | Meaning |
| --- | --- | --- |
| Grouping | `(A B)` | Treat a fragment as one expression |
| Repetition | `A?`, `A*`, `A+` | Optional, zero-or-more, one-or-more |
| Pipe | `A |> transform()` | Apply a structural grammar rewrite |
| Capture | `field: A` | Record the value under a field name |
| Sequence | `A B` | Match one expression after another |
| Choice | `A \| B` | Try alternatives in order |

This is grammar-expression precedence. It is separate from the [operator precedence](./precedence) of a target expression language.

## Sequence

Adjacent expressions form a sequence:

```yl
node Assignment = name: /[a-z]+/ "=" value: /[0-9]+/
```

Each element starts where the previous one ends, after any explicitly selected trivia. The sequence succeeds only when every element succeeds.

A sequence's value is determined by its value-producing elements. Literals and regexes produce text, node references produce AST nodes, and lookaround produces no value. See [Grammar value rules](./nodes#grammar-value-rules).

## Ordered choice

`|` introduces alternatives:

```yl
pattern BooleanText = "true" | "false"
node Boolean = value: BooleanText
```

Alternatives are tried from left to right. The first successful alternative supplies the value. Captures and diagnostics from a failed alternative do not leak into a later successful alternative.

Choice has lower precedence than sequence:

```yl
pattern Choice = "a" "b" | "c"
```

This means `("a" "b") | "c"`. Use parentheses to choose within a sequence:

```yl
pattern Choice = "a" ("b" | "c")
```

## Optional expressions

`A?` matches zero or one occurrence:

```yl
node Parameter = name: /[a-z]+/ ("=" default: /[0-9]+/)?
```

When the optional expression does not match, its value is absent. An optional capture is serialized as JSON `null` when absent. Conditions can inspect presence using `.isPresent()` or optional chaining.

## Repetition

`A*` accepts zero or more occurrences; `A+` requires at least one:

```yl
node Digit = value: /[0-9]/
node Digits = values: Digit+
```

Repetition produces a list in source order. Repeat a grammar that consumes text on success; empty matches cannot advance a repetition.

For punctuation-separated lists, use the ordinary YL [separatedBy pipe](./standard-library#separatedby-separator-trailing-none) instead of capturing separators into the list.

## Captures and grouping

A capture binds more tightly than sequence:

```yl
node Example = first: "a" "b"
```

Only `"a"` is captured. To capture the entire grammar value, group the sequence:

```yl
node Example = pair: ("a" "b")
```

`pair` receives a tuple containing both literal values. Concrete nodes omit uncaptured grammar values from their AST fields.

## Pipe application

A pipe applies to the expression immediately to its left:

```yl
A | B |> transform()
A B |> transform()
field: A |> transform()
A* |> transform()
```

These group as:

```text
A | (B |> transform())
A (B |> transform())
field: (A |> transform())
(A*) |> transform()
```

To transform a complete sequence or choice, group it explicitly:

```yl
(A B) |> transform()
(A | B) |> transform()
```

Chained pipes associate left to right:

```yl
A |> first() |> second()
```

`second` receives the transformed grammar produced by `first`. Every pipe preserves the input value type; see [Pipes and structural rewrite](./pipes).

## References followed by groups

A node reference or bound grammar parameter followed by parentheses is a sequence, not a callable invocation:

```yl
Name ("=" default: Expression)?
item (separator item)*
```

The compiler distinguishes these forms from calls to parameterized patterns by resolving the declaration category. Whitespace does not alter that interpretation.

Postfix operators and pipes following the parentheses apply to the group. A capture before the leading reference captures that reference:

```yl
name: Name ("=" default: Expression)?
```

To capture or transform both parts, parenthesize the entire sequence.
