# VS Code extension

Your Language provides highlighting, live diagnostics, formatting and composed example hints for `.yl` definitions. The editor connects to the Rust language server through `yl lsp`; current buffers, including unsaved imports, participate in analysis.

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

You can also select **Extensions: Install from VSIXâ€¦**. Reload the window after upgrading. The extension is packaged locally, without a Marketplace release.

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


Explicit `example "..."` metadata takes priority over bounded generation. The generator composes examples across nodes and patterns, omits optional parts, and includes one item for both `*` and `+`. Every candidate is parser-validated. Unsupported or nonterminating cases receive no hint. Meaning operations are never executed during editing.

## Experimental scope

The initial server supports YL definitions, UTF-16 positions, full-document synchronization, diagnostics, formatting, CodeLens, inlay hints and imported-file invalidation. Untitled buffers support syntax diagnostics and formatting; save them for import resolution. Completion, navigation, rename, incremental parsing and target-language servers remain future work.

Definition validation registers `std/syntax`, `std/variables` and `std/types` contracts. Unavailable custom native libraries produce diagnostics. The CLI [`fmt`](/api/cli#format-yl-definitions) and `examples` commands remain independent tools for scripts and agents.

Selected trivia accepting one space permits readable spacing between grammar fragments. No keyword rules are inferred; parser validation and bounded unspaced fallbacks preserve adjacency requirements.
