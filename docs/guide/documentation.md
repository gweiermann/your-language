# Maintain the documentation

The public site is built from `docs/` with VitePress. Tutorials teach complete workflows; reference pages define language rules; API pages describe the CLI, Rust interface, artifacts, ASTs, and diagnostics. Runnable definitions belong in `examples/`.

## Local workflow

```sh
npm ci
npm run docs:dev
npm run docs:check-examples
npm run docs:check-course
npm run docs:build
```

`docs:check-examples` compiles the checked-in syntax examples and verifies their expected results, then runs the inference tutorial modes and checks the composed Program preview. `docs:build` builds the production site and checks internal page links and anchors. Keep examples and documented command paths synchronized when moving files.

## Deployment

The production output is `docs/.vitepress/dist/`. A static web server can serve this directory. By default, the base path is `/`; set `VITEPRESS_BASE=/your-language/` when building for a project site under that path. Deployment does not require the Rust runtime.

The documentation workflow publishes the site to [GitHub Pages](https://gweiermann.github.io/your-language/) on pushes to `main`. Pull requests build and validate the same site with the `/your-language/` base path, but do not deploy. The workflow can also be run manually from `main`.

The repository's **Settings / Pages / Build and deployment / Source** must be set to **GitHub Actions**. The workflow uploads the checked production build and deploys it through the Pages environment. Pull requests validate the same site without deploying.

## Editing principles

Describe observable language behavior with complete explanations and focused examples. Keep planned features on the roadmap, and distinguish them from supported syntax. Preserve source spans and approved value/cardinality rules when revising examples. Historical implementation and agent notes are stored separately in `.agents/`.

## Course and maintainer material

The sequential course lives under `tutorials/course/`; concept guides live under
`guide/`, exact syntax under `reference/`, and core/host interfaces under `api/`.
The standard-library page is the canonical export reference. Keep old topic URLs
working when reorganizing content.

`docs:check-course` materializes complete filename-marked fences in chapter order,
checks their positive/negative programs, verifies bundled checkpoints and source
includes, reloads an artifact, and checks that standard exports are documented.
The check is also included in `docs:check-examples`. Add a filename fence or an
explicit checkpoint marker when extending the course; do not present disconnected
fragments as a complete runnable step.

Engine studies and benchmark methodology belong under `.agents/implementation/`
in the repository. The public query contract describes observable behavior and
practical authoring conventions, independent of cache policies.
