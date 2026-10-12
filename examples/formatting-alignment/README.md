# Match arrow alignment

This fixture demonstrates the canonical alignment of meaning-match arrows. It illustrates formatting only; `typeof`, `type`, and the named types are placeholders rather than registered semantic operations.

From the repository root, create a temporary copy and run:

```powershell
Copy-Item examples/formatting-alignment/match-arrows.yl "$env:TEMP/yl-match-arrows.yl"
cargo run -- fmt "$env:TEMP/yl-match-arrows.yl"
Get-Content "$env:TEMP/yl-match-arrows.yl"
cargo run -- fmt --check --json "$env:TEMP/yl-match-arrows.yl"
```

Before formatting:

```text
string, string | string, number | number, string => type(string)
number, number => type(number)
unknown, _ | _, unknown => type(unknown)
_ => error("Unsupported operands")
```

After formatting:

```text
string, string | string, number | number, string => type(string)
number, number                                   => type(number)
unknown, _ | _, unknown                          => type(unknown)
_                                                => error("Unsupported operands")
```

The normal `fmt` command aligns these arrows by default, and `fmt --check` checks that same layout. The earlier `--align-match-arrows` flag remains accepted for compatibility. Parsing and meaning behavior are unchanged. Arrows in separate match blocks align independently; rewrite arrows and multiline/comment-split headers retain their ordinary spacing. Changes to the longest case can change padding on its sibling cases.
