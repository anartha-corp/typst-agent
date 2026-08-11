#!/usr/bin/env bash
set -euo pipefail

owner="${GITHUB_REPOSITORY_OWNER,,}"
release_root="${RELEASE_ROOT:-.tmp/agent/release}"
attempts="${GHCR_VISIBILITY_ATTEMPTS:-180}"
interval="${GHCR_VISIBILITY_INTERVAL_SECONDS:-10}"
bootstrap_tag=visibility-bootstrap

[[ "$owner" =~ ^[a-z0-9][a-z0-9-]*$ ]]
[[ "$attempts" =~ ^[1-9][0-9]*$ ]]
[[ "$interval" =~ ^[0-9]+$ ]]

compiler_repository="$owner/typst-agent"
dev_repository="$owner/typst-agent-dev"

public_manifest_available() {
  local repository="$1"
  local token
  token="$(
    curl --fail --silent --get https://ghcr.io/token \
      --data-urlencode service=ghcr.io \
      --data-urlencode "scope=repository:$repository:pull" \
      | jq -er .token
  )" || return 1
  curl --fail --silent \
    --header "Authorization: Bearer $token" \
    --header 'Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.v2+json' \
    "https://ghcr.io/v2/$repository/manifests/$bootstrap_tag" \
    >/dev/null
}

if public_manifest_available "$compiler_repository" && \
  public_manifest_available "$dev_repository"; then
  printf 'GHCR namespaces are already public\n'
  exit 0
fi

bootstrap_package() {
  local kind="$1"
  local local_repository="$2"
  local remote_repository="$3"
  local archive="$release_root/artifacts/typst-agent-$kind-linux-amd64.docker.tar"
  local local_ref="$local_repository:prepared-linux-amd64"
  local remote_ref="ghcr.io/$remote_repository:$bootstrap_tag"
  test -s "$archive"
  docker load --input "$archive"
  docker tag "$local_ref" "$remote_ref"
  docker push "$remote_ref"
}

bootstrap_package compiler typst-agent-compiler "$compiler_repository"
bootstrap_package dev typst-agent-dev "$dev_repository"

compiler_settings="https://github.com/orgs/$owner/packages/container/typst-agent/settings"
dev_settings="https://github.com/orgs/$owner/packages/container/typst-agent-dev/settings"
{
  printf '## First-release GHCR visibility\n\n'
  printf 'GitHub creates new organization packages as private. Change both packages to Public:\n\n'
  printf -- '- %s\n' "$compiler_settings"
  printf -- '- %s\n' "$dev_settings"
} >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
printf 'Change both GHCR packages to Public before this bounded wait expires:\n%s\n%s\n' \
  "$compiler_settings" "$dev_settings"

for ((attempt = 1; attempt <= attempts; attempt++)); do
  if public_manifest_available "$compiler_repository" && \
    public_manifest_available "$dev_repository"; then
    printf 'both GHCR namespaces are publicly readable\n'
    exit 0
  fi
  if ((attempt == 1 || attempt % 6 == 0)); then
    printf 'waiting for public GHCR visibility (%s/%s)\n' "$attempt" "$attempts"
  fi
  sleep "$interval"
done

printf 'GHCR packages did not become public within the bounded wait\n' >&2
exit 5
