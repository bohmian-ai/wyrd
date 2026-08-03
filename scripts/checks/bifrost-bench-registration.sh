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

for lane in reference reference:inner capture compare smoke; do
  require "[tasks.\"bench:bifrost:cluster:$lane\"]" mise.toml
done
require "--bench bench_bifrost_cluster" mise.toml
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

for diagnostic in \
  bench_bifrost_scribe \
  bench_bifrost_forge \
  bench_bifrost_oracle \
  bench_bifrost_otlp; do
  require "$diagnostic" crates/wyrd/wyrd-testing/Cargo.toml
done

if rg -n -F 'production-readiness' \
  crates/wyrd/wyrd-testing/src/bifrost/bench_scribe.rs \
  crates/wyrd/wyrd-testing/src/bifrost/bench_forge.rs >/dev/null; then
  echo "component diagnostics still claim production-readiness authority" >&2
  exit 1
fi

echo "Bifrost full-cluster benchmark registration passed"
