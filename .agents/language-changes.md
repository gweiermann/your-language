# Language and library changes

Read this guide when changing YL syntax, compilation, runtime behavior, or authored libraries.

1. Locate the affected public contract in [the language reference](../docs/reference/lexical-syntax.md). Follow the relevant topic: [nodes](../docs/reference/nodes.md), [patterns](../docs/reference/patterns.md), [modules](../docs/reference/modules.md), [semantics](../docs/reference/meaning-libraries.md), or [backend hooks](../docs/reference/backends.md).
2. Keep parser primitives and query evaluation in core. Implement authorable policy in YL; keep opinionated presets separately named and explicitly imported. Consult [core and standard libraries](../docs/reference/standard-library.md) and [authored semantic APIs](../docs/api/standard-semantics.md).
3. Preserve spans and structured diagnostics through compilation and reload. Cover relevant behavior through the public API and source-free `.ylc` reload; consult [artifact contracts](../docs/api/artifacts.md), [query APIs](../docs/api/declarative-queries.md), and [diagnostics](../docs/api/diagnostics.md).
4. Keep host traversal, values, and execution state owned by the consumer of [backend dispatch](../docs/api/backends.md). Update the affected reference or tutorial alongside observable behavior.

Use [.agents/implementation/](implementation/README.md) only for historical design context; current public documentation and the user's decisions take precedence.
