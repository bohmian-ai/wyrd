#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

fixtures="scripts/checks/fixtures/bifrost-forge-parity/cases.json"
matrix="scripts/checks/bifrost-forge-report-matrix.sh"
target="d4a8483f7f0df0d27ccf0ec39495661cb476c4c0"

jq -e '
  .fixture_version == "wyrd.bifrost.forge-parity-fixtures/v1"
  and (.negative_cases | type == "array" and length == 13)
  and (.negative_cases | unique | length == 13)
' "$fixtures" >/dev/null

if rg -U -n 'record_(task_terminal|task_spill|conflict|cleanup)\([^;]*?("|\.as_str\(\))' \
  crates/vala/vala-bifrost-redux/src/forge >/dev/null; then
  echo "Forge telemetry record call sites must pass typed labels, not strings" >&2
  exit 1
fi

fixture_root="$(mktemp -d)"
trap 'rm -rf "$fixture_root"' EXIT
reports="$fixture_root/reports"
mkdir -p "$reports"
cp architecture/references/domain/bifrost-forge-parity.json "$fixture_root/ledger.json"

write_report() {
  local path="$1"
  local pods="$2"
  local tenants="$3"
  local throughput="$4"
  local topology="$5"
  jq -n --argjson pods "$pods" --argjson tenants "$tenants" --argjson throughput "$throughput" --argjson topology "$topology" '
    {
      envelope: {scenario: {lane: "forge", pods: $pods, tenants: $tenants}},
      maintenance: {
        throughput_mib_per_sec: $throughput,
        task_latency_p99_us: 1,
        backlog_age_us: 0,
        peak_parent_memory: 1,
        spill_bytes: 0,
        lease_contention: 0,
        fence_lost: 0,
        snapshot_changed: 0,
        fairness_lag_tasks: 0,
        cleanup_delay_us: 1,
        role_topology: $topology
      },
      slo: {group: "bifrost.forge_slo", metric: "p99_us", value: 1}
    }
  ' > "$path"
}

write_report "$reports/forge-single-tenant-single-node.json" 1 1 1 '{"all":{"max_active":1,"final_active":1,"starts":1}}'
write_report "$reports/forge-multi-tenant-single-node.json" 1 10 1 '{"all":{"max_active":1,"final_active":1,"starts":1}}'
write_report "$reports/forge-multi-tenant-multi-node.json" 3 10 2 '{"server":{"max_active":1,"final_active":1,"starts":1},"forge_worker":{"max_active":3,"final_active":3,"starts":4}}'
for scenario in '1 1' '1 10' '3 10'; do
  read -r pods tenants <<< "$scenario"
  jq -n --argjson pods "$pods" --argjson tenants "$tenants" '{scenario:{pods:$pods,tenants:$tenants},query:{query_latency_p99_us:1}}' > "$reports/query-$pods-$tenants.json"
done

write_receipt() {
  jq -n --arg target "$target" '
    {
      proof_version: "wyrd.bifrost.forge-parity-proof/v1",
      target_commit: $target,
      test_identity: "pg_bifrost_forge_distributed_journey",
      result_id: ($target + ":pg_bifrost_forge_distributed_journey"),
      passed: true,
      assertions: [
        {id:"forge.continuous-triggering",passed:true},
        {id:"forge.dedicated-workers",passed:true},
        {id:"forge.bin-packing",passed:true},
        {id:"forge.snapshot-expiry",passed:true},
        {id:"forge.orphan-cleanup",passed:true},
        {id:"forge.failover",passed:true}
      ]
    }
  ' > "$fixture_root/receipt.json"
}

run_matrix() {
  WYRD_BIFROST_PARITY_LEDGER="$fixture_root/ledger.json" \
  WYRD_BIFROST_PARITY_REPORT_ROOT="$reports" \
  WYRD_BIFROST_PARITY_PROOF="$fixture_root/receipt.json" \
  bash "$matrix"
}

write_receipt
run_matrix >/dev/null

for case_name in $(jq -r '.negative_cases[]' "$fixtures"); do
  cp architecture/references/domain/bifrost-forge-parity.json "$fixture_root/ledger.json"
  write_receipt
  run_matrix >/dev/null
  [[ -s "$reports/forge-matrix.json" ]]
  case "$case_name" in
    missing-concern) jq 'del(.rows[0])' "$fixture_root/ledger.json" > "$fixture_root/next.json" ;;
redacted
    missing-proof) rm -f "$fixture_root/receipt.json" ;;
    failed-result) jq '.passed = false | .assertions[0].passed = false' "$fixture_root/receipt.json" > "$fixture_root/next.json" ;;
    duplicate-concern) jq '.rows[1].concern = .rows[0].concern' "$fixture_root/ledger.json" > "$fixture_root/next.json" ;;
    duplicate-proof) jq '.assertions += [.assertions[0]]' "$fixture_root/receipt.json" > "$fixture_root/next.json" ;;
    unknown-classification) jq '.rows[0].classification = "unknown"' "$fixture_root/ledger.json" > "$fixture_root/next.json" ;;
    unknown-concern) jq '.rows[0].concern = "unknown"' "$fixture_root/ledger.json" > "$fixture_root/next.json" ;;
    extra-row) jq '.rows += [.rows[0]]' "$fixture_root/ledger.json" > "$fixture_root/next.json" ;;
    stale-target) jq '.target_commit = "stale"' "$fixture_root/receipt.json" > "$fixture_root/next.json" ;;
    missing-assertion) jq 'del(.assertions[0])' "$fixture_root/receipt.json" > "$fixture_root/next.json" ;;
    unselected-proof) jq '.assertions += [{id:"forge.unselected",passed:true}]' "$fixture_root/receipt.json" > "$fixture_root/next.json" ;;
    stale-success-removal) jq '.passed = false' "$fixture_root/receipt.json" > "$fixture_root/next.json" ;;
    *) echo "unknown Forge parity fixture: $case_name" >&2; exit 1 ;;
  esac
  if [[ "$case_name" != "missing-proof" ]]; then
    if [[ "$case_name" == failed-result || "$case_name" == duplicate-proof || "$case_name" == stale-target || "$case_name" == missing-assertion || "$case_name" == unselected-proof || "$case_name" == stale-success-removal ]]; then
      mv "$fixture_root/next.json" "$fixture_root/receipt.json"
    else
      mv "$fixture_root/next.json" "$fixture_root/ledger.json"
    fi
  fi
  if run_matrix >/dev/null 2>&1; then
    echo "Forge parity negative fixture unexpectedly passed: $case_name" >&2
    exit 1
  fi
  if [[ -e "$reports/forge-matrix.json" ]]; then
    echo "Forge parity failed rerun retained a stale success artifact: $case_name" >&2
    exit 1
  fi
done

echo "Forge parity matrix fixtures passed"
