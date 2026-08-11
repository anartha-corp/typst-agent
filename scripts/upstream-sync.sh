#!/usr/bin/env bash
set -euo pipefail

# Fetch upstream into dedicated read-only refs and print a deterministic sync
# receipt. This helper never pushes; the GitHub App workflow is the only actor
# allowed to update downstream mirror refs.

fetch_url="$(git remote get-url upstream)"
if [[ "$fetch_url" != https://github.com/typst/typst.git ]]; then
  printf 'unexpected upstream fetch URL: %s\n' "$fetch_url" >&2
  exit 3
fi
git remote set-url --push upstream https://invalid.example/typst/typst.git
git fetch --prune --no-tags upstream \
  refs/heads/main:refs/remotes/upstream/main \
  'refs/tags/*:refs/remotes/upstream-tags/*'

upstream_sha="$(git rev-parse refs/remotes/upstream/main)"
mirror_sha="$(git rev-parse refs/heads/mirror/upstream-main 2>/dev/null || true)"
upstream_tag_count="$(git for-each-ref --format='%(refname)' refs/remotes/upstream-tags | wc -l)"
local_tag_count="$(git for-each-ref --format='%(refname)' refs/tags | wc -l)"
if [[ "$upstream_tag_count" -eq 0 ]]; then
  printf 'upstream tag snapshot is empty\n' >&2
  exit 5
fi

missing_tag_count=0
while IFS=$'\t' read -r upstream_tag_sha tag; do
  git check-ref-format "refs/tags/$tag"
  local_tag_sha="$(git rev-parse "refs/tags/$tag" 2>/dev/null || true)"
  if [[ -z "$local_tag_sha" ]]; then
    missing_tag_count=$((missing_tag_count + 1))
    printf 'missing_tag=%s@%s\n' "$tag" "$upstream_tag_sha"
  elif [[ "$local_tag_sha" != "$upstream_tag_sha" ]]; then
    printf 'tag mismatch for %s: local=%s upstream=%s\n' \
      "$tag" "$local_tag_sha" "$upstream_tag_sha" >&2
    exit 3
  fi
done < <(
  git for-each-ref \
    --format='%(objectname)%09%(refname:strip=3)' \
    refs/remotes/upstream-tags
)

while IFS=$'\t' read -r local_tag_sha tag; do
  upstream_tag_sha="$(git rev-parse "refs/remotes/upstream-tags/$tag" 2>/dev/null || true)"
  if [[ -z "$upstream_tag_sha" ]]; then
    if [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-agent\.[0-9]+$ ]]; then
      continue
    fi
    printf 'downstream-only tag is forbidden: %s@%s\n' "$tag" "$local_tag_sha" >&2
    exit 3
  fi
done < <(git for-each-ref --format='%(objectname)%09%(refname:strip=2)' refs/tags)

printf 'upstream_sha=%s\n' "$upstream_sha"
printf 'mirror_sha=%s\n' "${mirror_sha:-missing}"
printf 'upstream_tag_count=%s\n' "$upstream_tag_count"
printf 'local_tag_count=%s\n' "$local_tag_count"
printf 'missing_tag_count=%s\n' "$missing_tag_count"
if [[ "$mirror_sha" == "$upstream_sha" && "$missing_tag_count" -eq 0 ]]; then
  printf 'status=up-to-date\n'
else
  printf 'status=update-required\n'
fi
