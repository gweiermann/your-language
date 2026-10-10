# Nodes and AST values

Nodes establish the structure of the generated abstract syntax tree. Concrete nodes introduce AST identity; abstract nodes group alternatives without introducing an extra wrapper.

## Concrete nodes

A concrete node has a grammar after `=`:

```yl
node Assignment =
    name: /[a-z]+/
    "="
    value: /[0-9]+/
```

Parsing `count=12` constructs an `Assignment` node with `name` and `value` fields. The `=` literal is consumed but is not stored as a field.

Every concrete AST node contains its declaration name, source span, and named fields. The exact JSON shape is described in the [AST API reference](/api/ast).

## Captures

The capture operator `name: expression` gives a grammar value a field name:

```yl
node Coordinates =
    "(" x: /[0-9]+/ "," y: /[0-9]+/ ")"
```

Only captured values become fields of a concrete node. Capturing a nested node stores that node; capturing a literal or regex stores text; capturing repetition stores a list.

```yl
node Name = text: /[a-z]+/
node Group = names: Name*
```

A capture does not change the grammar expression's value. It additionally stores the value when used to construct the enclosing concrete node. Duplicate captures that can occur together in a sequence are rejected. Alternative branches may use the same field name.

Capture precedence matters. `field: A B` captures `A`, while `field: (A B)` captures the sequence. See [Grammar operators](./grammar-operators).

## Abstract nodes

A node body containing member declarations defines an abstract family:

```yl
node Expression {
    node Number = value: /[0-9]+/
    node NameExpression = name: /[a-z]+/
}

node Statement = expression: Expression ";"
```

Parsing `Expression` returns the concrete member that matched. There is no intermediate `Expression` AST wrapper. A captured expression therefore holds a `Number` or `NameExpression` node.

Members are tried in declaration order when no recursive precedence plan applies. Put overlapping alternatives in an intentional order, and use explicit grouping or lookaround when a prefix overlap would otherwise choose an unintended alternative.

## Existing nodes as members

Add an existing node to a family without redefining its grammar:

```yl
node Call = callee: /[a-z]+/ "(" ")"

node Expression {
    node Call
    node Number = value: /[0-9]+/
}
```

`Call` and `Expression::Call` refer to the same node. Membership creates an alias, not a copy. A node can belong to several abstract families.

Abstract families can also become members of larger families:

```yl
node Arithmetic {
    node Number = value: /[0-9]+/
}

node Expression {
    node Arithmetic
    node NameExpression = name: /[a-z]+/
}
```

`Expression::Arithmetic::Number` resolves transitively to the original number node. Member names in each family must be unique.

## Grammar value rules

Grammar values exist independently of AST fields:

| Expression | Result |
| --- | --- |
| Concrete node | The constructed AST node |
| Abstract node | The concrete member that matched |
| Literal or regex | Matched text |
| Pattern | The value of its underlying grammar |
| `A?` | A value or absence |
| `A*` | A list containing zero or more values |
| `A+` | A list containing one or more values |
| `A \| B` | The value of the selected branch |
| Negative lookaround | No value |

A sequence with no value-producing elements produces no value. One value-producing element returns its value directly. Multiple value-producing elements produce a tuple. Tuples and lists are both represented as arrays in JSON, but their grammar meaning differs.

```yl
pattern Pair = /[a-z]+/ ":" /[a-z]+/
```

This pattern produces a three-element tuple, including the literal colon. Use a node with named captures when only selected components should become AST fields, or a [type-preserving pipe](./pipes) when matching additional structure around an existing value.

## Optional and repeated captures

```yl
node Parameter =
    name: /[a-z]+/
    ("=" default: /[0-9]+/)?
```

`default` may be absent. A capture inside an unmatched optional group may be missing from the field map; a capture of an optional expression serializes its absence as JSON `null`. Both are accessed as `absent` in [constraint conditions](./constraints#absence-and-optional-chaining).

For list fields, capture the repetition as a whole:

```yl
node Name = text: /[a-z]+/
node Names = values: Name*
```

`values` is an array, including `[]` when no names match. Parenthesized grammar lets a capture include a larger optional or repeated fragment.

## Extending a node

An extension adds constraints to an existing node while retaining its canonical grammar:

```yl
import { Name } from "./lexical"

extend node Name {
    constraints {
        when value.matches(/^[A-Z]/) {
            warning("Names conventionally start with a lowercase letter.")
        }
    }
}
```

Extensions cannot replace or append grammar. Their supported content is additive syntax-local constraints; node metadata is not part of the extension language. See [Constraints](./constraints#node-extensions).
