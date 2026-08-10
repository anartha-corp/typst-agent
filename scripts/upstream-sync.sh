#!/usr/bin/env bash
set -euo pipefail

# Fetch upstream and print a deterministic sync receipt. This helper never
# pushes to upstream; callers may update only the downstream mirror ref.

git remote get-url upstream >/dev/null
git remote set-url --push upstream https://invalid.example/typst/typst.git
git fetch --tags upstream main

upstream_sha="$(git rev-parse refs/remotes/upstream/main)"
mirror_sha="$(git rev-parse refs/heads/mirror/upstream-main 2>/dev/null || true)"
printf 'upstream_sha=%s\nmirror_sha=%s\n' "$upstream_sha" "${mirror_sha:-missing}"
if [[ -n "$mirror_sha" && "$mirror_sha" == "$upstream_sha" ]]; then
  printf 'status=up-to-date\n'
else
  printf 'status=update-required\n'
fi
