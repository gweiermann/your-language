# Modules and visibility

Each `.yl` file is a module. Modules separate reusable lexical definitions, expression grammars, statements, and entrypoint configuration while preserving a single resolved language definition.

## Private declarations and exports

Declarations are private by default. Prefix a declaration with `export` to make it accessible to other modules:

```yl
// lexical.yl
pattern IdentifierStart = /[_a-zA-Z]/

export node Name =
    value: /[_a-zA-Z][_a-zA-Z0-9]*/

export trivia Whitespace = /[ \t\r\n]+/
```

`Name` and `Whitespace` are importable; `IdentifierStart` remains an implementation detail of the lexical module. Patterns, pipes, enums, reusable constraints, and nodes use the same explicit visibility model.

## Named imports

Import only the declarations required by the current module:

```yl
import { Name, Whitespace } from "./lexical"

node Program = names: Name*

entry Program {
    trivia Whitespace
}
```

Relative paths resolve from the importing file. Local module paths may omit the `.yl` extension. Parent-directory paths can compose a language across directories:

```yl
import { Name } from "../lexical"
```

Built-in module names such as `core/parser` and `std/parser` are distinct from relative file paths.

Importing a private or nonexistent declaration is a definition error. Importing a trivia declaration does not activate automatic skipping; the [entry block](./trivia#entry-configuration) selects active trivia explicitly.

## Import aliases

Use `as` to choose a local name:

```yl
import { Call as FunctionCall } from "./calls"

node Statement = call: FunctionCall ";"
```

An alias refers to the existing declaration. It does not create a new AST node type or copy the grammar. The resulting AST retains the underlying concrete node's identity.

## Qualified members

Abstract node members have qualified names:

```yl
export node Expression {
    node Number = value: /[0-9]+/
    node NameExpression = name: /[a-z]+/
}
```

Inside the language, members may be addressed through `Expression::Number` and `Expression::NameExpression`. Membership paths resolve through aliases as well:

```yl
import { Expression as Expr } from "./expressions"

node NumericStatement = number: Expr::Number ";"
```

Visibility is explicit at each declaration. Exporting an outer node does not independently export every nested declaration. Its members remain addressable through an imported family's qualified membership paths, as above; directly importing a nested declaration is subject to that declaration's own export. Likewise, exporting a member does not export its parent.

## Forward references

Declarations are resolved as a graph rather than executed from top to bottom. A declaration can refer to another declaration written later in the same module:

```yl
node Pair = left: Name ":" right: Name
node Name = text: /[a-z]+/
entry Pair
```

This also enables ordinary recursive node grammars. Recursive operators should declare their [precedence](./precedence) explicitly.

An import cycle does not by itself determine grammar validity. The compiler collects modules before resolving declarations, and validates references and grammar recursion after collection.

## One entrypoint per language

The resolved language graph must contain exactly one `entry` declaration. Reusable modules should therefore export declarations without declaring an entrypoint. The entry module imports those declarations and chooses the start node and trivia policy.

```yl
import { Statement } from "./statements"
import { Whitespace, Comment } from "./lexical"

node Program = statements: Statement*

entry Program {
    trivia Whitespace, Comment
}
```

See [Nodes and AST values](./nodes) for node identity and [Trivia and entrypoints](./trivia) for explicit skipping behavior.
