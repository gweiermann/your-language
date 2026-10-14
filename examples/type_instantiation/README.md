# Explicit type instantiation

This experiment demonstrates reusable directed type relationships with ownership supplied explicitly by a Rust native library. The small grammar keeps a single parameter linked to its result. Two use sites receive independent instances; their original template remains unconstrained.

```sh
cargo run --quiet --example type_instantiation
cargo run --quiet --example type_instantiation -- errors
cargo run --quiet --example type_instantiation -- errors --json
```

The successful program reports `first` as `number` and `second` as `string`. The error program requires a numeric first result and underlines `"wrong"`, with the authored `number` requirement as a related location. Both flows compile YL, serialize and reload the artifact, then analyze through the public API.

The host registers the template root and the parameter's original syntax identity using `types::register_template`. All slots of each local identity share one fresh substitution in an instance. External identity reads remain captured; native effects are not replayed. `types::instantiate(template, use)` memoizes each pair.

Only writes to owned identities are cloned. Writes to captured destinations remain part of the original template and are not repeated for each instance. Mixed positional destinations and ownership changes after instantiation produce explicit diagnostics.

This is not automatic Mini TypeScript polymorphism. Automatic template ownership, retained match relations, higher-order call templates and generalization of recursive function components remain **PARKED**. The exact ownership question is whether the original function pattern and its syntax descendants define the freshened region, retaining record handles created outside it as captures, or whether the language library must explicitly designate the region and captures.
