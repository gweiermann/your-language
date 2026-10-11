# Your Language for VS Code

Edit YL language definitions with syntax highlighting, comment toggling, bracket pairing, and **Format Document** powered by the `yl` CLI. Both saved and unsaved `.yl` buffers can be formatted. The current buffer is copied to a temporary file; the original file is never modified by the formatter process.

## Install

Build and install the CLI from the repository:

```sh
cargo install --path . --locked
```

This extension requires a CLI that supports `yl fmt`. Install a build with the formatter implementation; earlier CLI releases do not provide that command. New YL syntax also requires a CLI that understands that syntax.

From the repository, build the extension package:

```sh
cd editors/vscode
npm ci
npm test
npm run package
code --install-extension your-language.vsix
```

Alternatively, use **Extensions: Install from VSIX…** in VS Code and select `your-language.vsix`. The extension is packaged locally, not published to the Marketplace.

## Format a definition

Open a `.yl` file, then run **Format Document**. If multiple formatters are available, select **Your Language**. To enable formatting on save:

```json
{
  "[yl]": {
    "editor.defaultFormatter": "gweiermann.your-language",
    "editor.formatOnSave": true
  }
}
```

The formatter uses the CLI's canonical four-space layout. Editor indentation preferences do not change that layout. Formatting operates on YL definitions; programs written in languages defined by YL need their own formatter.

By default, the extension looks for `yl` on the extension host's `PATH`. If it is installed elsewhere, configure its absolute executable path:

```json
{
  "yourLanguage.cliPath": "C:\\Users\\Gerrit\\.cargo\\bin\\yl.exe"
}
```

Use an executable path without arguments or shell quotes, including when the path contains spaces. In Remote SSH, WSL, or a development container, install and configure the CLI in that remote environment. The extension runs in the workspace extension host.

Invalid syntax, a missing CLI, or a formatter failure leaves the document unchanged. Choose **Show Details** on the error notification to see CLI diagnostics in the **Your Language** output channel. Formatting is cancelled when requested; results are discarded if the document changes during the request. The CLI has a 30-second execution timeout.

Syntax highlighting is available in untrusted workspaces; CLI formatting requires workspace trust. Only file and untitled YL documents are formatted. Browser-only VS Code and virtual workspaces are not supported.

## Development

Open `editors/vscode` as the workspace and press **F5** to launch an Extension Development Host. The launch task builds the TypeScript code and copies the shared highlighting grammar from `docs/.vitepress/yl-grammar.json`. Edit that source grammar when adding syntax; the generated extension copy is not committed.

Run tests against a real CLI in addition to the process/provider and TextMate tests:

```powershell
cargo build --manifest-path ../../Cargo.toml --bin yl
$env:YL_TEST_CLI = (Resolve-Path ../../target/debug/yl.exe).Path
npm test
```

On Unix, set `YL_TEST_CLI` to the absolute path of `../../target/debug/yl`. CI builds the CLI and runs these integration tests on Windows and Linux. The extension has no runtime npm dependencies and does not bundle the CLI.
