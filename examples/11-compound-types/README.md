# Compound types

Structural typing written in YL: tuples, lists, objects, explicit generic interfaces,
property/index access, and assignment checks. Types are plain descriptors; this
example does not require source-origin trees.

```typescript
interface Box<T> {
    value: T;
}

let box: Box<number> = { value: 42 };
let value: number = box.value;
let pair: [number, string] = (42, "answer");
let items: (number | string)[] = [42, "answer"];
```

From the repository root:

```powershell
cargo run -- project check examples/11-compound-types
cargo run -- project check examples/11-compound-types/programs/valid.txt
cargo run -- project check examples/11-compound-types/programs/invalid-assignment.txt
cargo run -- project check examples/11-compound-types/programs/invalid-assignment.txt --json
cargo run -- project ast examples/11-compound-types/programs/valid.txt
```

The project check succeeds when valid programs pass and every `expectErrors`
fixture fails as declared in `yl.json`. Checking one invalid file directly exits
with status 1 and displays its diagnostics.

`definition/typing.yl` contains constructors, recursive structural comparison,
generic substitution, and text formatting. `expression.yl`, `interface.yl`, and
`let.yl` connect these operations to syntax. An annotation establishes a write
contract without changing the initializer's inferred type.

A mismatch highlights the checked expression. For an original literal several
references away, continue with [diagnostic origins](../12-diagnostic-origins/README.md).

Policy: additional object fields are assignable; mutable list element types are
invariant; generic arguments are explicit. Duplicate fields are rejected. Only
the first structural difference is reported per check. Recursive named types,
inferred generic arguments, tuple indexing, flow-sensitive mutation, list bounds,
and methods such as `push` are not implemented by this example.
