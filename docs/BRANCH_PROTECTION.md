# Protected branch contract

PR #1 through #7 use `.github/branch-protection.bootstrap.json`. It is strict,
applies to administrators, forbids force-push/delete, requires every preserved
upstream CI aggregate, and uses the exact-head `Human owner approval` check
instead of a native approving-review count.

After PR #7 lands, apply `.github/branch-protection.json`. The final state adds
all downstream CI/security checks, one native CODEOWNER approval, stale-review
dismissal, and conversation resolution. The bootstrap owner check is removed.

Both JSON files are reviewable desired-state inputs for the GitHub branch
protection API. A human applies phase transitions; workflows never weaken or
bypass the protected branch themselves.
