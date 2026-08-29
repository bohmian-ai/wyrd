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
    /// SHA-256 digest over the batch's ordered logical slice identities.
    ///
    /// This is a client-reproducible fact: it binds slice ordinals, the Arrow
    /// schema fingerprint, and the logical row identity, and deliberately not
    /// the WAL payload bytes, whose audit envelope carries a fresh request
    /// identity on every attempt. An honest retry of the same batch therefore
    /// produces this same digest, which is what makes idempotent acknowledgement
    /// distinguishable from a genuine identity contradiction.
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
    /// Persisted ordered logical slice-set digest.
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

    /// Returns whether this row holds the same logical batch as `commit`.
    ///
    /// Logical identity is the rows themselves: the ordered slice-set digest
    /// and the closed-set cardinality. WAL coordinates and the request
    /// correlation describe *which attempt* durably wrote the batch, not what
    /// the batch is, so a client that re-sends the same batch under a new
    /// request is logically identical here and contradictory only in
    /// [`Self::matches`].
    #[must_use]
    fn matches_logical(&self, commit: &ScribeBatchCommit) -> bool {
        self.slice_set_digest == commit.slice_set_digest && self.slice_count == commit.slice_count
    }

    /// Names the durable identity fields that diverge from `commit`.
    ///
    /// A batch-identity contradiction is a fail-closed internal error, so the
    /// operator sees only a refusal. This lists the columns that actually
    /// disagree so the refusal can be attributed to a real cause — a client
    /// reusing one batch identity for different rows, a truncated slice set,
    /// or a WAL owner that moved — instead of requiring a database read.
    #[must_use]
    fn divergent_fields(&self, commit: &ScribeBatchCommit) -> Vec<&'static str> {
        let mut divergent = Vec::new();
        if self.slice_set_digest != commit.slice_set_digest {
            divergent.push("slice_set_digest");
        }
        if self.slice_count != commit.slice_count {
            divergent.push("slice_count");
        }
        if self.wal_node_id != commit.wal_node_id {
            divergent.push("wal_node_id");
        }
        if self.wal_writer_epoch != commit.wal_writer_epoch {
            divergent.push("wal_writer_epoch");
        }
        if self.wal_shard_id != commit.wal_shard_id {
            divergent.push("wal_shard_id");
        }
        if self.wal_segment_sequence != commit.wal_segment_sequence {
            divergent.push("wal_segment_sequence");
        }
        if self.wal_lsn_min != commit.wal_lsn_min {
            divergent.push("wal_lsn_min");
        }
        if self.wal_lsn_max != commit.wal_lsn_max {
            divergent.push("wal_lsn_max");
        }
        if self.request_id != commit.request_id {
            divergent.push("request_id");
        }
        divergent
    }
}

/// Durable resolution of one exact tenant-scoped Scribe batch identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScribeBatchCommitResolution {
    /// No durable fence exists, so the identical transaction may be retried.
    Absent,
    /// The durable fence exists and every identity field matches exactly.
    Committed,
    /// The same logical batch already committed from a different WAL attempt.
    ///
    /// The batch identity is spent. The caller must acknowledge it without
    /// making its rows visible a second time; the WAL slices this attempt
    /// wrote are suppressed on replay by [`resolve_replay`] for the same
    /// reason.
    AlreadyCommitted,
}

/// Durable replay decision for one tenant/table/batch identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScribeBatchReplayResolution {
    /// No durable fence exists, or this is the canonical WAL location to restore.
    Restore,
    /// The logical batch already committed from another WAL location.
    Suppress,
}

/// Resolves whether one replayed WAL commit is canonical or a later exact retry.
///
/// The durable row remains authoritative. Matching logical digest/count at the
/// original WAL coordinates restores; matching logical identity at different
/// coordinates suppresses duplicate restoration. Any logical mismatch fails.
///
/// # Errors
///
/// Returns [`SqlError`] when the tenant differs from `conn`, PostgreSQL is
/// unavailable, or an existing tenant/table/batch row contradicts digest/count.
pub async fn resolve_replay(
    conn: &mut TenantConn<'_>,
    commit: &ScribeBatchCommit,
) -> Result<ScribeBatchReplayResolution, SqlError> {
    if conn.data_tenant_id() != commit.tenant {
        return Err(invariant(
            "scribe batch replay tenant does not match TenantConn",
        ));
    }
    let Some(existing) = load(conn, commit).await? else {
        return Ok(ScribeBatchReplayResolution::Restore);
    };
    if existing.slice_set_digest != commit.slice_set_digest
        || existing.slice_count != commit.slice_count
    {
        return Err(invariant("scribe batch replay identity mismatch"));
    }
    if existing.matches(commit) {
        Ok(ScribeBatchReplayResolution::Restore)
    } else {
        Ok(ScribeBatchReplayResolution::Suppress)
    }
}

/// Resolves an uncertain transaction outcome against the durable batch fence.
///
/// The lookup runs through the authenticated tenant connection and compares
/// every persisted WAL, slice-set, and request field. It never treats key
/// presence alone as proof that the caller's batch committed.
///
/// # Errors
///
/// Returns [`SqlError`] when the tenant differs from `conn`, the lookup is
/// unavailable, or the durable row holds different rows under this identity.
pub async fn resolve(
    conn: &mut TenantConn<'_>,
    commit: &ScribeBatchCommit,
) -> Result<ScribeBatchCommitResolution, SqlError> {
    if conn.data_tenant_id() != commit.tenant {
        return Err(invariant(
            "scribe batch commit tenant does not match TenantConn",
        ));
    }
    let existing = load(conn, commit).await?;
    match existing {
        None => Ok(ScribeBatchCommitResolution::Absent),
        Some(existing) if existing.matches(commit) => Ok(ScribeBatchCommitResolution::Committed),
        Some(existing) if existing.matches_logical(commit) => {
            Ok(ScribeBatchCommitResolution::AlreadyCommitted)
        }
        Some(existing) => {
            tracing::warn!(
                logical_table = %commit.logical_table_fqn,
                batch_id = %commit.batch_id,
                divergent = ?existing.divergent_fields(commit),
                durable_slice_count = existing.slice_count,
                offered_slice_count = commit.slice_count,
                "Scribe batch identity contradicts its durable fence"
            );
            Err(invariant("scribe batch commit identity mismatch"))
        }
    }
}

/// Records a Scribe commit and its audit event, or classifies the conflict.
///
/// The caller commits the supplied tenant transaction. A first observation
/// inserts the fence, appends the audit event, and reports
/// [`ScribeBatchCommitResolution::Committed`]. A primary-key conflict never
/// emits a second audit event and is classified by logical identity: the same
/// attempt is `Committed`, the same rows from a different attempt are
/// [`ScribeBatchCommitResolution::AlreadyCommitted`] and the caller must not
/// make them visible again, and different rows under the same batch identity
/// are a contradiction. `Absent` is never returned.
///
/// # Errors
///
/// Returns [`SqlError`] when the tenant does not match `conn`, the insert or
/// audit write fails, the audit request identity differs from the durable
/// fence, or an existing commit holds different rows under the same identity.
pub async fn record(
    conn: &mut TenantConn<'_>,
    commit: &ScribeBatchCommit,
    audit_event: &AuditEvent,
) -> Result<ScribeBatchCommitResolution, SqlError> {
    if conn.data_tenant_id() != commit.tenant {
        return Err(invariant(
            "scribe batch commit tenant does not match TenantConn",
        ));
    }
    let audit_request_id = uuid::Uuid::parse_str(audit_event.request_id.as_str())
        .map_err(|_| invariant("scribe ingest audit request identity is not a UUID"))?;
    if audit_request_id != commit.request_id {
        return Err(invariant(
            "scribe ingest audit request identity does not match its control fence",
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
        return Ok(ScribeBatchCommitResolution::Committed);
    }

    let existing = load(conn, commit).await?;
    let Some(existing) = existing else {
        return Err(invariant(
            "scribe batch commit conflict was not visible through TenantConn",
        ));
    };
    if existing.matches(commit) {
        return Ok(ScribeBatchCommitResolution::Committed);
    }
    if existing.matches_logical(commit) {
        return Ok(ScribeBatchCommitResolution::AlreadyCommitted);
    }
    tracing::warn!(
        logical_table = %commit.logical_table_fqn,
        batch_id = %commit.batch_id,
        divergent = ?existing.divergent_fields(commit),
        durable_slice_count = existing.slice_count,
        offered_slice_count = commit.slice_count,
        "Scribe batch identity contradicts its durable fence"
    );
    Err(invariant("scribe batch commit identity mismatch"))
}

/// Loads one tenant-visible durable batch fence without interpreting identity.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot execute the tenant-scoped read.
async fn load(
    conn: &mut TenantConn<'_>,
    commit: &ScribeBatchCommit,
) -> Result<Option<ExistingScribeBatchCommit>, SqlError> {
    sqlx::query_as(
        "SELECT slice_set_digest, slice_count, wal_node_id, wal_writer_epoch, wal_shard_id, \
                    wal_segment_sequence, wal_lsn_min, wal_lsn_max, request_id \
               FROM vala.scribe_batch_commits \
              WHERE logical_table_fqn = $1 AND batch_id = $2",
    )
    .bind(&commit.logical_table_fqn)
    .bind(commit.batch_id)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Produces one invariant SQL error without widening the public error catalog.
fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
/// Exact durable-identity comparison proofs independent of PostgreSQL IO.
mod tests {
    use super::{ExistingScribeBatchCommit, ScribeBatchCommit};

    /// Builds one canonical control-fence identity and its persisted projection.
    fn identities() -> (ScribeBatchCommit, ExistingScribeBatchCommit) {
        let commit = ScribeBatchCommit {
            tenant: wyrd_spec::DataTenantId::new_v7(),
            logical_table_fqn: "vala.bifrost.events".to_owned(),
            batch_id: uuid::Uuid::now_v7(),
            slice_set_digest: [7; 32],
            slice_count: 2,
            wal_node_id: uuid::Uuid::now_v7(),
            wal_writer_epoch: 4,
            wal_shard_id: 3,
            wal_segment_sequence: 8,
            wal_lsn_min: 10,
            wal_lsn_max: 12,
            request_id: uuid::Uuid::now_v7(),
        };
        let existing = ExistingScribeBatchCommit {
            slice_set_digest: commit.slice_set_digest.to_vec(),
            slice_count: commit.slice_count,
            wal_node_id: commit.wal_node_id,
            wal_writer_epoch: commit.wal_writer_epoch,
            wal_shard_id: commit.wal_shard_id,
            wal_segment_sequence: commit.wal_segment_sequence,
            wal_lsn_min: commit.wal_lsn_min,
            wal_lsn_max: commit.wal_lsn_max,
            request_id: commit.request_id,
        };
        (commit, existing)
    }

    /// Exact identity succeeds while any durable contradiction fails closed.
    #[test]
    fn durable_batch_resolution_requires_every_identity_field() {
        let (commit, mut existing) = identities();
        assert!(existing.matches(&commit));

        existing.wal_lsn_max += 1;
        assert!(!existing.matches(&commit));
        existing.wal_lsn_max = commit.wal_lsn_max;
        existing.slice_set_digest[0] ^= 1;
        assert!(!existing.matches(&commit));
        existing.slice_set_digest = commit.slice_set_digest.to_vec();
        existing.request_id = uuid::Uuid::now_v7();
        assert!(!existing.matches(&commit));
    }
}
