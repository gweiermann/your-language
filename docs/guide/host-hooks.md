# Host hooks and execution

The compiled language can map a syntax construct to a declared hook:

```yl verify=hooks
export hook add(left, right)

node Addition = left: Number "+" right: Number {
    backend { add(left, right) }
}

node Number = /[0-9]+/
entry Addition
```

Both arguments are unevaluated syntax handles. The host chooses when to dispatch
children, how to represent values, and what to return. Parsing and semantic
checking never execute these hooks.

| Consumer | What its hooks produce |
| --- | --- |
| Interpreter | Runtime values, with its own bindings and scopes |
| Transpiler | Source text or a target-language AST |
| Compiler | Instructions, blocks, registers, or other compiler-owned handles |

An interpreter evaluates an `if` condition and visits one branch. A compiler may
visit both branches to emit a conditional instruction. A loop interpreter may
visit the same body repeatedly. Automatic eager child evaluation would prevent
these choices, so the shared engine provides dispatch rather than runtime policy.

Only hooks reachable through the entry grammar are required by the compiled
contract. Before dispatch, a consumer binds that contract and prepares checked
syntax. The integer preset supplies signatures; the example hosts supply their
integer values, scope maps, JavaScript generation, and WebAssembly encoding.
Strings or objects require host support rather than a different parser engine.

Follow the [course capstone](/tutorials/course/09-backends), then the
[backend API](/api/backends). Native static-analysis operations are a separate,
advanced [host extension interface](/api/native-semantics); they are not required
for authored reference or type libraries.
