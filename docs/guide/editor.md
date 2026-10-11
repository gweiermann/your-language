# VS Code extension

The Your Language extension provides syntax highlighting and CLI-backed formatting for `.yl` definitions. It registers **Your Language** as a language mode and formatter, including unsaved buffers.

## Install from source

First install the CLI from a repository revision that includes `yl fmt`:

```sh
cargo install --path . --locked
```

Build and install the extension:

```sh
cd editors/vscode
npm ci
npm test
npm run package
code --install-extension your-language.vsix
```

VS Code also accepts the package through **Extensions: Install from VSIX…**. Packages are built locally; there is no Marketplace release yet.

## Configure formatting

Run **Format Document** on a YL definition, or enable format on save:

```json
{
  "[yl]": {
    "editor.defaultFormatter": "gweiermann.your-language",
    "editor.formatOnSave": true
  }
}
```

The extension looks for `yl` on `PATH`. Set `yourLanguage.cliPath` to an absolute executable path if needed. Paths with spaces are supported; do not add shell quotes or arguments. In a remote workspace, the CLI must be installed in the remote environment.

The extension formats a temporary copy of the current buffer through [`yl fmt`](/api/cli#format-yl-definitions). It returns an editor edit only after successful completion and discards results when the buffer changes meanwhile. Failed requests leave the document unchanged; **Show Details** opens the CLI diagnostics in the **Your Language** output channel.

Highlighting works without a CLI and in untrusted workspaces. Formatting requires workspace trust and a CLI compatible with the definition's syntax. The extension supports file and untitled documents in desktop VS Code, including remote extension hosts. It does not format target-language programs or provide language-server features.

## Experimental representative examples

The example-metadata experiment adds optional `example "..."` metadata and
parser-validated inline previews. Configure `yourLanguage.exampleEntry` with an
absolute entry `.yl` path and use a CLI that supports `yl examples`. Previewing
requires a trusted workspace and saved definitions. The bounded generator omits
optional parts, includes one item for `*` and `+`, and returns no preview if it cannot
find a valid terminating representative. Explicit examples take priority.
