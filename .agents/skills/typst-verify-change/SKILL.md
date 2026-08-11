# typst-verify-change

Select the smallest truthful tier: `fast` for local lint and focused tests,
`pr` for diff-aware tests and evidence, and `full` for release or upstream
synchronization. Pass `--base <ref>` so PR/full selection includes the committed
three-dot diff and dirty worktree. Verify a clean/contained worktree, preserve
test output under `.tmp/agent/`, and report unavailable host tools as
unavailable rather than green.
