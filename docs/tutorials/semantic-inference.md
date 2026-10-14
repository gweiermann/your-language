# Scopes and type inference

The Mini TypeScript example demonstrates syntax, lexical binding resolution and inferred types with language-authored meanings. It is a small experimental language, not a complete TypeScript implementation. Run these commands from the repository root:

```sh
cargo run --example mini_typescript -- addition
cargo run --example mini_typescript -- errors
cargo run --example mini_typescript -- higher-order
```

The first and third succeed; `errors` reports mismatches with source underlines and exits 1. Add `--json` to obtain the analysis result with AST, retained occurrences, records and diagnostics.

## Organize the definition

```text
examples/mini_typescript/
├── definition/
│   ├── language.yl    # entry, trivia, outer ordering
│   ├── lexical.yl     # names and whitespace
│   ├── types.yl       # authored symbols and annotation syntax
│   ├── typing.yl      # reusable action/query wrappers
│   ├── expression.yl # literals, references, calls and addition
│   ├── parameter.yl  # parameter bindings and annotations
│   ├── function.yl   # scope, parameter/return slots and body boundary
│   └── let.yl        # binding registration and initializer constraints
└── programs/         # successful and diagnostic fixtures
```

The Rust consumer compiles these modules with `SemanticEngine::default()` and analyzes a selected program in memory. `.ylc` serialization is optional; integration tests also reload artifacts and revalidate native contracts.

## Give literals meaning

`types.yl` declares language-owned symbols such as `number`, `string` and `boolean`. The native type library supplies only `unknown`. Literal nodes call an authored wrapper:

```yl
node Number = value: /[0-9]+/ {
    meanings {
        annotateWithType(number)
    }
}
```

`annotateWithType` expands to `types::ensure(variables::identity(), value)`. Each occurrence has an identity; a registered declaration and its resolved references share a binding identity. This makes type information independent of names and shadowing.

## Connect declarations and annotations

The declaration's `Register` group contains:

```yl
declare(name)
annotateWithType(typeof(initializer))
restrictTypeTo(annotation)
```

The initializer is the expression after `=`. The query wrapper obtains its identity and reads its deferred type. The optional annotation supplies a requirement; absence contributes nothing.

For `let count: number = "wrong";`, the primary underline covers `"wrong"`, and the related underline covers only `number`. The colon and surrounding spaces are excluded. `restrictTypeTo` marks diagnostic provenance with `requirement=true`.

These wrapper names do not imply union accumulation yet: both use an equality kernel. Multiple incompatible contributions report a conflict.

## Keep operator policy in the language

Addition's meaning matches deferred operand types:

```yl
match (typeof(left), typeof(right)) {
    string, string | string, number | number, string => annotateWithType(string),
    number, number                                 => annotateWithType(number),
    unknown, _ | _, unknown                         => annotateWithType(unknown),
    _                                              => error("${left} + ${right} is not possible")
}
```

There is no native addition helper or hidden primitive-name policy. Queries wait for constraints to settle before selecting a case. The engine drives this automatically without a visible solver operation.

The checked-in `addition` program includes:

```javascript
function sum(left: number, right: number): number {
    return left + right;
}

let numberSum: number = sum(20, 22);
let stringSum = "hello" + " world";
let mixedLeft = "value: " + 42;
let mixedRight = 42 + " is the answer";
let enabled: boolean = true;
```

Expected binding types include `numberSum: number`, `stringSum: string`, `mixedLeft: string`, `mixedRight: string` and `enabled: boolean`. The single recursive `Addition` member needs no explicit grammar-precedence block; left associativity is inferred.

## Describe function slots

The language declares `Function`, `parameters` and `returnType` symbols. Its `functionSignature` wrapper associates the function identity with the marker and creates ordered parameter and return slots:

```yl
types::ensure((variables::identity(), parameters), types::typeof(variables::identities(arguments)))
types::ensure((variables::identity(), returnType), typeof(body))
```

The parameter list has a distinct binding identity for each position. A call equates argument types with the parameter list and reads the return slot. Slot projections propagate through aliases and function-valued parameters. The library does not recognize those slot names or any grammar node names.

`FunctionContents` creates the lexical scope shared by parameters and body. `FunctionBody` establishes an ordering boundary. Outer precedence can defer the entire body until bindings are registered without searching its locals as outer declarations. This is static analysis ordering, not runtime hoisting or execution.

## Limits and further reading

Inference is monomorphic: all calls share one function signature. Calling an unannotated identity function with both a number and a string conflicts instead of instantiating a generic signature per call. Unions, directional assignability, type guards, literal/range narrowing and target execution are not implemented. Explicit `unknown` is permissive and does not erase known information.

Read [meaning library contracts](/reference/meaning-libraries) for the public operations and [native semantics](/api/native-semantics) for lifecycle, ordering and custom Rust registration. The complete definitions and programs are in the repository's `examples/mini_typescript/` directory.
