# Engine source guide

The Rust library compiles a YL module graph into a normalized language and an
executable image. Loading that image does not require the source compiler. The
public Rust entry points remain documented in [the Rust API](../api/rust.md).

## Compilation

`src/compiler.rs` owns the public entry points and the working declaration tables.
`src/compiler/pipeline.rs` gives the order of compilation phases. Phase modules
share that per-compilation state; they do not expose another public host interface.

| Responsibility | Source modules |
| --- | --- |
| Load graphs and canonical bundled modules | `compiler/loading.rs` |
| Register declarations and resolve imports, scopes, and members | `compiler/imports.rs`, `compiler/resolution.rs` |
| Validate source declarations before expansion | `compiler/declarations.rs` |
| Lower concrete grammar, abstract families, and precedence | `compiler/grammar.rs`, `compiler/rules.rs` |
| Expand parameterized grammar, pipe rewrites, and arguments | `compiler/rewrites.rs`, `compiler/arguments.rs`, `compiler/substitution.rs` |
| Expand authored operations without losing argument effects or call-site identities | `compiler/wrappers.rs` |
| Compile recursive query functions, expressions, actions, and bindings | `compiler/query_functions.rs`, `compiler/query_expressions.rs`, `compiler/query_actions.rs`, `compiler/query_arguments.rs` |
| Lower native contracts and scheduling groups | `compiler/native_semantics.rs`, `compiler/semantic_groups.rs`, `compiler/semantic_definitions.rs` |
| Compile syntax checks and validate conditions | `compiler/checks.rs`, `compiler/condition_validation.rs` |
| Compile unevaluated backend hook arguments | `compiler/backend.rs` |
| Preserve highlighting and example templates | `compiler/tooling.rs` |
| Validate normalized artifacts, captures, projections, and value shapes | `compiler/validation.rs`, `compiler/capture_validation.rs`, `compiler/capture_shapes.rs`, `compiler/term_edges.rs` |

Module IDs, declaration paths, and source spans keep their meaning throughout
these phases. Ordered maps preserve deterministic traversal and diagnostic order.
`GrammarArgument` and `GrammarBindings` describe compile-time grammar arguments;
they are distinct from semantic values and host-owned backend values.

The final stage validates the normalized language and compiles `image/` tables.
See [compiled artifacts](../api/artifacts.md) for the serialized contract. Internal
module organization is independent of that artifact format.

## YL parsing

`src/frontend.rs` exposes full-file and embedded-expression parsing. Its parser
owns one token cursor and a depth budget. The files under `src/frontend/` separate
declarations, grammar precedence, semantic expressions/actions, structural
rewrites, and syntax checks. `documentation.rs` attaches documentation after
parsing, so comments do not become grammar expressions. `diagnostic_imports.rs`
recognizes explicit diagnostic imports before declaration parsing, including
forward imports and aliases.

## Target-language parsing

`src/runtime.rs` defines AST results and branch-local match state. Its private
modules execute borrowed parser-image instructions:

| Module | Responsibility |
| --- | --- |
| `runtime/session.rs` | Set up a source parse and publish AST, diagnostics, occurrences, and highlights |
| `runtime/matching.rs` | Match rules, regex terminals, trivia, and backtracking continuations |
| `runtime/instructions.rs` | Execute indexed grammar instructions and structural projections |
| `runtime/precedence.rs` | Execute recursive abstract families with Pratt binding powers |
| `runtime/occurrences.rs` | Construct AST nodes and source-linked semantic occurrences |
| `runtime/checks.rs` | Evaluate syntax-local checks |
| `runtime/highlighting.rs` | Apply contextual highlighting without leaking failed alternatives |

Every attempted match owns its captures, diagnostics, projection markers, and
occurrences. Backtracking discards the attempted match as a whole. Shared
executable tables stay immutable; per-source bookkeeping belongs to the session.

## Tests and changes

Compiler and frontend unit tests live beside their domain modules. Public API,
artifact reload, negative fixture, and golden tests remain under `tests/`. Move a
rule to its owning phase rather than adding another responsibility to the facade.
Preserve existing diagnostics and snapshots when reorganizing implementation.

Run the Rust gates:

```sh
cargo test --all-targets
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

For a complete host example, run `cargo run --example 14-backend-hooks`; the
[backend tutorial](../tutorials/backends.md) also executes the generated
JavaScript and WebAssembly outputs.
