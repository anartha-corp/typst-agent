# Invariant impact — PR 24 `widow-orphan-control` (#5931)

Branch: `execute-plan/typst-agent-native-pr-24-widow-orphan-control`.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Impact per invariant

- `tests-reference-review` (scope: tests): CHANGED. Five new reference images
  and additive hash entries for five NEW tests. No existing reference was
  modified. Requires explicit human approval; the visual report is
  `docs/dev/reviews/pr-24-widow-orphan-control-visual-report.md`.
- `layout-introspection` / `layout-incremental` (scope: typst-layout, typst):
  unaffected. `widows` and `orphans` are plain `par` styles resolved through
  the existing style chain; no new introspection state and no new memoized
  input beyond the styles that already key layout memoization.
- `eval-pure-world`, `eval-deterministic`, `syntax-*`, `ide-span-contract`:
  unaffected — no evaluation, syntax, or IDE changes.
- `cli-no-network-default`, `cli-agent-name`, `output-*`, `release-*`:
  unaffected — no CLI, output-encoding, or release changes.

## Behavioral compatibility

For the default values (widows=2, orphans=2) the generalized logic is
equivalent to the previous hardcoded rules: the orphan branch requires the
first two lines, the widow branch requires the last two lines, and the
overlap case (three-line paragraphs) requires the whole paragraph. The full
suite (3750 tests) passes with zero reference changes outside the five new
tests.
