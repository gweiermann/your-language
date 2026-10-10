# Roadmap

Your Language brings grammar, AST structure, and diagnostics into a shared language definition. Development extends that foundation in four areas. The following capabilities are planned directions, not APIs or delivery commitments.

## Language analysis

Relations, traits, scopes, declaration and reference resolution, and semantic graphs will describe relationships beyond local syntax. Semantic diagnostics and target-language type checking depend on this analysis model. They remain separate from grammar matching and syntax-local constraints.

## Developer tooling

Editor integration and language-server support will make definitions and parsed languages easier to work with. Incremental parsing will support responsive feedback as source changes. Grammar inspection tools and diagnostic navigation will assist language authors.

## Execution and integration

Parser generation and runtime optimization will address execution cost while preserving language behavior. Additional host bindings, including a C ABI, will broaden embedding options. Compilation strategies such as JIT execution will be evaluated against measured needs.

## Language authoring

Reusable libraries, richer inspection of normalized grammars, and metadata for nodes will support larger definitions and downstream tooling. Metadata requires explicit syntax and composition rules before becoming part of the language contract.

## Design principles

Changes should preserve explicit language policy, structured diagnostics, and the relationship between grammar values and ASTs. Language-design decisions are made before introducing observable behavior. Runtime and tooling improvements must remain general rather than relying on individual example grammars.
