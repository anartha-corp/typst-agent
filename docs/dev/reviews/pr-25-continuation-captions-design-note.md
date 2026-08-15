# Design note — PR 25 continuation captions (#5057)

Branch: `execute-plan/typst-agent-native-pr-25-continuation-captions`.
Upstream issue: https://github.com/typst/typst/issues/5057.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Upstream state (as researched)

- The issue asks for repeated/continuation captions ("Tabel 1: … (lanjutan)") for
  multi-page tables; PgBiel directed it to the *figure* level, since captions
  belong to figures.
- Upstream PR #8201 ("Add repeatable figure captions", by hongjr03) was
  **closed unmerged**. Its design: `figure.caption(repeat: bool)` plus a
  *synthesized* `continued: bool` field customized via
  `show figure.caption.where(continued: true)`, implemented with a dedicated
  figure multi-layouter. Note its example relies on
  `#show figure: set block(breakable: true)` — captioned figures are not
  breakable out of the box in v0.15.1 (the figure show rule wraps its body in
  `BlockElem::packed(..)`, whose `breakable` field is left absent and folds to
  `false`; verified against clean `main`).
- Maintainer feedback (laurmaedje) rejecting #8201: prefer building on the
  existing `grid` machinery (like #5191) or a *generic repeatable-content
  primitive* keyed by occurrence; synthesized fields are a last resort; keep
  the number of custom layouters as low as possible. Suggested shape:
  `grid.header(repeatable(key: .., n => ..))` where `n` is the occurrence
  index (0 = first fragment, 1+ = continuations).

## Design adopted here

Two layers, no synthesized fields, no new custom layouter:

1. **Generic low-level primitive** `block(continuation: none | content)`:
   content prepended to every fragment of a breakable block after the first.
   Implemented in the existing flow machinery (`MultiChild` /
   `MultiSpill::layout`): the continuation is laid out per fragment and the
   region heights are reduced through `Regions::map` so the spill backlog
   stays stable. This is exactly the kind of lower-level primitive
   laurmaedje asked for ("easier to reproduce in user space"); figures are
   one consumer, tables/polylux-like content are others.

2. **Figure sugar** `figure(caption-repeat: none | content)`: realized as a
   breakable block with `continuation` set to the repeat caption. Requires a
   `caption` and no `placement` (floats cannot break); sets `breakable: true`
   so captioned figures can break without the `show figure: set
   block(breakable: true)` workaround.

Example achieving the STIK need:

```typ
#figure(
  table(columns: 3, table.header(..), ..rows),
  caption: [Tabel 1: Data],
  caption-repeat: [Tabel 1: Data (lanjutan)],
)
```

## Divergences from the maintainer's preferred direction (open points)

- Occurrence-index expressiveness: `block(continuation: content)` only
  distinguishes first vs later fragments. The maintainer's `n => content`
  callback shape (or a keyed repeatable element) would be strictly more
  expressive ("is this the last page", "N of M") and can extend the same
  mechanism later (`continuation: (index, is_last) => content`) without
  breaking the `content` form.
- Per-figure customization without show rules: content is passed per figure,
  so unlike #8201 no global `show figure.caption.where(..)` filtering is
  needed for distinct continuation styles.
- `figure(caption-repeat)` silently implies `breakable: true`; upstream #8201
  instead required the user's show rule. If upstream prefers keeping figure
  non-breakability implicit, the auto-breakable behavior is a one-line change.

## Alternatives considered

- #8201's `figure.caption(repeat:, continued:)` with synthesized fields:
  rejected upstream; synthesized fields and a dedicated layouter.
- Pure template workaround (`context` + `show grid` counting, as shared in
  the issue thread): works but fragile (breaks with links/outline, duplicated
  logic per template).
- `table`-level continuation field: rejected by PgBiel (captions are a figure
  concern; other figure kinds benefit too).
