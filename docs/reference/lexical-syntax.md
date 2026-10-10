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
