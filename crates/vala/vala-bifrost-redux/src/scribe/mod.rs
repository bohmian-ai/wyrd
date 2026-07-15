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

use crate::contracts::{Scribe, ScribeAppend, ScribeError};
#[cfg(feature = "scribe-inspect")]
use crate::inspect::{MemtableKey, ScribeInspect};
use crate::scribe::memtable::Memtable;
use crate::scribe::seal_key::{EventDay, SealKey};
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
    /// Returns a `SealCommit` handle that the caller must pass to `seal_one_post_commit`
    /// after committing the transaction. The caller owns commit/rollback.
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

        // Cross-tenant guard: seal_key.tenant must match conn.data_tenant_id()
        let conn_tenant = conn.data_tenant_id();
        if seal_key.tenant != conn_tenant {
            return Err(ScribeError::Internal {
                detail: format!(
                    "tenant mismatch: seal_key.tenant={} vs conn.data_tenant_id={}",
                    seal_key.tenant, conn_tenant
                ),
            });
        }

        let driver = SealDriver::new(self.operator.clone());
        driver
            .pre_commit(
                &self.memtable,
                seal_key,
                conn,
                &self.node_id,
                self.writer_epoch,
            )
            .await
    }

    /// Complete seal post-commit stages (manifest + WAL retirement) after the
    /// caller commits the seal transaction.
    ///
    /// # Errors
    /// Returns [`ScribeError`] if any post-commit stage fails.
    pub async fn seal_one_post_commit(&self, handle: seal::SealCommit) -> Result<(), ScribeError> {
        use crate::scribe::seal::SealDriver;
        let driver = SealDriver::new(self.operator.clone());
        driver.post_commit(handle).await
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

#[cfg(feature = "scribe-inspect")]
#[async_trait]
impl ScribeInspect for ScribeImpl {
    fn wal_pending_bytes(&self) -> u64 {
        0 // TODO : Real WAL pending bytes
    }

    fn memtable_row_count(&self, key: &MemtableKey) -> usize {
        self.memtable.row_count(key).unwrap_or(0)
    }

    fn sealed_parquet_paths(&self) -> Vec<String> {
        // TODO : Query vala.file_list for sealed paths for this node
        vec![]
    }

    async fn force_seal(&self, conn: &mut TenantConn<'_>) -> Result<(), ScribeError> {
        // A `TenantConn` is bound to exactly one tenant. Seal only the
        // memtable buckets whose seal-key belongs to that tenant; the harness
        // iterates tenants and opens a fresh `TenantConn` per tenant.
        //
        // Pre-commit stages only — caller owns commit and post_commit.
        // This simplified implementation runs post_commit immediately after,
        // but real production usage would separate them.
        let tenant = conn.data_tenant_id();
        let keys = self.memtable.active_seal_keys_for_tenant(tenant)?;

        let mut handles = Vec::with_capacity(keys.len());
        for key in keys {
            let handle = self.seal_one(&key, conn).await?;
            handles.push(handle);
        }

        // Post-commit stages (simplified: run immediately without waiting for commit)
        for handle in handles {
            self.seal_one_post_commit(handle).await?;
        }

        Ok(())
    }
}
