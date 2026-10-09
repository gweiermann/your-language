# Your Language — syntax design

> **Status:** draft design. This document describes the currently agreed syntax. Some semantic details, especially scopes, relation binding, and explicit precedence configuration, are still under design.

See [target-syntax](./target-syntax) for a larger example.

## Modules

Declarations are private to a file by default.

Use `export` to expose declarations:

```yl
export node Identifier =
    value: /[_a-zA-Z]\w*/;
```

Import exported declarations with JavaScript-style imports:

```yl
import { Identifier, Expression } from "./syntax";
```

Re-export syntax may use:

```yl
export { Identifier, Expression };
```

## Nodes

A `node` defines syntax with AST identity.

```yl
export node Identifier =
    value: /[_a-zA-Z]\w*/;
```

Parsing:

```text
hello
```

produces an AST node conceptually similar to:

```json
{
  "type": "Identifier",
  "value": "hello"
}
```

Node references are written directly:

```yl
node VariableDeclaration =
    "let"
    name: Identifier
    "="
    initializer: Expression;
```

No function-call syntax is required for ordinary node references.

## Captures

Use `name: Pattern` to capture a parsed value into the resulting node:

```yl
node VariableDeclaration =
    keyword: ("let" | "const")
    name: Identifier
    ("=" initializer: Expression)?;
```

The colon is used for type/pattern binding throughout YL.

## Patterns

A `pattern` defines reusable grammar without introducing its own AST identity.

```yl
export pattern Parameters =
    Parameter*
    |> separatedBy(",", trailing=.optional);
```

It can be referenced like any other grammar construct:

```yl
node FunctionDeclaration =
    "function"
    name: Identifier
    "(" parameters: Parameters ")"
    body: Block;
```

## Grammar expressions

The core grammar operators are:

| Syntax | Meaning |
| --- | --- |
| `A B` | sequence |
| `A \| B` | choice |
| `A?` | optional |
| `A*` | zero or more |
| `A+` | one or more |
| `name: A` | capture |
| `A \|> pipe(...)` | transform a grammar expression |

Examples:

```yl
node Literal =
    String | Number | Boolean | Null;
```

```yl
node Parameter =
    name: Identifier
    ("=" default: Expression)?;
```

```yl
node Block =
    "{"
    statements: Statement*
    "}";
```

String literals and regular expressions are grammar primitives:

```yl
"function"
/[_a-zA-Z]\w*/
```

## Trivia and whitespace

Whitespace and comments are trivia and are normally accepted between grammar elements automatically.

For example:

```yl
node VariableDeclaration =
    "let" name: Identifier "=" initializer: Expression;
```

can match source with different spacing without encoding whitespace operators in the grammar itself.

Whitespace-sensitive behavior is expressed through constraints.

## Constraints

Validation logic is grouped under `constraints`.

A node may contain inline `when` constraints:

```yl
node Identifier =
    value: /\w+/
{
    constraints {
        when value.matches(/^\d/) {
            error("An identifier cannot start with a number")
        }
    }
}
```

`when` is only used inside constraint declarations or `constraints { ... }` blocks.

Reusable constraints use the `constraint` declaration:

```yl
constraint tight(left, right) {
    when whitespace.between(left, right) {
        error("Whitespace is not allowed here")
    }

    when comment.between(left, right) {
        error("Comments are not allowed here")
    }
}
```

They can be applied from a node:

```yl
node Update =
    operator: UpdateOperator
    argument: Identifier
{
    constraints {
        tight(operator, argument)
    }
}
```

Diagnostics may use:

```text
error(...)
warning(...)
help(...)
```

## Extending declarations across files

A node has one canonical grammar definition.

Additional constraints and semantic metadata can be added from another file with `extend node`:

```yl
// name.yl
export node Name =
    value: /\w+/;
```

```yl
// name-validation.yl
import { Name } from "./name";

extend node Name {
    constraints {
        when value.matches(/^\d/) {
            error("A name cannot start with a number")
        }
    }
}
```

The current design does not use `extend node` to modify the node's grammar.

## Pipes

Pipes transform structured grammar expressions.

Use `|>` to apply a pipe:

```yl
Parameter*
    |> separatedBy(",", trailing=.optional)
```

A pipe declaration uses structural `rewrite` patterns:

```yl
pipe wrapped(open, close) {
    rewrite pattern =>
        open pattern close
}
```

Rewrite patterns may destructure grammar operators:

```yl
pipe oneOrMoreSeparatedBy(separator) {
    rewrite item+ =>
        item (separator item)*
}
```

Pipes operate on the grammar structure rather than textual source syntax.

### `separatedBy`

The standard separated-list helper keeps cardinality on the input grammar expression:

```yl
Parameter* |> separatedBy(",")
Parameter+ |> separatedBy(",")
```

Trailing separators are configured as part of `separatedBy`:

```yl
Parameter* |> separatedBy(",", trailing=.optional)
Parameter+ |> separatedBy(",", trailing=.required)
```

The trailing mode is:

```yl
enum Trailing {
    none
    optional
    required
}
```

The default is `.none`.

For a zero-or-more list, `.required` means that a trailing separator is required when the list is non-empty. The empty list remains valid.

## Enums

Enum values can be written contextually:

```yl
.optional
.required
.none
```

or explicitly:

```yl
Trailing::optional
Trailing::required
Trailing::none
```

The shorthand form is used when the expected enum type is unambiguous.

## Function and argument syntax

Parameters use `:` for their type and `=` for defaults:

```yl
pipe example(
    mode: Mode = .default
) {
    ...
}
```

Calls use positional arguments or `=` for named arguments:

```yl
example(.default)
example(mode=.default)
```

The general forms are:

```text
foo(value)
foo(name=value)

parameter: Type
parameter: Type = default
```

## Expressions

Expression grammars use the `expression` declaration.

```yl
expression Expression {
    atom Number;
    atom Name;
    atom "(" Expression ")";

    postfix Member =
        "." property: Name;

    postfix Call =
        "("
        arguments: Expression* |> separatedBy(",", trailing=.optional)
        ")";

    prefix Unary =
        operator: ("+" | "-" | "!");

    infix Product =
        operator: ("*" | "/");

    infix Sum =
        operator: ("+" | "-");
}
```

Precedence is implicit by declaration order, from tighter binding to looser binding.

In the example above:

```text
postfix
prefix
Product
Sum
```

so:

```text
a + b * c
```

parses as:

```text
a + (b * c)
```

Infix operators are left-associative by default. Unusual associativity can be declared explicitly, for example:

```yl
infix Power right =
    operator: "**";
```

An explicit `precedence { ... }` form is planned for definitions that should not derive precedence from declaration order; its detailed syntax is still draft.

## Relations

Relations attach semantic roles to syntax nodes.

```yl
relation Variable on Name {
}
```

```yl
relation Function on Name {
}
```

Relations can be refined:

```yl
relation Variable on Name {
    relation Let {
        trait variable(mutable=true)
    }

    relation Const {
        trait variable(mutable=false)
    }
}
```

Refined relations are referenced with `::`:

```text
Variable::Let
Variable::Const
```

The full binding, scope, and resolution model is still being specified.

## Traits

Traits attach reusable semantic information or behavior.

Example usage:

```yl
relation Variable on Name {
    relation Let {
        trait variable(mutable=true)
    }
}
```

The full trait model is still being specified.

## Compilation model

The first implementation targets a compiled language artifact and a reusable runtime:

```text
*.yl
  ↓
YL compiler
  ↓
normalized internal representation
  ↓
*.ylc
```

At runtime:

```text
*.ylc + source
      ↓
Your Language runtime
      ↓
AST + diagnostics
```

The initial compiler/runtime implementation is planned in Rust. The runtime should expose a clean library API suitable for use from other host languages.

Generated native parser source for individual target languages is not part of the first implementation.
