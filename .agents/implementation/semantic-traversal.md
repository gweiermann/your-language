# Semantic traversal contract

Bare precedence selectors defer complete matched node/pattern traversal, including descendant meanings. Named-group selectors order only own operations. Enclosing and local strict chains accumulate; explicitly earlier descendants and their initializer subtrees can proceed. Chains remain transitive with absent intermediate occurrences; contradictions diagnose cycles before hooks.

Ordinary meanings use source-order DFS completion. Context providers initialize before descendants and reactivate around work; lifecycle cleanup preserves state across suspension. Patterns retain occurrence identities without AST wrappers. Pipe projections retain original value provenance and occurrences, discarding wrapper/separator actions.

Use role patterns to select top-level declarations without selecting nested local declarations. A broad descendant selector intentionally includes every matching role occurrence. Regression coverage lives in tests/semantic_review.rs; the runnable consumer is examples/semantic_analysis.rs.
