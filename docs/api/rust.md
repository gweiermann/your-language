# Rust API

The crate is named `your_language`. It exposes compilation, artifact loading, parsing, structured ASTs, diagnostics, and inspection of the YL frontend. All source offsets are UTF-8 byte offsets.

## `compile_language`

```rust
pub fn compile_language(
    entry_file: impl AsRef<std::path::Path>,
) -> CompileResult<CompiledLanguage>
```

Compiles a filesystem entry module and its dependency graph. Relative imports resolve from the importing module. The compiler loads embedded `core/parser` and `std/parser` modules as needed. Compile failures return `Vec<Diagnostic>` through `CompileResult`; non-fatal diagnostics are retained on the compiled language.

```rust
fn example() -> your_language::diagnostic::CompileResult<()> {
    let language = your_language::compile_language("examples/mini-js/minijs.yl")?;
    for diagnostic in language.diagnostics() {
        eprintln!("{}", diagnostic.message);
    }
    Ok(())
}
```

## `compile_sources`

```rust
pub fn compile_sources(
    entry: &str,
    sources: &std::collections::BTreeMap<String, String>,
) -> CompileResult<CompiledLanguage>
```

Compiles an in-memory module graph. Map keys are module IDs, and values are YL source. Relative imports resolve against the importing ID, normalizing `.` and `..`; omitted extensions become `.yl`. Every non-embedded imported module must be provided in the map. The compiler follows the graph reachable from the supplied entry rather than treating unrelated map entries as language modules.

```rust
fn example() -> your_language::diagnostic::CompileResult<()> {
    use std::collections::BTreeMap;
    use your_language::{compile_sources, parse};

    let sources = BTreeMap::from([
        (
            "names.yl".into(),
            "export node Name = value: /[A-Za-z]+/".into(),
        ),
        (
            "entry.yl".into(),
            r#"
        import { Name } from "./names"
        node Greeting = "hello:" name: Name
        entry Greeting
    "#
            .into(),
        ),
    ]);
    let language = compile_sources("entry.yl", &sources)?;
    let result = parse(&language, "hello:world");
    assert!(!result.has_errors());
    Ok(())
}
```

## `CompiledLanguage`

`CompiledLanguage` is the normalized runtime input. Its rule storage is encapsulated; callers compile or load a language rather than assembling one manually.

```rust
impl CompiledLanguage {
    pub fn diagnostics(&self) -> &[Diagnostic];
    pub fn to_bytes(&self) -> CompileResult<Vec<u8>>;
}
```

`diagnostics()` returns non-fatal definition diagnostics. `to_bytes()` serializes the artifact deterministically. Definition warnings/help are not serialized; runtime constraint checks are. See [artifacts](/api/artifacts).

## `load_compiled_language`

```rust
pub fn load_compiled_language(bytes: &[u8]) -> CompileResult<CompiledLanguage>
```

Deserializes and validates an artifact before returning it. Invalid JSON, unsupported formats, and invalid normalized references or checks produce diagnostics. Hosts should use this function rather than relying on direct Serde deserialization alone.

## `parse` and `parse_named`

```rust
pub fn parse(language: &CompiledLanguage, source: &str) -> ParseResult;
pub fn parse_named(language: &CompiledLanguage, file: &str, source: &str) -> ParseResult;
```

Both parse from the declared entry and require the input to be consumed apart from selected trivia. `parse` uses `<source>` as the file name. `parse_named` preserves the caller's file label in AST and target-source diagnostic spans. Neither function loads source from disk.

```rust
fn inspect(language: &your_language::CompiledLanguage) {
    let result = your_language::parse_named(language, "settings.txt", "source text");
    if result.has_errors() {
        for diagnostic in &result.diagnostics {
            eprintln!("{}: {}", diagnostic.code, diagnostic.message);
        }
    }
    if let Some(ast) = result.ast {
        println!("{}", ast.kind);
    }
}
```

## `ParseResult`

```rust
pub struct ParseResult {
    pub ast: Option<AstNode>,
    pub diagnostics: Vec<Diagnostic>,
}
impl ParseResult {
    pub fn has_errors(&self) -> bool;
}
```

`has_errors()` checks for error severity. Warnings/help alone do not count as errors. A parse failure has no AST; a successful match with a constraint error can retain its AST. Callers should inspect both fields rather than equating AST presence with validity.

## Frontend inspection

```rust
pub fn parse_yl(file: &str, source: &str) -> CompileResult<syntax::Module>;
```

Parses one YL source into a spanned surface AST without resolving imports or validating the complete language. The public `syntax` module models declarations, grammar expressions, condition expressions, argument lists, and source spans. Use this API for source inspection; use compilation for executable language validation.

The `ir` module exposes normalized representation types for inspection and Serde support. Those enums describe the interpreter input, not an alternative surface language. [Artifact documentation](/api/artifacts) describes the intended boundary. Generated Rust documentation gives exhaustive field and enum definitions:

```sh
cargo doc --no-deps --open
```
