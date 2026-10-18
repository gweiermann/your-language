# Lexical syntax

Your Language definitions are UTF-8 text files with the `.yl` extension. They describe the syntax of another language: declarations compose matchers, construct AST nodes, and attach syntax-local checks.

## Identifiers

An identifier begins with an ASCII letter or underscore and continues with ASCII letters, digits, or underscores:

```yl
node Identifier = text: /[_a-zA-Z][_a-zA-Z0-9]*/
pattern identifierList = Identifier*
```

YL declaration names and target-language identifiers are separate concepts. A grammar may recognize Unicode identifiers even though the names used to define that grammar follow YL's identifier rules.

Keywords are contextual where possible. A capture can use a word that also introduces a declaration:

```yl
node Wrapper = node: Identifier
```

Use `::` for qualified names, including nested node members and explicit enum variants:

```yl
Expression::Number
Trailing::optional
```

## Whitespace and declaration boundaries

Spaces, tabs, and line breaks separate tokens in YL definitions. Indentation is for readability; it does not establish scope. Braces delimit declaration bodies and parentheses group expressions.

YL does not use statement semicolons. Declaration boundaries follow the language grammar:

```yl
node Name = value: /[a-z]+/
node Pair = left: Name ":" right: Name
entry Pair
```

Whitespace in the definition is different from whitespace in the language being defined. Target-source whitespace is only skipped when selected explicitly as [entrypoint trivia](./trivia).

## Comments

Definitions support line comments and block comments:

```yl
// A name consists of lowercase letters.
node Name = value: /[a-z]+/

/* This node recognizes a parenthesized name. */
node Group = "(" name: Name ")"
```

Block comments are delimited by `/*` and `*/`. An unterminated comment is a definition error.

Comments in YL files do not configure comments in target source. Declare target comments using `trivia` or ordinary grammar.

### Documentation comments

Prefer `/** ... */` to document a declaration:

```yl
/**
 * Matches a decimal integer.
 *
 * The captured text is retained in the AST.
 */
export node Number = value: /[0-9]+/
```

Line documentation using `///` is also supported:

```yl
/// Matches a decimal integer.
export pattern Integer = /[0-9]+/
```

Documentation attaches to the following declaration, including local or nested
nodes, patterns, operations, hooks, symbols, and enums. A block can appear on the
same line before its declaration (`/** A key. */ export symbol key`) or on the
preceding line. Each documentation comment must start on its own source line,
with only indentation before its opening delimiter. Consecutive block and line
documentation comments are combined in source order with a newline between them.

The frontend exposes `Declaration.documentation` with its normalized text and
original span, including the delimiters. Block text loses its opening and closing
delimiters, conventional leading `*` margins and one following space, common
indentation, trailing line whitespace, and blank opening/closing lines. Interior
paragraph breaks and relative indentation are retained. `/**/` and `/***/` are
empty documentation blocks. Line text loses its `///` prefix and at most one
following space; remaining text is preserved. Use an empty `///` line or a blank
line inside a block for a paragraph.

A blank source line between comments or before the declaration, an ordinary
comment, or another token breaks attachment. Trailing inline and dangling
documentation comments remain ordinary unattached comments. They do not cause
errors or attach to an earlier declaration. Ordinary `/* ... */` comments do not
provide declaration documentation. An unterminated block of either kind reports
`yl.unterminated_comment`.

The formatter preserves original documentation spelling, style, text, and
attachment. The language server shows documentation when hovering the
declaration's name in a YL definition. Hovering references or imported names is
not supported yet. Documentation is frontend authoring metadata, not a target AST
field, parser rule, or `.ylc` execution instruction; existing artifact versions
remain unchanged.

## String literals

Double-quoted strings match exact target text:

```yl
node Assignment = name: /[a-z]+/ "=" value: /[0-9]+/
```

The supported escapes are `\n`, `\r`, `\t`, `\"`, and `\\`. Unknown escapes are definition errors.

```yl
pattern QuotedName = "\"" /[a-z]+/ "\""
pattern Newline = "\n"
```

A literal produces its matched text as a grammar value. It only becomes an AST field when captured inside a concrete node.

## Regular expressions

Slash-delimited regular expressions match target text at the current parsing position:

```yl
node Integer = digits: /[0-9]+/
node Identifier = text: /[_a-zA-Z][_a-zA-Z0-9]*/
```

Escape a slash inside the expression as `\/`:

```yl
trivia LineComment = /\/\/[^\n]*/
```

Regex syntax is the syntax accepted by Rust's `regex` engine. Character classes, alternation, Unicode classes, and greedy or lazy repetition are available. Backreferences and regex lookaround are not supported by that engine; use the [core parser primitives](./standard-library#core-parser) for grammar-level negative lookaround.

Regex matching starts at the parser's current position, rather than searching ahead for a match. Constraint `.matches(...)` performs a predicate check on an existing capture; use `^` and `$` when that check must cover the entire captured string.

## Source locations

The frontend preserves the source span of declarations, expressions, and arguments. All spans use half-open UTF-8 byte offsets: `start` is included and `end` is excluded. Target AST nodes and diagnostics use the same span convention, referring to the target source rather than the definition file.

These byte offsets are suitable for slicing UTF-8 source. They are not terminal display columns or character indices.
