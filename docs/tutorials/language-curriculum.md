# From grammar to a composed language

The examples form a learning path from visible engine mechanisms to reusable
language libraries and complete consumers. Each directory is a project with a
`yl.json`, grammar under `definition/`, and target programs under `programs/`.
Programs use `.txt` so their project's grammar controls editor highlighting.

Use either Cargo name from [the catalog](https://github.com/gweiermann/your-language/tree/main/examples).
For example, `cargo run --example getting-started` and
`cargo run --example 01-getting-started` run the same project. Each command
checks valid programs and expected negative fixtures. Backend execution is a
separate, explicit step.

## 1. Recognize syntax and expose its structure

Begin with **01-getting-started** and **02-constraints**. They require no
reference or type library. A declaration names its captures directly, and
syntax checks report diagnostics on those captures.

```yl
node Number = value: /[0-9]+/

entry Number
```

This complete language parses a number and preserves its `value` capture. The
[first-language tutorial](/tutorials/first-language) expands this into the
getting-started project; [syntax diagnostics](/tutorials/diagnostics) adds
optional syntax and checks.

**03-mini-js** composes modules, recursive expressions, precedence, and pipes.
It reuses the authored `std/parser` separator and boundary helpers; its own
grammar still chooses identifiers, keywords, and syntax. **04-ide-metadata**
adds examples and contextual token roles. **05-tuple-access** introduces
positional query values. These lessons provide the syntax and data tools used
by later semantic operations.

```shell
cargo run --example mini-js
cargo run --example ide-metadata
cargo run --example tuple-access
```

Each command verifies its project, including intentional errors. Follow
[modules and patterns](/tutorials/modules-and-patterns) for the reusable
`keyword(value)` pattern that combines recognition and highlighting.

## 2. Author references, then reuse them

For a mechanism-first route, study **07-declarative-queries** before
**06-references**. Catalog numbers and existing commands remain stable.

The small local library in 07 begins with scope identities:

```yl
import { contribute } from "core/query"

symbol scope

export operation createLexicalScope(target) {
    contribute((target, scope), target)
}
```

Its complete `definition/references.yl` also defines declaration keys and
nearest-scope lookup. No native reference policy or mutable current-scope
variable is required. Its grammar passes visible captures into these local
operations. [Authored lexical scopes](/tutorials/declarative-scopes) explains
the implementation and demonstrates inner shadowing, missing names, and
duplicates.

Then 06 composes a larger function grammar with canonical `std/references`.
The standard library adds schedule-sensitive visibility; 07 deliberately uses
whole-scope visibility. Reuse is an explicit policy choice.

```shell
cargo run --example declarative-queries
cargo run --example references
```

The first prints duplicate- and missing-name errors for its negative fixture;
the aggregate run succeeds because those errors are expected. The second adds
functions, closures, and ordering. See each README for direct negative commands
that exit with status 1.

## 3. Connect types to syntax and declarations

**08-mini-typescript** builds on references and `std/types`. Literal nodes
publish language-owned type values, references share their declarations' type
information, and annotation checks restrict initializers without changing
their actual inferred types. Its `typing.yl` implements the complete addition
matrix; the grammar calls `checkAddition(self, left, right)` beside those
captures. [Composable definitions](/tutorials/composable-definitions) shows
both halves, and [the type-inference tutorial](/tutorials/std-types) explains
the attachment channels and function signatures.

**09-type-instantiation** isolates temporary argument assumptions.
**10-declarative-functions** exposes lower-level function queries and reuses
the opt-in `std/types/arithmetic` policy for addition. The caller supplies its
own primitive type identities; the generic `std/types` module has no numeric
or string catalog.

```shell
cargo run --example mini_typescript_host -- generics
cargo run --example mini_typescript_host -- addition
cargo run --example declarative-functions
```

The host reports independent number and string results for generic identity
calls. Addition reports number for two numeric operands and string for the
supported string combinations. Unsupported known operands produce source
diagnostics. This is a selected TypeScript-like subset, not TypeScript
compatibility.

## 4. Build compound policies, then add diagnostic origins

**11-compound-types** implements descriptors, recursive comparison, generic
substitution, member access, indexing, and write contracts in its own YL
helpers. Read that implementation before using **12-diagnostic-origins**,
which imports the opt-in `std/types/structural` model and adds origin tracking.

The structural preset makes its opinionated policies explicit: a fixed
primitive catalog, structural object assignment, positional tuples, normalized
unions, and invariant mutable lists. For example, a reusable check can be
written directly over its exported descriptors:

```yl
import { isAssignableTo, listType, numberType } from "std/types/structural"

export operation acceptsNumberList(actual) =
    isAssignableTo(actual, listType(numberType()))
```

This helper checks a descriptor; it does not parse syntax or emit a diagnostic.
The grammar chooses what descriptor to check, and a separate reporter chooses
the source locations. Keeping these roles separate makes it possible for 12 to
trace a deep mismatch to an original literal several declarations away.

```shell
cargo run --example compound-types
cargo run --example diagnostic-origins
cargo run -- project check examples/12-diagnostic-origins/programs/invalid-scattered.txt
```

The last command intentionally fails, grouping the offending value, the
annotation, and the checked initializer. Follow [compound types](/tutorials/compound-types)
and [diagnostic origins](/tutorials/diagnostic-origins) for the full implementations.

## 5. Run one checked language through three hosts

**14-backend-hooks** combines reference analysis, integer type checks, IDE
metadata, and the opt-in `std/backend/integer` hook contract. Its values are
signed `i32`, not the richer structural descriptors from 11 and 12. Addition
and multiplication wrap; comparison produces `0` or `1`.

```javascript
let answer: i32 = 42;
result answer;
```

The same compiled language image drives an interpreter, JavaScript transpiler,
and WebAssembly compiler. All three host implementations remain readable under
`hosts/`. They own traversal, runtime values or emitted code, scopes, and control
flow. The standard module only shares their hook signatures.

```shell
cargo run --example backend-hooks
cargo run --example backend_interpret
cargo test --test backend_examples
```

The static project runner verifies positive and negative programs, the
interpreter prints `42`, and integration tests execute both generated
JavaScript and WebAssembly. Incompatible annotations stop execution before any
hook runs. Follow [the backend tutorial](/tutorials/backends) for generation
commands and exact limits.

**13-formatting** is an independent tooling lesson: use it alongside any stage
to explore match-arm alignment. The curriculum keeps syntax, query mechanics,
target-language policies, diagnostics, and host execution separable so that a
later language can compose or replace each layer.
