#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "Bifrost benchmark registration checker requires rg; refusing to run without it" >&2
  exit 1
fi

lane_chain_body() {
  local manifest="$1"
  local lane="$2"
  local task_body
  task_body="$(sed -n "/^\[tasks\.\"$lane\"\]/,/^\[tasks\./p" "$manifest")"
  printf '%s\n' "$task_body"
  if printf '%s\n' "$task_body" | rg -n -- "mise run ${lane}:inner([[:space:]'\";]|$)" >/dev/null; then
    sed -n "/^\[tasks\.\"$lane:inner\"\]/,/^\[tasks\./p" "$manifest"
  fi
}

require_lane_runner() {
  local manifest="$1"
  local lane="$2"
  local runner="$3"
  lane_chain_body "$manifest" "$lane" | rg -n -F -- "$runner" >/dev/null
}

for lane in \
  "bench:bifrost:scribe:slo" \
  "bench:bifrost:scribe:components" \
  "bench:bifrost:scribe:sustained" \
  "bench:bifrost:forge:slo" \
  "bench:bifrost:oracle:slo" \
  "bench:bifrost:capacity"; do
  if ! rg -n -F "[tasks.\"$lane\"]" mise.toml >/dev/null; then
    echo "missing required Bifrost benchmark lane: $lane" >&2
    exit 1
  fi
done

forge_topology_lanes=(
  "bench:bifrost:forge:single-tenant-single-node"
  "bench:bifrost:forge:multi-tenant-single-node"
  "bench:bifrost:forge:multi-tenant-multi-node"
)
for lane in "${forge_topology_lanes[@]}"; do
  if ! rg -n -F "[tasks.\"$lane\"]" mise.toml >/dev/null; then
    echo "missing required Forge topology benchmark lane: $lane" >&2
    exit 1
  fi
  if ! require_lane_runner mise.toml "$lane" "--bench bench_bifrost_forge"; then
    echo "Forge topology lane does not execute bench_bifrost_forge: $lane" >&2
    exit 1
  fi
done

forge_reports="$(for lane in "${forge_topology_lanes[@]}"; do
  lane_chain_body mise.toml "$lane" | rg -o 'WYRD_BIFROST_REPORT = "[^"]+"' || true
done)"
if [[ "$(printf '%s\n' "$forge_reports" | sed '/^$/d' | sort -u | wc -l | tr -d ' ')" != "3" ]]; then
  echo "Forge topology lanes must write three distinct WYRD_BIFROST_REPORT paths" >&2
  exit 1
fi
if ! rg -n -F '[tasks."bench:bifrost:forge:matrix"]' mise.toml >/dev/null || \
  ! rg -n -F 'bifrost-forge-report-matrix.sh' mise.toml >/dev/null; then
  echo "missing Forge topology report aggregation lane" >&2
  exit 1
fi

for lane in \
  "bench:bifrost:scribe:slo" \
  "bench:bifrost:scribe:components" \
  "bench:bifrost:scribe:sustained" \
  "bench:bifrost:forge:slo" \
  "bench:bifrost:oracle:slo" \
  "bench:bifrost:capacity"; do
  case "$lane" in
    bench:bifrost:scribe:*) runner="--bench bench_bifrost_scribe" ;;
    bench:bifrost:forge:slo|bench:bifrost:capacity) runner="--bench bench_bifrost_forge" ;;
    bench:bifrost:oracle:slo) runner="--bench bench_bifrost_oracle" ;;
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

if ! rg -n '^test-support = \[\]$' crates/shared/wyrd-telemetry/Cargo.toml >/dev/null; then
  echo "wyrd-telemetry test-support must remain an empty capture-only feature" >&2
  exit 1
fi
production_features="$(cargo tree --locked -p wyrd-server -e features,no-dev)"
if printf '%s\n' "$production_features" | rg -n 'wyrd-telemetry feature "test-support"|opentelemetry_sdk feature "testing"' >/dev/null; then
  echo "production wyrd-server feature graph enables test capture support" >&2
  exit 1
fi
test_features="$(cargo tree --locked -p wyrd-testing -e features,no-dev)"
capture_edges="$(printf '%s\n' "$test_features" | rg -c 'wyrd-telemetry feature "test-support"' || true)"
if [[ "$capture_edges" != "1" ]]; then
  echo "wyrd-testing graph must enable wyrd-telemetry/test-support exactly once" >&2
  exit 1
fi
if printf '%s\n' "$test_features" | rg -n 'opentelemetry_sdk feature "testing"' >/dev/null; then
  echo "test capture must not enable the broad OpenTelemetry SDK testing feature" >&2
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
printf '%s\n' \
  '[tasks."bench:bifrost:scribe:slo"]' \
  'run = "mise run bench:bifrost:scribe:slo:inner-wrong"' \
  '[tasks."bench:bifrost:scribe:slo:inner"]' \
  'run = "cargo bench --bench bench_bifrost_scribe"' > "$fixture"
if require_lane_runner "$fixture" "bench:bifrost:scribe:slo" "--bench bench_bifrost_scribe"; then
  echo "benchmark registration checker followed a non-exact inner task" >&2
  exit 1
fi

echo "Bifrost benchmark registration passed"
