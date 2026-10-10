# Your Language

Your Language (YL) is a declarative language for defining programming-language syntax, AST structure, semantic roles, constraints, and diagnostics.

A YL definition is intended to be compiled into a reusable language artifact and executed by the Your Language runtime.

## Current design

The current syntax design is documented in [documentation/readme.md](./documentation/readme.md).

A larger example is available in [documentation/target-syntax](./documentation/target-syntax).

## Rust compiler and runtime

```text
*.yl
  ↓
YL compiler
  ↓
*.ylc
  ↓
Your Language runtime
  ↓
AST + diagnostics
```

The Rust library exposes compilation, deterministic `.ylc` serialization/reloading,
and parsing with ASTs and structured diagnostics. The `yl` CLI supports `check`,
`compile`, and `parse --json`.

**Syntax-v0 / issue #5 is not complete.** MiniJS compilation and parsing pass; general constraints and metadata still have documented
language-design questions. See [implementation status](./documentation/implementation)
and the [exact PARKED decisions](./documentation/implementation/PARKED.md).

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings

cargo run -- check tests/fixtures/syntax-v0/language.yl
cargo run -- compile tests/fixtures/syntax-v0/language.yl -o independent.ylc
cargo run -- parse independent.ylc tests/fixtures/syntax-v0/program.txt --json
```

The independent fixture exercises the complete API/CLI pipeline; it does not replace
the checked-in MiniJS acceptance language. Native parser generation and the semantic
layer are outside this implementation.

## CLI tools

```sh
cargo run -- compile documentation/target-syntax/minijs.yl -o target/minijs.ylc
cargo run -- language target/minijs.ylc ast tests/fixtures/minijs/program.js
cargo run -- language target/minijs.ylc check tests/fixtures/minijs/unexpected-token.js
cargo run -- language target/minijs.ylc check tests/fixtures/minijs/unexpected-token.js --json
```

`ast` produces AST JSON. `check` reports diagnostics with terminal source
underlines; `--json` selects a diagnostic array. A clean human check is silent.
