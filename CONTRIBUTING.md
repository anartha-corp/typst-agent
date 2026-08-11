# Contributing to Typst Agent

Please read [`AGENTS.md`](AGENTS.md), [`GOVERNANCE.md`](GOVERNANCE.md), and
[`DCO`](DCO) before opening a pull request. This is an independent downstream:
do not open an upstream PR, copy credentials, or include proprietary material.

## Pull requests

Keep each PR focused and use the branch form
`execute-plan/typst-agent-native-pr-<n>-<slug>`. Describe the user-visible or
maintenance goal, changed invariants, test evidence, and upstream provenance.
Include the AI disclosure from [`AI_DISCLOSURE.md`](AI_DISCLOSURE.md) whenever
AI assistance was used. A human proposer is responsible for the entire change,
including generated code.

Run the smallest relevant checks before requesting review:

```sh
cargo agent doctor
cargo agent policy-check
cargo agent verify --tier fast
cargo agent review-pack --base main
```

Do not update a reference image or hash merely to make a test pass. Such a
change needs a visual report, invariant impact, and explicit human approval.

## Commit sign-off

Every commit must include a DCO sign-off (`git commit -s`). Commits should be
atomic and independently reviewable. Merge and release require a human
CODEOWNER approval; automation may report evidence but never approves or merges.
