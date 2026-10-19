# Type models and inference

A type system is authored language policy. The engine knows how to query values;
it does not decide whether `string + number` is valid or which fields an interface requires.

## Generic annotations

`std/types` provides a type channel, distinct candidate contributions, compatibility
checks, and isolated hypothetical assignments. Your language declares its own
primitive symbols or enum variants and chooses operator rules.
`annotateWithType(key, value)` adds possibilities. `restrictTypeTo` checks them;
it does not add the expected type or change the initializer's type.

A function signature can use keys such as `(functionIdentity, returnType)` and
`(functionIdentity, parameters, parameterName)`. `resolveTypeWith` asks for a
result under temporary explicit parameter types while retaining outer captures.
Separate calls do not permanently specialize the original declaration.

## Structural policy

The opt-in `std/types/structural` library supplies a different, complete descriptor
model: primitives, unions, tuples, lists, interfaces, and generic parameters. Its
annotation channel is separate from `std/types`; use matching annotation/read
operations or deliberately bridge the models in your language.

Structural assignment checks required fields; exact comparison also rejects
extra fields. Mutable list elements are invariant. Explicit generic substitution
preserves repeated parameter identity. Optional source-origin helpers can make a
nested mismatch point at the original literal rather than only its final use.

## Limits

An inferred type is not flow-sensitive knowledge about each program point. Literal
types, branch-local narrowing, assignment flow, loop analysis, and optimization
rules are future work. Mini TypeScript demonstrates useful inference and explicit
annotations without claiming TypeScript's entire type system.

Follow [the type chapter](/tutorials/course/05-types),
[function inference](/tutorials/course/06-functions), and
[compound data](/tutorials/course/07-compound-types). The
[standard-library reference](/reference/standard-library) defines the reusable operations.
