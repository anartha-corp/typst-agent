# Visual report — PR 23 `empty-page-marginals` (#2722)

Branch: `execute-plan/typst-agent-native-pr-23-empty-page-marginals`.
Upstream issue: https://github.com/typst/typst/issues/2722.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Change

New `page` field `marginals: "always" (default) | "hide-empty"`. When a page's
body frame is visually empty (contains no items, or only accessibility tags),
the page's header and footer (including the automatic page number) are not
rendered. Background and foreground are unaffected, the page counter still
counts empty pages, and PDF page-label numbering metadata is preserved. The
gating happens in `layout_page_run_impl` (`crates/typst-layout/src/pages/run.rs`)
when constructing `LayoutedPage`; `pages/finalize.rs` is unchanged.

## Test evidence

Five new suite tests in `tests/suite/layout/page-marginals.typ`, all using an
80pt-tall page with `margin: (top: 20pt, bottom: 20pt)`, header `[HEADER]`,
footer `[FOOTER]` unless noted. Per-page glyph-group evidence was extracted
from the generated SVGs (`tests/store/svg/*.svg`):

1. `page-marginals-parity` (hide-empty): 3 pages. Page 1: header + 2 text lines
   + footer. Page 2 (inserted by `pagebreak(to: "odd")`): **0 glyphs** — header
   and footer hidden. Page 3: header + text + footer.
2. `page-marginals-always` (default): 3 pages. Page 2 (parity blank): header
   and footer **shown** — default behavior preserved; differs from (1) only on
   the blank page.
3. `page-marginals-numbering` (hide-empty, `numbering: "1"`, no explicit
   header): 3 pages. Page 2: **0 glyphs** (no page number). The footer glyph on
   page 3 differs from page 1, i.e. page 3 renders "3" — the counter counts the
   empty page.
4. `page-marginals-background` (hide-empty, `background: [BG]`): 3 pages.
   Page 2: exactly the two "BG" glyphs — background preserved while the header
   is hidden.
5. `page-marginals-trailing` (hide-empty, `pagebreak(weak: false)`): 2 pages.
   Page 2: **0 glyphs** — the trailing empty page hides its header.

New reference outputs: `tests/ref/render/page-marginals-{parity,always,
numbering,background,trailing}.png` plus additive entries in
`tests/ref/{pdf,svg}/hashes.txt`. No existing reference output or hash was
modified (verified with `git diff` on both hash files: additions only).

## Test runs

- `cargo testit`: 3750 passed, 0 failed, 0 skipped (full suite, no regressions).
- `cargo test -p typst-layout -p typst-realize -p typst`: passed.
