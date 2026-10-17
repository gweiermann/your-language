# Authored semantic standard libraries

For a step-by-step introduction with runnable examples, follow [Build type inference with std/types](/tutorials/std-types).

The compiler bundles `std/references` and `std/types` as ordinary YL modules. Their operations lower to generic attachment actions and compiled pure query functions during compilation. Reloaded `.ylc` files execute indexed query instructions and need no library source, native lexical stack or native type solver.

## Reference identities and scopes

```yl
import { createLexicalScope, declareName, connectReference, referenceOf } from "std/references"

node Declaration = "let" name: Name ";" {
    semantics { declareName(self, name) }
}

node Reference = name: Name {
    semantics { connectReference(self, name) }
}

node Program = (Declaration | Reference)* {
    semantics { createLexicalScope(self) }
}
```

`createLexicalScope(target)` contributes `(target, scope) -> target`. Call it as `createLexicalScope(self)` inside semantics; operations have no implicit `self`. Scope identities are attached to syntax; they are not a mutable current-scope stack. `declareName(target, name, reassignable = true)` contributes `(scopeIdentity, declarations, text(name)) -> declarationIdentity` and associates the exact name capture with that identity. `connectReference(target, name)` resolves enclosing scopes, returns the declaration identity, associates the occurrence and its name capture, and reports an unknown-name diagnostic if no declaration exists.

`referenceOf(value)` reads the association of a capture or syntax identity. An unassociated value returns `absent`. Dollar signs, prefixes and other target-language syntax can remain outside the supplied name capture.

The library diagnoses same-scope duplicates and preserves inner shadowing. Declaration visibility uses the static meaning schedule: a declaration must precede its use. This includes source order, named groups, explicit precedence and boundaries. A self-initializer is rejected; explicit precedence can hoist function registration ahead of uses. Pending visible declarations are resolved before selecting an outer binding. Exported metadata symbols include `scope`, `declarations`, `referenceIdentity`, `reassignableSymbol` and `nameSymbol`.

Syntax captures and `self` carry their identities directly; redundant identity wrappers are not exported.

## Type evidence and restrictions

```yl
import { annotateWithType } from "std/types"

symbol number
symbol string

node Number = /[0-9]+/ {
    semantics { annotateWithType(self, number) }
}
```

The standard library defines `unknown` and the metadata-channel symbol `type`. Target-language types such as `number`, `string`, function signatures and record schemas are language-owned values.

| Operation | Behavior |
| --- | --- |
| `annotateWithType(key, value)` | Contribute possible type evidence at `(key, type)`. Repeated evidence is deduplicated; different evidence forms a union. |
| `candidates(key)` | Read the current distinct evidence as a list during fixed-point propagation. |
| `typeOf(key)` | Read one type, a set of alternatives, or `unknown` for an existing key with no evidence. An absent key returns `absent`. |
| `compatible(actual, expected)` | Check candidate membership; `unknown` remains permissive. Languages can author different structural compatibility rules. |
| `restrictTypeTo(key, expected, target = key, related = absent)` | Validate observed evidence without changing it. Emit an authored diagnostic at `target`, optionally marking a related annotation. |
| `resolveTypeWith(result, assumptions)` | Recompute a result under explicit temporary type assignments, preserving the original store. |

Keys can be syntax identities, resolved declaration identities or nested tuples. `(functionIdentity, returnType)` can describe a function's return without assigning a type to its body block. An ordered list is an ordinary compound value, not automatically a union. Repeated syntax captures are lists: use `map(captures, value => typeOf(value))` to form a positional type list.

A restriction alone does not annotate a value. An initializer contributes actual evidence; an explicit declaration annotation restricts that evidence:

```yl
operation bindingType(name, initializer, annotation) {
    annotateWithType(referenceOf(name), typeOf(initializer))
    restrictTypeTo(referenceOf(name), typeOf(annotation), initializer, annotation.value)
}
```

An optional absent annotation suppresses the dependent restriction without suppressing initializer evidence. No `isPresent` guard is required for this path. Missing-name diagnostics deliberately inspect absence in the reference library.

The initializer remains the primary error location; the related source points to the annotation's type word. Type policy is in the library and language definition, so a structural language can replace membership checks with authored field/subset queries.

## Generic function relationships

Each assumption is `(logicalTypeKey, temporaryType)`, optionally followed by an originating source capture. Only supplied keys are overridden. Dependent relationships are recalculated; outer captures and original parameter evidence remain unchanged.

```yl
resolveTypeWith(
    (functionIdentity, returnType),
    [((functionIdentity, parameters, "value"), number, argument)]
)
```

`resolveTypeWith` forwards authored error and warning contributions associated with the demanded relationships. A diagnostic whose dependencies reach one explicit origin underlines that argument; otherwise it underlines the invoking syntax and retains the original diagnostic location as related source. Successful union alternatives remain available when another alternative reports an error.

Explicit return annotations can serve as authored recursive-call anchors. Unanchored recursive hypotheses report structured cycles; the evaluator does not guess a target-language return type. Hypothetical nesting, alternative combinations and artifact depth are bounded.

The [Mini TypeScript example](https://github.com/gweiermann/your-language/tree/main/examples/mini_typescript) demonstrates per-parameter keys, independent identity calls, function aliases, higher-order calls, unions, explicit annotations and source diagnostics using these authored modules.
