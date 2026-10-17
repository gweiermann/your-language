# Build type inference with std/types

`std/types` is an ordinary YL module built on attachment queries. It supplies type type contributions, compatibility checks, and hypothetical evaluation. Your language supplies the type symbols, the meaning of each operator, and the representation of function signatures. No native helper recognizes Mini TypeScript node names or primitive types.

This tutorial follows the checked-in [Mini TypeScript definition](https://github.com/gweiermann/your-language/tree/main/examples/08-mini-typescript/definition). It is an experimental language, not a complete implementation of TypeScript.

## Start with identities and type type contributions

Inside a node or pattern's `semantics` block, `self` identifies the current node or pattern occurrence. A capture identifies the captured syntax occurrence directly; neither requires a syntax-identity wrapper. Repeated captures are lists, and optional captures may be `absent`.

Declare the primitive types in your language, then annotateWithType literal occurrences:

```yl
import { annotateWithType } from "std/types"

enum Type {
    number
    string
    boolean
}

node Number = value: /[0-9]+/ {
    semantics {
        annotateWithType(self, Type::number)
    }
}
```

`annotateWithType(key, value)` records possible types through the standard library. Repeating the same type contribution has no effect; distinct contributions form alternatives. `types::typeOf(key)` reads those alternatives, or the library's `unknown` symbol for an existing occurrence with no type contribution. `types::typeOf(absent)` returns `absent`, so optional missing syntax does not contribute an unknown type. Pending queries are an evaluation state, not another type symbol. `unknown` does not erase known type contributions.

The standard library does not export `number`, `string`, `boolean`, or a function-type marker. These are language-owned values. Documentation uses descriptive direct exports consistently; import renaming is not required.

## Choose where information belongs

A reference identity identifies a particular declaration, including its lexical scope. A declaration and the references that resolve to it share that identity, even when an inner declaration shadows the same spelling. Syntax identities remain distinct for individual occurrences.

Use `referenceOf(name)` from `std/references` to address the shared binding. Associate a reference occurrence with the binding's type contributions through an authored operation:

```yl
import { annotateWithType, typeOf } from "std/types"
import { connectReference, referenceOf } from "std/references"

node Reference = name: Name {
    semantics {
        ref := connectReference(self, name)
        annotateWithType(self, typeOf(ref))
    }
}
```

Here `Name` is the language's imported name pattern. The complete example defines it in `lexical.yl`. `connectReference(self, name)` records the resolved declaration on the exact name capture and on the reference occurrence. The type library has no dependency on the reference library; the language connects their identities.

## Keep constraints separate from annotation

`restrictTypeTo(key, expected, target = key, related = absent)` checks compatibility and reports a diagnostic. It does not replace the type contributions at `key` or contribute `expected` automatically. The default compatibility policy accepts every known actual candidate only if it is among the expected candidates; `unknown` is permissive. A language can author a different policy using generic data queries.

An initializer contributes actual type contributions. An explicit binding annotation imposes a restriction on that type contributions:

```yl
operation bindingType(name, initializer, annotation) {
    annotateWithType(referenceOf(name), typeOf(initializer))

    restrictTypeTo(
        referenceOf(name),
        typeOf(annotation),
        initializer,
        annotation.value
    )
}
```

No presence guard is needed: an absent annotation ends only the dependent restriction, while initializer type contributions still contributes. In this example, `annotation` is an optional `Annotation` node whose `value` capture contains the type word. `initializer` is the expression following `=`. The third argument chooses the primary diagnostic location; the fourth supplies a related location, excluding the colon and whitespace.

For `let count: number = "wrong";`, the check reports incompatible type contributions and marks `"wrong"` as the primary location. An annotation expresses a requirement; it does not retroactively change a string expression into a number.

## Define operator rules in YL

The type library accepts the supplied occurrence or declaration identity directly. Operations have no implicit `self`; their callers pass it explicitly. Mini TypeScript's addition node then spells out its own type policy:

```yl
import { error } from "core/diagnostics"

match (typeOf(left), typeOf(right)) {
    Type::string, Type::string | Type::string, Type::number | Type::number, Type::string => annotateWithType(self, Type::string),
    Type::number, Type::number                                  => annotateWithType(self, Type::number),
    unknown, _ | _, unknown                         => annotateWithType(self, unknown),
    _                                               => error("${left} + ${right} is not possible")
}
```

`typeOf`, `annotateWithType`, and the type enum here are imports from the example's `typing.yl` and `types.yl`. The engine evaluates relevant combinations of alternatives. The selected case sees the selected operand candidates, so rereading a selected operand does not reintroduce unrelated alternatives. An unsupported known combination contributes a diagnostic; valid combinations still contribute their results.

There is no special native addition operation and no explicit `solve()` call. The query engine tracks dependencies and propagates type contributions. Guards and diagnostics wait for the information needed to determine their result.

## Address function information with tuple keys

A function's public signature belongs to its declaration identity. Extra channels use language-owned symbols:

```yl
export symbol parameters
export symbol returnType
```

For a parameter named `value`, Mini TypeScript uses these logical type keys:

```text
(functionIdentity, parameters, "value")
(functionIdentity, returnType)
```

The type library stores type contributions in its own channel, so the corresponding physical attachment keys also include `types::type`. Authored operations normally use the logical keys and let `std/types` add that channel.

The body describes its return separately from its own type:

```yl
annotateWithType((self, returnType), typeOf(result))
```

The declaration connects that relationship to its binding:

```yl
annotateWithType(
    (referenceOf(name), returnType),
    typeOf((contents.body, returnType))
)
```

This does not give the block itself a type. A language with expression-valued blocks could instead annotateWithType the block's `self` directly. Lists and tuples are ordinary compound values; a list is not automatically a union. Use `map(parameters, parameter => types::typeOf(parameter))` when a positional list of types is needed.

## Infer each call under temporary assumptions

For `identity(42)`, ask for the return under one explicit parameter assignment:

```yl
resolveTypeWith(
    (functionIdentity, returnType),
    [((functionIdentity, parameters, "value"), number, argument)]
)
```

`functionIdentity` comes from the resolved callee, and `argument` is the captured argument expression. Each assumption contains a logical key and its temporary type; the optional third component preserves the argument's source location for diagnostics. Real calls build that list from their parameter names and argument captures.

Only the listed keys are overridden. The engine recomputes dependent relationships in isolation. It leaves original parameter type contributions and captured outer types unchanged. Calling the same function with a string therefore does not turn the earlier number result into `number | string`.

```javascript
let outside = 10;

function identity(value) {
    return value;
}

function describe(value) {
    return identity(value) + outside;
}

let count: number = identity(42);

let title: string = identity("hello");

let numberResult = describe(count);

let stringResult = describe(title);
```

Here `count` and `numberResult` are numbers; `title` and `stringResult` are strings. `outside` remains a number in both calculations. Explicit parameter annotations still impose compatibility requirements. Explicit return annotations can anchor recursive calls; unresolved cycles produce diagnostics rather than guessed return types.

Hypothetical queries retain authored error and warning provenance. They do not replay arbitrary external Rust effects such as writing files. Resource limits bound recursive hypotheses and expanding compound values.

## Run and inspect the example

From the repository root:

```shell
cargo run --example mini_typescript_host -- program
cargo run --example mini_typescript_host -- addition
cargo run --example mini_typescript_host -- higher-order
cargo run --example mini_typescript_host -- errors
cargo run --example mini_typescript_host -- errors --json
```

The first three fixtures succeed and print inferred bindings. The `errors` fixture exits with status `1` and displays source diagnostics; `--json` returns the full analysis result and structured diagnostics instead. The `generics` fixture can be edited directly to compare valid and invalid annotations:

```shell
cargo run --example mini_typescript_host -- generics
```

The separate `generics-errors` fixture changes `let count: number` to `let count: string`:

```shell
cargo run --example mini_typescript_host -- generics-errors
cargo run --example mini_typescript_host -- generics-errors --json
```

It exits with status `1` and reports `Type Type::number does not match Type::string`, with `identity(42)` as the primary location and the type word `string` as the related location. The incompatible call is still a number, regardless of the separate string call to `identity`. An annotation is a restriction, not type contributions that changes the inferred result.

The definitions are divided into `types.yl`, `typing.yl`, `expression.yl`, `parameter.yl`, `function.yl`, and `let.yl`. Follow the source to change operator or binding policy rather than extending Rust with target-language rules.

For all operations, see [authored semantic libraries](/api/standard-semantics). For the underlying dependency and contribution model, see [declarative attachment queries](/api/declarative-queries).
