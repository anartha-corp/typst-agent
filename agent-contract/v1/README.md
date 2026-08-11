# Agent contract v1

This directory is the portable, model-free contract for development agents.
The schema defines eight discriminated records: `AreaManifest`,
`InvariantRecord`, `TaskContract`, `ImpactReport`, `VerificationEvidence`,
`ReviewEvidence`, `UpstreamProvenance`, and `ReleaseManifest`. Every payload is
closed: missing or unknown fields are rejected. Records describe evidence and
scope; they do not grant authority to merge, release, or publish.

The contract is intentionally independent of Codex. Any agent can emit the same
JSON records and consume the deterministic `cargo agent` commands.
