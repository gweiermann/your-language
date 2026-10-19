# 3. Diagnose captured syntax

A parser can recognize a name even when your language wants to forbid it.
This lesson rejects the name `reserved`, without changing which characters form
a name. Continue inside **`first-language/`** with lesson 2's files.

## Add a rule about a captured value

A **diagnostic** is a message linked to source text. An error makes checking fail;
a warning or hint can explain a concern without failing. The diagnostic functions
are not implicitly available: import `error` from `core/diagnostics`.

`checks { ... }` adds rules about the syntax captured by this node.
`when condition { ... }` applies its diagnostic only when the condition is true.
Here `value` is the capture on `Name`. `value.matches(/^reserved$/)` tests its
text with a regular expression. `^` means the beginning and `$` the end, so this
matches the entire spelling `reserved`, rather than `reservedName`.

Replace only the lexical file. Its public names stay the same, so the other
modules and their imports need no changes:

<p class="course-file" data-file="definition/lexical.yl"><strong>Replace file:</strong> <code>definition/lexical.yl</code></p>

```yl file=definition/lexical.yl
import { boundedBy } from "std/parser"
import { error } from "core/diagnostics"

pattern IdentifierPart = /[A-Za-z0-9_]/

export pattern keyword(value) = value |> boundedBy(IdentifierPart)

export node Name = value: /[A-Za-z_][A-Za-z0-9_]*/ {
    checks {
        when value.matches(/^reserved$/) {
            error("Choose a name other than 'reserved'.")
        }
    }
}

export trivia Whitespace = /\s+/
```

<p class="course-file" data-file="programs/invalid.txt"><strong>Create file:</strong> <code>programs/invalid.txt</code></p>

```text file=programs/invalid.txt expect-error
let reserved = 42;
```

```sh
yl project check programs/invalid.txt
yl project check programs/invalid.txt --json
```

Both commands exit **1**. The first prints a diagnostic on standard error,
including the source line and an underline under `reserved`. The second prints
a diagnostic array on standard output: its `primary` span identifies the same
characters as byte offsets. Use that form when another program consumes results.

The primary part of the human output is:

```text
Error[parse.constraint]: Choose a name other than 'reserved'.
  --> programs/invalid.txt:1
  1 | let reserved = 42;
    |     ^^^^^^^^
```

The full output also points to the `error(...)` rule in `definition/lexical.yl`;
that second path is absolute and depends on where you created the project.
The JSON diagnostic's `primary` member is:

```json
{
  "file": "programs/invalid.txt",
  "start": 4,
  "end": 12
}
```

This is one member of the diagnostic, not the full array returned by `--json`.

A diagnostic has a code (so tools need not parse the message), a message, a source
file, and one or more spans. The underline is derived from those spans; it is not
part of the name's AST value. In this program `reserved` begins after `let `,
so its start offset is **4** and its end offset is **12**.

Change `reserved` to `available` in the test file and run the first command
again. It exits **0** without a diagnostic. Restore the intentional error if you
want to keep this fixture alongside the valid program.

`warn` and `hint` use
the same explicit import model but do not fail the check.

This rule needs only parsed captures, so it belongs in `checks`. Unknown names
and incompatible types need relationships between occurrences; those belong
in `semantics`. Keep each short connection beside its grammar and extract
substantial reusable rules into imported operations.

The repository checkpoint `cargo run --example 02-constraints` also shows optional
captures and guarded access. [Grouped diagnostics](/api/diagnostics#grouped-semantic-diagnostics)
can later connect a bad initializer and its required annotation.

Previous: [Modules](./02-modules).

Next: [References](./04-references).
