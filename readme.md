# Your Language

Your Language (YL) is a declarative language for defining programming-language syntax, AST structure, semantic roles, constraints, and diagnostics.

A YL definition is intended to be compiled into a reusable language artifact and executed by the Your Language runtime.

## Current design

The current syntax design is documented in [documentation/readme.md](./documentation/readme.md).

A larger example is available in [documentation/target-syntax](./documentation/target-syntax).

## Planned runtime model

```text
*.yl
  ↓
YL compiler
  ↓
*.ylc
  ↓
Your Language runtime
  ↓
AST + diagnostics
```

The first implementation is planned as a Rust compiler/runtime with a clean library API. Native parser source generation for other languages is a possible later extension.
