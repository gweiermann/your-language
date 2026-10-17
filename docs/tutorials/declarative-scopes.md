# Resolve names with authored scopes

The declarative-query example implements lexical scopes and references in YL. Rust provides attachment queries and data operations; the library defines declaration ownership, shadowing, duplicates, and missing-name diagnostics.

```text
examples/07-declarative-queries/
├── definition/
│   ├── lexical.yl
│   ├── statements.yl
│   └── language.yl
└── programs/
    ├── shadowing.txt
    └── errors.txt
```

The authored library is `stdlib/references.yl`, available through the canonical `std/references` import. It compiles to generic attachment queries. The separate declarative-query example uses an authored variation that makes declarations visible throughout a scope; the canonical library preserves meaning-schedule visibility. Neither uses a native reference adapter or a mutable current-scope stack.

## Give each scope an identity

```yl
export symbol scope

export operation createLexicalScope(target) {
    contribute((target, scope), target)
}
```

`Program` and `Block` invoke `createLexicalScope(self)` in their semantics blocks. The operation receives that occurrence explicitly and has no implicit `self`. Their syntax identities are scope identifiers. No scope map or push/pop lifecycle is necessary.

## Register a declaration

```yl
import { error } from "core/diagnostics"

export operation enclosingScope(value) =
    find(ancestors(value), ancestor => get((ancestor, scope)))

export operation declareName(target, name, reassignable = true) {
    contribute((enclosingScope(name), declarations, text(name)), target)
    contribute((name, referenceIdentity), target)
    contribute((target, referenceIdentity), target)
    contribute((target, reassignableSymbol), reassignable)

    when hasMultiple(getAll((enclosingScope(name), declarations, text(name)))) {
        error("Duplicate declaration '${name}'", name)
    }
}
```

The declaration identity is the declaring occurrence. The captured name and owner each receive an association to that identity. This supports querying either syntax view without performing a new string lookup.

The declaration store key is `(scopeIdentity, declarations, nameText)`. `declarations` separates these entries from other metadata. Duplicate policy is expressed in the library through `getAll`, a collection predicate, and an authored diagnostic.

## Resolve nearest-first

```yl
export operation resolve(name) =
    find(ancestors(name), ancestor =>
        get((get((ancestor, scope)), declarations, text(name)))
    )
```

For `greeting;`, the query examines each enclosing scope. An inner declaration takes precedence over an outer declaration. An unfinished inner declaration keeps lookup pending until it is settled or ruled out.

This library intentionally makes declarations visible throughout their enclosing scope. Source-order visibility is a separate authored language policy, not an automatic consequence of ancestry.

## Compile and run

From the repository root:

```shell
cargo run -- compile examples/07-declarative-queries/definition/language.yl -o target/declarative.ylc
cargo run -- language target/declarative.ylc check examples/07-declarative-queries/programs/shadowing.txt
cargo run -- language target/declarative.ylc check examples/07-declarative-queries/programs/errors.txt
cargo run -- language target/declarative.ylc check examples/07-declarative-queries/programs/errors.txt --json
```

The shadowing program succeeds, including a reference before its inner declaration. The error program reports both duplicate declaration names and the missing reference, with exact name spans. Its process exits with code `1`; the JSON form returns the same diagnostic information.

For the public API tests, run:

```shell
cargo test --test declarative_queries
cargo test --test query_regressions
```
