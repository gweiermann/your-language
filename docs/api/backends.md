# Backend dispatch API

`your_language::backend` is shared by Rust consumers, example hosts, and the CLI's hook inspection. No generated Rust code, dynamic library loader, or transport protocol is required.

## Load, analyze, and dispatch

```rust
use your_language::load_compiled_language;

fn run<H: your_language::backend::Backend>(
    bytes: &[u8],
    host: &mut H,
) -> Result<H::Value, String> {
    let language = load_compiled_language(bytes).map_err(|e| format!("{e:?}"))?;
    let program = language.prepare_backend("example.txt", "result 42;")
        .map_err(|e| format!("{e:?}"))?;
    program.run(host).map_err(|e| format!("{e:?}"))
}
```

`prepare_backend` parses and performs static analysis. It returns diagnostics if either fails and never invokes backend hooks. The resulting immutable `BackendProgram` retains source, syntax occurrence handles, and the complete `AnalysisResult`. It borrows its checked `CompiledLanguage`.

## Implement `Backend`

```rust
pub trait Backend: Sized {
    type Value;
    fn bindings(&self) -> Vec<HookBinding>;
    fn invoke(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        arguments: &[Capture],
    ) -> Result<Self::Value, Diagnostic>;
}
```

`Value` can be an interpreter value, generated AST, expression string, instruction list, or a consumer-defined handle. The engine does not inspect, annotate, clone, or cache that value.

`HookBinding::new("hooks.yl#add", &["left", "right"])` binds an exact module-qualified signature. A host should declare the signatures it implements, rather than copying arbitrary requirements out of an untrusted artifact. `CompiledLanguage::required_hooks()` returns only hooks used by entry-reachable backend mappings, not every declaration in imported modules. A host can implement just this subset or provide additional bindings for a broader preset. The complete required manifest is checked before the first invocation, including hooks in branches absent from the current source or skipped by an interpreter. Hook bodies can return a structured `Diagnostic`; missing source locations default to the dispatched occurrence. Existing grouped annotations are preserved.

`Backend` uses a binding list and one dispatch method; it is not a generated Rust trait with one method per hook. Only required bindings need implementations in that dispatch method. See [reachability and composition](/reference/backends#reachability-and-composition) for the grammar-level contract.

## Syntax and captures

`SyntaxRef` is an opaque, copyable handle scoped to one prepared program. It cannot be constructed from an arbitrary integer. Passing a handle from another program produces `yl.backend_handle`.

`Capture` is `Syntax(SyntaxRef)`, `List(Vec<Capture>)`, `Tuple(Vec<Capture>)`, or `Absent`. `syntax()` and `list()` validate expected argument shapes. A terminal handle supports source inspection but cannot be dispatched without a node or pattern mapping.

`BackendProgram` exposes:

- `root()` for the entry occurrence.
- `text(handle)`, `span(handle)`, and `definition(handle)` for inspection.
- `capture(handle, name)` for unevaluated capture/projection access.
- `semantic_identity(handle)` to address static attachment/query facts for that syntax.
- `analysis()` for the AST, occurrence tree, diagnostics, and semantic graph/attachments.
- `error(code, message, handle)` for a source-linked error.
- `run(host)` and `run_with_options(host, options)` for validated dispatch sessions.

Inside a hook, `context.dispatch(self, child)` explicitly invokes a child. `context.program()` provides read-only inspection. The same child can be dispatched repeatedly, and no other child is visited automatically except the selected member of an abstract node without a mapping.

## Budgets and cancellation

`DispatchOptions` defaults to one million dispatch calls and 128 nested dispatches. A smaller caller limit is respected; the engine also enforces a hard nesting ceiling of 128 to protect its Rust stack. `cancelled` accepts an optional shared `Arc<AtomicBool>`. Budget and cancellation failures are structured source diagnostics.

These limits count dispatches, not arbitrary host instructions. A hook performing its own long-running computation or loop must honor its application's cancellation and resource policy. Hooks are trusted application code; the engine does not sandbox Rust host implementations.

See `examples/14-backend-hooks/hosts/mod.rs` for complete interpreter, JavaScript, and WebAssembly implementations using this interface.
