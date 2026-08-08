#!/usr/bin/env bash
set -euo pipefail

# Capture one Bifrost tiered-runner report with git provenance.
#
# The runner binaries own report generation: each writes
# <output-root>/<run-id>/<workload>-<tier>.json. This wrapper picks a stage
# directory as the output root, drives the family-appropriate binary through the
# reference Postgres lifecycle, and stamps the produced report with the commit
# and worktree state so a captured stage is traceable.

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

workload="${1:?usage: capture-bifrost-stage.sh <workload> [topology] [tier]}"
topology="${2:-one-pod}"
tier="${3:-smoke}"

stage_slug="${workload}-${topology}-${tier}"
target="$(pwd)/target/bifrost-benchmarks/$stage_slug"
if [[ -e $target ]]; then
  target="$target/rerun-$(date -u +%Y%m%dT%H%M%SZ)"
fi
mkdir -p "$target"

run_id="capture-$(date +%s)"

# Ingest workloads run on the Scribe binary; every other family runs on Oracle.
binary=bench_bifrost_oracle
if [[ $workload == ingest-* ]]; then
  binary=bench_bifrost_scribe
fi

export WYRD_POSTGRES_COMPOSE_FILE="benches/bifrost/docker-compose.reference.yml"
export WYRD_BIFROST_REFERENCE_PROFILE="1"
scripts/postgres/with-test-postgres.sh -- bash -lc "
  ulimit -n 8192 2>/dev/null || true
  mise run db:migrate:inner
  cargo bench --locked -p wyrd-testing --features bench --bench $binary -- \
    --workload '$workload' --topology '$topology' --tier '$tier' \
    --run-id '$run_id' --output-root '$target'
"

git_sha="$(git rev-parse HEAD)"
dirty_worktree=false
if [[ -n $(git status --porcelain) ]]; then
  dirty_worktree=true
fi

report="$target/$run_id/${workload}-${tier}.json"
jq -n \
  --arg workload "$workload" \
  --arg topology "$topology" \
  --arg tier "$tier" \
  --arg git_sha "$git_sha" \
  --argjson dirty_worktree "$dirty_worktree" \
  --slurpfile report "$report" \
  '{workload: $workload, topology: $topology, tier: $tier, git_sha: $git_sha,
    dirty_worktree: $dirty_worktree, report: $report[0]}' \
  > "$target/stage.json"

printf 'Bifrost stage captured at %s\n' "$target"
