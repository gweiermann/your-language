# Meaning libraries

Meaning libraries compose queries, contributions, guards, and diagnostics. The compiler bundles `std/references` and `std/types` as authored YL modules over the generic `std/query` primitives.

`self` identifies the current syntax occurrence, and captures identify their syntax occurrences directly. `referenceOf(value)` queries the declaration identity associated with a declaration or reference. `std/types` accepts either identity or a tuple key and does not perform name resolution itself.

For signatures and behavior, see [authored semantic libraries](/api/standard-semantics) and [declarative attachment queries](/api/declarative-queries). The [std/types tutorial](/tutorials/std-types) connects these operations in a runnable inference example. [Rust runtime operations](/api/native-semantics) describe consumer extension contracts and the limits on hypothetical native effects.
