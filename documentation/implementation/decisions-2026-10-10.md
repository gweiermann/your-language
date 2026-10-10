# Syntax-v0 decisions, 2026-10-10

Decided with the language author in the implementation conversation.

## Conditions

JavaScript-style `!`, `==`, `!=`, `&&`, `||` with parentheses; precedence in
that order, highest first. Equality compares matching scalar string/boolean/enum
types, with contextual enum variants. No general truthiness or coercion.
`value.matches(/regex/)` and `value.isPresent()` are syntax-local predicates.
`value?.matches(/regex/)` preserves absence as `absent`, distinct from false.
A `when` accepts boolean/optional boolean and emits only for true.
Negating absent preserves absent. `absent && rhs` returns absent without evaluating
rhs; `absent || rhs` evaluates and returns rhs. Other boolean operands follow
short-circuit rules. Unguarded method access on an optional capture is a definition
error; presence guards may prove access safe within a condition. Separate when
clauses remain independent; emitting a diagnostic does not establish presence.

## Structural rewrites

`rewrite value match first` selects one enum parameter; `match (first, second)`
selects a tuple. Cases match all selected arguments. Single-enum shorthand remains.
`_` is a wildcard. Matching is ordered, first matching case wins, and must be
exhaustive. Completely unreachable cases are definition errors.
Case blocks contain diagnostic statements followed by a replacement grammar.
An error rejects the pipe application and requires no replacement. A warning
continues with the replacement; `value` is the identity transformation.
Definition diagnostics point to the caller's actual selected enum arguments;
additional argument spans and the diagnostic definition are secondary locations.
Defaults point to the pipe call and are identified in the diagnostic.

Every pipe preserves input type and cardinality: scalar exactly one, optional
zero or one, star list zero or more, plus list one or more. List duplication
collects items in source order, omission returns an empty list. Optional omission
returns absent. Tuple/list-valued list items retain their element boundaries.
Incompatible usage is diagnosed at the pipe application.

## Extensions

Metadata is explicitly deferred beyond syntax-v0. Extensions add constraints;
they cannot replace a canonical grammar definition.

## Explicit entrypoint trivia

The author approved `entry Program { trivia Whitespace, Comment }`.
Trivia declarations define matchers; only the entry block activates automatic
skipping. An entry without a trivia section skips nothing automatically. Selected
trivia uses normal imports/exports and may name abstract families. Imported helper
modules and unselected trivia must not affect skipping. This replaces the original
specification's implicit activation of all trivia in the loaded module graph.
