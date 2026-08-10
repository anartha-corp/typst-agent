#!/usr/bin/env bash
set -euo pipefail

repo="$(git rev-parse --show-toplevel)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/typst-agent-eval.XXXXXX")"
cleanup() {
  git -C "$repo" worktree remove --force "$tmp" >/dev/null 2>&1 || true
  rmdir "$tmp" >/dev/null 2>&1 || true
}
trap cleanup EXIT

git -C "$repo" worktree add --detach "$tmp" HEAD >/dev/null
git -C "$tmp" remote add upstream https://github.com/typst/typst.git
git -C "$tmp" remote set-url --push upstream https://invalid.example/typst/typst.git

(cd "$tmp" && cargo agent eval --format json)
test "$(find "$tmp/evals/tasks" -name '*.toml' | wc -l)" -eq 10
printf 'disposable_worktree=removed\ntasks=10\nmodel_calls=0\n'
