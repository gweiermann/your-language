# Mini TypeScript inference

This example defines a small expression language with type annotations, inferred bindings, functions, aliases, higher-order parameters and addition. Its reference and type rules are YL operations. Rust loads the definition, runs the generic query evaluator and prints the result; there is no Mini TypeScript native adapter.

```sh
cargo run --example mini_typescript -- program
cargo run --example mini_typescript -- generics
cargo run --example mini_typescript -- higher-order
cargo run --example mini_typescript -- addition
cargo run --example mini_typescript -- errors
cargo run --example mini_typescript -- errors --json
```

The same definition works through the standard CLI and serialized artifact:

```sh
cargo run -- compile examples/08-mini-typescript/definition/language.yl -o target/mini-typescript.ylc
cargo run -- language target/mini-typescript.ylc check examples/08-mini-typescript/programs/generics.txt
cargo run -- language target/mini-typescript.ylc check examples/08-mini-typescript/programs/errors.txt --json
```

Successful analysis prints inferred declaration types. Error programs print diagnostics with source underlines and exit 1. `--json` emits the AST, original syntax occurrences, query contributions and structured diagnostics. Integration tests compile the definitions to `.ylc`, reload the artifact and compare the resulting analysis.

## Definition modules

Definitions live in `definition/`; inputs live in `programs/`.

- `language.yl` selects the entry, trivia and statement grammar.
- `lexical.yl` defines identifiers and whitespace.
- `types.yl` declares this language's `number`, `string` and `boolean` symbols, type syntax and function-signature slots.
- `typing.yl` composes the standard reference and type operations into language-specific rules.
- `parameter.yl`, `function.yl`, `expression.yl` and `let.yl` attach those rules to syntax.

`std/references` resolves names through scope identifiers attached to syntax ancestors. `std/types` accumulates candidate facts, checks restrictions and resolves hypothetical inputs. Both are authored standard YL modules over the generic `core/query` primitives. Concrete target-language types belong to this example; only `unknown` belongs to the standard type module.

## Syntax and reference types

A number publishes its type directly:

```yl
node Number = value: /[0-9]+/ {
    semantics {
        annotateWithType(self, number)
    }
}
```

A reference resolves its name and shares the resolved declaration's type. A let declaration transfers initializer evidence into that binding and checks an explicit annotation without retyping the initializer:

```yl
export operation bindingType(name, value, annotation = absent) {
    annotateWithType(referenceOf(name), typeOf(value))
    when isPresent(annotation) {
        annotateWithType(referenceOf(name), typeOf(annotation))
        restrictTypeTo(referenceOf(name), typeOf(annotation), value, annotation.value)
    }
}
```

For `let count: number = "wrong";`, the binding retains both candidate types, while the diagnostic points to `"wrong"` and relates the exact `number` annotation.

Reference visibility follows the declared meaning order. A let initializer cannot use its own declaration or a later let declaration. Program precedence registers functions before lets and analyzes function bodies afterward, so a hoisted function can read an outer let that is initialized before the deferred body analysis.

## Function identity and hypothetical calls

A function's type value is its resolved binding identity. Signature metadata lives at keys combining that identity with language-owned slot symbols. Copying a function into another binding copies the type value, so aliases and higher-order parameters retain the original signature.

Each parameter has its own `(functionIdentity, parameters, parameterName)` type key. The `parameters` slot also presents the ordered signature for inspection. Calling a function asks what its return slot would contain under temporary argument types:

```yl
resolveTypeWith(
    (typeOf(callee), returnType),
    [((typeOf(callee), parameters, "value"), typeOf(argument), argument)]
)
```

The optional third assumption member preserves the argument source for diagnostics. Each calculation is isolated. `identity(42)` produces `number`; `identity("hello")` produces `string`; the declaration's unannotated signature remains `(unknown) => unknown`. Captured outer values stay shared. An explicit return annotation anchors the signature; calls read that declared result, and the body is checked independently against it. A function body stores return information at `(self, returnType)`, rather than making the block itself an expression type.

## Language-specific addition

The language chooses supported operand combinations:

```yl
import { error } from "core/diagnostics"

match (typeOf(left), typeOf(right)) {
    string, string | string, number | number, string => annotateWithType(self, string),
    number, number                                  => annotateWithType(self, number),
    unknown, _ | _, unknown                         => annotateWithType(self, unknown),
    _                                               => error("${left} + ${right} is not possible")
}
```

Candidate unions distribute through the cases. Supported combinations contribute their result types; unsupported combinations emit diagnostics. The standard type library contains no addition policy.

This is an inference example, not a TypeScript implementation. Control-flow narrowing, structural subtyping, overload resolution and target execution remain separate concerns.

## Run this example

```powershell
cargo run --example 08-mini-typescript
# Equivalent friendly name:
cargo run --example mini-typescript
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
