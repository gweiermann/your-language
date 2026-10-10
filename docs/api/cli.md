# Command-line interface

The executable is `yl`. Definition commands read `.yl` modules; language tools read a compiled `.ylc` artifact and a target-source file.

## Validate a definition

```sh
yl check <entry.yl>
```

Loads the entry module and its imports, parses YL, resolves declarations, validates grammar and constraints, and expands patterns and pipes. A valid definition prints `Language definition is valid` to stdout. Definition diagnostics are rendered to stderr with source locations and terminal underlines. This command does not write an artifact.

## Compile a definition

```sh
yl compile <entry.yl> -o <language.ylc>
```

Performs the same validation and writes the normalized artifact. The output file's parent directory must exist. Non-fatal definition diagnostics are printed to stderr; success prints `Compiled <language.ylc>` to stdout. The artifact is usable without the original `.yl` modules.

## Generate an AST

```sh
yl language <language.ylc> ast <source>
```

Loads and validates the artifact, reads UTF-8 source, and writes the AST as pretty JSON to stdout. Diagnostic arrays, when present, are written as JSON to stderr. A syntax match failure writes `null` to stdout. Syntax-local constraint errors can retain a successfully constructed AST while still causing a nonzero exit.

There is no human AST mode: JSON provides a stable structure for downstream tools. The AST command does not wrap the tree inside a diagnostic envelope; see [AST representation](/api/ast).

## Check source

```sh
yl language <language.ylc> check <source>
yl language <language.ylc> check <source> --json
```

Both forms run the same parser and syntax-local constraints as AST generation, but output only diagnostics.

The default form writes human diagnostics to stderr. Source snippets show line numbers and underlined ranges; errors are red and warnings/help are yellow when stderr is a terminal. Redirected output uses plain text. A clean check emits no output.

`--json` writes a diagnostic array to stdout, including `[]` for a clean source. No AST is included. Artifact and file I/O errors use the same selected diagnostic format.

## Exit codes and streams

| Command | Successful stdout | Diagnostic stream | Error exit |
| --- | --- | --- | --- |
| `check <entry.yl>` | Validation message | Human stderr | Nonzero |
| `compile <entry.yl> -o <file>` | Compilation message | Human stderr | Nonzero |
| `language <file> ast <source>` | AST JSON | JSON stderr | Nonzero |
| `language <file> check <source>` | Empty | Human stderr | Nonzero |
| `language <file> check <source> --json` | Diagnostic array | Included on stdout | Nonzero |

Warnings and help messages alone do not cause failure. Invalid command arguments produce a usage diagnostic. The CLI accepts one source file per invocation and has no target-language execution command.

## Repository examples

```sh
cargo run -- check examples/mini-js/minijs.yl
cargo run -- compile examples/mini-js/minijs.yl -o minijs.ylc
cargo run -- language minijs.ylc ast examples/mini-js/program.js
cargo run -- language minijs.ylc check examples/mini-js/invalid.js
```

`cargo run --quiet -- ...` suppresses Cargo's own build messages. It does not alter YL output.
