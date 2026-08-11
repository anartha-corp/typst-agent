# Upstream synchronization authority

Upstream synchronization is review-only. The workflow fetches
`https://github.com/typst/typst.git` into dedicated local refs, keeps the
upstream push URL inert, and may write only downstream mirror tags, the
`mirror/upstream-main` branch, and one deterministic compatibility branch.

## GitHub App

`typst-agent-sync` is the sole automation identity allowed to update mirror
refs. A human organization owner creates the App and installs it only on
`anartha-corp/typst-agent` with these repository permissions:

- Contents: read and write.
- Pull requests: read and write.
- Every other repository and organization permission: no access.

The repository variable `TYPST_AGENT_SYNC_CLIENT_ID` contains the App client
ID. The repository secret `TYPST_AGENT_SYNC_PRIVATE_KEY` contains its private
key. The workflow requests an installation token scoped again to the single
`typst-agent` repository and fails unless the returned App slug is exactly
`typst-agent-sync`.

The `upstream-sync-human-approval` environment uses the
`typst-agent-maintainers` team as its required reviewer and has self-review
prevention enabled. Compatibility PRs are App-authored, so `rixzkiye` can give
the exact-head environment approval and the separate native CODEOWNER review.

## Fail-closed sequence

1. `scripts/upstream-sync.sh` fetches main and tags into
   `refs/remotes/upstream/main` and `refs/remotes/upstream-tags/*`. It never
   pushes or calls an API.
2. Existing upstream tag names must resolve to the same Git object. New tags
   are pushed without force. Any deletion or mismatch stops the run before a
   tag write; strict downstream release tags such as `v0.15.1-agent.0` remain
   outside the upstream mirror set.
3. The mirror branch is pushed to the fetched new SHA with an exact
   `force-with-lease` against a freshly read downstream SHA, then read back.
4. One branch named `execute-plan/upstream-sync-<sha12>` is created. A closed
   PR must be reopened by a human; automation never creates a duplicate.
5. The PR carries strict upstream provenance and is gated by `Upstream
   compatibility`, which runs full verification, invariant drift, a current
   review pack, and human environment approval. It never auto-merges.

App creation and installation are deliberately not automated: GitHub does not
provide a repository REST operation that can create a GitHub App or transfer
human approval authority to a workflow.
