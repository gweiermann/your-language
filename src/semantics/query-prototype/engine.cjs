const P = { state: "pending" };
const A = { state: "absent" };
const V = (value) => ({ state: "present", value });
const canon = (value) => {
  if (Array.isArray(value)) return "[" + value.map(canon).join(",") + "]";
  if (value && typeof value === "object")
    return (
      "{" +
      Object.keys(value)
        .sort()
        .map((k) => JSON.stringify(k) + ":" + canon(value[k]))
        .join(",") +
      "}"
    );
  return JSON.stringify(value);
};
class QueryEngine {
  constructor({ parents = {}, discovered = true } = {}) {
    this.parents = parents;
    this.discovered = discovered;
    this.inputs = new Map();
    this.producers = [];
    this.deps = new Map();
    this.reads = 0;
  }
  input(name, result) {
    this.inputs.set(name, result);
    return this;
  }
  producer(id, { footprint, run, kind = "contribution" }) {
    this.producers.push({ id, footprint, run, kind, status: P, output: null });
    return this;
  }
  ancestors(id) {
    const out = [];
    let at = this.parents[id];
    const seen = new Set();
    while (at !== undefined && at !== null) {
      if (seen.has(at)) throw Error("parent cycle");
      seen.add(at);
      out.push(at);
      at = this.parents[at];
    }
    return out;
  }
  note(key) {
    if (this.current) this.deps.get(this.current).add(key);
  }
  read(name) {
    this.note("input:" + name);
    return this.inputs.get(name) || P;
  }
  tuple(parts) {
    if (parts.some((x) => x.state === "pending")) return P;
    if (parts.some((x) => x.state === "absent")) return A;
    return V(parts.map((x) => x.value));
  }
  get(keyResult) {
    this.reads++;
    if (keyResult.state !== "present") return keyResult;
    const key = keyResult.value;
    this.note("key:" + canon(key));
    if (this.override?.has(canon(key))) return this.override.get(canon(key));
    if (!this.discovered) return P;
    const relevant = this.producers.filter(
      (p) => p.kind === "contribution" && p.footprint(key),
    );
    relevant.forEach((p) => this.note("producer:" + p.id));
    if (relevant.some((p) => p.status.state === "pending")) return P;
    const contributions = relevant.filter(
      (p) => p.output && canon(p.output.key) === canon(key),
    );
    if (!contributions.length) return A;
    if (contributions.length > 1)
      return {
        state: "conflict",
        contributors: contributions.map((p) => p.id),
      };
    return V(contributions[0].output.value);
  }
  find(items, callback) {
    for (const item of items) {
      const r = callback(item);
      if (r.state !== "absent") return r;
    }
    return A;
  }
  resolve(at, name) {
    return this.find(this.ancestors(at), (ancestor) =>
      this.get(
        this.tuple([
          this.get(V([ancestor, "scope"])),
          V("declarations"),
          V(name),
        ]),
      ),
    );
  }
  diagnostic(id, run) {
    return this.producer(id, {
      footprint: () => false,
      kind: "diagnostic",
      run,
    });
  }
  recompute(overrides) {
    this.override = overrides;
    this.reads = 0;
    this.deps = new Map();
    for (const p of this.producers) {
      p.status = P;
      p.output = null;
      this.deps.set(p.id, new Set());
    }
    let changed = true,
      rounds = 0;
    while (changed && rounds++ < this.producers.length + 2) {
      changed = false;
      for (const p of this.producers) {
        this.current = p.id;
        const result = p.run(this);
        this.current = null;
        const next = result.state === "present" ? result.value : null;
        if (canon([p.status, p.output]) !== canon([result, next]))
          changed = true;
        p.status = result;
        p.output = next;
      }
    }
    this.override = undefined;
    return this.snapshot();
  }
  snapshot() {
    return {
      producers: this.producers.map((p) => ({
        id: p.id,
        state: p.status.state,
        output: p.output,
        deps: [...this.deps.get(p.id)],
      })),
      diagnostics: this.producers
        .filter((p) => p.kind === "diagnostic" && p.output)
        .map((p) => ({ id: p.id, ...p.output })),
      pending: this.producers
        .filter((p) => p.status.state === "pending")
        .map((p) => p.id),
      reads: this.reads,
    };
  }
  cycles() {
    const pending = new Set(
      this.producers
        .filter((p) => p.status.state === "pending")
        .map((p) => p.id),
    );
    const edges = (id) =>
      [...(this.deps.get(id) || [])]
        .filter((d) => d.startsWith("producer:"))
        .map((d) => d.slice(9))
        .filter((id) => pending.has(id));
    const result = [];
    const done = new Set(),
      active = [];
    const visit = (id) => {
      const ix = active.indexOf(id);
      if (ix !== -1) {
        result.push([...active.slice(ix), id]);
        return;
      }
      if (done.has(id)) return;
      active.push(id);
      edges(id).forEach(visit);
      active.pop();
      done.add(id);
    };
    pending.forEach(visit);
    return result;
  }
  hypothetical(key, assignments) {
    const copy = new QueryEngine({
      parents: this.parents,
      discovered: this.discovered,
    });
    copy.inputs = new Map(this.inputs);
    copy.producers = this.producers.map((p) => ({
      ...p,
      status: P,
      output: null,
    }));
    const override = new Map(
      assignments.map(([key, value]) => [canon(key), V(value)]),
    );
    copy.recompute(override);
    copy.override = override;
    const result = copy.get(V(key));
    return { result, snapshot: copy.snapshot() };
  }
}
const eqKey = (expected) => (key) => canon(expected) === canon(key);
const fixed = (e, id, key, value) =>
  e.producer(id, { footprint: eqKey(key), run: () => V({ key, value }) });
const query = (e, id, key, fn) =>
  e.producer(id, {
    footprint: eqKey(key),
    run: (e) => {
      const r = fn(e);
      return r.state === "present" ? V({ key, value: r.value }) : r;
    },
  });
function fixture() {
  const e = new QueryEngine({
    parents: {
      call: "wrapper",
      wrapper: "inner",
      inner: "program",
      program: null,
    },
  });
  fixed(e, "program-scope", ["program", "scope"], "S0");
  fixed(e, "inner-scope", ["inner", "scope"], "S1");
  fixed(e, "outer-greet", ["S0", "declarations", "greet"], "outer");
  e.producer("inner-greet", {
    footprint: eqKey(["S1", "declarations", "greet"]),
    run: (e) => {
      const ready = e.read("registration");
      if (ready.state !== "present") return ready;
      return ready.value
        ? V({ key: ["S1", "declarations", "greet"], value: "inner" })
        : A;
    },
  });
  return e;
}

if (typeof module !== "undefined")
  module.exports = {
    P,
    A,
    V,
    canon,
    QueryEngine,
    eqKey,
    fixed,
    query,
    fixture,
  };
