---
layout: home

hero:
  name: Your Language
  text: Syntax with structure.
  tagline: Define grammars, shape syntax trees, and report precise diagnostics from one declarative language.
  actions:
    - theme: brand
      text: Build a language
      link: /tutorials/first-language
    - theme: alt
      text: Read the reference
      link: /reference/lexical-syntax

features:
  - title: Grammar and AST together
    details: Concrete nodes give syntax identity. Named captures define fields. Reusable patterns remain transparent in the tree.
  - title: Portable language artifacts
    details: Compile modules into a normalized .ylc artifact and parse source without the original grammar files.
  - title: Diagnostics at the source
    details: Preserve source spans, validate local syntax, and consume diagnostics through a terminal or structured JSON.
  - title: Composable language definitions
    details: Explicit imports, exports, trivia selection, structural pipes, and precedence declarations make language policy visible.
---

## One definition, two tools

```sh
yl compile examples/mini-js/minijs.yl -o minijs.ylc
yl language minijs.ylc ast examples/mini-js/program.js
yl language minijs.ylc check examples/mini-js/invalid.js
```

Use the [CLI](/api/cli) to generate ASTs or check syntax, or embed the [Rust API](/api/rust) in an application. Begin with [installation](/guide/installation), then follow a [tutorial](/tutorials/first-language).
