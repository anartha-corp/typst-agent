# CLI

Authority is `crates/typst-cli/src/` and CLI integration tests. Keep the
compiler local by default; network and self-update behavior must be explicit.
Published downstream artifacts identify as `typst-agent` and state that they are
unofficial. Do not add agent or LLM runtime dependencies.

Required checks: `cargo test -p typst-cli`, `cargo agent policy-check`, and
`cargo agent release-manifest` when package metadata changes.
