#!/usr/bin/env bash
set -euo pipefail

first="$(mktemp -d "${TMPDIR:-/tmp}/typst-agent-build-a.XXXXXX")"
second="$(mktemp -d "${TMPDIR:-/tmp}/typst-agent-build-b.XXXXXX")"
cleanup() { rm -rf "$first" "$second"; }
trap cleanup EXIT

source_date_epoch="$(git show -s --format=%ct HEAD)"
export SOURCE_DATE_EPOCH="$source_date_epoch"
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$first" cargo build --locked --release --bin typst-agent
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$second" cargo build --locked --release --bin typst-agent

first_bin="$first/release/typst-agent"
second_bin="$second/release/typst-agent"
test -f "$first_bin" -a -f "$second_bin"
first_sha="$(sha256sum "$first_bin" | cut -d' ' -f1)"
second_sha="$(sha256sum "$second_bin" | cut -d' ' -f1)"
printf 'source_date_epoch=%s\nfirst_sha256=%s\nsecond_sha256=%s\n' "$source_date_epoch" "$first_sha" "$second_sha"
test "$first_sha" = "$second_sha"
