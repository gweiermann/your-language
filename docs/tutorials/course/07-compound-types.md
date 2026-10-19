# 7. Model interfaces and compound data

Types can be structured values, not only primitive symbols. This checkpoint
switches to the structural-data variant in `11-compound-types`: it keeps lexical
references and typed declarations, then adds interfaces, properties, tuples,
lists, assignment checks, and explicit generic arguments.

<!-- course:project=examples/11-compound-types -->

Copy its `definition/`, `programs/`, and `yl.json` into your learning project.
This is an explicit variant: it does not retain the previous checkpoint's function
syntax, and the final backend example does not execute these structural objects.
The course shares the building techniques without pretending those subsets are
already one complete TypeScript implementation.

```sh
cargo run --example 11-compound-types
cargo run -- project check examples/11-compound-types/programs/valid.txt
```

These commands run from the repository root. In the copied project, use
`yl project check programs/valid.txt`. The complete checked program is:

<<< ../../../examples/11-compound-types/programs/valid.txt

## Build the descriptor, then look up one field

Open `typing.yl` first. A primitive descriptor is a tag and payload; an interface
payload is a list of `(fieldName, fieldType)` pairs. These exact constructors are
small enough to inspect directly:

```yl source=examples/11-compound-types/definition/typing.yl#stringType
export operation stringType() =(Type::string, [])
```

```yl source=examples/11-compound-types/definition/typing.yl#structType
export operation structType(fields) =(Type::struct, fields)
```

The tag is internally named `struct`, but the target syntax is `interface`.
For `{ value: "hello" }`, the descriptor is equivalent to
`(Type::struct, [("value", stringType())])`. The `Object` node in `expression.yl`
builds it by pairing captured field names with `typeOf` each captured value.
It checks duplicate field names before attaching the descriptor to `self`.

Now read the `Member` node immediately below it. `base` and `name` are visible
in its grammar. Its semantic block performs these steps:

1. `parent := typeOf(base)` obtains the base expression's descriptor.
2. Verify the tag is `Type::struct`, then query `lookup(parent[1], text(name))`.
3. Attach the selected field type to the member occurrence, or underline an
   unknown property at `name`.
4. Separately propagate the field's declared write contract for assignments.

No engine-specific property lookup is hidden here: the descriptor, pair list,
and compatibility policy are authored in YL. `lookup` is the generic core data operation.

## Connect an explicit type parameter

Read `interface.yl`: each `TypeParameter` declares a name and attaches
`parameterType(self)`. `InterfaceContents` scopes those parameters. The interface
stores `genericType(parameters, typeOf(contents.body))` on its declaration.
When `Box<string>` is used, the type-expression rule resolves `Box`, checks its
argument count, and invokes this helper:

```yl source=examples/11-compound-types/definition/typing.yl#instantiateType
export operation instantiateType(generic, arguments) = choose(sameLength(generic[0], arguments), substituteType(generic[1], zip(generic[0], arguments)), absent)
```

`generic[0]` is the parameter-identity list; `generic[1]` is its body descriptor.
`substituteType` recursively replaces only those identities inside tuples, lists,
unions, and interfaces. Repeated uses of `T` therefore receive the same argument;
parameters from a different declaration remain different identities.

Add two focused programs:

```text file=programs/course-structure.txt
interface Box<T> {
    value: T;
}

let box: Box<string> = { value: "hello" };
let message: string = box.value;
```

```text file=programs/invalid-course.txt expect-error
interface Box<T> {
    value: T;
}

let box: Box<string> = { value: 42 };
```

The first passes `yl project check programs/course-structure.txt`. The second
fails `yl project check programs/invalid-course.txt`: recursive comparison finds
`number` where the instantiated `value` field requires `string`. Trace
`compareTypes` into `compareFields` and `compareField` in `typing.yl` to see the
path carried into that diagnostic. Exact comparison additionally rejects extra
fields; mutable list elements are invariant.

The next companion reuses the opt-in `std/types/structural` policy and adds source
origins as a separate concern:

```sh
cargo run --example 12-diagnostic-origins
cargo run -- project check examples/12-diagnostic-origins/programs/invalid-scattered.txt
```

The first checks the full project including expected errors. The second
intentionally fails: its grouped diagnostic connects an incompatible nested value
with the place requiring another type. Plain descriptors do not need origin records;
retain them only when richer diagnostics are part of the language's policy.

See [compound comparison](/tutorials/compound-types),
[diagnostic origins](/tutorials/diagnostic-origins), and the
[structural library contract](/reference/standard-library#std-types-structural).

Previous: [Functions](./06-functions).

Next: [Editor support](./08-editor).
