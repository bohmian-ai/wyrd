//! Typed, pod-local live-tail reads over writable and immutable memtable data.
//!
//! This module deliberately has no WAL, SQL, HTTP, tonic, or authentication
//! dependency. Oracle supplies an already-authorized tenant/table binding and
//! the Scribe validates only the stream identity and local data scope here.

use std::sync::Arc;

use arrow::array::{Array, StringArray};
use arrow::ipc::writer::StreamWriter;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;
use crate::scribe::memtable::Memtable;
use crate::scribe::routing::shard_for;
use crate::scribe::seal_key::EventDay;
use crate::scribe::shards::ScribeShardRuntime;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::WalLsn;

/// Exact, bounded hot-read request handed from Oracle to Scribe.
#[derive(Debug, Clone)]
pub struct FetchLiveTailRequest {
    /// Authenticated tenant/table binding resolved by Oracle.
    pub binding: TenantTableBinding,
    /// The writer stream the caller believes it is talking to.
    pub target_stream: StreamIdentity,
    /// Inclusive first partition day governed by the query.
    pub start_day: EventDay,
    /// Inclusive last partition day governed by the query.
    pub end_day: EventDay,
    /// Emit only records with `LSN > after_lsn`.
    pub after_lsn: WalLsn,
    /// Columns required by Oracle filters, ordering, tripwire, and projection.
    pub required_columns: Vec<String>,
}

impl FetchLiveTailRequest {
    /// Return the fixed shard selected by the tenant/table route.
    #[must_use]
    pub fn shard_id(&self) -> usize {
        shard_for(self.binding.tenant, &self.binding.table_ref)
    }
}

/// One shallow, structural hot snapshot returned by a shard owner.
#[derive(Debug, Clone)]
pub struct HotBatch {
    /// Exact partition day owning the batch.
    pub partition_day: EventDay,
    /// WAL LSN of the append.
    pub wal_lsn: WalLsn,
    /// Idempotency identity of the append.
    pub batch_id: [u8; 16],
    /// Arrow rows projected to the request's required columns.
    pub rows: arrow::record_batch::RecordBatch,
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
    shards: Option<Arc<ScribeShardRuntime>>,
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
            shards: None,
            config: TailConfig {
                max_bytes: config.max_bytes.max(1),
            },
        }
    }

    /// Construct a production reader that submits snapshots to the owning
    /// shard command queue instead of traversing Scribe state directly.
    #[must_use]
    pub(crate) fn with_runtime(
        stream: StreamIdentity,
        memtable: Arc<Memtable>,
        shards: Arc<ScribeShardRuntime>,
        config: TailConfig,
    ) -> Self {
        let mut service = Self::with_config(stream, memtable, config);
        service.shards = Some(shards);
        service
    }

    /// Stream identity this service serves.
    #[must_use]
    pub fn stream(&self) -> StreamIdentity {
        self.stream
    }

    /// Return the canonical pod-local shard for a live-tail scope.
    #[must_use]
    pub fn shard_id(&self, request: &FetchLiveTailRequest) -> usize {
        request.shard_id()
    }

    /// Return the current writable and immutable batches for one shard.
    ///
    /// Stream, scope, day range, and projection validation happen before an
    /// Arrow response is encoded. Production calls pass through the bounded
    /// shard command queue; the direct memtable branch is only for the narrow
    /// in-process adapter used by unit tests.
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

        if req.start_day > req.end_day {
            return Err(ScribeError::Internal {
                detail: "live-tail start day is after end day".to_owned(),
            });
        }

        let hot_batches = if let Some(shards) = &self.shards {
            shards.snapshot(req.clone()).await?
        } else {
            self.memtable
                .readable_batches_for_range(
                    req.binding.tenant,
                    &req.binding.table_ref,
                    req.start_day,
                    req.end_day,
                    &req.required_columns,
                )?
                .into_iter()
                .map(|readable| HotBatch {
                    partition_day: readable.partition_day,
                    wal_lsn: readable.meta.wal_lsn_max,
                    batch_id: readable.meta.batch_id,
                    rows: readable.batch,
                })
                .collect()
        };
        self.encode_frames(hot_batches, req.after_lsn, req.binding.tenant)
    }

    /// Return direct local Arrow handles for Oracle's `MemoryExec` path.
    pub async fn fetch_hot_batches(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<Vec<HotBatch>, ScribeError> {
        if request.target_stream != self.stream {
            return Err(ScribeError::StreamMismatch {
                requested: request.target_stream,
                actual: self.stream,
            });
        }
        if request.start_day > request.end_day {
            return Err(ScribeError::Internal {
                detail: "live-tail start day is after end day".to_owned(),
            });
        }
        if let Some(shards) = &self.shards {
            return shards.snapshot(request).await;
        }
        Ok(self
            .memtable
            .readable_batches_for_range(
                request.binding.tenant,
                &request.binding.table_ref,
                request.start_day,
                request.end_day,
                &request.required_columns,
            )?
            .into_iter()
            .map(|readable| HotBatch {
                partition_day: readable.partition_day,
                wal_lsn: readable.meta.wal_lsn_max,
                batch_id: readable.meta.batch_id,
                rows: readable.batch,
            })
            .collect())
    }

    fn encode_frames(
        &self,
        readable: Vec<HotBatch>,
        after_lsn: WalLsn,
        tenant: DataTenantId,
    ) -> Result<Vec<TailFrame>, ScribeError> {
        let mut candidates = Vec::new();
        for readable_batch in readable {
            let lsn = readable_batch.wal_lsn;
            if lsn <= after_lsn {
                continue;
            }
            let arrow_ipc = encode_arrow_batch(&readable_batch.rows, tenant)?;
            candidates.push(ArrowIpcBatch {
                lsn,
                batch_id: readable_batch.batch_id,
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
