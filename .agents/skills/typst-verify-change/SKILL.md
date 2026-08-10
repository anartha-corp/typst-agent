# typst-verify-change

Select the smallest truthful tier: `fast` for local lint and focused tests,
`pr` for diff-aware tests and evidence, and `full` for release or upstream
synchronization. Verify a clean/contained worktree, preserve test output under
`.tmp/agent/`, and report unavailable host tools as unavailable rather than
green.
