# Your Language for VS Code

The extension provides YL syntax highlighting, live definition diagnostics, formatting and parser-validated examples above declarations. A reusable Rust language server runs as `yl lsp` over stdio; unsaved buffers and their imported modules are analyzed together.

## Install or upgrade

Build the CLI and extension from the same repository revision:

```sh
cargo install --path . --locked
cd editors/vscode
npm ci
npm test
npm run package
code --install-extension your-language.vsix --force
```

Alternatively select **Extensions: Install from VSIXÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦**. Reload the VS Code window after upgrading. Packages are local; the extension is not published to the Marketplace.

## Configuration

The extension launches `yl lsp` from the extension host PATH. Set `yourLanguage.cliPath` to an absolute executable path if needed, without shell quotes or arguments. Remote workspaces require the executable in the remote environment. Workspace trust is required to launch the server; TextMate highlighting works without it.

Open an entry module to discover its import graph automatically. For editing helper modules independently, set `yourLanguage.exampleEntry` to the absolute entry `.yl` path. This existing setting now selects the analysis root and supports unsaved imports. Empty means automatic detection of open modules declaring `entry`. Changes to `.yl` files on disk trigger graph reanalysis.

Run **Format Document**, or configure:

```json
{
  "[yl]": {
    "editor.defaultFormatter": "gweiermann.your-language",
    "editor.formatOnSave": true
  }
}
```

Formatting uses the canonical four-space Rust formatter directly on the buffer. Invalid syntax leaves it unchanged. Diagnostics appear in Problems and editor underlines. Examples compose valid node/pattern examples, omit optional parts and include one item for `*` and `+`; explicit `ide { example "..." }` metadata takes priority. Generation never runs semantics. Unsupported or nonterminating cases have no hint.

## Example display

Examples appear above each declaration as a CodeLens label, such as `example: let name = 42 ;`. Click the label to copy the complete example. This is an editor overlay and does not insert a comment into the source. Long labels are shortened and line breaks are displayed as `\n`; copying preserves the full original text.

`yourLanguage.exampleDisplay` selects `above` (default), `inline` or `off`. Above-line previews require `editor.codeLens` to be enabled. The inline option uses inlay hints; the two displays are never shown together. Hiding examples leaves diagnostics and formatting active.

## Current scope

The server handles YL definitions, file and untitled buffers, full-document synchronization, UTF-16 positions, cancellation, stale-result rejection, dependency invalidation, diagnostics, formatting, CodeLens and optional inlay hints. It performs full reanalysis, not incremental parsing. Cancellation suppresses the response; in-progress compiler work completes before the next request. The stdio transport assumes a trusted local client and does not impose a separate message or document allocation limit. Disk imports are read only within discovered graphs. Untitled buffers get syntax diagnostics and formatting; save them before import resolution or example generation.

The authored `std/references` and `std/types` libraries and generic `core/query` primitives are available for definition checking. contextual `self` and captures identify syntax directly. Other host-provided native libraries must remain in their host consumer: unavailable contracts produce diagnostics, rather than being guessed or executed. Completion, navigation and rename remain future work.

The CLI `fmt` and `examples` commands remain available independently. The extension uses `vscode-languageclient` and does not bundle Rust. See [the metadata experiment](../../examples/example_metadata/README.md) for generator limits.

## Development

Open `editors/vscode` and press **F5**. The build copies the grammar from `docs/.vitepress/yl-grammar.json`. Tests use a real Rust server and Node JSON-RPC client:

```sh
cargo build --manifest-path ../../Cargo.toml --bin yl
npm test
```

Set `YL_TEST_CLI` to a different absolute executable path when testing another build. CI runs Rust, client, protocol and packaging checks on Windows and Linux.

Selected trivia accepting one space permits readable spacing between grammar fragments. No keyword rules are inferred; parser validation and bounded unspaced fallbacks preserve adjacency requirements.

Highlighting covers `semantics`, `checks`, reactive `when`, immutable `:=` definitions, operation `return`, and qualified enum variants. Engine queries and diagnostics are imported from `core/query` and `core/diagnostics`.


## Language projects

A project-local `yl.json` enables diagnostics and semantic tokens for matching source files:

```json
{
  "entry": "./definition/language.yl",
  "files": ["programs/**/*.ts"]
}
```

The nearest configuration wins. Paths/globs are relative to it; supported glob operators are `*`, `**` and `?`. The extension activates when a workspace contains `yl.json`, even before a YL definition is opened. Matching documents select **Your Language Program**, while unrelated TypeScript/JavaScript files keep their existing language support. Changes and discarded unsaved configuration edits restore associations as needed. No global file associations are written.

Highlight roles belong in YL, beside the grammar:

```yl
pattern FunctionName = Name {
    ide {
        highlightAs "function"
    }
}
```

Contextual wrappers override inner categories; editor themes choose colors. Metadata survives `.ylc` reload. Open language modules and configurations override disk files during program analysis, and unchanged compiled module graphs are reused. Programs receive parser, syntax-check and authored semantic diagnostics. Program formatting is not provided; YL definitions retain the existing formatter.

The CLI uses the same configuration with `yl project check <source> [--json]` and `yl project ast <source>`. See [the editor guide](../../docs/guide/editor.md) for runnable examples.
