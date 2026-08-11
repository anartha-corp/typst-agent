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

suffix=""
[[ "$target" == *windows* ]] && suffix=".exe"
scratch_root=".tmp/agent/reproducible-build/$target"
target_dir="$scratch_root/target"
first_bin_copy="$scratch_root/typst-agent-first$suffix"
cleanup() { rm -rf -- "$scratch_root"; }
trap cleanup EXIT
rm -rf -- "$scratch_root"
mkdir -p "$scratch_root"

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

build_once "$target_dir"
first_bin="$target_dir/$target/release/typst-agent$suffix"
test -f "$first_bin"

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

first_sha="$(sha256_file "$first_bin")"
cp "$first_bin" "$first_bin_copy"
rm -rf -- "$target_dir"
build_once "$target_dir"
second_bin="$target_dir/$target/release/typst-agent$suffix"
test -f "$second_bin"
second_sha="$(sha256_file "$second_bin")"
if [[ "$first_sha" != "$second_sha" ]]; then
  printf \
    'non-reproducible binary for %s: first_sha256=%s second_sha256=%s\n' \
    "$target" "$first_sha" "$second_sha" >&2
  exit 4
fi

mkdir -p "$output_dir"
cp "$first_bin_copy" "$output_dir/typst-agent$suffix"
printf \
  '{"target":"%s","first_sha256":"%s","second_sha256":"%s","identical":true}\n' \
  "$target" "$first_sha" "$second_sha" \
  > "$output_dir/reproducibility-$target.json"
printf 'target=%s\nsource_date_epoch=%s\nsha256=%s\n' \
  "$target" "$source_date_epoch" "$first_sha"
