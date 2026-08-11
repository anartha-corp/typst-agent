#!/usr/bin/env bash
set -euo pipefail

target="${1:?usage: reproducible-build.sh <target> <cargo|cross> <output-directory> [features]}"
builder="${2:?usage: reproducible-build.sh <target> <cargo|cross> <output-directory> [features]}"
output_dir="${3:?usage: reproducible-build.sh <target> <cargo|cross> <output-directory> [features]}"
features="${4:-self-update}"

[[ "$target" =~ ^[A-Za-z0-9_.-]+$ ]]
[[ "$features" =~ ^[A-Za-z0-9_,-]+$ ]]
case "$builder" in
  cargo|cross) ;;
  *) echo "builder must be cargo or cross" >&2; exit 2 ;;
esac

first="$(mktemp -d "${TMPDIR:-/tmp}/typst-agent-build-a.XXXXXX")"
second="$(mktemp -d "${TMPDIR:-/tmp}/typst-agent-build-b.XXXXXX")"
cleanup() { rm -rf -- "$first" "$second"; }
trap cleanup EXIT

source_date_epoch="$(git show -s --format=%ct HEAD)"
downstream_sha="$(git rev-parse HEAD)"
export SOURCE_DATE_EPOCH="$source_date_epoch"
export TYPST_AGENT_COMMIT_SHA="$downstream_sha"

build_once() {
  local target_dir="$1"
  CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$target_dir" \
    "$builder" build --locked --release -p typst-cli --bin typst-agent \
    --target "$target" --features "$features"
}

build_once "$first"
build_once "$second"

suffix=""
[[ "$target" == *windows* ]] && suffix=".exe"
first_bin="$first/$target/release/typst-agent$suffix"
second_bin="$second/$target/release/typst-agent$suffix"
test -f "$first_bin" -a -f "$second_bin"

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

first_sha="$(sha256_file "$first_bin")"
second_sha="$(sha256_file "$second_bin")"
test "$first_sha" = "$second_sha"

mkdir -p "$output_dir"
cp "$first_bin" "$output_dir/typst-agent$suffix"
printf \
  '{"target":"%s","first_sha256":"%s","second_sha256":"%s","identical":true}\n' \
  "$target" "$first_sha" "$second_sha" \
  > "$output_dir/reproducibility-$target.json"
printf 'target=%s\nsource_date_epoch=%s\nsha256=%s\n' \
  "$target" "$source_date_epoch" "$first_sha"
