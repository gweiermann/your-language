# Interpret, transpile, and compile

The `examples/backend-hooks` project defines one language and three consumers of the same compiled language image. The Rust interpreter executes the program, the JavaScript transpiler emits readable source, and the WebAssembly compiler emits a real binary module. All three use the public Rust library directly, without a CLI subprocess or separate parser implementation.

## The sample language

```javascript
let answer = 0;
let index = 0;

while (index < 3) {
    answer = answer + 14;
    index = index + 1;
}

result answer;
```

Definitions live in `definition/` and inputs in `programs/`. Hook contracts are in `definition/hooks.yl`. The example supports signed 32-bit integers, addition, multiplication, comparison, declarations, references, assignment, lexical blocks, conditionals, loops, and a final result. Arithmetic wraps to signed `i32`; conditions use zero as false and other values as true. Integer literals must fit signed `i32`. Division, strings, objects, functions, closures, and Mini TypeScript execution are outside this sample.

These are policies of the sample hosts, not requirements imposed by the engine. The Rust interpreter owns a scope stack and variable map. The JavaScript host produces expressions and statements. The Wasm host maps lexical declarations to local indexes and emits structured branches and loops.

## Compile and inspect the contract

```sh
cargo run -- compile examples/backend-hooks/definition/language.yl -o target/backend.ylc
cargo run -- language target/backend.ylc hooks
cargo run -- language target/backend.ylc hooks --json
cargo run -- language target/backend.ylc check examples/backend-hooks/programs/control-flow.txt
```

The contract lists qualified names such as `hooks.yl#add(left, right)`. Checking succeeds without binding any hooks.

## Execute the three consumers

```sh
cargo run --example backend_interpret -- examples/backend-hooks/programs/control-flow.txt
cargo run --example backend_javascript -- examples/backend-hooks/programs/control-flow.txt > target/backend.js
node target/backend.js
cargo run --example backend_wasm -- examples/backend-hooks/programs/control-flow.txt target/backend.wasm
node -e "const fs=require('node:fs'); const b=fs.readFileSync('target/backend.wasm'); console.log(new WebAssembly.Instance(new WebAssembly.Module(b)).exports.run())"
```

Each executed program prints `42`. The Wasm example writes binary bytes to the requested output file; it does not require WASI or an external Wasm runtime. Node.js provides the WebAssembly implementation used here.

The consumers compile and serialize the language, then reload the `.ylc` before dispatch. An application deploying an already compiled language can load those bytes directly and does not need the original `.yl` definitions.

WebAssembly is the first binary target because this integer language maps directly to its numeric operations and structured control flow. Java bytecode is another possible host implementation; the engine does not require a particular target. No Java class-file backend is provided by this example.

## Why hooks receive syntax

```yl
node Conditional =
    "if"
    "("
    condition: Expression
    ")"
    yes: Block
    "else"
    no: Block
{
    backend {
        conditional(condition, yes, no)
    }
}
```

The interpreter chooses a branch after evaluating the condition. The transpiler and Wasm compiler generate code for both branches. Automatically evaluating arguments in the engine would make these different uses impossible.

Likewise, the interpreter dispatches a loop body repeatedly rather than reusing a cached value. Dispatch budgets bound calls and nesting, and a cancellation flag is checked before each dispatch. A host remains responsible for work it performs inside a hook without dispatching.

## Verification

```sh
cargo test --test backend
cargo test --test backend_examples
```

The example tests execute generated JavaScript and validate and execute generated Wasm with Node. They compare results for arithmetic precedence, loops, branches, lexical shadowing, reassignment, and wrapping overflow. A separate interpreter test places an undefined name in an untaken branch to prove that child dispatch is controlled by the host. Code generators may diagnose undefined names in either branch even when an interpreter never executes that branch.
