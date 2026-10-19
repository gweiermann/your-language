# Examples and curriculum

Read this guide when adding or modifying an example, tutorial, or standard-library demonstration.

1. Identify the learning step in [the course](../docs/tutorials/course/index.md) and [example catalog](../examples/README.md). State its objective and prerequisites, which behavior it implements locally, and which libraries it reuses. Extend that step unless a distinct concept needs a separate lesson.
2. Teach the mechanism before its abstraction: core primitives, local authored helpers, canonical standard-library reuse, then explicit opinionated presets and composed capstones. Keep the full early implementation readable when later examples reuse it.
3. Keep captures beside short semantic, backend, and IDE connections. Put substantial reusable logic in explicit operations. Follow [composable definitions](../docs/tutorials/composable-definitions.md), [authored scopes](../docs/tutorials/declarative-scopes.md), and [type inference](../docs/tutorials/std-types.md).
4. Preserve numbered discovery order, both friendly and numbered Cargo names, `definition/`, `programs/`, and `yl.json`. Keep target programs as `.txt` so project-selected highlighting wins over unrelated extensions. Use [project configuration](../docs/api/projects.md) as the shared runner contract.
5. Include positive programs and intentional negative fixtures where the lesson has diagnostics. Verify catalog discovery, the affected Cargo launchers, and meaningful public-API/reload tests. For backend lessons, run the actual hosts as well as static checks.
6. Update the example README and relevant [tutorial](../docs/guide/introduction.md). Provide copyable commands, expected outputs or highlights, and honest subset limitations. In the PR, identify the learning step and explain why a new example was needed, if one was added.

Keep future examples part of this progression rather than adding a separate demonstration directory for every PR.
