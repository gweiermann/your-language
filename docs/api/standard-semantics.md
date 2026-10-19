# Authored semantic standard libraries

The canonical module signatures and behavior are maintained in the
[standard-library reference](/reference/standard-library). These are ordinary YL
libraries, not native reference or type adapters.

## Reference identities and scopes

See [`std/references`](/reference/standard-library#std-references) for
`createLexicalScope`, `declareName`, `connectReference`, resolution, and metadata.
The [course](/tutorials/course/04-references) introduces the mechanism before reuse.

## Type contributions and restrictions

See [`std/types`](/reference/standard-library#std-types) for annotations,
compatibility, restrictions, and isolated assumptions. The
[type model guide](/guide/type-models) distinguishes generic annotations from the
separate structural preset.

## Generic function relationships

Follow [function inference](/tutorials/course/06-functions) and the
[detailed inference walkthrough](/tutorials/std-types) for signature keys,
parameter-specific assumptions, and caller-linked diagnostics. Explicit generic
interfaces are covered in [compound data](/tutorials/course/07-compound-types).
