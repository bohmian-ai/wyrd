//! Concrete WAL/manifest crash-seam coverage for Task 15.

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use chrono::NaiveDate;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::audit_envelope::encode_audit_event;
use vala_bifrost_redux::scribe::manifest::{Manifest, read_manifest, write_atomic};
use vala_bifrost_redux::scribe::replay::replay_wal_directory;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::wal::{WalConfig, WalLsn, WalRecord, WalWriter};
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

fn data_bytes(value: i64) -> Vec<u8> {
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

fn audit() -> Vec<u8> {
    encode_audit_event(&AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: "task15.crash-seam".to_owned(),
        resource: "vala.bifrost.task15".to_owned(),
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

fn key() -> SealKey {
    SealKey::new(
        DataTenantId::new_v7(),
        TableRef::new(BifrostNamespace::Bifrost, "task15_crash"),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 24).expect("test day")),
    )
}

#[test]
fn torn_wal_tail_is_truncated_and_prior_records_replay() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, WalConfig::default())
        .expect("WAL writer");
    let key = key();
    writer
        .append_and_fsync_for_test(&key, [1_u8; 16], &audit(), &data_bytes(1))
        .expect("append");
    let path = temp_dir
        .path()
        .join(node.as_uuid().simple().to_string())
        .join("1")
        .join(format!(
            "shard-{:02}",
            vala_bifrost_redux::scribe::routing::shard_for(key.tenant, &key.table)
        ))
        .join("0.wal");
    let valid_len = std::fs::metadata(&path).expect("WAL metadata").len();
    let mut file = OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open WAL tail");
    file.write_all(&[0xff; 13]).expect("torn bytes");
    file.sync_all().expect("sync torn bytes");

    let replayed = replay_wal_directory(temp_dir.path()).expect("replay torn tail");
    assert_eq!(replayed.len(), 1);
    assert_eq!(
        std::fs::metadata(path).expect("WAL metadata").len(),
        valid_len
    );
}

#[test]
fn partial_frame_leaves_only_the_prior_acknowledged_prefix() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, WalConfig::default())
        .expect("WAL writer");
    let key = key();
    writer
        .append_and_fsync_for_test(&key, [1_u8; 16], &audit(), &data_bytes(1))
        .expect("acknowledged prefix append");
    let path = temp_dir
        .path()
        .join(node.as_uuid().simple().to_string())
        .join("1")
        .join(format!(
            "shard-{:02}",
            vala_bifrost_redux::scribe::routing::shard_for(key.tenant, &key.table)
        ))
        .join("0.wal");
    let valid_len = std::fs::metadata(&path).expect("WAL metadata").len();
    let partial = WalRecord::new(WalLsn::new(2), 2, [2_u8; 16], vec![1, 2, 3]).encode();
    let mut file = OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open WAL tail");
    file.write_all(&partial[..partial.len() / 2])
        .expect("partial WAL frame");
    file.sync_all().expect("sync partial WAL frame");

    let replayed = replay_wal_directory(temp_dir.path()).expect("replay partial frame");
    assert_eq!(replayed[&key.as_path_components()].data_records.len(), 1);
    assert_eq!(
        std::fs::metadata(path).expect("WAL metadata").len(),
        valid_len,
        "unacknowledged partial frame must be truncated"
    );
}

#[test]
fn segment_roll_keeps_all_complete_records_replayable() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(
        temp_dir.path(),
        *node.as_bytes(),
        1,
        WalConfig::new(256).expect("small segment config"),
    )
    .expect("WAL writer");
    let key = key();
    for value in 0_u8..4 {
        writer
            .append_and_fsync_for_test(&key, [value; 16], &audit(), &data_bytes(i64::from(value)))
            .expect("append");
    }
    let replayed = replay_wal_directory(temp_dir.path()).expect("replay rolled segments");
    assert_eq!(replayed[&key.as_path_components()].data_records.len(), 4);
}

#[test]
fn atomic_manifest_ignores_a_crashed_temporary_replacement() {
    let temp_dir = TempDir::new().expect("manifest directory");
    let identity = StreamIdentity::new(NodeId::new(Uuid::now_v7()), WriterEpoch::new(1));
    let path = temp_dir.path().join("manifest");
    write_atomic(&path, &Manifest::new(identity)).expect("manifest write");
    std::fs::write(path.with_extension("tmp"), b"partial manifest").expect("partial temp");
    let restored = read_manifest(&path)
        .expect("manifest read")
        .expect("manifest");
    assert_eq!(restored.stream_identity.writer_epoch, 1);
}
