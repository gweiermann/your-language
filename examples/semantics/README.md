# Native lexical analysis

This Rust consumer compiles the language and analyzes source entirely in memory:

```sh
cargo run --example semantic_analysis
cargo run --example semantic_analysis -- unresolved
cargo run --example semantic_analysis -- duplicate
cargo run --example semantic_analysis -- initializer
```

The negative examples exit with code 1 and return structured source diagnostics. Successful analysis returns an AST, retained syntax occurrences, and reference attachments pointing to declaration records.

`FunctionContents` establishes a lexical scope without adding an AST wrapper. Function names register in their surrounding scope; parameters and locals use the function context. Reusable `Reference` nodes perform lookup inside ordinary expressions and deferred function bodies.

`TopLevelDeclaration` distinguishes program declarations from function-local declarations using a normal pattern. The program orders function registration before top-level declarations and defers function bodies until those declarations are available. Local initializer references run before their own declaration registration. The examples cover forward functions, later outer bindings in closures, self-recursive function values, parameter/local shadowing, and rejected self-initializers.

This is static name analysis; it does not check execution-time initialization, assignment, target-language types, flow, or ownership. See [the native semantic interface](../../docs/api/native-semantics.md) for library registration, lifecycle hooks, and optional artifacts.
