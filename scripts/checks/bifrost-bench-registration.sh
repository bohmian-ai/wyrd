#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

require() {
  local pattern="$1"
  local file="$2"
  if ! rg -n -F -- "$pattern" "$file" >/dev/null; then
    echo "missing Bifrost cluster benchmark contract: $pattern in $file" >&2
    exit 1
  fi
}

for lane in capacity qualify compare components; do
  require "[tasks.\"bench:bifrost:$lane\"]" mise.toml
done
require "--bench bench_bifrost_cluster" mise.toml
require "--mode capacity" mise.toml
require "--mode qualification" mise.toml
require "--matrix" mise.toml
require "bench_bifrost_cluster" crates/wyrd/wyrd-testing/Cargo.toml
require "reference_scenario_matrix" crates/wyrd/wyrd-testing/src/bifrost/bench_cluster.rs
require "balanced-one-pod-one-tenant" crates/wyrd/wyrd-testing/src/bifrost/bench_cluster.rs
require "balanced-one-pod-eight-tenants" crates/wyrd/wyrd-testing/src/bifrost/bench_cluster.rs
require "balanced-three-server-three-worker-eight-tenants" crates/wyrd/wyrd-testing/src/bifrost/bench_cluster.rs
require "balanced-three-server-three-worker-thirty-two-tenants" crates/wyrd/wyrd-testing/src/bifrost/bench_cluster.rs
require "write-heavy-three-server-three-worker-eight-tenants" crates/wyrd/wyrd-testing/src/bifrost/bench_cluster.rs
require "read-heavy-three-server-three-worker-eight-tenants" crates/wyrd/wyrd-testing/src/bifrost/bench_cluster.rs
require "CLUSTER_REPORT_VERSION" crates/shared/wyrd-bench/src/cluster.rs
require "deny_unknown_fields" crates/shared/wyrd-bench/src/cluster.rs
require "docker-compose.reference.yml" mise.toml

if rg -n '^\[tasks\."bench(:workload|:check)?"\]' mise.toml >/dev/null; then
  echo "legacy headline benchmark commands remain registered" >&2
  exit 1
fi
if rg -n '^\[tasks\."bench:bifrost:(cluster|baseline|preflight|scribe|forge|oracle|otlp)' mise.toml >/dev/null; then
  echo "legacy Bifrost benchmark aliases remain registered" >&2
  exit 1
fi

echo "Bifrost full-cluster benchmark registration passed"
