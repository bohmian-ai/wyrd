//! Phase-2 `FetchLiveTail` server.
//!
//! `FetchLiveTailService` streams WAL data records to a caller (Oracle) that
//! is merging seals with an active writer's tail. The request targets a specific
//! writer stream `(node_id, writer_epoch)` plus an `after_lsn` position:
//!
//! - A request whose stream does not match this Scribe's current stream returns
//!   `WYRD_VALA_409_STREAM_MISMATCH` without touching memtable or WAL — this
//!   guards against Oracle contacting a replaced pod at the same
//!   `advertise_addr` (CONTRACTS §10).
//! - Otherwise, every WAL data record with `LSN > after_lsn` that is not
//!   already covered by a stream-scoped sealed range in `SealedRangeIndex` is
//!   emitted, so the caller never sees a row twice even mid-seal.
//!
//! This module compiles unconditionally. The tonic streaming wire-up and
//! in-crate consumer land in Phase B.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use vala_sql::OperatorPool;
use wyrd_spec::vala::error::BifrostError;

use crate::contracts::ScribeError;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{WalLsn, WalReader};

/// One sealed WAL LSN range attributed to a producing stream.
#[derive(Debug, Clone, Copy)]
pub struct SealedRange {
    /// The `(node_id, writer_epoch)` stream that produced the sealed file.
    pub stream: StreamIdentity,
    /// Inclusive lower bound of the sealed WAL LSN range.
    pub wal_lsn_min: WalLsn,
    /// Inclusive upper bound of the sealed WAL LSN range.
    pub wal_lsn_max: WalLsn,
}

/// In-memory index of sealed `[wal_lsn_min, wal_lsn_max]` ranges keyed by
/// producing stream.
///
/// LSNs are meaningful only within one `(node_id, writer_epoch)` stream, so the
/// index buckets ranges per stream and only ever compares a candidate LSN
/// against ranges from the same stream — cross-stream exclusion would be
/// meaningless (C4 collision).
#[derive(Debug, Default, Clone)]
pub struct SealedRangeIndex {
    ranges: HashMap<StreamIdentity, Vec<(WalLsn, WalLsn)>>,
}

impl SealedRangeIndex {
    /// Construct an empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build an index from an iterator of `SealedRange` records.
    pub fn from_ranges(ranges: impl IntoIterator<Item = SealedRange>) -> Self {
        let mut index = Self::new();
        for range in ranges {
            index.add(range);
        }
        index
    }

    /// Add one range to the index.
    pub fn add(&mut self, range: SealedRange) {
        self.ranges
            .entry(range.stream)
            .or_default()
            .push((range.wal_lsn_min, range.wal_lsn_max));
    }

    /// Whether `lsn` on `stream` falls in a sealed range for that stream.
    ///
    /// Ranges attributed to other streams are ignored — cross-stream LSN
    /// comparison is meaningless.
    #[must_use]
    pub fn contains(&self, stream: StreamIdentity, lsn: WalLsn) -> bool {
        self.ranges
            .get(&stream)
            .is_some_and(|ranges| ranges.iter().any(|(min, max)| lsn >= *min && lsn <= *max))
    }

    /// Load every sealed WAL LSN range for `stream` from `vala.file_list`.
    ///
    /// The tail RPC is pod-scoped: ranges for a `(node_id, writer_epoch)` stream
    /// span every tenant that pod has hosted. This uses the operator pool
    /// (BYPASSRLS) because the stream axis crosses tenants.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] if the query fails or an LSN column
    /// is negative (invariant violation — `wal_lsn_*` is unsigned semantically).
    pub async fn hydrate_from_file_list(
        pool: &OperatorPool,
        stream: StreamIdentity,
    ) -> Result<Self, ScribeError> {
        let node_uuid = stream.node_id.as_uuid();
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT wal_lsn_min, wal_lsn_max
             FROM vala.file_list
             WHERE node_id = $1 AND writer_epoch = $2",
        )
        .bind(node_uuid)
        .bind(stream.writer_epoch.as_i64())
        .fetch_all(pool.pool())
        .await
        .map_err(|e| ScribeError::Internal {
            detail: format!("SealedRangeIndex file_list query: {e}"),
        })?;

        let mut index = Self::new();
        for (min, max) in rows {
            let min_u = u64::try_from(min).map_err(|_| ScribeError::Internal {
                detail: format!("wal_lsn_min < 0 in file_list: {min}"),
            })?;
            let max_u = u64::try_from(max).map_err(|_| ScribeError::Internal {
                detail: format!("wal_lsn_max < 0 in file_list: {max}"),
            })?;
            index.add(SealedRange {
                stream,
                wal_lsn_min: WalLsn::new(min_u),
                wal_lsn_max: WalLsn::new(max_u),
            });
        }
        Ok(index)
    }
}

/// Live-tail RPC request.
///
/// The caller identifies the target writer stream and the last LSN it has
/// already observed; Scribe returns every subsequent data record on that
/// stream (subject to `SealedRangeIndex` exclusion).
#[derive(Debug, Clone, Copy)]
pub struct FetchLiveTailRequest {
    /// The writer stream the caller believes it is talking to.
    pub target_stream: StreamIdentity,
    /// Emit only records with `LSN > after_lsn`. `None` means "from stream start".
    pub after_lsn: Option<WalLsn>,
}

/// One live-tail response chunk — Arrow IPC bytes for a single WAL data record.
#[derive(Debug, Clone)]
pub struct ArrowIpcBatch {
    /// LSN of the record on the target stream.
    pub lsn: WalLsn,
    /// Idempotency batch ID copied from the WAL record header.
    pub batch_id: [u8; 16],
    /// Arrow IPC-encoded `RecordBatch` bytes (the WAL data payload).
    pub arrow_ipc: Vec<u8>,
}

/// Phase-2 `FetchLiveTail` server for one Scribe pod.
///
/// Owns pod-local read handles: the current `(node_id, writer_epoch)`, the WAL
/// directory to scan on each request, and a snapshot of sealed ranges. Reads
/// the WAL from disk on every call — Phase 2 does not maintain a persistent
/// cursor. Never writes memtable or WAL state.
#[derive(Debug)]
pub struct FetchLiveTailService {
    stream: StreamIdentity,
    wal_dir: PathBuf,
    sealed_index: Arc<SealedRangeIndex>,
}

impl FetchLiveTailService {
    /// Construct a service bound to `stream`.
    #[must_use]
    pub fn new(
        stream: StreamIdentity,
        wal_dir: PathBuf,
        sealed_index: Arc<SealedRangeIndex>,
    ) -> Self {
        Self {
            stream,
            wal_dir,
            sealed_index,
        }
    }

    /// Stream identity this service serves.
    #[must_use]
    pub fn stream(&self) -> StreamIdentity {
        self.stream
    }

    /// Handle one `FetchLiveTail` request.
    ///
    /// Returns `Err(BifrostError::StreamMismatch)` (409) if the request targets
    /// a `(node_id, writer_epoch)` that this pod does not own — the rejection
    /// path does not open the WAL or touch the memtable.
    ///
    /// On stream match, returns every WAL data record with `LSN > after_lsn`
    /// that is not covered by a sealed range for this stream in `sealed_index`.
    ///
    /// # Errors
    /// - [`BifrostError::StreamMismatch`] if `target_stream` differs from this pod's stream.
    /// - [`BifrostError::Internal`] wrapping a WAL open/read failure.
    #[allow(
        clippy::unused_async,
        reason = "async is intentional — Phase B wires this as a tonic streaming handler; the WAL read moves to spawn_blocking there."
    )]
    pub async fn fetch_live_tail(
        &self,
        req: FetchLiveTailRequest,
    ) -> Result<Vec<ArrowIpcBatch>, BifrostError> {
        self.read_tail_blocking(req)
    }

    fn read_tail_blocking(
        &self,
        req: FetchLiveTailRequest,
    ) -> Result<Vec<ArrowIpcBatch>, BifrostError> {
        if req.target_stream != self.stream {
            return Err(BifrostError::StreamMismatch {
                requested: req.target_stream.to_string(),
                actual: self.stream.to_string(),
            });
        }

        let reader = WalReader::open_directory(&self.wal_dir, self.stream).map_err(|e| {
            BifrostError::Internal {
                detail: format!("live-tail WAL open failed: {e}"),
            }
        })?;
        let records = reader
            .read_all_records()
            .map_err(|e| BifrostError::Internal {
                detail: format!("live-tail WAL read failed: {e}"),
            })?;

        let mut chunks = Vec::new();
        for record in records {
            if record.envelope_kind != 0 {
                continue;
            }
            if let Some(after) = req.after_lsn
                && record.lsn <= after
            {
                continue;
            }
            if self.sealed_index.contains(self.stream, record.lsn) {
                continue;
            }
            chunks.push(ArrowIpcBatch {
                lsn: record.lsn,
                batch_id: record.batch_id,
                arrow_ipc: record.payload,
            });
        }
        Ok(chunks)
    }
}
