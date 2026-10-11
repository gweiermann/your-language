# Composable Mini TypeScript inference prototype (throwaway)

Question: can local native type meanings, deferred semantic expressions and tuple matches compose into useful inference without a grammar-specific root traversal or native arithmetic rules?

This slice answers yes for a small monomorphic language. It remains an experiment, not a TypeScript implementation or production type library. The type library has no target node names and never scans the Program tree. Each grammar construct publishes its own constraints, and an analysis-local service propagates them automatically. There is no `solve()` meaning.

```sh
cargo run --example mini_typescript
cargo run --example mini_typescript -- addition
cargo run --example mini_typescript -- addition-errors
cargo run --example mini_typescript -- addition --json
cargo test --test mini_typescript_composable
```

Definitions and programs remain in separate directories. Compilation is directly in memory; tests additionally serialize/reload the optional `.ylc` representation. Default output prints inferred binding types and human underlined diagnostics. `--json` returns the complete AST, retained occurrences, lexical/type records and structured diagnostics. Error outcomes exit 1.

## Local meanings and library interface

Literal and annotation nodes import identity symbols and publish their types locally:

```yl
import { type, number, string } from "prototype/types"

node Number = value: /[0-9]+/ {
    meanings { type(number) }
}

node String = value: /"[^"\n]*"/ {
    meanings { type(string) }
}
```

`aliasType(value)` connects an owner's type to its captured value. `inheritType()` explicitly connects a transparent abstract family's type to its selected retained member. `declarationType()`, `bindingType(value)` and `referenceType()` consume lexical identity attachments; `checkType(annotation)`, `functionType(parameters, body, annotation)` and `callType(callee, arguments)` publish the remaining equalities. Each operation is imported normally through a registered native YL module. The library knows the operation contracts, not grammar role names.

Addition policy lives entirely in the language definition:

```yl
node Addition = left: Expression "+" right: Expression {
    meanings {
        match (typeof(left), typeof(right)) {
            string, string | string, number | number, string => type(string),
            number, number => type(number),
            unknown, _ | _, unknown => type(unknown),
            _ => error("${left} + ${right} is not possible")
        }
    }
}
```

A test changes only the `number, number` arm to `type(string)` and observes a string result. No Rust addition special case needs changing. `typeof(value)` is a native semantic value reader; its capture points to original occurrence identities instead of searching for equal source spans.

**Experimental syntax contract:** semantic calls may nest in arguments; tuple-match cases use imported identity symbols, text or boolean constants, `_` wildcards, and `|` alternatives. The first matching case runs. A final whole wildcard is required, even if the current cases appear exhaustive. `=> {}` is an ordinary void branch and publishes no result type. Use `type(unknown)` explicitly to publish a settled unknown result. A branch may contain native calls and `error(...)`/`warning(...)` diagnostics with `${capture}` interpolation. Error stops the remaining actions in that branch; warnings continue. These additions are experimental semantics syntax, not changes to grammar matching or the approved `boundary` marker.

## Propagation and unknowns

- Literal types widen to `number`, `string` and `boolean`; there are no singleton literal types.
- Declarations, parameters and references share type variables through the bundled lexical declaration records. Shadowed spellings remain distinct identities.
- Each local operation publishes equalities over occurrence and declaration handles. Service progress consumes new equations once. Calls, returns and annotations can anchor variables that earlier references did not know.
- `Pending` and settled `unknown` are different states. A tuple match waits until **all** its inputs are ready; it does not choose an unknown or fallback arm while a reader is pending.
- The engine alternates service progress and deferred actions to a fixed point. Only when neither progresses does it call each service's `settle` once. The type service settles remaining unconstrained classes to `unknown`, then the engine resumes propagation and deferred actions. There are no promises waiting on recursive cycles.
- Selected cases and action positions are retained. Each native branch effect, warning or error runs once, regardless of propagation rounds. Deferred native effects receive balanced own lifecycle hooks and cleanup attempts, while semantic value readers use the separate `evaluate` contract.
- An uncalled `function add(value) { return value + 1; }` settles to `(unknown) => unknown`. Calling it with a number before settlement yields `(number) => number`. Mutually recursive results without an anchor remain unknown; explicit return annotations can anchor the cycle. Operand types are not inferred backwards from a tuple-match result.
- Functions have fixed arity and are monomorphic across all call sites. An identity function called with both numbers and strings diagnoses a conflict. An occurs check rejects infinite types, such as a parameter called with itself.
- The prototype's settled unknown is a permissive best-effort marker: equating it with a concrete type contributes no information and reports no mismatch. This is not TypeScript's directional assignability policy for `unknown`. Explicit unknown can therefore coexist with a known annotation, and it does not overwrite already known information.
- Equations are processed in publication order. The first compatible equations determine a type; conflicting later equations diagnose without replacing it. Successful components of a failed function equation remain. Inference output after errors may be partially constrained or disagree with annotations. Production inference would need error types, annotation-priority rules, transactional unification and richer related diagnostics.

Original-source spans are carried by constraints and interpolated diagnostic operands. Lexical duplicate/unresolved diagnostics retain their existing origins and secondary locations. Syntax errors skip all native effects and services.

## Generic seams and lifecycle limits

`SourceCapture::occurrences` records the original retained occurrence roots selected by a capture, in order. Terminals and absent optional values have no handles. Imported aliases, transparent patterns, repetition and pipes preserve original handles; tests cover their identities and spans through public analysis and reload.

`OperationContext::attachment(occurrence, key)` lets independent native libraries read identity links. `register_service(id, factory)` creates fresh per-analysis service state. `with_service` provides library-local state alongside normal graph access. `AnalysisService::{progress, settle, finish}` defines automatic completion: progress until stable, settle once, progress until stable, finish once. A service that returns an error stops receiving progress/settle polls and still gets one finish attempt; its error is not repeated every round. Independent audit-service tests exercise this interface without the type library.

Deferred matches and nested-value calls run during finalization, after ordinary meaning lifecycle hooks. They consume persistent records and analysis services; they do not reactivate enclosing lexical contexts. Context-providing operations are rejected in deferred expressions/actions. Deferred jobs preserve their explicit ordering; phase-incompatible precedence is diagnosed instead of silently executing effects in the opposite order. Full interleaving of pending effects with active lexical contexts is outside this experiment.

Native value readers must be observational and may be retried on fresh instances. They must not emit diagnostics or external effects; persistent query state belongs to an analysis service. Match blocks currently appear directly inside `meanings`; named `group` blocks support native calls, but not nested matches yet.

Types are stored as `prototype/types#inferredType` metadata on lexical declaration records. The root `prototype/types#expressionTypes` attachment points to a report keyed by occurrence IDs, which link to original source spans in structured output. Native factories, services, deferred jobs and variables are fresh per analysis; tests verify repeatability, concurrent isolation and optional artifact reload.

## Gaps and possible next experiments

No assignments, object/array types, generic polymorphism, overloads, unions, subtyping, narrowing, guards, ranges, control-flow analysis, implicit/multiple returns, function expressions, nested function declarations, target-language modules, ownership or code generation. Only addition is demonstrated; accepted operand combinations are exactly the local match above. The grammar has no comments or escapes, and keyword boundary behavior is limited. Static lexical visibility does not assess execution-time initialization safety.

Possible follow-ups, **not approved or implemented syntax/behavior**:

1. Add directional annotation/assignability constraints when unions or subtyping are needed.
2. Infer operand information backwards through declared match relations rather than equality alone.
3. Replace permissive unknown/error recovery with explicit unresolved/error records and related source locations.
4. Explore active-context deferred effects only if a real library requires them, preserving lifecycle and ordering contracts.

Keep this prototype separate while assessing these policies. Its result is evidence that local imported meanings and generic native service propagation can compose; it is not a claim of full TypeScript compatibility or optimal inference.
