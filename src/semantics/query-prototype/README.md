# Declarative query prototype

Experimental, throwaway model for the question: can tracked contributions and composed queries express scopes, references, and dependent types without a mutable current-scope stack?

This folder is a design experiment beside the semantic engine. It does not change the Rust runtime, add supported YL syntax, or establish a public API. The HTML shell should stay on the experiment branch rather than become production tooling.

## Run

Open `index.html` directly in a browser. No server, network, dependencies, or persisted state are required. Use the guided scenario tabs or free-play controls to inspect pending queries, contributions, and diagnostics.

The executable checks use Node.js:

```powershell
node src/semantics/query-prototype/checks.cjs
node src/semantics/query-prototype/check-html.cjs
```

The user explicitly requested capability tests; these checks are intentional despite the prototype skill's usual recommendation to omit tests.

## Proposed query shape

```yl
find(ancestors(self), ancestor =>
    get((get((ancestor, scope)), declarations, text(name)))
)
```

Each nearest-first callback returns a present value, definite absence, or pending. Pending is not absence: a potential inner declaration prevents premature resolution to an outer declaration. Tuple keys are structural; `get` is separate from constructing a key.

Conditional contributions retain their producer identity. Reevaluation updates the producer's contribution rather than duplicating it. A false condition rules that contribution out; an unfinished condition remains possible. Diagnostics are also tracked and can disappear when their supporting condition changes.

## Boundaries of the experiment

- Producer discovery and contribution footprints are explicitly supplied. This does not implement static inference of producer regions from YL.
- Callbacks are assumed pure and values are immutable, JSON-friendly data. Structural keys are stringified in this small model; production tuple interning/hashing is not implemented.
- Reevaluation rescans every producer. Recorded dependencies explain waiting, but selective invalidation and parallel scheduling are not implemented.
- Recompute has a bounded round count; it does not diagnose arbitrary oscillating producer functions. Producers are expected to follow the modeled pure query contract.
- Final-only queries intentionally wait for their relevant producers. They cannot resolve positive recursive inference cycles by themselves.
- A separate monotone fixed-point experiment explores accumulating positive facts. It does not make absence-dependent cycles safe.
- Hypothetical assumptions use an isolated evaluation overlay. This is not an implementation of arbitrary YL replay, foreign effects, or the complete native type library.
- This prototype does not settle overwrite policy, source-order visibility, or generalized library compatibility.

The findings favor pure composed queries and tracked contributions, but require complete producer discovery and a distinction between final-value lookup and monotone fact propagation before integration.

## Verification

The saved-source checks pass 35 scenario assertions and all 24 permutations of the four declaration producers. The permutation check establishes order independence for that fixture, not for every possible producer graph.

The checks deliberately demonstrate failure cases too: incomplete producer discovery chooses the wrong outer binding, and a final-only cyclic lookup remains pending even with a seed. Those assertions document limitations rather than claim the cases are solved.

No browser surface was available to this agent, so the page has not been visually verified.
The separate HTML check executes the actual inline script and all six guided walkthroughs, free-play buttons, and reset handlers using a minimal DOM model. It verifies wiring, not rendered layout or accessibility.
