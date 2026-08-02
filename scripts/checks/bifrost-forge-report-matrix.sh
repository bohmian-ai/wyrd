#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! command -v jq >/dev/null 2>&1; then
  echo "Forge report matrix requires jq" >&2
  exit 1
fi

target_commit="d4a8483f7f0df0d27ccf0ec39495661cb476c4c0"
ledger_path="${WYRD_BIFROST_PARITY_LEDGER:-architecture/references/domain/bifrost-forge-parity.json}"
report_root="${WYRD_BIFROST_PARITY_REPORT_ROOT:-target/bifrost-benchmarks/task16}"
receipt_path="${WYRD_BIFROST_PARITY_PROOF:-$report_root/forge-parity-proof.json}"
matrix_path="$report_root/forge-matrix.json"
reports=(
  "$report_root/forge-single-tenant-single-node.json"
  "$report_root/forge-multi-tenant-single-node.json"
  "$report_root/forge-multi-tenant-multi-node.json"
)
query_reports=(
  "$report_root/query-1-1.json"
  "$report_root/query-1-10.json"
  "$report_root/query-3-10.json"
)

sha256() {
  shasum -a 256 | awk '{print $1}'
}

# A rerun may never retain a previous success artifact when its current input
# set is incomplete or invalid. Remove the final path before validating any
# ledger, receipt, or report so every failure is fail-closed for consumers.
rm -f "$matrix_path"

fail() {
  echo "$1" >&2
  exit 1
}

[[ -s "$ledger_path" ]] || fail "missing Forge parity ledger: $ledger_path"
jq -e --arg target "$target_commit" '
  .ledger_version == "wyrd.bifrost.forge-parity-ledger/v1"
redacted
  and (.rows | type == "array" and length == 10)
  and ([.rows[] |
    (.concern | type == "string" and length > 0)
    and (.classification == "match" or .classification == "Wyrd adaptation" or .classification == "not applicable")
    and (.proof_kind == "journey" or .proof_kind == "benchmark" or .proof_kind == "source-comparison")
    and (.wyrd_owner | type == "string" and length > 0)
    and (.invariant | type == "string" and length > 0)
redacted
    and (.wyrd_source | type == "string" and length > 0)
    and (.assertion | type == "string" and length > 0)
  ] | all)
  and ([.rows[].concern] | sort == ["CoW", "bin-packing", "continuous triggering", "dedicated workers", "delete compaction", "failover", "metrics", "orphan cleanup", "resource bounds", "snapshot expiry"])
  and ([.rows[].assertion] | unique | length == 9)
  and ([.rows[] | select(.proof_kind == "journey")] | length == 6)
  and ([.rows[] | select(.proof_kind == "journey") | .assertion] | sort == ["forge.bin-packing", "forge.continuous-triggering", "forge.dedicated-workers", "forge.failover", "forge.orphan-cleanup", "forge.snapshot-expiry"])
  and ([.rows[] | select(.proof_kind == "benchmark")] | length == 2)
  and ([.rows[] | select(.proof_kind == "source-comparison")] | length == 2)
' "$ledger_path" >/dev/null || fail "Forge parity ledger is malformed, stale, incomplete, or has an unknown row"

[[ -s "$receipt_path" ]] || fail "missing Forge parity journey receipt: $receipt_path"
jq -e --arg target "$target_commit" '
  .proof_version == "wyrd.bifrost.forge-parity-proof/v1"
  and .target_commit == $target
  and .test_identity == "pg_bifrost_forge_distributed_journey"
  and .result_id == ($target + ":pg_bifrost_forge_distributed_journey")
  and .passed == true
  and (.assertions | type == "array" and length == 6)
  and ([.assertions[] | (.id | type == "string") and (.passed == true)] | all)
  and ([.assertions[].id] | sort == ["forge.bin-packing", "forge.continuous-triggering", "forge.dedicated-workers", "forge.failover", "forge.orphan-cleanup", "forge.snapshot-expiry"])
  and ([.assertions[].id] | unique | length == 6)
' "$receipt_path" >/dev/null || fail "Forge parity journey receipt is malformed, failed, stale, incomplete, duplicate, or unselected"

for report in "${reports[@]}"; do
  [[ -s "$report" ]] || fail "missing Forge topology report: $report"
  jq -e '
    .envelope.scenario.lane == "forge"
    and (.maintenance.throughput_mib_per_sec | type == "number")
    and (.maintenance.throughput_mib_per_sec > 0)
    and (.maintenance.task_latency_p99_us | type == "number")
    and (.maintenance.task_latency_p99_us > 0)
    and (.maintenance.backlog_age_us | type == "number")
    and (.maintenance.backlog_age_us >= 0)
    and (.maintenance.peak_parent_memory | type == "number")
    and (.maintenance.peak_parent_memory > 0)
    and (.maintenance.spill_bytes | type == "number")
    and (.maintenance.spill_bytes >= 0)
    and (.maintenance.lease_contention | type == "number")
    and (.maintenance.lease_contention >= 0)
    and (.maintenance.fence_lost | type == "number")
    and (.maintenance.fence_lost >= 0)
    and (.maintenance.snapshot_changed | type == "number")
    and (.maintenance.snapshot_changed >= 0)
    and (.maintenance.fairness_lag_tasks | type == "number")
    and (.maintenance.fairness_lag_tasks >= 0)
    and (.maintenance.cleanup_delay_us | type == "number")
    and (.maintenance.cleanup_delay_us > 0)
    and (.maintenance.role_topology | type == "object")
    and (.maintenance.role_topology | length > 0)
    and ([.maintenance.role_topology[] |
      (.max_active | type == "number")
      and (.final_active | type == "number")
      and (.starts | type == "number")
    ] | all)
    and .slo.group == "bifrost.forge_slo"
    and .slo.metric == "p99_us"
    and (.slo.value | type == "number")
  ' "$report" >/dev/null || fail "Forge topology report has an incomplete closed schema: $report"
done

jq -s -e '
  all(.[];
  if .envelope.scenario.pods == 1 then
    (.maintenance.role_topology | keys) == ["all"]
    and .maintenance.role_topology.all.max_active == 1
    and .maintenance.role_topology.all.final_active == 1
    and .maintenance.role_topology.all.starts == 1
  else
    (.maintenance.role_topology | keys) == ["forge_worker", "server"]
    and .maintenance.role_topology.server.max_active == 1
    and .maintenance.role_topology.server.final_active == 1
    and .maintenance.role_topology.server.starts == 1
    and .maintenance.role_topology.forge_worker.max_active == .envelope.scenario.pods
    and .maintenance.role_topology.forge_worker.final_active == .envelope.scenario.pods
    and .maintenance.role_topology.forge_worker.starts == (.envelope.scenario.pods + 1)
  end)
' "${reports[@]}" >/dev/null || fail "Forge production role replacement evidence does not match executed topology"

single_worker_rate="$(jq -r '.maintenance.throughput_mib_per_sec' "$report_root/forge-multi-tenant-single-node.json")"
three_worker_rate="$(jq -r '.maintenance.throughput_mib_per_sec' "$report_root/forge-multi-tenant-multi-node.json")"
jq -en --argjson single "$single_worker_rate" --argjson three "$three_worker_rate" '$three > $single' >/dev/null || fail "three-worker production counter rate did not exceed the controlled one-worker rate"

for report in "${query_reports[@]}"; do
  [[ -s "$report" ]] || fail "missing server-integrated query report: $report"
  jq -e '
    (.scenario.pods | type == "number")
    and (.scenario.tenants | type == "number")
    and (.query.query_latency_p99_us | type == "number")
    and (.query.query_latency_p99_us > 0)
  ' "$report" >/dev/null || fail "server-integrated query report has an incomplete typed schema: $report"
done

scenario_keys="$(for report in "${reports[@]}"; do jq -r '[.envelope.scenario.pods, .envelope.scenario.tenants] | @csv' "$report"; done | sort -u | wc -l | tr -d ' ')"
[[ "$scenario_keys" == "3" ]] || fail "Forge topology reports do not contain three distinct pod/tenant scenarios"

benchmark_result_id="sha256:$(for report in "${reports[@]}"; do shasum -a 256 "$report" | awk '{print $1}'; done | sha256)"
matrix_tmp="$(mktemp "$report_root/forge-matrix.XXXXXX")"
trap 'rm -f "$matrix_tmp"' EXIT
jq -s \
  --slurpfile ledger "$ledger_path" \
  --slurpfile receipt "$receipt_path" \
  --arg target "$target_commit" \
  --arg benchmark_result_id "$benchmark_result_id" \
  '
  {
    report_version: "wyrd.bifrost.forge-matrix/v1",
    scenarios: [
      range(0; 3) as $index
      | .[$index] as $maintenance
      | ([.[3:][] | select(
          .scenario.pods == $maintenance.envelope.scenario.pods
          and .scenario.tenants == $maintenance.envelope.scenario.tenants
        )] | if length == 1 then .[0] else error("query scenario join mismatch") end) as $query
      | {
          scenario: {
            pods: $maintenance.envelope.scenario.pods,
            tenants: $maintenance.envelope.scenario.tenants
          },
          maintenance: $maintenance.maintenance,
          slo: $maintenance.slo,
          query: $query.query
        }
    ],
redacted
      $ledger[0].rows
      | map(
          . as $row
          | if .proof_kind == "journey" then
              $row + {result_id: $receipt[0].result_id, passed: true}
            elif .proof_kind == "benchmark" then
              $row + {result_id: $benchmark_result_id, passed: true}
            else
              $row + {result_id: ("source-comparison:" + .assertion), passed: true}
            end
        )
    )
  }
' "${reports[@]}" "${query_reports[@]}" > "$matrix_tmp"

# Source comparisons have no executable benchmark artifact, but their identity
# is the pinned comparison tuple. Replace the stable placeholder with the exact
# SHA-256 after jq preserves every ledger row unchanged.
redacted
redacted
redacted
  mv "${matrix_tmp}.next" "$matrix_tmp"
redacted

jq -e '
redacted
redacted
redacted
' "$matrix_tmp" >/dev/null || fail "Forge parity matrix could not join every selected passing proof"

mv "$matrix_tmp" "$matrix_path"
trap - EXIT
echo "Forge topology report matrix and parity proof join passed"
