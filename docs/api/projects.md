# Projects

A `yl.json` file is the shared project description for the CLI and editor. Paths are relative to that file:

```json
{
  "entry": "definition/language.yl",
  "files": ["programs/**/*.txt"],
  "expectErrors": ["programs/errors/**/*.txt"]
}
```

`entry` selects the language definition. `files` selects programs written in that language. `expectErrors` is optional and defaults to an empty list; it marks negative fixtures for batch checks. The editor still displays their diagnostics normally.

Globs support `*`, `**`, and `?`, with `/` separators. Parent traversal and absolute paths are rejected. Source ownership belongs to the nearest `yl.json`, including when that configuration is invalid. Nested projects therefore do not run twice in a parent's batch.

## Discover projects

```sh
yl project list examples
yl project list examples --json
```

Discovery recursively lists configurations and their matching programs in path order. It skips symlinks and `.git`, `target`, and `node_modules` directories. JSON output is an array of `{config, entry, sources}` objects, with each source containing `path` and `expectErrors`.

## Check an entire project

```sh
yl project check examples/01-getting-started
yl project check examples/01-getting-started/yl.json --json
```

The CLI compiles the configured definition once and analyzes each matching program. A project succeeds when ordinary programs have no errors and every negative fixture has at least one error. Warnings do not count as errors. Definition compilation failures, unreadable programs, missing expected errors, and projects without matching programs fail the command.

Human output preserves source underlines and prints a pass/fail line per program. JSON output is one `{config, success, sources}` report; each source has `path`, `expectErrors`, `success`, and its structured `diagnostics`. Successful negative fixtures retain their error diagnostics in the report.

## Inspect one program

```sh
yl project check examples/01-getting-started/programs/program.txt
yl project check examples/01-getting-started/programs/program.txt --json
yl project ast examples/01-getting-started/programs/program.txt
```

An individual check always reflects that program's actual diagnostics, independent of fixture expectations. Its JSON output remains the diagnostic array. AST output is JSON and requires an explicit source file. These commands use the same compilation and analysis path as batch checking.

## Cargo examples and Rust consumers

Every numbered example also has a Cargo entrypoint and an equivalent friendly name:

```sh
cargo run --example 01-getting-started
cargo run --example getting-started
```

Both thin launchers pass their `yl.json` to the same in-process project runner
used by the CLI. They print original diagnostics, each fixture's result, and a
summary; expected negative fixtures do not make the example fail. The complete
[example catalog](https://github.com/gweiermann/your-language/tree/main/examples) lists both names for every project.

Rust consumers can inspect the report without printing or invoking a subprocess:

```rust
use your_language::project::Project;
use std::path::Path;

let project = Project::load(Path::new("examples/01-getting-started/yl.json"))?;
let report = project.check()?;
assert!(report.success);
```

`Project::compile()` and `Project::analyze_file()` also support compiling once
and inspecting individual programs through the same analysis pipeline. None of
these checks invoke runtime backend hooks; the backend host examples demonstrate
execution separately.
