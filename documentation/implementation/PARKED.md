# PARKED language-design decisions — issue #5

Issue #5 is **not complete**. The original acceptance grammar remains unchanged.
These questions concern observable YL behavior, not Rust architecture choices.
The independent compiler/artifact/runtime path has executable tests; MiniJS's
dependent compile/parse acceptance remains parked rather than given an invented AST.

## P1 — Application versus juxtaposed grouping

Sources: specification “Patterns”, “Grammar operators”, “Pipes and structural
rewrite”; issue #5 sections 2, 7, 8, 11.

```yl
pipe p(separator) {
    rewrite item+ => item (separator item)*
}
node Item = value: /x/
node P = items: Item+ |> p(",")
entry P
```

The same tokens admit two parses:

1. `Sequence(item, Repeat(Sequence(separator, item), Star))` — intended list grammar.
2. `Repeat(Call(item, [Sequence(separator, item)]), Star)` — grammar-value application.

The language permits grammar-expression arguments, untyped grammar parameters,
grouped grammar, and whitespace without semantic significance. It does not give a
syntactic discriminator or explicitly state that resolution selects between these
forms. Whitespace-based call recognition would violate sections 2/8. Expansion of
application to a bound grammar parameter returns `yl.parked_application`.
Applying a node reference also fails validation; its potential grouping meaning
is not silently selected.

**Exact decision needed:** May declaration resolution disambiguate a pattern call
from a reference followed by grouped grammar using the callee's category? Are
grammar parameters callable, or can they only stand in for grammar expressions?
If resolution selects juxtaposition, confirm that postfix operators/pipes attach to
the group, and a preceding capture attaches only to the reference.

Implemented independently: parameter binding, typed pattern arguments, grouped
expressions, sequence/repetition precedence, structural rewrites and enum cases.
All six `separatedBy` combinations preserve this blocker in tests.

## P2 — Values of rewritten separated lists

Sources: specification “Grammar values”, “Pipes and structural rewrite”;
issue #5 sections 7, 9, 11 and 19.

Assuming P1 is decided:

```yl
node Item = value: /x/
node P = items: Item+ |> separatedBy(",")
entry P
```

For `x,x`, the input grammar has `List<Item>` value. Its documented rewrite is
`item (separator item)*`. Ordinary sequence/repetition value rules return a tuple
containing the first item and a repeated list of separator/item tuples, rather than
the flat list expected of separated lists. Even if separators have unit value,
the result is `(Item, List<Item>)`. The star form adds `Option` too.

Competing interpretations:

1. Return the ordinary rewritten grammar value (tuple/optional/nested list).
2. Preserve `List<Item>` by collecting only bound item values, discarding separator
   values and representing empty lists as `[]`.
3. Add an explicit documented value transformation facility to ordinary YL.

The specification allows pipes to preserve/transform result types but supplies no
value projection syntax or preservation rule.

**Exact decision needed:** Which value does this pipe produce, and what general
YL rule/facility produces it without recognizing `separatedBy` by name? Specify
empty star-list values and tuple-valued or captured item behavior.

Implemented independently: ordinary node, option, list, union and sequence values;
capture-only concrete AST fields; transparent patterns; normalized runtime terms.
No hidden list projection is implemented.

## P3 — General constraint boolean/string language

Sources: specification “Constraints”; issue #5 section 16.

```yl
node Update = operator: "++" argument: Name {
    constraints {
        when /* conjunction or negation: syntax not specified */ {
            error("Invalid update")
        }
    }
}
```

The documented `trivia.between(left,right)` and `value.matches(/regex/)` forms are
implemented, including reusable constraints and error/warning/help emission.
Section 16 additionally requires boolean/logical and string checks without defining
operators, precedence, coercions, string methods, or absent-capture behavior.
`someLocalCondition` is a placeholder, not an operation definition. Nested condition
composition is also parked.

**Exact decision needed:** Define condition-expression syntax and evaluation rules
(operators/precedence, operand types, missing captures, string operations).
Unsupported predicates return `yl.parked_constraint_logic`; unknown lexical operators
report a frontend unexpected-token diagnostic. No scope/relation layer was added.

## P4 — Enum case selection with multiple enum parameters

Sources: specification “Pipes and structural rewrite”, “Enums and arguments”.

```yl
enum Mode { a b }
pipe choose(first: Mode, second: Mode) {
    rewrite value { .a => value .b => value }
}
```

With two enum parameters, neither `.a` nor `Mode::a` identifies the argument whose
value selects a case. Single-enum-parameter dispatch is implemented; multiple enum
parameters return `yl.parked_enum_dispatch` when expansion would require a choice.

**Exact decision needed:** Specify a selector rule or selector syntax.

## P5 — Extension metadata and singular/named sections

Sources: specification “Extending nodes across files”; issue #5 section 17.

```yl
extend node Name {
    /* metadata or a singular definition: syntax not specified */
}
```

Additive constraint extensions, module-order-independent merging, and rejection of
grammar replacement are implemented. Metadata/singular definitions are mentioned,
but their declaration syntax, keys and merge identity are not specified. Inventing
annotations or section names would add syntax. Other extension sections return
`yl.extension_conflict`.

**Exact decision needed:** Define metadata/section syntax and conflict identity,
or explicitly limit syntax-v0 extensions to constraints.

## Dependent acceptance items

- `separatedBy` matching/value tests: **PARKED** on P1/P2.
- MiniJS compilation, artifact, target AST golden, valid/invalid source runs:
  **PARKED** on P1/P2. Full MiniJS YL AST golden and source fixtures exist.
- General boolean/string constraints: **PARKED** on P3; documented examples work.
- Multiple-enum case selection: **PARKED** on P4; single-enum cases work.
- Extension metadata: **PARKED** on P5; additive constraints work.

The passing `minijs_acceptance_is_explicitly_parked_not_weakened` test verifies a
parking diagnostic, **not** MiniJS acceptance. The `syntax-v0` pipeline fixture is
separate from the unchanged acceptance target.
