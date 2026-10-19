---
layout: home
hero:
  name: Your Language
  text: Build languages that compose.
  tagline: Define syntax, author references and types, deliver diagnostics and editor support, and choose how programs execute.
  actions:
    - theme: brand
      text: Build a language
      link: /tutorials/course/
    - theme: alt
      text: Explore the capabilities
      link: /guide/capabilities
features:
  - title: Grammar and AST together
    details: Named captures, patterns, structural pipes, modules, trivia, and precedence define what your language accepts and what its tree contains.
  - title: Meaning written in YL
    details: Compose lexical references, type inference, isolated function calls, and structural generic types through authored operations.
  - title: Diagnostics with context
    details: Connect bad values, required annotations, and optional source origins in one grouped diagnostic for terminals and editors.
  - title: Your runtime, your target
    details: Host hooks receive unevaluated syntax. Interpret values, generate JavaScript, or emit WebAssembly using the same loaded language.
  - title: Tools beside the grammar
    details: IDE roles and parser-validated examples live on nodes and patterns. A project configuration activates program diagnostics and highlighting.
  - title: Reusable language artifacts
    details: Compile a definition once, then load its validated .ylc through the CLI or Rust library without its original source files.
---

## Start with a small language. Keep building.

The [course](/tutorials/course/) grows a TypeScript-like teaching language from
its first declaration through expressions, diagnostics, references, types,
function inference, compound data, editor support, and host backends. Every step
has runnable files or an explicit checked-in checkpoint. Advanced data and runtime
chapters name their subset boundaries rather than claiming complete TypeScript.

```sh
cargo run --example 08-mini-typescript
cargo run --example mini_typescript_host -- addition
cargo run --example backend_interpret
```

The inference host reports `Type::number` and `Type::string` for the corresponding
bindings; the interpreter prints `42`. Follow [installation](/guide/installation)
to run the CLI and [editor setup](/guide/editor) to inspect live source diagnostics.

## Choose the level you need

Use [core queries](/api/core) when defining new analysis mechanisms. Reuse
[standard references and types](/reference/standard-library) for common behavior,
or explicitly opt into arithmetic and structural policies. Hook contracts and
host implementations remain separate, so static type information does not impose
a runtime value representation.

Browse [concepts](/guide/syntax-and-values), the
[language reference](/reference/lexical-syntax), and
[capabilities and limits](/guide/capabilities) for the current contract.
