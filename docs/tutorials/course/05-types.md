# 5. Infer types and check annotations

Keep the lexical module. Replace the expression and entry files, and add
`types.yl`. The code now reuses `std/references` instead of the local teaching
library. The unused local `bindings.yl` can be removed.

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

```text file=programs/program.txt
let count: number = 40;
let total = count + 2;
let label: string = "answer: " + total;
```

```text file=programs/invalid.txt expect-error
let count: number = "wrong";
```

Check each file with `yl project check programs/program.txt` or
`yl project check programs/invalid.txt`. The valid program passes; the invalid
one underlines `"wrong"` and relates the `number` annotation. A restriction
checks an inferred result; contributing `number` from the annotation would
incorrectly contaminate it.

The language owns `Type::number` and `Type::string`, including this addition
matrix. `unknown` comes from `std/types`. The optional absent annotation ends
only the dependent check. For reuse, `std/types/arithmetic` supplies an opt-in
matrix with explicit caller-owned type identities; it does not impose this policy
on every language. Continue with the full [type-library reference](/reference/standard-library#std-types).

Previous: [References](./04-references).

Next: [Functions](./06-functions).
