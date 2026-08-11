# Typst Agent repository contract

Typst Agent is an unofficial, public downstream of Typst. This repository may
contain AI-assisted changes, but the compiler and all published runtime
artifacts remain model-free. No source, credential, workflow, helper, or remote
in this repository may write to `typst/typst`.

## Authority and scope

1. Rust source, ordered tests, Cargo metadata, and the upstream mirror are the
   authority. Documentation describes intent and must not override executable
   behavior.
2. `agent-contract/v1/` contains the versioned machine-readable development
   contract. `.agents/areas/` and `.agents/invariants/` provide progressive
   guidance; keep this file short.
3. Before editing, run `cargo agent doctor` and inspect the relevant area guide.
   Keep changes on a task branch named
   `execute-plan/typst-agent-native-pr-<n>-<slug>`.
4. Stage only task-owned paths. Inspect `git diff --cached` before each atomic
   commit. Never stage credentials, generated output, or unrelated work.

## Agent control plane

`cargo agent` is deterministic, model-free, and local-only. It may read source,
Cargo metadata, and Git history, and may write bounded evidence under
`.tmp/agent/` (which is ignored). It never pushes, merges, publishes, changes a
reference baseline, or calls a model.

Available commands:

```text
cargo agent doctor
cargo agent context --paths <path>... --format human|json
cargo agent impact --base <ref>
cargo agent verify --tier fast|pr|full
cargo agent review-pack --base <ref>
cargo agent policy-check
cargo agent upstream-check
cargo agent eval
cargo agent release-manifest
```

Exit codes are stable: `0` success, `2` invalid invocation or contract input,
`3` policy violation, `4` verification failure, and `5` unavailable authority
(for example a missing upstream mirror). JSON output is bounded and stable so
other agents can consume it without depending on Codex.

## Upstream boundary

`upstream` must fetch from `https://github.com/typst/typst.git` and have an
invalid push URL. `mirror/upstream-main` and mirrored upstream tags contain no
downstream commit. Synchronization is review-only: it produces evidence and a
human-approved pull request; it never auto-merges or submits code upstream.

## Human authority

Humans own scope, review, merge, release, and incident response. AI assistance
must be disclosed according to [`AI_DISCLOSURE.md`](AI_DISCLOSURE.md). A passing
tool result is evidence, not approval. Reference-image or hash changes require
an invariant impact note, a visual report, and explicit human approval.

See the scoped guides in [`.agents/areas/`](.agents/areas/) and the invariant
registry in [`.agents/invariants/`](.agents/invariants/).
