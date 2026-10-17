# Formatting

A complete small language demonstrates aligned match arms. Its operation imports,
type tags, trivia, and number semantics are defined, so it also checks normally.

```powershell
cargo run -- fmt --check examples/13-formatting/definition/match-arrows.yl
cargo run -- project check examples/13-formatting
```

The first command succeeds on the checked-in formatted file. To experiment, change
its spacing, then run `yl fmt <file>` to write canonical formatting. Match arrows
align within each block; pipes remain on their pattern line. Project checks run
`programs/valid.txt` using the entry in `yl.json`.

## Run this example

```powershell
cargo run --example 13-formatting
# Equivalent friendly name:
cargo run --example formatting
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
