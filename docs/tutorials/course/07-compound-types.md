# 7. Model interfaces and compound data

An **interface** describes named properties: an object may have a `value`
property whose type is string. A **list** contains repeated elements, and a
**tuple** describes positions that may have different types. Their descriptions
contain other type descriptions, so these are compound types.

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
`yl project check programs/valid.txt` (exit **0**, no output). We will first
understand one field, then return to the larger maintained program.

## Build the descriptor, then look up one field

Open `definition/typing.yl` in the copied project. A **descriptor** is data
about a type, not a value in the program. `"hello"` is a program value;
`(Type::string, [])` describes its type. The first tuple item is a **tag**
identifying the kind; the second is its **payload**, or additional information.
An empty list means this primitive type needs no additional information.

Read `(Type::struct, [("value", (Type::string, []))])` from the outside inward:
interface, list of fields, field named value, string type. Parentheses group
fixed-position tuples; square brackets contain lists. This is ordinary YL data,
not a new engine-defined object representation.

The interface payload is a list of `(fieldName, fieldType)` pairs. These exact
constructors are small enough to inspect directly:

<p class="course-file" data-file="examples/11-compound-types/definition/typing.yl"><strong>Read this helper in:</strong> <code>examples/11-compound-types/definition/typing.yl</code></p>

```yl source=examples/11-compound-types/definition/typing.yl#stringType
export operation stringType() =(Type::string, [])
```

<p class="course-file" data-file="examples/11-compound-types/definition/typing.yl"><strong>Read this helper in:</strong> <code>examples/11-compound-types/definition/typing.yl</code></p>

```yl source=examples/11-compound-types/definition/typing.yl#structType
export operation structType(fields) =(Type::struct, fields)
```

The tag is internally named `struct`, but the target syntax is `interface`.
For `{ value: "hello" }`, the descriptor is equivalent to
`(Type::struct, [("value", stringType())])`. The `Object` node in `expression.yl`
builds it by pairing captured field names with `typeOf` each captured value.
It checks duplicate field names before attaching the descriptor to `self`.

Tuple indexing starts at zero: `parent[0]` is its tag and `parent[1]` its payload.

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

A **generic** description contains placeholders called **type parameters**.
In `Box<T>`, `T` is a placeholder; `Box<string>` supplies string as its replacement.

Read `interface.yl`: each `TypeParameter` declares a name and attaches
`parameterType(self)`. `InterfaceContents` scopes those parameters. The interface
stores `genericType(parameters, typeOf(contents.body))` on its declaration.
When `Box<string>` is used, the type-expression rule resolves `Box`, checks its
argument count, and invokes this helper:

<p class="course-file" data-file="examples/11-compound-types/definition/typing.yl"><strong>Read this helper in:</strong> <code>examples/11-compound-types/definition/typing.yl</code></p>

```yl source=examples/11-compound-types/definition/typing.yl#instantiateType
export operation instantiateType(generic, arguments) = choose(sameLength(generic[0], arguments), substituteType(generic[1], zip(generic[0], arguments)), absent)
```

Indexing starts at zero: `[0]` selects the first tuple item, and `[1]` the
second. `sameLength` checks argument count. `choose(condition, yes, no)` selects
one result, returning `absent` when counts differ here. `zip` pairs each parameter
identity with its supplied type.

`generic[0]` is the parameter-identity list; `generic[1]` is its body descriptor.
`substituteType` recursively replaces only those identities inside tuples, lists,
unions, and interfaces. Repeated uses of `T` therefore receive the same argument;
parameters from a different declaration remain different identities.

For `Box<T> { value: T; }`, the stored template contains the identity of `T`.
Applying `Box<string>` replaces that identity inside the `value` descriptor with
`(Type::string, [])`. The result matches the descriptor of `{ value: "hello" }`.
`Box<number>` makes a separate substitution and does not mutate the template.

Add two focused programs:

<p class="course-file" data-file="programs/course-structure.txt"><strong>Create file:</strong> <code>programs/course-structure.txt</code></p>

```text file=programs/course-structure.txt
interface Box<T> {
    value: T;
}

let box: Box<string> = { value: "hello" };
let message: string = box.value;
```

<p class="course-file" data-file="programs/invalid-course.txt"><strong>Create file:</strong> <code>programs/invalid-course.txt</code></p>

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

Try changing `box.value` to `box.missing` in the valid program. Checking it exits
**1** with an unknown-property diagnostic at `missing`. Restore `value`. Change
`42` to `"hello"` in the invalid program and check again: it now passes. Restore
`42` to keep the deliberate error fixture.

Once the one-field example is clear, read the larger maintained program. It
combines nested interfaces and lists using the same descriptors:

<p class="course-file" data-file="examples/11-compound-types/programs/valid.txt"><strong>Read the complete file:</strong> <code>examples/11-compound-types/programs/valid.txt</code></p>

<<< ../../../examples/11-compound-types/programs/valid.txt

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
