# Enums and arguments

Enums provide a finite set of named configuration values. They are useful for selecting grammar behavior in patterns, pipes, and reusable constraints.

## Declaring an enum

```yl
enum Delimiter {
    round
    square
}
```

Variants are separated by whitespace. The enum and its variants are definition-time values; this declaration does not recognize the words `round` or `square` in target source.

Export an enum when callers in another module need to refer to it explicitly:

```yl
export enum Delimiter {
    round
    square
}
```

## Explicit variants

Use the enum's qualified name:

```yl
Delimiter::round
Delimiter::square
```

This spelling identifies the enum without relying on an expected parameter type. Imported aliases can name the enum through the normal module resolver.

## Contextual variants

Use a leading dot when the expected enum type is known:

```yl
.round
.square
```

For example, a typed parameter supplies the context:

```yl
pipe delimit(kind: Delimiter = .round) {
    rewrite value match kind {
        .round => "(" value ")"
        .square => "[" value "]"
    }
}

node Name = value: /[a-z]+/
node Group = value: Name |> delimit(kind=.square)
```

The compiler validates that the variant belongs to the expected enum. A contextual variant without enough type information is not resolved by guessing.

## Parameters, defaults, and call syntax

Parameter declarations use `:` for a type and `=` for a default:

```yl
kind: Delimiter = .round
```

Call arguments may be positional or named:

```yl
delimit(.square)
delimit(kind=.square)
delimit(kind=Delimiter::square)
```

Omitted arguments use their declared defaults. Positional arguments precede named arguments. The compiler reports unknown parameters, duplicate arguments, missing required arguments, and incompatible enum values.

## Matching enum combinations

A rewrite may select one enum parameter or a tuple:

```yl
enum Mode { strict permissive }

pipe configured(first: Mode, second: Mode) {
    rewrite value match (first, second) {
        (.strict, .strict) => "(" value ")"
        (.strict, .permissive) => {
            error("This combination is not supported.")
        }
        _ => value
    }
}
```

Each tuple component corresponds to the selector in the same position. `_` matches one component or the complete selector tuple. Cases are ordered and must cover every possible combination.

With exactly one enum parameter, the shorthand `rewrite value { .round => ... .square => ... }` selects that parameter. Multiple enum parameters require an explicit selector, even when only one controls a particular rewrite.

See [Pipes and structural rewrite](./pipes#enum-selected-rewrites) for exhaustiveness, diagnostics, and output cardinality.
