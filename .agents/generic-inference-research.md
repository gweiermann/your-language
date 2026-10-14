# Research: polymorphism with identity-keyed types

Research date: 2026-10-12. Inspected `main` at `31aad8df`. This is a design investigation, not an implemented API or a change to language semantics.

## Recommendation

Keep the existing identity and `(identity, symbol)` addressing model. Add reusable **type schemes** behind those keys: a description of relationships, with some variables eligible to be freshened at each use. Store the description once; instantiate its type variables independently for each call/use. Binding identities remain stable.

Start with inferred rank-1 polymorphism for equality-only function relationships. Represent schemes in a way that can later retain declarative constraints such as the authored addition match. Add language-owned structured type constructors for nested generics. Do not claim complete TypeScript inference or introduce a native `identityFunction`/`callType` special case.

This follows the distinction between a type and a quantified type scheme in Damas–Milner: generalization excludes variables free in the environment, and instantiation substitutes fresh variables consistently. Its principal-type result applies to the paper's applicative language, not automatically to our unions, mutable bindings or arbitrary meaning effects. [Damas and Milner, sections 2, 5–6](https://steshaw.org/hm/milner-damas.pdf)

## What the current implementation actually does

The public kernel exports `std/types::{ensure, typeof, unknown}`. [Kernel](../src/semantics/types.rs), [authored wrappers](../examples/mini_typescript/definition/typing.yl)

* `TypeKey` is a binding/occurrence identity plus an optional authored slot symbol.
* `ensure` records equality constraints. Its `requirement` flag changes diagnostic provenance, **not** the equality relation.
* `typeof` returns a deferred handle; equality can connect its pending variable immediately, while a match waits for a readable value.
* Type values are `Variable`, `Unknown`, `Atom(symbol)` and an ordered `List` of types. The latter is a heterogeneous sequence, used for parameter signatures; it is not a target-language `List<T>` constructor.
* Union-find merges equalities and also merges matching slot projections. This preserves function slots through aliases, but it creates one shared signature for all calls.
* Recursive structural constraints receive an occurs/cycle diagnostic. At quiescence, unbound variables settle as `unknown`; deferred matches then select branches.

The wrappers write parameter types at `(functionIdentity, parameters)` and result types at `(functionIdentity, returnType)`. The current call wrapper connects arguments directly to those original slots. That is where polymorphism is lost.

I ran a temporary harness against the public API for both programs below. Each emitted exactly one `types.mismatch: Type number does not match string`. The harness was removed; no implementation changes are included in this research.

```ts
function identity(value) { return value; }
let a = identity(1);
let b = identity("hello");
```

```ts
function identity(value) { return value; }
function apply(f, value) { return f(value); }
let a = apply(identity, 1);
let b = apply(identity, "hello");
```

The existing successful alias, higher-order, recursive and diagnostic tests are in [mini_typescript_prototype.rs](../tests/mini_typescript_prototype.rs). They test shared signatures, not generalization. Union-producing `annotateWithType` remains a separate, unimplemented design; it must not be presented as an existing alternative to polymorphism.

## Identity, worked through

Desired target program:

```ts
function identity(value) { return value; }
let a = identity(1);       // number
let b = identity("hello"); // string
```

The reusable relationship is:

```text
identity's scheme: forall T. parameters = [T], return = T

first use:  parameters = [T1], return = T1; T1 = number
second use: parameters = [T2], return = T2; T2 = string
```

The declaration identity does not change. Instances have analysis-local type variables/slot environments, addressed through the use-site identity or another stable instance handle. All appearances of `T` within one instance use **one** substitution entry. Creating fresh variables independently for parameter and return would destroy the relationship.

This preserves precision in ways that widening a shared slot cannot:

```text
shared union:     (number | string) -> (number | string)
generic scheme:  forall T. T -> T
```

TypeScript's documented identity example likewise tracks the argument type into the result; its generic array example demonstrates the same variable inside a composite type. [TypeScript generics handbook](https://www.typescriptlang.org/docs/handbook/2/generics.html)

## Options

| Approach | Fit to this repository | Strength | Limitation |
| --- | --- | --- | --- |
| Inferred rank-1 type schemes | Extend the type service with quantification, generalization and instantiation | Smallest principled step for identity, aliases, composition and ordinary higher-order functions | Classic equality-only schemes cannot retain our overloaded match relation by themselves |
| Per-use constraint templates | Capture a type-constraint region; freshen its local variables and replay constraints for each instance | Can retain authored match cases and other deferred relations without baking operator logic into Rust | Ownership, captures, effects, recursion and bounded specialization require careful contracts; naive whole-AST replay is wrong |
| Explicit generic parameters | Author generic variables and their scope, then instantiate the resulting scheme | Predictable and supports cases inference cannot discover | More annotation burden; merely exporting a symbol named `T` does not make it a substitutable type variable |

Recommendation: schemes are the semantic model; reusable constraint templates are their representation when equality alone is insufficient. Explicit generic declarations can later complement inferred schemes. These are compatible layers rather than competing global modes.

A generic scheme carrying predicates has a research precedent in qualified types. That does **not** establish that every arbitrary YL match/action has principal inference; our finite match tables need their own behavior and termination tests. [Jones, A theory of qualified types](https://doi.org/10.1016/0167-6423(94)00005-0)

## Why saving only the parameter/result types is insufficient

```ts
function combine(a, b) { return a + b; }
```

The current authored rule permits number/number → number and three string/number combinations → string. With unconstrained parameters, no branch can run yet. If the declaration is finalized to unknown and only those resulting values are stored, a future instance cannot recover the original rule.

A reusable template must retain the **declarative relationship** between operand and result variables. Instance A can resolve number/number; instance B can resolve string/number. Unsupported boolean combinations get call-site diagnostics. The kernel still has no knowledge of addition.

Only constraint declarations should be reusable. Do not rerun lexical declaration operations, diagnostics already emitted for the declaration, arbitrary native side effects or the entire meaning tree at every call. A type service needs an explicit protocol for collecting reusable constraints, including retained match predicates. The current scheduler executes a match once; a reusable relation is a genuine extension, not a rename of that behavior.

## Nested types and inner slots

Tuple **keys** name storage locations. They do not encode composite type **values**. A nested generic needs something like this internal algebra:

```text
Atom(numberSymbol)
Variable(T)
Apply(ListSymbol, [T])
Apply(MapSymbol, [K, Apply(ListSymbol, [T])])
```

`ListSymbol` and `MapSymbol` belong to the target language. Std supplies the constructor/substitution machinery, not their meaning or a primitive catalog. A generic decomposition operation can constrain a value to `Apply(ListSymbol,[T])`, exposing `T` without spelling a native `listElementType` helper.

For example, the reusable signature of `first` could be `forall T. List<T> -> T`. Given `List<number>`, structural unification identifies `T = number`. A map-values function could have `forall K,T,U. ((T -> U), Map<K,List<T>>) -> Map<K,List<U>>`.

The same substitutions recurse into every argument. Returning the type of a parameter is one relation among many; generic construction/decomposition keeps more complicated relations expressible.

Functions can keep their authored `parameters`/`returnType` slots. But higher-order and nested types need a structural representation that can travel as a value, including a distinction between a monotype and a scheme. Type atoms alone and the current list representation do not provide that.

## Possible meaning-library seam

Names below are **provisional proposed operations**, not working commands or finalized syntax. Existing tuple-key addressing stays visible; authors can wrap it.

```yl
export operation call(callee, arguments) {
    types::ensure(
        (types::instantiate(variables::identityOf(callee), variables::identity()), parameters),
        types::typeof(variables::identities(arguments))
    )
    types::ensure(
        variables::identity(),
        types::typeof(
            (types::instantiate(variables::identityOf(callee), variables::identity()), returnType)
        )
    )
}
```

Here `instantiate(templateIdentity, useIdentity)` returns an instance identity/handle. Repeated requests for that pair must return the **same** instance, so its parameter and return slots remain connected. A different use identity gives a fresh instance. This requires a new analysis-local identity/handle representation: today's `SemanticIdentity` has only `Occurrence` and `Binding`.

The equally important declaration-side operation is to publish a reusable scheme after its constraints are known. A proposed `generalize(...)` needs an explicit eligible region and external environment/capture set. This note deliberately does not invent a complete argument signature: those ownership inputs are the unresolved seam.

No new YL keyword is inherently required for equality-only schemes: a native type-library operation can use the existing lifecycle/service API. Retaining generic match relations needs a generic reusable-expression/constraint protocol with the scheduler. Std must not identify `FunctionContents` or inspect a grammar's node names to infer boundaries.

Instantiation is conceptually **at polymorphic use**, not necessarily only at a call. `let alias = identity` should preserve a reusable scheme; consuming it as a monomorphic argument should instantiate it. Blindly merging the alias's base variable with the declaration's monotype would reintroduce the current problem.

## Stress cases and boundaries

| Case | Required behavior |
| --- | --- |
| Identity at number and string | Independent results; declaration remains a scheme |
| `(T,T) -> T` versus `(T,U) -> T` | Repeated variables share substitution; independent variables do not |
| Two parameters with different annotations | Parameter positions stay distinct; annotations remain fixed |
| `first(List<T>) -> T`, nested `Map<K,List<T>>` | Constructors substitute recursively; mismatched constructor or arity fails |
| Alias of a generic function | Preserve the scheme, instantiate separately at uses |
| `apply(identity, 1)` | Ordinary higher-order inference works without claiming higher-rank support |
| One callback invoked at unrelated types in one body | May need explicitly polymorphic callback signatures; do not silently freshen every parameter reference |
| Function returns a captured outer variable | Captured variable stays shared, not freshly generalized per call |
| Recursive/mutually recursive functions | Solve one dependency component with monomorphic placeholders before generalizing its exports |
| Unproductive recursion | Do not confuse an unconstrained type variable eligible for quantification with a proven generic relationship or with settled unknown |
| Generic addition | Preserve its relation in the scheme/template, not the unknown fallback result |
| Explicit annotation conflict | Primary diagnostic at bad argument/value, related location at declaration restriction |

The closure case is especially important:

```ts
let outer: number = 1;
function read(ignored) { return outer; }
```

Its result must remain number even if the parameter is generic. A blanket graph clone that freshens the outer binding is incorrect. Declaration-local variables are candidates for generalization; environment variables are not.

As a conservative first scope, support rank-1 schemes for eligible immutable function values and ordinary monomorphic recursion. Use explicit annotations for polymorphic recursion/higher-rank cases. Mutation requires a generalization policy: OCaml's value restriction illustrates why values capturing persistent mutable state cannot automatically be reused at unrelated types. Those are future policy choices, not reasons to add assignment semantics to the current prototype. [OCaml manual, polymorphism and its limitations](https://ocaml.org/manual/5.2/polymorphism.html)

## Implementation steps and decisions

1. Add scheme/type-variable ownership and substitution to `std/types`. Preserve stable binding keys, separate template constraints from per-use constraints, and preserve provenance.
2. Demonstrate identity, two independent parameters, aliases, external captures and rank-1 higher-order composition. Test reload through the existing artifact/API flow.
3. Add language-owned `Apply(constructor,args)` values, construction/decomposition, and recursive substitution; test nested generic containers.
4. Retain finite declarative match relations in schemes. Unknown settlement must happen after instance propagation, not erase the template.
5. Extend only after the preceding examples are precise: explicit generics, annotated higher-rank signatures, union/restriction interactions and bounded context-sensitive inference.

The immediate user-facing choice is whether the next prototype should infer reusable rank-1 schemes automatically for function declarations. I recommend yes. The implementation-side prerequisite is an explicit ownership/environment contract; derive it from authored library context, not hidden grammar-name conventions.

Do not merge `ensure` equalities directly between the call and the reusable declaration, generalize every unsolved variable, turn generic variables into `unknown` before publication, or replay arbitrary meaning operations. Those shortcuts pass simple examples while destroying either precision or composability.

## Reproduce the inspected baseline

```powershell
git checkout 31aad8df
cargo test --test mini_typescript_prototype
cargo test --test mini_typescript_prototype inference_is_monomorphic -- --exact
cargo run --example mini_typescript -- higher-order
```

Use a clean dedicated checkout for the historical revision. Expected: all 18 tests pass; the focused test passes by asserting one diagnostic for mixed calls, and the higher-order example succeeds with shared signatures. This does **not** demonstrate polymorphism. The mixed-call examples above were additionally evaluated through `SemanticEngine::compile_and_analyze` using the checked-in definition source map; both currently produce one mismatch.

No formatter, runtime, grammar, public API or official documentation was changed by this research.
