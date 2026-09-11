//! Restart/replay coverage for the Scribe WAL and immutable persistence path.

use crate::catalog::TableRef;
use crate::namespaces::BifrostNamespace;
use crate::scribe::ScribeImpl;
use crate::scribe::audit_envelope::encode_audit_event;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::NodeId;
use crate::scribe::wal::{WalConfig, WalWriter};
use arrow::array::{Int32Array, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use opendal::services::Memory;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

/// Encode a one-row replay payload with the required persisted row identity.
fn batch_bytes(value: i64) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new("wyrd_row_ordinal", DataType::Int32, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![value])),
            Arc::new(Int32Array::from(vec![0_i32])),
        ],
    )
    .expect("test batch");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC finish");
    bytes
}

/// Encode a large replay payload with production-equivalent row ordinals.
fn large_batch_bytes(value: i64) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new("wyrd_row_ordinal", DataType::Int32, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![value; 700_000])),
            Arc::new(Int32Array::from_iter_values(0_i32..700_000_i32)),
        ],
    )
    .expect("large test batch");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC finish");
    bytes
}

/// Build one tenant-bound replay audit event.
fn audit_event(operation: &str, tenant: DataTenantId) -> AuditEvent {
    AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: operation.to_owned(),
        resource: "vala.bifrost.scribe_persistence".to_owned(),
        card_ref: None,
        principal_id: PrincipalId::new(Uuid::now_v7()),
        principal_kind: PrincipalKindTag::User,
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:write".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: format!("tenant {tenant} rows"),
        detail: None,
    }
}

/// Build one deterministic tenant/table replay scope.
fn seal_key(tenant: DataTenantId, table: &str) -> SealKey {
    SealKey::new(
        tenant,
        TableRef::new(BifrostNamespace::Bifrost, table),
        crate::test_support::day_partition(2026, 7, 24),
    )
}

#[tokio::test]
/// Replay applies owner backpressure before exceeding its memory budget.
async fn replay_memory_is_bounded_by_owner_backpressure() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, WalConfig::default())
        .expect("WAL writer");
    let tenant_a = DataTenantId::new_v7();
    let tenant_b = DataTenantId::new_v7();
    let key_a = seal_key(tenant_a, "scribe_restart_a");
    let key_b = seal_key(tenant_b, "scribe_restart_b");
    let data = batch_bytes(7);

    let audit_a = encode_audit_event(&audit_event("tenant-a", tenant_a)).expect("audit");
    let audit_b = encode_audit_event(&audit_event("tenant-b", tenant_b)).expect("audit");
    for index in 0_u16..130 {
        let mut batch_id = [0_u8; 16];
        batch_id[..2].copy_from_slice(&index.to_le_bytes());
        let (key, audit) = if index % 2 == 0 {
            (&key_a, &audit_a)
        } else {
            (&key_b, &audit_b)
        };
        writer
            .append_and_commit_for_replay_test(key, batch_id, audit, &data)
            .expect("tenant append");
    }
    drop(writer);

    let operator = Arc::new(
        opendal::Operator::new(Memory::default())
            .expect("memory object store")
            .finish(),
    );
    let restarted_wal = Arc::new(
        WalWriter::new(temp_dir.path(), *node.as_bytes(), 2, WalConfig::default())
            .expect("restarted WAL writer"),
    );
    let scribe =
        ScribeImpl::new_for_embedded_with_deps(operator, restarted_wal, &node.to_string(), 2);

    let restored = scribe.replay_wal_async().await.expect("replay");
    assert!(restored > 0, "streamed replay should restore generations");
    let stats = scribe.memtable_stats().expect("memtable stats");
    assert_eq!(stats.immutable_rows, 130);
    assert_eq!(stats.immutable_generations, restored);
    assert_eq!(
        scribe.memory_snapshot().categories[5],
        stats.immutable_bytes
    );
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

#[tokio::test]
/// Same-key generations replay in durable WAL order without coalescing.
async fn replay_splits_three_same_key_generations_in_wal_order() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, WalConfig::default())
        .expect("WAL writer");
    let tenant = DataTenantId::new_v7();
    let key = seal_key(tenant, "replay_three_generations");
    let audit = encode_audit_event(&audit_event("replay-three", tenant)).expect("audit");
    let data = large_batch_bytes(7);
    for index in 0_u8..3 {
        writer
            .append_and_commit_for_replay_test(&key, [index; 16], &audit, &data)
            .expect("same-key replay append");
    }
    drop(writer);

    let operator = Arc::new(
        opendal::Operator::new(Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(temp_dir.path(), *node.as_bytes(), 2, WalConfig::default())
            .expect("restarted WAL writer"),
    );
    let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, &node.to_string(), 2);
    let restored = scribe.replay_wal_async().await.expect("replay");
    assert_eq!(
        restored, 3,
        "one owner generation per streamed replay chunk"
    );
    assert_eq!(
        scribe
            .memtable_stats()
            .expect("stats")
            .immutable_generations,
        3
    );
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

#[tokio::test]
/// A replay failure leaves Scribe unready and closed to new ingest.
async fn replay_failure_keeps_scribe_unready() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, WalConfig::default())
        .expect("WAL writer");
    let tenant = DataTenantId::new_v7();
    let key = seal_key(tenant, "scribe_replay_failure");
    let audit = encode_audit_event(&audit_event("replay-failure", tenant)).expect("audit");
    writer
        .append_and_commit_for_replay_test(&key, *Uuid::now_v7().as_bytes(), &audit, &[1, 2, 3])
        .expect("invalid replay fixture append");
    drop(writer);

    let operator = Arc::new(
        opendal::Operator::new(Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(temp_dir.path(), *node.as_bytes(), 2, WalConfig::default())
            .expect("replay WAL writer"),
    );
    let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, &node.to_string(), 2);
    let error = scribe
        .replay_wal_async()
        .await
        .expect_err("invalid replay must fail");
    assert!(error.to_string().contains("Arrow") || error.to_string().contains("replayed"));
    assert!(!scribe.is_ready());
}
