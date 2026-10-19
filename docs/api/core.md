# Core library reference

`core/...` exposes mechanisms that require engine support. `std/...` contains
[authored reusable policy](/reference/standard-library). Import both explicitly;
projects need no filesystem aliases for bundled module IDs.

## core/parser

| Primitive | Contract |
| --- | --- |
| `notAhead(grammar)` | Succeed without consuming when `grammar` does not match at the current source boundary. |
| `notBehind(grammar)` | Succeed without consuming when `grammar` does not match immediately before that boundary. |

Assertions inspect the actual boundary without first skipping trivia. They retain
original-source regex anchor context. Rust regex lookaround is not available;
these primitives accept complete grammar expressions instead.

```yl verify=parser
import { notAhead, notBehind } from "core/parser"
pattern IdentifierPart = /[A-Za-z0-9_]/
pattern keyword(value) = notBehind(IdentifierPart) value notAhead(IdentifierPart)
node Declaration = keyword("let") name: /[a-z]+/
trivia Whitespace = /\s+/
entry Declaration { trivia Whitespace }
```

## core/diagnostics

Import `error`, `warn`, or `hint`. All take
`(message, target = absent, related = absent)`. A message is text and can
interpolate semantic expressions. A target is source syntax, or a list of
`(syntax, labelOrAbsent)` annotations forming one diagnostic group; the first is
primary. An absent label still adds an underline. The optional related argument
adds another source location. See the complete
[grouped diagnostic contract](/api/diagnostics#grouped-semantic-diagnostics).

In semantic operations, an omitted target uses the evaluating occurrence. Syntax
checks inspect their matched captures; compile-time pipe diagnostics point at the
caller. Errors fail checks; warnings and hints do not change the success status.
No diagnostic names are implicitly in scope.

## core/query

All operations accept immutable semantic values. Captures retain source identity;
keys can be identities, scalar values, or nested tuples. Optional absence ends
dependent paths; pending inputs wait. The [query contract](/api/declarative-queries)
defines exact absence, ordering, alternative, and convergence behavior.

### Contributions and reads

| Signature | Behavior |
| --- | --- |
| `contribute(key, value)` | Supply one tracked contribution; absent keys/values supply none. |
| `contributeAll(key, values)` | Supply distinct list elements from one producer. |
| `get(key)` | Wait, then read a unique final contribution or absence; different producers conflict even when values are equal. |
| `getEarlier(key)` | Read contributions visible before this action in the semantic schedule. |
| `getAll(key)` | Wait, then read all final contributions in deterministic producer order. |
| `facts(key)` | Read currently available distinct facts for positive accumulation, not final absence. |

### Syntax and conditions

| Signature | Behavior |
| --- | --- |
| `ancestors(value)` | Enclosing occurrences, nearest first; terminal captures start with their containing occurrence. |
| `children(value)` | Direct committed child occurrences. |
| `text(value)` | Source text for syntax, or textual representation of a supported scalar value. |
| `isPresent(value)` | False for absence, true for present values; pending stays pending. |
| `equal(left, right)` | Compare values, including explicit absence. |
| `choose(condition, then, else)` | Evaluate only the selected branch; absent condition yields absence. |

### Collections and callbacks

| Signature | Behavior |
| --- | --- |
| `find(values, callback)` | First present callback result; pending waits rather than skipping it. |
| `map(values, callback)` | Map list positions, preserving absent results. |
| `filter(values, callback)` | Keep elements whose predicate is true; absent predicates exclude. |
| `every(values, callback)` | Check every predicate; a required absent result propagates absence. |
| `first(value)`, `last(value)` | Read a tuple/list element; an empty input returns `absent`. |
| `tail(value)` | Return the tuple/list remainder as a list, possibly empty. |
| `isList(value)` | Test list values; tuples are not lists. |
| `length(value)` | Nonnegative length of a tuple or list. |
| `hasMultiple(values)` | Test whether a list has more than one element. |
| `contains(values, value)`, `unique(values)` | Membership and deduplication of list values. |
| `isSubset(left, right)` | Compare list membership. |
| `flatten(values)` | Flatten one list nesting level. |
| `concat(values)` | Concatenate a list of strings, not arbitrary lists. |
| `zip(left, right)` | Pair lists up to the shorter length; check arity separately when required. |
| `keys(fields)`, `lookup(fields, key)` | Work with lists of `(key, value)` tuples; duplicate matching lookup keys are errors. |

Callbacks use `item => expression` and may query but not contribute effects.
`value[0]` reads a tuple/list position; its index must be a nonnegative integer literal.

### Alternatives and hypothetical queries

| Signature | Behavior |
| --- | --- |
| `values(value)` | Expose alternative candidates, wrap an ordinary value as one candidate, or return `[]` for absence. |
| `alternatives(values)` | Construct distinct alternatives used by distributed matches. |
| `hypothetical(key, assumptions)` | Read a unique result under isolated explicit assignments. |
| `hypotheticalAll(key, assumptions)` | Read all final results without forwarding diagnostics. |
| `hypotheticalChecked(key, assumptions)` | Read final results and forward demanded authored diagnostics. |

An assumption is `(key, value)` with optional third source-origin syntax. Only
supplied keys are overridden; captured outer relationships and original facts
remain intact. See [hypothetical evaluation](/api/declarative-queries#hypothetical-evaluation).
