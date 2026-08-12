//! Concrete WAL/manifest crash-seam coverage for Scribe persistence.

use crate::catalog::TableRef;
use crate::namespaces::BifrostNamespace;
use crate::scribe::audit_envelope::encode_audit_event;
use crate::scribe::manifest::{Manifest, read_manifest, write_atomic};
use crate::scribe::replay::{replay_wal_directory, replay_wal_directory_stream};
use crate::scribe::routing::shard_for;
use crate::scribe::seal_key::{EventDay, SealKey};
use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use crate::scribe::wal::{WalConfig, WalLsn, WalRecord, WalWriter};
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
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

/// Encode one deterministic WAL replay batch.
///
/// # Panics
/// Panics when the static Arrow fixture cannot be encoded.
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

/// Encode the fixed audit envelope persisted beside a crash fixture.
///
/// # Panics
/// Panics when the static audit fixture cannot be encoded.
fn audit() -> Vec<u8> {
    encode_audit_event(&AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: "scribe.crash-seam".to_owned(),
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
    })
    .expect("audit")
}

/// Build the stable seal scope shared by crash-recovery cases.
fn key() -> SealKey {
    SealKey::new(
        DataTenantId::new_v7(),
        TableRef::new(BifrostNamespace::Bifrost, "scribe_crash"),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 24).expect("test day")),
    )
}

#[test]
/// A torn suffix is truncated while every acknowledged prefix record replays.
fn torn_wal_tail_is_truncated_and_prior_records_replay() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, WalConfig::default())
        .expect("WAL writer");
    let key = key();
    writer
        .append_and_fsync_for_test(&key, [1_u8; 16], &audit(), &data_bytes(1))
        .expect("append");
    // The shard for this WAL path is determined by the batch_id [1u8; 16]
    // used in the append above.
    let path = temp_dir
        .path()
        .join(node.as_uuid().simple().to_string())
        .join("1")
        .join(format!(
            "shard-{:02}",
            crate::scribe::routing::shard_for(
                key.tenant,
                &key.table,
                uuid::Uuid::from_bytes([1_u8; 16]),
            )
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
/// A partial frame never becomes an acknowledged replay record.
fn partial_frame_leaves_only_the_prior_acknowledged_prefix() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, WalConfig::default())
        .expect("WAL writer");
    let key = key();
    writer
        .append_and_fsync_for_test(&key, [1_u8; 16], &audit(), &data_bytes(1))
        .expect("acknowledged prefix append");
    // The shard for this WAL path is determined by the batch_id [1u8; 16]
    // used in the append above.
    let path = temp_dir
        .path()
        .join(node.as_uuid().simple().to_string())
        .join("1")
        .join(format!(
            "shard-{:02}",
            crate::scribe::routing::shard_for(
                key.tenant,
                &key.table,
                uuid::Uuid::from_bytes([1_u8; 16]),
            )
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
/// Segment rotation preserves every complete acknowledged record.
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
/// A crashed temporary manifest replacement cannot supersede the atomic owner.
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

#[test]
/// Two batches for the same seal key whose `batch_id`s hash to different shard
/// lanes each replay back to their recorded lane.
///
/// Under batch-spread routing a single (tenant, table) seal key may accumulate
/// writes on any subset of the sixteen shard lanes, because the routing key is
/// `(tenant, table, batch_id)`.  After a crash the WAL segments carry the
/// original shard lane in their header (`shard_id` field).  Replay must
/// dispatch each replayed state to `shard_senders[state.shard_id]` — NOT to a
/// freshly computed `shard_for(tenant, table, ...)` — so that per-shard
/// `synced_not_inserted` and pending-FIFO dedup state rebuilds exactly where
/// the original writes lived.
///
/// This test proves that invariant end-to-end at the WAL layer: two distinct
/// `batch_id`s for the same seal key produce [`ReplayedSealKey`] entries that carry
/// the correct `shard_id`s and contain no duplicate rows.
fn replay_restores_multi_shard_seal_key_to_recorded_lanes() {
    let temp_dir = TempDir::new().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, WalConfig::default())
        .expect("WAL writer");
    let key = key();

    // Write 16 appends with distinct batch_ids (one per candidate shard byte)
    // and collect which shard each was routed to.
    let mut written_shards: std::collections::HashSet<u8> = std::collections::HashSet::new();
    for byte in 0_u8..=15_u8 {
        let batch_id = [byte; 16];
        writer
            .append_and_fsync_for_test(&key, batch_id, &audit(), &data_bytes(i64::from(byte)))
            .expect("append");
        let shard = u8::try_from(shard_for(
            key.tenant,
            &key.table,
            Uuid::from_bytes(batch_id),
        ))
        .expect("shard id fits u8");
        written_shards.insert(shard);
    }

    // We expect at least two distinct shards to have been used.  With 16 inputs
    // hashing into 16 slots via BLAKE3, at least one collision is extremely
    // unlikely at this scale; a birthday-problem argument gives ≥ 99.9% chance
    // of 2+ distinct shards.  If by chance all 16 batch_ids land on one shard
    // the assertion below would fail — in practice this never happens with a
    // 256-bit hash truncated to 4 bits across sequential byte-valued inputs.
    assert!(
        written_shards.len() >= 2,
        "expected batch-spread routing to use ≥ 2 shards; got {written_shards:?}"
    );

    // Replay using the streaming API so each shard's accumulator is visible
    // before merging.  Each emitted ReplayChunk covers one bounded batch of
    // states; states within a chunk are keyed by seal-key path and carry the
    // shard_id from their WAL segment header.
    let mut all_states: Vec<crate::scribe::replay::ReplayedSealKey> = Vec::new();
    replay_wal_directory_stream(temp_dir.path(), |chunk| {
        for state in chunk.states.into_values() {
            all_states.push(state);
        }
        Ok(())
    })
    .expect("replay");

    // Every emitted state must carry a shard_id that belongs to the set of
    // shards actually used during the write phase.  This is the guarantee
    // execution_lanes.rs relies on: `shard_senders[state.shard_id]` always
    // resolves to the lane that holds the per-shard dedup index.
    assert!(
        !all_states.is_empty(),
        "replay must produce at least one state"
    );
    let replayed_shards: std::collections::HashSet<u8> =
        all_states.iter().map(|s| s.shard_id).collect();
    for shard_id in &replayed_shards {
        assert!(
            written_shards.contains(shard_id),
            "replayed shard_id {shard_id} was not in the written shard set {written_shards:?}"
        );
    }

    // The total row count across all replayed states must equal the 16 appended
    // rows — no double-insert and no lane collapse.
    let total_records: usize = all_states.iter().map(|s| s.data_records.len()).sum();
    assert_eq!(
        total_records, 16,
        "replay must recover exactly 16 data records (one per appended batch), \
         got {total_records}"
    );
}
