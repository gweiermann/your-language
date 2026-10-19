# 9. Interpret, transpile, and compile

The final checkpoint composes syntax, references, type restrictions, IDE metadata,
and a shared backend contract. Its executable subset deliberately uses signed
32-bit integers, lexical variables, assignments, blocks, branches, and loops.
It is a backend example, not a runtime for all earlier TypeScript-like data types.

<!-- course:project=examples/14-backend-hooks -->

Use `examples/14-backend-hooks` as the project. The authored
`std/backend/integer` module declares hook signatures; Rust implementations in
`hosts/interpreter.rs`, `hosts/javascript.rs`, and `hosts/webassembly.rs` own
values, scopes, traversal, and emitted code. Hook arguments are unevaluated captures.

Read the complete expression module, including the addition mapping:

<<< ../../../examples/14-backend-hooks/definition/expression.yl

Static checking never invokes runtime hooks. A consumer loads the same `.ylc`,
binds every required reachable hook, prepares a program, and dispatches it.
An interpreter visits only the selected branch; a compiler visits both branches
to emit control flow. Return values belong to the host, not to a built-in engine
variable map.

From the repository root:

```sh
cargo run --example 14-backend-hooks
cargo run --example backend_interpret -- examples/14-backend-hooks/programs/control-flow.txt
cargo run --example backend_javascript -- examples/14-backend-hooks/programs/control-flow.txt > target/backend.js
node target/backend.js
cargo run --example backend_wasm -- examples/14-backend-hooks/programs/control-flow.txt target/backend.wasm
node -e "const fs=require('node:fs');const b=fs.readFileSync('target/backend.wasm');console.log(new WebAssembly.Instance(new WebAssembly.Module(b)).exports.run())"
```

The project check succeeds, the interpreter prints `42`, executed JavaScript
prints `42`, and the generated Wasm module's `run()` returns `42`. On Windows
PowerShell, use a UTF-8-preserving redirection method if your shell defaults to
UTF-16 when writing generated JavaScript.

Continue with [the backend walkthrough](/tutorials/backends) and
[Rust dispatch API](/api/backends) to implement another host. Strings and objects
require representations and operations in that host; adding their grammar does
not automatically implement their runtime values. Hooks unreachable from the
entry grammar do not become required implementations.

Previous: [Editor support](./08-editor).
