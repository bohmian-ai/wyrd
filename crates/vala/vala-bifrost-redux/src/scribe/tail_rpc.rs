//! Typed, pod-local live-tail reads over writable and immutable memtable data.
//!
//! This module deliberately has no WAL, SQL, HTTP, tonic, or authentication
//! dependency. Oracle supplies an already-authorized tenant/table binding and
//! the Scribe validates only the stream identity and local data scope here.

use std::sync::Arc;

use arrow::array::{Array, StringArray};
use arrow::ipc::writer::StreamWriter;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::{TableRef, TenantTableBinding};
use crate::contracts::ScribeError;
use crate::scribe::memtable::{Memtable, ReadableBatch};
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::WalLsn;

/// Exact tenant/table scope for one live-tail read.
#[derive(Debug, Clone)]
pub struct LiveTailShard {
    /// Canonical organization-qualified physical binding.
    pub tenant_table: TenantTableBinding,
    /// Logical table requested by the caller.
    pub table: TableRef,
    /// Data tenant requested by the caller.
    pub tenant: DataTenantId,
}

/// Live-tail request handled by one current Scribe stream.
#[derive(Debug, Clone)]
pub struct FetchLiveTailRequest {
    /// Exact tenant/table shard to read.
    pub shard: LiveTailShard,
    /// The writer stream the caller believes it is talking to.
    pub target_stream: StreamIdentity,
    /// Emit only records with `LSN > after_lsn`.
    pub after_lsn: WalLsn,
}

/// Arrow IPC bytes for one complete admitted frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowIpcBatch {
    /// LSN of the append on the target stream.
    pub lsn: WalLsn,
    /// Idempotency batch ID copied from the append metadata.
    pub batch_id: [u8; 16],
    /// Arrow IPC-encoded `RecordBatch` bytes.
    pub arrow_ipc: Vec<u8>,
}

/// Terminal and data frames for one bounded tail fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailFrame {
    /// One complete, atomically emitted admitted frame.
    Batch(ArrowIpcBatch),
    /// All currently readable records after the requested LSN were emitted.
    Complete,
    /// The caller should resume strictly after this emitted LSN.
    Exhausted { resume_after_lsn: WalLsn },
}

/// Configuration for a live-tail reader.
#[derive(Debug, Clone, Copy)]
pub struct TailConfig {
    /// Maximum encoded Arrow bytes emitted before a terminal frame.
    pub max_bytes: usize,
}

impl Default for TailConfig {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Pod-local live-tail service over the Scribe memtable.
#[derive(Debug)]
pub struct FetchLiveTailService {
    stream: StreamIdentity,
    memtable: Arc<Memtable>,
    config: TailConfig,
}

impl FetchLiveTailService {
    /// Construct a reader with the default bounded response size.
    #[must_use]
    pub fn new(stream: StreamIdentity, memtable: Arc<Memtable>) -> Self {
        Self::with_config(stream, memtable, TailConfig::default())
    }

    /// Construct a reader with an explicit byte budget.
    #[must_use]
    pub fn with_config(
        stream: StreamIdentity,
        memtable: Arc<Memtable>,
        config: TailConfig,
    ) -> Self {
        Self {
            stream,
            memtable,
            config: TailConfig {
                max_bytes: config.max_bytes.max(1),
            },
        }
    }

    /// Stream identity this service serves.
    #[must_use]
    pub fn stream(&self) -> StreamIdentity {
        self.stream
    }

    /// Return the current writable and immutable batches for one shard.
    ///
    /// Stream mismatch is checked before any memtable read. Scope validation is
    /// performed before Arrow IPC projection, so a malformed or cross-tenant
    /// batch cannot be serialized into a response.
    #[allow(
        clippy::unused_async,
        reason = "the typed service boundary remains async for the future streaming transport"
    )]
    pub async fn fetch_live_tail(
        &self,
        req: FetchLiveTailRequest,
    ) -> Result<Vec<TailFrame>, ScribeError> {
        if req.target_stream != self.stream {
            return Err(ScribeError::StreamMismatch {
                requested: req.target_stream,
                actual: self.stream,
            });
        }

        if req.shard.tenant != req.shard.tenant_table.tenant
            || req.shard.table != req.shard.tenant_table.table_ref
        {
            return Err(ScribeError::Internal {
                detail: "live-tail shard does not match its tenant-table binding".to_owned(),
            });
        }

        let readable = self
            .memtable
            .readable_batches(req.shard.tenant, &req.shard.table)?;
        self.encode_frames(readable, req.after_lsn, req.shard.tenant)
    }

    fn encode_frames(
        &self,
        readable: Vec<ReadableBatch>,
        after_lsn: WalLsn,
        tenant: DataTenantId,
    ) -> Result<Vec<TailFrame>, ScribeError> {
        let mut candidates = Vec::new();
        for readable_batch in readable {
            let lsn = readable_batch.meta.wal_lsn_max;
            if lsn <= after_lsn {
                continue;
            }
            let arrow_ipc = encode_arrow_batch(&readable_batch.batch, tenant)?;
            candidates.push(ArrowIpcBatch {
                lsn,
                batch_id: readable_batch.meta.batch_id,
                arrow_ipc,
            });
        }
        candidates.sort_by_key(|batch| batch.lsn);
        let candidate_count = candidates.len();

        let mut frames = Vec::new();
        let mut bytes = 0usize;
        let mut last_emitted = None;
        for (index, batch) in candidates.into_iter().enumerate() {
            let batch_bytes = batch.arrow_ipc.len();
            if last_emitted.is_some() && bytes.saturating_add(batch_bytes) > self.config.max_bytes {
                frames.push(TailFrame::Exhausted {
                    resume_after_lsn: last_emitted.ok_or_else(|| ScribeError::Internal {
                        detail: "tail budget exhausted without a resume LSN".to_owned(),
                    })?,
                });
                return Ok(frames);
            }

            bytes = bytes.saturating_add(batch_bytes);
            last_emitted = Some(batch.lsn);
            frames.push(TailFrame::Batch(batch));

            if index == candidate_count.saturating_sub(1) {
                frames.push(TailFrame::Complete);
                return Ok(frames);
            }
        }

        frames.push(TailFrame::Complete);
        Ok(frames)
    }
}

fn encode_arrow_batch(
    batch: &arrow::record_batch::RecordBatch,
    tenant: DataTenantId,
) -> Result<Vec<u8>, ScribeError> {
    let tenant_column =
        batch
            .column_by_name("data_tenant_id")
            .ok_or_else(|| ScribeError::Internal {
                detail: "live-tail batch is missing data_tenant_id".to_owned(),
            })?;
    let tenant_values = tenant_column
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: "live-tail data_tenant_id must be Utf8".to_owned(),
        })?;
    let expected = tenant.to_string();
    for row in 0..tenant_values.len() {
        if tenant_values.is_null(row) || tenant_values.value(row) != expected {
            return Err(ScribeError::Internal {
                detail: format!("live-tail batch contains a row outside tenant {tenant}"),
            });
        }
    }

    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &batch.schema()).map_err(|error| {
        ScribeError::Internal {
            detail: format!("live-tail Arrow IPC writer init failed: {error}"),
        }
    })?;
    writer.write(batch).map_err(|error| ScribeError::Internal {
        detail: format!("live-tail Arrow IPC write failed: {error}"),
    })?;
    writer.finish().map_err(|error| ScribeError::Internal {
        detail: format!("live-tail Arrow IPC finish failed: {error}"),
    })?;
    Ok(bytes)
}
