# Queries and contributions

Semantic operations describe facts and the relationships between them. They do
not execute the target program and should not depend on which pending calculation
happens to finish first.

```yl verify=query
import { contribute, get } from "core/query"

symbol category
symbol number

operation annotateNumber(target) {
    contribute((target, category), number)
}

node Number = /[0-9]+/ {
    semantics { annotateNumber(self) }
}

entry Number
```

The tuple `(target, category)` identifies an attachment slot. `contribute` supplies
a value; `get` reads a unique final value. Independent contributions are not
imperative assignments that overwrite each other. Use `getAll` for all values,
or `facts` for currently available distinct facts during positive accumulation.

## Pending is different from absent

A lookup waits while a relevant contribution could still arrive. It returns
`absent` only after the relevant work settles without a value. A guarded
contribution remains possible until its condition is known. Optional absent
captures end dependent query paths without suppressing unrelated operations.

`:=` names a dependency-tracked expression. It is immutable, can remain pending,
and does not impose top-to-bottom execution. `when` activates contributions
according to its condition. Explicit semantic precedence and boundaries express
visibility requirements; query readiness remains separate from source order.

## Conventions that make relationships clear

Use stable symbols to name channels, structured tuples to combine identities,
and the same key shape on both sides of a relationship. Pass context and captures
explicitly into helpers. Reuse pure operations for repeated calculations.
These conventions help the engine inspect dependencies and help readers follow
them; dynamic keys still work conservatively and carry no universal speed guarantee.
Caching is internal and must not change results, visibility, or diagnostics.

The [core reference](/api/core) lists the operations. The
[query contract](/api/declarative-queries) explains callbacks, alternatives,
isolated hypothetical evaluation, recursive operations, and convergence.
