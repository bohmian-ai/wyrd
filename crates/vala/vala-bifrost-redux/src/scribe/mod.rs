//! Scribe implementation — WAL append, fsync, replay, and memtable (/).

pub mod admission;
pub mod audit_envelope;
pub mod blocking_executor;
pub mod file_list_writer;
pub mod filename;
pub mod manifest;
pub mod memtable;
pub mod parquet_writer;
pub mod preprocess;
pub mod registry;
pub mod replay;
pub mod seal;
pub mod seal_key;
pub mod stream_identity;
pub mod tail_rpc;
pub mod telemetry;
pub mod wal;
pub mod writer;

use async_trait::async_trait;
use std::sync::Arc;
use vala_sql::TenantConn;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use crate::catalog::TenantTableBinding;
use crate::contracts::{Scribe, ScribeAppend, ScribeError};
use crate::scribe::admission::{
    AdmissionConfig, AdmissionController, MAX_REQUEST_BYTES, REQUEST_OVERHEAD_BYTES,
};
use crate::scribe::blocking_executor::ScribeBlockingExecutor;
use crate::scribe::memtable::Memtable;
use crate::scribe::preprocess::AdmittedAppend;
use crate::scribe::seal_key::SealKey;
use crate::scribe::tail_rpc::{FetchLiveTailRequest, FetchLiveTailService, TailFrame};
use crate::scribe::telemetry::ScribeTelemetry;
use crate::scribe::writer::TenantTableWriterRegistry;

/// Memtable key for per-bucket row-count inspection.
///
/// Type alias for [`SealKey`] — memtable buckets are keyed by
/// (`tenant`, `table`, `event_day`).
pub type MemtableKey = SealKey;

/// Scribe implementation with bounded admission, FIFO writers, WAL, memtable, and seal.
///
/// `append` admits a complete request to a bounded writer queue. The writer
/// consumer performs event-day splitting, serialization, WAL writes, memtable
/// insertion, and `sync_data` in order. Seal predicate triggers freeze at first-of:
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
    /// Pod-global request and writer admission counters.
    admission: AdmissionController,
    /// Fixed-width executor for preprocessing and blocking WAL operations.
    executor: ScribeBlockingExecutor,
    /// Atomic get-or-create registry for tenant/table FIFO writers.
    registry: Arc<TenantTableWriterRegistry>,
    /// Optional stage recorder used by benchmark and journey harnesses.
    telemetry: Option<Arc<ScribeTelemetry>>,
}

impl ScribeImpl {
    /// Construct a new `ScribeImpl` with empty memtable and provided dependencies.
    pub fn new_with_deps(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
    ) -> Self {
        Self::new_with_config(
            operator,
            wal,
            node_id,
            writer_epoch,
            AdmissionConfig::default(),
            None,
        )
    }

    fn new_with_config(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        admission_config: AdmissionConfig,
        telemetry: Option<Arc<ScribeTelemetry>>,
    ) -> Self {
        let memtable = Arc::new(Memtable::new());
        let admission = AdmissionController::with_config(admission_config);
        let executor = ScribeBlockingExecutor::new();
        let registry = TenantTableWriterRegistry::new(
            admission.clone(),
            Arc::clone(&memtable),
            Arc::clone(&wal),
            executor.clone(),
            telemetry.clone(),
        );
        Self {
            memtable,
            operator,
            wal,
            node_id,
            writer_epoch,
            admission,
            executor,
            registry,
            telemetry,
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

        Self::new_with_deps(
            operator,
            wal,
            "00000000-0000-0000-0000-000000000000".to_string(),
            1,
        )
    }

    /// Attach an opt-in stage recorder to this Scribe instance.
    #[must_use]
    pub fn with_telemetry(mut self, telemetry: Arc<ScribeTelemetry>) -> Self {
        self.registry.set_telemetry(Arc::clone(&telemetry));
        self.telemetry = Some(telemetry);
        self
    }

    /// Stop accepting new writer work and shut down the bounded executor.
    pub async fn shutdown(&self) {
        self.registry.shutdown().await;
        self.executor.shutdown();
    }

    /// Retire idle writers after the pod lifecycle scanner's tick.
    pub async fn retire_idle(&self, now: std::time::Instant) {
        self.registry.retire_idle(now).await;
    }

    /// Return a snapshot of the attached stage samples.
    #[must_use]
    pub fn telemetry(&self) -> Option<Vec<telemetry::ScribeStageSample>> {
        self.telemetry.as_ref().map(|recorder| recorder.snapshot())
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

        let driver = SealDriver::new_with_telemetry(self.operator.clone(), self.telemetry.clone());
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

#[cfg(test)]
impl Default for ScribeImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scribe for ScribeImpl {
    async fn append(&self, req: ScribeAppend) -> Result<(), ScribeError> {
        if req.measured_wire_bytes > MAX_REQUEST_BYTES {
            return Err(ScribeError::PayloadTooLarge {
                bytes: req.measured_wire_bytes,
            });
        }

        let binding = TenantTableBinding::resolve((req.principal.tenant_id, req.table.clone()))
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        binding
            .validate_authenticated_tenant(req.principal.tenant_id)
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;

        let estimated_bytes = req
            .rows
            .get_array_memory_size()
            .saturating_add(req.measured_wire_bytes)
            .saturating_add(REQUEST_OVERHEAD_BYTES);
        let reservation = self
            .admission
            .try_reserve(req.table.fqn(), estimated_bytes)?;
        let table_fqn = req.table.fqn();
        let audit_event = AuditEvent {
            request_id: req.request_id.clone(),
            trace_id: None,
            operation: "bifrost.append".to_string(),
            resource: table_fqn,
            card_ref: req.principal.card_ref().cloned(),
            principal_id: req.principal.id,
            principal_kind: req.principal.kind.tag(),
            auth_method: AuthMethod::Jwt,
            permission: "bifrost:append".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: format!("{} rows", req.rows.num_rows()),
            detail: None,
        };
        let admitted = AdmittedAppend {
            request_id: req.request_id,
            batch_id: req.batch_id,
            audit_event,
            rows: req.rows,
            measured_wire_bytes: req.measured_wire_bytes,
            admitted_bytes: reservation.bytes(),
            reservation,
            tenant: req.principal.tenant_id,
            table: req.table,
        };

        let (writer, created) = self.registry.get_or_create(binding)?;
        if let Err(error) = TenantTableWriterRegistry::enqueue(&writer, admitted) {
            if created {
                self.registry.remove_if(&writer.key, writer.instance_id);
            }
            return Err(error);
        }
        Ok(())
    }
}

impl ScribeImpl {
    /// Sum of pending (un-fsynced or un-truncated) WAL bytes on this pod.
    #[must_use]
    pub fn wal_pending_bytes(&self) -> u64 {
        self.wal.bytes_on_disk()
    }

    /// Return the number of bytes currently retained by this pod's WAL.
    #[must_use]
    pub fn wal_bytes_on_disk(&self) -> u64 {
        self.wal.bytes_on_disk()
    }

    /// Return aggregate writable and immutable memtable state.
    pub fn memtable_stats(&self) -> Result<memtable::MemtableStats, ScribeError> {
        self.memtable.stats()
    }

    /// Return the current pod-global admission counters.
    #[must_use]
    pub fn admission_snapshot(&self) -> admission::AdmissionSnapshot {
        self.admission.snapshot()
    }

    /// Return the number of active tenant/table writer actors.
    #[must_use]
    pub fn writer_count(&self) -> usize {
        self.registry.writer_count()
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
        self.registry.drain().await;
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
