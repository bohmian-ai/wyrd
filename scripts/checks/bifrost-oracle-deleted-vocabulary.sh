#!/usr/bin/env bash
set -euo pipefail

# Keep deleted Oracle lease/accounting vocabulary from reappearing in source,
# fixtures, benchmark consumers, or repository command definitions. Historical
# plan files intentionally retain the vocabulary as migration evidence.
root_dir="${BIFROST_DELETED_VOCABULARY_ROOT:-$(git rev-parse --show-toplevel)}"
checker_path="scripts/checks/bifrost-oracle-deleted-vocabulary.sh"
ledger_path="scripts/checks/bifrost-oracle-deleted-vocabulary-ledger.tsv"
fixture_path="scripts/checks/test-bifrost-oracle-deleted-vocabulary.sh"
patterns=(
  'bifrost_oracle_lease_renewals_total'
  'bifrost_oracle_lease_release_queue_total'
  'bifrost_oracle_admission_reconcile_total'
  'bifrost_oracle_admission_reconcile_queue_total'
  'bifrost_oracle_source_bytes_total'
  'bifrost_oracle_admission_wait_seconds'
  'bifrost_oracle_admission_rejections_total'
  'bifrost_oracle_slots_total'
  'bifrost_oracle_slots_in_use'
  'bifrost_oracle_admission_waiters'
  'bifrost_oracle_in_flight'
  'bifrost_oracle_classification_total'
  'bifrost_oracle_predicted_scan_seconds'
  'bifrost_oracle_queries_total'
  'bifrost_oracle_query_duration_seconds'
  'bifrost_oracle_time_to_first_batch_seconds'
  'bifrost_oracle_audit_seconds'
  'bifrost_oracle_spill_operations_total'
  'bifrost_oracle_spill_bytes_total'
  'bifrost_oracle_stale_replans_total'
  'bifrost_oracle_streams_total'
  'bifrost_oracle_stream_rows_total'
  'bifrost_oracle_stream_bytes_total'
  'bifrost_oracle_slot_reservations_total'
  'bifrost_oracle_source_rows_total'
  'bifrost_oracle_class_memory_bytes'
  'bifrost_oracle_memory_bytes'
  'bifrost_oracle_fragment_bytes_total'
  'bifrost_oracle_fragment_seconds'
  'bifrost_oracle_fragments_in_flight'
  'oracle_source_rows'
  'oracle_slots'
  'cleanup.oracle_in_flight'
  'cleanup.oracle_slots'
  'oracle.slots_final'
  'oracle.slots_peak.analytical'
  'oracle.classification.interactive_scan'
  'oracle.classification.predicted_scan'
  'oracle.classification.global_operator'
  'oracle.admission.pending_limit'
  'oracle.admission.lease_timeout'
  'oracle.admission.lease_cluster'
  'oracle.admission.lease_class'
  'oracle.admission.lease_tenant'
  'oracle.admission.local_slots'
  'oracle.source_rows'
  'phase.oracle_source_rows'
  'pillars.oracle_analytical_slots_peak'
  'pillars.oracle_class_lease_rejections'
  'pillars.oracle_cluster_lease_rejections'
  'pillars.oracle_global_operator_classifications'
  'pillars.oracle_interactive_scan_classifications'
  'pillars.oracle_lease_timeout_rejections'
  'pillars.oracle_local_slot_rejections'
  'pillars.oracle_pending_limit_rejections'
  'pillars.oracle_predicted_scan_classifications'
  'pillars.oracle_tenant_lease_rejections'
  'oracle_analytical_slots_peak'
  'oracle_slots_total'
  'oracle_interactive_scan_classifications'
  'oracle_predicted_scan_classifications'
  'oracle_global_operator_classifications'
  'oracle_pending_limit_rejections'
  'oracle_lease_timeout_rejections'
  'oracle_cluster_lease_rejections'
  'oracle_class_lease_rejections'
  'oracle_tenant_lease_rejections'
  'oracle_local_slot_rejections'
  'oracle_lease_scope'
  # Quoted values catch deleted telemetry labels while allowing live runtime
  # field names such as `pending_limit` and `local_slots`.
  '"lease_class"'
  '"lease_cluster"'
  '"lease_tenant"'
  '"lease_capacity"'
  '"lease_timeout"'
  '"pending_limit"'
  '"local_slots"'
  'slots_peak'
  'slots_final'
)

grep_args=()
for pattern in "${patterns[@]}"; do
  grep_args+=(-e "$pattern")
done

violations=""
violations="$(git -C "$root_dir" grep -n -I -F "${grep_args[@]}" -- \
  ':(exclude).dev/plan/**' \
  ":(exclude)$checker_path" \
  ":(exclude)$ledger_path" \
  ":(exclude)$fixture_path" \
  'crates/**' 'scripts/**' 'benchmarks/**' 'benches/**' 'mise.toml' \
  '*.toml' '*.yaml' '*.yml' '*.json' '*.py' '*.sh' '*.rs' '*.ts' '*.tsx' '*.mjs' \
  2>/dev/null || true)"

if [[ -n "$violations" ]]; then
  printf 'Deleted Oracle vocabulary found outside canonical plan history:\n'
  printf '  %s\n' "$violations"
  exit 1
fi

printf 'Deleted Oracle vocabulary check passed.\n'
