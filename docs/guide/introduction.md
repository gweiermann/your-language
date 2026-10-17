# Introduction

Your Language (YL) describes the syntax of another language. Its compiler resolves a graph of `.yl` modules and produces a reusable `.ylc` artifact. Its runtime executes that artifact against source text, producing an abstract syntax tree (AST) and structured diagnostics.

## The language pipeline

```text
YL modules → compiler → .ylc artifact → runtime + source → AST and diagnostics
```

A language definition contains grammar, AST identities, captures, trivia policy, and syntax-local checks. It can be used for a configuration format, a domain-specific expression language, or programming-language syntax. Parsing does not execute the resulting program.

## Grammar and tree structure

`node` declares an AST identity. A named capture creates an AST field:

```yl
node Assignment = name: /[A-Za-z]+/ "=" value: /[0-9]+/
entry Assignment
```

For `answer=42`, the node has `name` and `value` fields. The equals sign is matched without becoming a field. Patterns reuse grammar without creating an extra node. Abstract nodes select a concrete member without adding a wrapper.

## Explicit language policy

Declarations are private unless exported. Helpers and parser primitives must be imported. Trivia is defined separately from activation: the entry block names exactly which matchers are skipped. A pipe rewrites grammar while preserving its value type. Constraints inspect captured syntax and can emit errors, warnings, or help.

These rules keep grammar composition predictable: adding an import does not silently activate comments or whitespace, and adding a wrapper does not silently reshape the AST.

## Scope

The compiler and runtime provide syntax recognition, precedence, AST construction,
and syntax-local validation. Authored YL libraries add references, scopes, and
type inference over the [query engine](/api/declarative-queries). Generic
libraries remain target-neutral; explicit [standard presets](/reference/standard-library#opt-in-type-policies)
provide reusable arithmetic and structural type policies.

Parsing and static analysis do not execute target programs. [Backend hooks](/reference/backends)
connect checked syntax to a consumer that interprets, transpiles, or compiles it.
The host owns runtime values and control flow. Flow analysis, ownership, and
broader inference remain [roadmap](/roadmap) directions.

Continue with [installation](/guide/installation), the
[learning path](/tutorials/language-curriculum), the
[first-language tutorial](/tutorials/first-language), or the
[language reference](/reference/lexical-syntax).
