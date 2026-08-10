# Agent contract v1

This directory is the portable, model-free contract for development agents.
Schemas describe evidence and scope; they do not grant authority to merge,
release, or publish. Unknown fields are rejected by the local validator so a
fresh clone fails closed instead of silently ignoring a policy change.

The contract is intentionally independent of Codex. Any agent can emit the same
JSON records and consume the deterministic `cargo agent` commands.
