# First language

Captures, trivia, and JSON ASTs.

```powershell
cargo run -- project check examples/01-getting-started
cargo run -- project list examples/01-getting-started --json
```

Open `yl.json` for the language entrypoint, program globs, and intentional error
fixtures. `definition/` holds YL modules; `programs/` holds target `.txt` sources.
Use `yl project check <source>` for a single diagnostic or `yl project ast <source>`
for JSON AST output. An intentional error exits with status 1 when checked alone;
the complete project check verifies that it fails as expected.

## Run this example

```powershell
cargo run --example 01-getting-started
# Equivalent friendly name:
cargo run --example getting-started
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
