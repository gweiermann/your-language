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

The documentation workflow publishes the site to [GitHub Pages](https://gweiermann.github.io/your-language/) on pushes to `main`. Pull requests build and validate the same site with the `/your-language/` base path, but do not deploy. The workflow can also be run manually from `main`.

The repository's **Settings → Pages → Build and deployment → Source** must be set to **GitHub Actions**. The workflow uploads the checked production build as a Pages artifact, then deploys through the `github-pages` environment with Pages write and identity-token permissions. Deployments run one at a time without cancelling a deployment already in progress.

## Editing principles

Describe observable language behavior with complete explanations and focused examples. Keep planned features on the roadmap, and distinguish them from supported syntax. Preserve source spans and approved value/cardinality rules when revising examples. Historical implementation and agent notes are stored separately in `.agents/`.
