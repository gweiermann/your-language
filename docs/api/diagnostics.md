# Diagnostics and spans

Definition checks, artifact loading, and source parsing use structured diagnostics. Human terminal rendering is a CLI presentation of the same information.

## Source spans

```rust
pub struct Span {
    pub file: String,
    pub start: usize,
    pub end: usize,
}
```

Offsets are half-open UTF-8 byte ranges: `start` is inclusive and `end` is exclusive. They are not character columns or line numbers. A zero-length range can indicate an insertion point or end of input. `Span::new(file, start, end)` constructs a span; `Span::default()` has an empty file and zero offsets.

Target spans use the file label supplied to `parse_named`. Compiled definition spans use module IDs relative to the entry directory, including embedded library IDs where appropriate. Hosts render line/column information by consulting the associated source.

## Diagnostic fields

```rust
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub primary: Box<Span>,
    pub secondary: Vec<Span>,
    pub help: Option<String>,
}
```

`Severity` has `Error`, `Warning`, and `Help` variants, serialized as `error`, `warning`, and `help`. The primary span identifies the principal location; secondary spans provide related locations. `help` is optional supporting guidance. `Diagnostic::error(code, message, primary)` constructs an error with no secondary spans or help.

```json
{
  "severity": "error",
  "code": "parse.unexpected_token",
  "message": "Unexpected token",
  "primary": { "file": "input.txt", "start": 3, "end": 4 },
  "secondary": [],
  "help": null
}
```

This illustrates the schema; exact messages and positions depend on the grammar and source. The CLI JSON check output is an array of these objects.

## Definition diagnostics

Definition errors cover malformed YL, inaccessible imports, conflicting declarations, invalid references, unsupported argument types, incompatible rewrite cardinality, incomplete enum matches, and unsafe optional-capture access. Compile/load functions return `CompileResult<T>`, an alias for `Result<T, Vec<Diagnostic>>`.

Compile-time rewrite errors and warnings point to the selected enum arguments at the pipe application. Additional spans identify other selected arguments and the diagnostic declaration. Forwarded arguments retain their original caller spans. Defaulted arguments point to the call and are identified in help text.

## Runtime diagnostics

Source parsing reports unexpected input, missing syntax, syntax-local constraint emissions, and resource limits. Failed speculative alternatives do not leak constraint diagnostics into a successful alternative. Constraints can report errors while preserving the matched AST.

Warnings and help do not fail `ParseResult::has_errors()` or the CLI exit status. An error does. A clean JSON source check returns `[]`; a clean human source check is silent.

## Rendering

The CLI shows source lines and caret underlines, combining related spans on the same line. Terminal errors use red markers; warnings/help use yellow markers. Redirecting stderr disables ANSI colors. JSON output provides byte spans for editors or hosts that need their own renderer.
