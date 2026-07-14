//! Scribe implementation — WAL append, fsync, replay, and memtable (PR#3/PR#4).

pub mod audit_envelope;
pub mod manifest;
pub mod memtable;
pub mod replay;
pub mod seal_key;
pub mod stream_identity;
pub mod wal;

use async_trait::async_trait;
use std::sync::Arc;

use crate::contracts::{AppendAck, Scribe, ScribeAppend, ScribeError};
#[cfg(feature = "scribe-inspect")]
use crate::inspect::{MemtableKey, ScribeInspect};
use crate::scribe::memtable::Memtable;

/// Scribe implementation with WAL append, fsync, replay, and memtable (PR#3/PR#4).
///
/// `append` splits the batch by event day, writes paired (audit, data) WAL records,
/// fsyncs, and forwards to the memtable. Seal predicate triggers freeze at first-of:
/// 50k rows | 1s | 128 MiB | 5s inactivity.
#[derive(Debug)]
pub struct ScribeImpl {
    /// In-memory memtable keyed by seal-key.
    memtable: Arc<Memtable>,
    // TODO PR#5: WalWriter per seal-key for real WAL integration
}

impl ScribeImpl {
    /// Construct a new `ScribeImpl` with empty memtable.
    #[must_use]
    pub fn new() -> Self {
        Self {
            memtable: Arc::new(Memtable::new()),
        }
    }
}

impl Default for ScribeImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scribe for ScribeImpl {
    async fn append(&self, _req: ScribeAppend) -> Result<AppendAck, ScribeError> {
        // Generate batch_id using UUID v7 (time-ordered, monotonic)
        let batch_id = *uuid::Uuid::now_v7().as_bytes();

        // TODO PR#5: Real implementation flow:
        // 1. Parse table_fqn → TableRef
        // 2. split_batch_by_event_day(req.batch_data) → Vec<DaySlice>
        // 3. For each slice:
        //    - Construct SealKey(tenant, table, day)
        //    - Build AuditEvent from req.principal
        //    - Encode AuditEvent → audit_bytes (via audit_envelope::encode_audit_event)
        //    - Encode RecordBatch → data_bytes (Arrow IPC)
        //    - WalWriter::append_and_fsync(audit_bytes, data_bytes) → lsn
        //    - Build ScribeAppendMeta { batch_id, rows, lsn_min, lsn_max, seal_key }
        //    - Forward to memtable.insert(seal_key, event, meta, batch)
        //    - Check memtable.should_seal(seal_key)
        //    - If seal: memtable.freeze(seal_key) → FrozenMemtable
        // 4. Return AppendAck with real batch_id

        // Stub for PR#4 — real WAL + memtable integration in PR#5
        Ok(AppendAck {
            batch_id,
            tenant: wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
        })
    }
}

#[cfg(feature = "scribe-inspect")]
#[async_trait]
impl ScribeInspect for ScribeImpl {
    fn wal_pending_bytes(&self) -> u64 {
        0 // TODO PR#5: Real WAL pending bytes
    }

    fn memtable_row_count(&self, key: &MemtableKey) -> usize {
        self.memtable.row_count(key).unwrap_or(0)
    }

    fn sealed_parquet_paths(&self) -> Vec<String> {
        vec![] // TODO PR#6: Real sealed Parquet paths
    }

    async fn force_seal(&self) -> Result<(), ScribeError> {
        Ok(()) // TODO PR#6: Real seal state machine
    }
}
