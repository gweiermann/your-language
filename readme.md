# Your Language

**Define a language. Compile it once. Parse it anywhere.**

Your Language is a declarative language for building parsers, structured abstract syntax trees, and precise diagnostics. A single definition describes the syntax to recognize, the fields to retain, and the local rules to enforce. The Rust compiler turns that definition into a reusable `.ylc` artifact; the runtime parses source with that artifact without loading the original grammar files.

From configuration formats to expression languages and programming-language syntax, YL makes the relationship between source text and its tree explicit.

## Syntax becomes structure

Named captures become AST fields. Punctuation and uncaptured grammar are consumed without cluttering the tree:

```yl
node Name = value: /[A-Za-z_][A-Za-z_0-9]*/
node Number = value: /[0-9]+/

node Declaration =
    "let" name: Name "=" initializer: Number

node Program = declarations: Declaration*

trivia Whitespace = /[ \t\r\n]+/

entry Program {
    trivia Whitespace
}
```

The source `let answer = 42` becomes a `Program` containing a `Declaration` with `name` and `initializer` nodes. Every node carries its source span.

Reusable patterns and structural pipes express syntax without introducing artificial AST wrappers:

```yl
import { separatedBy } from "std/parser"

pattern Arguments =
    Expression* |> separatedBy(",", trailing=.optional)
```

The pipe handles separators and trailing commas while preserving the list of expressions. Abstract node families describe alternatives, precedence declarations resolve recursive expressions, and syntax-local constraints report errors directly against captured source.

```yl
constraints {
    when name?.matches(/^[A-Z]/) {
        warning("Names should begin with a lowercase letter.")
    }
}
```

Modules, explicit exports, and entry-selected trivia keep large language definitions composable and predictable. The [MiniJS example](./examples/mini-js/) combines expressions, functions, calls, precedence, comments, and reusable list syntax.

## Run a language

Install the CLI from the repository with the pinned Rust toolchain:

```sh
cargo install --path . --locked
```

Validate a language definition and compile a portable artifact:

```sh
yl check examples/mini-js/minijs.yl
yl compile examples/mini-js/minijs.yl -o minijs.ylc
```

Choose the result needed for a source file:

```sh
yl language minijs.ylc ast examples/mini-js/program.js
yl language minijs.ylc check examples/mini-js/invalid.js
yl language minijs.ylc check examples/mini-js/invalid.js --json
```

`ast` writes the AST as JSON. `check` reports diagnostics with source snippets and colored underlines in a terminal; `--json` provides structured diagnostics for tools. A clean source check is silent, and errors produce a nonzero exit code. Every command also works through `cargo run -- <command>` without installing the CLI.

Applications can use the same compiler and runtime through the [Rust API](./docs/api/rust.md).

## Documentation

Start with the [documentation](./docs/index.md), follow the [first-language tutorial](./docs/tutorials/first-language.md), or browse the [language reference](./docs/reference/lexical-syntax.md) and [CLI reference](./docs/api/cli.md).

The documentation site includes guided examples, language rules, standard library reference, artifact details, and integration guidance. To read it locally:

```sh
npm ci
npm run docs:dev
```

## Roadmap

The project develops along four connected areas:

- **Language analysis:** relations, traits, scopes, declaration and reference resolution, and semantic diagnostics.
- **Developer tooling:** editor integration, language-server support, and incremental parsing.
- **Execution and integration:** parser generation, runtime performance, and bindings for additional host environments.
- **Language authoring:** richer tooling for grammar inspection, reusable libraries, and node metadata.

These are planned directions rather than available capabilities. The public compiler and runtime focus on syntax, AST construction, and syntax-local validation. See the [roadmap](./docs/roadmap.md) for the relationship between these areas.
