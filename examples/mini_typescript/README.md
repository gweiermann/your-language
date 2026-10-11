# Mini TypeScript inference prototype (throwaway)

Question: can a native library infer useful types from retained syntax occurrences and lexical symbol identities without using meaning precedence to wait for known types?

The first slice answers yes for a small, monomorphic language. It is an experimental consumer, not a TypeScript implementation or a production type library. All inference state is local to the Program operation's `after` hook. The scheduler and bundled lexical resolution policy are unchanged.

```sh
cargo run --example mini_typescript
cargo run --example mini_typescript -- errors
cargo run --example mini_typescript -- --json
cargo test --test mini_typescript_prototype
```

The definition and readable programs are separate directories. The example compiles its source map directly in memory. Tests also serialize and reload the optional compiled artifact. Default output prints inferred bindings and original-source underlined diagnostics; `--json` returns the complete AST, occurrences, identity graph, type records and diagnostics. Errors exit 1.

## Implemented policies

- Decimal integer, double-quoted string and boolean literals widen to `number`, `string` and `boolean`. Literal values do not become singleton types.
- `let` declarations, parameters and named function declarations reuse the bundled lexical declaration records. References reuse the same records, so shadowed names cannot accidentally share a type variable.
- Each declaration and expression receives a fresh inference variable. References, initializers, returns, annotations and calls contribute equality constraints. Constraints are collected first, then unified; no type dependency is encoded in meaning precedence.
- Annotations accept only `number`, `string` or `boolean`, on lets, parameters and function returns. Parameters and returns can be omitted.
- Functions have fixed arity. Calls may have zero or multiple arguments; nested calls are supported. Inference is monomorphic across all call sites. An identity function used with both number and string arguments diagnoses a conflict.
- Function declarations are registered before lets, and function bodies are explicit deferred boundaries. Existing lexical scheduling therefore supports named recursion and forward mutually recursive functions. Constraints can refer to unknown function signatures; known return annotations can anchor recursive results. Unanchored recursion remains `unknown` without a warning.
- An occurs check rejects equations requiring infinite types, such as a parameter called with itself.
- Constraint traversal is postorder in the retained tree. Equations are solved in collection order. The first compatible equations determine an inferred type; a conflicting later annotation reports an error but does not replace that type. Successful components of a failed function equation are retained. Error output is best effort, and may display partially constrained or contradictory-to-annotation bindings. A future production checker would need explicit error types, annotation priority, transactional unification and better related locations.
- Semantic mismatch diagnostics preserve the source extent of the equation (annotation, initializer, reference or call). Lexical duplicate/unresolved diagnostics retain the bundled library's source spans and secondary origins. Syntax errors suppress all native effects.

Types are stored under the identity metadata key `prototype/types#inferredType` on lexical declaration records. A root `prototype/types#expressionTypes` attachment points to a report whose text keys are occurrence IDs. Those IDs link back to retained occurrences and their source spans in structured output. `unknown` means an unconstrained variable, not an `any` escape hatch. The report also exposes unconstrained grammar occurrences; it is intentionally raw prototype state.

## Public seam demonstrated

`OperationContext::attachment(occurrence, key)` reads the latest published attachment, or returns absent. This lets an independent native library consume declaration/reference identities without inspecting lexical current-scope state, copying name resolution, or sharing factory-owned mutable state. IDs and values remain local to one analysis. It adds no ordering guarantee: consumers still choose an appropriate lifecycle stage. A second audit library test reads another occurrence's declaration and writes its own typed-key metadata.

The solver deliberately knows this example grammar's node roles. This coupling lives only in `examples/mini_typescript/inference.rs`; generic semantic core contains no target-language names. This broad collection hook is sufficient for the first experiment. More reusable type operations would need their own explicit contract design.

## Gaps and provisional alternatives

No assignments, operators, object/array types, generics, overloads, unions, subtyping, literal narrowing, guards, ranges, control-flow analysis, implicit returns, multiple return paths, function expressions, nested function declarations, target-language modules, ownership or code generation. There are no comments or escapes in this small grammar. Keyword boundary behavior is a grammar limitation. Static lexical visibility does not assess execution-time initialization safety. This language accepts only lets and named functions at top level, and lets followed by one return in function bodies.

The grammar-following `meanings { boundary lexicalScope() group Register { declare(...) } precedence { ... } }` contract remains the approved foundation. `solve()` is an experimental library export imported through an ordinary YL module; it introduces no new YL syntax.

Possible follow-up experiments, **not approved syntax or implemented behavior**:

1. Replace grammar-specific root collection with native `literal`, `referenceType`, `bindingType`, `functionType` and `callType` operations that publish constraint records; a root solver reads them after traversal. This tests reusable composition at the cost of more library contracts.
2. Pass imported type-identity symbols to annotation operations rather than recognizing annotation text inside the solver. This separates target spelling from type identity.
3. Represent annotation checking as directional assignability rather than equality when unions/subtyping become a real requirement. This requires a new solver policy, not a precedence rule.

Keep this branch separate while assessing the policies. The supported question is feasibility of native constraint collection over public identity records, not full TypeScript compatibility or a claim of optimal inference.
