# Invariant impact — PR 23 `empty-page-marginals` (#2722)

Branch: `execute-plan/typst-agent-native-pr-23-empty-page-marginals`.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Impact per invariant

- `tests-reference-review` (scope: tests): CHANGED. Five new reference images
  and additive hash entries for five NEW tests. No existing reference was
  modified. Requires explicit human approval; the visual report is
  `docs/dev/reviews/pr-23-empty-page-marginals-visual-report.md`.
- `layout-introspection` / `layout-incremental` (scope: typst-layout, typst):
  unaffected. `marginals` is a plain styles field; no new introspection state,
  no new memoized input beyond the existing style chain.
- `eval-pure-world`, `eval-deterministic`, `syntax-*`, `ide-span-contract`:
  unaffected — no evaluation, syntax, or IDE changes.
- `cli-no-network-default`, `cli-agent-name`, `output-*`, `release-*`:
  unaffected — no CLI, output-encoding, or release changes.

## Rationale

The feature only suppresses already-laid-out header/footer frames on pages
whose body is visually empty, so all realized-document and introspection
semantics (counters, locators, tags, page labels) are preserved. The default
value keeps behavior byte-identical to the previous release; the full suite
(3750 tests) passes with zero reference changes outside the five new tests.
