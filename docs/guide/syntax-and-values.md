# Syntax, identities, and values

A language definition describes several different kinds of information. Keeping
them distinct makes it possible to reuse parsing, analysis, and execution independently.

## Grammar values and the AST

A `node` gives matched syntax an AST identity. Captures name fields; a `pattern`
composes grammar without adding an AST node. Repetition produces a list, optional
syntax can be absent, and a pipe preserves its input's value type while rewriting
how that value is recognized. See [nodes](/reference/nodes),
[patterns](/reference/patterns), and [pipes](/reference/pipes).

## Syntax identities and declaration identities

Inside `semantics`, `self` is this occurrence. Captures retain their syntax
identity and source span. Two occurrences with identical source text are still
different occurrences. `std/references` connects a name use to a declaration;
that shared declaration identity is different from the individual use's syntax identity.

An addition result normally belongs to its syntax occurrence. A binding's type
can belong to its declaration identity, with reference occurrences inheriting it.
The language makes this connection explicitly rather than asking the type
library to know how names resolve.

## Semantic values and runtime values

Symbols, enum variants, tuples, lists, and syntax captures can be values in
analysis queries. A type descriptor is an ordinary value whose interpretation
belongs to an authored library. It is not the value of the target program at runtime.

Backend hooks receive unevaluated syntax. A host may produce numbers, JavaScript
AST nodes, or compiler handles without adopting the engine's semantic value model.
Continue with [queries](/guide/queries), [type models](/guide/type-models), and
[host hooks](/guide/host-hooks).
