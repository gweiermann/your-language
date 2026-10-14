# Meaning libraries

Meaning operations connect syntax occurrences to analysis libraries. Import actions and queries explicitly; native contracts are validated during definition compilation and artifact loading. `SemanticEngine::default()` registers `std/variables` and `std/types`. The CLI and language server can validate these contracts without executing meanings.

## Bindings and scopes

```yl
import * as variables from "std/variables"

node Reference = name: Name {
    meanings {
        variables::use(name)
    }
}

node Declaration = "let" name: Name "=" value: Expression ";" {
    meanings {
        variables::declare(name)
    }
}
```

`Name` and `Expression` are declarations supplied by the surrounding language. The library has no dependency on those names.

| Export | Contract |
| --- | --- |
| `lexicalScope()` | Creates and activates a lexical scope for enclosed semantic work. |
| `declare(name, reassignable=true)` | Registers a named binding in the active scope. |
| `use(name)` | Resolves a name in the nearest enclosing scope. |
| `identity()` | Returns the current binding identity when available, otherwise the occurrence identity. |
| `identityOf(value)` | Returns one captured occurrence's identity; multiple roots are an error. |
| `identities(value)` | Returns repeated capture identities in order. |
| `childIdentity()` | Returns the first retained child's identity for explicit inheritance. |

Declarations and resolved references share an identity. Functions are ordinary bindings; the library does not require variable/function categories. Same-scope duplicate declarations and unresolved references produce diagnostics. Shadowing uses the nearest scope. `reassignable` is metadata, not assignment checking.

A lexical scope and an ordering `boundary` are independent. The former provides context; the latter stops an enclosing precedence search below the marked occurrence. A function's parameter-and-body pattern can create the scope while its body alone establishes the deferred boundary. See [native semantic ordering](/api/native-semantics#explicit-boundaries).

## Identity-keyed types

```yl
import * as types from "std/types"
import * as variables from "std/variables"

export symbol number
export symbol returnType

export operation annotateWithType(value) {
    types::ensure(variables::identity(), value)
}

export operation typeof(value) = types::typeof(variables::identityOf(value))
```

| Export | Contract |
| --- | --- |
| `ensure(key, value, requirement=false)` | Equates the type at a key with a symbol, deferred type value or type list. |
| `typeof(key)` | Queries a deferred type value. A list of keys queries the corresponding ordered type list. |
| `unknown` | The library's settled, permissive unknown marker. |

A key is an analysis-local identity or `(identity, slotSymbol)`. The language authors every primitive and slot symbol except `unknown`. For example, a function return slot uses `(functionIdentity, returnType)`; the library has no special function or return-type names.

`ensure(key, absent)` contributes no constraint. Explicit `unknown` contributes no incompatibility against a known type and does not erase known information. Pending values remain distinct from settled unknowns. The engine propagates constraints, settles remaining unconstrained variables, resumes deferred matches and publishes final type records automatically; no authored `solve()` call is needed.

The current kernel uses equality constraints, list arity checks and symbolic slot projections. It does **not** accumulate unions or implement directional assignability. The Mini TypeScript wrapper names `annotateWithType` and `restrictTypeTo` describe their intended roles, but both currently use equality. `requirement=true` changes diagnostic provenance only: the incompatible value is primary and its restricting annotation is related.

## Deferred matches

The language, rather than the native library, defines operator typing:

```yl
match (typeof(left), typeof(right)) {
    string, string | string, number | number, string => annotateWithType(string),
    number, number                                 => annotateWithType(number),
    unknown, _ | _, unknown                         => annotateWithType(unknown),
    _                                              => error("${left} + ${right} is not possible")
}
```

Cases wait while query values are pending. They execute after ordinary traversal during finalization. `=> {}` performs no action; an explicit unknown result requires a call such as `annotateWithType(unknown)`. Matches belong inside node/pattern meanings; action wrapper bodies currently contain calls, not matches.

## Authored wrappers

`operation name(parameters) { calls }` composes actions. `operation name(parameters) = query(...)` aliases a native query. Parameters allow named arguments and defaults. Expansion preserves definition-local imports and caller captures; recursive expansion is diagnosed, including in unused wrappers. The compiled representation contains expanded native calls rather than runtime wrapper declarations.

The [inference tutorial](/tutorials/semantic-inference) connects these operations to a complete example. [Native Rust operations](/api/native-semantics#registering-a-library) document how to register additional libraries.
