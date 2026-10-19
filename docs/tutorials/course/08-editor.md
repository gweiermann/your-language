# 8. Add editor roles beside the grammar

The editor can use the definition to underline errors, color syntax, and show
small examples. A highlighting **role** says what a piece represents (number,
variable, function); the editor theme chooses its actual color.

## Connect VS Code to the project

You need VS Code, the Your Language extension, and the installed `yl` executable.
Open the folder containing your copied project's `yl.json` with **File > Open
Folder**. Opening only a loose program file does not provide the project entry
or source-file patterns. Install the extension using [the editor setup guide](/guide/editor)
and set `yourLanguage.cliPath` to your executable if `yl` is not on PATH.
The extension starts a language server: a process that loads the definition and
answers editor requests such as diagnostics and highlighting.

Every checkpoint uses `.txt` programs and a `yl.json` project configuration.
After [installing the extension](/guide/editor), open that project in a trusted
workspace and open a matching program. The extension discovers the entry and
selects the program's diagnostics and highlighting without a global `.txt` association.

Keep visual roles on grammar patterns, so a captured name's role remains explicit:

<p class="course-file" data-file="definition/ide-example.yl"><strong>Standalone illustration (not a project replacement):</strong> <code>definition/ide-example.yl</code></p>

```yl verify=ide
node Name = value: /[A-Za-z_][A-Za-z0-9_]*/ {
    ide { highlightAs "variable" }
}

pattern FunctionName = Name {
    ide {
        example "identity"
        highlightAs "function"
    }
}

node Declaration = "function" name: FunctionName
trivia Whitespace = /\s+/
entry Declaration { trivia Whitespace }
```

Read the illustration from the entry upward. `Declaration` recognizes a function
keyword followed by a `name` capture. That capture uses `FunctionName`, a pattern
wrapping the ordinary `Name` node. Its `ide` block gives this particular use a
function role while ordinary names retain their variable role.

The contextual wrapper overrides `Name`'s category. The editor theme chooses
colors; the language chooses roles. `example` supplies a parser-validated preview.
Without one, the bounded generator attempts a valid small example. An optional
part is omitted; `*` and `+` each try one occurrence. Unsupported cases receive no hint.

The Mini TypeScript and structural checkpoints already use this pattern in their
lexical and expression files. Do not move captures into distant extension files
just to separate the highlighting. Short `ide` blocks belong beside the grammar;
substantial semantic behavior belongs in explicitly imported operations.

Run `cargo run --example 04-ide-metadata` from the repository root. Open its
programs to inspect contextual roles, then open a definition to use Format
Document and the example CodeLens. Formatting target programs is not supplied.
Open `programs/invalid-course.txt` from lesson 7 and confirm the incompatible
field is underlined. Change `42` to `"hello"`, save, and confirm the diagnostic
disappears; undo to retain the fixture. If colors or diagnostics are missing,
check `yourLanguage.cliPath`, the `files` patterns in `yl.json`, and the
**Output > Your Language** log before changing the grammar.

See [IDE metadata](/reference/ide) and [editor configuration](/guide/editor).

Previous: [Compound data](./07-compound-types).

Next: [Host backends](./09-backends).
