# Diagnostic origins

A type checker can compare plain descriptors without recording where they came
from. For deeper diagnostics, the [diagnostic-origins example](https://github.com/gweiermann/your-language/tree/main/examples/12-diagnostic-origins)
adds source information beside each contribution. This is optional authored YL,
not a special type feature built into the engine.

## Attach an origin

```yl
operation typeSource(value, origin, children = []) = (value, origin, children)

operation annotateWithType(target, value, origin = target, children = []) {
    contribute((target, typeSources), typeSource(value, origin, children))
}
```

The three positions mean **type**, **source syntax**, and **child sources**.
`typeSourceOf(target)` collects contributions with `facts`; `typeOf(target)` reads
just their combined descriptor. A number literal supplies its token capture as
the origin. Two number contributions from different locations remain distinct
because each record includes its source identity.

## Preserve an origin through references

```yl
operation inheritTypeFrom(target, source) {
    annotateTypeSource(target, typeSourceOf(source))
}
```

A reference resolves its declaration and inherits that declaration's type source.
The source remains the original expression rather than changing to the reference.
For objects, `structTypeSource` stores `(fieldName, childSource)` pairs. For tuples
and lists, child sources stay in element order. The recursive comparison selects
the matching child source while descending through the type descriptor.

## Return the offending node

A comparison difference adds one position to the plain example's result:

```yl
(path, actualType, expectedType, reason, offendingNode)
```

The path describes the structural mismatch. The source node selects the actual
underline; generating a textual path alone cannot do that.

## Group diagnostic annotations

```yl
error(
    describeDifference(problem),
    [
        (problem[4], absent),
        (annotation.value, "required here"),
        (initializer, "checked here")
    ]
)
```

One call produces one diagnostic group. `absent` omits a label but retains the
highlight. The first annotation is the primary location. The CLI renders every
annotation with its source. The LSP also provides additional source locations;
editor presentation depends on the client.

## Follow a scattered mismatch

`programs/invalid-scattered.txt` passes a string through declarations and nested
object fields before checking an `Envelope<number>`. Comparison finds
`payload.details.record.value` and returns the original string literal's node.
The reporter groups that literal with the annotation and checked initializer.

```powershell
cargo run -- project check examples/12-diagnostic-origins/programs/invalid-scattered.txt
cargo run -- project check examples/12-diagnostic-origins/programs/invalid-scattered.txt --json
```

Both commands exit with status 1. To verify every fixture, including intentional
errors, run `cargo run -- project check examples/12-diagnostic-origins`.

This keeps actual-value origins, not the entire history of a type derivation.
Tracing all expected-type substitutions is deferred. List indexing retains the
access location rather than identifying an initializer element through mutation.
