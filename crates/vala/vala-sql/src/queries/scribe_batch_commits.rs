//! Tenant-scoped durable Scribe batch-commit control state.
//!
//! The owner records the fsynced v4 WAL identity and appends the canonical
//! ingest audit event in the same caller-owned [`TenantConn`] transaction.

use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::{SqlError, TenantConn};

/// Immutable WAL identity that must match exactly for a replayed batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScribeBatchCommit {
    /// Authenticated tenant that owns the logical table and batch.
    pub tenant: DataTenantId,
    /// Canonical logical table FQN, not a physical table identifier.
    pub logical_table_fqn: String,
    /// Stable client batch identity.
    pub batch_id: uuid::Uuid,
    /// SHA-256 digest over ordered slice identities and payload digests.
    pub slice_set_digest: [u8; 32],
    /// Number of validated slices in the closed set.
    pub slice_count: i32,
    /// WAL writer node identity.
    pub wal_node_id: uuid::Uuid,
    /// WAL writer epoch.
    pub wal_writer_epoch: i64,
    /// Fixed WAL shard owner.
    pub wal_shard_id: i16,
    /// WAL segment sequence containing the terminal commit.
    pub wal_segment_sequence: i64,
    /// Inclusive first LSN in the batch slice set.
    pub wal_lsn_min: i64,
    /// Inclusive terminal commit LSN.
    pub wal_lsn_max: i64,
    /// Request correlation persisted with the durable fence.
    pub request_id: uuid::Uuid,
}

/// Tenant-visible durable identity loaded after an idempotency conflict.
#[derive(Debug, sqlx::FromRow)]
struct ExistingScribeBatchCommit {
    /// Persisted ordered slice-set digest.
    slice_set_digest: Vec<u8>,
    /// Persisted closed-set cardinality.
    slice_count: i32,
    /// Persisted WAL node identity.
    wal_node_id: uuid::Uuid,
    /// Persisted WAL writer epoch.
    wal_writer_epoch: i64,
    /// Persisted fixed shard owner.
    wal_shard_id: i16,
    /// Persisted segment sequence containing the terminal record.
    wal_segment_sequence: i64,
    /// Persisted first slice LSN.
    wal_lsn_min: i64,
    /// Persisted terminal commit LSN.
    wal_lsn_max: i64,
    /// Persisted correlation identifier.
    request_id: uuid::Uuid,
}

impl ExistingScribeBatchCommit {
    /// Returns whether this exact tenant-scoped control row authorizes `commit`.
    #[must_use]
    fn matches(&self, commit: &ScribeBatchCommit) -> bool {
        self.slice_set_digest == commit.slice_set_digest
            && self.slice_count == commit.slice_count
            && self.wal_node_id == commit.wal_node_id
            && self.wal_writer_epoch == commit.wal_writer_epoch
            && self.wal_shard_id == commit.wal_shard_id
            && self.wal_segment_sequence == commit.wal_segment_sequence
            && self.wal_lsn_min == commit.wal_lsn_min
            && self.wal_lsn_max == commit.wal_lsn_max
            && self.request_id == commit.request_id
    }
}

/// Records a Scribe commit and its audit event, or verifies an exact replay.
///
/// The caller commits the supplied tenant transaction. A first observation
/// inserts the fence and appends the audit event. A primary-key conflict is a
/// success only when every durable identity field is identical; it never emits
/// a second audit event.
///
/// # Errors
///
/// Returns [`SqlError`] when the tenant does not match `conn`, the insert or
/// audit write fails, or an existing commit has contradictory durable identity.
pub async fn record(
    conn: &mut TenantConn<'_>,
    commit: &ScribeBatchCommit,
    audit_event: &AuditEvent,
) -> Result<(), SqlError> {
    if conn.data_tenant_id() != commit.tenant {
        return Err(invariant(
            "scribe batch commit tenant does not match TenantConn",
        ));
    }
    let inserted: Option<bool> = sqlx::query_scalar(
        "INSERT INTO vala.scribe_batch_commits \
         (data_tenant_id, logical_table_fqn, batch_id, slice_set_digest, slice_count, \
          wal_node_id, wal_writer_epoch, wal_shard_id, wal_segment_sequence, \
          wal_lsn_min, wal_lsn_max, request_id) \
         VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         ON CONFLICT (data_tenant_id, logical_table_fqn, batch_id) DO NOTHING \
         RETURNING true",
    )
    .bind(&commit.logical_table_fqn)
    .bind(commit.batch_id)
    .bind(commit.slice_set_digest.as_slice())
    .bind(commit.slice_count)
    .bind(commit.wal_node_id)
    .bind(commit.wal_writer_epoch)
    .bind(commit.wal_shard_id)
    .bind(commit.wal_segment_sequence)
    .bind(commit.wal_lsn_min)
    .bind(commit.wal_lsn_max)
    .bind(commit.request_id)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    if inserted.is_some() {
        super::audit_outbox::append_audit(conn, audit_event).await?;
        return Ok(());
    }

    let existing: Option<ExistingScribeBatchCommit> = sqlx::query_as(
        "SELECT slice_set_digest, slice_count, wal_node_id, wal_writer_epoch, wal_shard_id, \
                    wal_segment_sequence, wal_lsn_min, wal_lsn_max, request_id \
               FROM vala.scribe_batch_commits \
              WHERE data_tenant_id = wyrd.current_tenant() \
                AND logical_table_fqn = $1 AND batch_id = $2",
    )
    .bind(&commit.logical_table_fqn)
    .bind(commit.batch_id)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    let Some(existing) = existing else {
        return Err(invariant(
            "scribe batch commit conflict was not visible through TenantConn",
        ));
    };
    if !existing.matches(commit) {
        return Err(invariant("scribe batch commit identity mismatch"));
    }
    Ok(())
}

/// Produces one invariant SQL error without widening the public error catalog.
fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}
