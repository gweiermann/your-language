# Resolve names with authored scopes

The declarative-query example implements lexical scopes and references in YL. Rust provides attachment queries and data operations; the library defines declaration ownership, shadowing, duplicates, and missing-name diagnostics.

```text
examples/declarative-queries/
├── definition/
│   ├── lexical.yl
│   ├── statements.yl
│   └── language.yl
└── programs/
    ├── shadowing.txt
    └── errors.txt
```

The authored library is `stdlib/references.yl`, imported as a normal relative module by this example. The older native `std/references` adapter remains available to existing native type consumers; replacing that adapter requires migration to the shared declarative hook contract. The example does not use that adapter or a mutable current-scope stack.

## Give each scope an identity

```yl
export symbol scope

export operation lexicalScope() {
    contribute((self, scope), self)
}
```

`Program` and `Block` invoke this operation. Their syntax identities are scope identifiers. No scope map or push/pop lifecycle is necessary.

## Register a declaration

```yl
export operation enclosingScope(value) =
    find(ancestors(value), ancestor => get((ancestor, scope)))

export operation declare(name, reassignable = true) {
    contribute((enclosingScope(self), declarations, text(name)), self)
    contribute((name, referenceIdentity), self)
    contribute((self, referenceIdentity), self)
    contribute((self, reassignableSymbol), reassignable)

    if hasMultiple(getAll((enclosingScope(self), declarations, text(name)))) {
        error("Duplicate declaration '${name}'", name)
    }
}
```

The declaration identity is the declaring occurrence. The captured name and owner each receive an association to that identity. This supports querying either syntax view without performing a new string lookup.

The declaration store key is `(scopeIdentity, declarations, nameText)`. `declarations` separates these entries from other metadata. Duplicate policy is expressed in the library through `getAll`, a collection predicate, and an authored diagnostic.

## Resolve nearest-first

```yl
export operation resolve(name) =
    find(ancestors(self), ancestor =>
        get((get((ancestor, scope)), declarations, text(name)))
    )
```

For `greeting;`, the query examines each enclosing scope. An inner declaration takes precedence over an outer declaration. An unfinished inner declaration keeps lookup pending until it is settled or ruled out.

This library intentionally makes declarations visible throughout their enclosing scope. Source-order visibility is a separate authored language policy, not an automatic consequence of ancestry.

## Compile and run

From the repository root:

```shell
cargo run -- compile examples/declarative-queries/definition/language.yl -o target/declarative.ylc
cargo run -- language target/declarative.ylc check examples/declarative-queries/programs/shadowing.txt
cargo run -- language target/declarative.ylc check examples/declarative-queries/programs/errors.txt
cargo run -- language target/declarative.ylc check examples/declarative-queries/programs/errors.txt --json
```

The shadowing program succeeds, including a reference before its inner declaration. The error program reports both duplicate declaration names and the missing reference, with exact name spans. Its process exits with code `1`; the JSON form returns the same diagnostic information.

For the public API tests, run:

```shell
cargo test --test declarative_queries
cargo test --test query_regressions
```
