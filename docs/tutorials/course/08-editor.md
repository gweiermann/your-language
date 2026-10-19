# 8. Add editor roles beside the grammar

Every checkpoint uses `.txt` programs and a `yl.json` project configuration.
After [installing the extension](/guide/editor), open that project in a trusted
workspace and open a matching program. The extension discovers the entry and
selects the program's diagnostics and highlighting without a global `.txt` association.

Keep visual roles on grammar patterns, so a captured name's role remains explicit:

```yl verify=ide
node Name = value: /[A-Za-z_][A-Za-z0-9_]*/ {
    ide { highlightAs "variable" }
}

pattern FunctionName = Name {
    ide {
        example "identity"
        highlightAs "function"
    }
}

node Declaration = "function" name: FunctionName
trivia Whitespace = /\s+/
entry Declaration { trivia Whitespace }
```

The contextual wrapper overrides `Name`'s category. The editor theme chooses
colors; the language chooses roles. `example` supplies a parser-validated preview.
Without one, the bounded generator attempts a valid small example. An optional
part is omitted; `*` and `+` each try one occurrence. Unsupported cases receive no hint.

The Mini TypeScript and structural checkpoints already use this pattern in their
lexical and expression files. Do not move captures into distant extension files
just to separate the highlighting. Short `ide` blocks belong beside the grammar;
substantial semantic behavior belongs in explicitly imported operations.

Run `cargo run --example 04-ide-metadata` from the repository root. Open its
programs to inspect contextual roles, then open a definition to use Format
Document and the example CodeLens. Formatting target programs is not supplied.
See [IDE metadata](/reference/ide) and [editor configuration](/guide/editor).

Previous: [Compound data](./07-compound-types).

Next: [Host backends](./09-backends).
