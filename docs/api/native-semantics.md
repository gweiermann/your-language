# Rust runtime operations

`SemanticEngine` compiles and analyzes languages through the public Rust API. Reference resolution and type inference are ordinary authored YL libraries; Rust supplies the generic attachment evaluator. Custom Rust operations remain available for host integration.

```rust
use std::collections::BTreeMap;
use your_language::semantics::SemanticEngine;

let engine = SemanticEngine::default();
let sources = BTreeMap::from([("language.yl".into(), definition.into())]);
let language = engine.compile("language.yl", &sources)?;
let analysis = engine.analyze(&language, "program", source);
```

The default engine registers generic `core/query` primitives. The compiler embeds [authored standard libraries](/api/standard-semantics) for `std/references` and `std/types`. A consumer does not register native reference or type adapters, including after artifact reload.

## Ordering groups

```yl
semantics {
    createLexicalScope(self)
    precedence {
        Declaration::semantics::Register > Reference
    }
}
```

A bare node or pattern selects all its own named and ungrouped semantics. `Node::semantics::Group` selects a named group. Each clause searches the actual retained occurrence tree throughout its owner's region, at every wrapper depth. Enclosing and local chains contribute dependencies, and chains remain transitive when an optional middle construct is absent.

## Explicit boundaries

```yl
node Program = (FunctionDeclaration | LetDeclaration)* {
    semantics {
        boundary
        createLexicalScope(self)
        precedence {
            FunctionDeclaration > LetDeclaration > FunctionBody
        }
    }
}

node FunctionBody = "{" LetDeclaration* "return" Expression ";" "}" {
    semantics {
        boundary
    }
}
```

`boundary` is a standalone structural marker on the containing node or pattern. It is compiled before native hooks execute. An enclosing search can select the nested boundary occurrence itself, but does not search below it. An owner's boundary does not hide its own contents: its local clauses search the interior and stop at the next nested boundaries.

Thus Program can find named and lambda bodies at different depths without collecting their local declarations as outer peers. A selected body is deferred as a whole; once its outer prerequisites finish, local clauses order its declarations and nested functions. Local ordering cannot release boundary contents before its ancestor entry requirements are satisfied.

The marker is independent of `createLexicalScope(self)`: a boundary does not create a scope, and a scope operation does not automatically create a boundary. An extracted pattern can attach a scope identity independently of the body's ordered work. Authored lexical scopes are ordinary contributions, not lifecycle context providers. Optional Rust context-provider hooks still follow their registered lifecycle contract.

Wrappers without `boundary` do not stop the search. Named patterns retain occurrences without adding AST nodes; capture and repetition syntax do not create regions. Explicit patterns can still group reusable syntax roles, but are not needed to compensate for varying wrapper depths.

Ordinary operations follow source-order depth-first completion: initializer references run before the declaration's own registration. Bare selectors defer complete node/pattern traversal; named-group selectors order only the chosen own operations. Within a non-boundary construct, explicit earlier subconstructs can proceed before deferred work. Strict self-dependencies and contradictory chains produce structured cycle diagnostics before any native hook runs.

Ordering describes static analysis, not target-language execution or initialization safety. The search uses committed matches, so failed alternatives, probes, and unreachable grammar alternatives cannot produce semantic effects.

## Registering a runtime operation

`OperationRegistry` associates an import module and exported name with a signature and a fresh operation factory. `SemanticEngine::new(registry)` uses that registry; `registry_mut()` adds exports before compilation. Canonical standard-library module names are reserved and cannot be replaced by native registrations.

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

A YL definition imports `audit` from `"example/audit"`. The compiler validates exports, argument kinds, captures and group selectors. Each committed occurrence receives a fresh operation instance. Its `before` and `after` callbacks bracket the entire occurrence; `enter` and `leave` bracket active traversal segments when ordered work is deferred. Context providers restore prior context when leaving an active segment.

Host state is local to one analysis. Failed parser alternatives and trivia probes do not create callbacks. Parallel analyses do not share operation instances or mutable semantic state. Errors are structured diagnostics; host callbacks do not run when query analysis has already failed.

## Query isolation

Authored query operations compose [tracked contributions, callbacks and conditions](/api/declarative-queries). Arbitrary runtime callbacks cannot be placed inside these query expressions or conditional query branches: compilation reports `yl.query_native_effect`. Hypothetical analysis evaluates authored relationships and diagnostics in an isolated store, without replaying arbitrary host effects.

## Migration from native standard adapters

The native lexical stack, type constraint service, template ownership API and `types::instantiate` adapter are removed. Use `self` and direct captures for syntax identities, `referenceOf` for binding identities, and `types::resolveTypeWith` for explicit hypothetical assumptions. `std/references` is an import alias for `std/references`; it loads the same authored declarations rather than a second implementation.

Artifacts containing retired native standard-operation contracts report `yl.artifact_retired_std`. Recompile their original YL definitions. Generic host-operation artifacts remain supported through their registered contracts. See [compiled artifacts](/api/artifacts) for version compatibility.
