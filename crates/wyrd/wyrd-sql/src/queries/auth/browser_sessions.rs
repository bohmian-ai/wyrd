//! Tenant-scoped queries for `wyrd.auth_browser_sessions`, the server-owned
//! record behind a production UI browser session.
//!
//! Every statement runs on the caller's RLS [`TenantConn`]; forced RLS is the
//! only tenant selection. A session is created once ([`insert_browser_session`]),
//! locked for each use ([`lock_browser_session`]) so concurrent BFF replicas
//! serialize renewal on the row, rotated in place when its access token is
//! renewed ([`rotate_browser_session`]), and revoked with every sealed value
//! wiped ([`revoke_browser_session`]). `PostgreSQL` owns every coordination
//! instant: callers bind durations, and producer-owned token expiries are
//! stored as issued.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wyrd_spec::auth::Sha256Hex;

use crate::TenantConn;

/// Drop this tenant's sessions past their absolute expiry.
const PURGE_EXPIRED_BROWSER_SESSIONS_SQL: &str = r#"
    DELETE FROM wyrd.auth_browser_sessions
     WHERE absolute_expires_at <= statement_timestamp()
"#;

/// Create one live session owned by the RLS tenant.
///
/// The absolute expiry is `PostgreSQL`'s clock plus the bound fixed lifetime
/// in seconds (`$13`); the refresh-token expiry (`$10`) is stored separately
/// as issued. The id hash is the primary key, so a reused id inserts nothing.
const INSERT_BROWSER_SESSION_SQL: &str = r#"
    INSERT INTO wyrd.auth_browser_sessions (
        id_hash, data_tenant_id, principal_id, connection_id, mode,
        access_token_sealed, refresh_token_sealed, api_key_sealed,
        access_expires_at, refresh_expires_at, csrf_hash, csrf_token_sealed,
        absolute_expires_at
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
              statement_timestamp() + ($13 * interval '1 second'))
    ON CONFLICT DO NOTHING
    RETURNING absolute_expires_at
"#;

/// Lock one live session for the rest of the caller's transaction.
///
/// Only an unrevoked row before its absolute expiry matches. `access_fresh`
/// reports whether the stored access token outlives `PostgreSQL`'s clock by
/// more than the bound margin in seconds. A concurrent caller blocks on the
/// row lock and, once the holder commits, re-reads the holder's rotated or
/// revoked row.
const LOCK_BROWSER_SESSION_SQL: &str = r#"
    SELECT principal_id, connection_id, mode, access_token_sealed,
           refresh_token_sealed, api_key_sealed, access_expires_at,
           absolute_expires_at, csrf_token_sealed,
           access_expires_at > statement_timestamp() + ($2 * interval '1 second')
               AS access_fresh
      FROM wyrd.auth_browser_sessions
     WHERE id_hash = $1
       AND revoked_at IS NULL
       AND absolute_expires_at > statement_timestamp()
       FOR UPDATE
"#;

/// Store a renewed credential on a live session.
///
/// The refresh pair is replaced only when one is bound (an SSO rotation); an
/// API-key renewal keeps its stored key and has no refresh token.
const ROTATE_BROWSER_SESSION_SQL: &str = r#"
    UPDATE wyrd.auth_browser_sessions
       SET access_token_sealed = $2,
           access_expires_at = $3,
           refresh_token_sealed = COALESCE($4, refresh_token_sealed),
           refresh_expires_at = COALESCE($5, refresh_expires_at)
     WHERE id_hash = $1
       AND revoked_at IS NULL
"#;

/// Revoke a session and wipe every sealed value it carries.
const REVOKE_BROWSER_SESSION_SQL: &str = r#"
    UPDATE wyrd.auth_browser_sessions
       SET revoked_at = statement_timestamp(),
           access_token_sealed = NULL,
           refresh_token_sealed = NULL,
           api_key_sealed = NULL,
           csrf_token_sealed = NULL
     WHERE id_hash = $1
       AND revoked_at IS NULL
"#;

/// How a browser session renews its Wyrd access token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserSessionMode {
    /// A tenant SSO session: rotates the refresh token its login issued.
    OidcRefresh,
    /// An OIDC-off operator session: re-exchanges its stored API key.
    ApiKeyExchange,
}

impl BrowserSessionMode {
    /// The stored `mode` column value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OidcRefresh => "oidc_refresh",
            Self::ApiKeyExchange => "api_key_exchange",
        }
    }

    /// Parse a stored `mode` column value.
    ///
    /// # Errors
    /// Returns [`sqlx::Error::Decode`] for any other value.
    fn parse(value: &str) -> Result<Self, sqlx::Error> {
        match value {
            "oidc_refresh" => Ok(Self::OidcRefresh),
            "api_key_exchange" => Ok(Self::ApiKeyExchange),
            _ => Err(sqlx::Error::Decode(
                "browser session mode is corrupt".into(),
            )),
        }
    }
}

/// One new session as [`insert_browser_session`] writes it.
///
/// Every credential is already a keyring envelope; nothing here is plaintext.
#[derive(Debug, Clone)]
pub struct BrowserSessionWrite {
    /// The principal the session's access tokens name.
    pub principal_id: Uuid,
    /// The human connection an SSO session logged in through.
    pub connection_id: Option<Uuid>,
    /// How the session renews.
    pub mode: BrowserSessionMode,
    /// Sealed current access token.
    pub access_token_sealed: Vec<u8>,
    /// Expiry of that access token, as issued.
    pub access_expires_at: DateTime<Utc>,
    /// Sealed refresh token of an SSO session.
    pub refresh_token_sealed: Option<Vec<u8>>,
    /// Expiry of that refresh token, as issued.
    pub refresh_expires_at: Option<DateTime<Utc>>,
    /// Sealed bootstrap API key of an API-key session.
    pub api_key_sealed: Option<Vec<u8>>,
    /// SHA-256 of the session CSRF token.
    pub csrf_hash: Sha256Hex,
    /// Sealed session CSRF token.
    pub csrf_token_sealed: Vec<u8>,
    /// Fixed lifetime from `PostgreSQL`'s clock at creation after which the
    /// session ends regardless of renewal.
    pub lifetime: Duration,
}

/// A live session locked by [`lock_browser_session`].
#[derive(Debug, Clone)]
pub struct LockedBrowserSession {
    /// The principal the session's access tokens name.
    pub principal_id: Uuid,
    /// The human connection an SSO session logged in through.
    pub connection_id: Option<Uuid>,
    /// How the session renews.
    pub mode: BrowserSessionMode,
    /// Sealed current access token.
    pub access_token_sealed: Vec<u8>,
    /// Sealed refresh token of an SSO session.
    pub refresh_token_sealed: Option<Vec<u8>>,
    /// Sealed bootstrap API key of an API-key session.
    pub api_key_sealed: Option<Vec<u8>>,
    /// Expiry of the current access token.
    pub access_expires_at: DateTime<Utc>,
    /// When the session ends regardless of renewal.
    pub absolute_expires_at: DateTime<Utc>,
    /// Sealed session CSRF token.
    pub csrf_token_sealed: Vec<u8>,
    /// Whether the access token outlives the requested margin.
    pub access_fresh: bool,
}

/// Raw locked row as [`LOCK_BROWSER_SESSION_SQL`] returns it.
#[derive(sqlx::FromRow)]
struct LockedRow {
    /// Principal id.
    principal_id: Uuid,
    /// SSO connection id.
    connection_id: Option<Uuid>,
    /// Stored mode text.
    mode: String,
    /// Sealed access token; non-null on a live row by table constraint.
    access_token_sealed: Option<Vec<u8>>,
    /// Sealed refresh token.
    refresh_token_sealed: Option<Vec<u8>>,
    /// Sealed API key.
    api_key_sealed: Option<Vec<u8>>,
    /// Access-token expiry.
    access_expires_at: DateTime<Utc>,
    /// Absolute expiry.
    absolute_expires_at: DateTime<Utc>,
    /// Sealed CSRF token; non-null on a live row by table constraint.
    csrf_token_sealed: Option<Vec<u8>>,
    /// Freshness against the bound margin.
    access_fresh: bool,
}

/// Create a session keyed by `id_hash` after purging this tenant's sessions
/// past their absolute expiry.
///
/// Returns the stored absolute expiry, or `None` when the id hash is already
/// recorded and nothing was written.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or the insert,
/// including a write that violates the table's mode constraints.
pub async fn insert_browser_session(
    conn: &mut TenantConn<'_>,
    id_hash: &Sha256Hex,
    row: &BrowserSessionWrite,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    let tenant = conn.data_tenant_id().as_uuid();
    sqlx::query(PURGE_EXPIRED_BROWSER_SESSIONS_SQL)
        .execute(&mut **conn.transaction())
        .await?;
    sqlx::query_scalar::<_, DateTime<Utc>>(INSERT_BROWSER_SESSION_SQL)
        .bind(id_hash.as_bytes().as_slice())
        .bind(tenant)
        .bind(row.principal_id)
        .bind(row.connection_id)
        .bind(row.mode.as_str())
        .bind(&row.access_token_sealed)
        .bind(row.refresh_token_sealed.as_deref())
        .bind(row.api_key_sealed.as_deref())
        .bind(row.access_expires_at)
        .bind(row.refresh_expires_at)
        .bind(row.csrf_hash.as_bytes().as_slice())
        .bind(&row.csrf_token_sealed)
        .bind(row.lifetime.as_secs_f64())
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Lock this tenant's live session keyed by `id_hash` until the caller's
/// transaction ends, reporting whether its access token outlives
/// `fresh_margin`.
///
/// Returns `None` for an unknown, revoked, absolutely expired, or other
/// tenant's session.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the statement, and
/// [`sqlx::Error::Decode`] when the stored mode or a live row's credentials
/// are corrupt.
pub async fn lock_browser_session(
    conn: &mut TenantConn<'_>,
    id_hash: &Sha256Hex,
    fresh_margin: Duration,
) -> Result<Option<LockedBrowserSession>, sqlx::Error> {
    let row = sqlx::query_as::<_, LockedRow>(LOCK_BROWSER_SESSION_SQL)
        .bind(id_hash.as_bytes().as_slice())
        .bind(fresh_margin.as_secs_f64())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(|row| {
        let corrupt = || sqlx::Error::Decode("live browser session lacks a credential".into());
        Ok(LockedBrowserSession {
            principal_id: row.principal_id,
            connection_id: row.connection_id,
            mode: BrowserSessionMode::parse(&row.mode)?,
            access_token_sealed: row.access_token_sealed.ok_or_else(corrupt)?,
            refresh_token_sealed: row.refresh_token_sealed,
            api_key_sealed: row.api_key_sealed,
            access_expires_at: row.access_expires_at,
            absolute_expires_at: row.absolute_expires_at,
            csrf_token_sealed: row.csrf_token_sealed.ok_or_else(corrupt)?,
            access_fresh: row.access_fresh,
        })
    })
    .transpose()
}

/// Store a renewed access token, and for an SSO session its rotated refresh
/// token, on this tenant's live session keyed by `id_hash`.
///
/// Returns `false` when no live session matched.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn rotate_browser_session(
    conn: &mut TenantConn<'_>,
    id_hash: &Sha256Hex,
    access_token_sealed: &[u8],
    access_expires_at: DateTime<Utc>,
    refresh: Option<(&[u8], DateTime<Utc>)>,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(ROTATE_BROWSER_SESSION_SQL)
        .bind(id_hash.as_bytes().as_slice())
        .bind(access_token_sealed)
        .bind(access_expires_at)
        .bind(refresh.map(|(sealed, _)| sealed))
        .bind(refresh.map(|(_, expires_at)| expires_at))
        .execute(&mut **conn.transaction())
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Revoke this tenant's session keyed by `id_hash` and wipe its sealed
/// credentials.
///
/// Returns `false` when the session is unknown or already revoked.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn revoke_browser_session(
    conn: &mut TenantConn<'_>,
    id_hash: &Sha256Hex,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(REVOKE_BROWSER_SESSION_SQL)
        .bind(id_hash.as_bytes().as_slice())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(updated.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::BrowserSessionMode;

    /// Each mode round-trips through its stored text, and unknown text is
    /// refused as corrupt.
    #[test]
    fn browser_session_mode_round_trips_and_refuses_unknown_text() {
        for mode in [
            BrowserSessionMode::OidcRefresh,
            BrowserSessionMode::ApiKeyExchange,
        ] {
            assert_eq!(
                BrowserSessionMode::parse(mode.as_str()).expect("mode parses"),
                mode
            );
        }
        assert!(BrowserSessionMode::parse("password").is_err());
    }
}
