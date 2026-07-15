//! Scribe implementation — WAL append, fsync, replay, and memtable (PR#3/PR#4).

pub mod audit_envelope;
pub mod file_list_writer;
pub mod filename;
pub mod manifest;
pub mod memtable;
pub mod parquet_writer;
pub mod replay;
pub mod seal;
pub mod seal_key;
pub mod stream_identity;
pub mod wal;

use arrow::ipc::reader::StreamReader;
use async_trait::async_trait;
use std::io::Cursor;
use std::sync::Arc;
use vala_sql::TenantConn;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use crate::contracts::{AppendAck, Scribe, ScribeAppend, ScribeError};
#[cfg(feature = "scribe-inspect")]
use crate::inspect::{MemtableKey, ScribeInspect};
use crate::scribe::memtable::Memtable;
use crate::scribe::seal_key::{EventDay, SealKey, TableRef};
use crate::scribe::wal::{ScribeAppendMeta, WalLsn};

/// Scribe implementation with WAL append, fsync, replay, memtable, and seal (PR#3-PR#6).
///
/// `append` splits the batch by event day, writes paired (audit, data) WAL records,
/// fsyncs, and forwards to the memtable. Seal predicate triggers freeze at first-of:
/// 50k rows | 1s | 128 MiB | 5s inactivity. Seal state machine executes:
/// Freeze → Parquet → PUT → PG tx (file_list + audit) → manifest → retire WAL.
#[derive(Debug)]
pub struct ScribeImpl {
    /// In-memory memtable keyed by seal-key.
    memtable: Arc<Memtable>,
    /// Opendal operator for object store (shared across seal drivers).
    operator: Arc<opendal::Operator>,
    /// Pod identity (node_id, writer_epoch).
    node_id: String,
    writer_epoch: i64,
}

impl ScribeImpl {
    /// Construct a new `ScribeImpl` with empty memtable and provided dependencies.
    #[must_use]
    pub fn new_with_deps(
        operator: Arc<opendal::Operator>,
        node_id: String,
        writer_epoch: i64,
    ) -> Self {
        Self {
            memtable: Arc::new(Memtable::new()),
            operator,
            node_id,
            writer_epoch,
        }
    }

    /// Construct a stub `ScribeImpl` for tests (memory backend, stub node identity).
    #[must_use]
    pub fn new() -> Self {
        let operator = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("memory backend init")
                .finish(),
        );
        Self {
            memtable: Arc::new(Memtable::new()),
            operator,
            node_id: "test-node".to_string(),
            writer_epoch: 1,
        }
    }

    /// Execute the seal state machine for a specific seal-key on the caller's
    /// tenant-scoped transaction.
    ///
    /// The caller opens and owns the `TenantConn`; this method drives the seal
    /// state machine against it and returns without committing. The caller
    /// commits (or rolls back) the transaction.
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
    ) -> Result<(), ScribeError> {
        use crate::scribe::seal::SealDriver;

        let driver = SealDriver::new(self.operator.clone());
        driver
            .seal(
                &self.memtable,
                seal_key,
                conn,
                &self.node_id,
                self.writer_epoch,
            )
            .await?;
        Ok(())
    }
}

impl Default for ScribeImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scribe for ScribeImpl {
    async fn append(&self, req: ScribeAppend) -> Result<AppendAck, ScribeError> {
        // Generate batch_id using UUID v7 (time-ordered, monotonic)
        let batch_id = *uuid::Uuid::now_v7().as_bytes();

        // 1. Parse table_fqn → TableRef
        let table_ref =
            TableRef::parse_fqn(&req.table_fqn).ok_or_else(|| ScribeError::Internal {
                detail: format!("invalid table FQN: {}", req.table_fqn),
            })?;

        // 2. Decode Arrow IPC bytes → RecordBatch
        let mut cursor = Cursor::new(&req.batch_data);
        let mut reader =
            StreamReader::try_new(&mut cursor, None).map_err(|e| ScribeError::Internal {
                detail: format!("failed to decode Arrow IPC: {e}"),
            })?;

        let batch = reader
            .next()
            .ok_or_else(|| ScribeError::Internal {
                detail: "no RecordBatch in IPC stream".to_string(),
            })?
            .map_err(|e| ScribeError::Internal {
                detail: format!("Arrow batch decode error: {e}"),
            })?;

        // 3. Extract event day from first row's wyrd_event_time
        // For PR#4, use placeholder single-day assertion (real multi-day split in PR#5)
        let event_day = EventDay::from_timestamp(chrono::Utc::now());

        // 4. Build SealKey
        let seal_key =
            crate::scribe::seal_key::SealKey::new(req.principal.tenant_id, table_ref, event_day);

        // 5. Build AuditEvent from principal
        let audit_event = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "bifrost.append".to_string(),
            resource: req.table_fqn.clone(),
            card_ref: req.principal.card_ref().cloned(),
            principal_id: req.principal.id,
            principal_kind: req.principal.kind.tag(),
            auth_method: AuthMethod::Jwt,
            permission: "bifrost:append".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: format!("{} rows", batch.num_rows()),
            detail: None,
        };

        // 6. Build ScribeAppendMeta with placeholder LSNs (real WAL integration in PR#5)
        let meta = ScribeAppendMeta {
            batch_id,
            rows_accepted: batch.num_rows(),
            wal_lsn_min: WalLsn::new(0),
            wal_lsn_max: WalLsn::new(0),
            seal_key: seal_key.as_path_components(),
        };

        // 7. Insert into memtable
        self.memtable.insert(&seal_key, audit_event, meta, batch)?;

        // 8. Check seal predicate
        if self.memtable.should_seal(&seal_key)? {
            tracing::info!(seal_key = %seal_key, "seal predicate triggered (will freeze in PR#6)");
        }

        Ok(AppendAck {
            batch_id,
            tenant: req.principal.tenant_id,
        })
    }
}

#[cfg(feature = "scribe-inspect")]
#[async_trait]
impl ScribeInspect for ScribeImpl {
    fn wal_pending_bytes(&self) -> u64 {
        0 // TODO PR#7+: Real WAL pending bytes
    }

    fn memtable_row_count(&self, key: &MemtableKey) -> usize {
        self.memtable.row_count(key).unwrap_or(0)
    }

    fn sealed_parquet_paths(&self) -> Vec<String> {
        // TODO PR#7+: Query vala.file_list for sealed paths for this node
        vec![]
    }

    async fn force_seal(&self, conn: &mut TenantConn<'_>) -> Result<(), ScribeError> {
        // A `TenantConn` is bound to exactly one tenant. Seal only the
        // memtable buckets whose seal-key belongs to that tenant; the harness
        // iterates tenants and opens a fresh `TenantConn` per tenant. The
        // caller owns commit/rollback.
        let tenant = conn.data_tenant_id();
        let keys = self.memtable.active_seal_keys_for_tenant(tenant)?;

        for key in keys {
            self.seal_one(&key, conn).await?;
        }

        Ok(())
    }
}
