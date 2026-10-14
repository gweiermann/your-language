# Declarative attachment queries

Declarative semantics describe contributions to an analysis-local attachment store. Queries compose those contributions through identities, nested tuple keys, callbacks, and conditional operations. The engine discovers producers from the compiled language and committed syntax occurrences; language authors do not provide producer lists or dependency metadata.

## Identities and keys

`self` is the current node or pattern occurrence only inside its `semantics` block. Operations have no implicit caller occurrence: pass it as an argument. An operation may explicitly name a formal parameter `self`; that parameter is an ordinary lexical binding. Unbound `self` in an operation, including defaults and callbacks, is a definition error. Captures carry their original syntax identities and source information. Repeated captures are lists, optional captures can be `absent`, and patterns retain identities without adding AST wrappers.

```yl
import { contribute, get } from "core/query"

symbol type
symbol number

node Number = value: /[0-9]+/
{
    semantics {
        contribute((self, type), number)
    }
}
```

Keys can be identities, symbols (including qualified enum variants), text, booleans, or nested tuples containing those values. Text and symbols remain distinct even when their spellings match. A tuple containing `absent` cannot identify an attachment, so its lookup returns `absent`. A pending key component keeps the lookup pending.

## Explicit targets and absent query paths

```yl
operation annotateNumber(target) {
    contribute((target, type), number)
}

node Number = /[0-9]+/ {
    semantics { annotateNumber(self) }
}
```

Operations evaluate their bodies even when an argument is `absent`. Only the expressions and contributions that depend on absence end without a result. Other actions in that operation continue. Optional arguments therefore do not need presence guards just to suppress an annotation.

Ordinary query primitives propagate top-level `absent`. A pending input stays pending and retains its dependencies; it never becomes absence merely because evaluation has not finished. A key containing absence at any nested tuple position suppresses its lookup or contribution. Ordinary tuples and lists retain optional absent slots as data.

Absence can still be inspected deliberately: `isPresent(absent)` is false, `isList(absent)` is false, `equal` compares absence explicitly, and `values(absent)` returns an empty list. `choose` evaluates only its selected branch, but an absent condition returns `absent`; negating absence also returns absence. An absent `when` condition activates neither branch.

For callbacks, `find` continues after an absent result, `map` preserves absent positions, `filter` excludes an absent predicate result, and `every` returns `absent` if a required predicate is absent. These rules preserve positional parameter and annotation lists.

## Immutable local definitions and operation results

Use `:=` to give a name to a dependency-tracked expression. The name is immutable; its value can remain pending until its dependencies settle. Textual order does not impose execution order, and sibling definitions can refer to one another. Duplicate definitions and unresolved definition cycles are errors.

```yl
import { connectReference } from "std/references"
import { annotateWithType, typeOf } from "std/types"

node Reference = name: Name {
    semantics {
        ref := connectReference(self, name)
        inferredType := typeOf(ref)
        annotateWithType(self, inferredType)
    }
}
```

An operation can combine tracked actions with a result. A single top-level `return` defines that result; it is not an imperative early exit. The operation's actions remain active even when the returned value is unused. Local definitions inside a guarded branch are scoped to that branch.

```yl
import { contribute } from "core/query"

operation attachAndRead(target, value) {
    contribute((target, metadata), value)
    return value
}
```

`when` enables contributions while its condition is true. It waits for pending inputs; false disables the branch. Use the same keyword in syntax-local `checks` blocks, whose conditions inspect parsed captures rather than semantic queries.

## Core primitives and authored libraries

`core/` modules expose engine capabilities that cannot be implemented as ordinary YL operations. `std/` modules are reusable authored YL libraries. Import diagnostic operations explicitly:

```yl
import { error, warn, hint } from "core/diagnostics"
```

They emit errors, warnings, and informational hints respectively. There are no implicit diagnostic names. Import aliases and namespace imports remain general module capabilities; documentation uses descriptive direct exports consistently.

## Contributions and lookup

| Operation | Behavior |
| --- | --- |
| `contribute(key, value)` | Register one tracked contribution. An absent key or value contributes nothing. |
| `contributeAll(key, values)` | Register each distinct element of a list from one producer. |
| `get(key)` | Wait for relevant producers, then return the unique value or `absent`. Multiple contributions produce a conflict diagnostic. |
| `getEarlier(key)` | Read the unique contribution scheduled before the current action, applying the same source order, named groups, precedence and boundaries as host scheduling. |
| `getAll(key)` | Wait for relevant producers, then return all contributed values in deterministic producer order. |
| `facts(key)` | Read the currently available distinct values during fixed-point propagation. This is an approximation, not a final lookup. |

Reevaluation updates the same producer's contribution rather than appending another copy. Producers include contributions inside guarded branches that have not yet settled. A known key is not sufficient to conclude that a missing value is absent.

## Ancestry and expression callbacks

`ancestors(value)` returns enclosing syntax occurrences, nearest first, excluding the supplied occurrence. Terminal captures start with their containing occurrence.
`children(value)` returns the directly nested committed syntax occurrences. An abstract expression family can explicitly inherit a member's type by querying its first child; this is authored language behavior.

`find(values, item => expression)` returns the first present callback result. An absent result continues the search; a pending result waits rather than skipping to an outer answer. Exhausting the list returns `absent`. This is a first-result query, rather than a boolean predicate search.

```yl
find(ancestors(self), ancestor =>
    get((get((ancestor, scope)), declarations, text(name)))
)
```

The inner lookup reads a scope identity. The outer lookup uses that identity, a metadata-channel symbol, and the fixed name text. The callback has a lexical parameter, can read surrounding captures, and can compose other callbacks. It cannot perform contribution actions.

## Conditions and diagnostics

```yl
import { error } from "core/diagnostics"

operation requireName(name) {
    when !isPresent(resolve(name)) {
        error("Unknown name '${name}'", name)
    }
}
```

`when` requires a boolean condition and supports an optional `else`. A pending condition keeps its possible contributions pending. A settled false condition rules its branch out. `match` selects the first matching case for each Cartesian combination of explicit alternative values and requires a final wildcard case. Branches may contribute values or emit diagnostics. Rereading a matched expression inside its branch yields that combination's selected member, including inside nested matches; unmatched alternatives do not become evidence through a branch reread.

The optional second diagnostic argument selects source syntax to underline; a third argument selects a related source location. Diagnostic messages can interpolate semantic expressions; pending interpolation values delay emission. Diagnostics belong to the evaluating occurrence and reflect its settled branch, so intermediate diagnostics do not accumulate across propagation rounds.

## Data operations

The standard query primitives operate on ordinary immutable values:

- `text(value)`, `isPresent(value)`, and `equal(left, right)`.
- `first(values)`, `last(values)`, `tail(values)`, `isList(value)`, `hasMultiple(values)`, `contains(values, value)`, and `unique(values)`.
- `isSubset(left, right)` compares list membership.
- `map(values, item => expression)` preserves positions; `filter(values, item => predicate)` retains matching values; `every(values, item => predicate)` evaluates boolean predicates.
- `keys(fields)` and `lookup(fields, key)` work on lists of two-element key/value tuples. Duplicate matching keys are diagnosed by `lookup`.
- `concat(values)` combines a list of strings; `flatten(values)` combines one list nesting level.
- `choose(condition, thenValue, elseValue)` evaluates only the selected expression.
- `alternatives(values)` represents distinct possible values for distributed matches. `values(value)` exposes these candidates, wraps an ordinary compound value as one candidate, and maps `absent` to an empty list.
- `zip(left, right)` pairs list positions up to the shorter list's length. Languages requiring equal arity should check that policy before zipping.

These functions do not establish target-language type rules. A struct description can be an ordinary value such as `[("x", number), ("y", number)]`; an authored operation decides whether exact equality, required-field membership, or field-type compatibility is appropriate.

## Hypothetical evaluation

```yl
hypothetical(
    (functionIdentity, returnType),
    [((functionIdentity, parameterTypes), argumentTypes)]
)
```

Each assumption is a `(key, value)` tuple. Only those explicitly supplied read keys are overridden. The engine recalculates the requested answer's dependent contributions in an isolated evaluation, retaining captured outer identities. It does not copy or traverse an AST as a substitute for type relationships.

Original contributions and diagnostics remain unchanged. Pure YL operations compose safely in this calculation. Native lifecycle operations cannot appear inside declarative query expressions or branches; arbitrary external actions cannot be rolled back by discarding attachment state.

## Producer discovery and cycles

Compilation expands authored operations into normalized actions. Analysis discovers every producer in the committed occurrence forest, including guarded branches, before lookup can report final absence. Internal structural key-footprint analysis narrows candidate producers by known tuple components and propagates possible scope identities through composed lookups. Unknown components remain conservative.

`get` and `getAll` await final results. A cycle that depends on final presence or absence produces a dependency diagnostic. Positive accumulation can use `facts` with `contributeAll` and reach a fixed point. `facts` must not be mistaken for a completed lookup; negative or oscillating equations are rejected when propagation fails to converge. Hypothetical evaluation has the same convergence requirement.

The implementation favors correctness over optimization: reads inspect candidate producers, propagation reevaluates producers, and hypothetical calculations clone analysis-local producer state. Settled hypothetical results are memoized within one analysis by the requested key and complete explicit assumption overlay. Recorded dependencies are available in `SemanticGraph.query_dependencies`; this does not provide incremental parsing or multithreaded execution.

## Rust and compiled artifacts

Use `SemanticEngine::compile`, optional `.ylc` serialization/reload, and `SemanticEngine::analyze`. Results include syntax occurrences, structured diagnostics, and `SemanticGraph.query_entries`, with normalized keys, values, producer IDs, and original source spans.

Declarative artifacts use format version `5`, preventing older runtimes from silently discarding query definitions. Syntax-only artifacts remain version `1`, and native lifecycle artifacts remain version `2`. Reload validates callback bindings, captures, primitive arities, branch structure, owner references, and resource limits.

The CLI's `yl language <language.ylc> check <source> [--json]` evaluates declarative semantics and supports both source-underlined diagnostics and machine-readable diagnostics. `parse_named` remains the syntax-only Rust API; use `SemanticEngine` for semantics.

See the [authored scopes tutorial](/tutorials/declarative-scopes) for a complete language.

## Authored function relationships

The `examples/declarative-functions` language keeps type symbols in its own `types.yl` module and type rules in `typing.yl`. It contains no native reference or type adapter.

```javascript
identity(42);
identity("hello");

function identity(value) {
    return value;
}
```

The two calls have types `number` and `string`. The original function's unresolved parameter and return remain `unknown`. Parameter types are attached to the resolved function declaration's public signature and addressed by parameter name. A function body stores its return information at `(self, returnType)` rather than acquiring a block type.

```shell
cargo run -- compile examples/declarative-functions/definition/language.yl -o target/query-functions.ylc
cargo run -- language target/query-functions.ylc check examples/declarative-functions/programs/identity.txt
cargo test --test declarative_functions
```

The integration tests verify separate parameter positions, nested hypothetical calls, unchanged original signatures, and authored arity diagnostics. This example illustrates the generic query mechanism; the Mini TypeScript example now uses the same authored canonical libraries. Neither example claims complete TypeScript or unrestricted generic inference.

`hypotheticalAll` returns all final contributions without forwarding diagnostics. `hypotheticalChecked` also forwards demanded authored diagnostic contributions. Assumptions can optionally carry a third source-origin value for precise caller underlines. Ordinary `hypothetical` and `hypotheticalAll` leave speculative diagnostics isolated. All modes preserve original contributions and use analysis-local memoization.

Compound query values are bounded to 16,384 retained nodes, 128 nesting levels and one MiB of owned text per collection. Builders enforce limits before another propagation round can clone growing values. Cyclic compound construction reports a structured resource or nonconvergence diagnostic. Match distribution is limited to 4,096 combinations.
