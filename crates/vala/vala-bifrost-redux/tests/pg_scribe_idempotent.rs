//! Durable WAL idempotency and replay coverage for the Scribe persistence path.

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use chrono::NaiveDate;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::audit_envelope::encode_audit_event;
use vala_bifrost_redux::scribe::replay::replay_wal_directory;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

fn batch_bytes(value: i64) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Int64Array::from(vec![value]))],
    )
    .expect("test batch");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC finish");
    bytes
}

fn audit_event(operation: &str) -> AuditEvent {
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
        payload_summary: "1 rows".to_owned(),
        detail: None,
    }
}

fn seal_key(tenant: DataTenantId) -> SealKey {
    SealKey::new(
        tenant,
        TableRef::new(BifrostNamespace::Bifrost, "scribe_idempotent"),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 24).expect("test day")),
    )
}

#[test]
fn duplicate_batch_id_replays_once_and_preserves_the_first_audit() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let writer = WalWriter::new(
        temp_dir.path(),
        *Uuid::now_v7().as_bytes(),
        1,
        WalConfig::default(),
    )
    .expect("WAL writer");
    let key = seal_key(DataTenantId::new_v7());
    let batch_id = [9_u8; 16];
    let first = encode_audit_event(&audit_event("first")).expect("audit");
    let second = encode_audit_event(&audit_event("duplicate")).expect("audit");
    let data = batch_bytes(42);

    writer
        .append_and_fsync_for_test(&key, batch_id, &first, &data)
        .expect("first WAL append");
    writer
        .append_and_fsync_for_test(&key, batch_id, &second, &data)
        .expect("duplicate WAL append");

    let replayed = replay_wal_directory(temp_dir.path()).expect("replay");
    let state = replayed.get(&key.as_path_components()).expect("state");
    assert_eq!(state.data_records.len(), 1);
    assert_eq!(state.audit_events.len(), 1);
    assert_eq!(state.audit_events[0].operation, "first");
}
