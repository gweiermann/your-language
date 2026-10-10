# Trivia and entrypoints

Trivia is source text that participates in matching and validation without becoming ordinary AST content. Typical examples are whitespace and comments.

Trivia declarations define matchers. Only the entrypoint configuration activates automatic skipping.

## Concrete trivia

Declare a matcher with `trivia`:

```yl
trivia Whitespace = /[ \t\r\n]+/
```

Trivia may use ordinary grammar rather than only a regex:

```yl
trivia LineComment = "//" /[^\n]*/
```

Export trivia when another module must select it:

```yl
export trivia Whitespace = /[ \t\r\n]+/
```

## Abstract trivia families

Group alternatives in a family:

```yl
export trivia Comment {
    trivia Line = /\/\/[^\n]*/
    trivia Block = /\/\*[\s\S]*?\*\//
}
```

`Comment` recognizes either member. Qualified paths such as `Comment::Line` and `Comment::Block` identify particular kinds of trivia for syntax-local checks.

Selecting the abstract family activates its member matcher as a whole; it is unnecessary to list each member in the entry configuration.

## Entry configuration

The entrypoint names the start node and explicitly selects trivia:

```yl
import { Name, Whitespace, Comment } from "./lexical"

node Program = names: Name*

entry Program {
    trivia Whitespace, Comment
}
```

Only the listed trivia is skipped automatically between ordinary grammar elements. Importing a module that declares trivia does not activate its matchers. Importing a trivia declaration without selecting it also does not activate it.

Selection follows the written list order. Repeated references to the same matcher are idempotent. Each selected name must resolve through the ordinary module/export rules and must identify a trivia declaration.

## Languages without automatic trivia

An entrypoint without a trivia section selects none:

```yl
node Pair = "a" "b"
entry Pair
```

This recognizes `ab`, but does not automatically accept `a b`. An empty entry block has the same trivia policy:

```yl
entry Pair {}
```

Match meaningful whitespace explicitly in ordinary grammar when the language requires it. Automatic skipping should only apply to text that the language considers ignorable between grammar elements.

## Matching trivia itself

Automatic trivia skipping is disabled while a trivia matcher is running. A comment grammar therefore consumes exactly its own content instead of recursively skipping whitespace or comments within it.

Trivia matchers should consume text on success. A matcher that accepts empty text cannot advance automatic skipping.

## Trivia constraints

Automatic skipping does not erase trivia information. Constraints can inspect trivia between captured source regions:

```yl
node Update = operator: ("++" | "--") argument: Name {
    constraints {
        when trivia.between(operator, argument) {
            error("The operator and argument must be adjacent.")
        }
    }
}
```

`trivia.between(left, right)` checks for any skipped trivia between the two captures. A named trivia declaration restricts the query:

```yl
when Comment.between(operator, argument) {
    warning("A comment separates the operator from its argument.")
}
```

Qualified trivia members can make the restriction more specific. The queried trivia name must be visible through the normal resolver. See [Constraints](./constraints#trivia-between-captures) for reusable adjacency checks.

## Parsing begins at the entry node

The language graph contains exactly one `entry`. Normal source parsing starts at that node and must account for the complete input, allowing selected trivia around ordinary matching. Unconsumed non-trivia source produces a diagnostic rather than an accepted partial parse.

The compiled artifact contains both the entry rule and its selected trivia policy. Loading it does not require the original YL modules.
