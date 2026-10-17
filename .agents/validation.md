# Validation and handoff

Read this guide before reporting implementation complete or preparing a PR.

Run the Rust gates from the repository root:

```shell
cargo test --all-targets
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

Run the affected integration checks:

| Changed area | Commands |
| --- | --- |
| Public docs or example definitions | `npm run docs:build` and `npm run docs:check-examples` |
| Editor or LSP behavior | `npm --prefix editors/vscode test` |
| Example or project behavior | `cargo run --example <friendly-or-numbered-name>` and the affected [project commands](../docs/api/projects.md) |
| Backend behavior | Actual interpreter, transpiler, and compiler commands in [the backend tutorial](../docs/tutorials/backends.md) |

Check changed behavior through the public API, including `.ylc` reload where compilation is affected. Review intentional golden changes. After checks pass, repeat a check only for a new change, failure, or unresolved concern.

Report the tested revision, commands and outcomes, and any remaining limitations. PRs should describe the final behavior and give a short reproducible example.
