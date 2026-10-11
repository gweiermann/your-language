# Experimental syntax examples

This prototype answers whether a definition can supply one small, parser-validated
representative and display it inline in an editor. It does not generate tutorials,
execute semantic operations, or promise a complete regex/grammar synthesizer.

```yl
node Number = /[0-9]+/ {
    example "42"
}

entry Number
```

`example` is optional metadata on a node or parameterless pattern. An explicit
example wins over generation, must consume the complete source, and must pass
syntax-local constraints. Invalid metadata produces `yl.invalid_example` at the
example declaration. Duplicate examples are rejected. Metadata does not alter
accepted syntax, target ASTs, or meaning operations.

When no example is authored, the prototype searches bounded candidates:

- Literals use their text; supported Rust regex HIR produces its shortest witness.
- Choices try shortest candidates first; declaration order breaks ties.
- Optional grammar is omitted. Both `*` and `+` include one original item.
- Pipes generate from normalized grammar and preserve one list item, omitting the
  normalized separator tail. Candidate provenance counts original projection markers
  so empty alternatives and nested optional wrappers cannot accidentally produce an
  empty list. The core does not recognize `separatedBy` by name.
- Recursive rules try a terminating branch. A recursive repetition is not silently
  reduced to zero to escape recursion.
- Entry-selected trivia allows candidate spacing. Every returned candidate is
  checked with the actual parser, including constraints and operator handling. Recursive operator members are
  validated through their Pratt family and must produce that member as the root.

Generation is limited to 48 nesting levels, 10,000 generation steps per definition,
24 candidate strings per composition, and 4,096 bytes per generated candidate. An authored top-level example can be longer;
composing it into an automatically generated parent still respects the parent budget.
No valid candidate means `text: null` with a reason; this is not a language error.
This is a minimal candidate among the explored possibilities, not a proof of the
mathematically shortest string of an arbitrary grammar. Regex assertions are
checked by parsing; if the one generated regex witness fails, supply an example.
Parameterized pattern metadata is explicitly unsupported until examples can be
validated against concrete arguments. Recursive parameterized patterns already
cannot be lowered by the compiler.

```sh
cargo run -- examples examples/example_metadata/language.yl
cargo run -- examples examples/mini-js/minijs.yl
cargo run --example mini_typescript -- examples
```

The first command generates `let name=42;` for `Program` and `42` for `List`.
MiniJS generates `function A(A){0}` for a function declaration. Mini TypeScript
uses its native library registry for compilation, then performs syntax-only
example generation; the CLI cannot compile externally registered libraries.
It produces `(A){letA=0;return0;}` for `FunctionContents`. This is valid according
to that experimental grammar; examples do not impose extra keyword boundaries.

`yl examples <entry.yl|language.ylc>` emits a JSON array. Each result contains its
canonical definition ID, original YL declaration span (UTF-8 bytes), text or null,
and a reason when unavailable. `CompiledLanguage::example_previews()` supplies the
same data to native consumers. Tooling templates and explicit metadata survive
`.ylc` serialization/reload. Named pattern templates are separate from executable
rules; wrappers carrying authored pattern examples are transparent during parsing.
The extended artifact shape remains experimental.

For inline VS Code previews, build/install this branch's CLI and VSIX, then set:

```json
{
  "yourLanguage.cliPath": "C:/absolute/path/to/yl.exe",
  "yourLanguage.exampleEntry": "C:/absolute/path/to/language.yl"
}
```

The extension connects to `yl lsp` and adds inlay hints such as `example: "42"`
at declaration ends. It requires workspace trust and launches without a shell.
Open modules declaring `entry` are detected automatically; the setting above is
useful when editing only helper modules. Unsaved buffers override imports on disk.
Closing a buffer restores the disk source. Diagnostics, formatting and hints share
one LSP session with UTF-16 positions, cancellation and stale-result rejection.
Queued changes are coalesced; analysis is otherwise full-graph reanalysis. In-progress
compiler work finishes before cancellation is acknowledged. Untitled buffers receive
syntax diagnostics and formatting; save them for module resolution and hints.

The server registers `std/variables` contracts and never executes meanings. Other
native libraries, including the Mini TypeScript host's `std/types`, are not currently
registered: missing contracts produce definition diagnostics. The Rust consumer API
and compiled-artifact CLI previews remain available for these languages. Completion,
rename, navigation, incremental parsing and target-language LSPs remain future work.

Validation covers explicit composition, imports, reload, constraints, regex limits,
productive/nonproductive recursion, optional/repetition/pipe behavior, AST invariance,
formatting idempotence, Unicode editor offsets, and a real CLI/editor bridge.
No extension was globally installed or published to the Marketplace by this task.
