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
commits arrive. Automation may perform the mechanical merge or publication only
after the applicable human approval; automation never supplies that approval.

### Single-owner bootstrap

PR #1 through #7 use a temporary environment approval because the repository
has one human owner and a pull-request author cannot supply an independent
native review. The `merge-single-owner` environment is restricted to the
maintainer team and, for these seven PRs only, permits self-review. Its required
`Human owner approval` check is bound to the exact pull-request head SHA and
revalidates the open PR, its `main` base, and every other required check after
the environment approval. A new commit therefore requires a new workflow run
and a new approval.

After PR #7 lands, branch protection switches to one native CODEOWNER approval
with stale-review dismissal and conversation resolution. The bootstrap check is
removed from protection, and the workflow is deleted by the first bot-authored
release PR. The environment remains disabled as an audit record and cannot be
used for later pull requests.

## Contributions

Contributors sign off each commit under the Developer Certificate of Origin
(see [`DCO`](DCO)). External AI-assisted contributions are allowed when the
human proposer remains accountable for the code, test evidence, and disclosure.
No contribution may contain credentials, proprietary Akademi material, or code
intended for submission to `typst/typst`.
