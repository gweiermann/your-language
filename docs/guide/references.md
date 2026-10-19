# References and lexical scopes

Name lookup has two questions: which scope contains a name, and which declaration
is visible there? A scope is attached to syntax, not held in a mutable global stack.
Nested scopes search outward; two declarations with the same spelling retain distinct identities.

`createLexicalScope(self)` marks a scope. `declareName(self, name)` registers a
declaration. `connectReference(self, name)` connects a use and returns the resolved
identity, reporting an unknown name when needed. All three are authored operations
in `std/references`, built from core queries.

Declaration visibility follows the semantic schedule. Source order is the default;
explicit groups, precedence, and boundaries can register functions before their
uses while deferring bodies. A pending inner declaration does not accidentally
fall through to an outer declaration. This is static analysis, not a runtime scope map.

The [course chapter](/tutorials/course/04-references) builds a small local
implementation before switching to the standard library. Consult the
[standard-library signatures](/reference/standard-library#std-references) for
parameters, exported metadata, and ordering behavior. Runtime scope policy belongs
to your [host backend](/guide/host-hooks).
