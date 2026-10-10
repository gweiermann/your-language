# Maintain the documentation

The public site is built from `docs/` with VitePress. Tutorials teach complete workflows; reference pages define language rules; API pages describe the CLI, Rust interface, artifacts, ASTs, and diagnostics. Runnable definitions belong in `examples/`.

## Local workflow

```sh
npm ci
npm run docs:dev
npm run docs:check-examples
npm run docs:build
```

`docs:check-examples` compiles the complete checked-in examples and verifies their expected results. `docs:build` builds the production site and checks internal page links and anchors. Keep examples and documented command paths synchronized when moving files.

## Deployment

The production output is `docs/.vitepress/dist/`. A static web server can serve this directory. By default, the base path is `/`; set `VITEPRESS_BASE=/your-language/` when building for a project site under that path. Deployment does not require the Rust runtime.

The repository's documentation workflow builds and uploads the static site as a CI artifact. It does not enable GitHub Pages or publish a site automatically. Deployment can be configured independently when a public hosting destination is selected.

## Editing principles

Describe observable language behavior with complete explanations and focused examples. Keep planned features on the roadmap, and distinguish them from supported syntax. Preserve source spans and approved value/cardinality rules when revising examples. Historical implementation and agent notes are stored separately in `.agents/`.
