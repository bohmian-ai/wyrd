//! Task 16 WAL failure, replay, and durable-ack closure coverage.

use std::sync::Arc;

use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::NaiveDate;
use opendal::services::Memory;
use tempfile::TempDir;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend, ScribeError};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::audit_envelope::encode_audit_event;
use vala_bifrost_redux::scribe::memory::{BifrostMemoryGovernor, MIN_MEMORY_BYTES, MemoryCategory};
use vala_bifrost_redux::scribe::replay::replay_wal_directory;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::stream_identity::NodeId;
use vala_bifrost_redux::scribe::wal::{SegmentHeader, WalConfig, WalWriter};
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

fn key(tenant: DataTenantId, table: &str) -> SealKey {
    SealKey::new(
        tenant,
        TableRef::new(BifrostNamespace::Bifrost, table),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 24).expect("test day")),
    )
}

fn audit() -> Vec<u8> {
    encode_audit_event(&AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: "task16.wal".to_owned(),
        resource: "vala.bifrost.task16_wal".to_owned(),
        card_ref: None,
        principal_id: PrincipalId::new(Uuid::now_v7()),
        principal_kind: PrincipalKindTag::User,
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:write".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "1 rows".to_owned(),
        detail: None,
    })
    .expect("audit")
}

fn batch() -> RecordBatch {
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ])),
        vec![
            Arc::new(
                TimestampMicrosecondArray::from(vec![
                    NaiveDate::from_ymd_opt(2026, 7, 24)
                        .expect("date")
                        .and_hms_opt(12, 0, 0)
                        .expect("time")
                        .and_utc()
                        .timestamp_micros(),
                ])
                .with_timezone("UTC"),
            ),
            Arc::new(Int64Array::from(vec![7_i64])),
        ],
    )
    .expect("batch")
}

fn principal(tenant: DataTenantId) -> Principal {
    Principal {
        id: PrincipalId::new(Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    }
}

fn scribe(
    wal_root: &TempDir,
    node: NodeId,
) -> (Arc<WalWriter>, vala_bifrost_redux::scribe::ScribeImpl) {
    let wal = Arc::new(
        WalWriter::new(wal_root.path(), *node.as_bytes(), 1, WalConfig::default())
            .expect("WAL writer"),
    );
    let operator = Arc::new(
        opendal::Operator::new(Memory::default())
            .expect("memory object store")
            .finish(),
    );
    let scribe = vala_bifrost_redux::scribe::ScribeImpl::new_for_embedded_with_deps(
        operator,
        Arc::clone(&wal),
        node.to_string(),
        1,
    );
    (wal, scribe)
}

async fn append(
    scribe: &vala_bifrost_redux::scribe::ScribeImpl,
    tenant: DataTenantId,
    table: &str,
    batch_id: Uuid,
) -> Result<(), ScribeError> {
    let rows = batch();
    scribe
        .append_durable(ScribeAppend {
            principal: principal(tenant),
            table: TableRef::new(BifrostNamespace::Bifrost, table),
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(rows.schema().as_ref()),
            request_id: RequestId::now_v7(),
            batch_id,
            measured_wire_bytes: 0,
            rows,
        })
        .await
        .map(|_| ())
}

#[test]
fn wal_v2_header_fails_with_typed_unsupported_version() {
    let mut header = SegmentHeader::new([3_u8; 16], 1, 0, 0);
    header.version = 2;
    let mut encoded = header.encode();
    let crc = crc32c::crc32c(&encoded[..60]);
    encoded[60..64].copy_from_slice(&crc.to_le_bytes());

    assert!(matches!(
        SegmentHeader::decode(&encoded),
        Err(ScribeError::UnsupportedWalVersion { version: 2 })
    ));
}

#[tokio::test]
async fn sync_failure_has_no_ack_or_memtable_visibility() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let (wal, scribe) = scribe(&wal_root, node);
    let tenant = DataTenantId::new_v7();
    wal.trip_sync_failure_for_test();

    let error = append(&scribe, tenant, "sync_failure", Uuid::now_v7())
        .await
        .expect_err("sync failure must not acknowledge");
    assert!(error.to_string().contains("sync failure"));
    let stats = scribe.memtable_stats().expect("memtable stats");
    assert_eq!(stats.writable_rows + stats.immutable_rows, 0);
    assert!(
        wal.bytes_on_disk() > 0,
        "record was written before sync failed"
    );
    scribe.shutdown().await;
}

#[tokio::test]
async fn failure_after_fsync_before_ack_reuses_stable_batch_once() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let (wal, scribe) = scribe(&wal_root, node);
    let tenant = DataTenantId::new_v7();
    let batch_id = Uuid::now_v7();
    wal.trip_post_sync_failure_for_test();

    let error = append(&scribe, tenant, "post_sync_failure", batch_id)
        .await
        .expect_err("post-sync failure must not acknowledge");
    assert!(error.to_string().contains("post-sync"));
    assert_eq!(scribe.memtable_stats().expect("stats").writable_rows, 0);

    append(&scribe, tenant, "post_sync_failure", batch_id)
        .await
        .expect("stable batch retry");
    assert_eq!(scribe.memtable_stats().expect("stats").writable_rows, 1);
    scribe.shutdown().await;

    let replayed = replay_wal_directory(wal_root.path()).expect("replay");
    let state = replayed
        .values()
        .find(|state| state.seal_key.table.name == "post_sync_failure")
        .expect("replayed stable batch");
    assert_eq!(
        state.data_records.len(),
        1,
        "retry must not append a duplicate WAL record"
    );
}

#[test]
fn multi_segment_replay_preserves_order_and_deduplicates() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(
        wal_root.path(),
        *node.as_bytes(),
        1,
        WalConfig::new(256).expect("small segments"),
    )
    .expect("WAL writer");
    let tenant = DataTenantId::new_v7();
    let seal_key = key(tenant, "multi_segment_replay");
    for index in 0_u8..20 {
        writer
            .append_and_fsync_for_test(&seal_key, [index % 10; 16], &audit(), &[index])
            .expect("append");
    }

    let replayed = replay_wal_directory(wal_root.path()).expect("replay");
    let state = &replayed[&seal_key.as_path_components()];
    assert_eq!(state.data_records.len(), 10);
    assert!(
        state
            .append_metas
            .windows(2)
            .all(|window| window[0].wal_lsn < window[1].wal_lsn)
    );
}

#[test]
fn parent_scribe_and_wal_hard_limits_reject_before_append() {
    let governor = BifrostMemoryGovernor::new(MIN_MEMORY_BYTES).expect("memory governor");
    let parent = governor
        .try_reserve_parent(governor.bifrost_limit_bytes())
        .expect("parent limit reservation");
    assert!(governor.try_reserve_parent(1).is_err());
    drop(parent);

    let scribe = governor
        .try_reserve(MemoryCategory::Raw, governor.scribe_limit_bytes())
        .expect("Scribe limit reservation");
    assert!(governor.try_reserve(MemoryCategory::Raw, 1).is_err());
    drop(scribe);

    let wal_root = tempfile::tempdir().expect("WAL directory");
    let writer =
        WalWriter::new(wal_root.path(), [8_u8; 16], 1, WalConfig::default()).expect("WAL writer");
    writer.trip_disk_full_for_test();
    let error = writer
        .append_and_fsync_for_test(
            &key(DataTenantId::new_v7(), "hard_limit"),
            [1_u8; 16],
            b"audit",
            b"data",
        )
        .expect_err("WAL hard limit");
    assert!(matches!(error, ScribeError::WalDiskFull));
    assert_eq!(writer.bytes_on_disk(), 0);
}
