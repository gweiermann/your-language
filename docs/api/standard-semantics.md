# Authored semantic standard libraries

The compiler bundles `std/references` and `std/types` as ordinary YL modules. Their operations expand into generic attachment queries during compilation. Reloaded `.ylc` files execute normalized query IR and need no library source, native lexical stack or native type solver.

## Reference identities and scopes

```yl
import { lexicalScope, declare, use, referenceOf } from "std/references"

node Declaration = "let" name: Name ";" {
    meanings { declare(name) }
}

node Reference = name: Name {
    meanings { use(name) }
}

node Program = (Declaration | Reference)* {
    meanings { lexicalScope() }
}
```

`lexicalScope()` contributes `(self, scope) -> self`. Scope identities are attached to syntax; they are not a mutable current-scope stack. `declare(name, reassignable = true)` contributes `(scopeIdentity, declarations, text(name)) -> declarationIdentity` and associates the exact name capture with that identity. `use(name)` resolves enclosing scopes, associates the use and its name capture, and reports an unknown-name diagnostic if no declaration exists.

`reference()` reads the association of `self`. `referenceOf(value)` reads the association of a capture or syntax identity. An unassociated value returns `absent`. Dollar signs, prefixes and other target-language syntax can remain outside the supplied name capture.

The library diagnoses same-scope duplicates and preserves inner shadowing. Declaration visibility uses the static meaning schedule: a declaration must precede its use. This includes source order, named groups, explicit precedence and boundaries. A self-initializer is rejected; explicit precedence can hoist function registration ahead of uses. Pending visible declarations are resolved before selecting an outer binding. Exported metadata symbols include `scope`, `declarations`, `referenceIdentity`, `reassignableSymbol` and `nameSymbol`.

`std/variables` aliases this same module and its symbol identities. Compatibility syntax helpers `identity()`, `identityOf(value)`, `identities(value)` and `childIdentity()` select syntax identities; new definitions can use `self`, captures and `children(self)` directly.

## Type evidence and restrictions

```yl
import * as types from "std/types"

symbol number
symbol string

node Number = /[0-9]+/ {
    meanings { types::annotate(self, number) }
}
```

The standard library defines `unknown` and the metadata-channel symbol `type`. Target-language types such as `number`, `string`, function signatures and record schemas are language-owned values.

| Operation | Behavior |
| --- | --- |
| `annotate(key, value)` | Contribute possible type evidence at `(key, type)`. Repeated evidence is deduplicated; different evidence forms a union. |
| `candidates(key)` | Read the current distinct evidence as a list during fixed-point propagation. |
| `typeof(key)` | Read one type, a set of alternatives, or `unknown` when no evidence is available. |
| `compatible(actual, expected)` | Check candidate membership; `unknown` remains permissive. Languages can author different structural compatibility rules. |
| `restrictTo(key, expected, target = key, related = absent)` | Validate observed evidence without changing it. Emit an authored diagnostic at `target`, optionally marking a related annotation. |
| `resolveWith(result, assumptions)` | Recompute a result under explicit temporary type assignments, preserving the original store. |

Keys can be syntax identities, resolved declaration identities or nested tuples. `(functionIdentity, returnType)` can describe a function's return without assigning a type to its body block. An ordered list is an ordinary compound value, not automatically a union. Repeated syntax captures are lists: use `map(captures, value => types::typeof(value))` to form a positional type list.

A restriction alone does not annotate a value. An explicit declaration annotation usually contributes both evidence and a restriction:

```yl
operation bindingType(name, initializer, annotation) {
    types::annotate(referenceOf(name), types::typeof(initializer))
    types::annotate(referenceOf(name), types::typeof(annotation))
    types::restrictTo(referenceOf(name), types::typeof(annotation), initializer, annotation.value)
}
```

The initializer remains the primary error location; the related source points to the annotation's type word. Type policy is in the library and language definition, so a structural language can replace membership checks with authored field/subset queries.

## Generic function relationships

Each assumption is `(logicalTypeKey, temporaryType)`, optionally followed by an originating source capture. Only supplied keys are overridden. Dependent relationships are recalculated; outer captures and original parameter evidence remain unchanged.

```yl
types::resolveWith(
    (functionIdentity, returnType),
    [((functionIdentity, parameters, "value"), number, argument)]
)
```

`resolveWith` forwards authored error and warning contributions associated with the demanded relationships. A diagnostic whose dependencies reach one explicit origin underlines that argument; otherwise it underlines the invoking syntax and retains the original diagnostic location as related source. Successful union alternatives remain available when another alternative reports an error.

Explicit return annotations can serve as authored recursive-call anchors. Unanchored recursive hypotheses report structured cycles; the evaluator does not guess a target-language return type. Hypothetical nesting, alternative combinations and artifact depth are bounded.

The [Mini TypeScript example](https://github.com/gweiermann/your-language/tree/main/examples/mini_typescript) demonstrates per-parameter keys, independent identity calls, function aliases, higher-order calls, unions, explicit annotations and source diagnostics using these authored modules.
