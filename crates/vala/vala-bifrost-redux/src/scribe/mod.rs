//! Scribe implementation — stub for PR#1.

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::contracts::{AppendAck, Scribe, ScribeAppend, ScribeError};
#[cfg(feature = "scribe-inspect")]
use crate::inspect::{MemtableKey, ScribeInspect};

/// Stub Scribe implementation for PR#1.
///
/// `append` records to an in-memory counter only. No real WAL, memtable, seal,
/// or Parquet writer.
#[derive(Debug)]
pub struct ScribeImpl {
    append_count: Arc<AtomicU64>,
}

impl ScribeImpl {
    /// Construct a new stub `ScribeImpl`.
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
