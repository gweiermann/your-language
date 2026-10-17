# Compound types and diagnostic evidence

This example defines structural typing in YL: primitive types, tuples, homogeneous
and mixed lists, object fields, explicit generic interfaces, member access, and
assignment checks. Type constructors, substitution, recursive comparison, and
diagnostic messages live in `definition/typing.yl`; the engine supplies generic
queries and diagnostics.

```typescript
interface Box<T> {
    value: T;
}

interface User {
    name: string;
    scores: number[];
}

let user: User = { name: "Ada", scores: [10, 20] };
let score: number = user.scores[0];
let pair: [number, string] = (42, "answer");
let mixed: (number | string)[] = [42, "answer"];
let box: Box<number> = { value: 42 };

user.name = "Grace";
```

Compile once, reload the artifact, and check a program:

```powershell
cargo run -- compile examples/compound-types/definition/language.yl -o target/compound-types.ylc
cargo run -- language target/compound-types.ylc check examples/compound-types/programs/valid.txt
cargo run -- language target/compound-types.ylc check examples/compound-types/programs/invalid-scattered.txt
cargo run -- language target/compound-types.ylc check examples/compound-types/programs/invalid-scattered.txt --json
```

The valid program passes. The scattered-evidence program builds nested objects
through several declarations before checking `Envelope<number>`. Its diagnostic
identifies `payload.details.record.value`, highlights the original `"wrong"`
literal, and includes the place requiring `Envelope<number>` in the same group.

Other negative programs isolate unknown member names and incompatible member
assignments. The latter highlights the right-hand value rather than the member
name. AST output is available with:

```powershell
cargo run -- language target/compound-types.ylc ast examples/compound-types/programs/valid.txt
```

Additional negative programs cover missing required fields, tuple lengths,
incompatible list elements, generic argument counts, and indexing a non-list.
The expected primary highlights are the incomplete object, tuple expression,
offending list element, generic type application, and index token respectively.
Duplicate object or interface field names produce one grouped diagnostic with
both name tokens highlighted.

The language policy in this example allows additional object fields during
assignment, treats mutable list element types invariantly, and requires explicit
generic arguments. It reports the first structural difference per check. Recursive
named types, inferred generic arguments, tuple indexing, flow-sensitive mutation,
and list methods such as `push` are outside this example. List index expressions
use nonnegative integer literals and return the list's element type; they do not
perform bounds checking.

Interface names become available to subsequent declarations. A recursive
`interface Loop { next: Loop; }` is rejected with an unknown-name diagnostic at
the inner `Loop`, rather than treated as an unconstrained type.

The field rules zip `fields.name` and `fields.value` before mapping them. Capture
paths retain the source identity of each field; tuple indexing then selects each
pair's name and value. Projecting a field directly from a callback's local value
(such as `field.name`) is not currently supported by the core query interface.
