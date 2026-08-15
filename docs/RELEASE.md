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
versioned image digest. After any first-release visibility bootstrap, it creates
a draft, pushes immutable versioned objects, attaches attestations, and
publishes the GitHub release only after all other writes succeed. A partial
failure therefore leaves the release draft; the tag is never moved and a
versioned container tag is never overwritten.

A partial failure after the draft, tag, and versioned image digests exist can
be resumed from the same `main` commit by dispatching with `resume: true`.
Resume re-verifies that the tag, draft, and all six versioned image refs exist
and point at the exact dispatch commit, reuses them without recreating or
overwriting anything, and continues with signing, attestation, asset upload,
aliasing, and publication. If the objects belong to a different commit, resume
fails loudly instead of mixing provenance.

GitHub creates the first organization-scoped container package as private. On
the first release only, the approved publication job pushes bounded
`visibility-bootstrap` aliases before it creates the release tag, prints the
two package-settings URLs, and waits up to 30 minutes for an organization owner
to make both packages public. Later releases verify the public aliases without
waiting. Successful publication repoints the bootstrap aliases to the final
multi-platform image digests.

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
