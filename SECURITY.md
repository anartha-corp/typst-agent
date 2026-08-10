# Security policy

Typst Agent is an independent downstream of Typst and has no affiliation with
Typst GmbH. Do not report downstream vulnerabilities to the upstream project.

## Reporting

Please use GitHub's private security advisory flow for this repository. If that
is unavailable, contact the maintainers listed in `.github/CODEOWNERS` with a
minimal reproduction and the affected commit or release. Do not include secrets
in an issue or pull request.

We acknowledge reports within seven days, keep the report private while a fix is
prepared, and publish a coordinated advisory after users have a remediation.
Security fixes require the normal human review and release evidence; no
automation may bypass branch protection.

## Supply-chain boundaries

The `upstream` remote is fetch-only and has an invalid push URL. CI and release
helpers must not hold credentials capable of writing to `typst/typst`. Release
artifacts include provenance, checksums, and a manifest connecting the upstream
and downstream object IDs.
