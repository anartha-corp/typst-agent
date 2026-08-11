# Downstream release contract

The first community release is `v0.15.1-agent.0`; later releases use
`v<upstream-version>-agent.<n>`. A release must include:

- `typst-agent` binaries and the compiler image for every supported platform;
- SHA-256 checksums, a CycloneDX SBOM, Sigstore bundles, and build
  provenance tied to the exact workflow run;
- `release-manifest.json` with upstream and downstream object IDs;
- install smoke output and a double-build reproducibility comparison.

The bot dispatches the workflow only from the exact `main` commit. Preparation
jobs have no repository or package publication permission. They double-build
every binary target, export both container images as OCI archives, run native
and QEMU smoke checks, and assemble a strict release manifest before any human
gate is opened.

The publication job is protected by the `release-human-approval` environment.
It refuses a moved source commit, an existing mismatched tag, or an existing
versioned image digest. It creates a draft first, pushes immutable versioned
objects, attaches attestations, and publishes the GitHub release only after all
other writes succeed. A partial failure therefore leaves the release draft;
the tag is never moved and a versioned container tag is never overwritten.

No release job has a credential or remote capable of writing to `typst/typst`.
An incomplete, null, or placeholder manifest is a failed release, not a warning.

The byte-preserved upstream `docker-image.yml` workflow must be disabled in the
downstream repository before the first release. `release.yml` is the only image
publisher; disabling the retained upstream workflow prevents an ungated rerun
from replacing an immutable versioned image digest.

Repository setup for this workflow is deliberately narrow:

- the dispatch actor is exactly `typst-agent-pr-bot[bot]`;
- `release-human-approval` requires the `typst-agent-maintainers` team, prevents
  self-review, and allows only the selected `main` branch;
- the App can read contents and write pull requests and Actions, but it has no
  package, release, environment, or administration permission;
- the publication job receives its scoped `contents`, `packages`, attestation,
  and OIDC permissions only after the protected environment is approved by
  `rixzkiye`.
