# AST representation

Concrete nodes produce AST objects. Abstract nodes return their concrete matched member. Patterns and pipes do not create artificial wrappers.

## `AstNode`

```rust
pub struct AstNode {
    pub kind: String,
    pub fields: std::collections::BTreeMap<String, AstValue>,
    pub span: Span,
}
```

The Rust `kind` field serializes as `type`. It contains the concrete declaration's qualified node name. A membership alias does not create a new identity. `fields` contains named captures only, ordered deterministically by key. `span` identifies the source consumed by the node.

For a simple grammar:

```yl
node Pair = key: /[A-Za-z]+/ "=" value: /[0-9]+/
entry Pair
```

Parsing `x=42` with file name `input.txt` produces:

```json
{
  "type": "Pair",
  "fields": { "key": "x", "value": "42" },
  "span": { "file": "input.txt", "start": 0, "end": 4 }
}
```

The uncaptured equals sign does not become a field. Literal and regex captures contain matched text; numeric text remains a string.

## `AstValue`

```rust
pub enum AstValue {
    Node(Box<AstNode>),
    Text(String),
    List(Vec<AstValue>),
    None,
}
```

Serialization is untagged: node values are objects, text values are strings, lists and tuples are arrays, and absent optional values are JSON `null`. `None` is the Rust representation of absence; the YL condition keyword is `absent`.

| Grammar value | JSON representation |
| --- | --- |
| Concrete node | Object with `type`, `fields`, `span` |
| Abstract node | Its matched concrete node object |
| Literal or regex | String |
| Repetition | Array of element values |
| Multi-value sequence | Array representing a tuple |
| Absent optional capture | `null` |

Tuple and list arrays share a JSON representation; their grammar determines the intended structure. List-valued elements remain nested elements rather than being flattened automatically by a pipe.

## Capture behavior

A capture retains the value produced by its expression. A concrete node ignores uncaptured values when creating fields. A pattern returns its grammar value directly, so capturing a pattern does not insert a node named after the pattern.

An absent optional element has an absence value. Captures inside a group that does not match may be missing from the field map. Hosts should handle both a missing field and JSON `null` when inspecting optional syntax; the grammar determines which structure is present.

## Parsing failures and constraints

`ParseResult.ast` is `None` when the grammar cannot produce a complete AST. The CLI writes `null` in this case. Constraint errors describe matched syntax and may retain the AST, allowing tools to inspect the same source while reporting invalid local combinations.

Resource guards also return structured errors rather than unbounded recursion. In particular, AST value depth is bounded to 128; reaching a guard produces `parse.resource_limit` with no AST. See [diagnostics](/api/diagnostics).
