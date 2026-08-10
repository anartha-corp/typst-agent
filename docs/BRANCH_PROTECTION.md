# Protected branch contract

`main` must be protected with the settings recorded in
`.github/branch-protection.json`. The JSON is a reviewable desired-state file;
an owner applies it through the GitHub API after the public repository exists.

Required checks cover the downstream policy/fast lane, the complete upstream
workspace matrix, clippy with and without defaults, formatting, MSRV, fuzz
build, Miri, CodeQL, dependency review, and DCO. One human CODEOWNER approval is
required, stale approvals are dismissed after new commits, and administrators
are subject to the same checks. No workflow may merge or publish a release.
