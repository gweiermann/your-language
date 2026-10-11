# Examples

The syntax examples contain complete language definitions and source files for the `yl` CLI. The experimental semantic example uses the Rust API.

| Example | Description | Guide |
| --- | --- | --- |
| [Getting started](./getting-started/) | Declarations, captures, explicit trivia, and AST generation | [First language](../docs/tutorials/first-language.md) |
| [Constraints](./constraints/) | Optional captures, warnings, presence checks, and structured diagnostics | [Syntax diagnostics](../docs/tutorials/diagnostics.md) |
| [Native semantics](./semantics/) | Lexical scopes, declarations, references, deferred bodies, and a Rust consumer | [Native semantic operations](../docs/api/native-semantics.md) |
| [MiniJS](./mini-js/) | Modular expression and statement grammar with precedence and list pipes | [MiniJS walkthrough](../docs/tutorials/mini-js.md) |

All commands are run from the repository root. Output `.ylc` files can be written to any existing directory.

The native semantic demo separates its [language modules](./semantics/definition/language.yl)
from [input programs](./semantics/programs/program.txt). Its Rust consumer loads
the definition directly in memory; the syntax examples use the CLI commands in
their linked guides.
