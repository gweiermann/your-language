# Structural types and precise evidence

The compound-types example implements tuples, lists, structural interfaces, explicit generic arguments, and recursive compatibility checks in YL. Rust supplies generic query evaluation, source identities, and diagnostics. The language definition owns the type representation and compatibility policy.

This is an experimental language, with its complete implementation in `examples/compound-types/definition/`. It is intended to make the design inspectable rather than to implement TypeScript completely.

Declared write contracts are separate from inferred initializer evidence. In
`let value: number | string = 1; value = "ok";`, the annotation permits either
type on writes while the original initializer still contributes only number
evidence. The contract also propagates through member and list-index targets.
Assignments are checked without updating inferred read types or performing flow
analysis.

## Run the example

From the repository root:

```sh
cargo run -- compile examples/compound-types/definition/language.yl -o compound.ylc
cargo run -- language compound.ylc check examples/compound-types/programs/valid.txt
cargo run -- language compound.ylc check examples/compound-types/programs/invalid-scattered.txt
cargo run -- language compound.ylc check examples/compound-types/programs/invalid-scattered.txt --json
```

The valid program produces no diagnostics. The invalid program exits unsuccessfully and reports a nested mismatch at `payload.details.record.value`. The primary underline identifies the original `"wrong"` literal, even though the value passed through several declarations before the final check. The JSON form contains one diagnostic with an `annotations` list.

The valid program includes:

```typescript
interface User {
    name: string;
    scores: number[];
}

let user: User = { name: "Ada", scores: [10, 20] };
let name: string = user.name;
let score: number = user.scores[0];
let pair: [number, string] = (42, "answer");
let mixed: (number | string)[] = [42, "answer"];

user.name = "Grace";
```

Interfaces are compile-time structural declarations. They do not introduce runtime constructors.

## Define the data, then the policy

`typing.yl` defines a language-owned `Type` enum and ordinary constructors. All descriptors use `(tag, payload)`:

```yl
export operation numberType() = (Type::number, [])
export operation listType(element) = (Type::list, element)
export operation tupleType(elements) = (Type::tuple, elements)
export operation structType(fields) = (Type::struct, fields)
```

For example, a record containing a list is:

```yl
structType([
    ("name", stringType()),
    ("scores", listType(numberType()))
])
```

These are ordinary tuples and lists. The core engine has no knowledge of a `User`, an interface, or a list type. The example chooses structural assignment with extra fields permitted, exact field sets for identity, invariant mutable lists, and a permissive unknown type. Other languages can choose different rules.

## Keep evidence beside each type

A descriptor alone cannot tell a diagnostic where a number came from. The example therefore carries `(descriptor, sourceSyntax, children)`:

```yl
export operation evidence(value, origin, children = []) =
    (value, origin, children)

export operation annotateWithType(target, value, origin = target, children = []) {
    contribute((target, typeEvidence), evidence(value, origin, children))
}
```

An absent target returns `absent`; a present syntax identity with no type evidence returns the unknown descriptor. Each contribution retains its own origin. `evidenceOf(target)` reads those contributions with `facts`, combines their descriptors, and retains the individual evidence for union members. Equal type descriptors can originate at different source locations; there is no global source location attached to `number`.

Container constructors also retain the association between each child and its evidence. `structEvidence` keeps field-name/evidence pairs; `listEvidence` and `tupleEvidence` keep element evidence in source order. Without that association, a later comparison could underline the container but could not identify its incompatible child.

Literal semantics connect real captures to this representation:

```yl
node Number = value: /[0-9]+/ {
    semantics {
        annotateWithType(self, numberType(), value)
    }
}
```

Reference semantics copy the declaration's evidence rather than replacing its origin with the reference's span. Object construction carries that evidence into its field children. This is how an error discovered much later can still identify the original literal.

## Compare recursively without emitting effects

`compareTypes` is a pure operation returning a list of differences. Its recursive helpers inspect tags, fields, tuple positions, list element types, and union members. Each difference contains:

```yl
(path, actualDescriptor, expectedDescriptor, reason, offendingSyntax)
```

An empty difference list means success. `isAssignableTo` and `isIdenticalTo` are convenience predicates over the same worker. The full helper definitions are in `typing.yl`; comparison is not delegated to a native type checker.

Ordinary pure operations support recursion and mutual recursion. A value-producing `match` evaluates only the selected arm:

```yl
operation substituteType(value, substitutions) = match (value[0]) {
    Type::list => listType(substituteType(value[1], substitutions)),
    Type::struct => structType(map(value[1], field => (
        field[0], substituteType(field[1], substitutions)
    ))),
    _ => value
}
```

This excerpt illustrates traversal; the complete implementation also handles parameters, tuples, and unions. Recursion belongs to pure computations. Contributions, diagnostics, and native effects cannot be hidden inside a recursive cycle. Repeated calls are memoized within an evaluation, while dependency reads and pending results remain tracked. Cycles that revisit the same arguments and resource limits produce structured diagnostics.

## Instantiate explicit generic parameters

An interface parameter is represented by `(Type::parameter, declarationIdentity)`. A template stores its parameter identities and body descriptor. `instantiateType` checks argument count and recursively substitutes the supplied descriptors throughout the body.

```typescript
interface Box<T> {
    value: T;
}

interface Envelope<T> {
    payload: {
        details: {
            record: Box<T>;
        };
    };
}
```

`Envelope<number>` substitutes `number` for its own parameter, including inside the nested `Box<T>` application. The original template is unchanged. Two different interface declarations can both spell a parameter `T` because declaration identities distinguish them.

The example requires explicit generic arguments. It does not infer missing arguments or support recursive named type declarations.

## Report a single diagnostic group

Formatting a difference returns text. Emitting the diagnostic happens outside the pure comparison:

```yl
error(
    describeDifference(problem),
    [
        (problem[4], "incompatible value originates here"),
        (initializer, "checked against this requirement")
    ]
)
```

Each pair supplies a real syntax identity and an optional label. `absent` omits the label while retaining the underline. One call creates one diagnostic group. The CLI renders the related ranges together; the LSP publishes linked diagnostics so each supplied range receives an editor squiggle.

The example reports the first difference while retaining all differences as data. Complete expected-type derivation chains are deferred: the engine does not invent a history explaining every generic substitution.

## Current boundaries

The implementation handles finite acyclic descriptors and concrete object/list construction. Member lookup and assignment checks are authored in `expression.yl`. Assignments validate compatibility; they do not execute a program or perform flow-sensitive mutation analysis. List indexing yields the list element descriptor and does not prove index bounds. Prototype lookup, `push`, flow narrowing, inferred generic arguments, and recursive named types remain separate work.

Syntax captures can be projected by their declared paths, such as `fields.name`. Callback variables currently use positional tuples for projected data: `zip(fields.name, fields.value)` keeps names and values associated without requiring a new native field helper.
