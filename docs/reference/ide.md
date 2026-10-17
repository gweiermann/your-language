# IDE metadata

Nodes and patterns can describe their editor presentation beside their grammar. This metadata does not add AST fields or alter parsing or semantic behavior.

```yl
pattern Identifier = /[a-zA-Z_][a-zA-Z0-9_]*/ {
    ide {
        example "name"
        highlightAs "variable"
    }
}

pattern FunctionKeyword = "function" {
    ide {
        highlightAs "keyword"
    }
}

pattern FunctionName = Identifier {
    ide {
        highlightAs "function"
    }
}

node FunctionDeclaration =
    FunctionKeyword
    name: FunctionName
    "("
    ")"

entry FunctionDeclaration
```

Both properties are optional. Each definition accepts one `ide` block, with at most one `example` and one `highlightAs`. Properties may appear in either order. An explicit example must parse completely and satisfy syntax-local checks; when omitted, bounded [example composition](/reference/examples) remains active. Explicit examples still require a parameterless pattern; highlighting is also supported on parameterized patterns. Extensions cannot add or replace IDE metadata.

`ide { example "42" }` is the canonical spelling. Earlier standalone `example "42"` sections are rejected with a migration diagnostic.

## Contextual highlighting

`highlightAs` accepts a standard semantic-token category string:

```text
namespace type class enum interface struct typeParameter parameter
variable property enumMember event function method macro keyword
modifier comment string number regexp operator decorator
```

Unknown strings are definition errors. Categories specify a role, never a fixed color. Themes decide how to render them.

A contextual wrapper overrides the wrapped definition. In the example, `Identifier` is normally a variable, but `FunctionName` colors the same matched identifier as a function. The outermost explicitly annotated wrapper wins where spans overlap. A wrapper applies its category to the matched terminal fragments, preserving unclassified gaps and selected trivia. An unannotated wrapper preserves its children's categories. Category ties at the same level follow the selected parse; rejected alternatives do not add tokens. You can annotate a trivia definition as `comment` separately.

Literals are not automatically classified as keywords. Use meaningful patterns such as `FunctionKeyword`, so a literal's role is explicit and reusable. Tokens keep target-source UTF-8 byte spans; the LSP converts these to UTF-16 positions and splits multiline tokens for editor rendering. On malformed input, a consistent parsed prefix can still supply highlighting.

## Compiled languages and editors

IDE metadata is compiled into the indexed `.ylc` image. A reloaded artifact produces the same highlights without access to the original YL modules. Rust consumers use `CompiledLanguage::highlight_named(file, source)`, returning `HighlightSpan { span, token_type }`. This API does not execute semantic operations.

The [project configuration](/guide/editor#programs-in-a-language-project) selects the language entry and matching program paths. Highlight categories stay in YL, never in `yl.json`. The language server supplies standard semantic tokens for those program documents and continues to supply example previews while editing definitions.
