# Core and standard libraries

YL separates parser primitives from helpers written in the language itself. Import callable functionality explicitly, including primitives.

```yl
import { notAhead, notBehind } from "core/parser"
import { boundedBy, separatedBy, Trailing } from "std/parser"
```

`core/parser` exposes primitives that need compiler/runtime support. `std/parser` composes those primitives and ordinary structural rewrites into reusable helpers.

## Core parser

### `notAhead(grammar)`

Negative lookahead succeeds when the supplied grammar does not match at the current source position. It consumes no source and produces no value.

```yl
import { notAhead } from "core/parser"

pattern Keyword = "let" notAhead(/[_a-zA-Z0-9]/)
node Declaration = Keyword name: /[a-z]+/
```

The boundary check prevents the keyword from matching the beginning of a longer identifier such as `letter`. Add explicit trivia selection if whitespace should be accepted between the keyword and name.

Lookaround observes the actual current source boundary; it is not an ordinary consuming grammar term that first skips trivia.

### `notBehind(grammar)`

Negative lookbehind succeeds when the supplied grammar does not match source immediately before the current position. It consumes no source and produces no value.

```yl
import { notBehind, notAhead } from "core/parser"

pattern IdentifierPart = /[_a-zA-Z0-9]/
pattern Keyword = notBehind(IdentifierPart) "let" notAhead(IdentifierPart)
```

Both sides now require an identifier boundary. Assertions retain the original source context for regex anchors and word boundaries.

The argument is a grammar expression, so these primitives can check reusable patterns as well as regexes and literals.

## Standard parser

### `boundedBy(boundary)`

`boundedBy` is a pipe that checks the same grammar on both sides of its input:

```yl
import { boundedBy } from "std/parser"

pattern IdentifierPart = /[_a-zA-Z0-9]/

pattern keyword(value) =
    value |> boundedBy(IdentifierPart)

node Declaration = keyword("let") name: /[a-z]+/
```

The helper is defined in ordinary YL:

```yl
import { notAhead, notBehind } from "core/parser"

export pipe boundedBy(boundary) {
    rewrite value => notBehind(boundary) value notAhead(boundary)
}
```

`boundary` describes text that must not occur immediately beside the input grammar. The pipe preserves the input value. Keyword policy belongs to the language definition; keyword names are not special cases in the runtime.

### `Trailing`

The standard library exports an enum controlling trailing separators:

```yl
export enum Trailing {
    none
    optional
    required
}
```

Use `.none`, `.optional`, or `.required` when the parameter context identifies `Trailing`. The explicit spellings are `Trailing::none`, `Trailing::optional`, and `Trailing::required`.

### `separatedBy(separator, trailing=.none)`

This pipe rewrites a zero-or-more or one-or-more list to match a separator between items:

```yl
import { separatedBy } from "std/parser"

node Name = text: /[a-z]+/

node Parameters =
    "(" names: Name* |> separatedBy(",", trailing=.optional) ")"
```

The `names` field remains a list of `Name` nodes. Commas and optional trailing commas do not become list elements.

| Input | Trailing mode | Accepted examples |
| --- | --- | --- |
| `Name*` | `.none` | Empty list; `a`; `a,b` |
| `Name*` | `.optional` | Empty list; `a`; `a,`; `a,b,` |
| `Name*` | `.required` | Empty list; `a,`; `a,b,` |
| `Name+` | `.none` | `a`; `a,b` |
| `Name+` | `.optional` | `a`; `a,`; `a,b,` |
| `Name+` | `.required` | `a,`; `a,b,` |

The separator can be any compatible grammar argument, not only a literal. The default is `.none`. Named syntax is `trailing=.optional`, using `=` rather than `:`.

The implementation consists of six ordinary rewrite cases:

```yl
export pipe separatedBy(separator, trailing: Trailing = .none) {
    rewrite item* {
        .none => (item (separator item)*)?
        .optional => (item (separator item)* separator?)?
        .required => (item (separator item)* separator)?
    }

    rewrite item+ {
        .none => item (separator item)*
        .optional => item (separator item)* separator?
        .required => item (separator item)* separator
    }
}
```

Projection collects only the bound `item` values. An empty star list becomes `[]`. Tuple-valued or list-valued items preserve their boundaries as individual elements.

## Defining language-specific helpers

A language can export its own patterns and pipes using exactly the same mechanisms as `std/parser`. For example, a module might define keyword boundaries, delimiter conventions, or list styles appropriate to its source language.

Importing a standard helper does not activate trivia. Always configure automatic skipping explicitly in the language's [entry block](./trivia#entry-configuration).
