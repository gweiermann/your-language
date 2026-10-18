# Executable language images

A `.ylc` file is an immutable program for the YL engine. Compilation resolves modules, aliases, parameterized patterns, pipes, operation wrappers, and enum values once. The engine loads the resulting tables and executes them against target source; it does not load YL source or reconstruct the compiler's recursive grammar and query representation.

## Compile, serialize, reload

```rust
fn example() -> your_language::diagnostic::CompileResult<()> {
    use your_language::{compile_language, load_compiled_language, parse_named};

    let compiled = compile_language("examples/03-mini-js/definition/minijs.yl")?;
    let bytes = compiled.to_bytes()?;
    let reloaded = load_compiled_language(&bytes)?;
    assert_eq!(bytes, reloaded.to_bytes()?);
    let result = parse_named(&reloaded, "sample.js", "let answer = 42");
    assert!(!result.has_errors());
    Ok(())
}
```

The CLI uses the same checked interface:

```sh
cargo run -- compile examples/03-mini-js/definition/minijs.yl -o target/minijs.ylc
cargo run -- language target/minijs.ylc ast examples/03-mini-js/programs/program.txt
cargo run -- language target/minijs.ylc check examples/03-mini-js/programs/invalid.txt
```

The final command reports the invalid program with source underlines and exits unsuccessfully. Add `--json` for structured diagnostics. The compiled language remains usable after its original `.yl` files are removed.

To inspect table counts and measure compilation, checked loading, and analysis separately:

```sh
cargo run --example compiled_image
cargo run --example compiled_image -- examples/08-mini-typescript/definition/language.yl examples/08-mini-typescript/programs/generics.txt
```

Both examples report format `10`, `AST: true`, and `diagnostics: 0`. Timing numbers describe the current machine and build; they are not performance guarantees.

## Representation

Format `10` uses deterministic pretty JSON. It contains executable tables with integer references:

```text
language image
├── version and required engine capabilities
├── parser
│   ├── rules and entry/trivia rule indices
│   ├── grammar instructions and child indices
│   ├── precomputed Pratt operator edges and binding powers
│   └── syntax-check expressions and operand indices
├── queries
│   ├── expression instructions with engine opcodes
│   ├── action instructions and expression/action indices
│   └── semantic producer groups
├── semantics: scheduling selectors and native operation contracts
└── previews: representative syntax and declaration locations
```

For example, a sequence refers to existing instruction rows instead of embedding another grammar tree:

```json
[
  { "Literal": "(" },
  { "Regex": "[a-z]+" },
  { "Literal": ")" },
  { "Sequence": [0, 1, 2] }
]
```

References to rules support recursive grammars. Grammar-expression and check-expression edges point backward within their tables; query expressions and actions form a validated directed acyclic graph. Compiled query calls carry opcodes such as `Get`, `Find`, and `Contribute`, rather than names that must be resolved through a YL module at execution time. Shared expression indices preserve correlated match values and immutable binding dependencies.

Source spans, capture names, rule names, opaque symbol identities, and diagnostic text remain operands or metadata. They preserve AST names, attachment identity, and useful error locations. The image contains neither import statements nor authored library bodies. Compile-time warning/help records are excluded.

Static regex programs are prepared when the image is compiled or loaded and reused across source sessions. Source-dependent lookbehind work remains session-local.

## Image and session state

The image describes what the engine can execute. Each target source gets its own parser stack, committed syntax occurrences, capture values, attachment entries, pending producers, diagnostics, and hypothetical-query state. Concurrent analyses share no mutable session state.

A language definition cannot precompute the actual scopes or references of a program it has not parsed. The image therefore contains producer templates; the engine discovers their concrete instances and dependencies from committed syntax, then evaluates them to settlement. Scheduling boundaries, guarded actions, pending values, and isolated hypothetical queries retain their normal behavior.

The image also records optional [backend hook mappings](/reference/backends). Compilation retains only mappings reachable from the entry grammar and the hook contracts those mappings reference, with their indexed addresses remapped deterministically. Imported unused hooks do not become host requirements. Grammar and static-analysis tables are retained independently. Parsing and analysis do not invoke these hooks. An external consumer explicitly binds a host backend and dispatches syntax to interpret, transpile, or compile a program; runtime values and execution policy remain host-owned.

## Validation on reload

Use `load_compiled_language` or `SemanticEngine::load`. Only these checked constructors create an executable language handle. Deserializing public image tables is useful for inspection and tooling, but does not produce a `CompiledLanguage`.

The loader rejects unsupported versions and capabilities, invalid JSON, trailing input, out-of-bounds references, invalid regexes, malformed operator plans, illegal graph cycles, invalid capture access, and inconsistent semantic scheduling metadata. It bounds the wire representation to 64 MiB and JSON nesting to 512 levels before deserializing; table validators also enforce their execution resource limits. Invalid images produce structured diagnostics before execution.

## Determinism and relocation

Module identities are relative to the entry directory. Relocating the same definition graph without changing its relative structure preserves those identities and serialized bytes. Table construction is deterministic. Loading and serializing a valid image preserves its bytes. Target-source file labels are supplied for each parse and do not alter the image.

## Compatibility

All newly compiled languages use format `10`. Formats `1` through `9` are retired: recompile their YL definitions with the current compiler. The loader reports this explicitly instead of rebuilding recursive normalization IR or maintaining a second production executor.

The image format is versioned but experimental. Long-term binary compatibility, a memory-mapped layout, and an ABI are not promised. An eventual binary encoding can represent these same tables without changing YL syntax.

## Native semantics

[Native semantic interfaces](/api/native-semantics) retain external operation contracts and lifecycle scheduling metadata. `SemanticEngine::load` additionally verifies compatibility with the host's registered operations. Artifact files remain optional: compilation and analysis can run entirely in memory.

## Compiled pure functions

Format `8` adds a pure-function table and indexed `Invoke` and value-`Match` instructions. Function bodies bind explicit parameters; calls reference the compiled function table rather than resolving YL modules at runtime. Instruction containment remains acyclic, while function calls may recurse through table references. The loader validates function addresses, arities, local bindings, pattern constants, and purity before execution. Mutable memoization, active-call guards, dependency observations, and hypothetical overlays remain analysis state rather than artifact data.
