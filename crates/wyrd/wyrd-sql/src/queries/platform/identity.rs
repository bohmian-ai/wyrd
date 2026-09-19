//! Query slots for the platform-scope identity plane.
//!
//! Three tables, one story: the single deployment-owned OIDC connection, the
//! short-lived login state a browser round-trip needs, and the durable
//! `(issuer, subject)` pin that maps a federated human onto a platform
//! principal.
//!
//! All of it runs on the BYPASSRLS [`OperatorPool`], because `platform.*` sits
//! outside the row-level-security tenant boundary. The `wyrd_app` role holds no
//! privilege here at all, which is what keeps "no tenant-plane path confers
//! platform authority" a property of the database rather than of server code.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::types::Uuid;

use crate::{OperatorPool, SqlError, TenantConn};

/// The deployment's one platform-scope OIDC connection.
///
/// Column-for-column the tenant `wyrd.auth_trusted_issuers` shape minus the
/// tenant, the group mapping, and the default roles: a platform principal's
/// authority comes from its grant, never from an issuer-supplied group, so
/// there is deliberately nowhere for a provider to assert a role.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformOidcConnectionRow {
    /// Issuer URL, as configured.
    pub issuer_url: String,
    /// Resolved JWKS endpoint.
    pub jwks_uri: String,
    /// Audience this deployment expects in an ID token.
    pub expected_audience: String,
    /// Wyrd's OAuth 2.0 client identifier at the provider.
    pub client_id: String,
    /// Client authentication discriminant.
    pub client_auth: String,
    /// Lossless claim mapping payload.
    pub claim_mapping: Value,
    /// JWKS cache TTL in seconds.
    pub jwks_ttl_secs: i64,
    /// Sealed client secret: a 12-byte AES-GCM nonce prepended to ciphertext.
    pub client_secret_enc: Option<Vec<u8>>,
}

/// A platform principal's federated identity, pinned or awaiting first login.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformIdentityRow {
    /// Platform principal this identity resolves to.
    pub principal_id: Uuid,
    /// Issuer the identity was registered against.
    pub issuer: String,
    /// Claim value matched on first login only.
    pub match_claim: String,
    /// Pinned subject; `None` until the first successful login.
    pub subject: Option<String>,
}

/// Read the platform OIDC connection, when one is configured.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn platform_oidc_connection(
    pool: &OperatorPool,
) -> Result<Option<PlatformOidcConnectionRow>, SqlError> {
    sqlx::query_as::<_, PlatformOidcConnectionRow>(
        "SELECT issuer_url, jwks_uri, expected_audience, client_id, client_auth,
                claim_mapping, jwks_ttl_secs, client_secret_enc
         FROM platform.oidc_connection",
    )
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Install or replace the platform OIDC connection.
///
/// An upsert on the singleton key rather than a delete-and-insert, so two
/// operators configuring the connection at once converge on one row instead of
/// racing to leave the plane unconfigured.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write fails, including when
/// `client_auth` is not a known discriminant.
// justification: this mirrors one table's column list on a single-row upsert;
// grouping the columns into a struct here would add a type whose only purpose is
// to be destructured immediately on the other side of the call.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_platform_oidc_connection(
    pool: &OperatorPool,
    issuer_url: &str,
    jwks_uri: &str,
    expected_audience: &str,
    client_id: &str,
    client_auth: &str,
    claim_mapping: &Value,
    jwks_ttl_secs: i64,
    client_secret_enc: Option<&[u8]>,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.oidc_connection
             (singleton, issuer_url, jwks_uri, expected_audience, client_id,
              client_auth, claim_mapping, jwks_ttl_secs, client_secret_enc)
         VALUES (TRUE, $1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (singleton) DO UPDATE SET
             issuer_url        = EXCLUDED.issuer_url,
             jwks_uri          = EXCLUDED.jwks_uri,
             expected_audience = EXCLUDED.expected_audience,
             client_id         = EXCLUDED.client_id,
             client_auth       = EXCLUDED.client_auth,
             claim_mapping     = EXCLUDED.claim_mapping,
             jwks_ttl_secs     = EXCLUDED.jwks_ttl_secs,
             client_secret_enc = EXCLUDED.client_secret_enc,
             updated_at        = now()",
    )
    .bind(issuer_url)
    .bind(jwks_uri)
    .bind(expected_audience)
    .bind(client_id)
    .bind(client_auth)
    .bind(claim_mapping)
    .bind(jwks_ttl_secs)
    .bind(client_secret_enc)
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// Remove the platform OIDC connection.
///
/// Returns whether a connection was present. Removal never touches platform
/// principals or their grants: federated login is an additional way in, and
/// taking it away must leave the global credential working.
///
/// # Errors
/// Returns [`SqlError::Query`] when the delete fails.
pub async fn delete_platform_oidc_connection(pool: &OperatorPool) -> Result<bool, SqlError> {
    sqlx::query("DELETE FROM platform.oidc_connection")
        .execute(pool.pool())
        .await
        .map(|done| done.rows_affected() > 0)
        .map_err(SqlError::from)
}

/// Persist one single-use login state row.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails, including on a repeated
/// state key.
pub async fn insert_platform_login_state(
    pool: &OperatorPool,
    state: &str,
    code_verifier: &str,
    nonce: &str,
    issuer: &str,
    redirect_uri: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.login_state
             (state, code_verifier, nonce, issuer, redirect_uri, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(state)
    .bind(code_verifier)
    .bind(nonce)
    .bind(issuer)
    .bind(redirect_uri)
    .bind(expires_at)
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// One login-state row as stored.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformLoginStateRow {
    /// PKCE verifier generated at login initiation.
    pub code_verifier: String,
    /// Nonce the returned ID token must carry.
    pub nonce: String,
    /// Issuer the login was initiated against.
    pub issuer: String,
    /// Redirect URI the authorization code was bound to.
    pub redirect_uri: String,
    /// When this row stops being usable.
    pub expires_at: DateTime<Utc>,
}

/// Consume a login state row exactly once.
///
/// Deletion and read are one statement, so a replayed callback finds nothing
/// rather than racing a second consumer: the second caller deletes zero rows.
/// An expired row is still deleted, then reported absent — the same outcome a
/// forged state key gets, so a caller learns nothing from the difference.
///
/// # Errors
/// Returns [`SqlError::Query`] when the statement fails.
pub async fn take_platform_login_state(
    pool: &OperatorPool,
    state: &str,
) -> Result<Option<PlatformLoginStateRow>, SqlError> {
    let row = sqlx::query_as::<_, PlatformLoginStateRow>(
        "DELETE FROM platform.login_state
         WHERE state = $1
         RETURNING code_verifier, nonce, issuer, redirect_uri, expires_at",
    )
    .bind(state)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)?;

    Ok(row.filter(|row| row.expires_at > Utc::now()))
}

/// Delete every login-state row that has expired.
///
/// Consumption already discards an expired row, so this only stops abandoned
/// logins accumulating; it is never what makes an expired state unusable.
///
/// # Errors
/// Returns [`SqlError::Query`] when the delete fails.
pub async fn purge_expired_platform_login_state(pool: &OperatorPool) -> Result<u64, SqlError> {
    sqlx::query("DELETE FROM platform.login_state WHERE expires_at <= now()")
        .execute(pool.pool())
        .await
        .map(|done| done.rows_affected())
        .map_err(SqlError::from)
}

/// Pre-register a platform principal's expected federated identity.
///
/// Writes the claim to match on first login with no subject pinned yet. Only a
/// caller already holding platform authority reaches this, which is what stops
/// a login from creating platform authority.
///
/// # Errors
/// Returns [`SqlError::UniqueViolation`] when the principal already has an
/// identity or the claim is already registered against this issuer, and
/// [`SqlError::Query`] when the insert otherwise fails.
pub async fn insert_platform_identity_tx(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    issuer: &str,
    match_claim: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.principal_identities (principal_id, issuer, match_claim)
         VALUES ($1, $2, $3)",
    )
    .bind(principal_id)
    .bind(issuer)
    .bind(match_claim)
    .execute(&mut **conn.transaction())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// Resolve a platform principal from an already-pinned federated subject.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn platform_identity_by_subject(
    pool: &OperatorPool,
    issuer: &str,
    subject: &str,
) -> Result<Option<PlatformIdentityRow>, SqlError> {
    sqlx::query_as::<_, PlatformIdentityRow>(
        "SELECT principal_id, issuer, match_claim, subject
         FROM platform.principal_identities
         WHERE issuer = $1 AND subject = $2",
    )
    .bind(issuer)
    .bind(subject)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Pin a subject onto an unpinned pre-registration, matching on the claim.
///
/// The `subject IS NULL` predicate is the whole safety property: it makes
/// pinning a one-time transition that a second login cannot repeat and cannot
/// redirect at a different subject. Returns the principal when this call is the
/// one that pinned it, and `None` when no unpinned registration matched.
///
/// # Errors
/// Returns [`SqlError::UniqueViolation`] when the subject is already pinned to
/// a different principal, and [`SqlError::Query`] when the update fails.
pub async fn pin_platform_identity(
    pool: &OperatorPool,
    issuer: &str,
    match_claim: &str,
    subject: &str,
) -> Result<Option<Uuid>, SqlError> {
    sqlx::query_scalar::<_, Uuid>(
        "UPDATE platform.principal_identities
         SET subject = $3, pinned_at = now()
         WHERE issuer = $1 AND match_claim = $2 AND subject IS NULL
         RETURNING principal_id",
    )
    .bind(issuer)
    .bind(match_claim)
    .bind(subject)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}
