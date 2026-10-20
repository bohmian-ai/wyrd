//! Tenant-scoped browser-session queries.
//!
//! A row is keyed by the SHA-256 of a random session identifier that exists
//! only in the browser's HttpOnly cookie. [`insert_browser_session`] records a
//! session once a provider sign-in or recovery key verified the principal,
//! [`live_browser_session`] finds an unexpired, unrevoked session whose
//! recovery key (if any) is still valid, and [`revoke_browser_session`] ends
//! one session at logout. Forced RLS is the only tenant selection, and
//! `PostgreSQL` owns every time: callers bind the lifetime, never an instant.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wyrd_spec::auth::Sha256Hex;

use crate::TenantConn;
use crate::row_types::auth::HumanConnectionBinding;

/// Drop this tenant's sessions past their absolute expiry.
const PURGE_EXPIRED_BROWSER_SESSIONS_SQL: &str = r"
    DELETE FROM wyrd.auth_browser_sessions
     WHERE expires_at <= statement_timestamp()
";

/// Record one session owned by the RLS tenant; `PostgreSQL` derives its
/// absolute expiry from the bound lifetime in seconds.
const INSERT_BROWSER_SESSION_SQL: &str = r"
    INSERT INTO wyrd.auth_browser_sessions (
        session_hash, data_tenant_id, principal_id, connection_id,
        connection_revision, api_key_id, expires_at
    ) VALUES ($1, $2, $3, $4, $5, $6, statement_timestamp() + ($7 * interval '1 second'))
    RETURNING expires_at
";

/// The unexpired, unrevoked session with this hash whose recovery key, when
/// it has one, is itself unrevoked and unexpired.
const LIVE_BROWSER_SESSION_SQL: &str = r"
    SELECT s.principal_id, s.connection_id, s.connection_revision, s.api_key_id,
           s.expires_at
      FROM wyrd.auth_browser_sessions s
      LEFT JOIN wyrd.auth_api_keys k
        ON k.data_tenant_id = s.data_tenant_id
       AND k.id = s.api_key_id
     WHERE s.session_hash = $1
       AND s.revoked_at IS NULL
       AND s.expires_at > statement_timestamp()
       AND (s.api_key_id IS NULL
            OR (k.id IS NOT NULL
                AND k.principal_id = s.principal_id
                AND k.revoked_at IS NULL
                AND (k.expires_at IS NULL OR k.expires_at > statement_timestamp())))
";

/// Revoke the live session with this hash.
const REVOKE_BROWSER_SESSION_SQL: &str = r"
    UPDATE wyrd.auth_browser_sessions
       SET revoked_at = statement_timestamp()
     WHERE session_hash = $1
       AND revoked_at IS NULL
";

/// The authority a browser session was created through, which every request
/// re-checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserSessionAuthority {
    /// A provider sign-in through this exact human connection revision.
    Connection(HumanConnectionBinding),
    /// A recovery sign-in with this API-key credential.
    ApiKey(Uuid),
}

/// A live browser session as [`live_browser_session`] found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveBrowserSession {
    /// The signed-in principal.
    pub principal_id: Uuid,
    /// The authority the session was created through.
    pub authority: BrowserSessionAuthority,
    /// The session's absolute expiry.
    pub expires_at: DateTime<Utc>,
}

/// Raw row returned by [`LIVE_BROWSER_SESSION_SQL`].
#[derive(sqlx::FromRow)]
struct LiveRow {
    /// The signed-in principal.
    principal_id: Uuid,
    /// The human connection id, for a provider sign-in.
    connection_id: Option<Uuid>,
    /// That connection's revision.
    connection_revision: Option<i64>,
    /// The recovery API key id, for a recovery sign-in.
    api_key_id: Option<Uuid>,
    /// Absolute expiry.
    expires_at: DateTime<Utc>,
}

/// Insert a browser session keyed by `session_hash` for `principal_id`,
/// created through `authority`, whose absolute expiry `PostgreSQL` derives
/// from `ttl`, and return that expiry.
///
/// First purges this tenant's expired sessions. The table refuses a lifetime
/// above twelve hours and a reused session hash.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or the insert.
pub async fn insert_browser_session(
    conn: &mut TenantConn<'_>,
    session_hash: &Sha256Hex,
    principal_id: Uuid,
    authority: BrowserSessionAuthority,
    ttl: Duration,
) -> Result<DateTime<Utc>, sqlx::Error> {
    let tenant = conn.data_tenant_id().as_uuid();
    sqlx::query(PURGE_EXPIRED_BROWSER_SESSIONS_SQL)
        .execute(&mut **conn.transaction())
        .await?;
    let (connection, api_key_id) = match authority {
        BrowserSessionAuthority::Connection(binding) => (Some(binding), None),
        BrowserSessionAuthority::ApiKey(api_key_id) => (None, Some(api_key_id)),
    };
    sqlx::query(INSERT_BROWSER_SESSION_SQL)
        .bind(session_hash.as_bytes().as_slice())
        .bind(tenant)
        .bind(principal_id)
        .bind(connection.map(|binding| binding.connection_id))
        .bind(connection.map(|binding| binding.connection_revision))
        .bind(api_key_id)
        .bind(ttl.as_secs_f64())
        .fetch_one(&mut **conn.transaction())
        .await
        .and_then(|row| sqlx::Row::try_get(&row, "expires_at"))
}

/// This tenant's unexpired, unrevoked session with `session_hash`, when its
/// recovery key (if it has one) still belongs to the session's principal and
/// is unrevoked and unexpired; `None` otherwise.
///
/// Reads only: resolving a session never extends or otherwise writes it.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read, and
/// [`sqlx::Error::Decode`] for a row naming no single authority.
pub async fn live_browser_session(
    conn: &mut TenantConn<'_>,
    session_hash: &Sha256Hex,
) -> Result<Option<LiveBrowserSession>, sqlx::Error> {
    let row = sqlx::query_as::<_, LiveRow>(LIVE_BROWSER_SESSION_SQL)
        .bind(session_hash.as_bytes().as_slice())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(|row| {
        let authority = match (row.connection_id, row.connection_revision, row.api_key_id) {
            (Some(connection_id), Some(connection_revision), None) => {
                BrowserSessionAuthority::Connection(HumanConnectionBinding {
                    connection_id,
                    connection_revision,
                })
            }
            (None, None, Some(api_key_id)) => BrowserSessionAuthority::ApiKey(api_key_id),
            _ => {
                return Err(sqlx::Error::Decode(
                    "browser session authority is corrupt".into(),
                ));
            }
        };
        Ok(LiveBrowserSession {
            principal_id: row.principal_id,
            authority,
            expires_at: row.expires_at,
        })
    })
    .transpose()
}

/// Revoke this tenant's session with `session_hash`, and return whether a
/// live one was found.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn revoke_browser_session(
    conn: &mut TenantConn<'_>,
    session_hash: &Sha256Hex,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(REVOKE_BROWSER_SESSION_SQL)
        .bind(session_hash.as_bytes().as_slice())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(updated.rows_affected() == 1)
}
