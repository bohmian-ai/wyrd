//! `file_list` writer — atomic INSERT + audit fan-out per CONTRACTS §11.

use uuid::Uuid;
use vala_sql::OperatorPool;
use vala_sql::{SqlError, TenantConn};

use crate::scribe::stream_identity::StreamIdentity;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::ParquetEncoded;

/// The unique replay key on `vala.file_list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileListConflictKey {
    /// Producing Scribe node.
    pub node_id: Uuid,
    /// Producing Scribe writer epoch.
    pub writer_epoch: i64,
    /// Inclusive lower WAL LSN.
    pub wal_lsn_min: i64,
    /// Inclusive upper WAL LSN.
    pub wal_lsn_max: i64,
}

/// The full identity that must match when a conflict key is replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListCommitKey {
    /// Organization stamped on the physical Parquet file.
    pub data_tenant_id: DataTenantId,
    /// Logical namespace stored in `vala.file_list`.
    pub namespace: String,
    /// Local logical table name.
    pub table_name: String,
    /// Producing Scribe node.
    pub node_id: Uuid,
    /// Producing Scribe writer epoch.
    pub writer_epoch: i64,
    /// Inclusive lower WAL LSN.
    pub wal_lsn_min: i64,
    /// Inclusive upper WAL LSN.
    pub wal_lsn_max: i64,
}

/// Result of a file-list write or an already-validated replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListInsertOutcome {
    /// Durable `vala.file_list` row identity.
    pub id: Uuid,
    /// Full identity validated against the durable row.
    pub commit_key: FileListCommitKey,
    /// Whether the unique replay key already existed.
    pub replayed: bool,
}

/// File-list INSERT row matching the `vala.file_list` columns.
pub struct FileListInsert<'a> {
    pub id: Uuid,
    pub data_tenant_id: DataTenantId,
    pub namespace: &'a str,
    pub table_name: &'a str,
    pub file_path: &'a str,
    pub file_size: i64,
    pub row_count: i64,
    pub min_event_time: chrono::DateTime<chrono::Utc>,
    pub max_event_time: chrono::DateTime<chrono::Utc>,
    pub partition_day: chrono::NaiveDate,
    pub node_id: Uuid,
    pub writer_epoch: i64,
    pub wal_lsn_min: i64,
    pub wal_lsn_max: i64,
}

impl FileListInsert<'_> {
    /// Return the full commit identity for this row.
    #[must_use]
    pub fn commit_key(&self) -> FileListCommitKey {
        FileListCommitKey {
            data_tenant_id: self.data_tenant_id,
            namespace: self.namespace.to_owned(),
            table_name: self.table_name.to_owned(),
            node_id: self.node_id,
            writer_epoch: self.writer_epoch,
            wal_lsn_min: self.wal_lsn_min,
            wal_lsn_max: self.wal_lsn_max,
        }
    }

    /// Return the unique replay key for this row.
    #[must_use]
    pub const fn conflict_key(&self) -> FileListConflictKey {
        FileListConflictKey {
            node_id: self.node_id,
            writer_epoch: self.writer_epoch,
            wal_lsn_min: self.wal_lsn_min,
            wal_lsn_max: self.wal_lsn_max,
        }
    }
}

/// Extract LSN range from append metadata.
///
/// LSN values are u64 but real-world Postgres LSNs fit in i64 (PG column is BIGINT).
fn extract_lsn_range(encoded: &ParquetEncoded) -> Result<(i64, i64), ScribeError> {
    let min = encoded
        .append_metas
        .iter()
        .map(|m| m.wal_lsn_min.as_u64())
        .min()
        .unwrap_or(0);
    let max = encoded
        .append_metas
        .iter()
        .map(|m| m.wal_lsn_max.as_u64())
        .max()
        .unwrap_or(0);

    let min_i = i64::try_from(min).map_err(|_| ScribeError::Internal {
        detail: format!("wal_lsn_min {min} exceeds i64::MAX (invariant violation)"),
    })?;
    let max_i = i64::try_from(max).map_err(|_| ScribeError::Internal {
        detail: format!("wal_lsn_max {max} exceeds i64::MAX (invariant violation)"),
    })?;

    Ok((min_i, max_i))
}

/// Extract min/max event time from row group stats.
fn extract_event_time_range(
    encoded: &ParquetEncoded,
) -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let epoch = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
    match encoded.row_group_stats.first() {
        Some(rg) => (
            rg.min_event_time
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros)
                .unwrap_or(epoch),
            rg.max_event_time
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros)
                .unwrap_or(epoch),
        ),
        None => (epoch, epoch),
    }
}

/// Build the `FileListInsert` row from freeze metadata and its canonical binding.
///
/// Persistence workers may move the encoded bytes into object storage before
/// constructing the SQL row; `file_size_override` preserves the original byte
/// count in that path. The direct seal path passes `None` and derives the size
/// from the retained payload.
///
/// # Errors
/// Returns [`ScribeError`] when the binding does not match the frozen seal key,
/// metadata cannot be represented in the SQL types, or the writer identity is
/// not a UUID.
pub fn build_insert<'a>(
    frozen: &'a FrozenMemtable,
    encoded: &'a ParquetEncoded,
    binding: &'a TenantTableBinding,
    node_id: &str,
    writer_epoch: i64,
    file_path: &'a str,
    file_size_override: Option<usize>,
) -> Result<FileListInsert<'a>, ScribeError> {
    if binding.tenant != frozen.seal_key.tenant || binding.table_ref != frozen.seal_key.table {
        return Err(ScribeError::Internal {
            detail: format!(
                "tenant-table binding does not match seal key: binding tenant/table=({},{}) seal tenant/table=({},{})",
                binding.tenant, binding.table_ref, frozen.seal_key.tenant, frozen.seal_key.table
            ),
        });
    }

    let (wal_lsn_min, wal_lsn_max) = extract_lsn_range(encoded)?;
    let (min_event_time, max_event_time) = extract_event_time_range(encoded);

    // Row count and file size are usize; the Postgres columns are BIGINT (i64).
    // A single Parquet file cannot approach i64::MAX rows or bytes on any real
    // machine, so a failed conversion is an invariant violation.
    let row_count = i64::try_from(frozen.row_count()).map_err(|_| ScribeError::Internal {
        detail: "row_count exceeds i64::MAX (invariant violation)".to_string(),
    })?;
    let file_size =
        i64::try_from(file_size_override.unwrap_or(encoded.bytes.len())).map_err(|_| {
            ScribeError::Internal {
                detail: "file_size exceeds i64::MAX (invariant violation)".to_string(),
            }
        })?;

    let node_uuid = Uuid::parse_str(node_id).map_err(|e| ScribeError::Internal {
        detail: format!("invalid node_id UUID: {e}"),
    })?;

    Ok(FileListInsert {
        id: Uuid::now_v7(),
        data_tenant_id: binding.tenant,
        namespace: &binding.logical_namespace,
        table_name: &binding.table_name,
        file_path,
        file_size,
        row_count,
        min_event_time,
        max_event_time,
        partition_day: encoded.partition_day.as_naive_date(),
        node_id: node_uuid,
        writer_epoch,
        wal_lsn_min,
        wal_lsn_max,
    })
}

/// Validate and return the row already stored for a replay conflict.
async fn validate_replay(
    conn: &mut TenantConn<'_>,
    row: &FileListInsert<'_>,
    commit_key: FileListCommitKey,
) -> Result<FileListInsertOutcome, SqlError> {
    let conflict = row.conflict_key();
    let existing: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        r"
 SELECT id, data_tenant_id, namespace, table_name
   FROM vala.file_list
  WHERE node_id = $1
    AND writer_epoch = $2
    AND wal_lsn_min = $3
    AND wal_lsn_max = $4
  FOR UPDATE
 ",
    )
    .bind(conflict.node_id)
    .bind(conflict.writer_epoch)
    .bind(conflict.wal_lsn_min)
    .bind(conflict.wal_lsn_max)
    .fetch_optional(&mut **conn.transaction())
    .await?;

    let Some((id, data_tenant_id, namespace, table_name)) = existing else {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list replay conflict has no RLS-visible row for stream {}:{}:{}-{}",
                conflict.node_id, conflict.writer_epoch, conflict.wal_lsn_min, conflict.wal_lsn_max
            ),
        });
    };

    if data_tenant_id != row.data_tenant_id.as_uuid()
        || namespace != row.namespace
        || table_name != row.table_name
    {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list replay conflict identity mismatch: expected ({},{},{}) found ({},{},{})",
                row.data_tenant_id,
                row.namespace,
                row.table_name,
                data_tenant_id,
                namespace,
                table_name
            ),
        });
    }

    tracing::debug!(
        node_id = %row.node_id,
        writer_epoch = row.writer_epoch,
        wal_lsn_min = row.wal_lsn_min,
        wal_lsn_max = row.wal_lsn_max,
        "replay-driven re-seal: validated existing file_list row; skipping audit fan-out",
    );
    Ok(FileListInsertOutcome {
        id,
        commit_key,
        replayed: true,
    })
}

/// Publishes one recovered or live generation while holding the replacement
/// Scribe actor's exact membership fence in the same operator transaction.
///
/// The actor fence authorizes the mutation only. The supplied row continues to
/// carry the source WAL stream and LSN range used for replay idempotency.
///
/// # Errors
/// Returns [`SqlError`] when the actor fence is stale, publication conflicts
/// with a different identity, audit append fails, or the transaction cannot
/// commit.
pub async fn insert_and_audit_fenced(
    operator_pool: &OperatorPool,
    actor: StreamIdentity,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
) -> Result<FileListInsertOutcome, SqlError> {
    insert_and_audit_fenced_inner(
        operator_pool,
        actor,
        row,
        events,
        false,
        #[cfg(any(test, feature = "test-support"))]
        None,
    )
    .await
}

/// Deterministic test barrier reached while the publication transaction owns its fence lock.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone)]
pub struct PublicationFenceBarrier {
    /// Signals that publication has acquired and validated the membership lock.
    acquired: std::sync::Arc<tokio::sync::Notify>,
    /// Releases publication to perform its atomic file-list and audit mutation.
    release: std::sync::Arc<tokio::sync::Notify>,
}

#[cfg(any(test, feature = "test-support"))]
impl PublicationFenceBarrier {
    /// Create one single-use publication fence barrier.
    #[must_use]
    pub fn new() -> Self {
        Self {
            acquired: std::sync::Arc::new(tokio::sync::Notify::new()),
            release: std::sync::Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Wait until production publication owns the membership row lock.
    pub async fn wait_until_acquired(&self) {
        self.acquired.notified().await;
    }

    /// Allow the paused production publication transaction to continue.
    pub fn release(&self) {
        self.release.notify_one();
    }

    /// Pause the production transaction after fence validation and before mutation.
    async fn pause(&self) {
        self.acquired.notify_one();
        self.release.notified().await;
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Default for PublicationFenceBarrier {
    /// Create the default single-use publication fence barrier.
    fn default() -> Self {
        Self::new()
    }
}

/// Publish through the production fenced transaction with a deterministic lock barrier.
///
/// # Errors
/// Returns the same fence, replay, audit, SQL, or commit errors as
/// [`insert_and_audit_fenced`].
#[cfg(any(test, feature = "test-support"))]
pub async fn insert_and_audit_fenced_with_barrier(
    operator_pool: &OperatorPool,
    actor: StreamIdentity,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
    barrier: &PublicationFenceBarrier,
) -> Result<FileListInsertOutcome, SqlError> {
    insert_and_audit_fenced_inner(operator_pool, actor, row, events, false, Some(barrier)).await
}

/// Inject a rollback immediately before the fenced publication commit.
///
/// # Errors
///
/// Always returns the injected commit error after validating and staging the
/// transaction, or an earlier fence, replay, audit, or SQL error.
#[cfg(any(test, feature = "test-support"))]
pub(crate) async fn insert_and_audit_fenced_with_commit_failure(
    operator_pool: &OperatorPool,
    actor: StreamIdentity,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
) -> Result<FileListInsertOutcome, SqlError> {
    insert_and_audit_fenced_inner(operator_pool, actor, row, events, true, None).await
}

/// Own the one atomic fenced publication transaction and optional test rollback.
///
/// # Errors
///
/// Returns fence, replay, audit, SQL, or injected pre-commit failures; every
/// error drops the transaction without publishing partial durable state.
async fn insert_and_audit_fenced_inner(
    operator_pool: &OperatorPool,
    actor: StreamIdentity,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
    fail_before_commit: bool,
    #[cfg(any(test, feature = "test-support"))] barrier: Option<&PublicationFenceBarrier>,
) -> Result<FileListInsertOutcome, SqlError> {
    let mut transaction = operator_pool.begin().await.map_err(SqlError::from)?;
    let actor_live: bool =
        sqlx::query_scalar("SELECT vala.assert_scribe_publication_fence($1, $2, $3)")
            .bind(wyrd_spec::ids::DataTenantId::SYSTEM_OWNER.as_uuid())
            .bind(actor.node_id.as_uuid())
            .bind(actor.writer_epoch.as_i64())
            .fetch_one(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
    if !actor_live {
        return Err(SqlError::InvariantViolation {
            detail: format!("Scribe publication fence lost for actor {actor}"),
        });
    }
    #[cfg(any(test, feature = "test-support"))]
    if let Some(barrier) = barrier {
        barrier.pause().await;
    }

    let result = insert_file(&mut transaction, row).await?;
    let commit_key = row.commit_key();
    let outcome = if result.rows_affected() == 0 {
        validate_replay_transaction(&mut transaction, row, commit_key).await?
    } else {
        let mut audit = vala_sql::queries::audit_outbox::OperatorAudit::new(
            row.data_tenant_id,
            &mut transaction,
        );
        for event in events {
            audit.append(event).await?;
        }
        FileListInsertOutcome {
            id: row.id,
            commit_key,
            replayed: false,
        }
    };
    if fail_before_commit {
        return Err(SqlError::InvariantViolation {
            detail: "test SQL commit failure".to_owned(),
        });
    }
    transaction.commit().await.map_err(SqlError::from)?;
    Ok(outcome)
}

/// Inserts the file-list row through an already-owned SQL transaction.
async fn insert_file(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    row: &FileListInsert<'_>,
) -> Result<sqlx::postgres::PgQueryResult, SqlError> {
    sqlx::query(
        r"INSERT INTO vala.file_list (
 id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,
 min_event_time,max_event_time,partition_day,node_id,writer_epoch,wal_lsn_min,wal_lsn_max)
 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
 ON CONFLICT (data_tenant_id,node_id,writer_epoch,wal_lsn_min,wal_lsn_max) DO NOTHING",
    )
    .bind(row.id)
    .bind(row.data_tenant_id.as_uuid())
    .bind(row.namespace)
    .bind(row.table_name)
    .bind(row.file_path)
    .bind(row.file_size)
    .bind(row.row_count)
    .bind(row.min_event_time)
    .bind(row.max_event_time)
    .bind(row.partition_day)
    .bind(row.node_id)
    .bind(row.writer_epoch)
    .bind(row.wal_lsn_min)
    .bind(row.wal_lsn_max)
    .execute(&mut **transaction)
    .await
    .map_err(SqlError::from)
}

/// Validates an idempotent replay through an operator-owned transaction.
async fn validate_replay_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    row: &FileListInsert<'_>,
    commit_key: FileListCommitKey,
) -> Result<FileListInsertOutcome, SqlError> {
    let existing: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT id,data_tenant_id,namespace,table_name FROM vala.file_list \
         WHERE data_tenant_id=$1 AND node_id=$2 AND writer_epoch=$3 \
         AND wal_lsn_min=$4 AND wal_lsn_max=$5 FOR UPDATE",
    )
    .bind(row.data_tenant_id.as_uuid())
    .bind(row.node_id)
    .bind(row.writer_epoch)
    .bind(row.wal_lsn_min)
    .bind(row.wal_lsn_max)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(SqlError::from)?;
    let Some((id, tenant, namespace, table_name)) = existing else {
        return Err(SqlError::InvariantViolation {
            detail: "file_list replay conflict has no operator-visible row".to_owned(),
        });
    };
    if tenant != row.data_tenant_id.as_uuid()
        || namespace != row.namespace
        || table_name != row.table_name
    {
        return Err(SqlError::InvariantViolation {
            detail: "file_list replay conflict identity mismatch".to_owned(),
        });
    }
    Ok(FileListInsertOutcome {
        id,
        commit_key,
        replayed: true,
    })
}

/// Insert one `vala.file_list` row + N `vala.audit_outbox` rows in one transaction.
///
/// The insert uses `FileListConflictKey` for replay detection. A conflict is
/// successful only after the existing row's complete `FileListCommitKey` is
/// visible under the same RLS-bound transaction and matches the attempted row.
///
/// # Errors
/// Returns [`vala_sql::SqlError`] on transaction failure or an unvalidated replay.
pub async fn insert_and_audit(
    conn: &mut TenantConn<'_>,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
) -> Result<FileListInsertOutcome, SqlError> {
    if row.data_tenant_id != conn.data_tenant_id() {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list tenant mismatch: row tenant `{}` does not match TenantConn tenant `{}`",
                row.data_tenant_id,
                conn.data_tenant_id()
            ),
        });
    }

    let result = sqlx::query(
        r"
 INSERT INTO vala.file_list (
 id,
 data_tenant_id,
 namespace,
 table_name,
 file_path,
 file_size,
 row_count,
 min_event_time,
 max_event_time,
 partition_day,
 node_id,
 writer_epoch,
 wal_lsn_min,
 wal_lsn_max
 )
 VALUES (
 $1,
 $2,
 $3,
 $4,
 $5,
 $6,
 $7,
 $8,
 $9,
 $10,
 $11,
 $12,
 $13,
 $14
 )
 ON CONFLICT (data_tenant_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) DO NOTHING
 ",
    )
    .bind(row.id)
    .bind(row.data_tenant_id.as_uuid())
    .bind(row.namespace)
    .bind(row.table_name)
    .bind(row.file_path)
    .bind(row.file_size)
    .bind(row.row_count)
    .bind(row.min_event_time)
    .bind(row.max_event_time)
    .bind(row.partition_day)
    .bind(row.node_id)
    .bind(row.writer_epoch)
    .bind(row.wal_lsn_min)
    .bind(row.wal_lsn_max)
    .execute(&mut **conn.transaction())
    .await?;

    let commit_key = row.commit_key();
    if result.rows_affected() == 0 {
        return validate_replay(conn, row, commit_key).await;
    }

    for event in events {
        vala_sql::queries::audit_outbox::append_audit(conn, event).await?;
    }

    Ok(FileListInsertOutcome {
        id: row.id,
        commit_key,
        replayed: false,
    })
}
