# typst-release

Release tags are `v<upstream-version>-agent.<n>`. Run policy, upstream, full
verification, and release-manifest checks. Attach checksums, Sigstore
signatures, SBOM, provenance, source SHAs, install smoke output, and a
double-build comparison. A human environment approval is required to publish;
the skill never creates or pushes a release on its own.
