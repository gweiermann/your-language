# Native analysis extensions

`SemanticEngine` compiles and analyzes languages through the public Rust API. Reference resolution and type inference are ordinary authored YL libraries; Rust supplies the generic attachment evaluator. Custom Rust operations remain available for advanced static-analysis integration. They are separate from [backend hooks](/api/backends), which interpret or compile target programs.

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

Static analysis ordering is a language contract, shared by authored and native
operations. See [ordering groups](/reference/meaning-libraries#ordering-groups).

## Explicit boundaries

See [semantic boundaries](/reference/meaning-libraries#explicit-boundaries) for
region selection and whole-occurrence deferral. Runtime branch and loop execution
remain the backend host's responsibility.

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
