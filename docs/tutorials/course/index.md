# Build a TypeScript-like language

Build one small language step by step: first recognize declarations, then add
expressions, modules, diagnostics, references, type rules, and function inference.
Start with visible mechanisms, then replace local implementations with reusable
standard-library operations. Each chapter identifies the files to create or replace.

This course is a teaching subset, not the TypeScript specification. The structural
data and backend chapters are explicit variants: the current repository does not
combine every demonstrated feature into one complete TypeScript compiler.

| Chapter | What it adds | Runnable repository checkpoint |
| --- | --- | --- |
| [1. Syntax and project](./01-syntax) | Captures, repetition, trivia, AST and `yl.json` | 01-getting-started |
| [2. Modules and expressions](./02-modules) | Imports, reusable keyword boundaries, recursive addition | 03-mini-js |
| [3. Diagnostics](./03-diagnostics) | Checks beside grammar and intentional errors | 02-constraints |
| [4. References](./04-references) | Local query-based scope/name operations | 07-declarative-queries, then 06-references |
| [5. Types](./05-types) | Language-owned types, propagation, restrictions | 08-mini-typescript |
| [6. Functions](./06-functions) | Function signatures and independent call inference | 08-mini-typescript, 09-type-instantiation, 10-declarative-functions |
| [7. Compound data](./07-compound-types) | Interface/list/tuple policies and explicit generic arguments | 11-compound-types, 12-diagnostic-origins |
| [8. Editor](./08-editor) | Contextual token roles and example previews | 04-ide-metadata; all projects |
| [9. Host backends](./09-backends) | Interpret, transpile, and emit Wasm from checked syntax | 14-backend-hooks |

Run shell commands from your project directory unless a chapter explicitly says
repository root. Install the CLI using [installation](/guide/installation).
The snippets marked with a filename are complete files. Later chapters replace
only the named files; other files remain. All such files and their checks are
executed by `npm run docs:check-course` in repository validation.

The numbered repository examples are checkpoints and focused companions, not
nine copies of the same project. [05-tuple-access](/api/declarative-queries#positional-access)
and 13-formatting are supporting exercises; both retain their common runners.

Next: [Create the project](./01-syntax).
