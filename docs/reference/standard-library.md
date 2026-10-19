# Core and standard libraries

YL separates parser primitives from helpers written in the language itself. Import callable functionality explicitly, including primitives.

Standard modules are included with the compiler and are always available through `std/...` imports. Projects do not need a local `stdlib/` directory, a relative path into this repository, or an alias in `yl.json`. The source files under the repository's `stdlib/` directory are the implementation of these modules; consumers use their stable module IDs. Explicit imports select which helpers and opinionated policies a language uses.

```yl
import { notAhead, notBehind } from "core/parser"
import { boundedBy, separatedBy, Trailing } from "std/parser"
```

`core/parser` exposes primitives that need compiler/runtime support. `std/parser` composes those primitives and ordinary structural rewrites into reusable helpers.

## Core parser

`notAhead(grammar)` and `notBehind(grammar)` are engine primitives. Their complete
contract and examples are in [the core library reference](/api/core#core-parser).
The authored parser helpers below compose them rather than replacing them.

## Standard parser

The module ID is `std/parser`; these are its three exports.

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

Projection collects only the bound item values. Empty star lists become `[]`; tuple/list-valued items retain their element boundaries.

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

## std/references

This authored module connects syntax identities to declarations in lexical scopes.
It does not implement a runtime variable map or choose target-language types.

```yl
import { createLexicalScope, declareName, connectReference, referenceOf } from "std/references"
```

| Export | Parameters and behavior |
| --- | --- |
| `createLexicalScope(target)` | Attach a scope identity to the supplied syntax occurrence. Operations have no implicit `self`; pass the current occurrence explicitly. |
| `enclosingScope(value)` | Find the nearest ancestor carrying a lexical scope; terminal captures begin with their containing occurrence. |
| `declareName(target, name, reassignable = true)` | Register `text(name)` in its enclosing scope, associate the name and declaration occurrence with `target`, and diagnose duplicates or a missing scope. `name` should exclude punctuation such as a dollar-sign prefix. |
| `resolve(name)` | Read the nearest visible declaration identity, without creating associations or reporting an unknown name. |
| `connectReference(target, name)` | Resolve the name, attach its declaration identity to both syntax arguments, report an unknown name when absent, and return the declaration identity. |
| `referenceOf(value)` | Read an existing association; unassociated syntax returns `absent`. |
| `scope`, `declarations`, `referenceIdentity` | Exported symbols naming scope, name-table, and association channels. |
| `reassignableSymbol`, `nameSymbol` | Exported metadata symbols for a declaration's mutability flag and source name. |

Declaration visibility follows the semantic schedule: source order, named groups,
explicit precedence, and boundaries. Pending visible inner declarations are
resolved before considering an outer binding. Same-scope duplicates are errors;
inner declarations can shadow outer names. Explicit function registration can be
hoisted before body analysis. A self-initializer is rejected by the composed
language's visibility rules. See [references](/guide/references) and
[semantic ordering](/reference/meaning-libraries).

## std/types

This target-neutral annotation library exports only `unknown` as a type sentinel.
Your language supplies primitive symbols, enum variants, signature keys, and
operator policy. It does not depend on `std/references`; the language connects
syntax and declaration identities explicitly.

```yl
import { annotateWithType, typeOf, restrictTypeTo } from "std/types"
```

| Export | Parameters and behavior |
| --- | --- |
| `annotateWithType(key, value)` | Add possible types at an identity or tuple key. Repeated candidates are deduplicated; distinct candidates form alternatives. `unknown` does not erase known candidates. |
| `candidates(key)` | Read currently available distinct candidates as a list; this is an approximation for accumulation. |
| `typeOf(key)` | Read a type or alternatives; an existing key with no candidate has type `unknown`. An absent key returns `absent`. |
| `compatible(actual, expected)` | Require every actual candidate to be among the expected candidates. `unknown` is permissive; this is membership policy, not structural comparison. |
| `restrictTypeTo(key, expected, target = key, related = absent)` | Validate the observed type without adding the expected type. Underline `target`, optionally relating an annotation at `related`. |
| `resolveTypeWith(result, assumptions)` | Read a result under isolated temporary assignments. Each assumption is `(logicalTypeKey, temporaryType)` or `(logicalTypeKey, temporaryType, originSyntax)`. Forward demanded authored diagnostics without mutating the original assignments. |
| `unknown`, `type` | The unresolved-type sentinel and exported annotation-channel symbol. |

For a binding, annotate its declaration identity from the initializer, then check
an optional annotation. Do not contribute the declared type as if it were inferred
from the initializer:

```yl
operation bindingType(name, initializer, annotation) {
    annotateWithType(referenceOf(name), typeOf(initializer))
    restrictTypeTo(referenceOf(name), typeOf(annotation), initializer, annotation.value)
}
```

This fragment additionally imports `referenceOf` from `std/references`. Its
`annotation` is an optional syntax node capturing the type word as `value`.
Absence suppresses only the dependent restriction. The complete
[type-inference walkthrough](/tutorials/std-types) defines those nodes and
explains per-parameter function keys and independent calls.

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
| `describeType(value)`, `describeDifference(item)`, `reportDifference(item, target)` | Pure descriptions; report one difference at the supplied syntax target. |
| `typeAnnotations`, `writeRestriction` | Exported symbols naming this preset's annotation and declared-write channels. |

`compareTypes` returns a list of `(path, actual, expected, reason)` differences; an empty list means success. Paths contain field names and tuple-position labels. The preset deliberately keeps source origins separate; the diagnostic-origins example adds them.

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


## Defining language-specific helpers

A language can export its own patterns and pipes using exactly the same mechanisms as `std/parser`. For example, a module might define keyword boundaries, delimiter conventions, or list styles appropriate to its source language.

Importing a standard helper does not activate trivia. Always configure automatic skipping explicitly in the language's [entry block](./trivia#entry-configuration).
