# Tuple and list positions

`pair[1]` reads the second tuple element. `names[0]` reads the first captured list element. Indexing preserves syntax identity, so diagnostics can underline the selected name.

```powershell
cargo run -- compile examples/05-tuple-access/definition/language.yl -o target/tuple-access.ylc
cargo run -- language target/tuple-access.ylc check examples/05-tuple-access/programs/valid.txt
cargo run -- language target/tuple-access.ylc check examples/05-tuple-access/programs/invalid.txt
```

The valid program succeeds. The invalid program reports `Unexpected wrong` and underlines `wrong`. Add `--json` to the final command for structured diagnostic data. Empty collections and missing positions return `absent`; indexes are zero-based nonnegative integer literals.

## Run this example

```powershell
cargo run --example 05-tuple-access
# Equivalent friendly name:
cargo run --example tuple-access
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
