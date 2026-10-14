# Native semantic operations

The experimental Rust semantic interface runs registered native operations over successfully parsed syntax. Language definitions describe operation calls and their ordering; the operation implementations supply the analysis algorithms.

```rust
use std::collections::BTreeMap;
use your_language::semantics::SemanticEngine;

let engine = SemanticEngine::default();
let sources = BTreeMap::from([(
    "language.yl".into(),
    include_str!("language.yl").into(),
)]);
let result = engine.compile_and_analyze(
    "language.yl", &sources, "program.txt", "let name = 1; let result = name;",
)?;
```

Compilation and analysis require no artifact file. `engine.compile(...)` can also return a compiled language for repeated analysis. Its `to_bytes()` method and `engine.load(...)` provide optional serialization. Loading checks that the engine supplies the exact native interfaces recorded in the artifact. Syntax-only artifacts retain format version 1; artifacts with meanings use version 2.

## Declaring meanings

Nodes and patterns accept a grammar-following body:

```yl
import { declare, use, lexicalScope } from "std/references"

node Declaration = "let" name: Name ";" {
    meanings {
        group Register { declare(name) }
    }
}

node Reference = name: Name ";" {
    meanings { use(name) }
}

pattern FunctionContents = "{" (Declaration | Reference)* "}" {
    meanings { lexicalScope() }
}
```

An operation argument can be a local capture, string, boolean, or identity symbol, according to the registered interface. A language may declare `symbol name` and expose it with `export symbol name`; imports and aliases preserve its module-qualified identity. Native modules can also export symbols. A symbol is an identity value, not a grammar pattern or a target-language type chosen by the core. Positional and named arguments use existing YL argument syntax. Captures preserve their values, matched text, and UTF-8 source spans. A library does not need to depend on a target language's `Name` node type.

Patterns retain semantic occurrences without adding AST wrappers. Operations run only for committed original matches. Failed alternatives, lookaround/trivia probes, and pipe wrappers do not introduce additional semantic actions. Projection preserves the original input's occurrences instead of creating actions from projected copies.

## Ordering groups

```yl
meanings {
    lexicalScope()
    precedence {
        Declaration::meanings::Register > Reference
    }
}
```

A bare node or pattern selects all its own named and ungrouped meanings. `Node::meanings::Group` selects a named group. Each clause searches the actual retained occurrence tree throughout its owner's region, at every wrapper depth. Enclosing and local chains contribute dependencies, and chains remain transitive when an optional middle construct is absent.

## Explicit boundaries

```yl
node Program = (FunctionDeclaration | LetDeclaration)* {
    meanings {
        boundary
        lexicalScope()
        precedence {
            FunctionDeclaration > LetDeclaration > FunctionBody
        }
    }
}

node FunctionBody = "{" LetDeclaration* "return" Expression ";" "}" {
    meanings {
        boundary
    }
}
```

`boundary` is a standalone structural marker on the containing node or pattern. It is compiled before native hooks execute. An enclosing search can select the nested boundary occurrence itself, but does not search below it. An owner's boundary does not hide its own contents: its local clauses search the interior and stop at the next nested boundaries.

Thus Program can find named and lambda bodies at different depths without collecting their local declarations as outer peers. A selected body is deferred as a whole; once its outer prerequisites finish, local clauses order its declarations and nested functions. Local ordering cannot release boundary contents before its ancestor entry requirements are satisfied.

The marker is independent of `lexicalScope()`: a boundary does not create a scope, and a scope operation does not automatically create a boundary. Extracting a scoped pattern around a body can establish context while keeping context setup separate from the body's ordered work. A context provider ordered after its own descendants is contradictory and produces a cycle diagnostic.

Wrappers without `boundary` do not stop the search. Named patterns retain occurrences without adding AST nodes; capture and repetition syntax do not create regions. Explicit patterns can still group reusable syntax roles, but are not needed to compensate for varying wrapper depths.

Ordinary operations follow source-order depth-first completion: initializer references run before the declaration's own registration. Bare selectors defer complete node/pattern traversal; named-group selectors order only the chosen own operations. Within a non-boundary construct, explicit earlier subconstructs can proceed before deferred work. Strict self-dependencies and contradictory chains produce structured cycle diagnostics before any native hook runs.

Ordering describes static analysis, not target-language execution or initialization safety. The search uses committed matches, so failed alternatives, probes, and unreachable grammar alternatives cannot produce semantic effects.

## Registering a library

`OperationRegistry` connects a virtual import module and exported operation name to a typed interface and a factory. `SemanticEngine::new(registry)` uses that registry for compilation and analysis. `SemanticEngine::default()` includes the bundled lexical adapter and identity-keyed type service; `registry_mut()` allows additional registration before compiling.

```rust
use your_language::{
    diagnostic::Diagnostic,
    semantics::{NativeOperation, OperationContext, OperationSignature, SemanticEngine, SemanticValue},
};

struct Audit;
impl NativeOperation for Audit {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        context.attach("example/audit#checked", SemanticValue::Bool(true));
        Ok(())
    }
}

let mut engine = SemanticEngine::default();
engine.registry_mut().register(
    "example/audit", "audit",
    OperationSignature {
        id: "example/audit#audit".into(),
        parameters: vec![],
        provides_context: false,
        finalization_only: false,
        contribution: false,
    },
    || Box::new(Audit),
)?;
```

The YL definition imports `audit` from `"example/audit"`. The compiler rejects missing exports, incompatible arguments, missing captures, and unknown meaning groups. Authored YL operations can compose calls to native dependencies through action wrappers and query aliases, as described below.

## Lifecycle and state

Every operation occurrence receives a fresh native instance. `before` runs once when its group executes. `enter` and `leave` balance its own activation; context providers additionally activate while enclosed operations run. `after` runs once when the owner has completed its semantic work.

Context providers clean up after their final leave, under the remaining enclosing contexts. If activation fails, cleanup is still attempted for every started operation under whatever contexts were successfully restored. Hook errors stop further actions; diagnostics emitted through `emit` can accumulate without aborting unrelated checks.

`OperationContext` exposes capture arguments, per-analysis shared values, typed-key record storage, attachments, and diagnostic emission. The bundled library uses context activation to save and restore a current-scope handle. Restoration changes the handle, not the contents of symbol tables. Repeated and concurrent analyses use independent state.

Parsing must produce an AST with no error-severity syntax diagnostics before native operations run. Syntax warnings and help messages may accompany semantic analysis. Syntax errors retain their AST and diagnostics, but produce no semantic effects.

## Lexical library policy

The bundled `std/variables` adapter exports `lexicalScope`, `declare(name, reassignable=true)` and `use(name)`, plus the identity queries described below. Declaration categories are not required: both variables and functions create ordinary named bindings. Function behavior is expressed by language-authored types, slots and meaning order.

Its explicit policy is one namespace per scope, errors for duplicate names in the same scope, and nearest-scope shadowing. `use` performs general name lookup, including bindings whose language-defined type is a function. The `reassignable` attribute records binding metadata; assignment checking and object mutation are outside this slice.

Scope, declaration, and reference attachments point to record IDs. Reference attachments reuse declaration records, preserving symbol identity independently of spelling. Records use separate identity-symbol metadata keys and text identifier keys. Unresolved references point to the name capture; duplicate declarations also retain the previous declaration's source span.

This API does not yet provide a CLI semantic session, host bindings, generated backends, flow analysis, or ownership analysis.

## Running the Rust consumer

The complete example separates language modules in
`examples/semantics/definition/` from source samples in
`examples/semantics/programs/`. `definition/language.yl` composes lexical helpers,
expressions, functions, and variable declarations from their own modules. A shared Rust
source-map loader provides all five modules to the compiler, including the normal
recursive imports between expressions and function bodies. The consumer requires
no `.ylc` file or separate compilation command.

```sh
cargo run --example semantic_analysis
cargo run --example semantic_analysis -- initializer
cargo run --example semantic_analysis -- duplicate
cargo run --example semantic_analysis -- --json
cargo run --example semantic_analysis -- unresolved --json
```

The default output reports success briefly, or renders semantic diagnostics to stderr with the same source underlines as the CLI. `--json` writes the complete analysis result to stdout and suppresses human diagnostics. Error outcomes exit with code 1 in either mode. Color follows stderr's terminal status; `CLICOLOR_FORCE=1` forces color and `NO_COLOR` takes precedence to disable it.

Rust consumers can reuse `diagnostic_render::print_diagnostics`, providing a source lookup function for diagnostic spans, or call `render_diagnostics` with an explicit color setting to obtain a string. Rendering does not change structured diagnostic data or source offsets.

## Type contributions and restrictions

The native `std/types` module exports five operations and one reserved type identity:

| Export | Contract |
| --- | --- |
| `annotate(key, type)` | Add possible types. Repeated contributions form a union. Deferred inputs establish directed dependencies. |
| `restrictTo(key, allowed)` | Require every known possibility to be compatible with the allowed possibilities. Preserve facts, including invalid ones. |
| `typeof(key)` | Return an analysis-local deferred possibility query. |
| `resolveWith(result, assumptions)` | Query dependent types under temporary explicit assignments without changing original facts. |
| `instantiate(template, use)` | Freshen an explicitly host-owned template once per identity pair. Automatic ownership remains experimental and parked. |
| `unknown` | Explicit unresolved possibility, compatible with restrictions without hiding incompatible known members. |

The language declares all other type identities. A key is a generic analysis-local identity or `(identity, authoredSlotSymbol)`. Ordered lists of keys support explicit positional contributions and restrictions. A list value is structural; it is not an alternative set. For example, contributing `[Red, Blue]` and `[Blue, Red]` preserves two correlated alternatives.

A restriction does not remove facts. Contributions of `number` and `string` followed by `restrictTo(key, number)` retain both possibilities and diagnose the string contribution. An explicit `unknown` member remains in the union; a known string still fails the numeric restriction. Unknown permits analysis to continue where information is unavailable; it does not establish that a program is safe.

Queries stay pending while information is unavailable. If a key has no facts but has an allowed contract, `typeof` exposes that contract. Multiple allowed contracts combine by structural intersection, with unknown as a permissive wildcard. Incompatible contracts produce a diagnostic even when no facts were contributed. Remaining unrestricted reads resolve to unknown at settlement. This fallback is not an explicit contribution and can refine when positive facts become available. Contributions and dependencies iterate to a bounded fixed point; recursive growth requiring an infinite type produces a structured diagnostic.

Base facts flow from source to destination without merging their stores. Authored structural slots retain shared value handles through aliases, including late-created slots. Parameter and return relationships remain authored YL; the kernel does not recognize node names, function markers, parameter slot names or primitive type catalogs.

## Syntax identities and binding bridges

`std/syntax` exposes `identity()`, `identityOf(capture)`, `identities(capture)` and `childIdentity()`. These queries select original syntax occurrences independently of the variable library. Scalar queries reject repeated captures; lists preserve their order. Absent captures remain absent, and pipe projections preserve the original occurrence identities.

The lexical adapter separately supplies binding identities. Declarations and resolved references share a binding handle. Source-authored wrappers connect that handle to syntax-local typing:

```yl
import * as types from "std/types"
import * as syntax from "std/syntax"
import * as variables from "std/references"

export operation annotateWithType(value) {
    types::annotate(syntax::identity(), value)
}

export operation typeof(value) = types::typeof(syntax::identityOf(value))

export operation restrictTypeTo(value) {
    types::restrictTo(syntax::identity(), typeof(value))
}

export operation referenceType() {
    types::annotate(syntax::identity(), types::typeof(variables::identity()))
}
```

The independent type library does not inspect lexical scopes, variable-library metadata or declaration names. Native record identities are opaque handles. The Mini TypeScript example keeps its variable/type bridge in `definition/typing.yl`.

## Authored operations and namespaces

Namespace imports expose exported module members through `::`; private source members remain inaccessible. Named imports and aliases retain the same canonical identities. Action wrappers and query aliases expand hygienically before normalization, including named arguments and defaults. Unused wrappers are checked, and recursive expansion is a structured compile error. The runtime executes native contracts, not wrapper declarations.

Wrapper parameters are untyped semantic values. Action bodies contain operation calls. Query aliases must expand to a native query; constant-only returns and matches inside wrapper bodies remain unsupported. Matches are available in node and pattern meanings.

## Matches over type possibilities

A possibility service explicitly returns tagged `SemanticValue::Alternatives`. The generic engine distributes a match over each distinct Cartesian combination of these values. Ordinary lists remain positional, and ordinary scalar matches retain their existing behavior.

```yl
match (typeof(left), typeof(right)) {
    string, string | string, number | number, string => annotateWithType(string),
    number, number => annotateWithType(number),
    unknown, _ | _, unknown => annotateWithType(unknown),
    _ => error("${left} + ${right} is not possible")
}
```

Every possible known operand combination is checked. Supported combinations contribute their result types even when another combination produces an error. Newly propagated possibilities activate newly discovered combinations. A combination executes once; diagnostics are emitted once for each distinct input combination and case/action at the source occurrence.

This first slice supports literal, idempotent type contributions and diagnostics in distributed cases. General arbitrary native effects and rereading an alternative query inside a distributed action are **PARKED**. A structured `semantic.distributed_effect` or `semantic.distributed_argument` diagnostic explains the unsupported case before its actions execute. Combination-local query views require a separate contract; the engine does not silently narrow a binding or replay arbitrary effects.

Native `OperationSignature::contribution` advertises idempotent contribution capability. Contributions cannot provide lifecycle contexts. `AnalysisService::offers_alternatives` identifies managed possibility readers, including pending readers, without hardcoded library names. Collectors can run behind reactive readers to supply late facts; ordinary successor effects wait for the reactive fixed point. Existing `finalization_only` contracts remain post-traversal collectors.

## Diagnostics and serialized results

Conflicting restrictions point to the incompatible value as primary and the precise required type expression as related. For `let count: number = "wrong";`, the primary span is `"wrong"` and the related span is `number`, excluding the colon and whitespace. Initializer origins survive aliases and pipes. Contract-derived binding values use their reference's source extent when diagnosing an incompatible use. Human and JSON diagnostics share the same spans.

`std/types#typeValues` records retain identity/slot keys and qualified type identities. Single possibilities are ordinary symbols or lists; unions use tagged alternatives. Display strings remain separate conveniences. Each parse/analysis has independent state, including after artifact reload and concurrent analysis.

The previous experimental `ensure` contract is replaced by `annotate` and `restrictTo`; affected artifacts must be recompiled. The diagnostic-only requirement flag is obsolete because restrictions have an intrinsic role.

## Inference experiment limits

The [Mini TypeScript example](https://github.com/gweiermann/your-language/tree/main/examples/mini_typescript) demonstrates literals, annotations, variable bridges, recursive functions, aliases, higher-order calls and language-authored addition. Unannotated function calls can resolve retained relationships under independent temporary parameter assignments with `resolveWith`. Mixed calls leave the declaration signature unchanged and infer each call result separately.

Automatic generic ownership, scoped control-flow narrowing, directional subtyping, ownership and target execution are separate work. `useAsType` is not implemented. The operation and serialized result contracts remain experimental.

The MiniTS bridge shares accumulated type facts between a binding and its declaration/reference occurrences using two directed contributions. Syntax occurrence identities remain distinct; initializer evidence flows one way and is never retyped by these bridges.

## Explicit type template experiment

`types::instantiate(template, use)` creates an analysis-local type instance. A repeated pair returns the same identity; a different use identity creates fresh local identities. The template must first be registered by a Rust native library through `types::register_template(context, root, locals)`. This host interface supplies ownership explicitly; it does not infer ownership from node names, lexical library fields or graph reachability.

Every slot of a local identity uses the same substitution. Directed contributions and restrictions are copied as unresolved relationships, rather than as a snapshot of settled types. Reads of identities outside the supplied local set remain captured. Original template facts remain separate from instance facts. Contributions registered later are included on the next propagation pass. Ordinary value aliases retain the original registered template through their directed base-type reads.

Only constraints whose entire destination is local are instantiated. Writes to captured destinations stay on the original template and are not replayed for an instance; a positional destination mixing local and captured keys is parked. A cached instance cannot switch to a different template owner if later registration changes an alias's origin.

```yl
// The host library has registered this template and its owned identities.
types::annotate(
    (types::instantiate(template, use), parameters),
    types::typeof(arguments)
)
types::annotate(
    result,
    types::typeof((types::instantiate(template, use), returnType))
)
```

`parameters` and `returnType` here are language-authored slot symbols, not native built-ins. The example is a wiring sketch with identity values supplied by the host; it is not a complete language definition.

Unregistered templates produce `types.template_ownership` after pending registration has had a chance to complete. Changing a template's ownership is an error. Templates containing owned meaning matches produce `types.template_match`: reusable match relationships are **PARKED**, and selected branches are not silently frozen into a reusable generic rule. Alternative template unions and positional writes combining local and captured identities are also explicitly **PARKED**. Instance identities and copied constraints are bounded by the analysis resource limit.

Mini TypeScript uses the hypothetical query described below instead of explicit template ownership. The Rust host template API remains a separate experiment; its owned meaning matches and mixed captured/local destination restrictions remain parked.

`OperationContext::record_origin(id)` exposes the original syntax occurrence that allocated a record. It provides name-independent provenance for future ownership libraries; it does not establish a freshening policy.


## Contextual syntax and resolved references

`self` is a core contextual value: the original current node or pattern occurrence. It requires no import and continues to refer to the caller occurrence inside authored operation wrappers. Captures can be passed directly as type keys. A repeated capture supplies an ordered key list; an absent optional capture supplies `absent`. Cardinality comes from the grammar, not the shape of a pattern's returned AST value.

```yl
import * as types from "std/types"

operation annotateWithType(value) {
    types::annotate(self, value)
}

operation typeof(value) = types::typeof(value)
```

Dotted capture projections such as `contents.parameters` and `contents.body` follow retained original occurrences, including transparent patterns. They preserve captured source extents and nested repetition groups. Terminal-only literal/regex captures retain their text and spans but have no standalone occurrence identity; wrap them in a node or pattern to address their type. Legacy `std/syntax` identity helpers remain available; the MiniTS bridge uses only `childIdentity()` for transparent abstract-node inheritance.

`std/references` exposes `declare`, `use`, `lexicalScope`, `reference()` and `referenceOf(value)`. A declaration and a use record the binding associated with the exact name capture. `referenceOf(name)` retrieves that association; it never performs another text lookup. Given a captured reference node, it retrieves that node's recorded binding. An unassociated syntax occurrence returns `absent`, not its syntax identity. Scopes and same-scope duplicate rules retain their existing lifecycle implementation in this change. The earlier `std/variables` imports and graph metadata IDs remain compatible aliases; reference queries are the preferred new interface.

## Hypothetical type queries

`types::resolveWith(resultKey, assumptions)` returns a managed query. Each assumption is `(key, temporaryType)`, and `assumptions` is a list. Only the supplied keys replace their incoming contributions in the calculation. Dependent type relationships are recomputed in an isolated overlay. Existing restrictions still apply; captured outer values and the original store remain unchanged.

```yl
operation call(callee, arguments) {
    types::restrictTo(callee, Function)
    types::annotate(self, types::resolveWith(
        (referenceOf(callee), returnType),
        [((referenceOf(callee), parameters), types::typeof(arguments))]
    ))
}
```

`parameters`, `returnType` and `Function` are language-owned symbols. The declaration stores its public signature on its resolved binding, not exclusively on `FunctionContents`. A body stores returned-expression information at `(self, returnType)` without giving the body itself an expression type.

```yl
meanings {
    types::annotate((self, returnType), typeof(result))
}
```

The engine retains pure type-match relationships as well as directed transfers and restrictions. A hypothetical query recomputes their choices under the temporary inputs instead of freezing the original selected case. Nested calls, aliases, captured outer values and explicitly annotated recursion are exercised by integration tests through artifact reload. Synthetic missing-evidence `unknown` defaults remain query defaults, not reusable contributed facts; explicitly authored `unknown` is retained.

Unsupported native effects and warning diagnostics inside a selected hypothetical type case produce `types.hypothetical_effect` with a **PARKED** explanation. They are not replayed or silently discarded. Error cases are retained and report the temporary argument's original span. This is a bounded experimental solver: nesting is limited to 32 hypothetical queries and key/fact combinations to the existing analysis limit. Query errors do not disable unrelated type checks. A general data store, `if`/callback syntax, ancestry queries and authored reference libraries are separate work.

Reproduce with `cargo run --example mini_typescript -- generics`; expect `count: number`, `title: string`, `numberResult: number`, `stringResult: string`, with `identity: (unknown) => unknown` unchanged. `cargo run --example mini_typescript -- errors` prints source underlines and exits 1; append `--json` for the same structured spans.
