# 1. Define syntax and a project

By the end of this lesson, you will turn a text file into a structured result,
then recognize two small variable declarations. You do not need to know what a
parser or an AST is. You need the [installed `yl` command](/guide/installation)
and a text editor.

## Two files, two different jobs

Suppose you want a language that accepts `42`. There are two things to write:

| File | What it contains |
| --- | --- |
| A **definition** ending in `.yl` | Rules describing what text is allowed. You write these rules in YL. |
| A **program** ending in `.txt` | Text written in the language you are defining: initially just `42`. |

The program is also called **source text**. A **grammar** is its set of accepted
text rules. A **parser** follows those rules to recognize the structure of the
source. Parsing `42` does not calculate anything or execute a program.

Create an empty folder and two subfolders:

```sh
mkdir first-language
cd first-language
mkdir definition
mkdir programs
```

**Working directory for every command in this lesson: `first-language/`.**
Create the files with your editor, using the paths printed above each code block.

## Recognize one number

A `node` names a piece of recognizable syntax. We choose the name `Number`.
`=` introduces its rule. `/[0-9]+/` is a **regular expression**, a compact text
pattern: `[0-9]` means one digit, and `+` means one or more digits.
`entry Number` tells the parser which rule to start with.

<p class="course-file" data-file="definition/language.yl"><strong>Create file:</strong> <code>definition/language.yl</code></p>

```yl file=definition/language.yl
node Number = /[0-9]+/
entry Number
```

For this first experiment, save exactly the two characters `42`, without a
newline after them. Extra characters matter to a parser; we will explicitly
allow whitespace later in this lesson.

<p class="course-file" data-file="programs/program.txt"><strong>Create file:</strong> <code>programs/program.txt</code></p>

```text file=programs/program.txt no-final-newline
42
```

Run:

```sh
yl compile definition/language.yl -o language.ylc
yl language language.ylc ast programs/program.txt
```

Compilation checks the definition and saves its reusable rules in `language.ylc`.
The first command prints `Compiled language.ylc`. The second reads that file,
parses the program, and prints this complete result:

```json
{
  "type": "Number",
  "fields": {},
  "span": {
    "file": "programs/program.txt",
    "start": 0,
    "end": 2
  }
}
```

<!-- course:checkpoint=number -->

This is an **abstract syntax tree**, abbreviated **AST**: structured data describing
what was recognized. Here it is a tree with just one named piece. JSON is the
notation used to print it: braces hold named properties.

- `type` says the parser recognized our `Number` node.
- `fields` is empty because we have not asked to save any part separately.
- `span` says where it came from. `start: 0` is the first byte in the source;
  `end: 2` is just after the two digits. The end is excluded. For these ASCII
  digits, each character occupies one byte.

The AST is not the runtime number `42`. It tells another tool where a number was
written. That tool might later evaluate it, highlight it, or compile it.

## See a rejected program

Temporarily replace the program with:

<p class="course-file" data-file="programs/program.txt"><strong>Replace file:</strong> <code>programs/program.txt</code></p>

```text file=programs/program.txt expect-error no-final-newline
hello
```

```sh
yl language language.ylc check programs/program.txt
```

The command exits with status **1** and writes this diagnostic to stderr:

```text
Error[parse.unexpected_token]: Expected /[0-9]+/
  --> programs/program.txt:1
  1 | hello
    | ^
```

<!-- course:checkpoint=rejected-number -->

The caret points at the first character that cannot start a number. The `.yl`
file is valid; the `.txt` program does not follow it. That distinction helps you
decide whether to edit the rules or the program. A successful check exits with
status **0**. Restore the program to `42` before continuing.

<p class="course-file" data-file="programs/program.txt"><strong>Replace file:</strong> <code>programs/program.txt</code></p>

```text file=programs/program.txt no-final-newline
42
```

## Save a piece as a field

Replace the definition with this small change:

<p class="course-file" data-file="definition/language.yl"><strong>Replace file:</strong> <code>definition/language.yl</code></p>

```yl file=definition/language.yl
node Number = value: /[0-9]+/
entry Number
```

`value:` is a **capture**: save the text matched by the following pattern under
the field name `value`. The colon belongs to the definition; it is not required
in the program. Re-run both compile and AST commands. The fields are now:

```json
{
  "value": "42"
}
```

<!-- course:checkpoint=captured-number -->

That is the `fields` portion of the result; its `Number` type and span stay the
same. The quotes around `"42"` show that this capture contains source text.

## Allow an ordinary line ending

Many editors automatically add a final newline when saving. A newline is source
text too: the original digit rule does not accept it. Instead of changing your
editor permanently, explicitly allow whitespace now.

**Trivia** is text the parser may skip between meaningful pieces. `/\s+/` means
one or more whitespace characters: spaces, tabs, or line endings. Naming it does
not activate it; the entry rule selects it in `{ trivia Whitespace }`.

<p class="course-file" data-file="definition/language.yl"><strong>Replace file:</strong> <code>definition/language.yl</code></p>

```yl file=definition/language.yl
node Number = value: /[0-9]+/
trivia Whitespace = /\s+/
entry Number { trivia Whitespace }
```

<p class="course-file" data-file="programs/program.txt"><strong>Replace file:</strong> <code>programs/program.txt</code></p>

```text file=programs/program.txt
42
```

Recompile and run the same AST command. It succeeds with or without a final
newline; the `Number` still captures `"42"`. This is the whitespace rule we keep
for the remaining lessons.

<!-- course:checkpoint=whitespace-number -->

## Recognize a declaration

Now aim for `let count = 42;`. A **sequence** matches its pieces in order:
`"let"`, a name, `"="`, a number, and `";"`. Quoted text in the definition is a
**literal**: it must appear exactly in the program, without those quote marks.
A rule can refer to another node by name, so `initializer: Number` saves a
nested number node instead of duplicating the digit pattern.

We also need a name rule. `[A-Za-z_]` accepts a letter or underscore first;
`[A-Za-z0-9_]*` allows zero or more following letters, digits, or underscores.
Here `*` means zero or more, while the earlier `+` required at least one.

Spaces and newlines are useful between those pieces. **Trivia** is text we choose
to skip between grammar items. `/\s+/` matches whitespace. Declaring it alone
is not enough: the `entry` block explicitly activates it.

Finally, `Program` captures a list of `LetDeclaration` nodes. In
`statements: LetDeclaration*`, the repetition means zero or more declarations.
An empty file is therefore allowed too.

Replace the whole definition with:

<p class="course-file" data-file="definition/language.yl"><strong>Replace file:</strong> <code>definition/language.yl</code></p>

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

Replace the program. You can now use normal line endings:

<p class="course-file" data-file="programs/program.txt"><strong>Replace file:</strong> <code>programs/program.txt</code></p>

```text file=programs/program.txt
let count = 42;
let total = 1;
```

Project commands compile the current entry definition automatically. The earlier
`language.ylc` is a separate saved snapshot; changing a `.yl` file does not update
that snapshot until you run `yl compile` again.

## Tell tools which files belong together

`yl.json` is a project configuration, not a YL grammar. `entry` names the
definition file. `files` selects programs: `*` matches names in one directory;
`**` also reaches subdirectories. Our selection includes `.txt` files under
`programs/` and leaves unrelated text files alone.

<p class="course-file" data-file="yl.json"><strong>Create file:</strong> <code>yl.json</code></p>

```json file=yl.json
{
  "entry": "./definition/language.yl",
  "files": ["programs/**/*.txt"]
}
```

```sh
yl project check .
yl project ast programs/program.txt
```

The check prints a `PASS` line for `programs/program.txt` and exits **0**. The AST
command now produces a `Program` whose `statements` field contains two declaration
nodes. The first declaration has a `name` node containing `"count"` and an
`initializer` node containing `"42"`. Punctuation such as `=` was matched but
was not captured as a field.

This structure is why an AST is a tree: a program contains declarations, and a
declaration contains its name and initializer. The parser knows structure; it
still has not created runtime variables or checked their types.

**Try it:** remove the final semicolon from one declaration and run the check.
It exits **1** and points where the grammar needs more text. Restore the semicolon
and run again; `PASS` returns. Save the working files for the next lesson.

Previous: [Course](./index).

Next: [Modules and expressions](./02-modules).
