//! Durable statements behind Oracle reader authority.
//!
//! Three owners share one rule: every mutation that changes what Forge may
//! destroy first takes a `FOR UPDATE` lock on a single row, in one order, and
//! commits with the audit its caller appends in the same transaction. The order
//! is always the `vala.cluster_nodes` Oracle role row, then
//! `vala.oracle_reader_epochs`, then the table's
//! `vala.bifrost_table_maintenance_authority` row; nothing here takes the
//! reverse order.
//!
//! Every read fails closed. A missing row, an identity mismatch, an unknown
//! encoding version, or a digest that does not reproduce is contradictory
//! evidence, and none of those may degrade into "nothing is protected".
// raw-query grep allowlist: the Oracle reader-authority tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use wyrd_spec::DataTenantId;

use crate::row_types::oracle_reader_authority::{
    ANCESTRY_DIGEST_VERSION, FRONTIER_ENCODING_VERSION, OracleEpochRow, OracleEpochState,
    OracleLeaseSample, ProtectionCas, ProtectionFrontier, ProtectionKey, ProtectionMember,
    ProtectionRecord, TableAuthorityIdentity,
};
use crate::{OperatorPool, SqlError, TenantConn};

/// Exact catalog name every reader-protection row must carry.
///
/// Duplicated as a durable expectation rather than imported from the Redux
/// crate because `vala-sql` sits below it; the schema check constraint, this
/// constant, and `catalog::BIFROST_CATALOG_NAME` are asserted equal by the
/// schema-contract test.
pub const BIFROST_CATALOG_NAME: &str = "wyrd-redux";

/// Constructs a fail-closed invariant error without leaking row payloads.
pub(crate) fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}

/// Decodes one stored 16-byte table UID.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] when the stored value is not
/// exactly 16 bytes, which the schema forbids and therefore indicates
/// corruption rather than a shorter identity.
fn table_uid(bytes: Vec<u8>) -> Result<[u8; 16], SqlError> {
    <[u8; 16]>::try_from(bytes.as_slice())
        .map_err(|_| invariant("stored Bifrost table UID is not 16 bytes"))
}

/// Decodes one stored 32-byte digest.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] when the stored value is not
/// exactly 32 bytes.
pub(crate) fn digest32(bytes: Vec<u8>) -> Result<[u8; 32], SqlError> {
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| invariant("stored reader protection digest is not 32 bytes"))
}

/// The single SQL serialization boundary for one tenant-qualified table.
///
/// Reader-protection expansion, narrowing, release, and snapshot-expiration
/// preparation all lock this one row, which is what gives the
/// admission-versus-destruction race exactly one durable winner per table
/// without taking a global physical lock.
pub struct BifrostTableMaintenanceAuthority<'conn, 'tx> {
    /// Tenant-bound connection every statement runs inside.
    conn: &'conn mut TenantConn<'tx>,
}

impl<'conn, 'tx> BifrostTableMaintenanceAuthority<'conn, 'tx> {
    /// Binds the serialization owner to one tenant transaction.
    pub fn new(conn: &'conn mut TenantConn<'tx>) -> Self {
        Self { conn }
    }

    /// Inserts the authority row for one newly registered Bifrost table.
    ///
    /// Registration writes this row inside its own transaction so the row
    /// exists before any reader or maintenance owner can want it. That is what
    /// lets every consumer treat a missing row as an identity failure instead
    /// of falling back to an advisory lock.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity names a
    /// foreign catalog or a blank name segment, and [`SqlError`] when the
    /// statement fails, including the foreign key to `vala.bifrost_tables`.
    pub async fn register(&mut self, identity: &TableAuthorityIdentity) -> Result<(), SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        sqlx::query(
            r"
            INSERT INTO vala.bifrost_table_maintenance_authority
                (data_tenant_id, catalog_name, namespace_name, table_name, table_uid)
            VALUES (wyrd.current_tenant(), $1, $2, $3, $4)
            ON CONFLICT (data_tenant_id, catalog_name, namespace_name, table_name)
            DO UPDATE SET table_uid = EXCLUDED.table_uid
            ",
        )
        .bind(&identity.catalog_name)
        .bind(&identity.namespace_name)
        .bind(&identity.table_name)
        .bind(identity.table_uid.as_slice())
        .execute(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Takes the exact table's serialization row `FOR UPDATE`.
    ///
    /// The lock is held for the remainder of the caller's transaction, so every
    /// competing protection or expiration decision for this table waits here
    /// rather than racing on the protection rows themselves.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity is malformed,
    /// when no row exists for the exact catalog/namespace/table, or when the
    /// stored table UID differs from the caller's — all of which are identity
    /// failures that must fail closed rather than proceed unserialized.
    pub async fn lock(&mut self, identity: &TableAuthorityIdentity) -> Result<(), SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        let stored: Option<Vec<u8>> = sqlx::query_scalar(
            r"
            SELECT table_uid
              FROM vala.bifrost_table_maintenance_authority
             WHERE data_tenant_id = wyrd.current_tenant()
               AND catalog_name = $1
               AND namespace_name = $2
               AND table_name = $3
               FOR UPDATE
            ",
        )
        .bind(&identity.catalog_name)
        .bind(&identity.namespace_name)
        .bind(&identity.table_name)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let stored = stored.ok_or_else(|| {
            invariant("Bifrost table has no maintenance authority row to serialize on")
        })?;
        if table_uid(stored)? != identity.table_uid {
            return Err(invariant(
                "Bifrost table maintenance authority names a different registered table UID",
            ));
        }
        Ok(())
    }

    /// Returns every snapshot of this table an unresolved Forge expiration has
    /// already claimed, in ascending order.
    ///
    /// This is the read side of the same serialization boundary [`Self::lock`]
    /// owns: a caller that holds the table row and finds a requested snapshot
    /// here lost the race to a prepared expiration and must re-resolve rather
    /// than widen over a snapshot that is about to disappear. The owner never
    /// mutates a claim; only the fenced Forge lifecycle does.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity is malformed
    /// and [`SqlError`] when the admission-index read fails.
    pub async fn claimed_snapshots(
        &mut self,
        identity: &TableAuthorityIdentity,
    ) -> Result<Vec<i64>, SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        sqlx::query_scalar(
            r"
            SELECT snapshot_id
              FROM vala.forge_snapshot_expiration_claims
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
             ORDER BY snapshot_id
            ",
        )
        .bind(identity.table_uid.as_slice())
        .fetch_all(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)
    }

    /// Counts this table's unresolved expired-cleanup preparations.
    ///
    /// A prepared cleanup candidate may already have been handed to the object
    /// store, so its object's existence is unknown until the owning attempt
    /// settles it. This read shares [`Self::lock`]'s serialization boundary, so
    /// a widening that observes zero here cannot be overtaken by a preparation
    /// that commits afterwards.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity is malformed
    /// and [`SqlError`] when the task read fails.
    pub async fn prepared_cleanup_candidates(
        &mut self,
        identity: &TableAuthorityIdentity,
    ) -> Result<i64, SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        sqlx::query_scalar(
            r"
            SELECT COUNT(*)
              FROM vala.forge_tasks
             WHERE data_tenant_id = wyrd.current_tenant()
               AND catalog_name = $1
               AND namespace_name = $2
               AND table_name = $3
               AND strategy = 'expired_cleanup'
               AND state = 'prepared'
               AND jsonb_typeof(evidence->'prepared_candidate_index') = 'number'
            ",
        )
        .bind(&identity.catalog_name)
        .bind(&identity.namespace_name)
        .bind(&identity.table_name)
        .fetch_one(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)
    }
}

/// Durable owner of one process's bounded, renewable Oracle reader lease.
///
/// Every statement here locks the exact `vala.cluster_nodes` Oracle role row
/// before touching the epoch, so registration of a replacement and renewal by
/// the incumbent serialize on one row: an epoch whose fence has been superseded
/// cannot renew even for one more round.
pub struct OracleReaderEpochs<'conn, 'tx> {
    /// System-owner-bound connection every statement runs inside.
    conn: &'conn mut TenantConn<'tx>,
}

impl<'conn, 'tx> OracleReaderEpochs<'conn, 'tx> {
    /// Binds the epoch owner to one `SYSTEM_OWNER` transaction.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the connection is bound to
    /// any tenant other than [`DataTenantId::SYSTEM_OWNER`]; an epoch is a
    /// process fact and must never be written under a data tenant.
    pub fn new(conn: &'conn mut TenantConn<'tx>) -> Result<Self, SqlError> {
        if conn.data_tenant_id() != DataTenantId::SYSTEM_OWNER {
            return Err(invariant(
                "Oracle reader epochs are owned by the system tenant only",
            ));
        }
        Ok(Self { conn })
    }

    /// Locks this node's Oracle role row and returns its current fence.
    ///
    /// This is the first statement of every epoch transaction. Taking it before
    /// the epoch row is what fixes one global lock order and makes the
    /// registration-versus-renewal race decidable.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the node has no Oracle
    /// role row or its stored fence is negative, and [`SqlError`] when the
    /// statement fails.
    async fn lock_role_fence(&mut self, node_id: uuid::Uuid) -> Result<i64, SqlError> {
        let fence: Option<i64> = sqlx::query_scalar(
            r"
            SELECT fencing_token
              FROM vala.cluster_nodes
             WHERE data_tenant_id = wyrd.current_tenant()
               AND node_id = $1
               AND role = 'oracle'
               FOR UPDATE
            ",
        )
        .bind(node_id)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let fence = fence.ok_or_else(|| invariant("node holds no Oracle cluster role row"))?;
        if fence <= 0 {
            return Err(invariant("Oracle cluster role fence is not positive"));
        }
        Ok(fence)
    }

    /// Locks the role row and proves it still carries the caller's exact fence.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the role row is absent or
    /// a replacement has already advanced the fence past the caller's.
    async fn lock_exact_fence(
        &mut self,
        node_id: uuid::Uuid,
        fencing_token: i64,
    ) -> Result<(), SqlError> {
        if self.lock_role_fence(node_id).await? != fencing_token {
            return Err(invariant(
                "Oracle cluster role fence was replaced; this epoch can no longer act",
            ));
        }
        Ok(())
    }

    /// Acquires one epoch at revision 1 under the node's current exact fence.
    ///
    /// The lease clock is Postgres: expiry is written as
    /// `statement_timestamp() + lease`, and the same statement returns the
    /// database time it used, so the caller converts a remaining duration onto
    /// its monotonic clock rather than comparing two wall clocks.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the role row is absent,
    /// when its fence is not the caller's, or when a row already exists at this
    /// exact new fence, which is an invariant failure rather than a retry.
    /// Returns [`SqlError`] when a statement fails.
    pub async fn acquire(
        &mut self,
        node_id: uuid::Uuid,
        fencing_token: i64,
        lease: std::time::Duration,
    ) -> Result<OracleLeaseSample, SqlError> {
        self.lock_exact_fence(node_id, fencing_token).await?;
        let lease_seconds = lease_seconds(lease)?;
        let row: Option<(DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
            r"
            INSERT INTO vala.oracle_reader_epochs
                (epoch_owner_tenant_id, node_id, fencing_token, state, state_revision,
                 acquired_at, activated_at, renewed_at, lease_expires_at, invalidated_at)
            VALUES (wyrd.current_tenant(), $1, $2, 'acquired', 1,
                    statement_timestamp(), NULL, statement_timestamp(),
                    statement_timestamp() + make_interval(secs => $3), NULL)
            ON CONFLICT DO NOTHING
            RETURNING statement_timestamp(), lease_expires_at
            ",
        )
        .bind(node_id)
        .bind(fencing_token)
        .bind(lease_seconds)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let (database_now, lease_expires_at) = row.ok_or_else(|| {
            invariant("an Oracle reader epoch already exists at this exact new fence")
        })?;
        Ok(OracleLeaseSample {
            database_now,
            lease_expires_at,
            state_revision: 1,
        })
    }

    /// Commits `acquired -> active` at exactly one next revision.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the fence was replaced or
    /// no `acquired` row matches the expected revision, and [`SqlError`] when a
    /// statement fails.
    pub async fn activate(
        &mut self,
        node_id: uuid::Uuid,
        fencing_token: i64,
        expected_revision: i64,
    ) -> Result<OracleLeaseSample, SqlError> {
        self.lock_exact_fence(node_id, fencing_token).await?;
        let row: Option<(DateTime<Utc>, DateTime<Utc>, i64)> = sqlx::query_as(
            r"
            UPDATE vala.oracle_reader_epochs
               SET state = 'active',
                   state_revision = state_revision + 1,
                   activated_at = statement_timestamp()
             WHERE epoch_owner_tenant_id = wyrd.current_tenant()
               AND node_id = $1
               AND fencing_token = $2
               AND state = 'acquired'
               AND state_revision = $3
               AND lease_expires_at > statement_timestamp()
            RETURNING statement_timestamp(), lease_expires_at, state_revision
            ",
        )
        .bind(node_id)
        .bind(fencing_token)
        .bind(expected_revision)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let (database_now, lease_expires_at, state_revision) = row.ok_or_else(|| {
            invariant("Oracle reader epoch could not activate at its confirmed revision")
        })?;
        Ok(OracleLeaseSample {
            database_now,
            lease_expires_at,
            state_revision,
        })
    }

    /// Extends the lease at the caller's confirmed revision.
    ///
    /// Renewal is bookkeeping, not an auditable transition: it changes only
    /// `renewed_at`, `lease_expires_at`, and the revision. It cannot revive an
    /// expired, draining, invalidated, or replacement-fenced epoch, which is
    /// why the predicate names both the allowed states and unexpired database
    /// time.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the exact fence was
    /// replaced, and [`SqlError`] when a statement fails. A predicate that
    /// matches no row returns `Ok(None)`: the caller must treat that as lease
    /// loss, not as a retryable error.
    pub async fn renew(
        &mut self,
        node_id: uuid::Uuid,
        fencing_token: i64,
        expected_revision: i64,
        lease: std::time::Duration,
    ) -> Result<Option<OracleLeaseSample>, SqlError> {
        self.lock_exact_fence(node_id, fencing_token).await?;
        let lease_seconds = lease_seconds(lease)?;
        let row: Option<(DateTime<Utc>, DateTime<Utc>, i64)> = sqlx::query_as(
            r"
            UPDATE vala.oracle_reader_epochs
               SET renewed_at = statement_timestamp(),
                   lease_expires_at = statement_timestamp() + make_interval(secs => $4),
                   state_revision = state_revision + 1
             WHERE epoch_owner_tenant_id = wyrd.current_tenant()
               AND node_id = $1
               AND fencing_token = $2
               AND state_revision = $3
               AND state IN ('acquired', 'active')
               AND lease_expires_at > statement_timestamp()
            RETURNING statement_timestamp(), lease_expires_at, state_revision
            ",
        )
        .bind(node_id)
        .bind(fencing_token)
        .bind(expected_revision)
        .bind(lease_seconds)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(row.map(
            |(database_now, lease_expires_at, state_revision)| OracleLeaseSample {
                database_now,
                lease_expires_at,
                state_revision,
            },
        ))
    }

    /// Commits one audited lifecycle edge at exactly one next revision.
    ///
    /// `Draining` is reachable only from `active`; `Invalidated` is reachable
    /// from `acquired`, `active`, or `draining`. An epoch that never admitted a
    /// read goes straight to `invalidated` rather than inventing a draining
    /// edge it never earned.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] for a target state that is not
    /// a legal edge, a replaced fence, or a predicate that matches no row, and
    /// [`SqlError`] when a statement fails.
    pub async fn transition(
        &mut self,
        node_id: uuid::Uuid,
        fencing_token: i64,
        expected_revision: i64,
        target: OracleEpochState,
    ) -> Result<i64, SqlError> {
        let sources: &[&str] = match target {
            OracleEpochState::Draining => &["active"],
            OracleEpochState::Invalidated => &["acquired", "active", "draining"],
            OracleEpochState::Acquired | OracleEpochState::Active => {
                return Err(invariant(
                    "Oracle reader epoch acquisition and activation have their own statements",
                ));
            }
        };
        self.lock_exact_fence(node_id, fencing_token).await?;
        let sources: Vec<String> = sources.iter().map(|state| (*state).to_owned()).collect();
        let revision: Option<i64> = sqlx::query_scalar(
            r"
            UPDATE vala.oracle_reader_epochs
               SET state = $4,
                   state_revision = state_revision + 1,
                   invalidated_at = CASE WHEN $4 = 'invalidated'
                                         THEN statement_timestamp()
                                         ELSE invalidated_at END
             WHERE epoch_owner_tenant_id = wyrd.current_tenant()
               AND node_id = $1
               AND fencing_token = $2
               AND state_revision = $3
               AND state = ANY($5)
            RETURNING state_revision
            ",
        )
        .bind(node_id)
        .bind(fencing_token)
        .bind(expected_revision)
        .bind(target.as_str())
        .bind(&sources)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        revision.ok_or_else(|| {
            invariant("Oracle reader epoch transition found no row at its confirmed revision")
        })
    }

    /// Invalidates a predecessor epoch Postgres proves is expired.
    ///
    /// Elapsed local time, a stale heartbeat, and replacement startup are never
    /// predicates here: the only evidence accepted is
    /// `lease_expires_at <= statement_timestamp()` evaluated by the database
    /// itself. Recovery locks the role row first even though the fence it finds
    /// may already belong to a replacement, so the shared lock order holds.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the node has no Oracle
    /// role row, and [`SqlError`] when a statement fails. `Ok(None)` means the
    /// epoch is not provably expired and must be left alone.
    pub async fn invalidate_expired(
        &mut self,
        node_id: uuid::Uuid,
        fencing_token: i64,
    ) -> Result<Option<i64>, SqlError> {
        self.lock_role_fence(node_id).await?;
        let revision: Option<i64> = sqlx::query_scalar(
            r"
            UPDATE vala.oracle_reader_epochs
               SET state = 'invalidated',
                   state_revision = state_revision + 1,
                   invalidated_at = statement_timestamp()
             WHERE epoch_owner_tenant_id = wyrd.current_tenant()
               AND node_id = $1
               AND fencing_token = $2
               AND state IN ('acquired', 'active', 'draining')
               AND lease_expires_at <= statement_timestamp()
            RETURNING state_revision
            ",
        )
        .bind(node_id)
        .bind(fencing_token)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(revision)
    }

    /// Deletes one invalidated epoch at its exact last confirmed revision.
    ///
    /// The caller must already have proven that no protection header remains
    /// for this epoch; retirement is the last statement of that sequence and
    /// manufactures no further revision.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when a header still exists for
    /// the epoch or no invalidated row matches the expected revision, and
    /// [`SqlError`] when a statement fails.
    pub async fn retire(
        &mut self,
        node_id: uuid::Uuid,
        fencing_token: i64,
        expected_revision: i64,
    ) -> Result<(), SqlError> {
        // The count must span every tenant while this transaction is bound to
        // the system owner, so it goes through the execute-only read-only
        // function rather than a statement forced RLS would answer with zero.
        let remaining: i64 =
            sqlx::query_scalar(r"SELECT vala.oracle_epoch_protection_count($1, $2)")
                .bind(node_id)
                .bind(fencing_token)
                .fetch_one(&mut **self.conn.transaction())
                .await
                .map_err(SqlError::from)?;
        if remaining != 0 {
            return Err(invariant(
                "Oracle reader epoch still protects tables and cannot be retired",
            ));
        }
        let deleted = sqlx::query(
            r"
            DELETE FROM vala.oracle_reader_epochs
             WHERE epoch_owner_tenant_id = wyrd.current_tenant()
               AND node_id = $1
               AND fencing_token = $2
               AND state = 'invalidated'
               AND state_revision = $3
            ",
        )
        .bind(node_id)
        .bind(fencing_token)
        .bind(expected_revision)
        .execute(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        if deleted.rows_affected() == 0 {
            return Err(invariant(
                "Oracle reader epoch retirement found no invalidated row at its revision",
            ));
        }
        Ok(())
    }

    /// Reads one epoch row exactly as Postgres holds it.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] for an unknown stored state and
    /// [`SqlError`] when the statement fails.
    pub async fn read(
        &mut self,
        node_id: uuid::Uuid,
        fencing_token: i64,
    ) -> Result<Option<OracleEpochRow>, SqlError> {
        let row: Option<EpochDbRow> = sqlx::query_as(
            r"
            SELECT node_id, fencing_token, state, state_revision, acquired_at,
                   activated_at, renewed_at, lease_expires_at, invalidated_at
              FROM vala.oracle_reader_epochs
             WHERE epoch_owner_tenant_id = wyrd.current_tenant()
               AND node_id = $1
               AND fencing_token = $2
            ",
        )
        .bind(node_id)
        .bind(fencing_token)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        row.map(TryInto::try_into).transpose()
    }
}

/// Converts a lease duration into the seconds Postgres builds an interval from.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] for a zero or unrepresentable
/// lease, both of which would write an already-expired row.
fn lease_seconds(lease: std::time::Duration) -> Result<f64, SqlError> {
    let seconds = lease.as_secs_f64();
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(invariant("Oracle reader lease duration is not positive"));
    }
    Ok(seconds)
}

/// Raw epoch projection decoded and validated before becoming authority.
#[derive(sqlx::FromRow)]
struct EpochDbRow {
    /// Physical node holding the epoch.
    node_id: uuid::Uuid,
    /// Exact Oracle role fence.
    fencing_token: i64,
    /// Stored state discriminator awaiting fail-closed parsing.
    state: String,
    /// Monotonic revision.
    state_revision: i64,
    /// Database time at acquisition.
    acquired_at: DateTime<Utc>,
    /// Database time at activation, when activated.
    activated_at: Option<DateTime<Utc>>,
    /// Database time of the last renewal.
    renewed_at: DateTime<Utc>,
    /// Database time the lease expires.
    lease_expires_at: DateTime<Utc>,
    /// Database time at invalidation, when invalidated.
    invalidated_at: Option<DateTime<Utc>>,
}

impl TryFrom<EpochDbRow> for OracleEpochRow {
    type Error = SqlError;

    /// Decodes one stored epoch row, failing closed on an unknown state.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] for unknown stored state text
    /// or a non-positive revision.
    fn try_from(row: EpochDbRow) -> Result<Self, Self::Error> {
        if row.state_revision < 1 {
            return Err(invariant("stored Oracle reader epoch has no revision"));
        }
        Ok(Self {
            node_id: row.node_id,
            fencing_token: row.fencing_token,
            state: OracleEpochState::parse(&row.state)?,
            state_revision: row.state_revision,
            acquired_at: row.acquired_at,
            activated_at: row.activated_at,
            renewed_at: row.renewed_at,
            lease_expires_at: row.lease_expires_at,
            invalidated_at: row.invalidated_at,
        })
    }
}

/// Durable owner of one epoch's per-table protection frontier.
///
/// Every mutation expects the caller to already hold the table's
/// `bifrost_table_maintenance_authority` row, and every one of them is a
/// compare-and-set on the table-local revision. A conflict returns the complete
/// winning record so the coordinator can adopt it when it already covers the
/// local cut instead of recomputing blindly.
pub struct OracleTableProtections<'conn, 'tx> {
    /// Tenant-bound transaction every statement runs inside.
    ///
    /// Owning the [`TenantConn`] itself, rather than a connection borrowed out
    /// of one, is what makes every `&mut self` method below provably
    /// tenant-scoped: there is no way to construct this owner from a connection
    /// whose RLS binding was never established.
    conn: &'conn mut TenantConn<'tx>,
}

impl<'conn, 'tx> OracleTableProtections<'conn, 'tx> {
    /// Binds the protection owner to one tenant transaction.
    pub fn new(conn: &'conn mut TenantConn<'tx>) -> Self {
        Self { conn }
    }

    /// Reads and validates one epoch's complete header and members.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the header or any member
    /// fails validation, when the stored identity payload disagrees with the
    /// caller's, or when a digest does not reproduce, and [`SqlError`] when a
    /// statement fails.
    pub async fn read(
        &mut self,
        identity: &TableAuthorityIdentity,
        node_id: uuid::Uuid,
        fencing_token: i64,
    ) -> Result<Option<ProtectionRecord>, SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        let header: Option<HeaderDbRow> = sqlx::query_as(
            r"
            SELECT catalog_name, namespace_name, table_name, revision,
                   frontier_encoding_version, frontier_digest, updated_at
              FROM vala.oracle_table_protections
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
               AND node_id = $2
               AND fencing_token = $3
            ",
        )
        .bind(identity.table_uid.as_slice())
        .bind(node_id)
        .bind(fencing_token)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let Some(header) = header else {
            return Ok(None);
        };
        if header.catalog_name != identity.catalog_name
            || header.namespace_name != identity.namespace_name
            || header.table_name != identity.table_name
        {
            return Err(invariant(
                "stored reader protection names a different table than the caller",
            ));
        }
        let members = self.read_members(identity, node_id, fencing_token).await?;
        let record = ProtectionRecord {
            revision: header.revision,
            frontier_encoding_version: header.frontier_encoding_version,
            frontier_digest: digest32(header.frontier_digest)?,
            updated_at: header.updated_at,
            frontier: ProtectionFrontier {
                members: sorted_members(members),
            },
        };
        record.validate(identity)?;
        Ok(Some(record))
    }

    /// Reads one header's members and decodes each into a validated value.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] for a malformed digest or path
    /// and [`SqlError`] when the statement fails.
    async fn read_members(
        &mut self,
        identity: &TableAuthorityIdentity,
        node_id: uuid::Uuid,
        fencing_token: i64,
    ) -> Result<Vec<ProtectionMember>, SqlError> {
        let rows: Vec<MemberDbRow> = sqlx::query_as(
            r"
            SELECT protected_snapshot_id, protected_snapshot_timestamp_ms,
                   retained_head_snapshot_id, retained_head_timestamp_ms,
                   ancestry_path, ancestry_digest_version, ancestry_digest
              FROM vala.oracle_table_protection_members
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
               AND node_id = $2
               AND fencing_token = $3
             ORDER BY protected_snapshot_id
            ",
        )
        .bind(identity.table_uid.as_slice())
        .bind(node_id)
        .bind(fencing_token)
        .fetch_all(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let mut members = Vec::with_capacity(rows.len());
        for row in rows {
            let member = ProtectionMember {
                protected_snapshot_id: row.protected_snapshot_id,
                protected_snapshot_timestamp_ms: row.protected_snapshot_timestamp_ms,
                retained_head_snapshot_id: row.retained_head_snapshot_id,
                retained_head_timestamp_ms: row.retained_head_timestamp_ms,
                ancestry_path: row.ancestry_path,
                ancestry_digest_version: row.ancestry_digest_version,
                ancestry_digest: digest32(row.ancestry_digest)?,
            };
            member.validate(identity)?;
            members.push(member);
        }
        Ok(members)
    }

    /// Commits one next revision of a table's frontier, or reports the winner.
    ///
    /// `expected_revision` is `None` for a first protection and the caller's
    /// locally confirmed revision otherwise. An empty frontier is a release:
    /// it removes the header and its members rather than storing a header that
    /// protects nothing.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity or frontier
    /// is malformed, and [`SqlError`] when a statement fails. A revision
    /// mismatch is [`ProtectionCas::Conflict`], not an error, because the
    /// caller must inspect the winner before deciding.
    pub async fn commit(
        &mut self,
        identity: &TableAuthorityIdentity,
        node_id: uuid::Uuid,
        fencing_token: i64,
        expected_revision: Option<i64>,
        frontier: &ProtectionFrontier,
    ) -> Result<ProtectionCas, SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        for member in &frontier.members {
            member.validate(identity)?;
        }
        let current = self.read(identity, node_id, fencing_token).await?;
        match (expected_revision, current.as_ref()) {
            (None, None) => {}
            (Some(expected), Some(record)) if record.revision == expected => {}
            (_, record) => {
                return Ok(ProtectionCas::Conflict(record.cloned().map(Box::new)));
            }
        }
        let next_revision = expected_revision.unwrap_or(0) + 1;

        if frontier.is_empty() {
            sqlx::query(
                r"
                DELETE FROM vala.oracle_table_protections
                 WHERE data_tenant_id = wyrd.current_tenant()
                   AND table_uid = $1 AND node_id = $2 AND fencing_token = $3
                ",
            )
            .bind(identity.table_uid.as_slice())
            .bind(node_id)
            .bind(fencing_token)
            .execute(&mut **self.conn.transaction())
            .await
            .map_err(SqlError::from)?;
            return Ok(ProtectionCas::Committed(Box::new(ProtectionRecord {
                revision: next_revision,
                frontier_encoding_version: FRONTIER_ENCODING_VERSION,
                frontier_digest: frontier.digest(identity),
                updated_at: chrono::Utc::now(),
                frontier: ProtectionFrontier::default(),
            })));
        }

        let updated_at: DateTime<Utc> = sqlx::query_scalar(
            r"
            INSERT INTO vala.oracle_table_protections
                (data_tenant_id, table_uid, node_id, fencing_token, catalog_name,
                 namespace_name, table_name, revision, frontier_encoding_version,
                 frontier_digest, updated_at)
            VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, $8, $9,
                    statement_timestamp())
            ON CONFLICT (data_tenant_id, table_uid, node_id, fencing_token)
            DO UPDATE SET revision = EXCLUDED.revision,
                          frontier_encoding_version = EXCLUDED.frontier_encoding_version,
                          frontier_digest = EXCLUDED.frontier_digest,
                          updated_at = EXCLUDED.updated_at
            RETURNING updated_at
            ",
        )
        .bind(identity.table_uid.as_slice())
        .bind(node_id)
        .bind(fencing_token)
        .bind(&identity.catalog_name)
        .bind(&identity.namespace_name)
        .bind(&identity.table_name)
        .bind(next_revision)
        .bind(FRONTIER_ENCODING_VERSION)
        .bind(frontier.digest(identity).as_slice())
        .fetch_one(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;

        sqlx::query(
            r"
            DELETE FROM vala.oracle_table_protection_members
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1 AND node_id = $2 AND fencing_token = $3
               AND protected_snapshot_id <> ALL($4::bigint[])
            ",
        )
        .bind(identity.table_uid.as_slice())
        .bind(node_id)
        .bind(fencing_token)
        .bind(
            frontier
                .members
                .iter()
                .map(|m| m.protected_snapshot_id)
                .collect::<Vec<i64>>(),
        )
        .execute(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;

        for member in &frontier.members {
            sqlx::query(
                r"
                INSERT INTO vala.oracle_table_protection_members
                    (data_tenant_id, table_uid, node_id, fencing_token,
                     protected_snapshot_id, protected_snapshot_timestamp_ms,
                     retained_head_snapshot_id, retained_head_timestamp_ms,
                     ancestry_path, ancestry_digest_version, ancestry_digest)
                VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (data_tenant_id, table_uid, node_id, fencing_token,
                             protected_snapshot_id)
                DO UPDATE SET
                    protected_snapshot_timestamp_ms = EXCLUDED.protected_snapshot_timestamp_ms,
                    retained_head_snapshot_id = EXCLUDED.retained_head_snapshot_id,
                    retained_head_timestamp_ms = EXCLUDED.retained_head_timestamp_ms,
                    ancestry_path = EXCLUDED.ancestry_path,
                    ancestry_digest_version = EXCLUDED.ancestry_digest_version,
                    ancestry_digest = EXCLUDED.ancestry_digest
                ",
            )
            .bind(identity.table_uid.as_slice())
            .bind(node_id)
            .bind(fencing_token)
            .bind(member.protected_snapshot_id)
            .bind(member.protected_snapshot_timestamp_ms)
            .bind(member.retained_head_snapshot_id)
            .bind(member.retained_head_timestamp_ms)
            .bind(&member.ancestry_path)
            .bind(ANCESTRY_DIGEST_VERSION)
            .bind(member.ancestry_digest.as_slice())
            .execute(&mut **self.conn.transaction())
            .await
            .map_err(SqlError::from)?;
        }

        Ok(ProtectionCas::Committed(Box::new(ProtectionRecord {
            revision: next_revision,
            frontier_encoding_version: FRONTIER_ENCODING_VERSION,
            frontier_digest: frontier.digest(identity),
            updated_at,
            frontier: frontier.clone(),
        })))
    }

    /// Reads the stored identity and revision behind one protection key.
    ///
    /// Recovery discovers a dead epoch's protections by key alone, so it needs
    /// the checked identity payload before it can lock that table's maintenance
    /// authority row. This deliberately takes no lock: reading it first is what
    /// keeps the canonical authority-row-then-protection order intact.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the stored identity
    /// payload is not a valid Bifrost table identity, and [`SqlError`] when the
    /// statement fails.
    pub async fn read_key_identity(
        &mut self,
        key: &ProtectionKey,
    ) -> Result<Option<(TableAuthorityIdentity, i64)>, SqlError> {
        let row: Option<(String, String, String, i64)> = sqlx::query_as(
            r"
            SELECT catalog_name, namespace_name, table_name, revision
              FROM vala.oracle_table_protections
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
               AND node_id = $2
               AND fencing_token = $3
            ",
        )
        .bind(key.table_uid.as_slice())
        .bind(key.node_id)
        .bind(key.fencing_token)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let Some((catalog_name, namespace_name, table_name, revision)) = row else {
            return Ok(None);
        };
        let identity = TableAuthorityIdentity {
            tenant: key.tenant,
            table_uid: key.table_uid,
            catalog_name,
            namespace_name,
            table_name,
        };
        identity.validate(BIFROST_CATALOG_NAME)?;
        Ok(Some((identity, revision)))
    }

    /// Lists every validated protection any epoch holds for one table.
    ///
    /// Forge consumes this as protection regardless of epoch state or heartbeat
    /// freshness: a header that still exists protects, and only the audited
    /// invalidation-and-release sequence removes one.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when any stored header or
    /// member fails validation, and [`SqlError`] when a statement fails.
    pub async fn list_table_protection(
        &mut self,
        identity: &TableAuthorityIdentity,
    ) -> Result<Vec<ProtectionRecord>, SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        let epochs: Vec<(uuid::Uuid, i64)> = sqlx::query_as(
            r"
            SELECT node_id, fencing_token
              FROM vala.oracle_table_protections
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
             ORDER BY node_id, fencing_token
            ",
        )
        .bind(identity.table_uid.as_slice())
        .fetch_all(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let mut records = Vec::with_capacity(epochs.len());
        for (node_id, fencing_token) in epochs {
            let record = self
                .read(identity, node_id, fencing_token)
                .await?
                .ok_or_else(|| {
                    invariant("reader protection header disappeared inside one transaction")
                })?;
            records.push(record);
        }
        Ok(records)
    }
}

/// Orders decoded members by digest so a read reproduces the written order.
pub(crate) fn sorted_members(mut members: Vec<ProtectionMember>) -> Vec<ProtectionMember> {
    members.sort_by_key(|member| member.ancestry_digest);
    members
}

/// Raw protection header projection awaiting validation.
#[derive(sqlx::FromRow)]
pub(crate) struct HeaderDbRow {
    /// Persisted catalog payload, checked against the caller's identity.
    pub(crate) catalog_name: String,
    /// Persisted namespace payload, checked against the caller's identity.
    pub(crate) namespace_name: String,
    /// Persisted table-name payload, checked against the caller's identity.
    pub(crate) table_name: String,
    /// Monotonic table-local revision.
    pub(crate) revision: i64,
    /// Stored frontier encoding version.
    pub(crate) frontier_encoding_version: i32,
    /// Stored header digest bytes awaiting length validation.
    pub(crate) frontier_digest: Vec<u8>,
    /// Database time of the commit that wrote this revision.
    pub(crate) updated_at: DateTime<Utc>,
}

/// Raw protection member projection awaiting validation.
#[derive(sqlx::FromRow)]
pub(crate) struct MemberDbRow {
    /// Oldest active cut on this chain.
    pub(crate) protected_snapshot_id: i64,
    /// Iceberg timestamp of the protected snapshot.
    pub(crate) protected_snapshot_timestamp_ms: i64,
    /// Newest active cut on this chain.
    pub(crate) retained_head_snapshot_id: i64,
    /// Iceberg timestamp of the retained head.
    pub(crate) retained_head_timestamp_ms: i64,
    /// Inclusive newest-to-oldest parent walk.
    pub(crate) ancestry_path: Vec<i64>,
    /// Stored ancestry digest version.
    pub(crate) ancestry_digest_version: i32,
    /// Stored ancestry digest bytes awaiting length validation.
    pub(crate) ancestry_digest: Vec<u8>,
}

/// Enumerates the exact protection keys one dead epoch still holds.
///
/// This is the only cross-tenant capability reader recovery has, and it is
/// read-only by construction: it returns identity keys so recovery can open one
/// actual-tenant transaction per key. It cannot mutate tenant state or append
/// tenant audit.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] for a malformed stored table UID,
/// and [`SqlError`] when the statement fails.
pub async fn enumerate_epoch_protection_keys_for_operator(
    op: &OperatorPool,
    node_id: uuid::Uuid,
    fencing_token: i64,
) -> Result<Vec<ProtectionKey>, SqlError> {
    let rows: Vec<(uuid::Uuid, Vec<u8>)> = sqlx::query_as(
        r"
        SELECT data_tenant_id, table_uid
          FROM vala.oracle_table_protections
         WHERE node_id = $1 AND fencing_token = $2
         ORDER BY data_tenant_id, table_uid
        ",
    )
    .bind(node_id)
    .bind(fencing_token)
    .fetch_all(op.pool())
    .await
    .map_err(SqlError::from)?;
    rows.into_iter()
        .map(|(tenant, uid)| {
            Ok(ProtectionKey {
                tenant: DataTenantId::try_from(tenant)
                    .map_err(|_| invariant("stored protection names an invalid tenant"))?,
                table_uid: table_uid(uid)?,
                node_id,
                fencing_token,
            })
        })
        .collect()
}

/// Lists every epoch Postgres itself proves is past its lease.
///
/// Expiry is evaluated by the database in the same statement that reads the
/// row, so no caller ever compares its own clock against a stored timestamp.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] for an unknown stored state, and
/// [`SqlError`] when the statement fails.
pub async fn list_expired_epochs_for_operator(
    op: &OperatorPool,
    limit: i64,
) -> Result<Vec<OracleEpochRow>, SqlError> {
    if limit <= 0 {
        return Err(invariant(
            "expired Oracle epoch scan bound must be positive",
        ));
    }
    let rows: Vec<EpochDbRow> = sqlx::query_as(
        r"
        SELECT node_id, fencing_token, state, state_revision, acquired_at,
               activated_at, renewed_at, lease_expires_at, invalidated_at
          FROM vala.oracle_reader_epochs
         WHERE lease_expires_at <= statement_timestamp()
         ORDER BY lease_expires_at, node_id, fencing_token
         LIMIT $1
        ",
    )
    .bind(limit)
    .fetch_all(op.pool())
    .await
    .map_err(SqlError::from)?;
    rows.into_iter().map(TryInto::try_into).collect()
}
