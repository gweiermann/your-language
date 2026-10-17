# Checked programs with host-controlled backends

This capstone combines `std/references`, `std/types`, and the opt-in `std/backend/integer` hook contract. One checked YL language drives a Rust interpreter, a JavaScript transpiler, and a WebAssembly binary compiler. The engine passes unevaluated syntax to hooks. Each host owns traversal, lexical bindings, runtime values or generated output.

```javascript
let answer: i32 = 0;
let index = 0;

while (index < 3) {
    answer = answer + 14;
    index = index + 1;
}

result answer;
```

Run these commands from the repository root:

```sh
cargo run --example backend_interpret -- examples/14-backend-hooks/programs/control-flow.txt
cargo run --example backend_javascript -- examples/14-backend-hooks/programs/control-flow.txt > target/backend.js
node target/backend.js
cargo run --example backend_wasm -- examples/14-backend-hooks/programs/control-flow.txt target/backend.wasm
node -e "const fs=require('node:fs'); const b=fs.readFileSync('target/backend.wasm'); console.log(new WebAssembly.Instance(new WebAssembly.Module(b)).exports.run())"
```

The interpreter, generated JavaScript, and generated Wasm each produce `42`. Omitting the source argument selects this same program. The Wasm file exports `run() -> i32` and has no imports, WASI dependency, or required npm packages. Node.js is required for the cross-target integration tests.

```sh
cargo run -- compile examples/14-backend-hooks/definition/language.yl -o target/backend.ylc
cargo run -- language target/backend.ylc hooks
cargo test --test backend_examples
cargo run --example backend_interpret -- examples/14-backend-hooks/programs/type-error.txt
```

The final command deliberately fails before executing any hooks and underlines `42`: the `string` annotation cannot accept an integer initializer. `runtime-error.txt` is the earlier unknown-name fixture and now fails during static checking as well. Use `programs/host-error.txt` to see an actual host failure: its integer literal exceeds signed `i32` range. The consumers compile, serialize, and reload the language before using the public backend API. Existing `.ylc` bytes can be loaded directly by a deploying application; the engine does not require source definitions at runtime.

## Files

- `../../stdlib/backend/integer.yl`: reusable hook declarations, imported as `std/backend/integer`; all three Rust hosts implement this exact contract.
- `definition/typing.yl`: this example's integer type, annotation syntax, and reusable checking operations.
- `definition/lexical.yl`: names, bounded keywords, and trivia.
- `definition/expression.yl`: literals, references, arithmetic, and precedence.
- `definition/statement.yl`: declarations, assignments, blocks, and control flow.
- `definition/language.yl`: the entry program.
- `hosts/mod.rs`: module exports shared by example binaries and tests.
- `hosts/common.rs`: load the project entry from `yl.json`, prepare sources, and render diagnostics.
- `hosts/interpreter.rs`: runtime values, lexical environments, branch selection, and loops.
- `hosts/javascript.rs`: JavaScript expression/statement generation and stable binding names.
- `hosts/webassembly.rs`: WebAssembly instructions, local allocation, and binary module construction.
- `programs/`: control flow, shadowing, initializer shadowing, arithmetic, explicit annotations, overflow, static errors, and a host range error.

## Language policies and limits

Values are signed `i32`; addition and multiplication wrap. Comparisons produce `0` or `1`, and conditions treat any nonzero value as true. Literals must fit signed `i32`. A declaration evaluates its initializer before introducing its binding, so an initializer may read an outer declaration with the same name. Duplicate declarations in one scope are errors. The optional `: i32` annotation checks an initializer without changing runtime representation. `: string` is recognized only to demonstrate a rejected contract; string values are unsupported. There are no string literals, division, functions, closures, or objects in this demonstration.

Grammar, visible captures, static checks, IDE metadata, and backend calls remain together in each construct's definition. Larger reusable checks live in `typing.yl` and take their targets explicitly. The shared backend module declares hooks only: it neither evaluates children nor implements values, scopes, arithmetic, or control flow. Those policies are implemented independently by each host.

The interpreter limits loop iterations to 10,000 and also uses the engine's dispatch budget. Generated JavaScript and Wasm retain the source loop behavior and have no injected runtime fuel limit. Their compilation dispatch is still bounded. Static analysis checks both branches before any host runs, including branches an interpreter will skip. After checking, both code generators visit both branches; the interpreter visits only the selected branch.

JavaScript bindings receive unique generated names to preserve the sample's initializer and shadowing behavior rather than inherit JavaScript temporal-dead-zone rules. The Wasm host allocates a separate local for every declaration.

See [the backend tutorial](../../docs/tutorials/backends.md) for the architecture and API walkthrough.

## Common project interface

```powershell
cargo run -- project check examples/14-backend-hooks
cargo run -- project ast examples/14-backend-hooks/programs/control-flow.txt
```

These commands parse and check every configured program without executing host
hooks. Unknown names and incompatible annotations are expected static failures. Overflow
and host range failures are separately exercised by the backend integration tests. The Rust consumers load their language entry from the
same `yl.json`; each backend lives in its own module for independent reading.

## Run this example

```powershell
cargo run --example 14-backend-hooks
# Equivalent friendly name:
cargo run --example backend-hooks
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
