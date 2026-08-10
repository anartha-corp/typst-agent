# Governance

Typst Agent is an independent community downstream. The project is not
affiliated with, sponsored by, or endorsed by Typst GmbH, and it does not submit
changes to the upstream project.

## Maintainers

Maintainers are listed in `.github/CODEOWNERS` and are responsible for review,
security response, release approval, and keeping the downstream/upstream
boundary intact. A maintainer may delegate implementation, but may not delegate
the final human approval for a merge or release.

## Decision process

Changes should be proposed as focused pull requests. A PR must include scope,
tests/evidence, invariant impact, and an AI disclosure when applicable. At least
one CODEOWNER human approval is required; approvals are dismissed when new
commits arrive. Merge and release are never performed by automation.

## Contributions

Contributors sign off each commit under the Developer Certificate of Origin
(see [`DCO`](DCO)). External AI-assisted contributions are allowed when the
human proposer remains accountable for the code, test evidence, and disclosure.
No contribution may contain credentials, proprietary Akademi material, or code
intended for submission to `typst/typst`.
