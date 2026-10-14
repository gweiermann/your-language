# Semantic traversal contract

A standalone `boundary` inside `semantics` marks its node/pattern structurally. Search all depths within the current owner's region, include a nested boundary itself, stop below it. The owner boundary does not stop its own interior search. Boundaries are independent of native context-provider operations and are known before hooks.

See the [current semantic syntax](../../docs/reference/meaning-libraries.md) for declarations and `Node::semantics::Group` selectors.

Bare selectors defer whole matched constructs. An ancestor's boundary-entry fence holds all contents; local ordering applies after entry and cannot exempt descendants from that ancestor requirement. Non-boundary constructs retain explicit earlier-subconstruct exceptions. Named-group selectors order only own selected operations. Local/enclosing chains accumulate; absent intermediate occurrences preserve transitivity; contradictory instance dependencies diagnose cycles before any hooks.

Ordinary semantic groups use source-order DFS completion. Context providers initialize before descendants and reactivate around work; cleanup waits deferred work, with before/after once and balanced enter/leave activations. Patterns retain source identities without AST wrappers. Pipe projection preserves original values, spans, and occurrences rather than wrapper/separator actions.

This replaces both global unrestricted descendant selection and the uncommitted BFS-first-depth experiment. No generic runtime node names or implicit lexical-library boundary detection. Regression coverage is tests/semantic_review.rs; public consumer is examples/semantic_analysis.rs.
