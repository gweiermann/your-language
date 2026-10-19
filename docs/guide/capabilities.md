# Capabilities and boundaries

YL combines a grammar, authored static analysis, editor metadata, and host hook
contracts in a reusable language artifact. The examples demonstrate deliberately
small languages; Mini TypeScript is not an implementation of the TypeScript standard.

| Area | Available now | Boundary |
| --- | --- | --- |
| Syntax | Spanned ASTs, modules, imports/exports, concrete/abstract nodes, patterns, pipes, precedence, trivia, syntax checks | No generated target-language parser or incremental parsing |
| References | Authored lexical scopes, declaration identity, shadowing, duplicates, ordered visibility, hoisted registration through explicit precedence | Target-language import/export resolution is not supplied automatically |
| Types | Language-owned symbols, union contributions, restrictions, operator rules, isolated per-call inference | No flow-sensitive literal types or branch narrowing |
| Compound types | Structural interfaces, tuples, homogeneous/mixed lists, explicit type parameters, recursive structural comparison | No complete TypeScript compatibility, recursive named-type solver, or prototype system |
| Diagnostics | Terminal/JSON diagnostics, grouped source annotations, optional origin tracing | Origin tracking is an authored policy, not automatic history for every type |
| Hosts | Reusable Rust API, unevaluated hook dispatch, interpreter/JavaScript/Wasm examples | Example hosts support an integer subset, not execution of all Mini TypeScript features |
| Editor | YL formatting, declaration hover, generated examples, project-selected program diagnostics and highlighting | No completion, navigation/rename, or target-language formatter |

## Experimental contracts

The language and its Rust API are usable from checked-in examples. `.ylc` is a
validated, versioned executable artifact; long-term format compatibility and a
stable foreign-function ABI are not promised. Host hooks leave runtime values,
control flow, and emitted code to the consumer.

Read the [artifact contract](/api/artifacts) and [backend interface](/api/backends)
when embedding. The [roadmap](/roadmap) distinguishes future flow analysis,
heuristics, ownership, and additional integration work from current features.
