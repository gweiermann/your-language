# 2. Compose modules and expressions

By the end of this lesson, the initializer after `=` can be a number, a string,
or an addition. We will also split the definition into smaller files. Continue
inside **`first-language/`**, with the files from lesson 1. Keep `yl.json`.

## Share rules between files

A **module** is a definition file. `export` makes a named rule available to
another module; `import { Name } from "./lexical"` brings it into that module.
`./` means relative to the importing file, and the `.yl` ending may be omitted.
These are definition imports: they do not add an `import` feature to your target
language. Exporting a rule also does not add a field to the AST; captures do that.

The new `lexical.yl` module groups small text rules: names, whitespace, and
keywords. A **pattern** is a reusable rule without an extra named node in the
AST. A pattern can receive parameters: `keyword(value)` recognizes the supplied
literal. Thus `keyword("let")` recognizes the word `let`.

Why not just use `"let"`? The literal alone can also match the beginning of
`letter`. `boundedBy(IdentifierPart)` checks that the word does not touch another
identifier character. `|>` is a **pipe**: it applies this check to the preceding
pattern and preserves its captured value. `std/parser` is the bundled parser
helper library; it needs no relative path or configuration.

Create this file before replacing the entry definition:

<p class="course-file" data-file="definition/lexical.yl"><strong>Create file:</strong> <code>definition/lexical.yl</code></p>

```yl file=definition/lexical.yl
import { boundedBy } from "std/parser"

pattern IdentifierPart = /[A-Za-z0-9_]/

export pattern keyword(value) = value |> boundedBy(IdentifierPart)

export node Name = value: /[A-Za-z_][A-Za-z0-9_]*/

export trivia Whitespace = /\s+/
```

## Offer several kinds of expression

An **expression** is a piece such as `40`, `"hello"`, or `40 + 2`. Unlike the
concrete nodes we have used so far, `Expression` below is a **family** of nodes:
it selects one of its listed members. This is how we express an alternative.
`Number` still captures digit text. `String` captures text between quotes; its
regular expression excludes a quote and a newline from the middle.

`Addition` has two captures, `left` and `right`, separated by a literal `+`.
Both captures refer to `Expression`, so an addition can contain another addition.
This is **recursion**: a rule refers back to a family that includes itself.
The parser supports this shape; you do not need to expand every possible length
of `1 + 2 + 3` yourself.

Create the expression module:

<p class="course-file" data-file="definition/expression.yl"><strong>Create file:</strong> <code>definition/expression.yl</code></p>

```yl file=definition/expression.yl
export node Number = value: /[0-9]+/

node String = value: /"[^"\n]*"/

node Addition = left: Expression "+" right: Expression

export node Expression {
    node Addition

    node Number

    node String
}
```

## Connect the new rules

The entry file imports the exported rules. Its only grammar changes are replacing
`"let"` with `keyword("let")`, and replacing the initializer's `Number` with
`Expression`. Replace the whole file so the old local definitions do not remain:

<p class="course-file" data-file="definition/language.yl"><strong>Replace file:</strong> <code>definition/language.yl</code></p>

```yl file=definition/language.yl
import { Name, Whitespace, keyword } from "./lexical"
import { Expression } from "./expression"

node LetDeclaration =
    keyword("let")
    name: Name
    "="
    initializer: Expression
    ";"

node Program = statements: LetDeclaration*

entry Program { trivia Whitespace }
```

<p class="course-file" data-file="programs/program.txt"><strong>Replace file:</strong> <code>programs/program.txt</code></p>

```text file=programs/program.txt
let count = 40 + 2;
let greeting = "hello" + " world";
```

Run these commands from `first-language/`:

```sh
yl project check programs/program.txt
yl compile definition/language.yl -o language.ylc
yl language language.ylc ast programs/program.txt
```

The check exits **0** without output. Compilation prints `Compiled language.ylc`.
The AST command exits **0** and prints JSON. Find the first declaration's
`initializer`: it contains an `Addition`, with separate `left` and `right`
expressions whose number values are `"40"` and `"2"`. A family introduces a
wrapper in the tree; follow its child to the selected concrete node.

 The grammar recognizes addition;
it does not calculate `42` or decide whether strings may be added. Those are
later semantic and backend choices.

One recursive operator can use the default left-associative level. Multiple
competing recursive operators require explicit [precedence](/reference/precedence).
`keyword("let")` applies the language's identifier boundary without embedding a
trailing space into its literal. `boundedBy` is an authored pipe built on core
lookaround; whitespace still comes from the entry's trivia selection.

Try changing `40 + 2` to `1 + 2 + 3` and inspect the AST again. At this single
operator level the left pair groups first: `(1 + 2) + 3`. Grouping is what
**associativity** describes. If we later add multiplication, **precedence** tells
the parser whether `1 + 2 * 3` groups the multiplication before the addition.
We are deliberately using only one operator here.

For a rejected example, change the first initializer to `40 + ;`. The check
exits **1**: the `+` needs a right expression. Restore `40 + 2` and check again.

From the repository root, `cargo run --example 03-mini-js` explores a larger
syntax-only checkpoint with calls, functions, comments, lists, and precedence.

Previous: [Syntax](./01-syntax).

Next: [Diagnostics](./03-diagnostics).
