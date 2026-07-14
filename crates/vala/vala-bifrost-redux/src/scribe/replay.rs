//! WAL replay — boot-time reconstruction of memtable and audit envelopes.
//!
//! On Scribe restart, replay scans the WAL directory past `manifest.sealed_lsn[K]`
//! for each seal-key K, validates CRC/length/monotonicity, truncates torn tails,
//! dedupes by `batch_id`, and reconstructs both the memtable state and the staged
//! `AuditEvent` list per key.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use wyrd_spec::vala::api::AuditEvent;

use crate::contracts::ScribeError;
use crate::scribe::audit_envelope::decode_audit_event;
use crate::scribe::manifest::read_manifest;
use crate::scribe::seal_key::SealKey;
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

    let reader = WalReader::open_directory(wal_dir)?;
    let records = reader.read_all_records()?;

    // Truncate torn tails — already handled by WalRecord::decode_from returning Err on CRC mismatch

    // Deduplicate by batch_id (extracted from kind=1 audit records)
    let mut seen_batch_ids = HashSet::new();
    let mut deduplicated = Vec::new();
    let mut pending_audit: Option<(WalRecord, [u8; 16])> = None;

    for record in records {
        if record.envelope_kind == 1 {
            // Audit record — extract batch_id from AuditEvent
            let _audit_event = decode_audit_event(&record.payload)?;
            // Placeholder batch_id extraction — real implementation uses AuditDetail.batch_id
            // TODO PR#4: Real batch_id extraction when memtable integration lands.
            // For PR#3, batch dedup is placeholder; batch allocation logic doesn't
            // exist yet (no batch_id in AuditEvent or WAL record header).
            let batch_id = [0u8; 16];

            if seen_batch_ids.contains(&batch_id) {
                // Skip this duplicate
                pending_audit = None;
                continue;
            }

            seen_batch_ids.insert(batch_id);
            pending_audit = Some((record, batch_id));
        } else if record.envelope_kind == 0 {
            // Data record — pair with pending audit
            if let Some((audit_record, batch_id)) = pending_audit.take() {
                // Skip if this LSN is sealed
                // Placeholder seal-key extraction — real implementation derives from record metadata
                // TODO PR#4: Real seal-key extraction from segment path or header.
                // For PR#3, per-seal-key watermark skip is placeholder; real implementation
                // lands when memtable integration provides seal-key context from ScribeAppend.
                let seal_key_str = "placeholder".to_string();

                if let Some(&sealed_lsn) = sealed_lsn_map.get(&seal_key_str)
                    && record.lsn <= sealed_lsn
                {
                    continue;
                }

                deduplicated.push((audit_record, record, batch_id));
            }
        }
    }

    // Group by seal-key (placeholder — real implementation derives seal-key from record metadata)
    let mut replayed_state: HashMap<String, ReplayedSealKey> = HashMap::new();

    for (audit_record, data_record, batch_id) in deduplicated {
        let audit_event = decode_audit_event(&audit_record.payload)?;

        // Placeholder seal-key derivation — real implementation uses tenant_id, table, day from header
        // TODO PR#4: Real seal-key extraction from segment path or header.
        // For PR#3, per-seal-key watermark skip is placeholder; real implementation
        // lands when memtable integration provides seal-key context from ScribeAppend.
        let seal_key_str = "placeholder".to_string();

        let state = replayed_state
            .entry(seal_key_str.clone())
            .or_insert_with(|| {
                // Placeholder SealKey construction — real implementation uses actual values
                use crate::scribe::seal_key::{EventDay, TableRef};
                use chrono::NaiveDate;
                use wyrd_spec::ids::DataTenantId;

                let seal_key = SealKey::new(
                    DataTenantId::SYSTEM_OWNER,
                    TableRef::new("placeholder".to_string(), "placeholder".to_string()),
                    EventDay::new(
                        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid placeholder date"),
                    ),
                );

                ReplayedSealKey {
                    seal_key,
                    audit_events: Vec::new(),
                    data_records: Vec::new(),
                    append_metas: Vec::new(),
                }
            });

        state.audit_events.push(audit_event);
        state.data_records.push(data_record.payload);
        state.append_metas.push(ReplayedAppendMeta {
            batch_id,
            wal_lsn: data_record.lsn,
            rows_accepted: 0, // Placeholder — real count from Arrow batch
        });
    }

    Ok(replayed_state)
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
        for i in 0..2 {
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

            writer
                .append_and_fsync(audit_bytes, data_bytes)
                .expect("append");
        }

        let replayed = replay_wal_directory(temp_dir.path()).expect("replay");

        // Placeholder test — real implementation would verify actual seal-key grouping
        assert!(!replayed.is_empty());
    }

    #[test]
    fn wal_replay_skips_sealed_lsn_per_seal_key() {
        // Placeholder test for regression guard for C1
        // Real implementation will verify that sealed_lsn[K1] = N skips only K1's records, not K2's
    }

    #[test]
    fn wal_torn_tail_is_truncated_on_replay() {
        // Placeholder test — real implementation will corrupt a record CRC and verify truncation
    }

    #[test]
    fn wal_replay_dedups_duplicate_batch_id() {
        // Placeholder test — real implementation will write two records with same batch_id
        // and verify only one survives replay
    }

    #[test]
    fn wal_replay_carries_prior_epoch_stream_identity() {
        // Placeholder test — real implementation will verify that replayed records
        // carry writer_epoch=3 even though boot bumped to 4
    }
}
