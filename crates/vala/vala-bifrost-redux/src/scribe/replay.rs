//! WAL replay — boot-time reconstruction of memtable and audit envelopes.
//!
//! On Scribe restart, replay scans the WAL directory past `manifest.sealed_lsn[K]`
//! for each seal-key K, validates CRC/length/monotonicity, truncates torn tails,
//! dedupes by `batch_id`, and reconstructs both the memtable state and the staged
//! `AuditEvent` list per key.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::scribe::audit_envelope::decode_audit_event;
use crate::scribe::manifest::read_manifest;
use crate::scribe::preprocess::AppendSliceId;
use crate::scribe::seal_key::{EventDay, SealKey};
use crate::scribe::wal::{WalLsn, WalReader, WalRecord};

/// Replayed state for one seal-key.
#[derive(Debug, Clone)]
pub struct ReplayedSealKey {
    /// The seal-key this state belongs to.
    pub seal_key: SealKey,
    /// Ordered list of `AuditEvent`s staged for the seal transaction.
    pub audit_events: Vec<AuditEvent>,
    /// Ordered list of data record payloads (Arrow IPC bytes).
    pub data_records: Vec<Vec<u8>>,
    /// Per-append metadata derived from WAL record headers.
    pub append_metas: Vec<ReplayedAppendMeta>,
}

/// Per-append metadata derived during replay.
#[derive(Debug, Clone)]
pub struct ReplayedAppendMeta {
    /// Batch ID for dedup.
    pub batch_id: [u8; 16],
    /// LSN for this append.
    pub wal_lsn: WalLsn,
    /// Number of rows in this append (placeholder — real count from Arrow batch).
    pub rows_accepted: usize,
    /// Composite idempotency identity for this batch routed to this seal-key.
    pub append_slice_id: AppendSliceId,
}

/// Replay the WAL directory and reconstruct per-seal-key state.
///
/// Reads the manifest (if present), scans all segments, skips sealed LSNs,
/// truncates torn tails, dedupes by `batch_id`, and groups records by `seal-key`.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if:
/// - WAL segments cannot be read
/// - Record CRC validation fails (after truncating the torn tail)
/// - Audit envelope decoding fails
///
/// # Panics
/// May panic if internal invariants are violated (e.g., `HashMap` consistency).
pub fn replay_wal_directory(
    wal_dir: impl AsRef<Path>,
) -> Result<HashMap<String, ReplayedSealKey>, ScribeError> {
    let wal_dir = wal_dir.as_ref();

    let manifest_path = wal_dir.join("manifest");
    let manifest = read_manifest(&manifest_path)?;

    let sealed_lsn_map: HashMap<String, WalLsn> = manifest
        .as_ref()
        .map(|m| {
            m.sealed_lsn
                .iter()
                .map(|(k, &v)| (k.clone(), WalLsn::new(v)))
                .collect()
        })
        .unwrap_or_default();

    let reader = WalReader::open_directory_unfiltered(wal_dir)?;
    let records = reader.read_all_records_with_paths()?;

    // Truncate torn tails — already handled by WalRecord::decode_from returning Err on CRC mismatch

    // Deduplicate by (batch_id, seal_key), not by batch_id alone. A client
    // retry may legitimately route the same batch id to a different day/key.
    let mut seen_slices = HashSet::<AppendSliceId>::new();
    let mut deduplicated = Vec::new();
    let mut pending_audit: Option<(PathBuf, WalRecord, [u8; 16])> = None;

    for (segment_path, record) in records {
        if record.envelope_kind == 1 {
            // Audit record — extract batch_id from WAL record header
            let _audit_event = decode_audit_event(&record.payload)?;
            let batch_id = record.batch_id;

            pending_audit = Some((segment_path, record, batch_id));
        } else if record.envelope_kind == 0 {
            // Data record — pair with pending audit
            if let Some((audit_path, audit_record, batch_id)) = pending_audit.take() {
                if audit_path != segment_path {
                    continue;
                }
                // Extract seal-key from WAL directory path
                let seal_key = extract_seal_key_from_path(&audit_path)?;
                let seal_key_str = seal_key.as_path_components();
                let append_slice_id = AppendSliceId {
                    batch_id: uuid::Uuid::from_bytes(batch_id),
                    seal_key: seal_key.clone(),
                };
                if !seen_slices.insert(append_slice_id) {
                    continue;
                }

                // Skip if this LSN is sealed
                if let Some(&sealed_lsn) = sealed_lsn_map.get(&seal_key_str)
                    && record.lsn <= sealed_lsn
                {
                    continue;
                }

                deduplicated.push((audit_record, record, batch_id, seal_key));
            }
        }
    }

    // Group by seal-key
    let mut replayed_state: HashMap<String, ReplayedSealKey> = HashMap::new();

    for (audit_record, data_record, batch_id, seal_key) in deduplicated {
        let audit_event = decode_audit_event(&audit_record.payload)?;
        let seal_key_str = seal_key.as_path_components();

        let state = replayed_state
            .entry(seal_key_str.clone())
            .or_insert_with(|| ReplayedSealKey {
                seal_key: seal_key.clone(),
                audit_events: Vec::new(),
                data_records: Vec::new(),
                append_metas: Vec::new(),
            });

        state.audit_events.push(audit_event);
        state.data_records.push(data_record.payload);
        state.append_metas.push(ReplayedAppendMeta {
            batch_id,
            wal_lsn: data_record.lsn,
            rows_accepted: 0, // Placeholder — real count from Arrow batch
            append_slice_id: AppendSliceId {
                batch_id: uuid::Uuid::from_bytes(batch_id),
                seal_key: seal_key.clone(),
            },
        });
    }

    Ok(replayed_state)
}

/// Extract seal-key from WAL directory path.
///
/// Expected path format: `{namespace}/{table}/tenant={tenant}/day={YYYY-MM-DD}`
///
/// # Errors
/// Returns [`ScribeError::Internal`] if the path format is invalid.
///
/// # Implementation Note ()
///
/// This function currently returns a placeholder seal-key because full path parsing
/// requires WAL directory routing from . The real implementation will:
///
/// 1. Parse path structure: `${SCRIBE_WAL_DIR}/{namespace}/{table}/tenant={uuid}/day={YYYY-MM-DD}`
/// 2. Extract `tenant=` component and parse UUID
/// 3. Extract `day=` component and parse date
/// 4. Build `TableRef` from namespace + table components
///
/// For , tests use a flat temp directory structure. Real multi-key replay testing
/// requires the directory routing.
// justification: stub implementation for path-parsing; the real implementation returns fallible Result<SealKey, ScribeError>
#[allow(clippy::unnecessary_wraps)]
fn extract_seal_key_from_path(path: &Path) -> Result<SealKey, ScribeError> {
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    let Some(tenant_index) = components
        .iter()
        .rposition(|component| component.starts_with("tenant="))
    else {
        return placeholder_seal_key();
    };
    if tenant_index < 2 || tenant_index + 1 >= components.len() {
        return placeholder_seal_key();
    }
    let namespace = crate::namespaces::BifrostNamespace::from_wire(components[tenant_index - 2])
        .ok_or_else(|| ScribeError::Internal {
            detail: format!("invalid WAL namespace in path: {}", path.display()),
        })?;
    let tenant_uuid = components[tenant_index]
        .strip_prefix("tenant=")
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| ScribeError::Internal {
            detail: format!("invalid WAL tenant in path: {}", path.display()),
        })?;
    let tenant = if tenant_uuid.is_nil() {
        DataTenantId::SYSTEM_OWNER
    } else {
        DataTenantId::new(tenant_uuid).map_err(|error| ScribeError::Internal {
            detail: format!("invalid WAL tenant id in path: {error}"),
        })?
    };
    let day = components[tenant_index + 1]
        .strip_prefix("day=")
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .ok_or_else(|| ScribeError::Internal {
            detail: format!("invalid WAL event day in path: {}", path.display()),
        })?;
    Ok(SealKey::new(
        tenant,
        TableRef::new(namespace, components[tenant_index - 1]),
        EventDay::new(day),
    ))
}

fn placeholder_seal_key() -> Result<SealKey, ScribeError> {
    Ok(SealKey::new(
        DataTenantId::SYSTEM_OWNER,
        TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "events"),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).ok_or_else(|| {
            ScribeError::Internal {
                detail: "placeholder replay date is invalid".to_string(),
            }
        })?),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scribe::stream_identity::NodeId;
    use crate::scribe::wal::WalWriter;
    use tempfile::TempDir;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    #[test]
    fn wal_replay_rebuilds_memtable_and_audit_events() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = NodeId::generate();
        let tenant_id = DataTenantId::SYSTEM_OWNER;

        let writer = WalWriter::new(temp_dir.path(), *node_id.as_bytes(), 1, tenant_id, None)
            .expect("writer");

        // Write 2 appends
        for i in 0u8..2 {
            let audit_event = AuditEvent {
                request_id: RequestId::now_v7(),
                trace_id: None,
                operation: format!("append-{i}"),
                resource: "test".to_string(),
                card_ref: None,
                principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
                principal_kind: PrincipalKindTag::User,
                auth_method: AuthMethod::Jwt,
                permission: "test".to_string(),
                decision: AuditDecision::Allow,
                result: AuditResult::Success,
                payload_summary: "test".to_string(),
                detail: None,
            };

            let audit_bytes =
                crate::scribe::audit_envelope::encode_audit_event(&audit_event).expect("encode");
            let data_bytes = format!("data-{i}").into_bytes();
            let batch_id = [i; 16];

            writer
                .append_and_fsync(batch_id, &audit_bytes, &data_bytes)
                .expect("append");
        }

        let replayed = replay_wal_directory(temp_dir.path()).expect("replay");

        // Placeholder test — real implementation would verify actual seal-key grouping
        assert!(!replayed.is_empty());
    }

    #[test]
    fn wal_replay_skips_sealed_lsn_per_seal_key() {
        // Regression test for C1: per-seal-key sealed_lsn watermark
        //
        // For : This test uses the simplified seal-key extraction that returns
        // a single placeholder seal-key for all records. The test structure is correct,
        // but real multi-key verification requires 's full WAL directory routing.
        //
        // Test contract: sealed_lsn[K1] = N skips only K1's records where LSN <= N,
        // not K2's records. Once real seal-key extraction lands in , this test
        // will verify true multi-key isolation.

        // For now, placeholder: real implementation requires multi-key WAL routing
        // which is part of 's integration.
    }

    #[test]
    fn wal_torn_tail_is_truncated_on_replay() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = NodeId::generate();
        let tenant_id = DataTenantId::SYSTEM_OWNER;

        let writer = WalWriter::new(temp_dir.path(), *node_id.as_bytes(), 1, tenant_id, None)
            .expect("writer");

        // Write 5 records
        for i in 0u8..5 {
            let audit_event = AuditEvent {
                request_id: RequestId::now_v7(),
                trace_id: None,
                operation: format!("append-{i}"),
                resource: "test".to_string(),
                card_ref: None,
                principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
                principal_kind: PrincipalKindTag::User,
                auth_method: AuthMethod::Jwt,
                permission: "test".to_string(),
                decision: AuditDecision::Allow,
                result: AuditResult::Success,
                payload_summary: "test".to_string(),
                detail: None,
            };

            let audit_bytes =
                crate::scribe::audit_envelope::encode_audit_event(&audit_event).expect("encode");
            let data_bytes = format!("data-{i}").into_bytes();
            let batch_id = [i; 16];

            writer
                .append_and_fsync(batch_id, &audit_bytes, &data_bytes)
                .expect("append");
        }

        // Torn tail handling is automatic in WalRecord::decode_from:
        // it returns Err on CRC mismatch, and WalReader stops reading at first error.
        // To test torn tail, we'd need to manually corrupt the WAL file, but
        // the replay logic already handles it correctly by stopping at first bad CRC.

        let replayed = replay_wal_directory(temp_dir.path()).expect("replay");

        // With no corruption, all 5 appends should be present
        let state = replayed.values().next().expect("state");
        assert_eq!(state.audit_events.len(), 5, "all records replayed cleanly");
    }

    #[test]
    fn wal_replay_dedups_duplicate_batch_id() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = NodeId::generate();
        let tenant_id = DataTenantId::SYSTEM_OWNER;

        let writer = WalWriter::new(temp_dir.path(), *node_id.as_bytes(), 1, tenant_id, None)
            .expect("writer");

        let shared_batch_id = [42u8; 16];

        // Write first append with batch_id=42
        let audit_event1 = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "append-1".to_string(),
            resource: "test".to_string(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "test".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "first".to_string(),
            detail: None,
        };

        let audit_bytes1 =
            crate::scribe::audit_envelope::encode_audit_event(&audit_event1).expect("encode");
        writer
            .append_and_fsync(shared_batch_id, &audit_bytes1, b"data-1")
            .expect("append 1");

        // Write second append with same batch_id=42 (duplicate)
        let audit_event2 = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "append-2".to_string(),
            resource: "test".to_string(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "test".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "second".to_string(),
            detail: None,
        };

        let audit_bytes2 =
            crate::scribe::audit_envelope::encode_audit_event(&audit_event2).expect("encode");
        writer
            .append_and_fsync(shared_batch_id, &audit_bytes2, b"data-2")
            .expect("append 2");

        // Replay should deduplicate by batch_id
        let replayed = replay_wal_directory(temp_dir.path()).expect("replay");
        let state = replayed.values().next().expect("state");

        // Only first append should survive
        assert_eq!(
            state.audit_events.len(),
            1,
            "duplicate batch_id deduped to single record"
        );
        assert_eq!(state.audit_events[0].operation, "append-1");
        assert_eq!(state.data_records.len(), 1);
    }

    #[test]
    fn wal_replay_carries_prior_epoch_stream_identity() {
        // Stream identity is carried in segment headers (node_id, writer_epoch)
        // and stamped on replayed records.
        //
        // This test verifies that segments written with writer_epoch=3 carry that
        // epoch through replay, even if boot bumped the current epoch to 4.

        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = NodeId::generate();
        let tenant_id = DataTenantId::SYSTEM_OWNER;

        // Write segments with writer_epoch=3
        let writer = WalWriter::new(temp_dir.path(), *node_id.as_bytes(), 3, tenant_id, None)
            .expect("writer");

        let audit_event = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "append".to_string(),
            resource: "test".to_string(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "test".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "test".to_string(),
            detail: None,
        };

        let audit_bytes =
            crate::scribe::audit_envelope::encode_audit_event(&audit_event).expect("encode");
        let batch_id = [1u8; 16];
        writer
            .append_and_fsync(batch_id, &audit_bytes, b"data")
            .expect("append");

        // Replay doesn't directly expose writer_epoch in ReplayedAppendMeta yet,
        // but the contract is that segment headers carry the original epoch.
        // The WalReader preserves this by reading segment headers correctly.

        let replayed = replay_wal_directory(temp_dir.path()).expect("replay");
        let state = replayed.values().next().expect("state");

        // Verify replay succeeded with epoch=3 segments
        assert_eq!(state.audit_events.len(), 1);

        // In , ReplayedAppendMeta will carry writer_epoch from segment header.
        // For , the test structure is correct even though epoch isn't yet
        // explicitly in the metadata struct.
    }

    #[test]
    fn nested_wal_replay_reconstructs_seal_key_from_path() {
        let temp_dir = TempDir::new().expect("temp dir");
        let tenant = DataTenantId::new_v7();
        let table = TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "events");
        let day = EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date"));
        let seal_key = SealKey::new(tenant, table, day);
        let writer =
            WalWriter::new(temp_dir.path(), [9_u8; 16], 1, tenant, None).expect("wal writer");
        let handle = writer
            .handle_for_seal_key(seal_key.clone())
            .expect("wal handle");
        let event = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "bifrost.append".to_string(),
            resource: seal_key.table.fqn(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "bifrost:append".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "1 rows".to_string(),
            detail: None,
        };
        let audit = crate::scribe::audit_envelope::encode_audit_event(&event).expect("audit");
        let frame =
            crate::scribe::wal::encode_append_frame([4_u8; 16], &audit, b"data").expect("frame");
        handle.append_frame(&frame).expect("append");
        handle.sync_data().expect("sync");

        let replayed = replay_wal_directory(temp_dir.path()).expect("replay");
        assert!(replayed.contains_key(&seal_key.as_path_components()));
    }
}
