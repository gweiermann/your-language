# Compound types

The [compound-types example](https://github.com/gweiermann/your-language/tree/main/examples/11-compound-types)
defines structural typing entirely in YL. It separates type correctness from the
optional source-origin tracking introduced in [diagnostic origins](/tutorials/diagnostic-origins).

## Describe a type with ordinary values

A language-owned enum identifies type constructors. Each descriptor is a tuple
containing a tag and its payload:

```yl
operation numberType() = (Type::number, [])
operation tupleType(elements) = (Type::tuple, elements)
operation listType(element) = (Type::list, element)
operation structType(fields) = (Type::struct, fields)
```

A struct payload is a list of `(fieldName, fieldType)` pairs. A tuple payload keeps
types in positional order. A list has one element type; mixed elements produce a
union. No source location is needed to compare these values.

## Connect syntax to descriptors

```yl
node List =
    "["
    items: Expression* |> separatedBy(",")
    "]"
{
    semantics {
        elementTypes := map(items, item => typeOf(item))
        annotateWithType(self, listType(unionType(elementTypes)))
    }
}
```

The local `annotateWithType` contributes descriptors to a type channel. `typeOf`
collects possible descriptors and constructs a union. These operations are written
in the example's `definition/typing.yl`; they implement this language's compound
representation rather than extending engine primitives.

## Compare recursively

`compareTypes(actual, expected, mode)` returns a list of differences. An empty list
means success. A difference contains `(path, actual, expected, reason)`.

The comparison walks corresponding struct fields, tuple elements, and union
members. Tuple lengths use `equal(length(actual), length(expected))`. Assignment
allows extra object fields; identity requires the same fields. Mutable lists are
invariant in this example. These are authored language policies.

The reporter supplies the checked syntax separately:

```yl
differences := compareTypes(typeOf(initializer), typeOf(annotation), Comparison::assignable)
when isPresent(differences[0]) {
    reportDifference(differences[0], initializer)
}
```

This produces a useful error at the checked expression without requiring an origin
tree throughout the type implementation.

## Explicit generic interfaces

```typescript
interface Box<T> {
    value: T;
}

let box: Box<number> = { value: 42 };
let value: number = box.value;
```

Type parameters use declaration identities, so unrelated parameters named `T` stay
separate. A generic descriptor contains its parameter identities and body type.
`instantiateType` substitutes each parameter recursively, after checking argument
count. Parameters and outer declarations remain unchanged.

## Check access and assignment

Member access looks up a named field in a struct descriptor. List access returns
the element descriptor. Unknown members and non-list indexing are diagnostics.
An explicit annotation establishes a write contract independently of the inferred
initializer type: `let x: number | string = 1` infers the initializer as number but
allows a later string assignment.

## Run the example

```powershell
cargo run -- project check examples/11-compound-types
cargo run -- project check examples/11-compound-types/programs/invalid-assignment.txt
```

The first command verifies all fixtures, including expected errors. The second
shows one human-readable diagnostic and exits with status 1. Add `--json` for
structured diagnostics, or use `project ast <source>` for JSON AST output.

Recursive named types, inferred generic arguments, tuple indexing, flow-sensitive
mutation, list bounds checks, and `push` are outside this example.

## Reuse the policy in a larger language

After studying the local implementation, import `std/types/structural` to reuse
its descriptor constructors, type channel, comparison, generic substitution, and
renderers. It is an explicit preset with the policies taught here, rather than
a change to the generic `std/types` API. The later diagnostic-origins example
uses that preset and supplies its own origin-aware annotation and comparison
operations. This keeps the source-independent model reusable without making
every type checker implement source tracing.
