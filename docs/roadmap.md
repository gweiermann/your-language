# Roadmap

YL already composes parsing, authored references and type analysis, IDE metadata,
and host-controlled execution through reusable language artifacts. See
[capabilities and boundaries](/guide/capabilities) for the implemented contract.
The following are development directions, not delivery commitments.

## Flow-sensitive analysis

Extend the current inference and restriction model with literal types, branch-local
narrowing, assignment flow, reachability, and loop analysis. Once these facts are
reliable, explore typed heuristics and optimization rules. Ownership and borrowing
are longer-term stress tests for composable analysis libraries.

## Language authoring and libraries

Expand explicit opt-in policies and compose them in the existing learning course.
Improve grammar inspection, reusable diagnostics, generic/structural type coverage,
and examples without hiding target-language decisions inside the engine.

## Editor tools

Build on project discovery, live diagnostics, highlighting, hover, and formatting
with definition/reference navigation, completion, rename, and incremental analysis.
Target-language formatting needs an explicit contract rather than assumptions
about whitespace inferred from parsing.

## Execution and integration

Broaden host hook presets and backend examples beyond the current integer subset.
Explore additional bindings, generated parsers, and portable artifact encodings
while retaining source-linked diagnostics and host ownership of runtime state.
Long-term binary compatibility and a C ABI require separate stable contracts.
