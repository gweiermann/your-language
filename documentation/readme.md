# Your Language — syntax specification

> **Status:** syntax-v0 design checkpoint. This document describes the syntax/parser layer that should be implemented before semantic relations, scopes, binding, and analysis.

See [target-syntax](./target-syntax) for the MiniJS validation language.

## Modules

Declarations are private to their module by default.

```yl
export node Name =
    value: /[_a-zA-Z][_a-zA-Z0-9]*/
```

```yl
import { Name } from "./name"
import { Call as FunctionCall } from "./call"
```

Only explicitly exported declarations can be imported. Export does not propagate through nesting or membership.

If `Call` is exported but `Expression` is not, another module may import `Call`, but not `Expression`.

YL has no statement semicolons. Construct boundaries are determined by the grammar of YL itself.

Language keywords are contextual where possible, so names such as `node` may still be used as captures:

```yl
node Example =
    node: Name
```

## Concrete nodes

A concrete `node` has AST identity.

```yl
node VariableDeclaration =
    keyword("let")
    name: Name
    "="
    initializer: Expression
```

Only named captures become fields of the resulting AST node.

For example, `keyword("let")` and `"="` are consumed but do not become fields.

## Abstract nodes

A node declared with a body and nested node members is abstract:

```yl
export node Expression {
    node Number =
        value: /\d+/

    node NameExpression =
        name: Name
}
```

An abstract node does not add an AST wrapper. Parsing `Expression` returns the concrete member that matched.

Nested declarations use qualified names:

```text
Expression::Number
Expression::NameExpression
```

### Existing nodes as members

An already declared or imported node can be added to an abstract node without redefining it:

```yl
import { Call } from "./call"

export node Expression {
    node Call
}
```

The same underlying node can then be addressed as both:

```text
Call
Expression::Call
```

The qualified name is a membership alias, not a copy.

A concrete or abstract node may belong to more than one abstract node.

If an imported abstract node is added:

```yl
import { Arithmetic } from "./arithmetic"

export node Expression {
    node Arithmetic
}
```

then its members are transitively members of `Expression`, and qualified paths such as `Expression::Arithmetic::Sum` are available.

Member names inside an abstract node must be unique. Conflicts are compile errors.

## Patterns

A `pattern` is reusable grammar without AST identity.

```yl
pattern Parameters =
    Parameter*
    |> separatedBy(",", trailing=.optional)
```

Using it through a capture:

```yl
parameters: Parameters
```

captures the value produced by the pattern directly. No `Parameters` AST wrapper is created.

Patterns may take parameters:

```yl
pattern wrapped(value) =
    "(" value ")"
```

Parameters are untyped by default.

They may optionally restrict the accepted grammar result type:

```yl
pattern wrappedExpression(value: Expression) =
    "(" value ")"
```

Then `Expression` and its subtypes are accepted, while unrelated grammar result types are rejected.

## Grammar values

Grammar expressions have values independent of AST construction.

Conceptually:

```text
Node             -> that AST node
AbstractNode     -> the concrete member node that matched
Pattern          -> the value of its underlying grammar
A?               -> Option<A>
A*               -> List<A>
A+               -> List<A>
A | B            -> union of the branch result types
```

A sequence returns:
- no value if none of its elements produce a value,
- the single value directly if exactly one element produces a value,
- a tuple if multiple elements produce values.

A capture does not change the value of its underlying expression. Inside a concrete node, it additionally stores that value under the capture name.

Concrete node AST construction ignores all uncaptured grammar values.

## Grammar operators

The grammar-expression precedence from strongest to weakest is:

```text
(...)
? * +
|>
capture :
sequence
|
```

Examples:

```yl
A | B |> transform()
```

means:

```text
A | (B |> transform())
```

```yl
A B |> transform()
```

means:

```text
A (B |> transform())
```

```yl
value: A |> transform()
```

means:

```text
value: (A |> transform())
```

```yl
A* |> transform()
```

means:

```text
(A*) |> transform()
```

To transform a larger grammar fragment, use parentheses explicitly:

```yl
(A | B) |> transform()
(A B) |> transform()
```

Pipes chain left-to-right:

```yl
A |> first() |> second()
```

is equivalent to:

```text
(A |> first()) |> second()
```

## Pipes and structural rewrite

A `pipe` performs a compile-time structural rewrite of a grammar expression.

```yl
pipe wrapped(open, close) {
    rewrite pattern =>
        open pattern close
}
```

Rewrite patterns can destructure grammar structure:

```yl
pipe separatedBy(
    separator,
    trailing: Trailing = .none
) {
    rewrite item* {
        .none =>
            (item (separator item)*)?

        .optional =>
            (item (separator item)* separator?)?

        .required =>
            (item (separator item)* separator)?
    }

    rewrite item+ {
        .none =>
            item (separator item)*

        .optional =>
            item (separator item)* separator?

        .required =>
            item (separator item)* separator
    }
}
```

A pipe receives the structured grammar expression on its left, not source text.

A rewrite arm may also restrict the result type it accepts:

```yl
rewrite value: Expression =>
    ...
```

Pipes may preserve or transform the result type of the grammar expression they rewrite.

## Enums and arguments

```yl
enum Trailing {
    none
    optional
    required
}
```

Contextual enum syntax:

```yl
.optional
```

Explicit syntax:

```yl
Trailing::optional
```

Parameters use `:` for types and `=` for defaults:

```yl
pipe separatedBy(
    separator,
    trailing: Trailing = .none
) {
    ...
}
```

Named call arguments use `=`:

```yl
separatedBy(",", trailing=.optional)
```

## Precedence inside abstract nodes

Recursive expression grammars remain ordinary abstract nodes.

```yl
node Expression {
    node Number =
        value: NumberLiteral

    node Group =
        "(" value: Expression ")"

    node Member =
        object: Expression
        "."
        property: Name

    node Call =
        callee: Expression
        "(" arguments: Expression* |> separatedBy(",") ")"

    node Unary =
        operator: ("+" | "-" | "!")
        argument: Expression

    node Power =
        left: Expression
        operator: "**"
        right: Expression

    node Product =
        left: Expression
        operator: ("*" | "/")
        right: Expression

    node Sum =
        left: Expression
        operator: ("+" | "-")
        right: Expression

    precedence {
        Member, Call >
        Unary >
        right Power >
        Product >
        Sum
    }
}
```

Within `precedence`:
- `,` means equal precedence,
- `>` means the left level binds tighter than the right level,
- binary recursive nodes are left-associative by default,
- `right X` makes the level right-associative,
- `nonassoc X` may be used for a level that must not chain.

The parser infers the structural role from recursive references:
- recursion on the left edge behaves like postfix/left-recursive syntax,
- recursion on the right edge behaves like prefix/right-recursive syntax,
- recursion on both edges behaves like binary/infix syntax,
- recursive references surrounded by other grammar, such as `"(" Expression ")"`, remain ordinary base alternatives.

Whitespace has no semantic meaning inside the `precedence` declaration. The canonical multiline format keeps `>` at the end of the preceding line.

## Trivia

Trivia is declared with `trivia` and is automatically skipped between ordinary grammar elements.

```yl
trivia Whitespace =
    /[ \t\r\n]+/
```

Trivia can use normal grammar, not only regexes.

Trivia can also be abstract:

```yl
trivia Comment {
    trivia Line =
        /\/\/[^\n]*/

    trivia Block =
        /\/\*[\s\S]*?\*\//
}
```

`Comment`, `Comment::Line`, and `Comment::Block` can be addressed from syntax-level constraints.

Trivia is not automatically skipped while the trivia parser itself is matching.

## Constraints

Syntax-local validation belongs inside `constraints`.

```yl
constraint tight(left, right) {
    when trivia.between(left, right) {
        error("Trivia is not allowed here")
    }
}
```

```yl
node Update =
    operator: ("++" | "--")
    argument: Name
{
    constraints {
        tight(operator, argument)

        when someLocalCondition {
            warning("...")
        }
    }
}
```

`when` is only valid inside a `constraint` declaration or a `constraints { ... }` block.

Syntax-v0 constraints may inspect current captures, source spans, and trivia. Scope/relation queries belong to the later semantic layer.

## Extending nodes across files

A node has one canonical grammar definition.

Other modules may add syntax-local constraints or metadata:

```yl
import { Name } from "./name"

extend node Name {
    constraints {
        when value.matches(/^\d/) {
            error("A name cannot start with a number")
        }
    }
}
```

The syntax-v0 `extend node` form does not modify the grammar itself.

Additive sections merge. Duplicate singular/named definitions are compile errors. Import order must not change language meaning.

## Core and standard libraries

Callable functionality is imported explicitly, including compiler/runtime primitives.

```yl
import { notAhead, notBehind } from "core/parser"
```

`core` contains primitives that cannot be implemented using ordinary YL.

`std` contains helpers implemented in YL on top of core primitives.

For example, a boundary helper can be implemented in the standard library, and a language can define its own keyword policy:

```yl
import { boundedBy } from "std/parser"

pattern IdentifierPart =
    /[_a-zA-Z0-9]/

pattern keyword(value) =
    value |> boundedBy(IdentifierPart)
```

Then:

```yl
keyword("let")
keyword("function")
```

requires identifier boundaries on both sides without making keywords a compiler special case.

## Entrypoint

A language has one entrypoint:

```yl
entry Program
```

The runtime may expose lower-level rules separately for tooling, but normal parsing begins at the declared entry.

## Forward references

Declarations are resolved as a module graph, not strictly in source order. A grammar may refer to a declaration defined later in the same resolved language definition.

## Syntax-v0 boundary

The first implementation covers the complete syntax-to-AST path:
- parsing YL itself,
- module resolution,
- grammar validation,
- pipes and rewrite,
- trivia,
- precedence,
- syntax-local constraints,
- compiling/loading a reusable language definition,
- parsing source text,
- AST construction,
- structured parse diagnostics.

The semantic layer is intentionally outside this checkpoint:
- relations,
- traits,
- scopes,
- declarations/references,
- name resolution,
- semantic constraints depending on those systems,
- semantic graph generation.

Those features are designed and implemented after the syntax runtime is validated end-to-end.
