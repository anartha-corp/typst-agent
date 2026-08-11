# Release and supply chain

Authority is the release workflow, `Dockerfile`, package metadata, and the Git
object graph. A release is downstream-only, uses `v<upstream-version>-agent.<n>`,
and contains checksums, signatures, SBOM, provenance, and upstream/downstream
SHAs. Automation prepares artifacts but a human environment approval is needed
to publish.

Run `cargo agent upstream-check` and `cargo agent policy-check` before a release
review. After the preparation jobs have assembled bounded evidence, run
`cargo agent release-manifest --input .tmp/agent/release/release-input.json`;
missing preparation evidence is an unavailable-authority failure.
