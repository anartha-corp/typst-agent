#!/usr/bin/env bash
set -euo pipefail

release_root=".tmp/agent/release"
artifact_dir="$release_root/artifacts"
reproducibility_dir="$release_root/reproducibility"
smoke_dir="$release_root/smoke"
evidence_dir="$release_root/evidence"
release_tag="${RELEASE_TAG:?RELEASE_TAG is required}"
downstream_sha="${GITHUB_SHA:-$(git rev-parse HEAD)}"
repository="${GITHUB_REPOSITORY:-anartha-corp/typst-agent}"
run_id="${GITHUB_RUN_ID:-local}"
run_attempt="${GITHUB_RUN_ATTEMPT:-1}"

test "$(git rev-parse --show-toplevel)" = "$PWD"
test -d "$artifact_dir" -a -d "$reproducibility_dir" -a -d "$smoke_dir"
mkdir -p "$evidence_dir"

if ! command -v syft >/dev/null 2>&1; then
  echo 'syft is required for the release SBOM' >&2
  exit 5
fi
if ! command -v cosign >/dev/null 2>&1; then
  echo 'cosign is required for the release Sigstore bundle' >&2
  exit 5
fi

mapfile -d '' artifact_files < <(find "$artifact_dir" -maxdepth 1 -type f -size +0c -print0 | sort -z)
if [[ "${#artifact_files[@]}" -eq 0 ]]; then
  echo 'release artifacts are missing' >&2
  exit 5
fi

(
  cd "$artifact_dir"
  find . -maxdepth 1 -type f -size +0c -printf '%P\0' \
    | sort -z \
    | xargs -0 sha256sum
) > "$evidence_dir/SHA256SUMS"

syft "dir:$artifact_dir" -o cyclonedx-json > "$evidence_dir/sbom.cyclonedx.json"
cosign sign-blob --yes \
  --bundle "$evidence_dir/SHA256SUMS.sigstore.json" \
  "$evidence_dir/SHA256SUMS" >/dev/null

subjects="$({
  while read -r digest name; do
    jq -cn --arg name "$name" --arg digest "$digest" \
      '{name:$name,digest:{sha256:$digest}}'
  done < "$evidence_dir/SHA256SUMS"
} | jq -s .)"

jq -n \
  --arg repository "$repository" \
  --arg release_tag "$release_tag" \
  --arg downstream_sha "$downstream_sha" \
  --arg run_id "$run_id" \
  --arg run_attempt "$run_attempt" \
  --argjson subject "$subjects" \
  '{
    _type:"https://in-toto.io/Statement/v1",
    subject:$subject,
    predicateType:"https://slsa.dev/provenance/v1",
    predicate:{
      buildDefinition:{
        buildType:"https://github.com/anartha-corp/typst-agent/.github/workflows/release.yml@v1",
        externalParameters:{release_tag:$release_tag},
        internalParameters:{repository:$repository,run_id:$run_id,run_attempt:$run_attempt},
        resolvedDependencies:[{uri:("git+https://github.com/"+$repository+".git"),digest:{gitCommit:$downstream_sha}}]
      },
      runDetails:{
        builder:{id:("https://github.com/"+$repository+"/actions/runs/"+$run_id)},
        metadata:{invocationId:($run_id+"/"+$run_attempt)}
      }
    }
  }' > "$evidence_dir/provenance.intoto.jsonl"

artifacts="$({
  for file in "${artifact_files[@]}"; do
    name="${file##*/}"
    case "$name" in
      typst-agent-compiler-*.docker.tar) kind="compiler-image" ;;
      typst-agent-dev-*.docker.tar) kind="dev-image" ;;
      typst-agent-*.tar.xz|typst-agent-*.zip) kind="binary" ;;
      agentctl) kind="agentctl" ;;
      typst-documentation.pdf) kind="documentation" ;;
      *) echo "unrecognized release artifact: $name" >&2; exit 5 ;;
    esac
    jq -cn --arg path "$file" --arg kind "$kind" '{path:$path,kind:$kind}'
  done
} | jq -s .)"

reproducibility="$(find "$reproducibility_dir" -maxdepth 1 -type f -name '*.json' -size +0c -print0 | sort -z | xargs -0 -r jq -s '.')"
smoke_results="$(find "$smoke_dir" -maxdepth 1 -type f -name '*.json' -size +0c -print0 | sort -z | xargs -0 -r jq -s '.')"
if [[ "$reproducibility" == '[]' || "$smoke_results" == '[]' ]]; then
  echo 'reproducibility and smoke evidence are required' >&2
  exit 5
fi

jq -n \
  --arg release_tag "$release_tag" \
  --arg sbom_path "$evidence_dir/sbom.cyclonedx.json" \
  --arg sigstore_path "$evidence_dir/SHA256SUMS.sigstore.json" \
  --arg provenance_path "$evidence_dir/provenance.intoto.jsonl" \
  --argjson artifacts "$artifacts" \
  --argjson reproducibility "$reproducibility" \
  --argjson smoke_results "$smoke_results" \
  '{
    release_tag:$release_tag,
    artifacts:$artifacts,
    sbom_path:$sbom_path,
    sigstore_bundle_paths:[$sigstore_path],
    provenance_attestation_paths:[$provenance_path],
    reproducibility:$reproducibility,
    smoke_results:$smoke_results
  }' > "$release_root/release-input.json"

cargo agent release-manifest --input "$release_root/release-input.json" --format json \
  > "$evidence_dir/release-manifest.receipt.json"
cp .tmp/agent/release-manifest.json "$evidence_dir/release-manifest.json"
