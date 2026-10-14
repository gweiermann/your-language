const {
  P,
  A,
  V,
  canon,
  QueryEngine,
  eqKey,
  fixed,
  query,
  fixture,
} = require("./engine.cjs");
process.nextTick(() => {
  const core = runCoreTests();
  const extra = runExtraTests();
  for (const check of [...core.results, ...extra.results])
    console.log("PASS " + check.label);
  console.log(
    `${core.assertions + extra.assertions} scenario assertions passed; ${core.permutationRuns} producer-order permutations passed.`,
  );
});
function runExtraTests() {
  const result = [];
  function check(label, actual, expected) {
    if (canon(actual) !== canon(expected))
      throw Error(label + ": " + canon(actual));
    result.push({ label, passed: true });
  }
  let e = new QueryEngine();
  e.producer("selected-match-case", {
    footprint: eqKey(["classification"]),
    run: (e) => {
      const r = e.read("choice");
      if (r.state !== "present") return r;
      switch (r.value) {
        case "number":
          return V({ key: ["classification"], value: "numeric" });
        case "string":
          return V({ key: ["classification"], value: "textual" });
        default:
          return A;
      }
    },
  });
  e.recompute();
  check(
    "Pending match scrutinee keeps output pending",
    e.get(V(["classification"])),
    P,
  );
  e.input("choice", V("number")).recompute();
  check(
    "Number branch contributes numeric",
    e.get(V(["classification"])),
    V("numeric"),
  );
  e.input("choice", V("string")).recompute();
  check(
    "Switching match branch replaces the same occurrence contribution",
    e.get(V(["classification"])),
    V("textual"),
  );
  check(
    "Query producer retains external scrutinee dependency",
    [...e.deps.get("selected-match-case")],
    ["input:choice"],
  );
  e.input("choice", V("other")).recompute();
  check(
    "Wildcard empty branch closes contribution as absent",
    e.get(V(["classification"])),
    A,
  );
  let conflict = fixture().input("registration", V(true));
  fixed(conflict, "duplicate", ["S1", "declarations", "greet"], "other");
  conflict.diagnostic("duplicate-error", (e) => {
    const r = e.resolve("call", "greet");
    return r.state === "pending"
      ? P
      : r.state === "conflict"
        ? V({ message: "Duplicate declaration", sources: r.contributors })
        : A;
  });
  conflict.recompute();
  check(
    "Authored duplicate diagnostic retains both declaration provenance",
    conflict.snapshot().diagnostics[0].sources,
    ["inner-greet", "duplicate"],
  );
  function positiveFixedPoint(seed, rules) {
    const facts = new Set(seed);
    let rounds = 0,
      changed = true;
    while (changed) {
      changed = false;
      rounds++;
      for (const [from, to] of rules)
        if (facts.has(from) && !facts.has(to)) {
          facts.add(to);
          changed = true;
        }
    }
    return { facts: [...facts].sort(), rounds };
  }
  check(
    "Separate monotone SCC can close seeded positive cycle",
    positiveFixedPoint(
      ["x"],
      [
        ["x", "y"],
        ["y", "x"],
      ],
    ).facts,
    ["x", "y"],
  );
  check(
    "Separate monotone SCC has empty least solution without seed",
    positiveFixedPoint(
      [],
      [
        ["x", "y"],
        ["y", "x"],
      ],
    ).facts,
    [],
  );
  let short = fixture().input("registration", V(true));
  short.producer("outer-pending", {
    footprint: eqKey(["S0", "declarations", "greet"]),
    run: () => P,
  });
  short.recompute();
  check(
    "Settled inner hit bypasses pending outer declarations",
    short.resolve("call", "greet"),
    V("inner"),
  );
  let derived = new QueryEngine();
  query(derived, "b", ["b"], (e) => {
    const r = e.get(V(["a"]));
    return r.state === "present" ? V(r.value + 1) : r;
  });
  query(derived, "c", ["c"], (e) => {
    const r = e.get(V(["b"]));
    return r.state === "present" ? V(r.value + 1) : r;
  });
  derived.producer("a", {
    footprint: eqKey(["a"]),
    run: (e) => {
      const r = e.read("seed");
      return r.state === "present" ? V({ key: ["a"], value: r.value }) : r;
    },
  });
  derived.input("seed", V(1)).recompute();
  check(
    "Dependency chain settles despite reverse producer order",
    derived.get(V(["c"])),
    V(3),
  );
  derived.input("seed", V(5)).recompute();
  check(
    "Dependency chain recomputes after external change",
    derived.get(V(["c"])),
    V(7),
  );
  check(
    "Derived query records dependency producer",
    [...derived.deps.get("c")].includes("producer:b"),
    true,
  );
  return {
    assertions: result.length,
    results: result,
    positiveCycle: positiveFixedPoint(
      ["x"],
      [
        ["x", "y"],
        ["y", "x"],
      ],
    ),
  };
}

function runCoreTests() {
  const results = [];
  function assert(label, actual, expected) {
    if (canon(actual) !== canon(expected))
      throw Error(label + ": " + canon(actual) + " !== " + canon(expected));
    results.push({ label, passed: true });
  }
  let e = fixture();
  e.recompute();
  assert(
    "Forward declaration waits instead of selecting outer declaration",
    e.resolve("call", "greet"),
    P,
  );
  e.input("registration", V(true)).recompute();
  assert(
    "Forward inner declaration wins across wrapper without scope",
    e.resolve("call", "greet"),
    V("inner"),
  );
  e.input("registration", V(false)).recompute();
  assert(
    "False guard permits outer declaration",
    e.resolve("call", "greet"),
    V("outer"),
  );
  assert(
    "Missing declaration settles to absent",
    e.resolve("call", "missing"),
    A,
  );
  e.input("registration", P).recompute();
  assert(
    "Changing true/false guard back to pending retracts result",
    e.resolve("call", "greet"),
    P,
  );
  let duplicate = fixture().input("registration", V(true));
  fixed(
    duplicate,
    "inner-greet-duplicate",
    ["S1", "declarations", "greet"],
    "duplicate",
  );
  duplicate.recompute();
  assert(
    "Duplicate declaration retains both source producer identities",
    duplicate.resolve("call", "greet"),
    {
      state: "conflict",
      contributors: ["inner-greet", "inner-greet-duplicate"],
    },
  );
  let nested = new QueryEngine();
  fixed(nested, "scope", ["owner", "scope"], "S");
  nested.producer("declaration", {
    footprint: eqKey(["S", "decl", "x"]),
    run: (e) => {
      const name = e.read("name");
      return name.state === "present"
        ? V({ key: ["S", "decl", name.value], value: 1 })
        : name;
    },
  });
  nested.recompute();
  assert(
    "Unknown nested tuple component remains pending",
    nested.get(
      nested.tuple([
        nested.get(V(["owner", "scope"])),
        V("decl"),
        nested.read("name"),
      ]),
    ),
    P,
  );
  assert(
    "Absent tuple component propagates absence",
    nested.get(nested.tuple([A, V("decl"), V("x")])),
    A,
  );
  let local = fixture().input("registration", V(true));
  local.producer("other-scope-unknown-key", {
    footprint: (key) => key[0] === "S2",
    run: () => P,
  });
  local.recompute();
  assert(
    "Unknown keys restricted to independent scope don't block S1",
    local.resolve("call", "greet"),
    V("inner"),
  );
  let broad = fixture().input("registration", V(true));
  broad.producer("global-unknown-key", { footprint: () => true, run: () => P });
  broad.recompute();
  assert(
    "Unbounded unknown-key producer conservatively blocks known scopes",
    broad.resolve("call", "greet"),
    P,
  );
  let undiscovered = fixture().input("registration", V(true));
  undiscovered.discovered = false;
  undiscovered.recompute();
  assert(
    "Open producer discovery never claims definitive presence",
    undiscovered.resolve("call", "greet"),
    P,
  );
  let unsafe = fixture().input("registration", V(true));
  unsafe.producers = unsafe.producers.filter((p) => p.id !== "inner-greet");
  unsafe.recompute();
  assert(
    "Incomplete discovery falsely chooses outer binding: completeness is a required assumption",
    unsafe.resolve("call", "greet"),
    V("outer"),
  );
  let diagnostic = fixture().input("registration", V(false));
  diagnostic.diagnostic("missing-name", (e) => {
    const r = e.resolve("call", "unknownName");
    return r.state === "pending"
      ? r
      : r.state === "absent"
        ? V({ message: "Unknown name", span: "call" })
        : A;
  });
  diagnostic.recompute();
  assert(
    "Missing-name diagnostic emitted once",
    diagnostic.snapshot().diagnostics.length,
    1,
  );
  diagnostic.recompute();
  assert(
    "Recomputation deduplicates diagnostic by occurrence",
    diagnostic.snapshot().diagnostics.length,
    1,
  );
  fixed(
    diagnostic,
    "resolved-later",
    ["S1", "declarations", "unknownName"],
    "found",
  );
  diagnostic.recompute();
  assert(
    "Diagnostic retracts after new contribution resolves name",
    diagnostic.snapshot().diagnostics.length,
    0,
  );
  let cycle = new QueryEngine();
  cycle.producer("circular-guard", {
    footprint: eqKey(["x"]),
    run: (e) => {
      const r = e.get(V(["x"]));
      return r.state === "present"
        ? V({ key: ["x"], value: "value" })
        : r.state === "absent"
          ? A
          : P;
    },
  });
  cycle.recompute();
  assert(
    "Unseeded guarded self-dependency remains pending",
    cycle.get(V(["x"])),
    P,
  );
  assert("Dependency cycle reports contributing occurrence", cycle.cycles(), [
    ["circular-guard", "circular-guard"],
  ]);
  fixed(cycle, "seed", ["x"], "seed");
  cycle.recompute();
  assert(
    "Final-only get cannot solve even seeded monotone cycle",
    cycle.get(V(["x"])),
    P,
  );
  let types = new QueryEngine();
  fixed(types, "parameter", ["parameter", "type"], "number");
  fixed(types, "outer", ["outside", "type"], "number");
  query(types, "result", ["return", "type"], (e) => {
    const a = e.get(V(["parameter", "type"])),
      b = e.get(V(["outside", "type"]));
    if (a.state === "pending" || b.state === "pending") return P;
    if (a.state !== "present" || b.state !== "present") return A;
    return V(
      a.value === "string" || b.value === "string" ? "string" : "number",
    );
  });
  types.recompute();
  const before = canon(types.snapshot());
  const hypothetical = types.hypothetical(
    ["return", "type"],
    [[["parameter", "type"], "string"]],
  );
  assert(
    "Hypothetical parameter override recomputes dependent return",
    hypothetical.result,
    V("string"),
  );
  assert(
    "Hypothetical evaluation leaves original contribution graph unchanged",
    canon(types.snapshot()),
    before,
  );
  assert(
    "Original return remains number",
    types.get(V(["return", "type"])),
    V("number"),
  );
  assert(
    "Captured outer type remains number",
    types.get(V(["outside", "type"])),
    V("number"),
  );
  function permutations(items) {
    if (!items.length) return [[]];
    return items.flatMap((item, i) =>
      permutations(items.filter((_, j) => j !== i)).map((rest) => [
        item,
        ...rest,
      ]),
    );
  }
  let orderRuns = 0;
  for (const order of permutations([0, 1, 2, 3])) {
    const f = fixture().input("registration", V(true));
    f.producers = order.map((i) => f.producers[i]);
    f.recompute();
    if (canon(f.resolve("call", "greet")) !== canon(V("inner")))
      throw Error("Permutation failed");
    orderRuns++;
  }
  assert(
    "All producer execution permutations yield same inner binding",
    orderRuns,
    24,
  );
  return {
    assertions: results.length,
    results,
    permutationRuns: orderRuns,
    cycleLimitation: cycle.snapshot(),
    hypothetical: hypothetical.result,
  };
}
