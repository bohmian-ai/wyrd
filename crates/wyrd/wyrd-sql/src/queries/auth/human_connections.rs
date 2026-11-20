//! Query slots for `wyrd.auth_human_connections`.
//!
//! Tenant functions take `&mut TenantConn<'_>`: RLS scopes every statement to
//! the bound tenant, so no statement names a tenant predicate. Mutations are
//! expected to run after [`lock_human_connection_slot`] in the same
//! transaction, which serializes every lifecycle change for one tenant across
//! all replicas while the partial unique indexes remain the final guard for
//! "one Active, one Candidate".
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use serde_json::Value;
use sqlx::types::Uuid;

use crate::row_types::auth::HumanConnectionRow;
use crate::{SqlError, TenantConn};

/// Serializes every lifecycle mutation for the bound tenant; see
/// [`lock_human_connection_slot`].
const LOCK_SLOT_SQL: &str = "SELECT pg_advisory_xact_lock(hashtextextended(\
     'wyrd.auth_human_connections:' || wyrd.current_tenant()::text, 0))";

/// Reads the tenant's live connections, newest revision first.
const LIVE_SQL: &str = r"
    SELECT connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
      FROM wyrd.auth_human_connections
     WHERE removed_at IS NULL
     ORDER BY revision DESC
";

/// Reads the tenant's one live connection in a state.
const IN_STATE_SQL: &str = r"
    SELECT connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
      FROM wyrd.auth_human_connections
     WHERE state = $1 AND removed_at IS NULL
";

/// Inserts a candidate at one more than the tenant's highest revision ever.
const INSERT_CANDIDATE_SQL: &str = r"
    INSERT INTO wyrd.auth_human_connections (
        connection_id, data_tenant_id, revision, state, issuer_url, client_id,
        client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs)
    VALUES ($1, $2,
            (SELECT COALESCE(max(revision), 0) + 1 FROM wyrd.auth_human_connections),
            'Candidate', $3, $4, $5, $6, $7, $8, $9)
    RETURNING connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
";

/// Replaces the candidate in place at the next revision, clearing its test.
const REPLACE_CANDIDATE_SQL: &str = r"
    UPDATE wyrd.auth_human_connections
       SET revision = (SELECT max(revision) + 1 FROM wyrd.auth_human_connections),
           issuer_url = $2, client_id = $3, client_auth = $4, client_secret_enc = $5,
           claim_mapping = $6, group_role_map = $7, jwks_ttl_secs = $8,
           jwks_uri = NULL, tested_revision = NULL, tested_until = NULL,
           updated_at = statement_timestamp()
     WHERE connection_id = $1 AND state = 'Candidate'
    RETURNING connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
";

/// Stamps the exact candidate revision tested until a database-clock deadline.
const STAMP_TESTED_SQL: &str = r"
    UPDATE wyrd.auth_human_connections
       SET jwks_uri = $3, tested_revision = revision,
           tested_until = statement_timestamp() + make_interval(secs => $4),
           updated_at = statement_timestamp()
     WHERE connection_id = $1 AND state = 'Candidate' AND revision = $2
    RETURNING connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
";

/// Retires the Active connection to Inactive.
const DEACTIVATE_ACTIVE_SQL: &str = r"
    UPDATE wyrd.auth_human_connections
       SET state = 'Inactive', updated_at = statement_timestamp()
     WHERE state = 'Active'
    RETURNING connection_id
";

/// Promotes the candidate at a revision whose test stamp is still current.
const PROMOTE_TESTED_SQL: &str = r"
    UPDATE wyrd.auth_human_connections
       SET state = 'Active', updated_at = statement_timestamp()
     WHERE state = 'Candidate' AND revision = $1
       AND tested_revision = revision
       AND tested_until > statement_timestamp()
    RETURNING connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
";

/// Reports whether the candidate at a revision carries an unexpired stamp.
const TEST_IS_CURRENT_SQL: &str = r"
    SELECT EXISTS (
        SELECT 1 FROM wyrd.auth_human_connections
         WHERE state = 'Candidate' AND revision = $1
           AND tested_revision = revision
           AND tested_until > statement_timestamp())
";

/// Reports whether one exact connection revision is the tenant's Active one.
const IS_ACTIVE_SQL: &str = r"
    SELECT EXISTS (
        SELECT 1 FROM wyrd.auth_human_connections
         WHERE connection_id = $1 AND revision = $2
           AND state = 'Active' AND removed_at IS NULL)
";

/// Swaps one live Active or Candidate connection's sealed client secret for
/// its encryption renewal while the row still holds the envelope that was
/// opened; revision, state, and test stamp are unchanged.
const RENEW_SECRET_SQL: &str = r"
    UPDATE wyrd.auth_human_connections
       SET client_secret_enc = $3, updated_at = statement_timestamp()
     WHERE connection_id = $1 AND client_secret_enc = $2
       AND state IN ('Active', 'Candidate') AND removed_at IS NULL
    RETURNING connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
";

/// Tombstones one live connection and wipes its secret.
const REMOVE_SQL: &str = r"
    UPDATE wyrd.auth_human_connections
       SET state = 'Inactive', client_secret_enc = NULL,
           removed_at = statement_timestamp(), updated_at = statement_timestamp()
     WHERE connection_id = $1 AND removed_at IS NULL
";

/// Column values for a new or replaced candidate connection.
///
/// `data_tenant_id` and `revision` are never taken from here: the tenant comes
/// from the [`TenantConn`] and the revision is derived in SQL as one more than
/// the tenant's highest revision ever, so a revision names exactly one staged
/// configuration.
#[derive(Debug, Clone)]
pub struct HumanConnectionWrite {
    /// Normalized issuer URL.
    pub issuer_url: String,
    /// Client id, also the ID-token audience.
    pub client_id: String,
    /// `SecretBasic`, `SecretPost`, or `Public`.
    pub client_auth: String,
    /// Sealed client secret; `None` exactly when `client_auth` is `Public`.
    pub client_secret_enc: Option<Vec<u8>>,
    /// Claim mapping JSON.
    pub claim_mapping: Value,
    /// Provider group to tenant role names JSON.
    pub group_role_map: Value,
    /// JWKS key-cache TTL in seconds.
    pub jwks_ttl_secs: i64,
}

/// Serialize every lifecycle mutation for the bound tenant.
///
/// Takes a transaction-scoped advisory lock keyed by the tenant, so it is
/// released when the caller commits or rolls back. Concurrent mutations on other
/// replicas block here instead of racing the unique indexes.
///
/// # Errors
/// Returns [`SqlError`] when the lock statement fails.
pub async fn lock_human_connection_slot(conn: &mut TenantConn<'_>) -> Result<(), SqlError> {
    sqlx::query(LOCK_SLOT_SQL)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
        .map_err(SqlError::from)
}

/// List the tenant's live (non-removed) connections, newest revision first.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn live_human_connections(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<HumanConnectionRow>, SqlError> {
    sqlx::query_as::<_, HumanConnectionRow>(LIVE_SQL)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Read the tenant's connection in `state` (`Active` or `Candidate`).
///
/// The partial unique indexes guarantee at most one row per state.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn human_connection_in_state(
    conn: &mut TenantConn<'_>,
    state: &str,
) -> Result<Option<HumanConnectionRow>, SqlError> {
    sqlx::query_as::<_, HumanConnectionRow>(IN_STATE_SQL)
        .bind(state)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Insert a new candidate as `connection_id` at the next tenant revision.
///
/// The caller chooses the id because the candidate's client secret is sealed
/// to it before this transaction opens.
///
/// # Errors
/// Returns [`SqlError`] when the insert fails, including a unique violation
/// when a candidate already exists.
pub async fn insert_human_candidate(
    conn: &mut TenantConn<'_>,
    connection_id: Uuid,
    write: &HumanConnectionWrite,
) -> Result<HumanConnectionRow, SqlError> {
    sqlx::query_as::<_, HumanConnectionRow>(INSERT_CANDIDATE_SQL)
        .bind(connection_id)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(&write.issuer_url)
        .bind(&write.client_id)
        .bind(&write.client_auth)
        .bind(write.client_secret_enc.as_deref())
        .bind(&write.claim_mapping)
        .bind(&write.group_role_map)
        .bind(write.jwks_ttl_secs)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Replace the existing candidate in place at the next tenant revision.
///
/// The test stamp and discovered JWKS URI are cleared, so a replaced
/// configuration must be tested again before it can be activated.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
pub async fn replace_human_candidate(
    conn: &mut TenantConn<'_>,
    connection_id: Uuid,
    write: &HumanConnectionWrite,
) -> Result<Option<HumanConnectionRow>, SqlError> {
    sqlx::query_as::<_, HumanConnectionRow>(REPLACE_CANDIDATE_SQL)
        .bind(connection_id)
        .bind(&write.issuer_url)
        .bind(&write.client_id)
        .bind(&write.client_auth)
        .bind(write.client_secret_enc.as_deref())
        .bind(&write.claim_mapping)
        .bind(&write.group_role_map)
        .bind(write.jwks_ttl_secs)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Stamp the exact candidate revision as tested for `validity`, returning the
/// stamped row.
///
/// PostgreSQL derives `tested_until` from `statement_timestamp()`. Returns
/// `None` when the candidate no longer exists at `revision` (it was replaced
/// or activated while the network test ran), in which case nothing changes.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
pub async fn stamp_human_candidate_tested(
    conn: &mut TenantConn<'_>,
    connection_id: Uuid,
    revision: i64,
    jwks_uri: &str,
    validity: Duration,
) -> Result<Option<HumanConnectionRow>, SqlError> {
    sqlx::query_as::<_, HumanConnectionRow>(STAMP_TESTED_SQL)
        .bind(connection_id)
        .bind(revision)
        .bind(jwks_uri)
        .bind(validity.as_secs_f64())
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Retire the current Active connection to Inactive, returning its id.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
pub async fn deactivate_active_human_connection(
    conn: &mut TenantConn<'_>,
) -> Result<Option<Uuid>, SqlError> {
    sqlx::query_scalar(DEACTIVATE_ACTIVE_SQL)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Promote the candidate at `revision` to Active if its test stamp is current.
///
/// The stamp must name this exact revision and `tested_until` must still be in
/// the future by the database clock. Returns `None` when any condition fails.
/// The caller retires the previous Active first in the same transaction.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
pub async fn promote_tested_human_candidate(
    conn: &mut TenantConn<'_>,
    revision: i64,
) -> Result<Option<HumanConnectionRow>, SqlError> {
    sqlx::query_as::<_, HumanConnectionRow>(PROMOTE_TESTED_SQL)
        .bind(revision)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Report whether the candidate at `revision` carries an unexpired test stamp.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn human_candidate_test_is_current(
    conn: &mut TenantConn<'_>,
    revision: i64,
) -> Result<bool, SqlError> {
    sqlx::query_scalar(TEST_IS_CURRENT_SQL)
        .bind(revision)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Report whether the connection `connection_id` at exactly `revision` is the
/// bound tenant's live Active connection.
///
/// Session issuance and refresh rotation call this under
/// [`lock_human_connection_slot`], so a lifecycle change on any replica either
/// commits before the check (and the session is refused) or waits for the
/// issuing transaction to commit.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn human_connection_is_active(
    conn: &mut TenantConn<'_>,
    connection_id: Uuid,
    revision: i64,
) -> Result<bool, SqlError> {
    sqlx::query_scalar(IS_ACTIVE_SQL)
        .bind(connection_id)
        .bind(revision)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Tombstone one live connection: Inactive, secret wiped, `removed_at` set.
///
/// Returns `false` when no live connection has that id in the bound tenant.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
pub async fn remove_human_connection(
    conn: &mut TenantConn<'_>,
    connection_id: Uuid,
) -> Result<bool, SqlError> {
    sqlx::query(REMOVE_SQL)
        .bind(connection_id)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(SqlError::from)
}

/// Replace live connection `connection_id`'s sealed client secret with
/// `renewed`, its encryption renewal, only while it still holds `observed`
/// and is Active or Candidate.
///
/// Returns `None` when a deactivation, removal, replacement, or another
/// renewal committed first; the caller reports a conflict and the winner
/// stands.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
pub async fn renew_human_connection_secret(
    conn: &mut TenantConn<'_>,
    connection_id: Uuid,
    observed: &[u8],
    renewed: &[u8],
) -> Result<Option<HumanConnectionRow>, SqlError> {
    sqlx::query_as::<_, HumanConnectionRow>(RENEW_SECRET_SQL)
        .bind(connection_id)
        .bind(observed)
        .bind(renewed)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}
