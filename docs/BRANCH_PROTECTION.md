# Protected branch contract

PR #1 through #7 use `.github/branch-protection.bootstrap.json`. It is strict,
applies to administrators, forbids force-push/delete, and requires the fast
policy/agent lane, conditional reference evidence, DCO, and the exact-head
`Human owner approval` check. The bootstrap gate validates human authority and
the current head only; branch protection independently combines it with the
required checks, so approval may happen before or after CI.

After PR #7 lands, apply `.github/branch-protection.json`. The final state adds
one native CODEOWNER approval, stale-review dismissal, and conversation
resolution, and removes the bootstrap owner check. Ordinary PRs block only on
the fast policy/agent lane, conditional reference evidence, and DCO. The full
workspace and OS matrices, docs, clippy variants, MSRV, fuzz, Miri, CodeQL, and
dependency review remain visible evidence but are non-blocking for ordinary
merges.

Release publication keeps its separate protected environment, immutable
artifact identity, provenance, SBOM, reproducibility, and human approval. The
upstream push boundary remains fail-closed. For stacked work, only retarget and
rebase the next PR when it is ready to merge; descendants do not need eager CI
or ancestry rewrites.

Both JSON files are reviewable desired-state inputs for the GitHub branch
protection API. A human applies phase transitions; workflows never weaken or
bypass the protected branch themselves.
