# Declarative functions

Function typing through authored queries.

```powershell
cargo run -- project check examples/10-declarative-functions
cargo run -- project list examples/10-declarative-functions --json
```

Open `yl.json` for the language entrypoint, program globs, and intentional error
fixtures. `definition/` holds YL modules; `programs/` holds target `.txt` sources.
Use `yl project check <source>` for a single diagnostic or `yl project ast <source>`
for JSON AST output. An intentional error exits with status 1 when checked alone;
the complete project check verifies that it fails as expected.

## Run this example

```powershell
cargo run --example 10-declarative-functions
# Equivalent friendly name:
cargo run --example declarative-functions
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
