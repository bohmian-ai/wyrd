#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "Bifrost benchmark registration checker requires rg; refusing to run without it" >&2
  exit 1
fi

for lane in \
  "bench:bifrost:scribe:slo" \
  "bench:bifrost:scribe:components" \
  "bench:bifrost:scribe:sustained" \
  "bench:bifrost:forge:slo" \
  "bench:bifrost:oracle:slo" \
  "bench:bifrost:oracle:calibrate" \
  "bench:bifrost:capacity"; do
  if ! rg -n -F "[tasks.\"$lane\"]" mise.toml >/dev/null; then
    echo "missing required Bifrost benchmark lane: $lane" >&2
    exit 1
  fi
done

require_lane_runner() {
  local manifest="$1"
  local lane="$2"
  local runner="$3"
  local task_body
  task_body="$(sed -n "/^\[tasks\.\"$lane\"\]/,/^\[tasks\./p" "$manifest")"
  printf '%s\n' "$task_body" | rg -n -F -- "$runner" >/dev/null
}

for lane in \
  "bench:bifrost:scribe:slo" \
  "bench:bifrost:scribe:components" \
  "bench:bifrost:scribe:sustained" \
  "bench:bifrost:forge:slo" \
  "bench:bifrost:oracle:slo" \
  "bench:bifrost:oracle:calibrate" \
  "bench:bifrost:capacity"; do
  case "$lane" in
    bench:bifrost:scribe:*) runner="--bench bench_bifrost_scribe" ;;
    bench:bifrost:forge:slo|bench:bifrost:capacity) runner="--bench bench_bifrost_forge" ;;
    bench:bifrost:oracle:slo|bench:bifrost:oracle:calibrate) runner="--bench bench_bifrost_oracle" ;;
    *) echo "unknown benchmark lane: $lane" >&2; exit 1 ;;
  esac
  if ! require_lane_runner mise.toml "$lane" "$runner"; then
    task_line="$(rg -n -F "[tasks.\"$lane\"]" mise.toml | head -n 1)"
    echo "benchmark lane does not execute its real workload runner: $lane -> $runner (${task_line:-mise.toml})" >&2
    exit 1
  fi
done

for runner in \
  crates/wyrd/wyrd-testing/benches/bench_bifrost_scribe.rs \
  crates/wyrd/wyrd-testing/benches/bench_bifrost_otlp.rs \
  crates/wyrd/wyrd-testing/benches/bench_bifrost_forge.rs \
  crates/wyrd/wyrd-testing/benches/bench_bifrost_oracle.rs; do
  if [[ ! -f "$runner" ]]; then
    echo "missing canonical Bifrost benchmark adapter: $runner" >&2
    exit 1
  fi
  for symbol in BifrostLane BifrostScenario; do
    if ! rg -n -w "$symbol" "$runner" >/dev/null; then
      echo "$runner is missing typed scenario selection: $symbol" >&2
      exit 1
    fi
  done
done

for symbol in REPORT_SCHEMA_VERSION DurableAckSample DurableAckReport summarize_durable_acks; do
  if ! rg -n -w "$symbol" crates/shared/wyrd-bench/src/lane.rs >/dev/null; then
    echo "wyrd-bench is missing the versioned durable-ACK seam: $symbol" >&2
    exit 1
  fi
done

if rg -n 'bench_real_bifrost_workload|bench_ingest_ack_latency|bench_otlp_span_ingest|otlp:ingest:rewrite' \
  mise.toml crates/wyrd/wyrd-testing/Cargo.toml crates/wyrd/wyrd-testing/benches; then
  echo "legacy Bifrost benchmark registration remains" >&2
  exit 1
fi

for symbol in BifrostHarness WyrdTestCluster ScribeInspectionSnapshot; do
  if ! rg -n -w "$symbol" crates/wyrd/wyrd-testing/src/bifrost >/dev/null; then
    echo "real Bifrost harness is missing required path: $symbol" >&2
    exit 1
  fi
done

fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT
fixture="$fixture_dir/mise.toml"
printf '%s\n' '[tasks."bench:bifrost:scribe:slo"]' 'run = "echo registered but no workload"' > "$fixture"
if require_lane_runner "$fixture" "bench:bifrost:scribe:slo" "--bench bench_bifrost_scribe"; then
  echo "benchmark registration checker negative self-test fixture unexpectedly passed" >&2
  exit 1
fi

echo "Bifrost benchmark registration passed"
