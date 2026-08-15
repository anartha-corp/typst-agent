# Golden backlog mining

Authority is `.agents/backlog/registry.toml` and the deterministic scorer in
`crates/typst-agent-dev`; this guide explains intent and must not override
them. The pipeline turns upstream `typst/typst` issues into scoped downstream
patches that are designed to be dropped when upstream ships a canonical
equivalent.

## Sources

`scripts/backlog-fetch.sh` snapshots, under `.tmp/agent/backlog/raw/` (ignored):
open issues, open upstream PRs with their linked issues, closed "not planned"
issues, maintainer logins, and maintainer comments on the top demand issues.
The script only reads GitHub and never writes to `typst/typst`.

## Scoring

`cargo agent backlog` computes, for every registry entry:

```text
score = (user demand x implementation confidence x compatibility safety
         x ecosystem impact) / long-term maintenance burden
```

Demand is derived from snapshot reactions/comments (5: >=100 or >=30; 4: >=40
or >=20; 3: >=15 or >=10; 2: >=5 or >=4; else 1). The other factors are
annotated 1-5 in the registry. Tiers: `a` >= 120, `b` >= 48, `c` below.
Hard exclusions win over any score: registry `exclude_reason`, upstream
"not planned" closure, and open upstream PRs updated within 180 days that
link the issue. Calibration references (#2722, #6059) must stay in tier a/b
and known-bad issues (#1765, #955, #5382) must stay excluded.

## Lifecycle

`candidate` -> `mined` -> `shipped` | `watch` | `excluded` | `upstream-shipped`.
Every shipped mine keeps a drop-when-upstream plan: drop the patch, add a
deprecated compat alias, remove the alias after two minor releases.

## Checks

`cargo agent backlog --self-check` is required for this area. Never mine a
hard-excluded issue without a recorded human override.
