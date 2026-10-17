# VS Code extension

Your Language provides highlighting, live diagnostics, formatting and composed example hints for `.yl` definitions. The editor connects to the Rust language server through `yl lsp`; current buffers, including unsaved imports, participate in analysis. Project configurations also enable diagnostics and semantic highlighting for programs written in a YL-defined language.

Highlighting includes the `=>` operator in both match cases and expression callbacks, together with `when`, `else`, `ide`, `highlightAs`, and `self`. Strings, regexes, and comments retain their own highlighting even when they contain these tokens. Upgrade and reload an older installed VSIX to pick up grammar changes.

## Install or upgrade

Build the CLI and extension from the same revision:

```sh
cargo install --path . --locked
cd editors/vscode
npm ci
npm test
npm run package
code --install-extension your-language.vsix --force
```

You can also select **Extensions: Install from VSIXÃ¢â‚¬Â¦**. Reload the window after upgrading. The extension is packaged locally, without a Marketplace release.

## Configure

`yourLanguage.cliPath` selects the executable, defaulting to `yl` on PATH. Use an absolute path without arguments or shell quotes. In remote workspaces, install the executable in the remote environment. Highlighting works in untrusted workspaces; launching the server requires trust.

Open a module declaring `entry` to discover its import graph automatically. Set `yourLanguage.exampleEntry` to an absolute entry `.yl` path when editing helper modules separately. Unsaved imported buffers override files on disk, and closing a buffer restores its disk contents.

Run **Format Document** or enable formatting on save:

```json
{
  "[yl]": {
    "editor.defaultFormatter": "gweiermann.your-language",
    "editor.formatOnSave": true
  }
}
```

The server uses the buffer formatter directly. Diagnostics appear as underlines and in Problems. Changes invalidate stale responses; cancellation suppresses responses while in-progress analysis completes.

## Example hints

Examples appear above each declaration as a CodeLens label, such as `example: let name = 42 ;`. Click the label to copy the complete example. This is an editor overlay and does not insert a comment into the source. Long labels are shortened and line breaks are displayed as `\n`; copying preserves the full original text.

`yourLanguage.exampleDisplay` selects `above` (default), `inline` or `off`. Above-line previews require `editor.codeLens` to be enabled. The inline option uses inlay hints; the two displays are never shown together. Hiding examples leaves diagnostics and formatting active.


Explicit `ide { example "..." }` metadata takes priority over bounded generation. The generator composes examples across nodes and patterns, omits optional parts, and includes one item for both `*` and `+`. Every candidate is parser-validated. Unsupported or nonterminating cases receive no hint. Meaning operations are never executed during editing.

## Experimental scope

The initial server supports YL definitions, UTF-16 positions, full-document synchronization, diagnostics, formatting, CodeLens, inlay hints and imported-file invalidation. Untitled buffers support syntax diagnostics and formatting; save them for import resolution. Completion, navigation, rename, incremental parsing remain future work.

Definition validation loads authored `std/references` and `std/types` modules over the generic query primitives; both are ordinary authored YL modules. Unavailable custom native libraries produce diagnostics. The CLI [`fmt`](/api/cli#format-yl-definitions) and `examples` commands remain independent tools for scripts and agents.

Selected trivia accepting one space permits readable spacing between grammar fragments. No keyword rules are inferred; parser validation and bounded unspaced fallbacks preserve adjacency requirements.


## Programs in a language project

Place `yl.json` next to your language's `definition/` and `programs/` folders:

```json
{
  "entry": "./definition/language.yl",
  "files": ["programs/**/*.txt"]
}
```

Open a matching program in a trusted workspace. The extension discovers the nearest configuration and selects **Your Language Program** for that document. Its diagnostics use your compiled grammar, syntax checks and authored semantic queries. Highlighting comes from the grammar's [IDE metadata](/reference/ide); the editor theme supplies colors. Unmatched TypeScript and JavaScript files retain their normal language support. This is a document-specific association, not a global file-extension setting. Removing a configuration or changing its globs restores the original language for open documents.

The entry and globs are relative to `yl.json`. `*` matches characters within a directory, `**` crosses directories, and `?` matches one character. `programs/**/*.ts` includes both direct and nested children. Parent traversal and bracket/brace glob alternatives are rejected. A nearer configuration always takes precedence; an invalid nearer file does not silently select an outer project.

Changing an open YL module or configuration invalidates dependent programs. Open buffers override disk; closing them returns to disk contents. The server caches each compiled module graph while the graph's source texts remain unchanged. Program diagnostics execute authored YL semantic queries; definition example previews continue to validate parsing only. Missing entries and invalid configuration files produce structured diagnostics. Syntax errors in programs retain highlighting for the consistent parsed prefix.

For example, open `examples/08-mini-typescript/programs/program.txt`. The function declaration's name uses the `function` category through a contextual pattern, while ordinary references use `variable`. Another runnable project is `examples/12-diagnostic-origins/programs/invalid-scattered.txt`, whose grouped diagnostic points at the original incompatible value and the requirement. The CLI discovers the same project:

```sh
cargo run --quiet -- project check examples/08-mini-typescript/programs/program.txt
cargo run --quiet -- project check examples/12-diagnostic-origins/programs/invalid-scattered.txt --json
cargo run --quiet -- project ast examples/08-mini-typescript/programs/program.txt
```

A clean check is silent (or `[]` with `--json`). An invalid program exits nonzero; `ast` prints only the AST JSON and writes diagnostics as JSON to stderr. The target language's formatter is not inferred from its grammar: Format Document remains available for `.yl` definitions only.
