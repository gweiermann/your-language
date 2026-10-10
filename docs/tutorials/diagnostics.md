# Add syntax diagnostics

Constraints validate syntax already captured by a node. This guide demonstrates optional captures, presence checks, and the difference between AST output and diagnostic output. Complete files are in `examples/constraints/`.

## Capture an optional name

```yl
node Program = name: /[A-Za-z]+/? ":" {
    constraints {
        when name?.matches(/^[A-Z]/) {
            warning("Names should begin with a lowercase letter.")
        }
        when !name.isPresent() {
            error("A name is required.")
        }
    }
}
entry Program
```

The grammar accepts both `Example:` and `:`. Constraints can therefore report a domain-specific message for the missing name rather than failing to match the node.

`?.matches` returns `absent` if the name did not match and a boolean otherwise. `when` emits only for `true`. `.isPresent()` distinguishes absence from a present value. Separate clauses run independently.

## Inspect human diagnostics

```sh
yl compile examples/constraints/language.yl -o constraints.ylc
yl language constraints.ylc check examples/constraints/warning.txt
yl language constraints.ylc check examples/constraints/error.txt
```

The first source emits a warning and exits successfully. The second emits an error and exits nonzero. Terminal diagnostics include a source location, the relevant line, and colored underlines when stderr is a terminal.

## Integrate with tools

```sh
yl language constraints.ylc check examples/constraints/error.txt --json
yl language constraints.ylc ast examples/constraints/error.txt
```

The check command writes a diagnostic array to stdout. The AST command writes the tree to stdout and diagnostic JSON to stderr. Syntax-local errors retain a successfully built AST; a grammar parse failure produces `null` instead.

## Guard an ordinary method call

An ordinary `.matches` call on a possibly absent capture must have a presence guard:

```yl
when name.isPresent() && name.matches(/^[A-Z]/) {
    warning("Names should begin with a lowercase letter.")
}
```

Short-circuiting prevents the method call when the capture is absent. Optional chaining is more compact when absence should simply skip the check. For comparisons, negation, reusable checks, and trivia queries, see [constraints](/reference/constraints).
