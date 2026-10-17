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
    pub annotations: Box<Vec<DiagnosticAnnotation>>,
    pub help: Option<String>,
}
```

`Severity` has `Error`, `Warning`, and `Help` variants, serialized as `error`, `warning`, and `help`. The primary span identifies the principal location; secondary spans provide related locations. `help` is optional supporting guidance. `Diagnostic::error(code, message, primary)` constructs an error with no secondary spans, annotations, or help.

Each `DiagnosticAnnotation` contains a `span: Span` and `label: Option<String>`. The `annotations` field is omitted from JSON when empty, and defaults to empty when loading older diagnostic JSON.

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

## Grouped semantic diagnostics

Explicitly import diagnostic operations from `core/diagnostics`. One call produces one diagnostic containing the requested highlights:

```yl
import { error } from "core/diagnostics"

node Assignment = name: /[a-z]+/ "=" value: /[0-9]+/ {
    semantics {
        error("This assignment is not allowed", [
            (value, "incompatible value"),
            (name, absent)
        ])
    }
}
```

The second argument is a list of `(syntax, label)` tuples. A label is text or `absent`; an absent label still produces an underline. The first retained location becomes the primary span, and the remaining locations become secondary spans. No extra highlight of the diagnostic's definition is added in this form. An absent syntax location is skipped. An empty list, or a list whose locations are all absent, produces no diagnostic. Pending query inputs keep the diagnostic pending until their dependencies settle.

Messages may be computed by a pure operation, for example `error(describeDifference(problem), highlights)`. Text literals also support `${...}` interpolation. The same annotation form works with `warn` and `hint`. The existing `error(message)`, `error(message, syntax)`, and three-argument related-location forms remain supported.

Human CLI output renders the source ranges and their labels together under one message. JSON returns one diagnostic object with an `annotations` array:

```json
{ "annotations": [
  { "span": { "file": "input", "start": 4, "end": 6 }, "label": "incompatible value" },
  { "span": { "file": "input", "start": 0, "end": 3 }, "label": null }
] }
```

LSP diagnostics support a single highlighted range each. The language server therefore publishes a linked diagnostic at every annotation range, rather than relying on related information alone to draw squiggles. Their `data.group` identifiers match; `data.primary` identifies the first entry. The primary entry links to all related locations, and companions link back to the primary. Editors can consequently show several Problems entries for one engine diagnostic. Byte offsets are converted to UTF-16 positions for LSP.

Diagnostic groups are bounded to 128 supplied annotations. Messages, labels, and their editor publication costs share the query evaluation content budget. Exceeding a limit reports a structured `query.resource` diagnostic.

Type-origin explanations are explicit: a type library must retain the source nodes that contributed each type and pass those nodes as annotations. Diagnostic rendering does not reconstruct a propagation history or infer where a type originated.
