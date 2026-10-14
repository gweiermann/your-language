# Mini TypeScript inference experiment

This example demonstrates language-authored typing over a small native contribution library. It includes primitive annotations, bindings, references, function signatures, calls, recursive inference, aliases, higher-order parameters and locally defined addition rules. It is an analysis experiment, not a TypeScript implementation.

```sh
cargo run --example mini_typescript
cargo run --example mini_typescript -- higher-order
cargo run --example mini_typescript -- union
cargo run --example mini_typescript -- union-errors
cargo run --example mini_typescript -- errors
cargo run --example mini_typescript -- union --json
```

Success prints inferred binding types. Error modes underline incompatible source values, retain related type annotations and exit 1. `--json` emits the AST, retained occurrences, semantic records and structured diagnostics. The public API tests also serialize `.ylc`, reload it and analyze concurrently with isolated state.

## Definition modules

`language.yl` selects entry and trivia. `types.yl` authors primitive and slot identities. `typing.yl` contains reusable operation wrappers and the variable/type bridge. `lexical.yl`, `parameter.yl`, `expression.yl`, `function.yl` and `let.yl` organize grammar definitions. Programs are separate files in `programs/`.

## Native type mechanisms

`std/types` exports exactly `annotate`, `restrictTo`, `typeof` and the `unknown` identity. The language owns `number`, `string`, `boolean`, `Function`, `parameters` and `returnType`.

`annotate(key, type)` accumulates possible types and directed dependencies. Two contributions of number and string produce `number | string`. `restrictTo(key, number)` checks every known member; the string is diagnosed without removing it. Unknown remains explicit and permissive, while incompatible known members still fail.

`typeof(key)` returns a managed deferred query. When no facts exist, an allowed contract provides static information. Multiple allowed contracts intersect structurally; incompatible contracts are diagnosed even without contributed facts. An unrestricted read remains pending until settlement, when its fallback is unknown. Explicit unknown contributions are retained even after known facts arrive. No visible solver call is required.

A key is an identity or `(identity, slotSymbol)`. Ordered key lists provide explicit positional relationships. Structural list alternatives preserve their correlation. Base possibilities flow one way; references to a structural value share its authored slot handles, including slots added later.

## Syntax-local facts and binding handles

Syntax identity queries belong to `std/syntax`, independently of lexical name resolution. `identity()` selects the current original occurrence; `identityOf(value)` selects one captured root; `identities(value)` preserves repeated-root order. `childIdentity()` supports explicit transparent inheritance. Pipes retain original occurrence identities.

```yl
import * as types from "std/types"
import * as syntax from "std/syntax"

export operation annotateWithType(value) {
    types::annotate(syntax::identity(), value)
}

export operation typeof(value) = types::typeof(syntax::identityOf(value))

export operation inheritType() {
    types::annotate(syntax::identity(), types::typeof(syntax::childIdentity()))
}

export operation restrictTypeTo(value) {
    types::restrictTo(syntax::identity(), typeof(value))
}
```

The variable library creates ordinary named bindings. Declarations and resolved references share a binding identity. `typing.yl` bridges those opaque handles to syntax-local contributions; the type kernel does not inspect scopes, declaration names or variable-library fields.

Explicit annotations both contribute their declared type and restrict the binding. Initializer facts remain separate and flow into that binding. Declaration and reference occurrences share accumulated binding facts through explicit bidirectional authored bridges. Their original syntax identities remain distinct; initializer transfer is one way. Consequently `let count: number = "wrong";` retains number and string, and diagnoses the string with primary `"wrong"` and related `number`.

## Language-authored functions

`functionSignature` stores parameter and return relationships in authored slots. It explicitly contributes the parameter list into the parameter slot and provides a positional reverse relationship from that slot into formal parameter keys. Calls contribute argument evidence to the same structural slot; return facts flow into the call expression. There are no native function-specific operations or slot names.

Aliases retain structural handles, so higher-order evidence reaches the original parameter/return relationship. Renaming the slot symbols preserves behavior. This prototype still has shared signatures: calling an unannotated identity function with a number and a string makes its parameter and both call results `number | string`. Independent generic instantiation is a subsequent feature.

## Addition and unions

Addition remains a local meaning rule:

```yl
match (typeof(left), typeof(right)) {
    string, string | string, number | number, string => annotateWithType(string),
    number, number                                 => annotateWithType(number),
    unknown, _ | _, unknown                         => annotateWithType(unknown),
    _                                              => error("${left} + ${right} is not possible")
}
```

The match checks every distinct possible pair. Supported pairs contribute result types; an unsupported known pair emits an error without cancelling supported pairs. Late facts trigger new pairs. Literal type-contribution effects are idempotent, and warnings/errors are emitted once per distinct pair and case/action at the source occurrence. Ordinary scalar matches and ordinary positional lists are unchanged.

`union` demonstrates widening and a union result. `union-errors` includes boolean and string possibilities: string-plus-number contributes string while boolean-plus-number emits an underlined error.

## PARKED distributed effects

General native effects require a separate replay contract:

```yl
match (typeof(left)) {
    _ => audit()
}
```

The engine reports `semantic.distributed_effect` before running that effect. It does not assume an arbitrary library operation can be repeated safely.

Rereading a query inside a case requires a combination-local view contract:

```yl
match (typeof(left)) {
    _ => annotateWithType(typeof(left))
}
```

This reports `semantic.distributed_argument`; the engine does not silently choose between the whole union and the selected member. Literal result contributions used by the checked-in addition rule are supported. Matches inside wrapper definitions and constant-only query bodies remain unsupported as before.

## Result contract and remaining scope

Qualified identities and structural alternatives are preserved in `std/types#typeValues` records. Display strings are presentation conveniences. Restrictions retain every fact and its provenance, so downstream checks can produce additional diagnostics for an already-invalid possibility.

The previous native `ensure` interface is replaced; experimental artifacts using it need recompilation. Bounded Cartesian products and fact sets prevent unbounded analysis, and recursive contributions requiring infinite types are diagnosed.

Scoped control-flow narrowing, subtyping, ownership and target execution remain separate work. `useAsType` is not implemented. The experimental APIs remain subject to review.
