//! Query slots for `wyrd.auth_human_connections`.
//!
//! Tenant functions take `&mut TenantConn<'_>`: RLS scopes every statement to
//! the bound tenant, so no statement names a tenant predicate. Mutations are
//! expected to run after [`lock_human_connection_slot`] in the same
//! transaction, which serializes every lifecycle change for one tenant across
//! all replicas while the partial unique indexes remain the final guard for
//! "one Active, one Candidate".
//!
//! The two sealed-secret rewrap functions are the only cross-tenant slots: they
//! run on the BYPASSRLS [`OperatorPool`] because sealing-key rotation is a
//! deployment operation over every tenant's ciphertext, and each update is a
//! compare-and-swap on the exact bytes read so a concurrent rotation cannot be
//! overwritten. Besides this table's client secrets they cover the workload
//! issuer secrets and every sealed column of live browser sessions, so one
//! inventory answers for every long-lived tenant ciphertext.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use serde_json::Value;
use sqlx::types::Uuid;

use crate::row_types::auth::HumanConnectionRow;
use crate::{OperatorPool, SqlError, TenantConn};

/// Serializes every lifecycle mutation for the bound tenant; see
/// [`lock_human_connection_slot`].
const LOCK_SLOT_SQL: &str = "SELECT pg_advisory_xact_lock(hashtextextended(\
     'wyrd.auth_human_connections:' || wyrd.current_tenant()::text, 0))";

/// Reads the tenant's live connections, newest revision first.
const LIVE_SQL: &str = r#"
    SELECT connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
      FROM wyrd.auth_human_connections
     WHERE removed_at IS NULL
     ORDER BY revision DESC
"#;

/// Reads the tenant's one live connection in a state.
const IN_STATE_SQL: &str = r#"
    SELECT connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
      FROM wyrd.auth_human_connections
     WHERE state = $1 AND removed_at IS NULL
"#;

/// Inserts a candidate at one more than the tenant's highest revision ever.
const INSERT_CANDIDATE_SQL: &str = r#"
    INSERT INTO wyrd.auth_human_connections (
        connection_id, data_tenant_id, revision, state, issuer_url, client_id,
        client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs)
    VALUES ($1, $2,
            (SELECT COALESCE(max(revision), 0) + 1 FROM wyrd.auth_human_connections),
            'Candidate', $3, $4, $5, $6, $7, $8, $9)
    RETURNING connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
"#;

/// Replaces the candidate in place at the next revision, clearing its test.
const REPLACE_CANDIDATE_SQL: &str = r#"
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
"#;

/// Stamps the exact candidate revision tested until a database-clock deadline.
const STAMP_TESTED_SQL: &str = r#"
    UPDATE wyrd.auth_human_connections
       SET jwks_uri = $3, tested_revision = revision,
           tested_until = statement_timestamp() + make_interval(secs => $4),
           updated_at = statement_timestamp()
     WHERE connection_id = $1 AND state = 'Candidate' AND revision = $2
    RETURNING connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
"#;

/// Retires the Active connection to Inactive.
const DEACTIVATE_ACTIVE_SQL: &str = r#"
    UPDATE wyrd.auth_human_connections
       SET state = 'Inactive', updated_at = statement_timestamp()
     WHERE state = 'Active'
    RETURNING connection_id
"#;

/// Promotes the candidate at a revision whose test stamp is still current.
const PROMOTE_TESTED_SQL: &str = r#"
    UPDATE wyrd.auth_human_connections
       SET state = 'Active', updated_at = statement_timestamp()
     WHERE state = 'Candidate' AND revision = $1
       AND tested_revision = revision
       AND tested_until > statement_timestamp()
    RETURNING connection_id, data_tenant_id, revision, state, issuer_url, client_id,
           client_auth, client_secret_enc, claim_mapping, group_role_map, jwks_ttl_secs,
           jwks_uri, tested_revision, tested_until, removed_at, created_at, updated_at
"#;

/// Reports whether the candidate at a revision carries an unexpired stamp.
const TEST_IS_CURRENT_SQL: &str = r#"
    SELECT EXISTS (
        SELECT 1 FROM wyrd.auth_human_connections
         WHERE state = 'Candidate' AND revision = $1
           AND tested_revision = revision
           AND tested_until > statement_timestamp())
"#;

/// Reports whether one exact connection revision is the tenant's Active one.
const IS_ACTIVE_SQL: &str = r#"
    SELECT EXISTS (
        SELECT 1 FROM wyrd.auth_human_connections
         WHERE connection_id = $1 AND revision = $2
           AND state = 'Active' AND removed_at IS NULL)
"#;

/// Tombstones one live connection and wipes its secret.
const REMOVE_SQL: &str = r#"
    UPDATE wyrd.auth_human_connections
       SET state = 'Inactive', client_secret_enc = NULL,
           removed_at = statement_timestamp(), updated_at = statement_timestamp()
     WHERE connection_id = $1 AND removed_at IS NULL
"#;

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

/// One sealed secret column value, addressed for a compare-and-swap rewrap.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SealedSecretRow {
    /// Owning tenant, used only for operator diagnostics.
    pub data_tenant_id: Uuid,
    /// Row key as text: the connection id, the trusted issuer URL, or the
    /// lowercase-hex browser-session id hash.
    pub row_key: String,
    /// The sealed bytes of the table's sealed column exactly as stored.
    pub client_secret_enc: Vec<u8>,
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

/// Insert a new candidate at the next tenant revision.
///
/// # Errors
/// Returns [`SqlError`] when the insert fails, including a unique violation
/// when a candidate already exists.
pub async fn insert_human_candidate(
    conn: &mut TenantConn<'_>,
    write: &HumanConnectionWrite,
) -> Result<HumanConnectionRow, SqlError> {
    sqlx::query_as::<_, HumanConnectionRow>(INSERT_CANDIDATE_SQL)
        .bind(Uuid::now_v7())
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

/// List every sealed value of one [`SealedSecretTable`] column across all
/// tenants, for sealing-key rewrap.
///
/// Every column lists every stored (non-null) envelope, regardless of the
/// row's lifecycle. For browser sessions that includes rows past their
/// absolute expiry: expiry makes a session unreachable but writes nothing, and
/// its ciphertext stays stored until the tenant's next session insertion
/// purges the row, so it must still be counted and resealed. Revoked rows need
/// no filter because revocation wipes every sealed value and the table
/// constraint keeps them null.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
// tenant-isolation: cross-tenant OperatorPool
pub async fn sealed_tenant_secrets(
    operator: &OperatorPool,
    table: SealedSecretTable,
) -> Result<Vec<SealedSecretRow>, SqlError> {
    sqlx::query_as::<_, SealedSecretRow>(table.select_sql())
        .fetch_all(operator.pool())
        .await
        .map_err(SqlError::from)
}

/// Replace one sealed secret if it still holds exactly `previous`.
///
/// Returns `false` when the row changed or disappeared since it was read; the
/// rewrap owner then leaves it for the next pass.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
// tenant-isolation: cross-tenant OperatorPool
pub async fn swap_sealed_tenant_secret(
    operator: &OperatorPool,
    table: SealedSecretTable,
    row: &SealedSecretRow,
    rewrapped: &[u8],
) -> Result<bool, SqlError> {
    sqlx::query(table.swap_sql())
        .bind(row.data_tenant_id)
        .bind(&row.row_key)
        .bind(&row.client_secret_enc)
        .bind(rewrapped)
        .execute(operator.pool())
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(SqlError::from)
}

/// A tenant table column that holds keyring envelopes, as rewrap walks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealedSecretTable {
    /// `wyrd.auth_human_connections.client_secret_enc`, keyed by connection id.
    HumanConnections,
    /// `wyrd.auth_trusted_issuers.client_secret_enc`, keyed by issuer URL.
    TrustedIssuers,
    /// `wyrd.auth_browser_sessions.access_token_sealed`, keyed by id hash.
    BrowserSessionAccess,
    /// `wyrd.auth_browser_sessions.refresh_token_sealed` (SSO sessions).
    BrowserSessionRefresh,
    /// `wyrd.auth_browser_sessions.api_key_sealed` (API-key sessions).
    BrowserSessionApiKey,
    /// `wyrd.auth_browser_sessions.csrf_token_sealed`.
    BrowserSessionCsrf,
}

impl SealedSecretTable {
    /// Every column rewrap walks, in a stable order.
    pub const ALL: [Self; 6] = [
        Self::HumanConnections,
        Self::TrustedIssuers,
        Self::BrowserSessionAccess,
        Self::BrowserSessionRefresh,
        Self::BrowserSessionApiKey,
        Self::BrowserSessionCsrf,
    ];

    /// Stable label for operator logs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::HumanConnections => "wyrd.auth_human_connections",
            Self::TrustedIssuers => "wyrd.auth_trusted_issuers",
            Self::BrowserSessionAccess => "wyrd.auth_browser_sessions.access_token_sealed",
            Self::BrowserSessionRefresh => "wyrd.auth_browser_sessions.refresh_token_sealed",
            Self::BrowserSessionApiKey => "wyrd.auth_browser_sessions.api_key_sealed",
            Self::BrowserSessionCsrf => "wyrd.auth_browser_sessions.csrf_token_sealed",
        }
    }

    /// The operator statement listing every stored (non-null) sealed value in
    /// this column, with no liveness or expiry filter, so the rewrap report
    /// and keyless boot decision cover all ciphertext the table holds.
    fn select_sql(self) -> &'static str {
        match self {
            Self::HumanConnections => {
                "SELECT data_tenant_id, connection_id::text AS row_key, client_secret_enc
                   FROM wyrd.auth_human_connections WHERE client_secret_enc IS NOT NULL"
            }
            Self::TrustedIssuers => {
                "SELECT data_tenant_id, issuer_url AS row_key, client_secret_enc
                   FROM wyrd.auth_trusted_issuers WHERE client_secret_enc IS NOT NULL"
            }
            Self::BrowserSessionAccess => {
                "SELECT data_tenant_id, encode(id_hash, 'hex') AS row_key,
                        access_token_sealed AS client_secret_enc
                   FROM wyrd.auth_browser_sessions
                  WHERE access_token_sealed IS NOT NULL"
            }
            Self::BrowserSessionRefresh => {
                "SELECT data_tenant_id, encode(id_hash, 'hex') AS row_key,
                        refresh_token_sealed AS client_secret_enc
                   FROM wyrd.auth_browser_sessions
                  WHERE refresh_token_sealed IS NOT NULL"
            }
            Self::BrowserSessionApiKey => {
                "SELECT data_tenant_id, encode(id_hash, 'hex') AS row_key,
                        api_key_sealed AS client_secret_enc
                   FROM wyrd.auth_browser_sessions
                  WHERE api_key_sealed IS NOT NULL"
            }
            Self::BrowserSessionCsrf => {
                "SELECT data_tenant_id, encode(id_hash, 'hex') AS row_key,
                        csrf_token_sealed AS client_secret_enc
                   FROM wyrd.auth_browser_sessions
                  WHERE csrf_token_sealed IS NOT NULL"
            }
        }
    }

    /// The operator compare-and-swap statement replacing one sealed value; it
    /// matches only while the column still holds exactly the bytes read.
    fn swap_sql(self) -> &'static str {
        match self {
            Self::HumanConnections => {
                "UPDATE wyrd.auth_human_connections SET client_secret_enc = $4
                  WHERE data_tenant_id = $1 AND connection_id::text = $2
                    AND client_secret_enc = $3"
            }
            Self::TrustedIssuers => {
                "UPDATE wyrd.auth_trusted_issuers SET client_secret_enc = $4
                  WHERE data_tenant_id = $1 AND issuer_url = $2 AND client_secret_enc = $3"
            }
            Self::BrowserSessionAccess => {
                "UPDATE wyrd.auth_browser_sessions SET access_token_sealed = $4
                  WHERE data_tenant_id = $1 AND id_hash = decode($2, 'hex')
                    AND access_token_sealed = $3"
            }
            Self::BrowserSessionRefresh => {
                "UPDATE wyrd.auth_browser_sessions SET refresh_token_sealed = $4
                  WHERE data_tenant_id = $1 AND id_hash = decode($2, 'hex')
                    AND refresh_token_sealed = $3"
            }
            Self::BrowserSessionApiKey => {
                "UPDATE wyrd.auth_browser_sessions SET api_key_sealed = $4
                  WHERE data_tenant_id = $1 AND id_hash = decode($2, 'hex')
                    AND api_key_sealed = $3"
            }
            Self::BrowserSessionCsrf => {
                "UPDATE wyrd.auth_browser_sessions SET csrf_token_sealed = $4
                  WHERE data_tenant_id = $1 AND id_hash = decode($2, 'hex')
                    AND csrf_token_sealed = $3"
            }
        }
    }
}
