# Installation

## Requirements

The repository pins Rust 1.85.0 through `rust-toolchain.toml`. Install Rust with rustup so that Cargo selects the required toolchain automatically. Node.js 22 or later is recommended for building the documentation; it is not required to compile or run YL languages.

## Build the CLI

```sh
git clone https://github.com/gweiermann/your-language.git
cd your-language
cargo install --path . --locked
```

The binary is named `yl`. Ensure the Cargo binary directory is on `PATH`. Alternatively, run commands through Cargo without installing:

```sh
cargo run -- check examples/getting-started/language.yl
```

## Verify the installation

```sh
yl check examples/getting-started/language.yl
yl compile examples/getting-started/language.yl -o getting-started.ylc
yl language getting-started.ylc ast examples/getting-started/program.txt
```

The first command validates the definition. The second writes an artifact. The third writes a JSON AST containing two declarations. Paths are relative to the shell's working directory; run these examples from the repository root.

## Build the documentation

The site uses [VitePress](https://vitepress.dev/). Its dependencies are locked in `package-lock.json`:

```sh
npm ci
npm run docs:dev
```

The development server prints its local URL. To verify a production build and check internal links:

```sh
npm run docs:build
npm run docs:preview
```

The build output is `docs/.vitepress/dist/`. Local builds use `/`; the GitHub Pages workflow sets `VITEPRESS_BASE=/your-language/` automatically. The published documentation is available at [gweiermann.github.io/your-language](https://gweiermann.github.io/your-language/).
