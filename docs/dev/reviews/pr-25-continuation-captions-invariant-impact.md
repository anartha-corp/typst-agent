# Invariant impact — PR 25 continuation captions (#5057)

Branch: `execute-plan/typst-agent-native-pr-25-continuation-captions`.
Upstream anchor: `v0.15.1` (`a51e028041cac426f97d34335bb01d8f1d8e5e8f`).

## Impact per invariant

- `tests-reference-review` (scope: tests): CHANGED. Three new reference images
  and additive hash entries for NEW tests; two error-annotation tests without
  reference outputs. No existing reference was modified. Requires explicit
  human approval; the visual report is
  `docs/dev/reviews/pr-25-continuation-captions-visual-report.md`.
- `layout-introspection` / `layout-incremental` (scope: typst-layout, typst):
  unaffected. `continuation` participates in the existing memoized multi-block
  layout (`CachedCell` keyed by regions, styles, and locator); no new
  introspection state. The continuation prelude uses
  `Locator::relayout().split().next(..)` like the existing marginal layout
  path in page runs.
- `eval-pure-world`, `eval-deterministic`, `syntax-*`, `ide-span-contract`:
  unaffected.
- `cli-no-network-default`, `cli-agent-name`, `output-*`, `release-*`:
  unaffected.

## Behavioral compatibility

`block` without `continuation` and `figure` without `caption-repeat` behave
identically to before (verified: full suite, see the visual report).
`figure(caption-repeat: ..)` is the only path that newly enables figure
breakability, and only for figures that opt in.
