# Your Language — syntax specification

> **Status:** syntax-v0 design checkpoint. This document describes the syntax/parser layer that should be implemented before semantic relations, scopes, binding, and analysis.

See [MiniJS example](../examples/mini-js/) for the MiniJS validation language.

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

Node references and grammar parameters followed by parentheses denote a reference
followed by grouped grammar, as in `Name ("=" default: Expression)?` and
`item (separator item)*`. Declaration resolution distinguishes these from calls to
parameterized patterns. Postfix operators and pipes apply to the group, while a
capture before the reference captures that reference. Parenthesize the whole
sequence to capture or transform the larger fragment. Whitespace has no role in
this distinction.

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

Pipes must preserve the result type of the grammar expression they rewrite.

For a scalar rewrite, the bound input contributes its original value; surrounding
grammar is matched but its values and captures are discarded. A wrapping pipe
therefore returns the wrapped value directly, without a tuple of delimiters.

For `rewrite item*` and `rewrite item+`, only values of matched `item` occurrences
are collected, in source order, into the original list type. Separators and other
wrapper grammar do not contribute values. An empty star list produces `[]`.
If an item itself produces a tuple or list, that value remains one list element.

This rule applies to all pipes, including chained pipes, and does not depend on the
pipe's name. It was clarified during syntax-v0 implementation: a pipe cannot change
the type of its output.

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

Trivia is declared with `trivia`. Declarations define matchers; they do not activate
automatic skipping. The entry block explicitly selects the trivia skipped between
ordinary grammar elements. Importing a helper or trivia declaration alone does
not change the language's trivia policy.

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

### Boolean conditions and absence

Condition operators use JavaScript-style spelling, without general truthiness or
implicit coercions. From strongest to weakest: parentheses, `!`, `==`/`!=`, `&&`,
`||`. Binary operators associate left. Equality compares scalar strings, booleans,
and variants of the same enum; `absent` can be compared with optional values.

```yl
when name.isPresent() && name.matches(/^[A-Z]/) {
    warning("Capitalized name")
}
when name?.matches(/^[A-Z]/) {
    warning("Capitalized name")
}
when name?.matches(/^[A-Z]/) == false {
    help("Present but not capitalized")
}
when name?.matches(/^[A-Z]/) == absent {
    help("Missing name")
}
```

`isPresent()` distinguishes absent captures from present values (including empty
strings/lists). `.matches` tests the captured string value; for node captures it tests the captured
source text. An ordinary method call
on a possibly absent capture is a definition error unless its presence is proved
by an earlier short-circuit guard. `?.matches` returns `absent` for an absent
receiver, otherwise a boolean. A `when` accepts boolean or optional boolean and
emits only for `true`; no `== true` is required.

`!absent` is `absent`. `false && rhs` and `absent && rhs` skip `rhs` and preserve
the left value; `true && rhs` returns `rhs`. `true || rhs` skips `rhs`, while
`false || rhs` and `absent || rhs` return `rhs`. Equality always returns a boolean.

Separate `when` clauses are independent and evaluated in source order. Emitting a
diagnostic does not establish presence or stop subsequent checks. Nested `when`
clauses run only if their enclosing conditions are true. Reusable constraints
accept positional/named arguments and defaults; declared node parameters require
captures of that node type or its members, and enum parameters resolve contextual
variants using their declared enum type.

## Extending nodes across files

A node has one canonical grammar definition.

Other modules may add syntax-local constraints. Metadata is deferred beyond syntax-v0:

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
entry Program {
    trivia Whitespace, Comment
}
```

The runtime may expose lower-level rules separately for tooling, but normal parsing begins at the declared entry.

Selected trivia must be accessible through normal module/import/export rules and
must name trivia declarations. Selecting an abstract trivia family activates its
members through the family's matcher; members need not be individually listed.
Imported trivia is not activated unless selected. Selection follows the written
list order; repeated references to the same matcher are idempotent.

`entry Program` (or an empty entry block) selects no automatic trivia. The node's
grammar and all required declarations are still resolved through ordinary imports.


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

## Explicit enum selection in structural rewrites

A rewrite can select one enum parameter or a tuple of enum parameters:

```yl
rewrite value match (first, second) {
    (.a, .a) => "(" value ")"
    (.a, .b) => { error("This combination is not allowed.") }
    _ => { warning("Pipe not applied.") value }
}
```

For one selector, use `rewrite value match first { ... }`. The documented
`rewrite value { .a => ... .b => ... }` shorthand remains valid with exactly
one enum parameter. Multiple enum parameters require explicit selection.

Cases are checked in source order and the first matching case wins. `_` matches
any value, either as a tuple component (`(.a, _)`) or the entire selector tuple.
Matching must be exhaustive. A completely shadowed case is a definition error.
Case blocks contain diagnostic statements followed by a replacement grammar.
An `error` rejects the pipe application and does not require a replacement;
`warning` or `help` continues with the replacement. Returning the bound grammar
unchanged is the identity transformation.

These diagnostics occur when compiling the language. Their primary location is
at the pipe caller's selected enum argument, with other selected arguments and
the diagnostic definition as secondary spans. Forwarded arguments preserve their
original caller spans. Defaulted arguments point to the application and are
identified in diagnostic help text.

Every pipe preserves its input type and cardinality. Required scalars must produce
exactly one original value, optional scalars zero or one, `*` lists zero or more,
and `+` lists one or more. List duplication collects original items in source
order, preserving tuple/list-valued element boundaries. Omitting an optional
input produces `absent`; omitting a star list produces an empty list. Omitting a
required scalar/nonempty list, or duplicating an optional scalar so that both can
produce values, is a definition error at the pipe application.
