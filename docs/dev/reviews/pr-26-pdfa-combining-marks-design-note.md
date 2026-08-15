# Design note — PR 26 PDF/A combining marks across fonts (#8489)

Branch: `execute-plan/typst-agent-native-pr-26-pdfa-combining-marks`.
Upstream issue: https://github.com/typst/typst/issues/8489.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Scope

The core fix lives in the **krilla** repository (the PDF backend pinned in
`Cargo.toml` as `https://github.com/LaurenzV/krilla` rev `7772dbe`, currently
`typst/typst`-side code cannot be modified from this repository per its
contract). This note delivers the reproduction, the root-cause chain, fix
directions for the krilla PR (human-filed), and the pin-bump readiness
checklist for this repository.

## Upstream state

Issue open with label `needs-reproduction`; laurmaedje asked which fonts are
used (Libertinus embedded + a system Arabic font). Reproduced here on this
machine (Noto Naskh Arabic UI / Noto Sans Arabic present via fontconfig).

## Reproduction

Fixture: `docs/dev/reviews/fixtures/pr-26-pdfa-combining-marks.typ`.

```sh
typst-agent compile pr-26-pdfa-combining-marks.typ out.pdf --pdf-standard a-3a
```

Observed (v0.15.1-era code):

```text
error: PDF/A-3a error: the text `"0"` could not be displayed with font `"Libertinus Serif"`
  ┌─ …: digit and Arabic mark across fonts: 0ً
  │                                             ^
  = hint: try using a different font
```

Without `--pdf-standard` the same document compiles; the rejection is purely
a validation artifact, matching the issue's claim that the sequence is
renderable.

## Root-cause chain

1. **Typst shaping/segmentation** (`crates/typst-layout/src/inline/shaping.rs`,
   font selection in `crates/typst-library/src/text/font/mod.rs`): the text
   `0ً` mixes a Latin digit (Libertinus Serif) and a combining Arabic mark
   `U+064B`. Shaping assigns the mark's cluster to the preceding base's
   cluster; when the run is (not) split across the font boundary, a `.notdef`
   glyph with cluster text `"0"` ends up in the Libertinus run.
2. **Krilla glyph pipeline** (`crates/krilla/src/text/group.rs`,
   `crates/krilla/src/text/cid.rs`): `GlyphSpanner` maps glyph clusters to
   codepoints and the validator flags glyphs whose cluster text cannot be
   rendered by their font.
3. **Krilla validation** (`crates/krilla/src/configure/validate.rs`,
   `ValidationError::ContainsNotDefGlyph(font, loc, text)`): under PDF/A-3a
   the notdef glyph raises `ContainsNotDefGlyph` with the cluster text `"0"`
   and the Libertinus font.
4. **typst-pdf diagnostic mapping** (`crates/typst-pdf/src/convert.rs`):
   `ContainsNotDefGlyph` is converted to the user-facing error quoted above.
   (`NoCodepointMapping` has an existing hint acknowledging that complex
   scripts like Arabic may not produce compliant documents.)

## Fix directions (for the krilla/typst PR, human-filed)

- Preferred: fix the segmentation/shaping side so the mark is shaped in its
  own Arabic run and its cluster maps to the mark's own text (no notdef with
  foreign cluster text). Likely a typst-side change in `inline/shaping.rs`.
- Alternative (krilla-side): when a glyph's cluster text contains characters
  the font cannot map, fall back to per-grapheme codepoint mapping instead of
  a hard `ContainsNotDefGlyph` under PDF/A validation.
- Both should be covered by the krilla/typst test suites before the pin bump.

## Pin-bump readiness checklist (this repository)

1. krilla PR lands upstream; update `Cargo.toml` (`krilla`/`krilla-svg` rev)
   and regenerate `Cargo.lock`.
2. Add a regression test (paged + pdf) under `tests/suite` using the fixture
   text with an Arabic system font, asserting successful PDF/A-3a export, and
   run `cargo testit --update` for the new reference outputs (reference-image
   approval applies as usual).
3. Run the release-area checks for the workspace metadata change:
   `cargo agent policy-check`, `cargo agent upstream-check`,
   `cargo agent release-manifest --input .tmp/agent/release/release-input.json`
   (per `.agents/area-manifest.json`, the Cargo.toml/Cargo.lock change selects
   the `release` area).
