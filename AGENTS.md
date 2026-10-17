# Working instructions

- Read the applicable task guides; follow their links as needed:
  - Language syntax, compilation, runtime, or standard libraries: [.agents/language-changes.md](.agents/language-changes.md).
  - Examples, tutorials, or curriculum: [.agents/examples.md](.agents/examples.md).
  - CLI, project discovery, editor integration, or packaging: [.agents/tooling.md](.agents/tooling.md).
  - Before reporting completion: [.agents/validation.md](.agents/validation.md).
- Treat public documentation under `docs/` as the language contract. Keep implementation and affected public docs consistent; link to that contract instead of copying it into agent guides.
- Keep generic engine mechanisms separate from authored language policy. Put reusable YL operations in the standard library and make opinionated presets explicit opt-ins.
- Keep grammar captures beside their semantic and backend connections. Pass captures and the current occurrence explicitly into supporting operations.
- Extend the appropriate existing learning step before adding an example. Preserve its runnable `yl.json`, Cargo entrypoints, and `.txt` target programs.
- Reuse a suitable clean checkout. Preserve user changes and unrelated files; keep extra worktrees and build artifacts to a minimum.
- Make routine implementation choices independently. Record genuine observable language ambiguities without silently changing the contract, and continue unrelated work.
- Describe changes with runnable commands, expected results, and remaining limitations. Keep PR titles in semantic commit style and include a small illustrative snippet when it clarifies behavior.
