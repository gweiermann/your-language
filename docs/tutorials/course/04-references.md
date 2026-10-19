# 4. Give names meaning

Add a small local library before reusing the standard one. A scope is an identity
attached to syntax. A declaration contributes a name inside that scope. Lookup
searches ancestors and uses scheduled visibility; it is not a runtime map.

```yl file=definition/bindings.yl
import { ancestors, contribute, find, get, getEarlier, isPresent, text } from "core/query"
import { error } from "core/diagnostics"

symbol scope
symbol declarations
symbol reference

export operation createLexicalScope(target) {
    contribute((target, scope), target)
}

operation enclosingScope(value) = find(ancestors(value), ancestor => get((ancestor, scope)))

export operation declareName(target, name) {
    contribute((enclosingScope(name), declarations, text(name)), target)
    contribute((name, reference), target)
    contribute((target, reference), target)
}

export operation connectReference(target, name) {
    declaration := find(ancestors(name), ancestor =>
        getEarlier((get((ancestor, scope)), declarations, text(name))))
    contribute((target, reference), declaration)
    contribute((name, reference), declaration)
    when !isPresent(declaration) {
        error("Unknown name '${name}'", name)
    }
    return declaration
}

export operation referenceOf(value) = get((value, reference))
```

Replace the expression module to add references:

```yl file=definition/expression.yl
import { Name } from "./lexical"
import { connectReference } from "./bindings"

node Reference = name: Name {
    semantics { connectReference(self, name) }
}

export node Number = value: /[0-9]+/

node String = value: /"[^"\n]*"/

node Addition = left: Expression "+" right: Expression

export node Expression {
    node Addition

    node Number

    node String

    node Reference
}
```

```yl file=definition/language.yl
import { Name, Whitespace, keyword } from "./lexical"
import { Expression } from "./expression"
import { createLexicalScope, declareName } from "./bindings"

node LetDeclaration =
    keyword("let")
    name: Name
    "="
    initializer: Expression
    ";"
{
    semantics { declareName(self, name) }
}

node Program = statements: LetDeclaration* {
    semantics { createLexicalScope(self) }
}

entry Program { trivia Whitespace }
```

```text file=programs/program.txt
let count = 40;
let total = count + 2;
```

`yl project check programs/program.txt` succeeds. A missing name produces a
semantic diagnostic rather than a parse error. `:=` keeps the lookup's pending
dependencies; it is not an imperative assignment. Pending lookup does not mean
absence, and a nearer pending scope cannot be skipped just because an outer
answer is already available.

This local implementation intentionally omits duplicate-declaration reporting
and mutability metadata. [07-declarative-queries](/tutorials/declarative-scopes)
teaches the full mechanism; `std/references` adds the reusable contracts. In the
next chapter, replace `./bindings` imports with `std/references` and remove the
local module when no longer used. The call sites and explicit captures remain clear.

Previous: [Diagnostics](./03-diagnostics).

Next: [Types](./05-types).
