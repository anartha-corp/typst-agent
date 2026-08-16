# typst-mine-backlog

Use to mine upstream `typst/typst` issues into the downstream golden backlog.
Authority is `.agents/areas/backlog.md`; this skill is the short procedure.

## Langkah

1. `cargo agent policy-check` — the upstream push URL must remain inert.
2. `scripts/backlog-fetch.sh` — snapshots upstream issues, PRs, maintainer
   comments, per-issue comments, timeline cross-references, and closed "not
   planned" issues under `.tmp/agent/backlog/raw/`. It never writes to
   `typst/typst`.
3. `cargo agent backlog --self-check` — deterministic scoring; calibration
   references (#2722, #6059) must land in tier a/b and known-bad issues
   (#1765, #955, #5382) must be excluded. Closed upstream issues are listed
   as `upstream_closed`, outdated annotations as `stale`.
4. `cargo agent backlog` — ranked report plus `.tmp/agent/backlog.json`
   evidence for the weekly curation review.
5. Curate: pick at most two tier-a/b candidates. For each one run
   `cargo agent backlog --investigate <n>` to build the context pack and the
   annotation proposal template. An agent or human fills the proposal, which
   lands in `.agents/backlog/registry.toml` only through a reviewed PR.
6. `cargo agent backlog --audit` — every `shipped` entry must have a commit
   tagged `(#NNNN)` and every `mined` entry a downstream PR. Fix violations
   before merging registry changes.
7. Implementation follows `typst-plan-change` on a
   `execute-plan/typst-agent-native-pr-<n>-<slug>` branch with commit tags
   `(#NNNN)` and an `Upstream issue:` design note. When upstream ships an
   equivalent, follow the drop-when-upstream plan from the registry.

Never mine a hard-excluded issue without a recorded human override, and never
send code or automation to `typst/typst`.

## Stop

Serahkan ke manusia pemilihan kandidat, keputusan API, override hard-exclude,
dan setiap keputusan drop/alias saat upstream menutup issue.
