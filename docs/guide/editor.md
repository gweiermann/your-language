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

You can also select **Extensions: Install from VSIX…**. Reload the window after upgrading. The extension is packaged locally, without a Marketplace release.

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

Explicit `example "..."` metadata takes priority over bounded generation. The generator composes examples across nodes and patterns, omits optional parts, and includes one item for both `*` and `+`. Every candidate is parser-validated. Unsupported or nonterminating cases receive no hint. Meaning operations are never executed during editing.

## Experimental scope

The initial server supports YL definitions, UTF-16 positions, full-document synchronization, diagnostics, formatting, inlay hints and imported-file invalidation. Untitled buffers support syntax diagnostics and formatting; save them for import resolution. Completion, navigation, rename, incremental parsing and target-language servers remain future work.

Definition validation registers `std/variables` contracts. Unavailable custom native libraries produce diagnostics. The Mini TypeScript example's host-defined `std/types` contract is not yet installed in the server. The CLI [`fmt`](/api/cli#format-yl-definitions) and `examples` commands remain independent tools for scripts and agents.
