# 6. Add functions and independent call inference

A function names reusable work. A **parameter** is the local name for an input;
an **argument** is the expression supplied by a call. In `identity(42)`,
`identity` names the function and `42` is its argument. A return expression
provides the result. We want an independently inferred result for each call:
a number in, a number out; a string in, a string out.

The teaching project now has expressions, references, and type restrictions.
The next checkpoint expands it with parameters, signatures, return annotations,
and calls. Use the maintained multi-file Mini TypeScript implementation rather
than paste isolated function snippets into the earlier reduced grammar.

<!-- course:project=examples/08-mini-typescript -->

From the repository root, copy `examples/08-mini-typescript/definition/`,
`programs/`, and `yl.json` into your project, replacing those earlier files.
The public operations have the same roles; this checkpoint adds `function.yl`,
`parameter.yl`, `typing.yl`, and richer expression/type modules. Each syntax
construct still keeps its semantic connections beside its captures.

```sh
cargo run --example 08-mini-typescript
cargo run --example mini_typescript_host -- addition
```

The project runner checks valid and intentional-error programs. The host reports
bindings such as `numberSum: Type::number` and `stringSum: Type::string`.
In your copied project, use `yl project check programs/program.txt`.

## Read the declaration from the outside inward

Start at `FunctionDeclaration` below: its `name` capture identifies the declared
function. `FunctionContents` contains parameters in parentheses, an optional
return annotation, and a body. `* |> separatedBy(",")` recognizes a list of
parameters separated by commas; the separators do not become captured parameters.
The contents create a lexical scope so parameter names are visible in the body.
The body's `boundary` prevents an outer scheduling rule from collecting nested
function declarations as if they were top-level declarations.

This complete file is a reading reference, not a replacement entry file:


<p class="course-file" data-file="examples/08-mini-typescript/definition/function.yl"><strong>Read the complete file:</strong> <code>examples/08-mini-typescript/definition/function.yl</code></p>

<<< ../../../examples/08-mini-typescript/definition/function.yl

## Trace the signature before the call

Open `types.yl`, then `parameter.yl`, `function.yl`, and finally `typing.yl` in that
order. The symbols in `types.yl` name relationships, not extra runtime types:

| Key | Meaning |
| --- | --- |
| `(functionIdentity, parameterNames)` | Ordered text names used to pair arguments with parameters. |
| `(functionIdentity, parameters, nameText)` | The type slot for one specific parameter. |
| `(functionIdentity, returnType)` | The public return-type relationship. |
| `(contentsOccurrence, functionIdentity)` | Connect the parameter/body scope to its owning function declaration. |

`functionSignature(name, arguments, body, annotation, contents)` receives the
captured declaration name, parameter occurrences, body, optional return annotation,
and scope wrapper. It registers these relationships on `referenceOf(name)`.
An explicitly annotated return restricts `body.result`; otherwise the return slot
depends on `(body, returnType)`. The block itself does not acquire a value type.

For `function identity(value) { return value; }`, call the declaration identity
**F** and the body occurrence **B**. These are explanatory labels, not extra YL
variables. The relationships form this chain:

| Key | Value or dependency |
| --- | --- |
| `(F, parameterNames)` | `["value"]` |
| `(F, parameters, "value")` | The parameter type slot, initially without a concrete type |
| `(B, returnType)` | The type of the `return value` expression |
| `(F, returnType)` | The exposed body return relationship |

Inside `FunctionBody`, the decisive connection is:

<p class="course-file" data-file="examples/08-mini-typescript/definition/function.yl"><strong>Read this line in:</strong> <code>examples/08-mini-typescript/definition/function.yl</code></p>

```yl
annotateWithType((self, returnType), typeOf(result))
```

Here `result` is the captured return expression. It eventually reads the parameter's
reference type. In `typing.yl`, this exact helper connects that parameter occurrence
to the function's named slot:

<p class="course-file" data-file="examples/08-mini-typescript/definition/typing.yl"><strong>File:</strong> <code>examples/08-mini-typescript/definition/typing.yl</code></p>

```yl source=examples/08-mini-typescript/definition/typing.yl#parameterType
export operation parameterType(target, name, annotation) {
    annotateWithType((functionOf(target), parameters, text(name)), typeOf(annotation))
    annotateWithType(referenceOf(name), typeOf((functionOf(target), parameters, text(name))))
    annotateWithType(name, typeOf(referenceOf(name)))
}
```

`target` is the parameter occurrence, `name` its exact name capture, and
`annotation` its optional type syntax. `functionOf(target)` reads the owning
function from the enclosing contents scope. `typeOf(annotation)` contributes
only when present; an unannotated parameter remains available for call-specific inference.

## Author the call assumptions

`zip(names, arguments)` pairs corresponding items: `["value"]` and
`[the42Occurrence]` become one pair. `map(pairs, pair => result)` transforms each
pair into a result. `first(pair)` reads its name and `last(pair)` its argument.
A nested tuple below is a parameter key grouped inside an assumption record.

Read these helpers next. They are ordinary operations from the same `typing.yl`:

<p class="course-file" data-file="examples/08-mini-typescript/definition/typing.yl"><strong>File:</strong> <code>examples/08-mini-typescript/definition/typing.yl</code></p>

```yl source=examples/08-mini-typescript/definition/typing.yl#assumptions
operation assumptions(function, arguments) = map(zip(get((function, parameterNames)), arguments), pair => ((function, parameters, first(pair)), typeOf(last(pair)), last(pair)))
```

`function` is a resolved declaration identity. `arguments` is the positional list
of captured expressions. Each result tuple contains a logical parameter key,
the corresponding argument's type, and that argument as the diagnostic origin.
For `identity(42)`, the assumption is conceptually:

```text
((identityDeclaration, parameters, "value"), Type::number, the42Occurrence)
```

Trace two calls independently:

| Call | Temporary parameter slot | Return expression sees | Result |
| --- | --- | --- | --- |
| `identity(42)` | `(F, parameters, "value")` has `Type::number` | A number | `Type::number` |
| `identity("hello")` | The same key has `Type::string` in a separate query | A string | `Type::string` |

This is a hypothetical question: *if the parameter had this argument's type,
what would the return type be?* The first call does not permanently specialize
the declaration. Its temporary assumption is isolated from the second call.

The call first checks arity and parameter restrictions; `zip` alone truncates,
so it cannot enforce the number of arguments. Then the result helper chooses
an explicit declared return or asks for the dependent inferred return:

<p class="course-file" data-file="examples/08-mini-typescript/definition/typing.yl"><strong>File:</strong> <code>examples/08-mini-typescript/definition/typing.yl</code></p>

```yl source=examples/08-mini-typescript/definition/typing.yl#callResult
operation callResult(function, arguments) = choose(isPresent(declaredResult(function)), declaredResult(function), resolveTypeWith((function, returnType), assumptions(function, arguments)))
```

`resolveTypeWith` overrides only the supplied parameter type slots for this query.
The body return depends on the parameter reference, so the temporary type reaches
the return. Outer captures and the original declaration remain unchanged.
The exported `call(target, callee, arguments)` helper annotates the call occurrence
with that result; it also diagnoses a non-callable target, wrong arity, and
incompatible annotated parameters. Read that helper last, once its inputs and
result relationship are clear.

## Test both specializations and the error

Add these files to the copied project:

<p class="course-file" data-file="programs/course-inference.txt"><strong>File:</strong> <code>programs/course-inference.txt</code></p>

```text file=programs/course-inference.txt
function identity(value) {
    return value;
}

let count: number = identity(42);
let title: string = identity("hello");
```

<p class="course-file" data-file="programs/course-errors.txt"><strong>File:</strong> <code>programs/course-errors.txt</code></p>

```text file=programs/course-errors.txt expect-error
function identity(value) {
    return value;
}

let broken: number = identity("wrong");
```

```sh
yl project check programs/course-inference.txt
yl project check programs/course-errors.txt --json
```

The first passes with two independently inferred call results. The second fails
at the incompatible initializer and relates the `number` requirement. Changing
`broken` to `string` makes it pass. The function is not globally specialized by
either call. Explicit return annotations can anchor supported recursive
relationships; unconstrained recursive hypotheses can report cycles.

Try another deliberate mistake: replace `identity(42)` with `identity(42, 1)`
in the valid file. Its check exits **1** with an argument-count diagnostic.
Restore the call and check again. **Arity** is the number of parameters or
arguments; checking it matters because `zip` alone drops unmatched items.

For lower-level views of the same mechanism, run these repository companions:

```sh
cargo run --example 09-type-instantiation
cargo run --example 10-declarative-functions
```

Read [the inference walkthrough](/tutorials/std-types) for the helper definitions
and [the query contract](/api/declarative-queries#hypothetical-evaluation) for exact
isolation and diagnostic behavior. This is useful per-call inference, not a claim
of complete TypeScript generic inference.

Previous: [Types](./05-types).

Next: [Compound data](./07-compound-types).
