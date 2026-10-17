# Host-controlled backends

One YL language drives a Rust interpreter, a JavaScript transpiler, and a WebAssembly binary compiler. The engine passes unevaluated syntax to hooks. Each host owns traversal, lexical bindings, runtime values or generated output.

```javascript
let answer = 0;
let index = 0;

while (index < 3) {
    answer = answer + 14;
    index = index + 1;
}

result answer;
```

Run these commands from the repository root:

```sh
cargo run --example backend_interpret -- examples/backend-hooks/programs/control-flow.txt
cargo run --example backend_javascript -- examples/backend-hooks/programs/control-flow.txt > target/backend.js
node target/backend.js
cargo run --example backend_wasm -- examples/backend-hooks/programs/control-flow.txt target/backend.wasm
node -e "const fs=require('node:fs'); const b=fs.readFileSync('target/backend.wasm'); console.log(new WebAssembly.Instance(new WebAssembly.Module(b)).exports.run())"
```

The interpreter, generated JavaScript, and generated Wasm each produce `42`. Omitting the source argument selects this same program. The Wasm file exports `run() -> i32` and has no imports, WASI dependency, or required npm packages. Node.js is required for the cross-target integration tests.

```sh
cargo run -- compile examples/backend-hooks/definition/language.yl -o target/backend.ylc
cargo run -- language target/backend.ylc hooks
cargo test --test backend_examples
cargo run --example backend_interpret -- examples/backend-hooks/programs/runtime-error.txt
```

The final command deliberately fails and underlines `missing`. The consumers compile, serialize, and reload the language before using the public backend API. Existing `.ylc` bytes can be loaded directly by a deploying application; the engine does not require source definitions at runtime.

## Files

- `definition/hooks.yl`: the fixed host contract.
- `definition/lexical.yl`: names, bounded keywords, and trivia.
- `definition/expression.yl`: literals, references, arithmetic, and precedence.
- `definition/statement.yl`: declarations, assignments, blocks, and control flow.
- `definition/language.yl`: the entry program.
- `hosts/mod.rs`: all three host implementations, shared by the example binaries and tests.
- `programs/`: control flow, shadowing, initializer shadowing, arithmetic, overflow, and a runtime error.

## Language policies and limits

Values are signed `i32`; addition and multiplication wrap. Comparisons produce `0` or `1`, and conditions treat any nonzero value as true. Literals must fit signed `i32`. A declaration evaluates its initializer before introducing its binding, so an initializer may read an outer declaration with the same name. Duplicate declarations in one scope are errors. There are no strings, division, functions, closures, or objects in this demonstration.

The interpreter limits loop iterations to 10,000 and also uses the engine's dispatch budget. Generated JavaScript and Wasm retain the source loop behavior and have no injected runtime fuel limit. Their compilation dispatch is still bounded. Both code generators visit both branches and reject unresolved names even in a branch an interpreter would skip.

JavaScript bindings receive unique generated names to preserve the sample's initializer and shadowing behavior rather than inherit JavaScript temporal-dead-zone rules. The Wasm host allocates a separate local for every declaration.

See [the backend tutorial](../../docs/tutorials/backends.md) for the architecture and API walkthrough.
