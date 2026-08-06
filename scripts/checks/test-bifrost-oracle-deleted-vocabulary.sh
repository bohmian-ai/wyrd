#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
checker_source="$repo_root/scripts/checks/bifrost-oracle-deleted-vocabulary.sh"
ledger_path="$repo_root/scripts/checks/bifrost-oracle-deleted-vocabulary-ledger.tsv"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/bifrost-vocabulary-check.XXXXXX")"
trap 'rm -rf "$fixture_root"' EXIT

mkdir -p "$fixture_root/scripts/checks" "$fixture_root/crates"
cp "$checker_source" "$fixture_root/scripts/checks/bifrost-oracle-deleted-vocabulary.sh"
chmod +x "$fixture_root/scripts/checks/bifrost-oracle-deleted-vocabulary.sh"
git -C "$fixture_root" init -q
git -C "$fixture_root" add scripts/checks/bifrost-oracle-deleted-vocabulary.sh

run_checker() {
  BIFROST_DELETED_VOCABULARY_ROOT="$fixture_root" \
    "$fixture_root/scripts/checks/bifrost-oracle-deleted-vocabulary.sh"
}

validate_ledger() {
  local candidate_path="${1:-$ledger_path}"
  awk -F '\t' '
    BEGIN { valid = 1 }
    /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
    NF != 3 {
      printf "invalid ledger row %d: expected three tab-separated fields\n", NR > "/dev/stderr"
      valid = 0
      next
    }
    {
      token = $1
      classification = $2
      evidence = $3
      if (token == "" || evidence == "") {
        printf "invalid ledger row %d: token and evidence are required\n", NR > "/dev/stderr"
        valid = 0
      }
      if (classification != "deleted" && classification != "current_unrelated") {
        printf "invalid ledger row %d: unsupported classification %s\n", NR, classification > "/dev/stderr"
        valid = 0
      }
      if (++seen[token] > 1) {
        printf "duplicate ledger token %s at lines %d and %d\n", token, first[token], NR > "/dev/stderr"
        valid = 0
      } else {
        first[token] = NR
      }
    }
    END { exit valid == 0 ? 1 : 0 }
  ' "$candidate_path"
}

assert_ledger_rejected() {
  local fixture_content="$1"
  local description="$2"
  local fixture_ledger="$fixture_root/ledger.tsv"
  printf '%s\n' "$fixture_content" >"$fixture_ledger"
  if validate_ledger "$fixture_ledger" >/dev/null 2>&1; then
    printf 'ledger parser accepted %s\n' "$description" >&2
    exit 1
  fi
  rm -f "$fixture_ledger"
}

validate_ledger
duplicate_ledger_row=$'duplicate.ledger.token\tdeleted\trepresentative duplicate test row'
assert_ledger_rejected "$duplicate_ledger_row
$duplicate_ledger_row" 'an identical duplicate classification'
assert_ledger_rejected "$duplicate_ledger_row
duplicate.ledger.token	current_unrelated	conflicting duplicate test row" 'a conflicting duplicate classification'

run_checker >/dev/null

assert_rejected() {
  local fixture_content="$1"
  printf '%s\n' "$fixture_content" >"$fixture_root/crates/deleted_fixture.rs"
  git -C "$fixture_root" add crates/deleted_fixture.rs
  if run_checker >/dev/null 2>&1; then
    printf 'checker accepted deleted vocabulary fixture: %s\n' "$fixture_content" >&2
    exit 1
  fi
  git -C "$fixture_root" rm -q --cached --force crates/deleted_fixture.rs
  rm -f "$fixture_root/crates/deleted_fixture.rs"
}

deleted_tokens=(
  'bifrost_oracle''_lease_renewals_total'
  'bifrost_oracle''_lease_release_queue_total'
  'bifrost_oracle''_admission_reconcile_total'
  'bifrost_oracle''_admission_reconcile_queue_total'
  'bifrost_oracle''_source_bytes_total'
  'bifrost_oracle''_admission_wait_seconds'
  'bifrost_oracle''_admission_rejections_total'
  'bifrost_oracle''_slots_total'
  'bifrost_oracle''_slots_in_use'
  'bifrost_oracle''_admission_waiters'
  'bifrost_oracle''_in_flight'
  'bifrost_oracle''_classification_total'
  'bifrost_oracle''_predicted_scan_seconds'
  'bifrost_oracle''_queries_total'
  'bifrost_oracle''_query_duration_seconds'
  'bifrost_oracle''_time_to_first_batch_seconds'
  'bifrost_oracle''_audit_seconds'
  'bifrost_oracle''_spill_operations_total'
  'bifrost_oracle''_spill_bytes_total'
  'bifrost_oracle''_stale_replans_total'
  'bifrost_oracle''_streams_total'
  'bifrost_oracle''_stream_rows_total'
  'bifrost_oracle''_stream_bytes_total'
  'bifrost_oracle''_slot_reservations_total'
  'bifrost_oracle''_source_rows_total'
  'bifrost_oracle''_class_memory_bytes'
  'bifrost_oracle''_memory_bytes'
  'bifrost_oracle''_fragment_bytes_total'
  'bifrost_oracle''_fragment_seconds'
  'bifrost_oracle''_fragments_in_flight'
  'oracle_source''_rows'
  'oracle''_slots'
  'cleanup.oracle''_in_flight'
  'cleanup.oracle''_slots'
  'oracle.source''_rows'
  'phase.oracle_source''_rows'
  'pillars.oracle_analytical''_slots_peak'
  'pillars.oracle_class''_lease_rejections'
  'pillars.oracle_cluster''_lease_rejections'
  'pillars.oracle_global_operator''_classifications'
  'pillars.oracle_interactive_scan''_classifications'
  'pillars.oracle_lease_timeout''_rejections'
  'pillars.oracle_local_slot''_rejections'
  'pillars.oracle_pending_limit''_rejections'
  'pillars.oracle_predicted_scan''_classifications'
  'pillars.oracle_tenant_lease''_rejections'
  'oracle.slots''_final'
  'oracle.slots_peak''.analytical'
  'oracle.classification.interactive''_scan'
  'oracle.classification.predicted''_scan'
  'oracle.classification.global''_operator'
  'oracle.admission.pending''_limit'
  'oracle.admission.lease''_timeout'
  'oracle.admission.lease''_cluster'
  'oracle.admission.lease''_class'
  'oracle.admission.lease''_tenant'
  'oracle.admission.local''_slots'
  'oracle_analytical''_slots_peak'
  'oracle_slots''_total'
  'oracle_interactive''_scan_classifications'
  'oracle_predicted''_scan_classifications'
  'oracle_global''_operator_classifications'
  'oracle_pending''_limit_rejections'
  'oracle_lease''_timeout_rejections'
  'oracle_cluster''_lease_rejections'
  'oracle_class''_lease_rejections'
  'oracle_tenant''_lease_rejections'
  'oracle_local''_slot_rejections'
  'oracle_lease''_scope'
  '"lease_class"'
  '"lease_cluster"'
  '"lease_tenant"'
  '"lease_capacity"'
  '"lease_timeout"'
  '"pending_limit"'
  '"local_slots"'
  'slots''_peak'
  'slots''_final'
)
for deleted_token in "${deleted_tokens[@]}"; do
  assert_rejected "deleted_token = \"$deleted_token\""
done

assert_deleted_ledger() {
  local candidate_path="$1"
  validate_ledger "$candidate_path"
  for deleted_token in "${deleted_tokens[@]}"; do
    ledger_token_count="$(awk -F '\t' -v token="$deleted_token" \
      '$1 == token && $2 == "deleted" { count++ } END { print count + 0 }' \
      "$candidate_path")"
    if [[ "$ledger_token_count" != 1 ]]; then
      printf 'deletion ledger must contain exactly one deleted checker token row: %s (count=%s)\n' \
        "$deleted_token" "$ledger_token_count" >&2
      return 1
    fi
  done
}

ledger_path="$repo_root/scripts/checks/bifrost-oracle-deleted-vocabulary-ledger.tsv"
if [[ ! -s "$ledger_path" ]] || ! grep -Fq $'source baseline: 4cf66014b' "$ledger_path"; then
  printf 'deletion ledger is missing its RR7 baseline header: %s\n' "$ledger_path" >&2
  exit 1
fi
assert_deleted_ledger "$ledger_path"

classification_token='bifrost_oracle_class_memory_bytes'
mutated_ledger="$fixture_root/classification-mutation.tsv"
awk -F '\t' -v OFS='\t' -v token="$classification_token" \
  '$1 == token { $2 = "current_unrelated" } { print }' \
  "$ledger_path" >"$mutated_ledger"
if mutation_output="$(assert_deleted_ledger "$mutated_ledger" 2>&1)"; then
  printf 'classification-only ledger mutation unexpectedly passed: %s\n' \
    "$classification_token" >&2
  exit 1
fi
if ! grep -Fq -- "$classification_token" <<<"$mutation_output"; then
  printf 'classification-only mutation diagnostic omitted token: %s\n' \
    "$classification_token" >&2
  exit 1
fi

run_checker >/dev/null
printf 'Deleted Oracle vocabulary self-tests passed.\n'
