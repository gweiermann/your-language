# Examples

Each numbered directory is a complete YL project. Open its `.txt` programs in VS
Code to use that project's grammar, diagnostics, and highlighting. Definitions
remain `.yl`; target programs use `.txt` so installed JavaScript or TypeScript
extensions do not select a different language.

Run commands from the repository root:

```powershell
cargo run --example getting-started
cargo run --example 01-getting-started
cargo run -- project list examples
cargo run -- project list examples --json
cargo run -- project check examples/11-compound-types
cargo run -- project check examples/11-compound-types/programs/valid.txt
cargo run -- project ast examples/11-compound-types/programs/valid.txt
```

`yl.json` is the entrypoint for every project. A project-level check verifies the
language and every matching program. Its `expectErrors` patterns identify
intentional negative fixtures: the aggregate command succeeds only when valid
programs pass and these fixtures fail. Checking a negative source directly still
exits with status 1 and displays the error. Add `--json` to either check form for
structured output. The project runner uses the same language/compiler APIs as the
editor; the Cargo entrypoints below are thin wrappers around that same in-process runner.

| Project / numbered Cargo name | Friendly Cargo name | What it teaches |
| --- | --- | --- |
| [01-getting-started](./01-getting-started/README.md) | `getting-started` | Captures, trivia, and JSON ASTs |
| [02-constraints](./02-constraints/README.md) | `constraints` | Optional syntax, warnings, and errors |
| [03-mini-js](./03-mini-js/README.md) | `mini-js` | Modules, recursive grammar, precedence, and pipes |
| [04-ide-metadata](./04-ide-metadata/README.md) | `ide-metadata` | Composed examples and contextual highlighting |
| [05-tuple-access](./05-tuple-access/README.md) | `tuple-access` | Positional tuple and list queries |
| [06-references](./06-references/README.md) | `references` | Authored lexical scopes and name connections |
| [07-declarative-queries](./07-declarative-queries/README.md) | `declarative-queries` | Dependency discovery and nearest-scope lookup |
| [08-mini-typescript](./08-mini-typescript/README.md) | `mini-typescript` | Inference, functions, and isolated generic calls |
| [09-type-instantiation](./09-type-instantiation/README.md) | `type-instantiation` | Hypothetical parameter assignments |
| [10-declarative-functions](./10-declarative-functions/README.md) | `declarative-functions` | Function typing through authored queries |
| [11-compound-types](./11-compound-types/README.md) | `compound-types` | Interfaces, tuples, lists, generics, and assignments |
| [12-diagnostic-origins](./12-diagnostic-origins/README.md) | `diagnostic-origins` | Optional source tracing and grouped diagnostics |
| [13-formatting](./13-formatting/README.md) | `formatting` | Runnable match-arm alignment demonstration |
| [14-backend-hooks](./14-backend-hooks/README.md) | `backend-hooks` | Interpret, transpile to JavaScript, and compile to WebAssembly |

Use `cargo run --example <name>` with either name in the table. Each launcher
checks valid programs and expected errors; it does not execute backend hooks.
For the formatting example, the README also shows `yl fmt` to inspect or change
formatting.

## Follow the implementation, then reuse it

Start with 01–05 for syntax, diagnostics, IDE roles, and query values. These
examples expose the mechanism being taught. MiniJS deliberately reuses
`std/parser` for list separators and boundaries; its local `keyword(value)`
pattern supplies the language's identifier policy and keyword highlighting.

For the mechanism-first reference lesson, read 07 and then 06: 07 builds a small
reference library directly on `core/query`; 06 connects a larger grammar to
`std/references`. The existing catalog numbers and commands remain stable.
Read 07's `definition/references.yl` to see scope identities,
declaration keys, nearest-scope lookup, and diagnostics without a hidden native
adapter. Its whole-scope visibility is an explicit teaching policy; 06 uses the
standard library's schedule-sensitive visibility.

Then 08 builds function inference with `std/references` and `std/types`, while
its own `typing.yl` teaches addition rules. After isolated instantiation in 09,
10 demonstrates the lower-level function queries and reuses the explicit
`std/types/arithmetic` policy instead of reimplementing its addition matrix.
Example 11 builds structural comparison and generic type policies locally.
Example 12 reuses that descriptor model through `std/types/structural` and adds
diagnostic origins separately. Finally, 14 composes static references and type
checks with `std/backend/integer`, implemented by three independent hosts.

Keep each construct's grammar and its short `semantics`, `backend`, and `ide`
connections together. Put substantial reusable operations in a supporting
module and pass captures explicitly. Files are grouped by construct, rather
than augmenting grammar declarations from distant metadata files.

The opinionated modules are opt-ins: the generic reference and type libraries
do not assume a target language's primitive types or operators. Mini TypeScript
demonstrates selected features rather than full TypeScript. The checked backend
language is a separate all-`i32` subset, not a runtime for the structural type
example. Follow [the curriculum](../docs/tutorials/language-curriculum.md) for
the full mechanism-to-capstone route.

## Extend the curriculum

Improve the existing lesson that owns a concept before adding another project.
Each lesson should name its objective, prerequisites, locally implemented rules,
and imported abstractions. A new numbered project needs a distinct learning
objective; preserve the common `yl.json` runner and both Cargo names. Keep
target programs as `.txt`, include intentional errors where relevant, and give
commands with expected results. More advanced lessons should compose earlier
mechanisms rather than duplicate them or hide the entire analysis in one call.

## Host API examples

Root `.rs` files are Cargo consumers, not target-language programs, so they keep
Rust filenames. Supporting Rust modules live in each project's `hosts/` directory.
These consumers illustrate additional API use; the YL project itself remains
runnable through the common project commands above.

```powershell
cargo run --example compiled_image
cargo run --example semantic_analysis
cargo run --example mini_typescript_host
cargo run --example type_instantiation_host
cargo run --example backend_interpret
cargo run --example backend_javascript -- examples/14-backend-hooks/programs/control-flow.txt > target/backend.js
node target/backend.js
cargo run --example backend_wasm -- examples/14-backend-hooks/programs/control-flow.txt target/backend.wasm
```

See the backend project's README for WebAssembly execution and error fixtures.
Static project checks do not invoke host hooks: `overflow.txt` tests wrapping
behavior, and `host-error.txt` tests a host range failure. The earlier
`runtime-error.txt` fixture now demonstrates unknown-name rejection during
static analysis.
