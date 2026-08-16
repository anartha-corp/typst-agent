# Design note — PR 27 searchable text in complex shaping (#4225)

Branch: `execute-plan/typst-agent-native-pr-27-pdf-text-extraction`.
Upstream issue: https://github.com/typst/typst/issues/4225.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Scope

The PDF text layer lives in **krilla** (rev `7772dbe` pinned in `Cargo.toml`);
the fix cannot be implemented inside this repository. This note delivers the
reproduction, the mechanism analysis, fix directions for the krilla PR
(human-filed), and the pin-bump readiness checklist.

## Upstream state

Open, labels `bug`, `pdf`, `text`. LaurenzV traced the Devanagari case to
script-specific shaping producing reordered glyphs whose ToUnicode mapping
breaks; khaledhosny pointed to the HarfBuzz `Renderer=HarfBuzz` approach
(cluster-based codepoint emission + `ActualText`) and noted that plain
`ActualText` only helps in Acrobat/Chrome. Waelwindows reproduced the same
with Arabic harakat.

## Reproduction

Fixture: `docs/dev/reviews/fixtures/pr-27-arabic-harakat-extraction.typ`
(system font Noto Naskh Arabic UI).

```sh
typst-agent compile pr-27-arabic-harakat-extraction.typ out.pdf
pdftotext out.pdf out.txt
```

Source line:

```text
اقتباس من النص العربي: "صَحفيّة ودَوليّة" مع الحركات والتشكيل.
```

Observed extraction (v0.15.1-era code, pdftotext from poppler):

```text
"َصحفّيّي ة َد
َص
وَدولّيّي ة" مع الحركات
والتشكيل.
```

`صَحفيّة` becomes `َصحفّيّي ة`, `ودَوليّة` splits across lines with a stray
`َص`, i.e. combining marks detach and reorder. Same-class corruption occurs
for Devanagari per the issue (requires a Devanagari font such as Siddhanta).

## Mechanism

1. **Typst shaping** (`crates/typst-layout/src/inline/shaping.rs`) produces
   reordered glyphs and ligatures with cluster ranges for complex scripts.
2. **Krilla glyph pipeline** (`crates/krilla/src/text/group.rs`):
   `GlyphSpanner` assigns each glyph the codepoints of its cluster text and
   wraps many-to-one/many-to-many cluster cases in `ActualText`. For viewers
   without ActualText support (Firefox, Apple Preview, poppler's pdftotext)
   the fallback per-glyph ToUnicode is used, which misattributes marks and
   reordered glyphs.
3. **Krilla cmap writing** (`crates/krilla/src/text/cid.rs`): the ToUnicode
   CMap is written per glyph from those assignments.

The core problem is the ToUnicode fallback path: it cannot represent
many-to-one cluster mappings correctly, so non-ActualText extractors scramble
harakat-bearing and reordered text.

## Fix directions (krilla PR, human-filed)

- Follow the HarfBuzz `Renderer=HarfBuzz` approach: decompose each shaped
  cluster into the original Unicode sequence per glyph position (including
  marks), so the ToUnicode fallback is correct even without ActualText.
- Keep ActualText for clusters that genuinely cannot be expressed as a
  codepoint sequence.
- Validate against multiple extractors (`pdftotext`, Chromium, Acrobat) and
  against Devanagari + Arabic fixtures; khaledhosny's comments in #4225 and
  #526 are the reference.

## Pin-bump readiness checklist (this repository)

1. krilla fix lands upstream; update `Cargo.toml` revs and `Cargo.lock`.
2. Add regression tests: extract the fixture PDF in CI (pdfinfo/pdftotext are
   available) or assert stable PDF bytes + manual extraction evidence in the
   visual report; new reference outputs follow the usual approval flow.
3. Release-area checks for the workspace metadata change:
   `cargo agent policy-check`, `cargo agent upstream-check`,
   `cargo agent release-manifest --input .tmp/agent/release/release-input.json`.
