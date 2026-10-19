# Your Language

**Define syntax. Compose semantics. Choose your backend.**

Your Language (YL) is a declarative toolkit for building languages. Describe the
grammar and AST, connect names and types through authored libraries, add precise
diagnostics and editor roles, then compile the definition into a reusable `.ylc`.
A Rust consumer can interpret, transpile, or compile the resulting programs through
host-controlled hooks.

## A definition connects syntax and meaning

```yl
import { annotateWithType } from "std/types"

enum Type { number }

node Number = value: /[0-9]+/ {
    ide {
        example "42"
        highlightAs "number"
    }
    semantics {
        annotateWithType(self, Type::number)
    }
}

entry Number
```

Captures shape the AST. Semantic operations attach information to syntax or
resolved declaration identities. The language owns its types and operator rules;
`std/references` and `std/types` supply composable mechanisms written in YL.
Imports such as `std/types` are bundled module names, with no repository-relative paths.

The examples build beyond primitive annotations:

```text
interface Box<T> {
    value: T;
}

let box: Box<string> = { value: "hello" };
let message: string = box.value;
```

The [compound-data example](examples/11-compound-types/) implements structural
interfaces, tuples, lists, explicit generics, property/index access, and assignment
checks. [Mini TypeScript](examples/08-mini-typescript/) separately demonstrates
function signatures and per-call inference. These are focused language subsets,
not a complete TypeScript implementation. Optional
[diagnostic origins](examples/12-diagnostic-origins/) trace a nested mismatch back
to its incompatible literal and group related source locations.

## One language, different consumers

```yl
export hook add(left, right)

node Addition = left: Number "+" right: Number {
    backend { add(left, right) }
}

node Number = /[0-9]+/
entry Addition
```

Hooks receive **unevaluated syntax**. The host decides which children to visit,
how values and scopes work, and whether to return a runtime value, generated
source, or compiled instructions. Static checking does not execute the program.
The [backend capstone](examples/14-backend-hooks/) uses one reloaded language for
three independent Rust hosts: an interpreter, JavaScript transpiler, and WebAssembly compiler.

## Try the examples

From a cloned repository with Rust installed:

```sh
cargo run --example 01-getting-started
cargo run --example 08-mini-typescript
cargo run --example mini_typescript_host -- addition
cargo run --example backend_interpret
```

Project runners check positive and intentional-error fixtures. The inference host
reports values including `numberSum: Type::number`; the interpreter prints `42`.
Each [numbered project](examples/README.md) has a `yl.json`, readable definitions,
and `.txt` target programs.

Install the CLI and inspect a project:

```sh
cargo install --path . --locked
yl project list examples
yl project check examples/08-mini-typescript/programs/program.txt
yl project ast examples/08-mini-typescript/programs/program.txt
```

Or persist a language for another application:

```sh
yl compile examples/03-mini-js/definition/minijs.yl -o minijs.ylc
yl language minijs.ylc ast examples/03-mini-js/programs/program.txt
yl language minijs.ylc check examples/03-mini-js/programs/invalid.txt --json
```

The final command intentionally reports errors and exits nonzero. Human checks
show source snippets and colored underlines; JSON exposes the same structured
diagnostics. `yl fmt` formats YL definitions.

## Editor and documentation

The [VS Code extension](editors/vscode/) provides definition formatting, hover,
example previews, and live diagnostics. Project discovery activates diagnostics
and contextual highlighting for matching target programs. Language definitions
choose roles; the editor theme chooses colors.

Read the [documentation](https://gweiermann.github.io/your-language/):

- [Build a TypeScript-like language, from the first grammar to host backends](https://gweiermann.github.io/your-language/tutorials/course/).
- [Concepts and capabilities](https://gweiermann.github.io/your-language/guide/capabilities).
- [Language reference](https://gweiermann.github.io/your-language/reference/lexical-syntax), [core library](https://gweiermann.github.io/your-language/api/core), and [authored standard library](https://gweiermann.github.io/your-language/reference/standard-library).
- [CLI](https://gweiermann.github.io/your-language/api/cli), [Rust API](https://gweiermann.github.io/your-language/api/rust), and [editor setup](https://gweiermann.github.io/your-language/guide/editor).

## Roadmap

Next directions include literal types and flow-sensitive narrowing, typed
heuristics, richer reusable policies and hosts, navigation and completion, and
incremental analysis. Ownership is a longer-term analysis stress test. See the
[roadmap](https://gweiermann.github.io/your-language/roadmap) for boundaries and priorities.
