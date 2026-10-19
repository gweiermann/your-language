# 9. Interpret, transpile, and compile

Recognizing and checking a program does not execute it. A **backend** turns a
checked program into a useful result:

- An **interpreter** visits syntax and computes values immediately.
- A **transpiler** writes source in another language, here JavaScript.
- A **compiler** writes an executable representation, here WebAssembly (Wasm).

A **hook** connects a grammar node to an action implemented by the host
application. YL names the action and its captured inputs; the host decides how
to evaluate them. These examples require Rust to run the commands. Basic Rust
knowledge is useful for changing the host implementations, but you can compare
their results before learning Rust's interface terminology.

The final checkpoint composes syntax, references, type restrictions, IDE metadata,
and a shared backend contract. Its executable subset deliberately uses signed
32-bit integers, lexical variables, assignments, blocks, branches, and loops.
It is a backend example, not a runtime for all earlier TypeScript-like data types.

<!-- course:project=examples/14-backend-hooks -->

Use `examples/14-backend-hooks` as the project. The authored
`std/backend/integer` module declares hook signatures; Rust implementations in
`hosts/interpreter.rs`, `hosts/javascript.rs`, and `hosts/webassembly.rs` own
values, scopes, traversal, and emitted code. Hook arguments are unevaluated captures.

## Trace one addition before reading the full module

For `40 + 2`, the number nodes capture the text `40` and `2`. Their hooks let a
host turn that text into numeric values or generated code. The addition hook
receives the `left` and `right` captures **unevaluated**: they are syntax, not
already-computed numbers.

An interpreter dispatches the left child to obtain 40, dispatches the right
child to obtain 2, then returns 42. The JavaScript backend instead obtains source
fragments and combines them into addition source. The Wasm backend emits numeric
instructions. The grammar mapping is shared; the hosts own their result values.

This freedom matters for `if`: an interpreter visits only the selected branch,
whereas a compiler visits both branches to emit conditional code. Automatically
evaluating all children first would break that distinction.

Read the complete expression module and find each captured argument beside its
backend mapping:


<p class="course-file" data-file="examples/14-backend-hooks/definition/expression.yl"><strong>Read the complete file:</strong> <code>examples/14-backend-hooks/definition/expression.yl</code></p>

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

## Try a change

Copy `control-flow.txt` to a temporary `.txt` file outside the example's programs
folder. Change `total = total + 14;` to `total = total + 15;`. The loop adds the
number three times, so passing that copy to `backend_interpret` prints **45**
instead of 42 and exits **0**. Delete a required semicolon and run again:
preparation reports a source error before executing any runtime hooks. Restore
the semicolon to recover.

Read `hosts/interpreter.rs` first, then compare the other two host files.
Dispatch asks the engine to visit a child; the host owns variable values, scope,
and control flow. The consumer implements the engine's `Backend` interface and
supplies hook bindings. The engine does not generate a method for each hook name.

Continue with [the backend walkthrough](/tutorials/backends) and
[Rust dispatch API](/api/backends) to implement another host. Strings and objects
require representations and operations in that host; adding their grammar does
not automatically implement their runtime values. Hooks unreachable from the
entry grammar do not become required implementations.

Previous: [Editor support](./08-editor).
