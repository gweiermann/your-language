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
import { declare, use, lexicalScope } from "std/variables"

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

## Separate composable inference experiment

The Mini TypeScript consumer demonstrates identity-keyed type constraints. The
native `std/types` module exports `ensure(key, type)`, `typeof(key)` and the
`unknown` symbol. Primitive identities and named slots belong to the language.
Authored operation wrappers connect literals, bindings, function signatures and
calls without target node names in the native library. See the checked-in
[Mini TypeScript example](https://github.com/gweiermann/your-language/tree/main/examples/mini_typescript).

## Operation wrappers and namespaces

```yl
import * as types from "std/types"
import * as variables from "std/variables"

export operation annotateWithType(value) {
    types::ensure(variables::identity(), value)
}

export operation typeof(value) =
    types::typeof(variables::identityOf(value))
```

Namespace imports expose exported module members through `::`; private source
members remain inaccessible. Named imports and aliases retain the same canonical
identities. Imported action wrappers and query aliases expand hygienically before
normalization, including named arguments and defaults. Recursive expansion is a
structured compile error. Unused wrappers are also checked for unresolved names
and recursive expansion. Wrapper parameters are untyped semantic values; their
arguments can use positional names, named arguments, and defaults. Action bodies
currently contain operation calls. Query aliases must expand to a native query;
constant-only query returns and matches inside wrapper bodies are not supported.
The runtime does not interpret wrapper declarations.

Semantic tuple/list values are available in operation arguments. Type keys use
an identity or `(identity, authoredSlotSymbol)`. Deferred handles can be passed
through generic native `ParameterKind::Value` parameters. Matches resolve handles
through `AnalysisService::resolve_value`; pending values defer case selection.

A native operation can declare `finalization_only` for an analysis collector.
Such an operation runs after ordinary lifecycle hooks, while relative collector
ordering is preserved. This cannot be combined with `provides_context`.

`std/variables` supplies identity queries: `identity()` for the current occurrence,
`identityOf(capture)` for one retained capture root, `identities(capture)` for an
ordered list, and `childIdentity()` for explicit first-child inheritance.
Declarations and resolved references return the same binding identity.

The Mini TypeScript wrappers `annotateWithType` and `restrictTypeTo` currently use the same equality kernel. Separate accumulation of possible types and universal compatibility requirements remains future work; the names do not imply union inference in this prototype. `identity()` accepts no arguments. `identityOf(value)` requires an explicit capture or identity and preserves absence rather than selecting the current occurrence.

The declaration API replaces the experimental category-taking `declare`, `declareVariable`, `declareFunction` and `useVariable` helpers. `variable`, `function` and `kindSymbol` are no longer native exports or binding metadata. Recompile artifacts containing the previous native contracts.
