# CLI and editor changes

Read this guide when changing commands, project discovery, editor integration, or packaging.

- Use [CLI commands](../docs/api/cli.md), [project discovery](../docs/api/projects.md), and [editor behavior](../docs/guide/editor.md) as the shared public contracts. Keep editor and CLI use of `yl.json` consistent.
- Keep human diagnostics and JSON output driven by the same structured result. Cover precise source spans, imported definitions, and unsaved buffers when affected; see [diagnostics](../docs/api/diagnostics.md).
- Run the extension's tests when changing LSP, formatting, project activation, tokens, examples, or extension code. Follow [documentation development](../docs/guide/documentation.md) for documentation grammar and link checks.
- Build packages for review when relevant. Install extensions or change user editor configuration only within the user's requested scope; preserve unrelated settings.
