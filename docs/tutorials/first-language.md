# Build a first language

This tutorial defines a small declaration language, compiles it, and inspects the resulting tree. The complete files are in `examples/getting-started/`.

## Define names and numbers

Create `language.yl`:

```yl
node Name = value: /[A-Za-z_][A-Za-z_0-9]*/
node Number = value: /[0-9]+/
```

Each `node` becomes an AST object. The regex matches source text; `value:` retains that text as a field. Numeric text remains a string: parsing does not convert it to a target-language numeric type.

## Compose a declaration

```yl
node Declaration = "let" name: Name "=" initializer: Number
node Program = declarations: Declaration*
```

`Declaration` consumes the keyword, a name, an equals sign, and a number. Only `name` and `initializer` become fields. `*` allows zero or more declarations and produces a list.

This introductory grammar matches `"let"` literally. A production identifier-based language should define keyword boundaries; the [modular-language tutorial](/tutorials/modules-and-patterns) demonstrates that policy.

## Select trivia and the entry

```yl
trivia Whitespace = /[ \t\r\n]+/
entry Program {
    trivia Whitespace
}
```

The declaration defines the whitespace matcher. The entry block activates it between ordinary grammar elements. Without the selection, spaces are ordinary input and are not skipped.

## Compile and parse

Create `program.txt` containing:

```text
let answer = 42
let count = 7
```

For the checked-in files, run from the repository root:

```sh
yl check examples/getting-started/language.yl
yl compile examples/getting-started/language.yl -o getting-started.ylc
yl language getting-started.ylc ast examples/getting-started/program.txt
```

The JSON root has `type: "Program"`, a `fields.declarations` array, and a source span. Each declaration contains `Name` and `Number` nodes. Source spans use half-open UTF-8 byte offsets.

## Check instead of generating a tree

```sh
yl language getting-started.ylc check examples/getting-started/program.txt
```

A valid file produces no diagnostic output. Add `--json` to receive `[]`. Replace a number with an unexpected character to inspect a parse diagnostic.

Continue with [modules and patterns](/tutorials/modules-and-patterns), then [syntax diagnostics](/tutorials/diagnostics).
