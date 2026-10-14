# Roadmap

Your Language brings grammar, AST structure, and diagnostics into a shared language definition. Development extends that foundation in four areas. The following capabilities are planned directions, not APIs or delivery commitments.

## Language analysis

Native meaning libraries already provide experimental lexical scopes, declaration/reference identity and type constraints. Further work will develop composable analysis protocols, union-producing annotations, directional compatibility and per-call generic inference, followed by flow-sensitive narrowing and ownership experiments. These analyses remain separate from grammar matching and syntax-local constraints.

## Developer tooling

The YL language server supplies live definition diagnostics, formatting and composed example previews. Next steps include completion, navigation and rename with explicit semantic contracts. Target-language servers and incremental analysis will extend tooling beyond full-graph YL authoring.

## Execution and integration

Parser generation and runtime optimization will address execution cost while preserving language behavior. Additional host bindings, including a C ABI, will broaden embedding options. Compilation strategies such as JIT execution will be evaluated against measured needs.

## Language authoring

Parser-validated `example` metadata and bounded generation already provide representative syntax for nodes and parameterless patterns. Further grammar inspection and reusable library mechanisms will support larger definitions. Additional metadata must define its syntax and composition rules before introduction.

## Design principles

Changes should preserve explicit language policy, structured diagnostics, and the relationship between grammar values and ASTs. Language-design decisions are made before introducing observable behavior. Runtime and tooling improvements must remain general rather than relying on individual example grammars.
