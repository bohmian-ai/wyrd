#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

stage="${1:?usage: capture-bifrost-stage.sh <stage> [before-cluster-json]}"
before_manifest="${2:-}"
target="$(pwd)/target/bifrost-benchmarks/$stage"
if [[ -e $target ]]; then
  target="$target/rerun-$(date -u +%Y%m%dT%H%M%SZ)"
fi
mkdir -p "$target/diagnostics"

cluster_report="$target/cluster.json"
WYRD_BIFROST_REPORT="$cluster_report" mise run bench:bifrost:capacity

diagnostics="$target/diagnostics/components.json"
WYRD_BIFROST_REPORT="$diagnostics" mise run bench:bifrost:components

git_sha="$(git rev-parse HEAD)"
dirty_worktree=false
if [[ -n $(git status --porcelain) ]]; then
  dirty_worktree=true
fi

jq -n \
  --arg stage "$stage" \
  --arg git_sha "$git_sha" \
  --argjson dirty_worktree "$dirty_worktree" \
  --slurpfile cluster "$cluster_report" \
  --slurpfile components "$target/diagnostics/components.json" \
  '{stage: $stage, git_sha: $git_sha, dirty_worktree: $dirty_worktree,
    capacity: $cluster[0], diagnostics: {components: $components[0]}}' \
  > "$target/stage.json"

if [[ -n $before_manifest ]]; then
  cargo run --locked -p wyrd-bench --bin bifrost_compare -- \
    --before "$before_manifest" \
    --after "$cluster_report" \
    --output "$target/comparison.json"
fi

printf 'Bifrost stage captured at %s\n' "$target"
