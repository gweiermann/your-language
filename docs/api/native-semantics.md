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

A bare node or pattern selects all its own named and ungrouped meanings. `Node::meanings::Group` selects a named group. Selection searches occurrences under the declaration that owns the precedence block; enclosing and local chains both contribute dependencies. A chain remains transitive when an optional middle construct is absent.

Bare selectors defer the whole matched node or pattern, including nested meanings. Named-group selectors order only the selected own operations. Explicit earlier descendants are exceptions: their own prerequisites and traversal can proceed before the deferred construct. Strict self-dependencies and contradictory accumulated chains produce structured cycle diagnostics before any native hook runs.

Ordinary operations follow source-order depth-first completion: an initializer's nested references run before the declaration's own registration. Context-provider setup precedes semantic descendants. A provider ordered after its descendants creates a cycle; extract a scoped pattern around the ordered construct to keep setup distinct.

Selectors distinguish roles through normal YL composition. For example:

```yl
pattern TopLevelDeclaration = LetDeclaration
node Program = (FunctionDeclaration | TopLevelDeclaration)* {
    meanings {
        lexicalScope()
        precedence { FunctionDeclaration > TopLevelDeclaration > FunctionBody }
    }
}
```

Only program-level declarations use `TopLevelDeclaration`. Function-body locals remain `LetDeclaration`, so their initializers stay within deferred bodies. A broad `LetDeclaration > FunctionBody` chain would explicitly pull body-local declarations and their initializer traversal earlier as well. Ordering controls static analysis, not target-language execution or initialization safety.

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
