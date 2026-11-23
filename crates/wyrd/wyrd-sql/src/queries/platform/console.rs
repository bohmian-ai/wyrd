//! Query slots for platform console sign-in: platform-user passwords and
//! platform browser sessions.
//!
//! A platform `user` principal may hold one Argon2id verifier on its row,
//! with a revision that advances on every change. A platform browser session
//! is the SHA-256 of an opaque cookie secret, anchored to the exact password
//! revision or platform OIDC connection revision it was created through.
//! Every read re-checks the principal and that anchor in the same statement
//! that records activity, so suspension, a password change or a connection
//! replacement ends the session on its next request at every replica.
//! PostgreSQL derives every deadline from bound durations.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

use crate::{OperatorPool, SqlError, TenantConn};

/// A platform principal's password verifier, as sign-in reads it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformPasswordRow {
    /// The principal.
    pub id: Uuid,
    /// The Argon2id PHC verifier.
    pub password_hash: String,
    /// The revision the verifier belongs to.
    pub password_revision: i64,
}

/// A live platform browser session, resolved for one request.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformBrowserSessionRow {
    /// The session.
    pub id: Uuid,
    /// The principal it acts as.
    pub principal_id: Uuid,
    /// The principal's stored kind.
    pub principal_kind: String,
    /// The principal's operator-facing name.
    pub name: String,
    /// Whether the session was created through a platform-user password;
    /// otherwise through the platform OIDC connection.
    pub password: bool,
    /// The absolute deadline; activity never moves it.
    pub expires_at: DateTime<Utc>,
    /// The idle deadline after this request's activity.
    pub idle_expires_at: DateTime<Utc>,
    /// Whether the last fresh human authentication is within the bound
    /// recent-proof window.
    pub recent: bool,
}

/// Replace platform `user` `id`'s password verifier, advancing its
/// revision, on the caller's audited transaction. Returns the new revision,
/// or `None` when no platform `user` has that id.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn set_platform_password_tx(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    password_hash: &str,
) -> Result<Option<i64>, SqlError> {
    sqlx::query_scalar(
        "UPDATE platform.principals
            SET password_hash = $2, password_revision = password_revision + 1,
                updated_at = now()
          WHERE id = $1 AND principal_kind = 'user'
          RETURNING password_revision",
    )
    .bind(id)
    .bind(password_hash)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// The verifier of the active platform `user` named `name`, when it has
/// one.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn platform_password_by_name(
    pool: &OperatorPool,
    name: &str,
) -> Result<Option<PlatformPasswordRow>, SqlError> {
    sqlx::query_as(
        "SELECT id, password_hash, password_revision
           FROM platform.principals
          WHERE name = $1 AND status = 'active' AND password_hash IS NOT NULL",
    )
    .bind(name)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// The verifier of active platform principal `id`, when it has one.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn platform_password_by_id(
    pool: &OperatorPool,
    id: Uuid,
) -> Result<Option<PlatformPasswordRow>, SqlError> {
    sqlx::query_as(
        "SELECT id, password_hash, password_revision
           FROM platform.principals
          WHERE id = $1 AND status = 'active' AND password_hash IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// The anchor a new platform browser session records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSessionAnchor {
    /// A platform-user password at this revision.
    Password(i64),
    /// The platform OIDC connection at this revision.
    Connection(i64),
}

/// The deadlines of a platform browser session, bound as durations.
#[derive(Debug, Clone, Copy)]
pub struct PlatformSessionWindows {
    /// Inactivity after which the session ends.
    pub idle: Duration,
    /// Lifetime after creation after which the session ends.
    pub absolute: Duration,
    /// How long a fresh human authentication counts as recent proof.
    pub recent: Duration,
}

/// Create a browser session for principal `principal_id` under `anchor`
/// when, in the same statement, the principal is active and the anchor is
/// still its current password revision or the current connection revision.
/// Returns the session, or `None` when that check fails.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails.
pub async fn insert_platform_browser_session(
    pool: &OperatorPool,
    id: Uuid,
    session_hash: &[u8],
    principal_id: Uuid,
    anchor: PlatformSessionAnchor,
    windows: PlatformSessionWindows,
) -> Result<Option<PlatformBrowserSessionRow>, SqlError> {
    let (password, connection) = match anchor {
        PlatformSessionAnchor::Password(revision) => (Some(revision), None),
        PlatformSessionAnchor::Connection(revision) => (None, Some(revision)),
    };
    sqlx::query_as(
        "WITH created AS (
            INSERT INTO platform.browser_sessions
                (id, session_hash, principal_id, password_revision, connection_revision,
                 idle_expires_at, expires_at)
            SELECT $1, $2, p.id, $4, $5,
                   statement_timestamp() + LEAST($6, $7) * interval '1 second',
                   statement_timestamp() + $7 * interval '1 second'
              FROM platform.principals p
             WHERE p.id = $3 AND p.status = 'active'
               AND ($4::BIGINT IS NULL
                    OR (p.password_hash IS NOT NULL AND p.password_revision = $4))
               AND ($5::BIGINT IS NULL
                    OR EXISTS (SELECT 1 FROM platform.oidc_connection o WHERE o.revision = $5))
            RETURNING id, principal_id, password_revision, expires_at, idle_expires_at
         )
         SELECT c.id, c.principal_id, p.principal_kind, p.name,
                c.password_revision IS NOT NULL AS password,
                c.expires_at, c.idle_expires_at, TRUE AS recent
           FROM created c JOIN platform.principals p ON p.id = c.principal_id",
    )
    .bind(id)
    .bind(session_hash)
    .bind(principal_id)
    .bind(password)
    .bind(connection)
    .bind(windows.idle.as_secs_f64())
    .bind(windows.absolute.as_secs_f64())
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Resolve the live session `session_hash` names and record this request's
/// activity, which moves the idle deadline but never past the absolute one.
///
/// Live means unrevoked, inside both deadlines, its principal active, and
/// its anchor still that principal's current password revision or the
/// current connection revision; anything else resolves to `None` and
/// writes nothing.
///
/// # Errors
/// Returns [`SqlError::Query`] when the statement fails.
pub async fn resolve_platform_browser_session(
    pool: &OperatorPool,
    session_hash: &[u8],
    windows: PlatformSessionWindows,
) -> Result<Option<PlatformBrowserSessionRow>, SqlError> {
    sqlx::query_as(
        "UPDATE platform.browser_sessions s
            SET last_active_at = statement_timestamp(),
                idle_expires_at = LEAST(s.expires_at,
                                        statement_timestamp() + $2 * interval '1 second')
           FROM platform.principals p
          WHERE s.session_hash = $1
            AND s.revoked_at IS NULL
            AND s.idle_expires_at > statement_timestamp()
            AND s.expires_at > statement_timestamp()
            AND p.id = s.principal_id AND p.status = 'active'
            AND (s.password_revision IS NULL
                 OR (p.password_hash IS NOT NULL AND p.password_revision = s.password_revision))
            AND (s.connection_revision IS NULL
                 OR EXISTS (SELECT 1 FROM platform.oidc_connection o
                             WHERE o.revision = s.connection_revision))
          RETURNING s.id, s.principal_id, p.principal_kind, p.name,
                    s.password_revision IS NOT NULL AS password,
                    s.expires_at, s.idle_expires_at,
                    s.fresh_at > statement_timestamp() - $3 * interval '1 second' AS recent",
    )
    .bind(session_hash)
    .bind(windows.idle.as_secs_f64())
    .bind(windows.recent.as_secs_f64())
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Record fresh human authentication on live session `id` of principal
/// `principal_id`. Returns whether a live session was refreshed.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn refresh_platform_browser_session(
    pool: &OperatorPool,
    id: Uuid,
    principal_id: Uuid,
) -> Result<bool, SqlError> {
    sqlx::query(
        "UPDATE platform.browser_sessions
            SET fresh_at = statement_timestamp()
          WHERE id = $1 AND principal_id = $2 AND revoked_at IS NULL
            AND idle_expires_at > statement_timestamp()
            AND expires_at > statement_timestamp()",
    )
    .bind(id)
    .bind(principal_id)
    .execute(pool.pool())
    .await
    .map(|done| done.rows_affected() == 1)
    .map_err(SqlError::from)
}

/// Revoke the one session `session_hash` names, if it is unrevoked.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn revoke_platform_browser_session(
    pool: &OperatorPool,
    session_hash: &[u8],
) -> Result<(), SqlError> {
    sqlx::query(
        "UPDATE platform.browser_sessions SET revoked_at = statement_timestamp()
          WHERE session_hash = $1 AND revoked_at IS NULL",
    )
    .bind(session_hash)
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}
