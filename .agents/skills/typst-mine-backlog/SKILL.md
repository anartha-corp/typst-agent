# typst-mine-backlog

Use to mine upstream `typst/typst` issues into the downstream golden backlog.
Authority is `.agents/areas/backlog.md`; this skill is the short procedure.

## Langkah

1. `cargo agent policy-check` — the upstream push URL must remain inert.
2. `scripts/backlog-fetch.sh` — snapshots upstream issues, PRs, maintainer
   comments, and closed "not planned" issues under `.tmp/agent/backlog/raw/`.
   It never writes to `typst/typst`.
3. `cargo agent backlog --self-check` — deterministic scoring; calibration
   references (#2722, #6059) must land in tier a/b and known-bad issues
   (#1765, #955, #5382) must be excluded.
4. `cargo agent backlog` — ranked report plus `.tmp/agent/backlog.json`
   evidence for the weekly curation review.
5. Curate: pick at most two tier-a/b candidates, record acceptance criteria
   and the API shape in `.agents/backlog/registry.toml`, then follow
   `typst-plan-change` for the implementation branch. Each patch ships on a
   `execute-plan/typst-agent-native-pr-<n>-<slug>` branch with commit tags
   `(#NNNN)` and an `Upstream issue:` design note.

Never mine a hard-excluded issue without a recorded human override, and never
send code or automation to `typst/typst`.

## Stop

Serahkan ke manusia pemilihan kandidat, keputusan API, dan setiap override
hard-exclude.
