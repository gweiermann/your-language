# Embed the Rust runtime

Applications can compile definitions and parse source directly, without invoking the CLI. This tutorial uses filesystem compilation; the [Rust API reference](/api/rust) also describes in-memory module graphs.

## Compile and reload

```rust
use your_language::diagnostic::CompileResult;
use your_language::{compile_language, load_compiled_language, parse_named};

fn main() -> CompileResult<()> {
    let language = compile_language("examples/mini-js/minijs.yl")?;
    let bytes = language.to_bytes()?;
    let reloaded = load_compiled_language(&bytes)?;

    let result = parse_named(&reloaded, "sample.js", "let answer = 42");
    assert!(!result.has_errors());
    assert!(result.ast.is_some());
    Ok(())
}
```

Compilation failures and artifact-load failures return structured diagnostic vectors. Definition warnings and help messages are available through `language.diagnostics()` before serialization. They are not stored as runtime checks in the artifact.

## Handle both outputs

Parsing always returns a `ParseResult` containing an optional AST and a diagnostic vector. A malformed grammar match has no AST. A matched node with a constraint error can still have an AST, so check `has_errors()` independently from `ast.is_some()`.

```rust
use your_language::{parse_named, CompiledLanguage};
fn inspect(language: &CompiledLanguage) {
    let result = parse_named(language, "input.txt", "source text");
    for diagnostic in &result.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    if let Some(ast) = result.ast {
        println!("Root node: {}", ast.kind);
    }
}
```

## Preserve file names and byte offsets

`parse_named` records the supplied file name in target-source spans. The host owns the source text and can use the half-open byte ranges to render diagnostics, highlight AST nodes, or navigate an editor. Validate offsets as UTF-8 boundaries before slicing externally supplied data.

Read [AST values](/api/ast), [diagnostics](/api/diagnostics), and [artifacts](/api/artifacts) for serialization and error-handling details.
