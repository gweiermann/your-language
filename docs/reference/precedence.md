# Recursive operator precedence

Expression languages often contain recursive prefix, postfix, and infix forms. Declare them as ordinary members of an abstract node and use a `precedence` block to specify their binding relationships.

## An expression family

```yl
node Expression {
    node Number = value: /[0-9]+/

    node Group = "(" value: Expression ")"

    node Unary =
        operator: ("+" | "-" | "!")
        argument: Expression

    node Power =
        left: Expression operator: "**" right: Expression

    node Product =
        left: Expression operator: ("*" | "/") right: Expression

    node Sum =
        left: Expression operator: ("+" | "-") right: Expression

    precedence {
        Unary >
        right Power >
        Product >
        Sum
    }
}

entry Expression
```

The precedence declaration is local to `Expression`. Each named member retains its normal concrete AST identity and captures.

## Binding strength

`>` separates levels, with the left level binding more tightly than the right. In the example, `Product` binds more tightly than `Sum`, so `1+2*3` forms a sum whose right child is a product.

Commas put members at the same level:

```yl
precedence {
    Member, Call >
    Unary >
    Product >
    Sum
}
```

Whitespace and line breaks do not determine precedence. Keep `>` at the end of the preceding line for a consistent multiline layout.

## Associativity

Binary recursive levels are left-associative by default:

```yl
precedence { Product > Sum }
```

For a subtraction member at the `Sum` level, `a-b-c` groups as `(a-b)-c`.

Prefix a level with `right` for right associativity:

```yl
precedence { right Power > Product > Sum }
```

`a**b**c` groups as `a**(b**c)`.

Use `nonassoc` for operators that must not chain at the same level:

```yl
precedence { Product > Sum > nonassoc Comparison }
```

A comparison member at that level accepts a single comparison; chaining it without explicit grouping produces a parse diagnostic.

## Structural roles

The compiler infers an operator's role from recursive references to the enclosing family:

| Recursive position | Role |
| --- | --- |
| Left edge | Postfix or left-growing form |
| Right edge | Prefix or right-growing form |
| Both edges | Infix form |
| Surrounded by other grammar | Ordinary base alternative |

The distinction is structural, not based on node names or operator spelling.

```yl
node Member = object: Expression "." property: Name
node Call = callee: Expression "(" arguments: Arguments ")"
node Unary = operator: "!" argument: Expression
node Sum = left: Expression "+" right: Expression
node Group = "(" value: Expression ")"
```

`Member` and `Call` grow an already parsed left expression. `Unary` parses its recursive argument according to its prefix binding level. `Sum` combines a left expression with a right expression. `Group` is a base alternative because its recursion is enclosed by delimiters.

## AST effects

Precedence changes tree shape, not the fields or identity of a member:

```text
1 + 2 * 3

Sum
  left: Number(1)
  right: Product
    left: Number(2)
    right: Number(3)
```

The abstract `Expression` node adds no wrapper. Explicit parentheses can alter the grouping by selecting the `Group` alternative, which returns its own concrete node when declared as above.

## Grammar precedence versus expression precedence

The `precedence` block controls target-language operators. YL's `?`, `*`, `+`, `|>`, capture, sequence, and choice operators have their own fixed [grammar precedence](./grammar-operators#operator-precedence).

When debugging an expression grammar, verify both: first that YL groups the member's grammar as intended, then that the family's precedence block gives the target operators the intended relationships.
