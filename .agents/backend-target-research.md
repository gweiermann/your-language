# Backend compilation target research

Date: 2026-10-17. This is research for the host-controlled hook prototype, not a claim that the prototype has already shipped.

## Recommendation

Use WebAssembly core modules as the first binary compilation example, alongside a Rust interpreter and JavaScript transpiler. WebAssembly explicitly targets portable, compact executable binaries and supports non-browser hosts. This choice exercises compilation without tying the core engine to a target language or requiring a JVM class-file writer. It does not mean compiling the YL engine itself to WebAssembly. [WebAssembly goals](https://webassembly.org/docs/high-level-goals/)

Keep the demonstration deliberately narrow: signed 32-bit integers, declarations, references, assignment, arithmetic, comparisons, blocks, conditionals, and loops. Export a zero-argument `run` function returning `i32`. Avoid memory, garbage collection, components, WASI, imports, and a Rust Wasm runtime until an example actually needs them. Core modules have no inherent OS API; imports are a host decision. [WebAssembly portability](https://webassembly.org/docs/portability/)

This is an engineering recommendation, not a new generic YL runtime-value contract. The host still owns values, scopes, evaluation, emitted expressions, and machine instructions. Hook dispatch must pass unevaluated syntax captures so an interpreter chooses one branch while a compiler emits both.

## Encoder dependency and Rust compatibility

The repository declares Rust 1.85. Pin `wasm-encoder = { version = "=0.225.0", default-features = false }` for the example. Its release workspace declares Rust 1.76, and the encoder manifest has `leb128fmt` as its only mandatory direct dependency; `wasmparser` is optional. The pin is intentional: the currently inspected main release line, 0.262, requires Rust 1.88. [0.225 workspace manifest](https://raw.githubusercontent.com/bytecodealliance/wasm-tools/v1.225.0/Cargo.toml), [encoder manifest](https://raw.githubusercontent.com/bytecodealliance/wasm-tools/v1.225.0/crates/wasm-encoder/Cargo.toml), [current workspace manifest](https://raw.githubusercontent.com/bytecodealliance/wasm-tools/main/Cargo.toml)

Prefer an example-specific package or development dependency so the general parsing/loading library does not need a binary-emission dependency. Do not introduce Wasmtime merely to test execution when Node is already available.

The release API provides `Module`, `TypeSection`, `FunctionSection`, `ExportSection`, `CodeSection`, `Function`, and `Instruction`. Construct the function signature through `types.ty().function([], [ValType::I32])`; add a function with type index zero; export it; build its locals and instructions; finish with `Instruction::End`; call `module.finish()`. The release's own example demonstrates this exact section-based pattern. [Encoder source and complete example](https://raw.githubusercontent.com/bytecodealliance/wasm-tools/v1.225.0/crates/wasm-encoder/src/lib.rs)

Use structured `If`/`Else`/`End`, `Block`/`Loop`, and relative branch depths for control flow. Keep a host-side lexical environment mapping declaration identities to local indexes. A shadowed declaration receives a different local index. Assignment looks up the existing declaration's index. Do not use identifier text alone across nested scopes. Instruction builders encode `LocalGet`, `LocalSet`, arithmetic, comparisons, and structured branches. [Instruction definitions](https://raw.githubusercontent.com/bytecodealliance/wasm-tools/v1.225.0/crates/wasm-encoder/src/core/code.rs)

## Align the three example backends

Document the sample language's arithmetic explicitly. Recommend signed `i32` values with wrapping addition, subtraction, and multiplication. WebAssembly integer arithmetic is defined modulo the bit width. [Numeric specification](https://webassembly.github.io/spec/core/exec/numerics.html)

The Rust interpreter should use `wrapping_add`, `wrapping_sub`, and `wrapping_mul`; the JavaScript output should coerce addition/subtraction with `| 0` and use `Math.imul` for multiplication; Wasm should emit the corresponding `i32` instructions. This is example-language policy, not a generic engine decision. Use zero as false and nonzero as true for sample conditions. If division is added, explicitly handle division by zero and signed minimum divided by minus one consistently; omit division from the first example rather than silently differing across targets.

For declarations and assignments used as statements, the compiler needs to leave the operand stack balanced. A final value should be explicitly specified by the sample grammar/host, rather than assuming every statement leaves one value.

## Actual runtime availability and smoke experiment

Read-only local inspection found:

- Node.js `v24.11.1`, at `C:\Program Files\nodejs\node.exe`.
- Java and javac `17.0.9` on PATH.

A real hand-encoded module was validated and executed in Node during research. It exported `add(i32, i32) -> i32`; `add(19, 23)` returned 42, and `add(2147483647, 1)` returned -2147483648. This verifies the local execution route and wrapping convention, not the future YL-to-Wasm backend implementation.

Node provides `WebAssembly` directly. No npm dependency or WASI setup is necessary for a module with no imports. Its official example reads a `.wasm`, instantiates it, and invokes an exported function. [Node WebAssembly guide](https://nodejs.org/learn/getting-started/nodejs-with-webassembly)

Required emitted-artifact test pattern:

```javascript
const assert = require("node:assert/strict");
const fs = require("node:fs");
const bytes = fs.readFileSync(process.argv[2]);
assert(WebAssembly.validate(bytes));
const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes));
assert.equal(instance.exports.run(), 42);
```

Validation must run against the bytes emitted by the YL example compiler, followed by actual execution. Checking the magic header alone is insufficient. Compare interpreter results, execution of generated JavaScript, and execution of emitted Wasm for the same checked-in programs.

Include tests for precedence; nested lexical shadowing; reassignment; an untaken branch containing an invalid runtime operation; a terminating loop; overflow; and a compilation artifact reloaded from `.ylc` without consulting the original grammar files. A backend may additionally impose its own documented fuel or iteration limit; target-code behavior should not accidentally differ just because the interpreter has a safety limit.

## Why not Java bytecode first?

Java is feasible and installed. A proper JVM backend must emit a valid class structure, constant-pool entries, method descriptors and Code attributes, operand-stack/local bounds, and valid control-flow verification information. Those requirements are documented in the JVM specification, including StackMapTable and class-file verification. They add useful backend work but do not better test the generic hook boundary than a smaller Wasm emitter. [JVM class-file specification](https://docs.oracle.com/javase/specs/jvms/se24/html/jvms-4.html)

Defer JVM emission rather than provide Java source and call that direct bytecode compilation. If later implemented, load and run the generated `.class` through a real JVM, just as the initial Wasm example must execute actual emitted bytes.
