# Golden backlog mining

Authority is `.agents/backlog/registry.toml` and the deterministic scorer in
`crates/typst-agent-dev`; this guide explains intent and must not override
them. The pipeline turns upstream `typst/typst` issues into scoped downstream
patches that are designed to be dropped when upstream ships a canonical
equivalent.

## Sources

`scripts/backlog-fetch.sh` snapshots, under `.tmp/agent/backlog/raw/` (ignored):
open issues, open upstream PRs with their linked issues, closed "not planned"
issues, maintainer logins, maintainer comments, per-issue comments and
timeline cross-references for the registry plus the top demand issues. The
script only reads GitHub and never writes to `typst/typst`.

## Scoring

`cargo agent backlog` computes, for every registry entry:

```text
score = (user demand x implementation confidence x compatibility safety
         x ecosystem impact) / long-term maintenance burden
```

Demand is derived from snapshot reactions/comments (5: >=100 or >=30; 4: >=40
or >=20; 3: >=15 or >=10; 2: >=5 or >=4; else 1). The other factors are
annotated 1-5 in the registry; `stance` must be one of `endorsing`,
`neutral`, `skeptical`, `planned`, `none`. Tiers: `a` >= 120, `b` >= 48, `c`
below. Hard exclusions win over any score: registry `exclude_reason`, upstream
"not planned" closure, and open upstream PRs updated within 180 days that
link the issue. Calibration references (#2722, #6059) must stay in tier a/b
and known-bad issues (#1765, #955, #5382) must stay excluded. Entries whose
upstream issue is closed are reported as `upstream_closed`, and annotations
older than 28 days relative to the snapshot as `stale`.

## Curation

`cargo agent backlog --investigate <n>` builds a deterministic context pack
(issue meta, maintainer and all comments, cross-references, earlier mines in
the same subsystem, area guide) plus an annotation proposal template under
`.tmp/agent/backlog/`. An LLM or a human fills the proposal; it lands in the
registry only through a reviewed PR. The scorer never calls a model.

`cargo agent backlog --audit` cross-checks lifecycles against git history:
`shipped` needs a downstream PR and a commit tagged `(#NNNN)`, `mined` needs
a downstream PR, unworked statuses must not carry one.

## Lifecycle

`candidate` -> `mined` -> `shipped` | `watch` | `excluded` | `upstream-shipped`.
Every shipped mine keeps a drop-when-upstream plan: drop the patch, add a
deprecated compat alias, remove the alias after two minor releases. When the
snapshot reports an entry as `upstream_closed`, a human decides between
dropping the patch and keeping an alias.

## Checks

`cargo agent backlog --self-check` is required for this area. Never mine a
hard-excluded issue without a recorded human override.

## Known limits (v2 directions)

The v1 scorer is deliberate ranking of curated candidates, not autonomous
mining; the annotated factors are the human/agent judgment slots. Known
weaknesses to address in later versions, always as deterministic additions
rather than a black-box score:

- Demand via reactions/comments is a rough proxy: a flashy 120-thumbs-up issue
  can be niche while a 4-thumbs-up bug can break a whole document class.
  Candidate improvements: duplicate-issue clustering, forum/discord mentions,
  affected-package counts.
- The upstream-PR exclusion only sees PRs that link the issue and were updated
  within 180 days. It can miss unlinked fix PRs, design work in discussions,
  or resumed stale PRs; the weekly snapshot mitigates this only partially.
- The multiplicative formula punishes a single low factor hard (safety=1 kills
  the score). This is intentional conservatism, but thresholds (120/48) need
  recalibration as the registry grows.
- Future signals worth adding: maintainer sentiment extraction, dependency
  ownership detection, regression severity, testability, patch deletability,
  API-surface cost, and affected user class.
