#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

stage="${1:?usage: capture-bifrost-stage.sh <stage> [before-stage-json]}"
before_manifest="${2:-}"
root=".dev/benchmarks/06-bifrost-rebuild"
target="$(pwd)/$root/$stage"
if [[ -e "$target" ]]; then
  target="$target/rerun-$(date -u +%Y%m%dT%H%M%SZ)"
fi
mkdir -p "$target/reports"

for lane in scribe forge oracle capacity; do
  output="$target/reports/$lane.json"
  task="bench:bifrost:${lane}:slo"
  if [[ "$lane" == "capacity" ]]; then
    task="bench:bifrost:capacity"
  fi
  WYRD_BIFROST_OUTPUT="$output" mise run "$task" \
    || { echo "Bifrost lane failed: $lane" >&2; exit 1; }
done

git_sha="$(git rev-parse HEAD)"
dirty_worktree=false
if ! git diff --quiet || ! git diff --cached --quiet; then
  dirty_worktree=true
fi

jq -n \
  --arg stage "$stage" \
  --arg git_sha "$git_sha" \
  --argjson dirty_worktree "$dirty_worktree" \
  --arg pods "${WYRD_BIFROST_PODS:-3}" \
  --arg tenants "${WYRD_BIFROST_TENANTS:-10}" \
  --arg run_deadline_seconds "${WYRD_BIFROST_RUN_DEADLINE_SECONDS:-540}" \
  --slurpfile scribe "$target/reports/scribe.json" \
  --slurpfile forge "$target/reports/forge.json" \
  --slurpfile oracle "$target/reports/oracle.json" \
  --slurpfile capacity "$target/reports/capacity.json" \
  '{stage: $stage, git_sha: $git_sha, dirty_worktree: $dirty_worktree,
    configuration: {pods: $pods, tenants: $tenants,
      run_deadline_seconds: $run_deadline_seconds,
      runner: "bench_real_bifrost_workload"},
    reports: [$scribe[0], $forge[0], $oracle[0], $capacity[0]]}' \
  > "$target/stage.json"

if [[ -n "$before_manifest" ]]; then
  cargo run --locked -p wyrd-bench --bin bifrost_compare -- \
    "--before=$before_manifest" \
    "--after=$target/stage.json" \
    "--output=$target/comparison.json"
fi

printf 'Bifrost stage captured at %s\n' "$target"
