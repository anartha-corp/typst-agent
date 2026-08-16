# Agent navigation index

Run `cargo agent doctor`, then route every changed path through
[`area-manifest.json`](area-manifest.json). That JSON record is the sole routing
authority for area IDs, path rules, source authorities, guides, checks, and
invariant IDs. Markdown guides explain intent but do not override source, tests,
Cargo metadata, the manifest, or ordered upstream history.

The invariant registry is [`invariants.yml`](invariants.yml). Cross-cutting
rules live in [`AGENTS.md`](../AGENTS.md), and all portable record shapes are
strictly defined by [`agent-contract/v1/schema.json`](../agent-contract/v1/schema.json).

Golden-backlog mining is governed by [`areas/backlog.md`](areas/backlog.md), the
registry at [`backlog/registry.toml`](backlog/registry.toml), and the
[`typst-mine-backlog`](skills/typst-mine-backlog/SKILL.md) skill.
