# Mini TypeScript inference experiment

This example explores whether language-authored meanings can express type inference using a small native constraint library. It supports primitive annotations, bindings, references, function signatures, calls, recursive inference and language-defined addition rules. It remains a monomorphic experiment, not a TypeScript implementation.

```sh
cargo run --example mini_typescript
cargo run --example mini_typescript -- addition
cargo run --example mini_typescript -- higher-order
cargo run --example mini_typescript -- addition-errors
cargo run --example mini_typescript -- addition --json
```

Successful runs print inferred binding types. Errors use source underlines and exit 1; `--json` returns the AST, retained syntax occurrences, semantic records and structured diagnostics. Tests also compile, serialize `.ylc`, reload and analyze concurrently with isolated state.

## Definition files

`definition/language.yl` selects the entry and trivia. `types.yl` declares the language's type and slot symbols and annotation grammar. `typing.yl` defines reusable semantic wrappers. The remaining grammar lives in `lexical.yl`, `parameter.yl`, `expression.yl`, `function.yl` and `let.yl`. Programs live in `programs/`.

## Two native type operations

`std/types` exports exactly:

- `ensure(key, type)`: equates the type associated with a key with a primitive symbol, deferred type, or list of types.
- `typeof(key)`: returns a deferred type handle. Constraint collectors can link the handle before its type is known; matches wait until it is resolved.
- `unknown`: the only type identity supplied by the library. The language authors all other identities.

There is no visible solver call or configuration operation. The engine drives propagation automatically, settles remaining unconstrained variables as `unknown`, resumes matches, and publishes the final records. Pending information and settled `unknown` remain distinct.

A key is an analysis-local identity, or `(identity, slotSymbol)`. A list of keys reads the corresponding list of types. `std/variables::identity()` selects the current declaration/reference identity when available, otherwise the current syntax occurrence. A declaration and its resolved references share one identity. `identity(value)` selects a captured occurrence; `identities(value)` selects a repeated capture's ordered identities. `childIdentity()` selects the first retained child for explicit inheritance.

A scalar identity query rejects captures containing multiple occurrence roots. It does not silently choose one. Captures preserve their original occurrence roots through pipes.

## Authored wrappers

The language defines conveniences in ordinary imported YL:

```yl
import * as types from "std/types"
import * as variables from "std/variables"

export operation annotate(value) {
    types::ensure(variables::identity(), value)
}

export operation typeof(value = absent) =
    types::typeof(variables::identity(value))

export operation inherit() {
    types::ensure(variables::identity(), types::typeof(variables::childIdentity()))
}
```

Action wrappers contain calls; query wrappers expand to a native query. Parameters support positional/named arguments and defaults. Expansion preserves definition-local imported symbols and caller captures. Recursive wrapper expansion is diagnosed. The normalized artifact contains native calls, not wrapper declarations. Namespace imports are optional; named imports and aliases remain available.

Literal nodes can now state their meaning locally:

```yl
export symbol number

node Number = /[0-9]+/ {
    meanings { annotate(number) }
}
```

## Functions are a language-defined slot protocol

`types.yl` authors `Function`, `parameters` and `returnType`. The library does not know those names or treat them specially. `functionSignature` in `typing.yl` supplies the protocol:

```yl
types::ensure(variables::identity(), Function)
types::ensure((variables::identity(), parameters),
    types::typeof(variables::identities(arguments)))
types::ensure((variables::identity(), returnType), typeof(body))
types::ensure((variables::identity(), returnType), typeof(annotation))
```

The authored call wrapper requires the same marker, equates the callee's parameter slot with its arguments, and equates the current expression with the callee's return slot. A function declaration links its binding to its contents' type. Generic projection-aware equality propagates authored slots through that link, through function aliases, and through function-valued parameters. Renaming the slot symbols preserves inference; reload tests cover this.

Absent optional annotations contribute no requirement: `typeof(absent)` returns absence, and `ensure(key, absent)` is a no-op. Explicit `unknown` is a separate, permissive type marker. It contributes no incompatibility against a known type and does not erase known information.

## Addition policy remains in YL

```yl
match (typeof(left), typeof(right)) {
    string, string | string, number | number, string => annotate(string),
    number, number => annotate(number),
    unknown, _ | _, unknown => annotate(unknown),
    _ => error("${left} + ${right} is not possible")
}
```

Changing this definition changes the language's accepted combinations. The native library contains no arithmetic or primitive-name rules. A `=> {}` branch performs no action; explicit `annotate(unknown)` publishes an unknown result.

## Native implementation and limits

The generic library uses equality constraints, variable equivalence classes, lists and symbolic slot projections. It checks list arity and cyclic type shapes. Failed unification rolls back its partial changes before reporting a diagnostic. Type queries preserve source origins so annotation errors underline the annotation.

Constraint collectors declare `finalization_only` in their native interface: their effects occur after ordinary traversal and retain their order relative to other collectors. They cannot provide live lifecycle contexts. Deferred handles are analysis-local; matches resolve them through their owning service. Query evaluation is retryable and publishes no external effects.

Final `std/types#typeValues` records retain identity/slot keys and qualified primitive symbol identities or type lists; display strings are separate conveniences.

The example's Rust consumer only selects standard libraries and formats its authored function-slot schema for presentation. The native type implementation does not walk a grammar-specific tree or recognize target node names.

Inference is monomorphic. Calling the same unannotated identity function with incompatible argument types reports a conflict. Generic instantiation, unions, directional assignability, guards, literal/range narrowing, ownership and target execution remain future experiments. Explicit `unknown` follows this prototype's permissive policy rather than TypeScript's full assignability rules.

The current wrapper syntax deliberately supports action call blocks and query aliases. Match statements inside wrapper definitions and constant-only query definitions are not yet supported; matches remain available in node/pattern meanings.
