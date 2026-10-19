# 5. Infer types and check annotations

A value is something such as `40` or `"hello"`. Its **type** describes which
kind of value it is: a number or a string. Type inference works out that kind
without the program spelling it out. A type annotation, such as `: number`,
asks us to check a requirement rather than guess a different answer.

Continue inside **`first-language/`**. Keep the lexical file, replace the
expression and entry files below, and add `types.yl`. We now reuse the fuller
`std/references` library. Remove the unused local `bindings.yl`; its teaching
operations are replaced by the standard library's duplicate-name checks and
reference contracts.

## Give the language its own type names

`enum Type { number string }` declares a named group of distinct symbols.
`Type::number` selects one member; `::` is qualification, not a capture colon.
These symbols are our language's policy. The generic `std/types` library does
not decide that every language has numbers or strings.

`annotateWithType(identity, type)` attaches a possible type to an identity.
`typeOf(identity)` reads the resulting type. Multiple contributions can form a
union of possibilities. A number literal contributes `Type::number`; a string
literal contributes `Type::string`.

The `TypeName` family below recognizes the words used in annotations. Its
selected child contributes a type. `children(self)` lists those child
occurrences, and `first(...)` selects the chosen one. The family's final semantic
line copies that child's type to its wrapper, so querying the wrapper works too.
The `Annotation` node similarly forwards the captured `value` type. Its colon
literal recognizes the program's `:`, while `value:` names a grammar capture.

Create the annotation module:

<p class="course-file" data-file="definition/types.yl"><strong>Create file:</strong> <code>definition/types.yl</code></p>

```yl file=definition/types.yl
import { annotateWithType, typeOf } from "std/types"
import { children, first } from "core/query"

export enum Type { number string }

export node TypeName {
    node NumberType = "number" {
        semantics { annotateWithType(self, Type::number) }
    }

    node StringType = "string" {
        semantics { annotateWithType(self, Type::string) }
    }

    semantics { annotateWithType(self, typeOf(first(children(self)))) }
}

export node Annotation = ":" value: TypeName {
    semantics { annotateWithType(self, typeOf(value)) }
}
```

## Infer the type of each expression

A reference first connects to a declaration. The immutable `ref := ...` binding
lets us reuse that declaration identity when asking for its type. This does not
force an unresolved lookup to finish: the dependent annotation waits for it.

Addition needs a language-specific decision. `match (leftType, rightType)`
examines a tuple of two types. A row before `=>` describes one combination; the
action after it attaches the result. `|` puts several combinations on one row.
`_` matches any value, and the final wildcard handles unsupported combinations.
Here we choose JavaScript-like number/string addition; this policy lives in our
definition, rather than inside the engine. `unknown` means no useful type has
been inferred; it comes from `std/types`.

Replace the expression module, reading the literal nodes before `Addition`:

<p class="course-file" data-file="definition/expression.yl"><strong>Replace file:</strong> <code>definition/expression.yl</code></p>

```yl file=definition/expression.yl
import { Name } from "./lexical"
import { Type } from "./types"
import { connectReference } from "std/references"
import { annotateWithType, typeOf, unknown } from "std/types"
import { children, first } from "core/query"
import { error } from "core/diagnostics"

node Reference = name: Name {
    semantics {
        ref := connectReference(self, name)
        annotateWithType(self, typeOf(ref))
    }
}

node Number = value: /[0-9]+/ {
    semantics { annotateWithType(self, Type::number) }
}

node String = value: /"[^"\n]*"/ {
    semantics { annotateWithType(self, Type::string) }
}

node Addition = left: Expression "+" right: Expression {
    semantics {
        match (typeOf(left), typeOf(right)) {
            Type::number, Type::number => annotateWithType(self, Type::number),
            Type::string, Type::string | Type::string, Type::number | Type::number, Type::string => annotateWithType(self, Type::string),
            unknown, _ | _, unknown => annotateWithType(self, unknown),
            _ => error("These operands cannot be added.", self)
        }
    }
}

export node Expression {
    node Addition

    node Number

    node String

    node Reference

    semantics { annotateWithType(self, typeOf(first(children(self)))) }
}
```

## Separate inference from a requirement

`annotation: Annotation?` captures an annotation **if present**; `?` means zero
or one occurrence. An omitted annotation yields `absent`, rather than a made-up
type. The declaration's inferred type comes from `initializer`.
`restrictTypeTo` checks that inferred result against the annotation and reports
an incompatibility; it does not add the annotation to the inferred result.

The final two arguments give diagnostic locations: the initializer is the bad
value, and `annotation.value` is the required type's source. If no annotation
exists, the dependent restriction ends without a diagnostic.

Replace the entry file:

<p class="course-file" data-file="definition/language.yl"><strong>Replace file:</strong> <code>definition/language.yl</code></p>

```yl file=definition/language.yl
import { Name, Whitespace, keyword } from "./lexical"
import { Expression } from "./expression"
import { Annotation } from "./types"
import { createLexicalScope, declareName, referenceOf } from "std/references"
import { annotateWithType, typeOf, restrictTypeTo } from "std/types"

node LetDeclaration =
    keyword("let")
    name: Name
    annotation: Annotation?
    "="
    initializer: Expression
    ";"
{
    semantics {
        declareName(self, name)
        annotateWithType(referenceOf(name), typeOf(initializer))
        restrictTypeTo(referenceOf(name), typeOf(annotation), initializer, annotation.value)
    }
}

node Program = statements: LetDeclaration* {
    semantics { createLexicalScope(self) }
}

entry Program { trivia Whitespace }
```

<p class="course-file" data-file="programs/program.txt"><strong>Replace file:</strong> <code>programs/program.txt</code></p>

```text file=programs/program.txt
let count: number = 40;
let total = count + 2;
let label: string = "answer: " + total;
```

<p class="course-file" data-file="programs/invalid.txt"><strong>Replace file:</strong> <code>programs/invalid.txt</code></p>

```text file=programs/invalid.txt expect-error
let count: number = "wrong";
```

## Observe an inferred result and a failed requirement

In the valid program, `count` is a number because `40` is a number. `total` has
no annotation: its reference to `count` and the literal `2` select the
number/number addition row. `label` selects the string/number row and becomes a
string. Neither `total` nor `label` needs a separate solver call.

Check each file with `yl project check programs/program.txt` or
`yl project check programs/invalid.txt`. The valid program passes; the invalid
one underlines `"wrong"` and relates the `number` annotation. A restriction
checks an inferred result; contributing `number` from the annotation would
incorrectly contaminate it.

Change the invalid program's requirement from `number` to `string` and rerun
its check. It exits **0**, because the initializer was already a string. Change
it back to preserve the intentional error. Changing the requirement must never
make `"wrong"` infer as `number | string`: a restriction checks, rather than
contributes, the type.

The language owns `Type::number` and `Type::string`, including this addition
matrix. `unknown` comes from `std/types`. The optional absent annotation ends
only the dependent check. For reuse, `std/types/arithmetic` supplies an opt-in
matrix with explicit caller-owned type identities; it does not impose this policy
on every language. Continue with the full [type-library reference](/reference/standard-library#std-types).

Previous: [References](./04-references).

Next: [Functions](./06-functions).
