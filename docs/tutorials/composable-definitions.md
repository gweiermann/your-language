# Compose a language without hiding its grammar

Keep a construct's grammar and the operations connected to its captures in the
same declaration. Extract substantial logic into imported operations. A reader
can then see where every argument comes from before following the implementation.

## Keep the connection visible

Mini TypeScript's `definition/expression.yl` imports `checkAddition` from
`./typing`. Its addition node contains the complete grammar and one explicit
connection:

```yl
node Addition =
    left: Expression
    "+"
    right: Expression
{
    semantics {
        checkAddition(self, left, right)
    }
}
```

`Expression` is the exported abstract node defined in that same file. The
operation receives the current occurrence and both captures; it cannot access
an implicit `self` or undeclared captures from another file.

The supporting `typing.yl` defines the whole addition policy:

```yl
import { error } from "core/diagnostics"
import { annotateWithType, typeOf, unknown } from "std/types"
import { Type } from "./types"

export operation checkAddition(target, left, right) {
    match (typeOf(left), typeOf(right)) {
        Type::string, Type::string | Type::string, Type::number | Type::number, Type::string => annotateWithType(target, Type::string),
        Type::number, Type::number => annotateWithType(target, Type::number),
        unknown, _ | _, unknown => annotateWithType(target, unknown),
        _ => error("${left} + ${right} is not possible")
    }
}
```

`Type` is the language-owned enum in `types.yl`, with `number`, `string`, and
`boolean` variants. The standard library supplies queries and annotations; it
does not choose which operands this language may add. Pending operands and
hypothetical function arguments still use the normal dependency engine.

Run the example and inspect its inferred types:

```shell
cargo run --example mini-typescript
cargo run --example mini_typescript_host -- addition
cargo run -- project check examples/08-mini-typescript/programs/addition-errors.txt
```

The first command checks all configured valid and negative programs. The second
prints numeric and string addition results. The last exits with status 1 and
underlines the unsupported boolean additions.

## Keep small role metadata local

A short IDE block is useful context beside a token definition. MiniJS and the
backend example share this shape in their respective `lexical.yl` modules:

```yl
import { boundedBy } from "std/parser"

pattern IdentifierPart = /[_a-zA-Z0-9]/

export pattern keyword(value) = value |> boundedBy(IdentifierPart) {
    ide {
        highlightAs "keyword"
    }
}
```

Calls such as `keyword("let")` and `keyword("function")` inherit highlighting
and boundaries. `letname` is not split into a keyword and a name. The boundary
alphabet belongs to this language. A contextual wrapper can override a more
general role, such as giving a function name the `function` category instead of
the generic `variable` category.

## Explain a mechanism before abstracting it

The local library in `examples/07-declarative-queries/definition/references.yl`
builds scopes and references from `core/query`. Its three symbols identify
scope attachments, declaration keys, and reference attachments. The complete
implementation is small enough to read alongside its grammar; see
[authored lexical scopes](/tutorials/declarative-scopes) for the walkthrough.

Examples 06 and 08 reuse `std/references` rather than copying that implementation.
They use its schedule-sensitive lookup policy. Example 07 deliberately teaches
whole-scope visibility instead: references may precede their declarations.
Both policies are authored in YL.

Example 11 builds compound-type representation and comparison locally. Example
12 imports the explicit `std/types/structural` preset and adds source-origin
tracking as a separate lesson. Example 10 reuses `std/types/arithmetic` after
08 teaches the addition matrix. Each abstraction is an authored YL module whose
policy can be inspected and replaced; no native TypeScript adapter is involved.

## Keep backend dispatch beside its captures

The backend example uses the same organization:

```yl
import { add } from "std/backend/integer"

node Addition =
    left: Expression
    "+"
    right: Expression
{
    backend {
        add(left, right)
    }
}
```

`std/backend/integer` declares `export hook add(left, right)`. `Expression` is the abstract
expression node in the example's expression module. The interpreter,
JavaScript transpiler, and WebAssembly compiler implement that hook separately
under `hosts/`; each decides how to process the unevaluated captures.

Read [the backend tutorial](/tutorials/backends) for the complete host code and
execution commands. The standard module shares the hook contract; the example
keeps all three implementations visible. This capstone also uses authored
references and type checks before any host receives the program.
