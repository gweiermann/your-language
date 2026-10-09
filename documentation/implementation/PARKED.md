# PARKED language-design decisions — issue #5

Issue #5 is **not complete**. The original acceptance grammar remains unchanged.
These questions concern observable YL behavior, not Rust architecture choices.
The compiler/artifact/runtime path and unchanged MiniJS acceptance have executable tests.
General constraint operations and metadata remain parked as described below.

## P1 — RESOLVED from the documented declaration categories

The original parking was too broad. The checked-in MiniJS Parameter rule explicitly
requires `Name ("=" default: Expression)?` to be a node reference followed by grouped
grammar. Structural rewrite bindings likewise hold grammar expressions, not callable
definitions. Treating these as calls would require undocumented callable-node or
higher-order-grammar semantics.

Resolution now classifies name-plus-parentheses after module collection. Nodes and
bound grammar expressions form a sequence with the group; documented pattern/core
calls remain calls. Postfix/pipes attach to the group and a capture before the node
captures the reference. Explicit parentheses around the whole sequence retain their
larger-fragment meaning. No whitespace distinction or MiniJS name checks are used.

The unchanged MiniJS target now compiles, serializes, reloads and parses its valid
program. All six separatedBy modes have matching and flat-list tests; imported,
qualified, forward-reference and capture/group/pipe cases have focused tests.

## P2 — RESOLVED: values of rewritten separated lists

The user clarified on 2026-10-09 that **all pipes preserve their input type**.
Repeated rewrites collect only Item values and discard separators and other wrapper
values. This overrides the earlier statement that pipes may transform result types.
The specification now records the decision. Generic normalized value projections
implement it without recognizing stdlib helper names. Empty star lists are `[]`;
tuple-valued items remain individual elements. Scalar wrappers and chained pipes
have type-preservation tests. The original question is retained below as context.

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
No stdlib-specific list projection is implemented; generic projection terms are used.

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
report a frontend unexpected-token diagnostic. Applying a predicate to a capture
which may be absent returns `yl.parked_constraint_optional`, rather than choosing
an implicit coercion. No scope/relation layer was added.

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

## Acceptance status

- MiniJS and separatedBy compilation, artifact/reload, AST and negative diagnostics:
  **PASSING**. Full frontend/normalized/runtime snapshots are committed.
- General boolean/string constraints and absent-capture evaluation: **PARKED** on P3.
- Multiple-enum case selection: **PARKED** on P4; single-enum cases work.
- Extension metadata: **PARKED** on P5; additive constraints work.
- Scalar binding omission/duplication: **PARKED** on P6; single-value wrappers work.

Issue #5 remains incomplete because P3/P5 affect required acceptance items. The
independent fixture remains additional coverage, rather than a MiniJS substitute.

## P6 — Scalar rewrite omits or duplicates the original binding

The clarified invariant is that every pipe preserves its input type. For a scalar
wrapper, a single bound value can be forwarded unambiguously. Other cardinalities
do not specify which scalar value should be returned:

```yl
pipe twice() { rewrite value => value value }
pipe omit() { rewrite value => "," }
```

Returning the first or last match, a default value, or a tuple would either invent
selection behavior or violate the type rule. These cases return
`yl.parked_projection_cardinality`; scalar wrappers with one binding on every path
and repeated-list rewrites are implemented.

**Exact decision needed:** Are scalar rewrites required to use their binding exactly
once on every successful path, or is there another specified selection/default rule?
