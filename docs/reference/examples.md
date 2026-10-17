# Syntax examples

Representative examples let language authors preview matched source without running meaning operations. Nodes and parameterless patterns accept one optional `example` property inside an `ide` block:

```yl
node Number = /[0-9]+/ {
    ide {
        example "42"
    }
}

pattern Argument = Number {
    ide {
        example "7"
    }
}

entry Argument
```

An authored example takes priority over generation. It must parse completely as its definition and satisfy syntax-local constraints. Duplicate metadata is rejected; an invalid example reports `yl.invalid_example` at its declaration. Metadata does not change the grammar, target AST or meaning behavior. Parameterized pattern metadata and example metadata in node extensions are not supported.

## Automatic composition

Without explicit metadata, a bounded generator explores representative candidates:

| Grammar | Default candidate |
| --- | --- |
| Literal | Its exact text. |
| Supported regex | Its shortest generated witness, validated by parsing. |
| Choice | Short candidates, with readable spacing preferred and declaration order breaking ties. |
| Optional | Omitted. |
| `*` or `+` | One original item. |
| Recursive grammar | A terminating path when found. |
| Pipe | Normalized grammar with original list-item projection preserved. |

A recursive repetition is not reduced to zero just to escape recursion. Regex assertions are validated by the parser; when the generated witness fails, provide explicit metadata. No valid candidate produces an unavailable preview, not an error in the language definition.

Selected trivia accepting a single space allows readable spacing between fragments. Spaces are never inserted inside literal or regex matches. Every candidate is validated, and an unspaced fallback can satisfy adjacency constraints. Without selected whitespace trivia, fragments concatenate. This does not infer keywords: `"let" Name` may accept `letname` unless the language explicitly requires a separator or boundary.

Generation is bounded to 48 nesting levels, 10,000 steps per definition, 24 candidates per composition and 4,096 bytes per generated candidate. An explicit top-level example may be longer; its composition into a generated parent still respects that parent's budget. The result is a short explored representative, not a proof of the globally shortest accepted source.

## CLI and editor

```sh
cargo run -- examples examples/example_metadata/language.yl
```

The JSON array includes canonical definition IDs, UTF-8 declaration spans, text or null, and an unavailable reason. The checked-in fixture produces `let name = 42 ;` for `Program` and `42` for `List`. The command also accepts `.ylc` artifacts. Metadata and tooling templates survive artifact reload.

The [VS Code language server](/guide/editor#example-hints) shows copyable CodeLens labels above declarations by default. Optional inline hints are selected with `yourLanguage.exampleDisplay`. Unsaved buffers participate in composition; meaning operations are never executed for previews. Rust consumers can call `CompiledLanguage::example_previews()` for the same data.
