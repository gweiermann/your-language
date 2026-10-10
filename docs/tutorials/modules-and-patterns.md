# Compose modules and patterns

This guide uses `examples/mini-js/lexical.yl` to build a reusable lexical module, then shows how its exports compose into the entry module.

## Define a keyword policy

```yl
import { boundedBy } from "std/parser"

pattern IdentifierPart = /[_a-zA-Z0-9]/

export pattern keyword(value) =
    value |> boundedBy(IdentifierPart)

export node Name =
    value: /[_a-zA-Z][_a-zA-Z0-9]*/
```

`boundedBy` rejects identifier-part matches immediately before and after the wrapped grammar. `keyword("let")` therefore recognizes `let` without accepting the prefix of `letter`. The helper is ordinary YL; the runtime has no built-in list of keywords.

`IdentifierPart` stays private. `keyword` and `Name` are explicitly exported so another module can import them. Resolution still includes private dependencies needed by the exported definitions.

## Import the public declarations

```yl
import { Name, keyword, Whitespace, Comment } from "./lexical"
```

Relative imports resolve against the importing module, with `.yl` added when omitted. Importing declarations does not re-export them and does not activate trivia.

## Reuse list syntax

```yl
import { separatedBy } from "std/parser"

node Parameter = name: Name
pattern Parameters =
    Parameter* |> separatedBy(",", trailing=.optional)
```

The pattern returns a list of `Parameter` nodes. The pipe consumes commas and permits a trailing comma without adding separators or an extra wrapper to that list. `.optional` resolves against the helper's `Trailing` enum parameter.

Use the pattern directly as a capture:

```yl
node Signature = "(" parameters: Parameters ")"
```

The `Signature` node receives a `parameters` list. See [grammar values](/reference/patterns) and [structural pipes](/reference/pipes) for the preservation rules.

## Keep trivia policy at the entry

```yl
entry Program {
    trivia Whitespace, Comment
}
```

The entry module chooses the active trivia explicitly. Selecting the abstract `Comment` matcher includes its alternatives. Exporting or importing comments alone never enables them.

Explore the complete definition in the [MiniJS walkthrough](/tutorials/mini-js).
