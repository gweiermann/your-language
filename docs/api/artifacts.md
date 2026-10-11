# Compiled language artifacts

A `.ylc` file stores the normalized language executed by the runtime. Compilation resolves surface constructs once, so parsing does not need original YL modules, imports, pattern definitions, or pipe implementations.

## Compile, serialize, reload

```rust
fn example() -> your_language::diagnostic::CompileResult<()> {
    use your_language::{compile_language, load_compiled_language, parse_named};

    let compiled = compile_language("examples/mini-js/minijs.yl")?;
    let bytes = compiled.to_bytes()?;
    let reloaded = load_compiled_language(&bytes)?;
    assert_eq!(bytes, reloaded.to_bytes()?);
    let result = parse_named(&reloaded, "sample.js", "let answer = 42");
    assert!(!result.has_errors());
    Ok(())
}
```

The CLI equivalent is `yl compile <entry.yl> -o <language.ylc>`, followed by a `yl language` tool.

## Representation

Artifacts are versioned pretty JSON with deterministic ordered rule maps. Syntax-only languages retain version `1`; experimental languages with native meanings use version `2`. It includes the entry rule, selected trivia, normalized rules, precedence plans, syntax-local checks, and definition spans. It does not include the original module source. Compile-time warning/help records are excluded from serialization.

Rule bodies distinguish concrete grammar from abstract node families. Terms include literals, regexes, references, sequences, choices, repetitions, captures, negative lookaround, and generic value marking/projection. Marking and projection implement type-preserving rewrite results without teaching the runtime individual pipe names.

Imports, contextual enum values, parameterized patterns, structural rewrite cases, and standard-library helper calls are resolved during compilation. The runtime operates on their normalized result.

## Validation on reload

`load_compiled_language` checks the artifact format and executable invariants, including entry and rule references, regex validity, trivia ownership, captures, precedence plans, and constraint operands. Invalid JSON or invalid normalized data produces diagnostics. Direct Serde deserialization is not a substitute for these checks.

## Determinism and relocation

Module IDs are relative to the entry directory. Relocating the same definition graph without changing its relative structure preserves its normalized IDs and serialized bytes. Target-source file labels are supplied at parse time and do not alter the language artifact.

## Compatibility

The version field allows incompatible artifact changes to be rejected rather than interpreted accidentally. Long-term artifact compatibility and a binary ABI are not promised. Recompile language definitions with the runtime version used by the host when updating tooling.

## Native meanings

[Native semantic artifacts](/api/native-semantics) also retain operation contracts and scheduling boundaries. Use `SemanticEngine::load` to verify registered library compatibility and `analyze` to execute them. Artifact files remain optional: registry-aware compile and analyze can run entirely in memory. Existing parse APIs and CLI tools perform syntax analysis only.
