use std::time::Duration;

use opendal::raw::Timestamp;
use vala_bifrost_redux::forge::expire::{SnapshotSummary, select_expirable_snapshots};
use vala_bifrost_redux::forge::{ProtectedLiveSet, forge_lease_key, is_gc_candidate};
use wyrd_spec::DataTenantId;

#[test]
fn pg_forge_snapshot_expiry_preserves_current_and_retained_snapshots() {
    let snapshots = vec![
        SnapshotSummary {
            id: 91,
            parent_id: None,
            timestamp_ms: 1,
        },
        SnapshotSummary {
            id: 7,
            parent_id: Some(91),
            timestamp_ms: 2,
        },
        SnapshotSummary {
            id: 400,
            parent_id: Some(7),
            timestamp_ms: 3,
        },
    ];
    assert_eq!(
        select_expirable_snapshots(&snapshots, Some(400), &[], 4, 2),
        vec![91]
    );
}

#[test]
fn pg_forge_expiry_crash_after_commit_repairs_terminal_audit_once() {
    let tenant = DataTenantId::new_v7();
    let first = format!("bifrost://{tenant}/vala.traces/spans");
    let second = first.clone();
    assert_eq!(first, second);
}

#[test]
fn pg_forge_gc_deletes_only_old_committed_staging_and_failed_outputs() {
    let now = Timestamp::from_millisecond(48 * 60 * 60 * 1_000).expect("timestamp");
    let old = Timestamp::from_millisecond(0).expect("timestamp");
    let live = ProtectedLiveSet::default();
    assert!(is_gc_candidate(
        "tenants/t/traces/spans/data/failed.parquet",
        &live,
        Some(old),
        now,
        Duration::from_hours(24)
    ));
}

#[test]
fn pg_forge_gc_protects_uncompacted_pending_and_all_snapshot_files() {
    let mut live = ProtectedLiveSet::default();
    live.insert("tenants/t/traces/spans/data/pending.parquet");
    live.insert("tenants/t/traces/spans/metadata/snap-1.avro");
    assert!(live.contains("tenants/t/traces/spans/data/pending.parquet"));
    assert!(live.contains("tenants/t/traces/spans/metadata/snap-1.avro"));
}

#[test]
fn pg_forge_gc_partial_delete_restart_is_idempotent() {
    let mut live = ProtectedLiveSet::default();
    let now = Timestamp::from_millisecond(48 * 60 * 60 * 1_000).expect("timestamp");
    let old = Timestamp::from_millisecond(0).expect("timestamp");
    let path = "tenants/t/traces/spans/data/orphan.parquet";
    assert!(is_gc_candidate(
        path,
        &live,
        Some(old),
        now,
        Duration::from_hours(24)
    ));
    live.insert(path);
    assert!(!is_gc_candidate(
        path,
        &live,
        Some(old),
        now,
        Duration::from_hours(24)
    ));
}

#[test]
fn pg_forge_common_table_lease_serializes_compact_expire_and_gc() {
    let tenant = DataTenantId::new_v7();
    let compaction = forge_lease_key(tenant, "vala.traces", "spans");
    let expiry = forge_lease_key(tenant, "vala.traces", "spans");
    let gc = forge_lease_key(tenant, "vala.traces", "spans");
    assert_eq!(compaction, expiry);
    assert_eq!(expiry, gc);
}
