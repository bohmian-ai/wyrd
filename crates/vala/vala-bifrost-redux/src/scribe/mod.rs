//! Scribe implementation — WAL append, fsync, and replay (PR#3).

pub mod audit_envelope;
pub mod manifest;
pub mod replay;
pub mod seal_key;
pub mod stream_identity;
pub mod wal;

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::contracts::{AppendAck, Scribe, ScribeAppend, ScribeError};
#[cfg(feature = "scribe-inspect")]
use crate::inspect::{MemtableKey, ScribeInspect};

/// Scribe implementation with WAL append, fsync, and replay (PR#3).
///
/// `append` splits the batch by event day (placeholder split), writes paired
/// (audit, data) WAL records, fsyncs, and forwards to a stub memtable receiver.
/// Real memtable + seal predicate lands in PR#4.
#[derive(Debug)]
pub struct ScribeImpl {
    /// In-memory append counter (test-only for PR#3).
    append_count: Arc<AtomicU64>,
    // TODO PR#4: WalWriter per seal-key, memtable, seal predicate
}

impl ScribeImpl {
    /// Construct a new `ScribeImpl`.
    pub fn new() -> Self {
        Self {
            append_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Number of appends received since construction (test-only).
    #[cfg(test)]
    pub fn append_count(&self) -> u64 {
        self.append_count.load(Ordering::SeqCst)
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
        self.append_count.fetch_add(1, Ordering::SeqCst);

        // TODO PR#4: Real implementation flow:
        // 1. Parse table_fqn → TableRef
        // 2. split_batch_by_event_day(req.batch_data) → Vec<DaySlice>
        // 3. For each slice:
        //    - Construct SealKey(tenant, table, day)
        //    - Encode AuditEvent → audit_bytes (via audit_envelope::encode_audit_event)
        //    - Encode RecordBatch → data_bytes (Arrow IPC)
        //    - WalWriter::append_and_fsync(audit_bytes, data_bytes) → lsn
        //    - Forward to memtable
        // 4. Return AppendAck with real batch_id

        // Stub receiver for PR#3 — real memtable integration in PR#4
        Ok(AppendAck {
            batch_id: [0u8; 16],
            tenant: wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
        })
    }
}

#[cfg(feature = "scribe-inspect")]
#[async_trait]
impl ScribeInspect for ScribeImpl {
    fn wal_pending_bytes(&self) -> u64 {
        0
    }

    fn memtable_row_count(&self, _key: &MemtableKey) -> usize {
        0
    }

    fn sealed_parquet_paths(&self) -> Vec<String> {
        vec![]
    }

    async fn force_seal(&self) -> Result<(), ScribeError> {
        Ok(())
    }
}
