# Native lexical analysis

The language definition and its input programs are separate:

```text
definition/
  lexical.yl       Names and whitespace
  expression.yl    References, numbers, and expression alternatives
  function.yl      Parameters, scoped bodies, and function forms
  statements.yl    Variable declarations and their ordering
  language.yl      Program, outer ordering, and entry
programs/
  program.txt           Valid bindings, functions, and closures
  unresolved.txt        A function-local name used outside its scope
  duplicate.txt         Two declarations with the same name
  initializer-error.txt A declaration referring to its own initializer
```

Start with [the entry module](./definition/language.yl), then follow the
[expression](./definition/expression.yl), [function](./definition/function.yl),
[statement](./definition/statements.yl), and [lexical](./definition/lexical.yl)
definitions. Declarations used across modules are explicitly exported and imported.

Expressions can contain functions whose bodies contain statements with expression
initializers. These relationships form normal import cycles. The Rust consumer
and integration tests share `definition.rs`, which supplies all five modules to
the in-memory compiler. No artifact file is required.

This Rust consumer compiles the language and analyzes source entirely in memory:

```sh
cargo run --example semantic_analysis
cargo run --example semantic_analysis -- unresolved
cargo run --example semantic_analysis -- duplicate
cargo run --example semantic_analysis -- initializer
cargo run --example semantic_analysis -- --json
cargo run --example semantic_analysis -- initializer --json
```

The default output is a brief success summary or source diagnostics with underlines. Errors go to stderr and exit with code 1. Color follows the terminal; `CLICOLOR_FORCE=1` forces it, while `NO_COLOR` disables it.

Pass `--json` to write the complete analysis result to stdout, including the AST, retained syntax occurrences, semantic records, reference attachments, and structured diagnostics. JSON mode does not also print human diagnostics.

The default [program](./programs/program.txt) places one declaration or function
per block, making the forward references and scope boundaries visible. Diagnostics
name their input under `programs/`, matching the files in this example directory.

`FunctionContents` establishes a lexical scope without adding an AST wrapper. Function names register in their surrounding scope; parameters and locals use the function context. Reusable `Reference` nodes perform lookup inside ordinary expressions and deferred function bodies.

`TopLevelDeclaration` distinguishes program declarations from function-local declarations using a normal pattern. The program orders function registration before top-level declarations and defers function bodies until those declarations are available. Local initializer references run before their own declaration registration. The examples cover forward functions, later outer bindings in closures, self-recursive function values, parameter/local shadowing, and rejected self-initializers.

This is static name analysis; it does not check execution-time initialization, assignment, target-language types, flow, or ownership. See [the native semantic interface](../../docs/api/native-semantics.md) for library registration, lifecycle hooks, and optional artifacts.
