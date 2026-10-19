# 4. Give names meaning

Parsing `count` tells us its spelling. It does not tell us **which declaration**
that name refers to. This lesson connects a use of a name to its declaration,
and reports an unknown name. Continue inside **`first-language/`**.

## Decide what a name means

Our target program will be:

```text
let count = 40;
let total = count + 2;
```

The first `count` declares a name. The second uses it. They are different
occurrences in the text, but refer to the same declaration. Each syntax
occurrence has a distinct **identity**, even when two occurrences have identical
text. We can attach information to that identity without changing its AST fields.

A **lexical scope** is a region in which names are visible. Here the entire
`Program` is one scope. Later a function can introduce another scope, so the
same spelling can have a different declaration inside it.

| Scope | Name text | Declaration identity |
| --- | --- | --- |
| This program | `count` | The first declaration |
| This program | `total` | The second declaration |

Our library stores relationships as **facts**, each with a key and a value.
A **symbol** such as `declarations` is a unique label for a kind of relationship;
it is different from the source text `"count"`, which is just a string.
A **tuple** groups values in order: `(programIdentity, declarations, "count")`
is one compound key. Its value is the declaration identity.

## Read the query tools before building the library

`contribute(key, value)` adds a fact. `get(key)` asks for its value. These are
**queries**: an answer can depend on facts that have not finished being produced.
Such an answer is **pending**, not missing. A settled lookup with no answer is
`absent`. A dependent contribution waits while pending; if its input is absent,
that dependent path produces no fact. Independent actions can still succeed.

`ancestors(value)` lists the containing syntax occurrences, nearest first.
`find(list, item => query)` tries the query for each item and takes the first
present answer. The arrow names the current item: `ancestor` is the containing
occurrence being inspected. A nearer pending answer must settle before lookup
can safely continue to a more distant scope.

`isPresent(answer)` is false for `absent`; `!` negates a boolean, so
`when !isPresent(declaration)` reports an error only after lookup settles missing.

An `operation` packages reusable work. Unlike a node's semantic block, an
operation does not automatically know the current syntax: callers pass it.
`return` describes its result. `declaration := ...` names a query result along
with its pending dependencies; it is not a mutable variable or a command to
finish that lookup immediately.

Now create the small teaching library. Read its operations in this order:

1. `createLexicalScope` marks the program as a scope by attaching its identity.
2. `enclosingScope` finds the nearest marked ancestor.
3. `declareName` stores the scope/name key and connects the declaration's name.
4. `connectReference` finds a declaration visible earlier in the scheduled order,
   connects the use to it, or diagnoses the missing name.
5. `referenceOf` retrieves the established connection.

`text(name)` reads the spelling; `getEarlier` respects declaration visibility
from the scheduler's source order and precedence. It is not target-program
execution and does not run an assignment.

<p class="course-file" data-file="definition/bindings.yl"><strong>Create file:</strong> <code>definition/bindings.yl</code></p>

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

## Connect grammar captures to the library

`semantics { ... }` gives recognized syntax meaning. Inside it, `self` identifies
the current node occurrence and `name` is its grammar capture. The new `Reference`
node calls `connectReference(self, name)` for each recognized use of a name.
Replace the expression module:

<p class="course-file" data-file="definition/expression.yl"><strong>Replace file:</strong> <code>definition/expression.yl</code></p>

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

The entry file now declares each `LetDeclaration` and gives `Program` a scope.
Both use the same operations we just defined; their captures remain visible
beside the calls. Replace the entry file:

<p class="course-file" data-file="definition/language.yl"><strong>Replace file:</strong> <code>definition/language.yl</code></p>

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

<p class="course-file" data-file="programs/program.txt"><strong>Replace file:</strong> <code>programs/program.txt</code></p>

```text file=programs/program.txt
let count = 40;
let total = count + 2;
```

## Check the connection

Run `yl project check programs/program.txt` from `first-language/`. It exits
**0** without output. The second declaration can find `count` in this program's
scope. Now replace its use of `count` with `missing` and check again. The command
exits **1**, reports `Unknown name 'missing'`, and underlines that name. Restore
`count`; the command succeeds again.

 A missing name produces a
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
