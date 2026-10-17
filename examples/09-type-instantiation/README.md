# Hypothetical type relationships

The identity template relates its result to its parameter through authored YL operations. Each call asks what that result would be under explicit parameter assumptions; the original parameter and unrelated types remain unchanged.

The standard type operations are authored YL. The Rust consumer loads the compiled language and evaluates it without registering template ownership or native type adapters.

```powershell
cargo run --example type_instantiation_host
cargo run --example type_instantiation_host -- errors
cargo run --example type_instantiation_host -- errors --json
```

The successful program prints independent `number` and `string` results. The error program underlines the string argument that cannot satisfy the required number type and exits with status 1. JSON output contains structured diagnostics and query contributions.

## Run this example

```powershell
cargo run --example 09-type-instantiation
# Equivalent friendly name:
cargo run --example type-instantiation
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
