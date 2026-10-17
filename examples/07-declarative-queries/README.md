# Declarative queries

Dependency discovery and nearest-scope lookup, implemented locally on
`core/query` rather than imported from `std/references`.

- `definition/references.yl` defines the scope, declaration, and reference keys
  and the operations that read and contribute to them.
- `definition/statements.yl` keeps captures beside explicit calls such as
  `declareName(self, name)` and `connectReference(self, name)`.
- `definition/language.yl` selects the entry, trivia, and declaration ordering.

An inner scope's declaration wins over an outer declaration of the same name,
including when the inner declaration comes after its reference. This example
uses `get` for whole-scope lookup. The canonical `std/references` library used
in 06 and 08 uses `getEarlier` for schedule-sensitive visibility and also
provides binding metadata needed by those examples. Neither policy is native
engine behavior.

Start with the small local library here to understand the mechanism; use the
standard library when its policy fits your language. The
[authored scopes tutorial](../../docs/tutorials/declarative-scopes.md) explains
each query and the missing-name and duplicate-name diagnostics.

```powershell
cargo run -- project check examples/07-declarative-queries
cargo run -- project list examples/07-declarative-queries --json
```

Open `yl.json` for the language entrypoint, program globs, and intentional error
fixtures. `definition/` holds YL modules; `programs/` holds target `.txt` sources.
Use `yl project check <source>` for a single diagnostic or `yl project ast <source>`
for JSON AST output. An intentional error exits with status 1 when checked alone;
the complete project check verifies that it fails as expected.

## Run this example

```powershell
cargo run --example 07-declarative-queries
# Equivalent friendly name:
cargo run --example declarative-queries
```

Both entrypoints load `yl.json` and check all configured programs in process.
Intentional negative fixtures print their diagnostics and count as a passing
example only when an error is found. Host execution commands remain separate.
