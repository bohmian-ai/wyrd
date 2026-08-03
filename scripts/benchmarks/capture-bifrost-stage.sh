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
WYRD_BIFROST_REPORT="$cluster_report" mise run bench:bifrost:cluster:capture

for lane in scribe forge oracle otlp; do
  output="$target/diagnostics/$lane.json"
  case "$lane" in
    scribe) task="bench:bifrost:scribe:components" ;;
    forge) task="bench:bifrost:forge:components" ;;
    oracle) task="bench:bifrost:oracle:components" ;;
    otlp) task="bench:bifrost:otlp:ingest" ;;
  esac
  WYRD_BIFROST_REPORT="$output" mise run "$task"
done

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
  --slurpfile scribe "$target/diagnostics/scribe.json" \
  --slurpfile forge "$target/diagnostics/forge.json" \
  --slurpfile oracle "$target/diagnostics/oracle.json" \
  --slurpfile otlp "$target/diagnostics/otlp.json" \
  '{stage: $stage, git_sha: $git_sha, dirty_worktree: $dirty_worktree,
    production_readiness: $cluster[0],
    diagnostics: {scribe: $scribe[0], forge: $forge[0], oracle: $oracle[0], otlp: $otlp[0]}}' \
  > "$target/stage.json"

if [[ -n $before_manifest ]]; then
  cargo run --locked -p wyrd-bench --bin bifrost_compare -- \
    --before "$before_manifest" \
    --after "$cluster_report" \
    --output "$target/comparison.json"
fi

printf 'Bifrost stage captured at %s\n' "$target"
