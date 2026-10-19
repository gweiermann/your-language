# Semantic libraries

Semantic libraries compose queries, contributions, guards, and diagnostics. The compiler bundles `std/references` and `std/types` as authored YL modules over the generic `core/query` primitives.

`self` identifies the current syntax occurrence inside a node or pattern's `semantics` block, and captures identify their syntax occurrences directly. `referenceOf(value)` queries the declaration identity associated with a declaration or reference. `std/types` accepts either identity or a tuple key and does not perform name resolution itself.

Operations receive their targets explicitly; they do not inherit `self` from a caller:

```yl
operation annotateNumber(target) {
    annotateWithType(target, number)
}

node Number = /[0-9]+/ {
    semantics { annotateNumber(self) }
}
```

An explicitly declared parameter named `self` is allowed. A bare contextual `self` inside an operation is a definition error. Captures can be passed directly as targets.

An absent query argument ends only its dependent query or contribution. Pending inputs wait; optional absent inputs do not cancel an entire operation. Presence checks remain useful when choosing a fallback or deliberately diagnosing a missing name.

For signatures and behavior, see [authored semantic libraries](/api/standard-semantics) and [declarative attachment queries](/api/declarative-queries). The [std/types tutorial](/tutorials/std-types) connects these operations in a runnable inference example. [Rust runtime operations](/api/native-semantics) describe consumer extension contracts and the limits on hypothetical native effects.

## Ordering groups

```yl
semantics {
    createLexicalScope(self)
    precedence {
        Declaration::semantics::Register > Reference
    }
}
```

A bare node or pattern selects all its own named and ungrouped semantics. `Node::semantics::Group` selects a named group. Each clause searches the actual retained occurrence tree throughout its owner's region, at every wrapper depth. Enclosing and local chains contribute dependencies, and chains remain transitive when an optional middle construct is absent.

## Explicit boundaries

```yl
node Program = (FunctionDeclaration | LetDeclaration)* {
    semantics {
        boundary
        createLexicalScope(self)
        precedence {
            FunctionDeclaration > LetDeclaration > FunctionBody
        }
    }
}

node FunctionBody = "{" LetDeclaration* "return" Expression ";" "}" {
    semantics {
        boundary
    }
}
```

`boundary` is a standalone structural marker on the containing node or pattern. It is compiled before analysis operations run. An enclosing search can select the nested boundary occurrence itself, but does not search below it. An owner's boundary does not hide its own contents: its local clauses search the interior and stop at the next nested boundaries.

Thus Program can find named and lambda bodies at different depths without collecting their local declarations as outer peers. A selected body is deferred as a whole; once its outer prerequisites finish, local clauses order its declarations and nested functions. Local ordering cannot release boundary contents before its ancestor entry requirements are satisfied.

The marker is independent of `createLexicalScope(self)`: a boundary does not create a scope, and a scope operation does not automatically create a boundary. An extracted pattern can attach a scope identity independently of the body's ordered work. Authored lexical scopes are ordinary contributions, not lifecycle context providers. Optional Rust context-provider hooks still follow their registered lifecycle contract.

Wrappers without `boundary` do not stop the search. Named patterns retain occurrences without adding AST nodes; capture and repetition syntax do not create regions. Explicit patterns can still group reusable syntax roles, but are not needed to compensate for varying wrapper depths.

Ordinary operations follow source-order depth-first completion: initializer references run before the declaration's own registration. Bare selectors defer complete node/pattern traversal; named-group selectors order only the chosen own operations. Within a non-boundary construct, explicit earlier subconstructs can proceed before deferred work. Strict self-dependencies and contradictory chains produce structured cycle diagnostics before analysis operations run.

Ordering describes static analysis, not target-language execution or initialization safety. The search uses committed matches, so failed alternatives, probes, and unreachable grammar alternatives cannot produce semantic effects.
