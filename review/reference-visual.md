# Visual report — PR 24 `widow-orphan-control` (#5931)

Branch: `execute-plan/typst-agent-native-pr-24-widow-orphan-control`.
Upstream issue: https://github.com/typst/typst/issues/5931.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Change

New `par` fields `widows: int` and `orphans: int` (default `2`, values ≥ 1),
the minimum number of lines kept together at the bottom and top of a page.
Default `2` reproduces the previous behavior exactly; `1` disables the
respective prevention; larger values move more lines. The existing
`text(costs: (widow: .., orphan: ..))` ratios remain the on/off switch.
Engine change in `layout_flow` line collection
(`crates/typst-layout/src/flow/collect.rs`), replacing the hardcoded
two-line rules with configurable counts; when the two requirements overlap,
the whole paragraph moves as a unit.

## Test evidence

Five new suite tests in `tests/suite/layout/par-widow-orphan.typ`, using
52.5pt-tall pages (exactly four 13.08pt lines) and explicit `linebreak()`
paragraphs so line counts are deterministic. Line counts per page were
extracted from the generated SVGs (`tests/store/svg/*.svg`):

1. `par-widows-default` (5-line par, default widows=2): pages [3, 2] — the
   last two lines move together.
2. `par-widows-three` (`par(widows: 3)`): pages [2, 3] — the last three lines
   move together.
3. `par-widows-one` (`par(widows: 1)`): pages [4, 1] — prevention disabled, a
   single line ends a page.
4. `par-orphans-default` (3-line par + 5-line par, default orphans=2):
   pages [3, 3, 2] — no single line starts a page.
5. `par-orphans-one` (`par(orphans: 1)`): pages [4, 4] — a single line may
   start a page.

New reference outputs: `tests/ref/render/par-widows-{default,three,one}.png`
and `tests/ref/render/par-orphans-{default,one}.png`, plus additive entries in
`tests/ref/{pdf,svg}/hashes.txt`. No existing reference output or hash was
modified (verified via `git diff` on both hash files: additions only).

## Test runs

- `cargo testit`: 3750 passed, 0 failed, 0 skipped (full suite, no regressions).
- `cargo test -p typst-layout -p typst-realize -p typst`.
