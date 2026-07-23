#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

for lane in \
  "bench:bifrost:scribe:slo" \
  "bench:bifrost:forge:slo" \
  "bench:bifrost:oracle:slo" \
  "bench:bifrost:capacity"; do
  if ! rg -n -F "[tasks.\"$lane\"]" mise.toml >/dev/null; then
    echo "missing required Bifrost benchmark lane: $lane" >&2
    exit 1
  fi
done

runner="crates/wyrd/wyrd-testing/benches/bench_real_bifrost_workload.rs"
if [[ ! -f "$runner" ]]; then
  echo "registered Bifrost lanes must use $runner" >&2
  exit 1
fi

for symbol in WyrdTestCluster BifrostHarness run_maintenance_tick reqwest BenchmarkReport; do
  if ! rg -n -w "$symbol" "$runner" >/dev/null; then
    echo "real Bifrost benchmark runner is missing required path: $symbol" >&2
    exit 1
  fi
done

scribe_runner="crates/wyrd/wyrd-testing/benches/bench_ingest_ack_latency.rs"
if [[ ! -f "$scribe_runner" ]]; then
  echo "Scribe SLO lane is missing $scribe_runner" >&2
  exit 1
fi
for symbol in compact_scribe_matrix WyrdTestCluster register_dataset ScribeBenchmarkReport ScribeTopologyEvidence; do
  if ! rg -n -w "$symbol" "$scribe_runner" >/dev/null; then
    echo "Scribe benchmark runner is missing required path: $symbol" >&2
    exit 1
  fi
done
if rg -n 'TABLE_COUNT|bench_scribe_matrix|task-13d' "$scribe_runner" crates/shared/wyrd-bench/src/report.rs >/dev/null; then
  echo "Scribe benchmark runner contains stale topology or matrix vocabulary" >&2
  exit 1
fi

if ! rg -n -w "ScribeRuntimeSnapshot" \
  crates/wyrd/wyrd-testing/src/bifrost/harness.rs \
  crates/vala/vala-bifrost-redux/src/scribe/telemetry.rs >/dev/null; then
  echo "real Bifrost benchmark harness is missing ScribeRuntimeSnapshot" >&2
  exit 1
fi

if rg -n 'Memory::default|chunks\(256\)|sort_unstable' "$runner" >/dev/null; then
  echo "registered Bifrost runner contains a synthetic or in-memory substitute" >&2
  exit 1
fi

echo "Bifrost benchmark registration passed"
