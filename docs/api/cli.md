# Command-line interface

The executable is `yl`. Definition commands read `.yl` modules; language tools read a compiled `.ylc` artifact and a target-source file.

## Format YL definitions

```sh
yl fmt <files...>
yl fmt --check <files...>
yl fmt --check --json <files...>
yl fmt --json <files...>
```

`fmt` formats the supplied UTF-8 YL definition files in place. It uses four-space indentation, spaces around operators, separate declaration paragraphs, and expanded blocks. A single grammar unit stays after `=` on the declaration line. Sequences place each unit on a separate indented line; alternatives start separate lines with `|`. Captures, grouped expressions, postfix operators and pipes stay together. A body following a multiline grammar starts with `{` on its own line. Declaration containers begin with their first child immediately after the opening brace. Sibling declarations and parent `semantics`, `precedence`, and `constraints` sections following nested declarations are separated by a blank line. Comments and the raw spelling of strings and regexes are preserved. Nonempty output ends with a newline. There is no configurable line-width policy.

Meaning-match `=>` arrows align to the widest single-line header in each match block. Each block is independent; rewrite arrows retain their usual spacing. Tuple alternatives and single/block actions are supported. Literal, regex, and comment contents are preserved. Headers split across lines or containing raw tabs retain their ordinary spacing. Unicode displayed widths use the non-CJK convention of [unicode-width](https://docs.rs/unicode-width/0.2.2/unicode_width/trait.UnicodeWidthStr.html); font-dependent rendering can differ. Alignment permits at most one MiB of added spaces per file, otherwise returns `yl.format_alignment_limit` without writing.

`fmt`, `fmt --check`, and the Rust `format_yl` API all use this aligned canonical layout. The earlier `--align-match-arrows` flag remains accepted for compatibility; it is no longer needed. See `examples/13-formatting` for a reproducible before/after comparison.

`--check` makes no writes. Human output prints `Would format <file>` and renders a `yl.format_required` warning at the first layout difference, with a source underline and help. Check mode exits 1 when any file needs formatting, even though formatting warnings are separate from syntax errors. A clean human check is silent. Write mode prints `Formatted <file>` only for files it changes. Syntax and I/O errors use structured error diagnostics and exit 1.

`--json` emits one report on stdout and suppresses human stdout/stderr. Flags can appear before or after file paths; `--` ends option parsing. Use explicit paths or your shell's glob expansion.

```json
{
  "check": true,
  "success": false,
  "files": [
    {
      "path": "language.yl",
      "status": "would_change",
      "would_change": true,
      "changed": false
    }
  ],
  "diagnostics": [
    {
      "severity": "warning",
      "code": "yl.format_required",
      "message": "This file needs formatting",
      "primary": { "file": "language.yl", "start": 6, "end": 7 },
      "secondary": [],
      "help": "Run yl fmt without --check to apply the canonical layout"
    }
  ]
}
```

The report's `success` matches the exit status. Each unique canonical file appears once, using its first supplied path. `status` is `clean`, `would_change` (check mode), `changed` (write completed), `error`, or `skipped` (a write prevented by another batch error). `would_change` is null when syntax or reading failed before a comparison could be made. `changed` records actual completed replacements, including replacements before a later runtime I/O failure. Diagnostics include formatting requirements, source syntax errors, argument errors and file I/O errors. A clean JSON check still emits a successful report with an empty diagnostics array.

The formatter validates YL syntax without loading imports or running semantic operations. Within `semantics`, the standalone structural `boundary` marker is placed first; each operation, group, match and precedence block starts its own line. Moving that marker does not reorder operations. Before returning output, the formatter checks that all raw token spellings and their order (apart from this marker placement) are preserved and that the parsed module is unchanged apart from source spans. Syntax errors produce structured diagnostics rendered through the normal CLI and leave the whole batch untouched. Changed files are replaced through a temporary file in the same directory, preserving file permissions; a file changed concurrently is rejected before replacement.

The command formats YL definitions. Formatting programs written in a target language is a separate language-specific capability. Editor integrations can reuse the [pure Rust formatting interface](/api/rust#format-yl).

## Validate a definition

```sh
yl check <entry.yl>
```

Loads the entry module and its imports, parses YL, resolves declarations, validates grammar and constraints, and expands patterns and pipes. A valid definition prints `Language definition is valid` to stdout. Definition diagnostics are rendered to stderr with source locations and terminal underlines. This command does not write an artifact.

## Compile a definition

```sh
yl compile <entry.yl> -o <language.ylc>
```

Performs the same validation and writes the indexed executable image. The output file's parent directory must exist. Non-fatal definition diagnostics are printed to stderr; success prints `Compiled <language.ylc>` to stdout. The artifact is usable without the original `.yl` modules.

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
| `fmt <files...>` | Changed-file messages | Human stderr | Nonzero |
| `fmt --check <files...>` | Files needing formatting | Human stderr | Nonzero |
| `fmt [--check] --json <files...>` | Formatting report | Included on stdout | Nonzero |
| `check <entry.yl>` | Validation message | Human stderr | Nonzero |
| `compile <entry.yl> -o <file>` | Compilation message | Human stderr | Nonzero |
| `language <file> ast <source>` | AST JSON | JSON stderr | Nonzero |
| `language <file> check <source>` | Empty | Human stderr | Nonzero |
| `language <file> check <source> --json` | Diagnostic array | Included on stdout | Nonzero |

Warnings and help messages alone do not cause failure in language checking. Formatter check mode exits 1 when layout changes are required, independently of diagnostic severity. Invalid command arguments produce a usage diagnostic. Language tools accept one target-source file per invocation; the formatter accepts multiple YL definition files.

## Repository examples

```sh
cargo run -- check examples/03-mini-js/definition/minijs.yl
cargo run -- compile examples/03-mini-js/definition/minijs.yl -o minijs.ylc
cargo run -- language minijs.ylc ast examples/03-mini-js/programs/program.txt
cargo run -- language minijs.ylc check examples/03-mini-js/programs/invalid.txt
```

`cargo run --quiet -- ...` suppresses Cargo's own build messages. It does not alter YL output.

## Language server

```sh
cargo run -- lsp
```

`yl lsp` serves YL authoring over standard LSP stdio framing. Standard output is
reserved for protocol messages. It supports full-document synchronization, live
syntax/definition diagnostics, buffer formatting and parser-validated example
CodeLens labels (above declarations) or optional inline inlay hints. See [editor setup](/guide/editor) for installation, unsaved imports and current
native-library limits. Target-language servers and incremental parsing are not
provided by this command.

## Representative examples

```sh
cargo run -- examples examples/04-ide-metadata/definition/language.yl
cargo run -- examples language.ylc
```

`yl examples` emits JSON with definition IDs, source spans, validated text and reasons
for unavailable previews. Explicit node/pattern `example "..."` metadata takes priority
within the experimental bounded generator.


## Check a project program

```sh
yl project check examples/08-mini-typescript/programs/program.txt
yl project check examples/12-diagnostic-origins/programs/invalid-scattered.txt --json
yl project ast examples/08-mini-typescript/programs/program.txt
```

The nearest `yl.json` selects the entry and project-relative file globs. The source must match that configuration. The CLI compiles the entry in memory; there is no intermediate artifact file to manage. `check` runs parsing, syntax checks and authored semantic queries, with the same diagnostics as the editor. A clean check is silent, JSON checks emit a diagnostic array, and errors exit nonzero. `ast` prints the parsed AST as JSON and emits any parse diagnostics as JSON to stderr. Project discovery, malformed configuration and missing language entry errors follow the requested JSON/human format. See [project configuration](/guide/editor#programs-in-a-language-project).

## Inspect backend hooks

```sh
cargo run -- language target/backend.ylc hooks
cargo run -- language target/backend.ylc hooks --json
```

Lists the artifact's module-qualified hook names and positional parameter contracts. JSON output includes the declaration spans. No hook is bound or executed. The command works with a checked `.ylc` independently of its original YL source files. See [backend hooks](/reference/backends) for consumer binding and execution.
