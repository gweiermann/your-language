# Backend hooks

Hooks connect successful syntax occurrences to a consumer. The consumer can interpret the program, produce another language, or emit machine instructions. Hook dispatch does not evaluate children, create variable scopes, or choose a runtime value representation.

## Contracts

```yl
export hook add(left, right)
```

A hook is declared at module level with distinct parameter names. Parameters are positional; types and defaults are not supported. Hooks use ordinary imports, exports, aliases, and namespace imports. Their compiled identities are module-qualified, such as `hooks.yl#add`; two modules may export hooks with the same short name.

A host supplies bindings for the complete compiled contract before execution starts. Missing bindings, duplicate host bindings, or different parameter names/order are errors before any hook runs. Additional host bindings are allowed. Compiling, loading, parsing, checking, and editor analysis require no host bindings.

## Mapping syntax

```yl
import { add } from "./hooks"

node Addition =
    left: Expression
    "+"
    right: Expression
{
    backend {
        add(left, right)
    }
}
```

A `backend` block belongs to a node or pattern and contains exactly one hook invocation. There is no `return` keyword. The host hook's result becomes the result of dispatching that occurrence; it is never a semantic query value.

Arguments are unevaluated captures, `self`, or tuple/list groups of those arguments. Capture projections such as `contents.body` are allowed. Query calls, literals, computations, named arguments, and imperative actions are not backend arguments. Capture names are validated when compiling; a projection unavailable in a particular occurrence produces a source diagnostic when dispatched. Optional captures retain absence and repeated captures retain list shape. Terminal captures have source handles and exact spans but no independent hook mapping.

Patterns retain occurrence identities despite being absent from the AST. Pipes retain the original grammar captures and their source origins. A backend attached to a pattern runs only when that pattern is explicitly dispatched; it is not an automatic traversal callback.

An abstract node without a backend transparently dispatches its one selected concrete child. Concrete wrappers and patterns require an explicit backend mapping; the engine does not guess which captured child should be evaluated. An explicit backend on an abstract node takes precedence over this transparent dispatch.

## Host control

An interpreter's conditional hook can dispatch its condition and one chosen branch. A compiler's hook can dispatch both branches and build a conditional instruction. A loop hook can dispatch the same syntax repeatedly. There is no automatic memoization of runtime results, eager child evaluation, or engine-owned runtime variable map.

The `semantics` block remains independent static analysis. It can resolve names and types for the host to inspect, but it does not choose how the host stores values or executes assignments. Backend hooks never run during query settlement, parsing, checking, or editor analysis.

See the [three-backend tutorial](/tutorials/backends) and [Rust dispatch API](/api/backends).
