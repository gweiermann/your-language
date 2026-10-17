# Declarative functions

Function typing through authored queries, with the addition policy reused from
the opt-in `std/types/arithmetic` library.

This lesson builds on 08's visible addition matrix and 09's isolated hypothetical
assignments. `definition/typing.yl` keeps the function signature and return
queries local so their dependencies remain visible. `definition/expression.yl`
passes its language-owned `number` and `string` identities to the reusable pure
`additionType` operation. It annotates a supported result or emits its own
diagnostic for an unsupported operand pair. The standard policy chooses the
combination; this language still chooses storage, source locations, and type names.

```yl
result := additionType(typeOf(left), typeOf(right), number, string)
when isPresent(result) {
    annotateWithType(self, result)
}
else {
    error("${left} + ${right} is not possible", self)
}
```

These actions live directly beside the `Addition` captures; the imported
operations and symbols are declared at the top of that same file. Each
hypothetical function call recomputes the result with its own argument type.
The earlier lesson implements the policy; this lesson composes it.

`programs/addition.txt` combines generic calls with numeric and string additions.
`programs/invalid-addition.txt` deliberately adds a function identity to a number
and underlines that addition. This is not a full JavaScript coercion policy.

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
