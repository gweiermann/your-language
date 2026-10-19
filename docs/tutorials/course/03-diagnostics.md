# 3. Diagnose captured syntax

Replace the lexical module. The grammar still captures the name; its local
check now rejects a reserved spelling. Diagnostic functions are explicit imports.

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

```text file=programs/invalid.txt expect-error
let reserved = 42;
```

```sh
yl project check programs/invalid.txt
yl project check programs/invalid.txt --json
```

Both commands fail with a source-linked diagnostic. The human version underlines
`reserved`; the JSON version exposes the same byte span. `warn` and `hint` use
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
