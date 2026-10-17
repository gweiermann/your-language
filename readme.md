# Your Language

**Define syntax. Compose meaning. Build language tools.**

Your Language is a declarative language for building parsers, structured abstract syntax trees, and precise diagnostics. Describe a language's syntax in YL, compile it into a reusable `.ylc` artifact, and parse source through the CLI or Rust API.

## From syntax to an AST

Define a grammar with named captures:

```yl
node Declaration =
    "let" name: /[A-Za-z_][A-Za-z_0-9]*/ "=" value: /[0-9]+/

trivia Whitespace = /[ \t\r\n]+/

entry Declaration {
    trivia Whitespace
}
```

Parse a source file written in that language:

```text
let answer = 42
```

Get a structured tree:

```json
{
  "type": "Declaration",
  "fields": {
    "name": "answer",
    "value": "42"
  }
}
```

Source spans are omitted here for brevity.

Reusable pipes make comma-separated lists just as straightforward:

```yl
import { separatedBy } from "std/parser"

pattern Arguments = Expression* |> separatedBy(",")
```

Syntax-local constraints can also add errors, warnings, and help messages to a language. Explore the [MiniJS example](./examples/mini-js/) for expressions, functions, calls, precedence, and comments working together.

## Compose meaning

Language-authored semantics connect syntax to reusable libraries for scopes, references, and type inference. The [Mini TypeScript example](./examples/mini_typescript/) demonstrates scopes, resolved bindings and inferred types through ordinary imported operations:

```yl
import { annotateWithType } from "./typing"
import { number } from "./types"

node Number = value: /[0-9]+/ {
    semantics {
        annotateWithType(self, number)
    }
}
```

The language owns its primitive type symbols and operator rules; the engine supplies generic identity-keyed queries. Run the complete example from the repository root:

```sh
cargo run --example mini_typescript -- addition
```

It reports `numberSum: number`, `stringSum: string` and the types of the other bindings. The authored `std/types` library tracks type evidence and restrictions through generic queries. Function calls infer independent results under temporary parameter assignments. See the [std/types tutorial](https://gweiermann.github.io/your-language/tutorials/std-types) for the full workflow and limits.

The [compound-types example](./examples/compound-types/) adds structural interfaces, tuples, lists, and explicit generic arguments. Its recursive comparison and source-evidence tracking are implemented in YL. A mismatch discovered through several declarations can underline the original incompatible literal and the final requirement together; see the [structural types tutorial](https://gweiermann.github.io/your-language/tutorials/compound-types).

## Editor support

The VS Code extension provides live diagnostics, formatting and composed syntax examples for YL definitions. A project-local `yl.json` also enables diagnostics and highlighting for programs written in your language. Visual roles stay beside the grammar:

```yl
pattern FunctionName = Name {
    ide {
        highlightAs "function"
    }
}
```

See the [editor guide](https://gweiermann.github.io/your-language/guide/editor) and [IDE metadata reference](https://gweiermann.github.io/your-language/reference/ide).

## Run a language

Install the CLI from the repository:

```sh
cargo install --path . --locked
```

Check and compile the MiniJS definition:

```sh
yl check examples/mini-js/minijs.yl
yl compile examples/mini-js/minijs.yl -o minijs.ylc
```

Format YL definition files or check their layout:

```sh
yl fmt examples/getting-started/language.yl
yl fmt --check examples/getting-started/language.yl
```

Generate an AST or check a source file for diagnostics:

```sh
yl language minijs.ylc ast examples/mini-js/program.js
yl language minijs.ylc check examples/mini-js/invalid.js
yl language minijs.ylc check examples/mini-js/invalid.js --json
```

`ast` writes JSON. `check` shows source snippets and colored underlines; `--json` provides diagnostics for tools. Clean checks are silent, and errors produce a nonzero exit code. Commands also work through `cargo run -- <command>`.

## Documentation

The [VS Code extension](./editors/vscode/) provides highlighting, live definition diagnostics, formatting and copyable example previews through the Rust language server. See the [editor setup guide](https://gweiermann.github.io/your-language/guide/editor) for installation and configuration.

Visit the [Your Language documentation](https://gweiermann.github.io/your-language/) for tutorials, the language reference, and CLI and Rust API guides.

- [Build your first language](https://gweiermann.github.io/your-language/tutorials/first-language)
- [Language reference](https://gweiermann.github.io/your-language/reference/lexical-syntax)
- [CLI reference](https://gweiermann.github.io/your-language/api/cli)
- [Rust API](https://gweiermann.github.io/your-language/api/rust)

To run the documentation locally:

```sh
npm ci
npm run docs:dev
```

## Roadmap

Planned development extends the syntax, AST, and diagnostic foundation across four areas:

- **Language analysis:** generalize the experimental native semantic libraries with richer type inference, flow analysis and reusable analysis protocols.
- **Developer tooling:** extend the YL language server with navigation and completion, then explore tooling for languages defined in YL and incremental analysis.
- **Execution and integration:** parser generation, runtime performance, and bindings for additional host environments.
- **Language authoring:** expand grammar inspection, reusable libraries and parser-validated examples.

See the [roadmap](https://gweiermann.github.io/your-language/roadmap) for more detail.
