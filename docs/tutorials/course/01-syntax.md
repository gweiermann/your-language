# 1. Define syntax and a project

Create `definition/` and `programs/` under a new project directory. Save:

```json file=yl.json
{
  "entry": "./definition/language.yl",
  "files": ["programs/**/*.txt"]
}
```

```yl file=definition/language.yl
node Name = value: /[A-Za-z_][A-Za-z0-9_]*/

node Number = value: /[0-9]+/

node LetDeclaration =
    "let"
    name: Name
    "="
    initializer: Number
    ";"

node Program = statements: LetDeclaration*

trivia Whitespace = /\s+/

entry Program {
    trivia Whitespace
}
```

```text file=programs/program.txt
let count = 42;
let total = 1;
```

Run:

```sh
yl project check .
yl project ast programs/program.txt
yl compile definition/language.yl -o language.ylc
yl language language.ylc ast programs/program.txt
```

The project check reports PASS for the program. Both AST commands produce a `Program` with two
`LetDeclaration` nodes and named `name`/`initializer` fields. The persisted
artifact can parse without reading the definition files again.

`node` names AST structure; capture names create fields. `*` is a list, including
an empty one. Whitespace is declared separately and activated explicitly by
`entry`. A plain literal does not add a captured field. See [ASTs](/api/ast).

The repository companion is `cargo run --example 01-getting-started`, run from
the repository root. It checks its valid and intentional-error fixtures.

Previous: [Course](./index).

Next: [Modules and expressions](./02-modules).
