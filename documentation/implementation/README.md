# Syntax-v0 implementation status

Issue #5 is **incomplete**, with explicit [PARKED design questions](./PARKED.md).
MiniJS passes the compile/reload/parse flow. Required general constraints and metadata remain parked. The generic layers
below work independently and are covered by tests through the public API.

## Architecture

```text
UTF-8 YL files
  lexer.rs + frontend.rs -> spanned syntax.rs AST
  compiler.rs            -> module graph / exports / membership / argument binding
                         -> pattern and structural pipe expansion
                         -> precedence operator plans / constraint checks
  ir.rs                  -> deterministic normalized rules
  artifact.rs            -> versioned JSON .ylc / validation on reload
  runtime.rs             -> generic grammar interpreter / Pratt expression families
                         -> AST + structured diagnostics
  main.rs                -> yl check / compile / parse --json
```

Rules use deterministic module-qualified IDs. Concrete AST names retain their
declaration's qualified name; imported membership paths are aliases to that rule.
An abstract rule returns the matched concrete node without adding a wrapper.
Patterns are expanded at compile time, with no AST identity. Runtime terms contain
no pattern calls, pipes, enum variants, import statements or stdlib source.

All pipes preserve their input type, following the user's implementation-time
clarification. Lowering marks the original grammar values and emits a generic
projection around the transformed matcher. Repeated rewrites collect only the
bound item's values; wrapper captures/values are discarded. Scalar wrappers forward
the original scalar value. Tuple-valued list items remain individual elements.

The interpreter tries alternatives in declaration order, with transactional captures
and diagnostics. Recursive abstract families execute precedence plans, handling
left-growing member/postfix trees, right-growing prefix trees and infix associativity.
Guards limit nesting/work and return structured diagnostics rather than recurse
indefinitely. Production optimization is outside this issue.

Lookbehind constrains consumption to its left-context boundary, including zero-width
matches. Regex assertions retain the original source context for anchors and word
boundaries; greedy, lazy and alternative regex matches are checked at the boundary.
Nested lookahead may inspect the source beyond that consumption bound.

All source positions are half-open UTF-8 byte ranges. AST nodes carry target-source
spans. Captures retain matching spans internally for syntax-local constraints.
Diagnostic categories/severity/file/primary and secondary spans/help are serialized.
Failed parse branches do not leak constraint diagnostics into a successful branch.

Literals and regexes produce matched text (also required for literal operator
captures to obey the unchanged-value rule). Lookaround produces unit. Sequences
return unit, a single value, or an array representing a tuple. Repetitions return
arrays and optional values use JSON null when absent. Concrete nodes retain only
named fields; uncaptured values never become AST fields. A pattern wrapping several
value-producing terms returns their tuple, with no invented wrapper node.

`.ylc` is pretty JSON, version 1, with ordered maps. Loading validates versions,
entry, references, regexes, trivia ownership, captures and constraint operands.
The artifact is reusable without original YL files. The Rust API is a v0 API,
not a promised long-term binary/serialization compatibility contract.

`stdlib/parser.yl` is embedded as ordinary YL for `std/parser`; `boundedBy` uses
explicit imports of `core/parser` lookaround. `separatedBy` is shipped as the
documented ordinary YL pipe, expanded through the same generic resolution/lowering path. The compiler/runtime has no
checks for MiniJS node names, keywords or grammar files and no hard-coded list helper.

## Acceptance status

| Issue sections | Status / evidence |
| --- | --- |
| 1–6 frontend, modules, nodes/membership | Implemented; spanned parser, graph/visibility diagnostics, alias and cycle tests |
| 7 grammar values / parameterized patterns | Implemented, including type-preserving pipe projections; P2 resolved by user |
| 8 grammar operator precedence | Implemented; choice/sequence/capture/postfix/pipe/grouping/chaining tests |
| 9 structural rewrites | Implemented for documented structural arms; application/grouping resolved from declaration categories |
| 10 enums/arguments | Implemented for unambiguous expected types; multiple-enum case dispatch PARKED P4 |
| 11 separatedBy | Implemented in ordinary YL; all six modes and flat values tested |
| 12 recursive precedence | Implemented; product/sum, power, member/call, unary, grouping, nonassoc tests |
| 13–15 trivia/core/std/entry | Implemented; ordinary trivia, explicit core imports, boundedBy and entry checks |
| 16 constraints | Documented between/matches and reusable constraints implemented; general boolean/string syntax PARKED P3 |
| 17 extensions | Additive constraints implemented, grammar changes rejected; metadata PARKED P5 |
| 18 normalized artifact | Implemented; deterministic roundtrip and corrupt-artifact validation tests |
| 19 MiniJS | Frontend/normalized/runtime snapshots, valid and negative fixtures; public API and CLI flow passing |
| 20 tests | Unit/integration/CLI/negative/robustness and goldens present, including MiniJS AST/diagnostics |
| 21 quality | Root tests, format and strict Clippy checks run; CI for Linux/Windows added |
| 22 definition of done | MiniJS flow passes; issue remains incomplete on P3/P5 |

No semantic-layer features, parser generation, JIT, incremental parsing, FFI or LSP
were added. Conflicting legacy crates were removed.

## Public API

```rust,no_run
use your_language::{compile_language, load_compiled_language, parse_named};

let language = compile_language("entry.yl")?;
let bytes = language.to_bytes()?;
let language = load_compiled_language(&bytes)?;
let result = parse_named(&language, "input.txt", "source text");
# Ok::<(), Vec<your_language::diagnostic::Diagnostic>>(())
```

`compile_sources` accepts a map of module IDs to source for in-memory hosts.
`frontend::parse_yl` exposes the spanned frontend AST for inspection.
`parse` defaults the target file name to `<source>`; `parse_named` sets it explicitly.
Compile/load failures return diagnostic vectors. Parse failures set `ast` to null.
Syntax-local constraint errors retain the AST plus their diagnostics.

## Running tests and CLI

The pinned toolchain is Rust 1.85.0 with rustfmt and Clippy. The lockfile is committed.

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo install --path . --locked

yl check tests/fixtures/syntax-v0/language.yl
yl compile tests/fixtures/syntax-v0/language.yl -o independent.ylc
yl parse independent.ylc tests/fixtures/syntax-v0/program.txt --json
```

The equivalent `cargo run -- <command>` works without installation. Tests invoke
the built CLI directly, check exit codes, and inspect JSON. Golden updates are
explicit: `UPDATE_GOLDENS=1 cargo test --test integration` (PowerShell:
`$env:UPDATE_GOLDENS='1'`). Review snapshots before committing. Tracked text uses LF
so source-span snapshots are identical on Linux and Windows.

`yl parse ... --json` emits the AST/diagnostics object on stdout for source parse
failures, artifact errors and file I/O errors alike. Error exits are nonzero; file
errors retain their path in the diagnostic's primary span. Other commands report
their structured errors on stderr.

The requested acceptance command:

```sh
yl check documentation/target-syntax/minijs.yl
```

passes. The full check/compile/parse sequence is covered by integration tests. The
independent fixture provides additional generic-layer coverage.
