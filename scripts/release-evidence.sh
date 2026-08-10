#!/usr/bin/env bash
set -euo pipefail

artifact_dir="${1:?usage: release-evidence.sh <artifact-directory> <output-directory>}"
output_dir="${2:?usage: release-evidence.sh <artifact-directory> <output-directory>}"
mkdir -p "$output_dir"

test -d "$artifact_dir"
find "$artifact_dir" -type f -print0 | sort -z | xargs -0 sha256sum > "$output_dir/SHA256SUMS"
cargo agent release-manifest --format json > "$output_dir/release-manifest.receipt.json"
cp .tmp/agent/release-manifest.json "$output_dir/release-manifest.json"

if ! command -v syft >/dev/null 2>&1; then
  echo 'syft is required for the release SBOM' >&2
  exit 5
fi
if ! command -v cosign >/dev/null 2>&1; then
  echo 'cosign is required for the release signature' >&2
  exit 5
fi
syft "dir:$artifact_dir" -o cyclonedx-json > "$output_dir/sbom.cyclonedx.json"
cosign sign-blob --yes --output-signature "$output_dir/SHA256SUMS.sig" "$output_dir/SHA256SUMS"
