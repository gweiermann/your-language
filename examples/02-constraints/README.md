# Syntax diagnostics

Optional syntax, warnings, and errors.

```powershell
cargo run -- project check examples/02-constraints
cargo run -- project list examples/02-constraints --json
```

Open `yl.json` for the language entrypoint, program globs, and intentional error
fixtures. `definition/` holds YL modules; `programs/` holds target `.txt` sources.
Use `yl project check <source>` for a single diagnostic or `yl project ast <source>`
for JSON AST output. An intentional error exits with status 1 when checked alone;
the complete project check verifies that it fails as expected.

## Run this example

```powershell
cargo run --example 02-constraints
# Equivalent friendly name:
cargo run --example constraints
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
