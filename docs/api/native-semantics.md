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
import { declareVariable, useVariable, lexicalScope } from "std/semantics"

node Declaration = "let" name: Name ";" {
    meanings {
        group Register { declareVariable(name) }
    }
}

node Reference = name: Name ";" {
    meanings { useVariable(name) }
}

pattern FunctionContents = "{" (Declaration | Reference)* "}" {
    meanings { lexicalScope() }
}
```

An operation argument can be a local capture, string, boolean, or imported identity symbol, according to the registered interface. Positional and named arguments use existing YL argument syntax. Captures preserve their values, matched text, and UTF-8 source spans. A library does not need to depend on a target language's `Name` node type.

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

`OperationRegistry` connects a virtual import module and exported operation name to a typed interface and a factory. `SemanticEngine::new(registry)` uses that registry for compilation and analysis. `SemanticEngine::default()` includes the bundled lexical adapter; `registry_mut()` allows additional registration before compiling.

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

The YL definition imports `audit` from `"example/audit"`. The compiler rejects missing exports, incompatible arguments, missing captures, and unknown meaning groups. There is no YL operation implementation language in this slice: native implementations are explicit dependencies.

## Lifecycle and state

Every operation occurrence receives a fresh native instance. `before` runs once when its group executes. `enter` and `leave` balance its own activation; context providers additionally activate while enclosed operations run. `after` runs once when the owner has completed its semantic work.

Context providers clean up after their final leave, under the remaining enclosing contexts. If activation fails, cleanup is still attempted for every started operation under whatever contexts were successfully restored. Hook errors stop further actions; diagnostics emitted through `emit` can accumulate without aborting unrelated checks.

`OperationContext` exposes capture arguments, per-analysis shared values, typed-key record storage, attachments, and diagnostic emission. The bundled library uses context activation to save and restore a current-scope handle. Restoration changes the handle, not the contents of symbol tables. Repeated and concurrent analyses use independent state.

Parsing must produce an AST with no error-severity syntax diagnostics before native operations run. Syntax warnings and help messages may accompany semantic analysis. Syntax errors retain their AST and diagnostics, but produce no semantic effects.

## Lexical library policy

The bundled `std/semantics` adapter exports `lexicalScope`, generic `declare`, `declareVariable`, `declareFunction`, `use`, and `useVariable`. `declare` accepts an imported category symbol, a name capture, and `reassignable` (default `true`). The `variable` and `function` category symbols classify declarations; they do not create separate namespaces or target-language types.

Its explicit policy is one namespace per scope, errors for duplicate names in the same scope, and nearest-scope shadowing. `useVariable` is a convenience alias for general name lookup and can resolve function-category bindings too. The `reassignable` attribute records binding metadata; assignment checking and object mutation are outside this slice.

Scope, declaration, and reference attachments point to record IDs. Reference attachments reuse declaration records, preserving symbol identity independently of spelling. Records use separate identity-symbol metadata keys and text identifier keys. Unresolved references point to the name capture; duplicate declarations also retain the previous declaration's source span.

This API does not yet provide a CLI semantic session, host bindings, generated backends, type analysis, flow analysis, or ownership analysis.

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
