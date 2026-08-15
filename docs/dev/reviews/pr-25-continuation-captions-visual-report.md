# Visual report — PR 25 continuation captions (#5057)

Branch: `execute-plan/typst-agent-native-pr-25-continuation-captions`.
Upstream issue: https://github.com/typst/typst/issues/5057.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Change

New `block(continuation: none | content)` primitive (prepends content to every
fragment of a breakable block after the first) and figure sugar
`figure(caption-repeat: none | content)` (realized as a breakable block whose
continuation is the repeat caption; requires a caption, forbids `placement`).

## Test evidence

Suite file `tests/suite/model/figure-continuation.typ`; all pages 70pt tall,
text 10pt:

1. `figure-continuation-table`: 24-row table in a figure with bottom caption
   and `caption-repeat`. Page 1: header + rows + caption, **no** repeat.
   Pages 2+: the repeat caption ("TABEL 1: DATA (LANJUTAN)") appears above the
   repeated table header and rows. (Verified with a bold repeat caption:
   continuation baseline at 6.45pt vs body baseline 6.58pt, and by the
   parent `translate(0 6.45)` group wrapping the spilled frame in the SVG.)
2. `figure-continuation-block`: non-table breakable body (`lorem(60)`) with
   caption and `caption-repeat`; 4 pages, repeat caption only on pages 2-4.
3. `figure-continuation-no-caption`: error — `` `caption-repeat` requires a
   caption `` with hint (error annotation test).
4. `figure-continuation-float`: error — `` `caption-repeat` is not available
   for floating figures `` (error annotation test).
5. `block-continuation`: the raw primitive with `*LANJUT*`; the bold repeat
   line is present on continuation pages only.

New reference outputs: `tests/ref/render/{block-continuation,
figure-continuation-block,figure-continuation-table}.png` plus additive
entries in `tests/ref/{pdf,svg}/hashes.txt`. No existing reference output or
hash was modified (verified via `git diff` on both hash files: additions
only).

## Notes

- A pre-existing upstream behavior was confirmed while developing this:
  captioned figures do not break across pages on clean `main` (the figure's
  internal block folds `breakable` to `false`). `caption-repeat` enables
  breakability for the figure it is set on; figures without it keep their
  previous layout. See the design note for details.
- During development, an infinite spill loop and a region-mapping bug in the
  continuation path were found and fixed (the spill backlog now stays stable
  via `Regions::map`); the full suite verifies termination.
