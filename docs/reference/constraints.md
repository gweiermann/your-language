# Syntax-local constraints

Constraints validate a matched node using its captures, captured source text, source spans, and skipped trivia. They emit structured errors, warnings, or help messages without requiring a semantic model of the target language.

Use constraints for local rules such as adjacency, spelling conventions, or restrictions on a node's own captured values. Scope lookup, binding resolution, and target-language type checking are outside this condition language.

## Node constraint blocks

A concrete node can place a `constraints` block after its grammar:

```yl
node Name = value: /[_a-zA-Z][_a-zA-Z0-9]*/ {
    constraints {
        when value.matches(/^[A-Z]/) {
            warning("Names conventionally begin with a lowercase letter.")
        }
    }
}
```

The condition sees captures from the current node. It does not see arbitrary captures from unrelated nodes. Conditions are checked as part of parsing a successfully matched node.

`when` belongs inside a reusable `constraint` declaration or a node's `constraints` block. It is not a top-level statement.

## Diagnostic actions

Conditions may emit these actions:

```yl
error("This syntax is not allowed.")
warning("This syntax is accepted but discouraged.")
help("Consider an alternative spelling.")
```

`error` affects the success reported by checking tools. `warning` and `help` alone do not cause a failing exit status. Messages are string values; reusable constraints can parameterize them.

A syntax-local error can coexist with a constructed AST. It identifies invalid local syntax rather than necessarily preventing grammar matching. By contrast, a grammar failure leaves no complete AST. See the [AST API reference](/api/ast).

Diagnostics from an abandoned parse alternative do not appear in the final result. This prevents speculative parsing from reporting errors for a branch that was not selected.

## Boolean operators

Condition operators use familiar JavaScript-style spelling with explicit operand rules:

| Precedence, strongest first | Operator |
| --- | --- |
| Grouping | `(condition)` |
| Negation | `!` |
| Equality | `==`, `!=` |
| Conjunction | `&&` |
| Disjunction | `\|\|` |

Binary operators associate to the left. Boolean literals are `true` and `false`. The operators do not coerce strings, nodes, or lists to booleans.

```yl
when name.isPresent() && !name.matches(/^[A-Z]/) {
    warning("The name does not begin with a capital letter.")
}
```

Equality compares matching scalar string, boolean, or enum types. Enum values must belong to the same enum. Comparisons involving optional values can use `absent`. Equality always produces a boolean.

```yl
when keyword == "let" || keyword == "const" {
    help("This is a variable declaration.")
}
```

No general ordering, arithmetic, implicit coercion, or arbitrary JavaScript string method is part of the condition language.

## String matching

`capture.matches(/regex/)` tests a captured text value against a regex:

```yl
when name.matches(/^[A-Z][A-Za-z0-9_]*$/) {
    warning("This name uses the capitalized naming convention.")
}
```

For a node capture, `.matches` tests the source text covered by the captured node. For a text capture, it tests the captured string. Regex matching can succeed on a substring; use anchors when the entire value must satisfy the expression.

The receiver must be available. A possibly absent receiver needs optional chaining or a presence guard that proves access safe within the same condition.

## Absence and optional chaining

An optional grammar result may be `absent`. This differs from `false`, an empty string, and an empty list.

`.isPresent()` returns a boolean indicating whether the capture has a value:

```yl
when !name.isPresent() {
    error("A name is required here.")
}
```

Empty strings and empty lists are present values. Presence is a cardinality question, not a test of content length.

Optional chaining preserves absence:

```yl
when name?.matches(/^[A-Z]/) {
    warning("The name begins with a capital letter.")
}
```

If `name` is absent, the expression produces `absent`; otherwise it produces the matching boolean. A `when` accepts boolean or optional boolean and runs its body only when the result is `true`. It does not require `== true`.

Use explicit equality to distinguish absence from a failed match:

```yl
when name?.matches(/^[A-Z]/) == false {
    help("The name exists but does not begin with a capital letter.")
}

when name?.matches(/^[A-Z]/) == absent {
    help("The name was omitted.")
}
```

An ordinary `.matches` call on a possibly absent capture is a definition error unless the condition establishes presence before the call:

```yl
when name.isPresent() && name.matches(/^[A-Z]/) {
    warning("Capitalized name.")
}
```

This guard works because `&&` short-circuits.

## Short-circuit behavior

Negating absence preserves it: `!absent` is `absent`. Thus `!name?.matches(...)` means the name is present and does not match, when used directly in a `when`.

For conjunction:

- `false && rhs` returns `false` without evaluating `rhs`.
- `absent && rhs` returns `absent` without evaluating `rhs`.
- `true && rhs` evaluates and returns `rhs`.

For disjunction:

- `true || rhs` returns `true` without evaluating `rhs`.
- `false || rhs` evaluates and returns `rhs`.
- `absent || rhs` evaluates and returns `rhs`.

```yl
when primary?.matches(/^[A-Z]/) || fallback?.matches(/^[A-Z]/) {
    help("At least one name uses the capitalized form.")
}
```

To include an absent name in a negative test, compare explicitly:

```yl
when name?.matches(/^[A-Z]/) != true {
    error("A capitalized name is required.")
}
```

## Independent and nested conditions

Separate `when` clauses run independently, in source order. Emitting an error does not stop later clauses or establish presence for them:

```yl
when !name.isPresent() {
    error("A name is required.")
}

when name?.matches(/^[A-Z]/) {
    warning("Names conventionally start with lowercase letters.")
}
```

Nested `when` clauses execute only when their enclosing conditions are true:

```yl
when name.isPresent() {
    when name.matches(/^[A-Z]/) {
        warning("Capitalized name.")
    }
}
```

The enclosing presence condition permits the inner ordinary method access.

## Trivia between captures

`trivia.between(left, right)` checks skipped trivia between two captures:

```yl
node Update = operator: ("++" | "--") argument: Name {
    constraints {
        when trivia.between(operator, argument) {
            error("Whitespace or comments are not allowed here.")
        }
    }
}
```

Use a named trivia matcher to restrict the check:

```yl
when Comment.between(operator, argument) {
    warning("A comment appears inside the update expression.")
}
```

The endpoints must refer to current captures, and the trivia name must resolve as a visible declaration. Trivia must be explicitly selected in the [entry configuration](./trivia#entry-configuration) to participate in automatic skipping.

## Reusable constraints

Declare a named constraint for rules used by several nodes:

```yl
constraint adjacent(left, right, message="Trivia is not allowed here.") {
    when trivia.between(left, right) {
        error(message)
    }
}

node Update = operator: ("++" | "--") argument: Name {
    constraints {
        adjacent(operator, argument)
    }
}
```

Constraint parameters can receive capture references, regexes, literal messages, and configuration values. Calls support positional arguments, named arguments, defaults, and forwarding to other reusable constraints:

```yl
constraint matchesConvention(value, expression=/^[a-z]/) {
    when !value.matches(expression) {
        warning("The value does not follow the convention.")
    }
}
```

A parameter declared with a node type requires a capture of that node or one of its members. An enum-typed parameter resolves contextual variants against its declared enum. These checks validate the grammar definition; they do not infer target-language types.

Reusable constraints follow ordinary module visibility. Export them to share rules across modules.

## Node extensions

An extension contributes constraints to the existing canonical node:

```yl
import { Name } from "./lexical"

extend node Name {
    constraints {
        when value.matches(/^reserved$/) {
            error("This spelling is reserved.")
        }
    }
}
```

Additive constraints merge across modules. An extension cannot redefine grammar or add unspecified metadata sections. Import order must not change the resulting language meaning.
