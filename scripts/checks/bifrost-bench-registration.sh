#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

require() {
  local pattern="$1"
  local file="$2"
  if ! rg -n -F -- "$pattern" "$file" >/dev/null; then
    echo "missing Bifrost benchmark contract: $pattern in $file" >&2
    exit 1
  fi
}

forbid() {
  local pattern="$1"
  local file="$2"
  if rg -n -F -- "$pattern" "$file" >/dev/null; then
    echo "deleted Bifrost benchmark contract still present: $pattern in $file" >&2
    exit 1
  fi
}

# Tiered-runner taxonomy: preflight (no database), smoke, and the qualification
# suite plus its four per-family lanes.
require '[tasks."bench:bifrost:preflight"]' mise.toml
require '[tasks."bench:bifrost:smoke"]' mise.toml
require '[tasks."bench:bifrost:qualification"]' mise.toml
for family in ingest oracle distributed mixed; do
  require "[tasks.\"bench:bifrost:qualification:$family\"]" mise.toml
done

# Both binaries are launched as harness-free bench targets and registered as such.
require '--bench bench_bifrost_scribe' mise.toml
require '--bench bench_bifrost_oracle' mise.toml
require 'name = "bench_bifrost_scribe"' crates/wyrd/wyrd-testing/Cargo.toml
require 'name = "bench_bifrost_oracle"' crates/wyrd/wyrd-testing/Cargo.toml

# Runner dispatch and family classification back the binaries.
require 'pub async fn run_family' crates/wyrd/wyrd-testing/src/bifrost/bench_families.rs
require 'pub enum RunnerFamily' crates/wyrd/wyrd-testing/src/bifrost/bench_runner.rs

# Reference Postgres identity parity is preserved on the database-backed lanes.
require 'docker-compose.reference.yml' mise.toml
require 'WYRD_BIFROST_REFERENCE_PROFILE = "1"' mise.toml

# The deleted legacy lane taxonomy is gone. `smoke` is reused by the new
# taxonomy, so only the retired lanes are forbidden.
for lane in capacity qualify compare components; do
  forbid "[tasks.\"bench:bifrost:$lane\"]" mise.toml
done
forbid '--bench bench_bifrost_cluster' mise.toml

# The deleted balanced-RPS cluster bench target is unregistered.
forbid 'name = "bench_bifrost_cluster"' crates/wyrd/wyrd-testing/Cargo.toml

# Every symbol, file, and target retired by the balanced-RPS deletion cone stays
# absent from the tree. The checked-in ledger is the source of truth; assert each
# ledgered name is gone (word-boundary fixed-string match, so retained names such
# as BifrostCapacityStage never collide with deleted CapacityStage). Canonical
# plan history, the ledger, and this checker are the only permitted mentions.
ledger="scripts/checks/bifrost-bench-deletion-ledger.tsv"
if [[ ! -f "$ledger" ]]; then
  echo "missing Bifrost benchmark deletion ledger: $ledger" >&2
  exit 1
fi
while IFS=$'\t' read -r kind name _file _replacement; do
  [[ "$kind" == \#* || -z "$kind" ]] && continue
  if git grep -n -w -I -F -e "$name" -- \
      ':(exclude).dev/plan/**' \
      ":(exclude)$ledger" \
      ':(exclude)scripts/checks/bifrost-bench-registration.sh' \
      'crates/**' 'scripts/**' 'benches/**' 'benchmarks/**' 'mise.toml' \
      >/dev/null 2>&1; then
    echo "ledgered deleted Bifrost benchmark name still present in tree: $name (see $ledger)" >&2
    git grep -n -w -I -F -e "$name" -- \
      ':(exclude).dev/plan/**' \
      ":(exclude)$ledger" \
      ':(exclude)scripts/checks/bifrost-bench-registration.sh' \
      'crates/**' 'scripts/**' 'benches/**' 'benchmarks/**' 'mise.toml' >&2 || true
    exit 1
  fi
done < "$ledger"

echo "Bifrost tiered-runner benchmark registration passed"
