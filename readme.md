# Your Language

**Define a language. Compile it once. Parse it anywhere.**

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

Generate an AST or check a source file for diagnostics:

```sh
yl language minijs.ylc ast examples/mini-js/program.js
yl language minijs.ylc check examples/mini-js/invalid.js
yl language minijs.ylc check examples/mini-js/invalid.js --json
```

`ast` writes JSON. `check` shows source snippets and colored underlines; `--json` provides diagnostics for tools. Clean checks are silent, and errors produce a nonzero exit code. Commands also work through `cargo run -- <command>`.

## Documentation

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

- **Language analysis:** relations, traits, scopes, declaration and reference resolution, and semantic diagnostics.
- **Developer tooling:** editor integration, language-server support, and incremental parsing.
- **Execution and integration:** parser generation, runtime performance, and bindings for additional host environments.
- **Language authoring:** grammar inspection, reusable libraries, and node metadata.

See the [roadmap](https://gweiermann.github.io/your-language/roadmap) for more detail.
