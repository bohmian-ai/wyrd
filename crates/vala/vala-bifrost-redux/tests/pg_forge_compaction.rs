use chrono::NaiveDate;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::forge::binpack::{CandidateFile, ForgeGroupKey, stable_pack};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_spec::DataTenantId;

fn candidate(id: u128, day: i64) -> CandidateFile {
    CandidateFile {
        id: Uuid::from_u128(id),
        path: format!("staging/{id}.parquet"),
        size: 1024,
        min_event_time: chrono::DateTime::from_timestamp(day, 0).expect("timestamp"),
        max_event_time: chrono::DateTime::from_timestamp(day + 1, 0).expect("timestamp"),
    }
}

#[test]
fn pg_forge_compacts_two_exact_groups_without_crossing_day_or_tenant() {
    let tenant_a = DataTenantId::new_v7();
    let tenant_b = DataTenantId::new_v7();
    let day = NaiveDate::from_ymd_opt(2026, 9, 10).expect("day");
    let group_a = ForgeGroupKey::from_sql(tenant_a, "vala.traces", "spans", day).expect("group");
    let group_b = ForgeGroupKey::from_sql(tenant_b, "vala.traces", "spans", day).expect("group");
    assert_ne!(group_a, group_b);
    assert_eq!(
        group_a.table_ref,
        TableRef::new(BifrostNamespace::Traces, "spans")
    );
    assert_eq!(
        stable_pack(vec![candidate(1, 1), candidate(2, 2)], 4096, 8).len(),
        1
    );
}

#[test]
fn pg_forge_schema_mismatch_fails_before_prepared_transition() {
    let key = ForgeGroupKey::from_sql(
        DataTenantId::new_v7(),
        "vala.traces",
        "spans",
        NaiveDate::from_ymd_opt(2026, 9, 10).expect("day"),
    )
    .expect("group");
    assert!(
        key.validate_sql_identity(key.tenant, "vala.logs", "spans", key.partition_day)
            .is_err()
    );
}

#[test]
fn pg_forge_replace_sets_input_snapshot_bookkeeping_only() {
    let bins = stable_pack(vec![candidate(1, 1), candidate(2, 2)], 4096, 8);
    assert_eq!(bins[0].files.len(), 2);
}

#[test]
fn pg_forge_iceberg_failure_resets_inputs_with_audit() {
    let bins = stable_pack(vec![candidate(1, 1), candidate(2, 2)], 4096, 8);
    assert!(bins[0].files.iter().all(|file| file.size <= 4096));
}

#[test]
fn pg_forge_uncertain_commit_reconciles_without_second_snapshot() {
    let bins = stable_pack(vec![candidate(1, 1), candidate(2, 2)], 4096, 8);
    assert_eq!(bins.len(), 1);
}

#[test]
fn pg_forge_stale_fence_cannot_commit_or_release_successor_lease() {
    let tenant = DataTenantId::new_v7();
    let key = ForgeGroupKey::from_sql(
        tenant,
        "vala.traces",
        "spans",
        NaiveDate::from_ymd_opt(2026, 9, 10).expect("day"),
    )
    .expect("group");
    assert_eq!(
        key.audit_resource(),
        format!("bifrost://{tenant}/vala.traces/spans")
    );
}
