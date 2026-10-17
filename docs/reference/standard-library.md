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

## Opt-in type policies

The generic `std/types` API remains independent of a target language's type
catalog and operators. These separately imported, authored YL modules provide
more opinionated policies. They compile into `.ylc` like any other YL module;
loading an artifact does not require their source files or native adapters.

### `std/types/arithmetic`

```yl
import { additionType, checkAddition } from "std/types/arithmetic"
```

Both operations receive the caller's numeric and string type identities. The
module exports no primitive type catalog.

| Operation | Behavior |
| --- | --- |
| `additionType(left, right, numberType, stringType)` | Pure policy for one candidate pair: two numbers produce number; string/string and string/number in either order produce string; `unknown` propagates; unsupported pairs produce `absent`. |
| `checkAddition(target, left, right, numberType, stringType)` | Reads syntax types through generic `std/types`, distributes candidate combinations, annotates valid results, and reports unsupported combinations at `target`. |

Use the pure operation when your language owns its annotation storage and
reporter. Use the adapter when sharing the generic `std/types` channel. These
rules are deliberately narrower than full JavaScript coercion: booleans,
objects, and user-defined conversion are unsupported.

Example 08 implements the matrix locally. Example 10 imports this preset,
retaining local function queries and hypothetical argument resolution.

### `std/types/structural`

```yl
import { structType, listType, numberType, isAssignableTo } from "std/types/structural"

operation acceptsScores(actual) =
    isAssignableTo(actual, structType([("scores", listType(numberType()))]))
```

This preset owns a complete descriptor model and its own annotation channel:

| Exports | Purpose |
| --- | --- |
| `Type`, `Comparison`, `Difference` | Tags, comparison modes, and difference reasons. |
| `numberType()`, `stringType()`, `booleanType()`, `unknownType()` | Primitive descriptors. |
| `tupleType(elements)`, `listType(element)`, `structType(fields)`, `unionType(members)`, `parameterType(identity)` | Compound and parameter descriptors. |
| `annotateWithType(target, value)`, `typeOf(target)` | Contribute and collect descriptors using this model. |
| `restrictWritesTo(target, value)`, `writeTypeOf(target)` | Keep the declared write contract separate from inferred initializer types. |
| `compareTypes(actual, expected, mode)`, `isIdenticalTo(left, right)`, `isAssignableTo(left, right)` | Recursive comparison; the worker returns structural differences. |
| `genericType(parameters, body)`, `instantiateType(generic, arguments)` | Explicit parameter identities and isolated descriptor substitution. |
| `describeType(value)`, `describeDifference(item)`, `reportDifference(item, target)` | Pure descriptions and optional source-linked reporting. |

Objects are structural: assignment permits additional fields; identity requires
the same field names regardless of order. Tuple positions and lengths matter.
Mutable list element types are invariant. Unions are flattened and deduplicated;
an empty union becomes unknown and a single member collapses to that member.
Unknown is permissive. Generic arguments are explicit; recursive named types
and flow-sensitive inference are outside this preset.

Import this preset's `typeOf` together with its `annotateWithType`: its compound
descriptor channel differs from generic `std/types`. A language may instead
reuse only the pure constructors and comparisons, as the origin-aware example
does with its own annotation records.

Example 11 teaches the full local implementation. Example 12 reuses this preset
and adds [diagnostic origins](/tutorials/diagnostic-origins).

## Opt-in backend contract

### `std/backend/integer`

This module declares the following host hooks:

```yl
export hook number(value)
export hook reference(name)
export hook add(left, right)
export hook multiply(left, right)
export hook less(left, right)
export hook declare(name, initializer)
export hook assign(name, value)
export hook block(statements)
export hook conditional(condition, yes, no)
export hook repeat(condition, body)
export hook program(statements, result)
```

Qualified identities use `std/backend/integer#`, so all consumers of this module
can implement the same stable manifest. The demonstration hosts agree on signed
`i32` values, wrapping addition/multiplication, `0`/`1` comparison results, and
nonzero conditions. The module provides declarations only; host implementations
own values, scopes, traversal, and interpretation or code generation.

The [checked backend capstone](/tutorials/backends) composes this contract with
static references and type checking, then executes the same `.ylc` using three
independent hosts.

Projection collects only the bound `item` values. An empty star list becomes `[]`. Tuple-valued or list-valued items preserve their boundaries as individual elements.

## Defining language-specific helpers

A language can export its own patterns and pipes using exactly the same mechanisms as `std/parser`. For example, a module might define keyword boundaries, delimiter conventions, or list styles appropriate to its source language.

Importing a standard helper does not activate trivia. Always configure automatic skipping explicitly in the language's [entry block](./trivia#entry-configuration).
