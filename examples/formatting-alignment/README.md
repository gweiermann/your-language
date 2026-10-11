# Match arrow alignment experiment

This fixture compares the default formatter with optional alignment of meaning-match arrows. It illustrates formatting only; `typeof`, `type`, and the named types are placeholders rather than registered semantic operations.

From the repository root, create a temporary copy and run:

```powershell
Copy-Item examples/formatting-alignment/match-arrows.yl "$env:TEMP/yl-match-arrows.yl"
cargo run -- fmt --align-match-arrows "$env:TEMP/yl-match-arrows.yl"
Get-Content "$env:TEMP/yl-match-arrows.yl"
cargo run -- fmt --align-match-arrows --check --json "$env:TEMP/yl-match-arrows.yl"
```

Default case headers:

```text
string, string | string, number | number, string => type(string)
number, number => type(number)
unknown, _ | _, unknown => type(unknown)
_ => error("Unsupported operands")
```

With `--align-match-arrows`:

```text
string, string | string, number | number, string => type(string)
number, number                                   => type(number)
unknown, _ | _, unknown                          => type(unknown)
_                                                => error("Unsupported operands")
```

The normal `fmt` command keeps the current style and removes optional alignment padding. Aligned checks require the flag too. Nothing in this experiment changes parsing or meaning behavior. Arrows in separate match blocks align independently; rewrite arrows and multiline/comment-split headers retain their ordinary spacing. A change to the longest case may change padding on its sibling cases; this is the trade-off to evaluate before choosing a default style.
