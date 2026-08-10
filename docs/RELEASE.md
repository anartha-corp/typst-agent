# Downstream release contract

The first community release is `v0.15.1-agent.0`; later releases use
`v<upstream-version>-agent.<n>`. A release must include:

- `typst-agent` binaries and the compiler image for every supported platform;
- SHA-256 checksums, a CycloneDX SBOM, a Sigstore signature, and build
  provenance tied to the exact workflow run;
- `release-manifest.json` with upstream and downstream object IDs;
- install smoke output and a double-build reproducibility comparison.

The release workflow prepares evidence only. Publishing is protected by a human
environment approval and never has credentials for the `typst/typst` repository.
An incomplete manifest is a failed release, not a warning.
