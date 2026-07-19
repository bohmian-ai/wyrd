//! Scribe implementation — WAL append, fsync, replay, and memtable (/).

pub mod audit_envelope;
pub mod file_list_writer;
pub mod filename;
pub mod manifest;
pub mod memtable;
pub mod parquet_writer;
pub mod registry;
pub mod replay;
pub mod seal;
pub mod seal_key;
pub mod stream_identity;
pub mod tail_rpc;
pub mod wal;

use arrow::array::Array;
use arrow::compute::take;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use vala_sql::TenantConn;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use crate::catalog::TenantTableBinding;
use crate::contracts::{Scribe, ScribeAppend, ScribeError};
use crate::scribe::memtable::Memtable;
use crate::scribe::seal_key::{EventDay, SealKey};
use crate::scribe::tail_rpc::{FetchLiveTailRequest, FetchLiveTailService, TailFrame};

/// Memtable key for per-bucket row-count inspection.
///
/// Type alias for [`SealKey`] — memtable buckets are keyed by
/// (`tenant`, `table`, `event_day`).
pub type MemtableKey = SealKey;
use crate::scribe::wal::ScribeAppendMeta;

/// Scribe implementation with WAL append, fsync, replay, memtable, and seal ().
///
/// `append` splits the batch by event day, writes paired (audit, data) WAL records,
/// fsyncs, and forwards to the memtable. Seal predicate triggers freeze at first-of:
/// 50k rows | 1s | 128 MiB | 5s inactivity. Seal state machine executes:
/// Freeze → Parquet → PUT → PG tx (`file_list` + audit) → manifest → retire WAL.
#[derive(Debug)]
pub struct ScribeImpl {
    /// In-memory memtable keyed by seal-key.
    memtable: Arc<Memtable>,
    /// Opendal operator for object store (shared across seal drivers).
    operator: Arc<opendal::Operator>,
    /// WAL writer for durable append fsync.
    wal: Arc<wal::WalWriter>,
    /// Pod identity (`node_id`, `writer_epoch`).
    node_id: String,
    writer_epoch: i64,
}

impl ScribeImpl {
    /// Construct a new `ScribeImpl` with empty memtable and provided dependencies.
    pub fn new_with_deps(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
    ) -> Self {
        Self {
            memtable: Arc::new(Memtable::new()),
            operator,
            wal,
            node_id,
            writer_epoch,
        }
    }

    /// Construct a stub `ScribeImpl` for tests (memory backend, stub node identity, temp WAL).
    ///
    /// # Panics
    /// Panics if temp WAL directory or operator init fails.
    #[must_use]
    #[cfg(test)]
    pub fn new() -> Self {
        let operator = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("memory backend init")
                .finish(),
        );

        let temp_dir = tempfile::tempdir().expect("temp WAL dir");
        let node_id_bytes = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000")
            .expect("valid UUID")
            .as_bytes()
            .to_owned();
        let wal = Arc::new(
            wal::WalWriter::new(
                temp_dir.path(),
                node_id_bytes,
                1,
                wyrd_spec::ids::DataTenantId::new_v7(),
                None,
            )
            .expect("test WAL init"),
        );

        // Leak temp_dir to keep WAL files for the test lifetime
        std::mem::forget(temp_dir);

        Self {
            memtable: Arc::new(Memtable::new()),
            operator,
            wal,
            node_id: "00000000-0000-0000-0000-000000000000".to_string(),
            writer_epoch: 1,
        }
    }

    /// Execute seal pre-commit stages (Freeze → Parquet → PUT → PG tx) for a
    /// specific seal-key on the caller's tenant-scoped transaction.
    ///
    /// Returns a `SealCommit` handle whose post-commit token must be completed
    /// only after the caller commits the transaction. The caller owns commit/rollback.
    ///
    /// Repo rule (`check:from-pools-allowlist`): this signature MUST take
    /// `&mut vala_sql::TenantConn<'_>` and MUST NOT accept `sqlx::PgPool`.
    ///
    /// # Errors
    /// Returns [`ScribeError`] if any seal stage fails.
    pub async fn seal_one(
        &self,
        seal_key: &SealKey,
        conn: &mut TenantConn<'_>,
    ) -> Result<seal::SealCommit, ScribeError> {
        use crate::scribe::seal::SealDriver;

        let binding = TenantTableBinding::resolve((seal_key.tenant, seal_key.table.clone()))
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        binding
            .validate_authenticated_tenant(conn.data_tenant_id())
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;

        let driver = SealDriver::new(self.operator.clone());
        driver
            .pre_commit(
                &self.memtable,
                seal_key,
                &binding,
                conn,
                &self.node_id,
                self.writer_epoch,
            )
            .await
    }
}

/// Split a `RecordBatch` by `wyrd_event_time` day, returning (`EventDay`, `RecordBatch`) pairs.
fn split_batch_by_event_day(
    batch: &RecordBatch,
) -> Result<Vec<(EventDay, RecordBatch)>, ScribeError> {
    let ts_col = batch
        .column_by_name("wyrd_event_time")
        .ok_or_else(|| ScribeError::Internal {
            detail: "missing wyrd_event_time column".into(),
        })?;

    let ts_array = ts_col
        .as_any()
        .downcast_ref::<arrow::array::TimestampMicrosecondArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: "wyrd_event_time must be TimestampMicrosecond".into(),
        })?;

    // Group row indices by UTC day
    let mut day_indices: HashMap<chrono::NaiveDate, Vec<u32>> = HashMap::new();
    for i in 0..ts_array.len() {
        if ts_array.is_null(i) {
            continue;
        }
        let micros = ts_array.value(i);
        let dt = chrono::DateTime::from_timestamp_micros(micros).ok_or_else(|| {
            ScribeError::Internal {
                detail: format!("invalid timestamp micros: {micros}"),
            }
        })?;
        let day = dt.date_naive();
        day_indices
            .entry(day)
            .or_default()
            .push(u32::try_from(i).expect("row index within u32 range"));
    }

    // Build one RecordBatch per day using arrow::compute::take
    let mut result = Vec::with_capacity(day_indices.len());
    for (day, indices) in day_indices {
        let indices_array = arrow::array::UInt32Array::from(indices);
        let columns: Result<Vec<_>, _> = batch
            .columns()
            .iter()
            .map(|col| {
                take(col.as_ref(), &indices_array, None).map_err(|e| ScribeError::Internal {
                    detail: format!("arrow take failed: {e}"),
                })
            })
            .collect();
        let day_batch =
            RecordBatch::try_new(batch.schema(), columns?).map_err(|e| ScribeError::Internal {
                detail: format!("RecordBatch::try_new failed: {e}"),
            })?;
        result.push((EventDay::new(day), day_batch));
    }

    Ok(result)
}

#[cfg(test)]
impl Default for ScribeImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scribe for ScribeImpl {
    async fn append(&self, req: ScribeAppend) -> Result<(), ScribeError> {
        // batch_id comes from the request (client-supplied v7 UUID); 2PC
        // recovery in catalog::recovery keys off this exact value.
        let batch_id = *req.batch_id.as_bytes();
        let table_fqn = req.table.fqn();

        // Split cross-day input into per-day slices; a cross-day batch
        // produces two seals into two file_list rows (see seal_key module).
        let event_days = split_batch_by_event_day(&req.rows)?;

        for (event_day, day_batch) in event_days {
            let seal_key = SealKey::new(req.principal.tenant_id, req.table.clone(), event_day);

            let audit_event = AuditEvent {
                request_id: req.request_id.clone(),
                trace_id: None,
                operation: "bifrost.append".to_string(),
                resource: table_fqn.clone(),
                card_ref: req.principal.card_ref().cloned(),
                principal_id: req.principal.id,
                principal_kind: req.principal.kind.tag(),
                auth_method: AuthMethod::Jwt,
                permission: "bifrost:append".to_string(),
                decision: AuditDecision::Allow,
                result: AuditResult::Success,
                payload_summary: format!("{} rows", day_batch.num_rows()),
                detail: None,
            };

            let audit_payload =
                serde_json::to_vec(&audit_event).map_err(|e| ScribeError::Internal {
                    detail: format!("failed to serialize AuditEvent: {e}"),
                })?;

            let mut data_payload = Vec::new();
            {
                let mut writer = StreamWriter::try_new(&mut data_payload, &day_batch.schema())
                    .map_err(|e| ScribeError::Internal {
                        detail: format!("Arrow IPC writer init: {e}"),
                    })?;
                writer
                    .write(&day_batch)
                    .map_err(|e| ScribeError::Internal {
                        detail: format!("Arrow IPC write: {e}"),
                    })?;
                writer.finish().map_err(|e| ScribeError::Internal {
                    detail: format!("Arrow IPC finish: {e}"),
                })?;
            }

            let wal_lsn = self
                .wal
                .append_and_fsync(batch_id, audit_payload, data_payload)?;

            let meta = ScribeAppendMeta {
                batch_id,
                rows_accepted: day_batch.num_rows(),
                wal_lsn_min: wal_lsn,
                wal_lsn_max: wal_lsn,
                seal_key: seal_key.as_path_components(),
            };

            self.memtable
                .insert(&seal_key, audit_event, meta, day_batch)?;

            if self.memtable.should_seal(&seal_key)? {
                tracing::info!(seal_key = %seal_key, "seal predicate triggered");
            }
        }

        Ok(())
    }
}

impl ScribeImpl {
    /// Sum of pending (un-fsynced or un-truncated) WAL bytes on this pod.
    #[must_use]
    pub fn wal_pending_bytes(&self) -> u64 {
        0 // TODO : Real WAL pending bytes
    }

    /// Row count in the writable bucket for `key` on this pod; 0 if no bucket.
    #[must_use]
    pub fn memtable_row_count(&self, key: &MemtableKey) -> usize {
        self.memtable.row_count(key).unwrap_or(0)
    }

    /// Every Parquet path this pod has sealed since boot.
    #[must_use]
    pub fn sealed_parquet_paths(&self) -> Vec<String> {
        // TODO : Query vala.file_list for sealed paths for this node
        vec![]
    }

    /// Force-seal every non-empty writable or pending bucket on this pod.
    ///
    /// The returned batch is only a set of post-commit capabilities. The
    /// caller must commit its `TenantConn` first, then pass the batch to
    /// [`Self::complete_post_commit`].
    ///
    /// # Errors
    /// Returns `ScribeError` if any seal stage fails.
    pub async fn force_seal(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<seal::PostCommitBatch, ScribeError> {
        // A `TenantConn` is bound to exactly one tenant. Seal only the
        // memtable buckets whose seal-key belongs to that tenant; the harness
        // iterates tenants and opens a fresh `TenantConn` per tenant.
        //
        let tenant = conn.data_tenant_id();
        let keys = self.memtable.seal_keys_for_tenant(tenant)?;

        let mut tokens = Vec::with_capacity(keys.len());
        for key in keys {
            match self.seal_one(&key, conn).await {
                Ok(handle) => tokens.push(handle.token),
                Err(error) => {
                    for token in tokens {
                        let _ = self.abort_post_commit(token);
                    }
                    return Err(error);
                }
            }
        }

        Ok(seal::PostCommitBatch(tokens))
    }

    /// Complete one or more seal generations after the caller commits SQL.
    pub fn complete_post_commit<T>(&self, post_commit: T) -> Result<(), ScribeError>
    where
        T: Into<seal::PostCommitBatch>,
    {
        for token in post_commit.into().0 {
            self.memtable
                .complete_post_commit(token.seal_id, token.file_list_key)?;
        }
        Ok(())
    }

    /// Abort one or more post-commit capabilities after SQL rollback.
    pub fn abort_post_commit<T>(&self, post_commit: T) -> Result<(), ScribeError>
    where
        T: Into<seal::PostCommitBatch>,
    {
        for token in post_commit.into().0 {
            self.memtable.abort_post_commit(token.seal_id)?;
        }
        Ok(())
    }

    /// Reconcile a pending generation against the exact durable file-list row.
    ///
    /// The lookup runs through the caller's tenant-bound connection. A found
    /// row completes the generation; a missing row deliberately leaves it
    /// pending so WAL replay can retry the seal.
    pub async fn reconcile_post_commit(
        &self,
        token: &seal::PostCommitToken,
        conn: &mut TenantConn<'_>,
    ) -> Result<bool, ScribeError> {
        let key = &token.file_list_key;
        if conn.data_tenant_id() != key.data_tenant_id {
            return Err(ScribeError::Internal {
                detail: format!(
                    "post-commit reconciliation tenant mismatch: key={} connection={}",
                    key.data_tenant_id,
                    conn.data_tenant_id()
                ),
            });
        }
        let row: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id
               FROM vala.file_list
              WHERE data_tenant_id = $1
                AND namespace = $2
                AND table_name = $3
                AND node_id = $4
                AND writer_epoch = $5
                AND wal_lsn_min = $6
                AND wal_lsn_max = $7",
        )
        .bind(key.data_tenant_id.as_uuid())
        .bind(&key.namespace)
        .bind(&key.table_name)
        .bind(key.node_id)
        .bind(key.writer_epoch)
        .bind(key.wal_lsn_min)
        .bind(key.wal_lsn_max)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|error| ScribeError::Internal {
            detail: format!("file-list post-commit reconciliation failed: {error}"),
        })?;

        if row.is_some() {
            self.memtable
                .complete_post_commit(token.seal_id, key.clone())?;
            Ok(true)
        } else {
            self.memtable.abort_post_commit(token.seal_id)?;
            Ok(false)
        }
    }

    /// Restore one replayed WAL range as a pending immutable generation.
    pub fn restore_replayed(
        &self,
        replayed: &replay::ReplayedSealKey,
    ) -> Result<memtable::FrozenMemtable, ScribeError> {
        self.memtable.restore_replayed(replayed)
    }

    /// Sweep committed immutable generations whose grace period elapsed.
    pub fn sweep_once_at(
        &self,
        now: std::time::Instant,
    ) -> Result<Vec<(SealKey, memtable::WalRange)>, ScribeError> {
        self.memtable.sweep_once_at(now)
    }

    /// Construct the pod-local typed tail reader.
    pub fn tail_service(&self) -> Result<FetchLiveTailService, ScribeError> {
        let node_id =
            uuid::Uuid::parse_str(&self.node_id).map_err(|error| ScribeError::Internal {
                detail: format!("invalid Scribe node_id: {error}"),
            })?;
        let stream = stream_identity::StreamIdentity::new(
            stream_identity::NodeId::new(node_id),
            stream_identity::WriterEpoch::new(self.writer_epoch),
        );
        Ok(FetchLiveTailService::new(
            stream,
            Arc::clone(&self.memtable),
        ))
    }

    /// Fetch the typed live tail from this pod's memtable.
    pub async fn fetch_live_tail(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<Vec<TailFrame>, ScribeError> {
        self.tail_service()?.fetch_live_tail(request).await
    }
}
