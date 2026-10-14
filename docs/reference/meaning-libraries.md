# Meaning libraries

Meaning libraries compose queries, contributions, guards, and diagnostics. The compiler bundles `std/references` and `std/types` as authored YL modules over the generic `std/query` primitives.

`self` identifies the current syntax occurrence inside a node or pattern's `meanings` block, and captures identify their syntax occurrences directly. `referenceOf(value)` queries the declaration identity associated with a declaration or reference. `std/types` accepts either identity or a tuple key and does not perform name resolution itself.

Operations receive their targets explicitly; they do not inherit `self` from a caller:

```yl
operation annotateNumber(target) {
    types::annotate(target, number)
}

node Number = /[0-9]+/ {
    meanings { annotateNumber(self) }
}
```

An explicitly declared parameter named `self` is allowed. A bare contextual `self` inside an operation is a definition error. Captures can be passed directly as targets.

An absent query argument ends only its dependent query or contribution. Pending inputs wait; optional absent inputs do not cancel an entire operation. Presence checks remain useful when choosing a fallback or deliberately diagnosing a missing name.

For signatures and behavior, see [authored semantic libraries](/api/standard-semantics) and [declarative attachment queries](/api/declarative-queries). The [std/types tutorial](/tutorials/std-types) connects these operations in a runnable inference example. [Rust runtime operations](/api/native-semantics) describe consumer extension contracts and the limits on hypothetical native effects.
