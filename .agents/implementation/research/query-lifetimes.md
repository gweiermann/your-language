# Query caching and lifetime analysis

Research baseline: `b02e783dadad8cce7787df5914fc1ce186fbbbc5`, 18 October 2026. This note separates observed engine behavior, recommendations, and measurements. Proposed syntax is not part of the language contract.

## Conclusions

Compile-time analysis can determine **families of possible reads and writes** and eliminate incompatible dependencies. Runtime analysis must specialize those families to the parsed occurrences, concrete keys, selected guards, and hypothetical assumptions. A reusable language artifact cannot know the number or identity of occurrences in every future input program.

Two proofs have different purposes:

- **Finality:** no possible producer can change the observed answer. A final answer can be reused without re-evaluation in the same context.
- **Last use:** no future consumer or exported result can require the retained data. Only then can its final required copy be discarded.

A finalized answer is not necessarily dead. A cache entry can be evicted even while its answer remains live, provided re-evaluation remains possible. An authored fact cannot simply be evicted if doing so changes subsequent lookup results.

## Observed engine behavior

These observations describe the baseline; implementation changes should update their status rather than treating them as permanent limitations.

| Mechanism | Baseline behavior | Consequence |
| --- | --- | --- |
| Producer discovery | `Engine::new` discovers effects throughout the committed occurrence forest, including conditional branches, before lookup; `discover_expression` handles effectful invocation blocks. | An untaken or pending branch cannot be omitted from the possible-writer set just because it has not produced a value. |
| Key footprints | `Pattern` supports exact keys, tuple components, unions, empty, and unconstrained patterns; `infer_footprints` specializes producers. | Structured keys already permit conservative candidate filtering. Unconstrained components must retain all compatible candidates. |
| Lookup | `lookup_visible` scans contribution producers, records keys/candidates/waits, and considers pending compatible producers. `getEarlier` also filters by logical producer order. | A memo key must distinguish lookup mode and order context. A first successful lookup alone does not prove finality. |
| Partial facts | `facts` returns currently available distinct values and marks the trace approximate. | An empty or nonempty partial result may change; it cannot be treated like a finalized `getAll`. |
| Function memoization | `eval` clears function memoization at top-level evaluation entry. Recursive invocations use a serialized key containing function, owner, order, branch values, and arguments. Allocation accounting limits the computation to 16 MiB. | Persistence across evaluations needs invalidation. Existing accounting includes cloned values/traces and is not a direct measurement of live heap bytes. |
| Hypothetical evaluation | Copies producers with overrides; the memo table is shared between copies. Its key includes query/mode/assumptions/origins. Ready results retain shared reads and diagnostics; the baseline table has no eviction budget. | Bound retained memo memory and preserve context-sensitive diagnostics/dependencies. Reusing a result must account for changed shared inputs. |
| Public output | `execute` exports contribution values into `SemanticGraph.query_entries` and dependency reads/waits into `query_dependencies`. | Those values and dependencies are output roots, even when the engine has no internal readers left. |

Primary implementation sources: [`Engine`, `Pattern`, discovery, lookup, memoization, hypothetical evaluation, and result export](https://github.com/gweiermann/your-language/blob/b02e783dadad8cce7787df5914fc1ce186fbbbc5/src/semantics/queries.rs), [indexed query instructions](https://github.com/gweiermann/your-language/blob/b02e783dadad8cce7787df5914fc1ce186fbbbc5/src/image/query.rs), and [public query result contract](../../../docs/api/declarative-queries.md#rust-and-compiled-artifacts).

**Memory lower bound:** retaining the current public result requires memory proportional to the exported keys, values, and dependency data. Moving a fact from a producer to an output archive can reclaim a duplicate; it cannot erase the required output. Returning less data would need an explicit, separately designed API contract.

## Static plan and runtime obligations

The recommended compiled plan summarizes each action/function's read and write shapes, purity, order sensitivity, and dependence on partial facts or assumptions. Summaries of recursive functions must reach a conservative fixed point; unsupported analysis becomes “may access anything,” never “accesses nothing.” Retain the normalized plan in `.ylc` or derive it from validated indexed instructions, without requiring YL source at execution time.

At runtime, instantiate summaries against the committed syntax occurrences. Maintain candidate producer sets and record actual reads. When a producer changes state, invalidate affected cached answers, including cached absence and conflicts. A direct read set alone is insufficient if a previously pending producer can later compute a matching key: candidate-producer and discovery information must participate too.

Before releasing an internal value, establish:

1. Its producer domain is closed for this context, or the value can safely be recomputed.
2. No unfinished compatible consumer, suspended guard, recursive computation, or hypothetical evaluation can still require that retained copy.
3. Exported graph entries, diagnostics/source labels, and host-visible results retain everything they require.
4. Eviction removes indexes, dependency edges, and accounting consistently; nested shared values remain valid until their final owner releases them.

Unknown dynamic keys delay reclamation only for compatible families when that separation is proven. They should not force a process-wide cache flush unless the analysis genuinely cannot narrow their reach.

## Why combine static and dynamic analysis?

Liveness asks whether a value may be needed in a future computation. Conservative analysis retains values on either branch and iterates through loops; a single traversal is insufficient in general. This supports a safe “retain when uncertain” fallback, rather than an exact last-reader promise for every program. See the [CMU compiler course's liveness analysis notes](https://www.cs.cmu.edu/~janh/courses/411/24/lectures/05-liveness.pdf).

Rust's query model requires immutable keys/results and pure providers for memoization. Its dependency tracking records actual query accesses; red-green validation avoids propagating changes when a recomputed result is unchanged. These are useful design precedents, not a drop-in algorithm: YL also has multiple contribution producers, partial facts, and pending states. See [the query model](https://rustc-dev-guide.rust-lang.org/queries/query-evaluation-model-in-detail.html) and [incremental compilation details](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html).

Heap reachability is not identical to future use. Static access-graph research shows how bounded summaries can identify some dead-but-reachable data, while requiring aliasing and availability reasoning. For YL, this motivates reclaiming proven-unused internal copies without pretending exported facts have become unobservable. See [Khedker, Sanyal, and Karkare, *Heap Reference Analysis Using Access Graphs*](https://arxiv.org/abs/cs/0608104).

## Authoring conventions that improve precision

These recommendations follow from the engine's key-pattern representation; they do not change valid YL.

- Prefer tuple keys such as `(declaration, typeSlot)` over concatenated strings. Stable symbol components distinguish families even when another component is unknown.
- Pass occurrence, scope, and hypothetical context explicitly. Hidden mutable context makes both cache identity and lifetime inference harder to justify.
- Keep operations small enough that read/write summaries remain inspectable; ordinary wrappers and namespaces need not defeat interprocedural analysis.
- Use finalized reads when the operation requires a settled answer. Use `facts` deliberately for accumulation; do not assume an early fact set is complete.
- Return or retain compact identities instead of repeatedly copying large source captures when the text/AST is unnecessary.
- Treat diagnostic origins and public graph output as deliberate consumers. They extend lifetime even after type computation finishes.

Avoid encoding key structure into text, selecting wholly unrelated key families through unrestricted dynamic computation, or retaining whole histories in every published value. These remain valid patterns where supported, but require broader candidate sets or larger retained output. Cache policy cannot remove the cost of explicitly copied, externally required data.

## Possible future syntax or API improvements

Do not introduce these as part of an optimization without agreement:

- An explicit **private/intermediate query family** versus published output could remove the current all-facts-exported retention requirement. Existing families must keep their observable behavior.
- Closed read/write-family declarations could help opaque host integrations, but inferred summaries should remain the default. False declarations require validation or a trusted-host boundary.
- Explicit ordered regions could make logical visibility inspectable. They must describe language order, not cache evaluation order, and require separate treatment of branches and loops.

The first implementation should measure how much can be inferred from existing tuples, symbols, explicit parameters, and normalized instructions before adding author-facing annotations.

## Implemented policy

The compiled plan records read/write key-expression roots and a fixed-point
store-read flag for every executable expression, including recursive calls,
callbacks and inactive branches. Concrete occurrences specialize this into a
last-reader schedule for functions and query-free producer retirement. This
requires no new YL syntax.

The engine now:

1. Retires evaluation of proven query-free producers after their first result.
   Their facts remain roots for lookups and the public graph.
2. Uses exact instantiated keys to index compatible writers. Partial/unknown
   footprints still receive ordinary conservative checks. The index is disabled
   while footprints themselves are being refined.
3. Keeps function answers across top-level expressions and propagation passes.
   Store-dependent answers are collected at a planned last consumer, and all
   remaining store-dependent answers are invalidated before snapshot mutation.
   Query-free answers can remain cached.
4. Bounds each function memo store and the shared hypothetical-result memo store
   to 4 MiB of accounted payload, using least-recently-used eviction. Oversized
   answers are usable without retention. Nested hypothetical calculations have
   their own function stores; this is not an 8 MiB process-wide memory ceiling.
5. Moves final contribution values into public output instead of cloning a
   second attachment store at teardown. Rust ownership drops reclaimed caches
   and intermediate buffers; no separate tracing heap is introduced.

A memo can be removed while it still has potential readers because its answer
is recomputable. The stronger last-use proof is required to remove the final
copy of an authored fact. The current all-facts output contract makes those
facts public roots. A selective publication/private-family interface could
permit earlier fact destruction, but is a separate observable design decision.

On/off equivalence testing also exposed a preexisting receipt bug: a cached
hypothetical answer omitted the requested root key and producer observations
that an uncached evaluation recorded. Memo entries now preserve the complete
answer receipt. ASTs, contributed values and diagnostics remain unchanged;
dependency reporting is consistent between hits and misses.

## Initial cache-policy measurements (18 October)

These measurements describe the initial implementation before the partial-key
extension below. Current study commands also run the added structural-key case.

Measured on Windows, Intel Core i5-11400, Rust release profile, 18 October 2026.
Engine implementation milestone: `9a0380e`; subsequent changes add studies,
documentation and optimization-metadata goldens. The code is based on
`b02e783dadad8cce7787df5914fc1ce186fbbbc5`.

### Method

```powershell
$env:CARGO_TARGET_DIR = "D:/CodexBuilds/your-language-query-lifetimes"
cargo test --release --lib query_lifetime_study -- --ignored --nocapture --test-threads=1
```

Each corpus run compiles and reloads its language before measurement. Engine
timing includes discovery, specialization and settlement; it excludes parsing,
serialization and language compilation. The table reports medians of seven
fresh-engine runs after one warm-up. Both modes use the same executable and
compiled language. `enabled=false` is a **cache-disabled reference policy**:
it disables function/hypothetical retention, exact indexing and query-free
retirement. It also disables the old per-expression recursive memoization, so
these timings are not a previous-production versus new-production comparison.

Equivalence checks compare every producer's final value, reads, waits and
diagnostics across all target programs in learning steps 07, 09, 10 and 11.
Additional fixtures cover positive fact cycles, negative cycles, guarded
producers and dynamically selected keys. Public API integration tests cover
source-free reload, ASTs, graph output and diagnostic spans. The initial three ignored
study tests are run explicitly by the command above; the extension adds a fourth.

| Corpus input | Reference policy | Optimized | Statically retired producers |
| --- | ---: | ---: | ---: |
| Query shadowing | 2.609 ms | 2.514 ms | 6 / 20 (30.0%) |
| Type instantiation | 1.331 ms | 0.793 ms | 3 / 10 (30.0%) |
| Generic addition | 46.099 ms | 13.074 ms | 13 / 58 (22.4%) |
| Compound valid | 360.068 ms | 152.681 ms | 119 / 489 (24.3%) |
| Compound invalid assignment | 38.695 ms | 18.979 ms | 24 / 93 (25.8%) |

For the compound-valid input, the plan proves 16 of 17 pure functions do not
read the store, but only 119 of 489 instantiated producers are query-free:
query reads in arguments and guards still matter. The other 370 producers
retain runtime settlement. Exact keys are known for all 286 writers by final
settlement, but those syntax identities/texts were not known at language
compile time. Top-level candidate inspections fall from 15,469,515 to 339,759;
unknown-key refinement still scans conservatively. These counters exclude
inspections inside isolated hypothetical engine copies.

### Scale and retained-memory studies

The structured-key fixture uses an independent tuple-key contribution and a
read per occurrence. Single-run timings are illustrative; inspection counts
are deterministic. This is not a new flow/history implementation.

| Occurrences | Reference / optimized inspections | Reference / optimized time | Static retirement |
| ---: | ---: | ---: | ---: |
| 100 | 60,000 / 300 | 2.547 / 1.901 ms | 100 / 200 |
| 400 | 960,000 / 1,200 | 14.682 / 8.114 ms | 400 / 800 |
| 1,000 | 6,000,000 / 3,000 | 60.221 / 19.792 ms | 1,000 / 2,000 |

A separate real-engine fixture makes 6,000 independent recursive pure calls
with long text arguments. Bounded and unbounded memo policies produce identical
values/dependencies. The bounded cache peaks at **4,194,304 accounted bytes**
versus **7,113,288** with retention unbounded, and evicts 4,940 entries. Elapsed
engine time was 130.421 versus 136.502 ms in this run. All 6,000 producers are
statically query-free. This compares memo retention, not total process RSS or
public-output memory; the required contributed facts remain in both results.

For compound-valid, the root function cache peaks at 1,559,048 accounted bytes,
records 5,866 hits and collects 1,134 entries through planned/snapshot retirement.
Hypothetical memo retention is analysis-local and bounded separately. Recursive
hypothetical copies can own additional bounded function caches, and producer
copies/public facts are outside these memo accounting counters.

### Actual editor comparison

The existing LSP benchmark was run once per binary after the corpus studies:

```powershell
node scripts/benchmark-lsp.cjs <pre-change-release-exe> 08-mini-typescript 11-compound-types --max-warm-open-ms=250
node scripts/benchmark-lsp.cjs <new-release-exe> 08-mini-typescript 11-compound-types --max-warm-open-ms=250
```

The pre-change installed executable was `yl-doc-comments-release.exe`, SHA-256
`D868F6C17691F7D3F408DE8A141ECC9536658B0468578ECB40E8FDB619F4A31A`.
These are actual protocol measurements, not the cache-disabled reference policy.

| Project | Cold tokens before / after | Source edit before / after | Warm reopen before / after |
| --- | ---: | ---: | ---: |
| Mini TypeScript | 170 / 166 ms | 54 / 53 ms | 3 / 4 ms |
| Compound types | 513 / 338 ms | 421 / 257 ms | 4 / 4 ms |

Both binaries report zero diagnostics and the same token integer counts (145
and 265). Warm document reuse remains fast. These single protocol runs establish
no broad latency guarantee; hardware, input and process scheduling affect time.

### Limits and next measurements

- Static retirement is intentionally a coarse proof: no store reads at all.
  Runtime-free classification for store-reading producers would require stronger
  family/domain-closure proofs and a publication contract, not assumptions about
  which writes happen to have run first.
- Store-dependent memo invalidation currently uses propagation snapshots, not
  per-key versioning. This is safe for negative reads, changing footprints,
  partial facts and deferred guards; finer invalidation is future work.
- Last-reader planning conservatively includes all branches and callback paths.
  Dynamic subsets in hypothetical demand evaluation may fall back to end-of-pass
  invalidation or the byte budget instead of hitting a planned final reader.
- Memo keys still serialize owned arguments, and immutable YL values can still
  explicitly copy large structures. Caching does not turn materialized full-map
  copies into persistent maps, nor prove a future flow history has O(1) lookup.
- No authored fact is silently deleted from public output. The returned graph
  and retained diagnostics establish a memory lower bound until their owner is
  released. Selective publication would need an explicit decision.

### Raw corpus counters

The following records are the study's unrounded output. Cache bytes are
accounted estimates, not allocator measurements. Function counters describe the
root engine; hypothetical-result counters include its shared memo store.

```json
{"example":"07-declarative-queries/shadowing.txt","enabled":false,"median_us":2609,"candidate_checks":14080,"exact_writers":12,"residual_writers":0,"store_free_functions":0,"functions":0,"static_producers":6,"producers":20,"retired":0,"memo_hits":0,"memo_misses":0,"memo_collected":0,"memo_peak_bytes":0,"hypothetical_hits":0,"hypothetical_peak_bytes":0}
{"example":"07-declarative-queries/shadowing.txt","enabled":true,"median_us":2514,"candidate_checks":10176,"exact_writers":12,"residual_writers":0,"store_free_functions":0,"functions":0,"static_producers":6,"producers":20,"retired":6,"memo_hits":0,"memo_misses":0,"memo_collected":0,"memo_peak_bytes":0,"hypothetical_hits":0,"hypothetical_peak_bytes":0}
{"example":"09-type-instantiation/program.txt","enabled":false,"median_us":1331,"candidate_checks":750,"exact_writers":9,"residual_writers":0,"store_free_functions":0,"functions":0,"static_producers":3,"producers":10,"retired":0,"memo_hits":0,"memo_misses":0,"memo_collected":0,"memo_peak_bytes":0,"hypothetical_hits":0,"hypothetical_peak_bytes":0}
{"example":"09-type-instantiation/program.txt","enabled":true,"median_us":793,"candidate_checks":103,"exact_writers":9,"residual_writers":0,"store_free_functions":0,"functions":0,"static_producers":3,"producers":10,"retired":3,"memo_hits":0,"memo_misses":0,"memo_collected":0,"memo_peak_bytes":0,"hypothetical_hits":15,"hypothetical_peak_bytes":5442}
{"example":"10-declarative-functions/generic-addition.txt","enabled":false,"median_us":46099,"candidate_checks":102254,"exact_writers":40,"residual_writers":0,"store_free_functions":1,"functions":1,"static_producers":13,"producers":58,"retired":0,"memo_hits":0,"memo_misses":25,"memo_collected":0,"memo_peak_bytes":0,"hypothetical_hits":0,"hypothetical_peak_bytes":0}
{"example":"10-declarative-functions/generic-addition.txt","enabled":true,"median_us":13074,"candidate_checks":17075,"exact_writers":40,"residual_writers":0,"store_free_functions":1,"functions":1,"static_producers":13,"producers":58,"retired":13,"memo_hits":23,"memo_misses":2,"memo_collected":0,"memo_peak_bytes":1568,"hypothetical_hits":10,"hypothetical_peak_bytes":14576}
{"example":"11-compound-types/valid.txt","enabled":false,"median_us":360068,"candidate_checks":15469515,"exact_writers":286,"residual_writers":0,"store_free_functions":16,"functions":17,"static_producers":119,"producers":489,"retired":0,"memo_hits":0,"memo_misses":8644,"memo_collected":0,"memo_peak_bytes":0,"hypothetical_hits":0,"hypothetical_peak_bytes":0}
{"example":"11-compound-types/valid.txt","enabled":true,"median_us":152681,"candidate_checks":339759,"exact_writers":286,"residual_writers":0,"store_free_functions":16,"functions":17,"static_producers":119,"producers":489,"retired":119,"memo_hits":5866,"memo_misses":1992,"memo_collected":1134,"memo_peak_bytes":1559048,"hypothetical_hits":0,"hypothetical_peak_bytes":0}
{"example":"11-compound-types/invalid-assignment.txt","enabled":false,"median_us":38695,"candidate_checks":324756,"exact_writers":56,"residual_writers":0,"store_free_functions":16,"functions":17,"static_producers":24,"producers":93,"retired":0,"memo_hits":0,"memo_misses":1195,"memo_collected":0,"memo_peak_bytes":0,"hypothetical_hits":0,"hypothetical_peak_bytes":0}
{"example":"11-compound-types/invalid-assignment.txt","enabled":true,"median_us":18979,"candidate_checks":8813,"exact_writers":56,"residual_writers":0,"store_free_functions":16,"functions":17,"static_producers":24,"producers":93,"retired":24,"memo_hits":737,"memo_misses":348,"memo_collected":176,"memo_peak_bytes":308780,"hypothetical_hits":0,"hypothetical_peak_bytes":0}
```

## Partial-key extension (19 October)

Implementation milestone: `44e2bff`, extending the original PR head `0c6d93a`.
No syntax, source visibility, AST, artifact format or authored diagnostic changes
are introduced. The original indexed key expressions already contain the
structural information needed to specialize the plan without YL source.

### What changed

The original matcher compared tuple positions conservatively, but the index only
accelerated fully exact keys. Every partial key otherwise entered one residual
list. The new index partitions possible writers by compatible root shape, then
uses known scalar/arity constraints at **any nested tuple path**. Unknown paths
remain wildcards. Union summaries include all possible root shapes and retain
only constraints common to every branch; full structural matching preserves
correlations after index selection.

For example, a contribution key containing an unresolved scope can still reveal
its channel and name:

```yl
contribute((get((self, scope)), declarations, "count"), value)
```

The fixed `declarations` slot and `"count"` name can exclude unrelated families
before the scope settles. They do not make that name globally unique. A scope
identity still selects the actual declaration, and a pending key remains pending.

Readiness now tracks concrete compatible-candidate IDs and producer versions.
A reader can sleep while both remain unchanged. A newly matching writer, changed
value or changed approximation status wakes it again. Negative lookups retain
an empty candidate observation, which is rechecked after footprint refinement.
Inactive guarded writers are still potential producers. The logical order used
by `getEarlier` is unchanged; overinclusive wakeups can do extra work but cannot
make a later declaration visible.

Immutable structural template bounds are shared among hypothetical copies of
the same occurrence forest. Runtime footprints and observations remain local.
The engine disables the changing dynamic index while refining footprints and
uses only those immutable bounds there. Public fact roots are retained; skipped
evaluations still trigger applicable memo retirement points.

### Comparative study

```powershell
cargo test --release --lib query_lifetime_study_partial_keys -- --ignored --nocapture --test-threads=1
```

The fixture contains unresolved guarded writers and independently settling
contributions for many names. It deliberately stresses partial keys and pending
cycles; it is not a general language-performance benchmark. Three policies run
against the same source-free reloaded language:

- **Conservative:** no indexing, memo retention or producer retirement.
- **Exact:** the preceding PR's exact-key index and memo policy; partial writers
  remain in its residual list and readers are reevaluated as before.
- **Structural:** partial-position indexing plus candidate/version wakeups.

All final producer values, reads, waits and diagnostics are compared for equality.
Timings below are single fresh-engine release runs made after other build/test
jobs finished. They include specialization and settlement, excluding parsing and
language compilation. Candidate counts measure lookup inspections, not all index
construction or observation-comparison operations.

| Occurrences | Conservative / exact / structural inspections | Conservative / exact / structural time | Skipped evaluations |
| ---: | ---: | ---: | ---: |
| 50 | 975,000 / 825,750 / 2,950 | 90.777 / 83.896 / 10.287 ms | 650 |
| 200 | 15,600,000 / 13,203,000 / 11,800 | 1,460.165 / 1,353.697 / 41.041 ms | 2,600 |
| 500 | 97,500,000 / 82,507,500 / 29,500 | 9,557.664 / 9,032.405 / 121.659 ms | 6,500 |

This fixture's structural inspection count grows linearly (59 per occurrence).
That is an observed property of this family, not a bound for arbitrary YL queries.
An opaque reader with no known structural component keeps **500 of 500** candidate
writers: **zero pruning gain**, despite 345,036 accounted index bytes. Unknown
families and opaque helpers can therefore retain broad scans; these are valid
cases, not reasons to guess absence.

| Occurrences | Accounted structural index metadata | Retained wakeup observations |
| ---: | ---: | ---: |
| 50 | 328,712 bytes | 12,800 bytes |
| 200 | 1,313,012 bytes | 51,200 bytes |
| 500 | 3,281,612 bytes | 128,000 bytes |

Index bytes combine the root engine's immutable templates and residual index.
They exclude required candidate ID rosters, temporary constraint construction,
public output and hypothetical-local dynamic indexes. Immutable template indexes
are shared rather than copied into each hypothetical context. Each index has a
4 MiB conservative metadata budget; exhaustion falls back to a broader roster.
Wakeup observations have a 4 MiB retained budget and a per-observation size limit;
exhaustion falls back to reevaluation. These accounted bounds are not total RSS
or allocator guarantees.

### Tests and remaining limits

The cache-disabled, exact and structural policies are compared across every
program in learning steps 07, 09, 10 and 11. Focused tests cover late-position
mismatches with a leading hole, nested tuples, unions, pending-to-known scope
refinement, inactive guards, newly matching writers, negative lookup closure,
source-order visibility, hypothetical contexts and exhausted metadata budgets.
Independent review found no correctness blocker. The final local all-target run
passes 554 Rust tests; four release studies pass explicitly. Formatting, strict
Clippy, editor tests and documentation example flows also pass.

No whole-statement sequencing rule is added. Unrelated queries may settle while
another key remains pending. This does not implement multithreading, flow-order
syntax, or early destruction of facts exported through the public graph.

Structured tuple keys with stable channel symbols remain the most useful
convention. Their known components can now appear anywhere, so authors need not
reorder valid key structure just for a prefix index. Exposing a known tuple frame
around an unresolved payload preserves more information than hiding the whole
key behind an opaque/recursive computation. Supporting richer abstract summaries
for those helpers could improve precision later without changing the language.
