#!/usr/bin/env bash
set -euo pipefail

# Snapshot upstream typst/typst backlog signals for deterministic golden-backlog
# scoring. This helper only READS the GitHub API and writes under
# .tmp/agent/backlog/raw/ (ignored). It never writes to typst/typst and never
# modifies repository history or config.
#
# Requirements: `gh` (authenticated) and `jq`.
#
# Outputs in .tmp/agent/backlog/raw/:
#   provenance.json          snapshot date, upstream sha, counts
#   issues.json              open issues (demand data for scoring)
#   pulls.json               open upstream PRs and their linked issues
#   closed-not-planned.json  upstream-rejected issues (negative signal)
#   maintainers.json         typst org member logins
#   maintainer-comments.json maintainer comments on the top demand issues

usage() {
    printf 'usage: %s [--limit N]\n' "$0" >&2
    printf '  --limit N  max open issues collected per ranking (default 120)\n' >&2
    exit 2
}

limit=120
while (($# > 0)); do
    case "$1" in
        --limit) limit="${2:?missing limit value}"; shift 2 ;;
        -h|--help) usage ;;
        *) usage ;;
    esac
done

command -v gh >/dev/null 2>&1 || { printf 'gh is required\n' >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { printf 'jq is required\n' >&2; exit 2; }
gh auth status >/dev/null 2>&1 || { printf 'gh is not authenticated\n' >&2; exit 2; }

root="$(git rev-parse --show-toplevel)"
out="$root/.tmp/agent/backlog/raw"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$out"

log() { printf 'backlog-fetch: %s\n' "$*" >&2; }

today="$(date -u +%F)"
upstream_sha="$(git rev-parse refs/remotes/upstream/main 2>/dev/null || true)"

# ---- maintainers -----------------------------------------------------------
log "collecting typst org members"
{
    gh api orgs/typst/members --jq '.[].login' 2>/dev/null || true
    gh api orgs/typst/public_members --jq '.[].login' 2>/dev/null || true
    printf 'laurmaedje\nreknih\nelegaanz\nsaecki\n'
} | sort -u > "$tmp/maintainers.txt"
jq -R -s 'split("\n") | map(select(length > 0))' "$tmp/maintainers.txt" \
    > "$out/maintainers.json"

# ---- open issues by demand ranking -----------------------------------------
log "collecting open issues by reactions and comments"
gh search issues --repo typst/typst --state open \
    --sort reactions --order desc --limit "$limit" \
    --json number > "$tmp/by-reactions.json"
gh search issues --repo typst/typst --state open \
    --sort comments --order desc --limit "$limit" \
    --json number > "$tmp/by-comments.json"
gh search issues --repo typst/typst --state open --label bug \
    --sort reactions --order desc --limit 80 \
    --json number > "$tmp/bugs.json"
for label in designed 'good contribution' 'good first issue' 'help wanted' \
    'waiting-on-decision' 'tracking-issue'; do
    gh search issues --repo typst/typst --state open --label "$label" \
        --limit 50 --json number > "$tmp/label-$(printf '%s' "$label" | tr ' ' '-').json"
done

# ---- open pull requests and linked issues ----------------------------------
log "collecting open upstream pull requests"
gh search prs --repo typst/typst --state open --limit 100 \
    --json number,title,author,isDraft,createdAt,updatedAt,body \
    > "$tmp/pulls.json"
jq '[.[] | {
        number,
        title,
        author: .author.login,
        draft: .isDraft,
        created_at: (.createdAt[0:10]),
        updated_at: (.updatedAt[0:10]),
        linked_issues: ([(.body // "") | scan("#([0-9]+)") | .[0] | tonumber] | unique)
    }]' "$tmp/pulls.json" > "$out/pulls.json"

# ---- closed "not planned" issues (negative signal) --------------------------
log "collecting closed not-planned issues"
gh api -X GET search/issues \
    -f q='repo:typst/typst is:issue state:closed reason:"not planned"' \
    -f sort=updated -f order=desc -f per_page=100 --paginate \
    --jq '.items[] | {number, title, closed_at: (.closed_at[0:10])}' \
    > "$tmp/not-planned.ndjson" || {
    printf 'warning: not-planned collection failed; writing empty list\n' >&2
    : > "$tmp/not-planned.ndjson"
}
jq -s '.' "$tmp/not-planned.ndjson" > "$out/closed-not-planned.json"

# ---- issue details for the scoring snapshot --------------------------------
log "collecting issue details"
if [[ -f "$root/.agents/backlog/registry.toml" ]]; then
    grep -oE '^number = [0-9]+' "$root/.agents/backlog/registry.toml" \
        | grep -oE '[0-9]+' \
        | jq -R -s 'split("\n") | map(select(length > 0) | {number: tonumber})' \
        > "$tmp/registry-ids.json"
else
    printf '[]\n' > "$tmp/registry-ids.json"
fi
jq -s 'map(.[].number) | unique | sort' \
    "$tmp/by-reactions.json" "$tmp/by-comments.json" "$tmp/bugs.json" \
    "$tmp"/label-*.json "$tmp/registry-ids.json" > "$tmp/ids.json"
ids="$(jq -r '.[]' "$tmp/ids.json")"
issue_count="$(jq 'length' "$tmp/ids.json")"

printf '[\n' > "$out/issues.json"
first=1
while read -r n; do
    entry="$(gh api "repos/typst/typst/issues/$n" \
        --jq '{number, title, state, labels: (.labels | map(.name)),
               reactions: .reactions.total_count, thumbs_up: (.["reactions"]["+1"]),
               comments, created_at: (.created_at[0:10]), updated_at: (.updated_at[0:10]),
               is_pr: (.pull_request != null)}' 2>/dev/null || true)"
    if [[ -n "$entry" ]]; then
        if ((first)); then first=0; else printf ',\n' >> "$out/issues.json"; fi
        printf '%s' "$entry" >> "$out/issues.json"
    fi
done <<< "$ids"
printf '\n]\n' >> "$out/issues.json"

# ---- per-issue full comments (registry + top demand) -----------------------
log "collecting per-issue comments"
mkdir -p "$out/comments"
jq -s 'map(.[].number) | unique | sort' \
    "$tmp/by-reactions.json" "$tmp/by-comments.json" "$tmp/registry-ids.json" \
    > "$tmp/comment-ids.json"
jq -r '.[0:170][]' "$tmp/comment-ids.json" > "$tmp/comment-ids.txt"
comment_files=0
while read -r n; do
    comments="$(gh api "repos/typst/typst/issues/$n/comments" --paginate \
        --jq "[.[] | {author: .user.login, created_at: (.created_at[0:10]),
                     body: (.body | gsub(\"[\n\r]\"; \" \") | .[0:1500])}]" \
        2>/dev/null || true)"
    if [[ -n "$comments" ]]; then
        printf '%s' "$comments" > "$out/comments/$n.json"
        comment_files=$((comment_files + 1))
    fi
done < "$tmp/comment-ids.txt"

# ---- per-issue timeline cross-references (registry + top demand) ------------
log "collecting timeline cross-references"
mkdir -p "$out/crossrefs"
jq -r '.[0:140][]' "$tmp/comment-ids.json" > "$tmp/crossref-ids.txt"
crossref_files=0
while read -r n; do
    crossrefs="$(gh api "repos/typst/typst/issues/$n/timeline" --paginate \
        --jq "{references: ([.[] | select(.event == \"cross-referenced\") |
                (.source.issue.number // empty)] | unique | sort),
               closed_reason: ([.[] | select(.event == \"closed\") |
                .state_reason // empty] | .[0])}" 2>/dev/null || true)"
    if [[ -n "$crossrefs" ]]; then
        printf '%s' "$crossrefs" > "$out/crossrefs/$n.json"
        crossref_files=$((crossref_files + 1))
    fi
done < "$tmp/crossref-ids.txt"

# ---- maintainer comments on the top demand issues --------------------------
log "collecting maintainer comments"
maintainer_cond="$(
    while read -r m; do printf '.user.login == "%s" or ' "$m"; done < "$tmp/maintainers.txt"
    printf '.user.login == "__never__"'
)"
jq -s 'map(.[].number) | unique | sort | .[0:140] | .[]' \
    "$tmp/by-reactions.json" "$tmp/by-comments.json" > "$tmp/top-ids.txt"
printf '[\n' > "$out/maintainer-comments.json"
first=1
while read -r n; do
    comments="$(gh api "repos/typst/typst/issues/$n/comments" --paginate \
        --jq ".[] | select($maintainer_cond) |
              {issue: $n, author: .user.login,
               body: (.body | gsub(\"[\n\r]\"; \" \") | .[0:400])}" \
        2>/dev/null || true)"
    if [[ -n "$comments" ]]; then
        if ((first)); then first=0; else printf ',\n' >> "$out/maintainer-comments.json"; fi
        printf '%s' "$comments" | tr '\n' ',' | sed 's/,$//' >> "$out/maintainer-comments.json"
    fi
done < "$tmp/top-ids.txt"
printf '\n]\n' >> "$out/maintainer-comments.json"
jq -s 'add | sort_by(.issue, .author)' "$out/maintainer-comments.json" > "$tmp/comments-sorted.json"
mv "$tmp/comments-sorted.json" "$out/maintainer-comments.json"

# ---- provenance -------------------------------------------------------------
maintainer_count="$(wc -l < "$tmp/maintainers.txt")"
jq -n \
    --arg date "$today" \
    --arg sha "$upstream_sha" \
    --argjson issues "$issue_count" \
    --argjson pulls "$(jq 'length' "$out/pulls.json")" \
    --argjson not_planned "$(jq 'length' "$out/closed-not-planned.json")" \
    --argjson maintainers "$maintainer_count" \
    --argjson comments "$(jq 'length' "$out/maintainer-comments.json")" \
    --argjson comment_files "$comment_files" \
    --argjson crossref_files "$crossref_files" \
    '{snapshot_date: $date, upstream_sha: $sha, issue_count: $issues,
      pull_count: $pulls, closed_not_planned_count: $not_planned,
      maintainer_count: $maintainers, maintainer_comment_count: $comments,
      comment_file_count: $comment_files, crossref_file_count: $crossref_files}' \
    > "$out/provenance.json"

log "snapshot written to $out"
jq -r '"  snapshot_date=\(.snapshot_date) upstream_sha=\(.upstream_sha // "unavailable")
  issues=\(.issue_count) pulls=\(.pull_count) not_planned=\(.closed_not_planned_count)
  maintainer_comments=\(.maintainer_comment_count) comment_files=\(.comment_file_count)
  crossref_files=\(.crossref_file_count)"' "$out/provenance.json"
