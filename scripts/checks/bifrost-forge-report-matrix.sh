#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! command -v jq >/dev/null 2>&1; then
  echo "Forge report matrix requires jq" >&2
  exit 1
fi

report_root="target/bifrost-benchmarks/task16"
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

for report in "${reports[@]}"; do
  if [[ ! -s "$report" ]]; then
    echo "missing Forge topology report: $report" >&2
    exit 1
  fi
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
  ' "$report" >/dev/null || {
    echo "Forge topology report has an incomplete closed schema: $report" >&2
    exit 1
  }
done

# These assertions consume reports emitted by the real production recorder and
# supervised owners. Scenario configuration validates topology; it never
# supplies a report value.
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
' "${reports[@]}" >/dev/null || {
  echo "Forge production role replacement evidence does not match executed topology" >&2
  exit 1
}

single_worker_rate="$(jq -r '.maintenance.throughput_mib_per_sec' "$report_root/forge-multi-tenant-single-node.json")"
three_worker_rate="$(jq -r '.maintenance.throughput_mib_per_sec' "$report_root/forge-multi-tenant-multi-node.json")"
jq -en --argjson single "$single_worker_rate" --argjson three "$three_worker_rate" \
  '$three > $single' >/dev/null || {
  echo "three-worker production counter rate did not exceed the controlled one-worker rate" >&2
  exit 1
}

for report in "${query_reports[@]}"; do
  if [[ ! -s "$report" ]]; then
    echo "missing server-integrated query report: $report" >&2
    exit 1
  fi
  jq -e '
    (.scenario.pods | type == "number")
    and (.scenario.tenants | type == "number")
    and (.query.query_latency_p99_us | type == "number")
    and (.query.query_latency_p99_us > 0)
  ' "$report" >/dev/null || {
    echo "server-integrated query report has an incomplete typed schema: $report" >&2
    exit 1
  }
done

scenario_keys="$(for report in "${reports[@]}"; do jq -r '[.envelope.scenario.pods, .envelope.scenario.tenants] | @csv' "$report"; done | sort -u | wc -l | tr -d ' ')"
if [[ "$scenario_keys" != "3" ]]; then
  echo "Forge topology reports do not contain three distinct pod/tenant scenarios" >&2
  exit 1
fi

jq -s '
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
      {concern: "continuous triggering", classification: "match"},
      {concern: "dedicated workers", classification: "match"},
      {concern: "bin-packing", classification: "Wyrd adaptation"},
      {concern: "snapshot expiry", classification: "match"},
      {concern: "orphan cleanup", classification: "match"},
      {concern: "failover", classification: "Wyrd adaptation"},
      {concern: "resource bounds", classification: "Wyrd adaptation"},
      {concern: "metrics", classification: "Wyrd adaptation"},
      {concern: "delete compaction", classification: "not applicable"},
      {concern: "CoW", classification: "not applicable"}
    ]
  }
' "${reports[@]}" "${query_reports[@]}" > "$report_root/forge-matrix.json"

echo "Forge topology report matrix passed"
