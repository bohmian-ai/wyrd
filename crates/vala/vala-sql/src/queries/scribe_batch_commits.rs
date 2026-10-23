//! Tenant-scoped durable Scribe batch-commit control state.
//!
//! The owner records the fsynced v4 WAL identity in the caller-owned
//! [`TenantConn`] transaction. A batch commit evaluates no principal
//! permission, so it is lineage only and appends no audit event.
//!
//! The fence also records when Wyrd ingested the batch. [`ingest_instant`]
//! reads the `PostgreSQL` clock that Scribe's admission clock is anchored on;
//! Scribe stamps each batch's admission instant on every row as
//! `wyrd_ingested_at` and [`record`] stores the same value here.
// raw-query grep allowlist: every statement is fixed and tenant-bound; `vala.scribe_batch_commits` post-dates the sqlx offline cache, so run `mise run sqlx:prepare` to promote them to macros.

use chrono::{DateTime, Utc};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::ids::TableVersion;

use crate::{OperatorPool, SqlError, TenantConn};

/// Immutable WAL identity that must match exactly for a replayed batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScribeBatchCommit {
    /// Authenticated tenant that owns the logical table and batch.
    pub tenant: DataTenantId,
    /// Canonical logical table FQN, not a physical table identifier.
    pub logical_table_fqn: String,
    /// Stable client batch identity.
    pub batch_id: uuid::Uuid,
    /// Table version the batch was accepted under; a retry is compared under
    /// this version, never the table's current one.
    pub table_version: TableVersion,
    /// SHA-256 digest over the batch's ordered logical slice identities.
    ///
    /// This is a client-reproducible fact: it binds slice ordinals, the Arrow
    /// schema fingerprint, and the logical row identity, and deliberately not
    /// the WAL payload bytes, which carry a fresh request identity on every
    /// attempt. An honest retry of the same batch therefore
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
    /// Persisted canonical logical table FQN, half of the lookup key.
    logical_table_fqn: String,
    /// Persisted batch identity, the other half of the lookup key.
    batch_id: uuid::Uuid,
    /// Persisted accepting table version; `None` for a fence recorded before
    /// versions existed, which no offered version matches.
    table_version: Option<i64>,
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
        self.matches_logical(commit)
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
    /// Logical identity is the rows themselves under the version that
    /// accepted them: the table version, the ordered slice-set digest, and
    /// the closed-set cardinality. A fence with no recorded version matches
    /// no offered batch. WAL coordinates and the request
    /// correlation describe *which attempt* durably wrote the batch, not what
    /// the batch is, so a client that re-sends the same batch under a new
    /// request is logically identical here and contradictory only in
    /// [`Self::matches`].
    #[must_use]
    fn matches_logical(&self, commit: &ScribeBatchCommit) -> bool {
        self.table_version == Some(commit.table_version.get())
            && self.slice_set_digest == commit.slice_set_digest
            && self.slice_count == commit.slice_count
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
        if self.table_version != Some(commit.table_version.get()) {
            divergent.push("table_version");
        }
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
/// The durable row remains authoritative. Matching table version and logical
/// digest/count at the original WAL coordinates restores; matching logical identity at different
/// coordinates suppresses duplicate restoration. Any logical mismatch fails.
///
/// # Errors
///
/// Returns [`SqlError`] when the tenant differs from `conn`, PostgreSQL is
/// unavailable, or an existing tenant/table/batch row contradicts version,
/// digest, or count.
pub async fn resolve_replay(
    conn: &mut TenantConn<'_>,
    commit: &ScribeBatchCommit,
) -> Result<ScribeBatchReplayResolution, SqlError> {
    if conn.data_tenant_id() != commit.tenant {
        return Err(invariant(
            "scribe batch replay tenant does not match TenantConn",
        ));
    }
    let existing = load(conn, std::iter::once(commit)).await?;
    let Some(existing) = find(&existing, commit) else {
        return Ok(ScribeBatchReplayResolution::Restore);
    };
    if !existing.matches_logical(commit) {
        return Err(invariant("scribe batch replay identity mismatch"));
    }
    if existing.matches(commit) {
        Ok(ScribeBatchReplayResolution::Restore)
    } else {
        Ok(ScribeBatchReplayResolution::Suppress)
    }
}

/// Resolves an uncertain transaction outcome against the durable batch fences.
///
/// One tenant-scoped read loads every offered fence's durable row, and each
/// is classified by comparing every persisted WAL, slice-set, and request
/// field. Key presence alone never proves that the caller's batch committed.
/// The result holds one resolution per offered fence, in offered order.
///
/// # Errors
///
/// Returns [`SqlError`] when a fence's tenant differs from `conn`, the lookup
/// is unavailable, or a durable row holds different rows under its identity.
pub async fn resolve<'a>(
    conn: &mut TenantConn<'_>,
    commits: impl Iterator<Item = &'a ScribeBatchCommit> + Clone,
) -> Result<Vec<ScribeBatchCommitResolution>, SqlError> {
    require_tenant(conn, commits.clone())?;
    let existing = load(conn, commits.clone()).await?;
    commits
        .map(|commit| match find(&existing, commit) {
            None => Ok(ScribeBatchCommitResolution::Absent),
            Some(existing) => classify(existing, commit),
        })
        .collect()
}

/// One batch's accepted logical identity, as a retry offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedScribeBatch {
    /// Canonical logical table FQN, half of the lookup key.
    pub logical_table_fqn: String,
    /// Stable client batch identity, the other half of the lookup key.
    pub batch_id: uuid::Uuid,
    /// Table version the retry was prepared under.
    pub table_version: TableVersion,
    /// Ordered logical slice-set digest of the retry.
    pub slice_set_digest: [u8; 32],
    /// Closed slice-set cardinality of the retry.
    pub slice_count: i32,
}

/// Whether `batch` is exactly the logical batch a durable fence accepted.
///
/// The exceptional recovery read for a retry that cannot be admitted as a
/// new batch, such as one naming a table version that is no longer current:
/// it runs only after that refusal, never on fresh ingest. A fence with no
/// recorded version, a different version, digest, or count, or no fence at
/// all answers `false`, and the caller keeps its original refusal.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot execute the tenant-scoped read.
pub async fn is_accepted(
    conn: &mut TenantConn<'_>,
    batch: &AcceptedScribeBatch,
) -> Result<bool, SqlError> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM vala.scribe_batch_commits \
          WHERE logical_table_fqn = $1 AND batch_id = $2 AND table_version = $3 \
            AND slice_set_digest = $4 AND slice_count = $5)",
    )
    .bind(&batch.logical_table_fqn)
    .bind(batch.batch_id)
    .bind(batch.table_version.get())
    .bind(batch.slice_set_digest.to_vec())
    .bind(batch.slice_count)
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Reads the `PostgreSQL` instant Scribe anchors its admission clock on.
///
/// PostgreSQL is the clock of record for `wyrd_ingested_at`: the value is
/// `statement_timestamp()` from the tenant transaction, so every replica
/// stamps from the same source. The caller commits or drops `conn`; the read
/// writes nothing.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot execute the read.
pub async fn ingest_instant(conn: &mut TenantConn<'_>) -> Result<DateTime<Utc>, SqlError> {
    sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Records one tenant group's Scribe commits, or classifies their conflicts.
///
/// The caller commits the supplied tenant transaction. One statement inserts
/// every offered fence; fences the insert did not create are classified by
/// one more read in the same transaction, and an all-fresh group needs none.
/// Each fence resolves in offered order: a first observation is
/// [`ScribeBatchCommitResolution::Committed`]; on a primary-key conflict the
/// same attempt is `Committed`, the same rows from a different attempt are
/// [`ScribeBatchCommitResolution::AlreadyCommitted`] and the caller must not
/// make them visible again, and different rows under the same batch identity
/// are a contradiction. `Absent` is never returned.
///
/// A fence is keyed by tenant, table, and batch id, never by its position:
/// `RETURNING` does not preserve input order, and one batch id may name
/// batches of different tables. A contradiction returns an error before the
/// caller commits, so none of the group's fresh fences become durable.
///
/// `ingested_at` is each batch's admission instant from [`ingest_instant`],
/// the same value stamped on its rows. It is not part of the identity: a
/// retry carries a fresh instant and still resolves against the first row.
///
/// # Errors
///
/// Returns [`SqlError`] when a fence's tenant does not match `conn`, the
/// insert fails, or an existing commit holds different rows under the same
/// identity.
pub async fn record(
    conn: &mut TenantConn<'_>,
    commits: &[(ScribeBatchCommit, DateTime<Utc>)],
) -> Result<Vec<ScribeBatchCommitResolution>, SqlError> {
    require_tenant(conn, commits.iter().map(|(commit, _)| commit))?;
    let inserted: Vec<(String, uuid::Uuid)> = sqlx::query_as(
        "INSERT INTO vala.scribe_batch_commits \
         (data_tenant_id, logical_table_fqn, batch_id, slice_set_digest, slice_count, \
          wal_node_id, wal_writer_epoch, wal_shard_id, wal_segment_sequence, \
          wal_lsn_min, wal_lsn_max, request_id, ingested_at, table_version) \
         SELECT wyrd.current_tenant(), offered.* \
           FROM unnest($1::text[], $2::uuid[], $3::bytea[], $4::integer[], $5::uuid[], \
                       $6::bigint[], $7::smallint[], $8::bigint[], $9::bigint[], \
                       $10::bigint[], $11::uuid[], $12::timestamptz[], \
                       $13::bigint[]) AS offered \
         ON CONFLICT (data_tenant_id, logical_table_fqn, batch_id) DO NOTHING \
         RETURNING logical_table_fqn, batch_id",
    )
    .bind(column(commits, |c| c.logical_table_fqn.clone()))
    .bind(column(commits, |c| c.batch_id))
    .bind(column(commits, |c| c.slice_set_digest.to_vec()))
    .bind(column(commits, |c| c.slice_count))
    .bind(column(commits, |c| c.wal_node_id))
    .bind(column(commits, |c| c.wal_writer_epoch))
    .bind(column(commits, |c| c.wal_shard_id))
    .bind(column(commits, |c| c.wal_segment_sequence))
    .bind(column(commits, |c| c.wal_lsn_min))
    .bind(column(commits, |c| c.wal_lsn_max))
    .bind(column(commits, |c| c.request_id))
    .bind(commits.iter().map(|(_, at)| *at).collect::<Vec<_>>())
    .bind(column(commits, |c| c.table_version.get()))
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    let mut returned = inserted;
    let fresh: Vec<bool> = commits
        .iter()
        .map(|(commit, _)| {
            returned
                .iter()
                .position(|(fqn, batch_id)| {
                    *fqn == commit.logical_table_fqn && *batch_id == commit.batch_id
                })
                .map(|position| returned.swap_remove(position))
                .is_some()
        })
        .collect();
    let conflicted = commits
        .iter()
        .zip(&fresh)
        .filter(|(_, fresh)| !**fresh)
        .map(|((commit, _), _)| commit);
    let existing = if fresh.iter().all(|fresh| *fresh) {
        Vec::new()
    } else {
        load(conn, conflicted).await?
    };
    commits
        .iter()
        .zip(fresh)
        .map(|((commit, _), fresh)| {
            if fresh {
                return Ok(ScribeBatchCommitResolution::Committed);
            }
            let existing = find(&existing, commit).ok_or_else(|| {
                invariant("scribe batch commit conflict was not visible through TenantConn")
            })?;
            classify(existing, commit)
        })
        .collect()
}

/// Returns the WAL segment sequence and COMMIT LSN of the newest fence of
/// one WAL shard stream, across every tenant, or `None` when it has none.
///
/// WAL recovery calls this before it ends a shard's chain at a bad record: a
/// fence in that segment beyond the last good record means an acknowledged
/// batch lies in the discarded region. A WAL shard stream spans tenants, so
/// the read runs on the operator session; the `vala.scribe_wal_newest_fence`
/// security-definer function answers it from the top of one index and returns
/// one WAL position, never a tenant's rows.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot execute the read.
// tenant-isolation: cross-tenant OperatorPool read of one WAL position per shard stream.
pub async fn newest_wal_fence(
    operator: &OperatorPool,
    node_id: uuid::Uuid,
    writer_epoch: i64,
    shard_id: i16,
) -> Result<Option<(i64, i64)>, SqlError> {
    sqlx::query_as("SELECT * FROM vala.scribe_wal_newest_fence($1, $2, $3)")
        .bind(node_id)
        .bind(writer_epoch)
        .bind(shard_id)
        .fetch_optional(operator.pool())
        .await
        .map_err(SqlError::from)
}

/// Loads the tenant-visible durable fences of `commits` in one read, without
/// interpreting identity.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot execute the tenant-scoped read.
async fn load<'a>(
    conn: &mut TenantConn<'_>,
    commits: impl Iterator<Item = &'a ScribeBatchCommit> + Clone,
) -> Result<Vec<ExistingScribeBatchCommit>, SqlError> {
    sqlx::query_as(
        "SELECT logical_table_fqn, batch_id, table_version, slice_set_digest, slice_count, \
                wal_node_id, \
                wal_writer_epoch, wal_shard_id, wal_segment_sequence, wal_lsn_min, \
                wal_lsn_max, request_id \
           FROM vala.scribe_batch_commits \
          WHERE (logical_table_fqn, batch_id) IN \
                (SELECT * FROM unnest($1::text[], $2::uuid[]))",
    )
    .bind(
        commits
            .clone()
            .map(|commit| commit.logical_table_fqn.clone())
            .collect::<Vec<_>>(),
    )
    .bind(commits.map(|commit| commit.batch_id).collect::<Vec<_>>())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Finds the durable fence `existing` holds for `commit`'s table and batch.
fn find<'a>(
    existing: &'a [ExistingScribeBatchCommit],
    commit: &ScribeBatchCommit,
) -> Option<&'a ExistingScribeBatchCommit> {
    existing.iter().find(|row| {
        row.logical_table_fqn == commit.logical_table_fqn && row.batch_id == commit.batch_id
    })
}

/// Classifies `commit` against the durable fence holding its identity.
///
/// # Errors
///
/// Returns [`SqlError`] when the durable fence holds different rows.
fn classify(
    existing: &ExistingScribeBatchCommit,
    commit: &ScribeBatchCommit,
) -> Result<ScribeBatchCommitResolution, SqlError> {
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

/// Refuses a fence offered for a tenant other than `conn`'s.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] for the first foreign fence.
fn require_tenant<'a>(
    conn: &TenantConn<'_>,
    mut commits: impl Iterator<Item = &'a ScribeBatchCommit>,
) -> Result<(), SqlError> {
    if commits.any(|commit| commit.tenant != conn.data_tenant_id()) {
        return Err(invariant(
            "scribe batch commit tenant does not match TenantConn",
        ));
    }
    Ok(())
}

/// Projects one fence field of every offered fence as a bindable array.
fn column<T>(
    commits: &[(ScribeBatchCommit, DateTime<Utc>)],
    field: impl Fn(&ScribeBatchCommit) -> T,
) -> Vec<T> {
    commits.iter().map(|(commit, _)| field(commit)).collect()
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
            table_version: wyrd_spec::vala::ids::TableVersion::INITIAL,
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
            logical_table_fqn: commit.logical_table_fqn.clone(),
            batch_id: commit.batch_id,
            table_version: Some(commit.table_version.get()),
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

    /// A fence accepted under another table version, or under no recorded
    /// version, is not the same logical batch even with identical rows.
    #[test]
    fn logical_identity_is_bound_to_the_accepting_table_version() {
        let (commit, mut existing) = identities();
        assert!(existing.matches_logical(&commit));

        existing.table_version = Some(commit.table_version.get() + 1);
        assert!(!existing.matches_logical(&commit));
        assert!(!existing.matches(&commit));
        existing.table_version = None;
        assert!(!existing.matches_logical(&commit));
        assert_eq!(existing.divergent_fields(&commit), vec!["table_version"]);
    }
}
