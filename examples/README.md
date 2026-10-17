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

See the backend project's README for WebAssembly execution and runtime-error
fixtures. Static project checks do not invoke host hooks: `runtime-error.txt` and
`overflow.txt` intentionally test runtime behavior in those hosts.
