# Diagnostic origins

An optional extension to [compound types](../11-compound-types/README.md).
Type correctness does not need origin tracking. Origin tracking answers a separate
question: **which source expression contributed the incompatible type?**

The plain constructors, generic substitution, type tags, and renderers are imported
from the preceding example through ordinary YL imports. This example adds a small
source record and a comparison worker that carries the matching source along.

```yl
// (type descriptor, source syntax, child sources)
operation typeSource(value, origin, children = []) = (value, origin, children)

operation annotateWithType(target, value, origin = target, children = []) {
    contribute((target, typeSources), typeSource(value, origin, children))
}
```

`typeSourceOf(node)` reads that record; `typeOf(node)` reads only its descriptor.
`inheritTypeFrom(target, source)` propagates the record through references and wrapper
syntax. `structTypeSource(fields, origin)` retains each field's record, while list
and tuple constructors retain records by element position. `childTypeSource`
selects a retained child or falls back to the enclosing expression's origin.
These are ordinary authored functions, not engine-defined concepts.

For example, `programs/invalid-scattered.txt` creates a wrong string, puts it in
nested objects through several declarations, and finally checks an
`Envelope<number>`. The diagnostic reports `payload.details.record.value`,
underlines the original `"wrong"`, and marks the annotation and checked initializer
in the same diagnostic group.

```powershell
cargo run -- project check examples/12-diagnostic-origins
cargo run -- project check examples/12-diagnostic-origins/programs/invalid-scattered.txt
cargo run -- project check examples/12-diagnostic-origins/programs/invalid-scattered.txt --json
```

The complete project check succeeds, including expected failures. A direct check
of `invalid-scattered.txt` exits with status 1.

A difference is `(path, actualType, expectedType, reason, offendingNode)`. The
reporter passes that node and additional annotations to `error(message, labels)`.
Equal descriptors can have different origins: origins belong to contributions,
not globally to `number` or `string`. The example preserves field and tuple
origins; a list index access currently highlights the access site. Full expected-
type derivation chains and flow-sensitive provenance remain deferred.
