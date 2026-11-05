//! Tenant-scoped local-password lifecycle queries: one-use enrollment
//! invitations, operator-issued password resets, and fixed-window password
//! failure counters.
//!
//! Codes are stored only as the SHA-256 of their 256 random bits. Every
//! deadline is derived by `PostgreSQL` from a bound or fixed duration, and
//! expired rows are purged when the tenant next writes the same kind, as
//! browser sessions and login state are. Forced RLS is the only tenant
//! selection.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wyrd_spec::auth::Sha256Hex;

use crate::TenantConn;

/// A stored enrollment invitation's safe metadata.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct InvitationRow {
    /// Invitation id.
    pub id: Uuid,
    /// The principal that issued it.
    pub issuer_principal_id: Uuid,
    /// Role ids granted at enrollment, ordered by UUID.
    pub role_ids: Vec<Uuid>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Absolute expiry.
    pub expires_at: DateTime<Utc>,
}

/// A stored password reset's safe metadata.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PasswordResetRow {
    /// Reset id.
    pub id: Uuid,
    /// The local User it resets.
    pub user_id: Uuid,
    /// The User's password revision when it was issued.
    pub password_revision: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Absolute expiry.
    pub expires_at: DateTime<Utc>,
}

/// Which fixed failure window a counter belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptScope {
    /// One tenant's canonical username.
    Account,
    /// One trusted connection source.
    Source,
}

impl AttemptScope {
    /// The stored `scope` label.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Source => "source",
        }
    }
}

/// Record an invitation for `role_ids` (stored ordered by UUID), issued by
/// `issuer`, whose code hashes to `code_hash`, expiring twenty-four hours
/// after creation; returns its stored metadata. Expired invitations are
/// purged first.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or insert.
pub async fn insert_invitation(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    code_hash: &Sha256Hex,
    issuer: Uuid,
    role_ids: &[Uuid],
) -> Result<InvitationRow, sqlx::Error> {
    sqlx::query("DELETE FROM wyrd.auth_user_invitations WHERE expires_at <= statement_timestamp()")
        .execute(&mut **conn.transaction())
        .await?;
    let mut ordered = role_ids.to_vec();
    ordered.sort_unstable();
    sqlx::query_as::<_, InvitationRow>(
        r"
        INSERT INTO wyrd.auth_user_invitations (
            id, data_tenant_id, code_hash, issuer_principal_id, role_ids, created_at, expires_at
        ) VALUES ($1, $2, $3, $4, $5, statement_timestamp(),
                  statement_timestamp() + interval '24 hours')
        RETURNING id, issuer_principal_id, role_ids, created_at, expires_at
        ",
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(code_hash.as_bytes().as_slice())
    .bind(issuer)
    .bind(&ordered)
    .fetch_one(&mut **conn.transaction())
    .await
}

/// Consume the live invitation whose code hashes to `code_hash` for the new
/// User `user_id`, and return it; `None` when none is unexpired, unrevoked
/// and unconsumed.
///
/// The row is consumed by one conditional update, so of two concurrent
/// enrollments with one code exactly one finds it. The caller inserts
/// `user_id` in the same transaction before this runs, since the consumer
/// references it; a rollback restores the invitation.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn consume_invitation(
    conn: &mut TenantConn<'_>,
    code_hash: &Sha256Hex,
    user_id: Uuid,
) -> Result<Option<InvitationRow>, sqlx::Error> {
    sqlx::query_as::<_, InvitationRow>(
        r"
        UPDATE wyrd.auth_user_invitations
           SET consumed_at = statement_timestamp(), consumed_by = $2
         WHERE code_hash = $1
           AND consumed_at IS NULL
           AND revoked_at IS NULL
           AND expires_at > statement_timestamp()
        RETURNING id, issuer_principal_id, role_ids, created_at, expires_at
        ",
    )
    .bind(code_hash.as_bytes().as_slice())
    .bind(user_id)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Revoke the unconsumed invitation `id`; returns whether one was revoked.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn revoke_invitation(conn: &mut TenantConn<'_>, id: Uuid) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        r"
        UPDATE wyrd.auth_user_invitations
           SET revoked_at = statement_timestamp()
         WHERE id = $1
           AND revoked_at IS NULL
           AND consumed_at IS NULL
        ",
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Record a reset of `user_id` at its current `password_revision`, issued by
/// `issuer`, whose code hashes to `code_hash`, expiring fifteen minutes after
/// creation; returns its stored metadata. Expired resets are purged first.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or insert.
pub async fn insert_password_reset(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    code_hash: &Sha256Hex,
    user_id: Uuid,
    password_revision: i64,
    issuer: Uuid,
) -> Result<PasswordResetRow, sqlx::Error> {
    sqlx::query("DELETE FROM wyrd.auth_password_resets WHERE expires_at <= statement_timestamp()")
        .execute(&mut **conn.transaction())
        .await?;
    sqlx::query_as::<_, PasswordResetRow>(
        r"
        INSERT INTO wyrd.auth_password_resets (
            id, data_tenant_id, code_hash, user_id, password_revision, issuer_principal_id,
            created_at, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, statement_timestamp(),
                  statement_timestamp() + interval '15 minutes')
        RETURNING id, user_id, password_revision, created_at, expires_at
        ",
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(code_hash.as_bytes().as_slice())
    .bind(user_id)
    .bind(password_revision)
    .bind(issuer)
    .fetch_one(&mut **conn.transaction())
    .await
}

/// Consume the live reset whose code hashes to `code_hash`, and return it;
/// `None` when none is unexpired and unconsumed. The caller still requires
/// the returned revision to be the User's current one.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn consume_password_reset(
    conn: &mut TenantConn<'_>,
    code_hash: &Sha256Hex,
) -> Result<Option<PasswordResetRow>, sqlx::Error> {
    sqlx::query_as::<_, PasswordResetRow>(
        r"
        UPDATE wyrd.auth_password_resets
           SET consumed_at = statement_timestamp()
         WHERE code_hash = $1
           AND consumed_at IS NULL
           AND expires_at > statement_timestamp()
        RETURNING id, user_id, password_revision, created_at, expires_at
        ",
    )
    .bind(code_hash.as_bytes().as_slice())
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Seconds until the current window for (`scope`, `subject`) ends when it
/// already holds `limit` or more failures; `None` when an attempt is
/// admitted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read.
pub async fn attempt_refusal(
    conn: &mut TenantConn<'_>,
    scope: AttemptScope,
    subject: &str,
    limit: i32,
) -> Result<Option<Duration>, sqlx::Error> {
    let seconds: Option<f64> = sqlx::query_scalar(
        r"
        SELECT extract(epoch FROM window_ends_at - statement_timestamp())::float8
          FROM wyrd.auth_password_attempts
         WHERE scope = $1
           AND subject = $2
           AND failures >= $3
           AND window_ends_at > statement_timestamp()
        ",
    )
    .bind(scope.as_str())
    .bind(subject)
    .bind(limit)
    .fetch_optional(&mut **conn.transaction())
    .await?;
    Ok(seconds.map(|seconds| Duration::from_secs_f64(seconds.max(1.0).ceil())))
}

/// Count one failure for (`scope`, `subject`) in its fixed fifteen-minute
/// window, opening a new window when none is current. Expired windows are
/// purged first. One atomic upsert, so concurrent failures on any replica are
/// all counted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or upsert.
pub async fn record_attempt_failure(
    conn: &mut TenantConn<'_>,
    scope: AttemptScope,
    subject: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM wyrd.auth_password_attempts WHERE window_ends_at <= statement_timestamp()",
    )
    .execute(&mut **conn.transaction())
    .await?;
    sqlx::query(
        r"
        INSERT INTO wyrd.auth_password_attempts AS a (
            data_tenant_id, scope, subject, failures, window_ends_at
        ) VALUES ($1, $2, $3, 1, statement_timestamp() + interval '15 minutes')
        ON CONFLICT (data_tenant_id, scope, subject) DO UPDATE
           SET failures = CASE WHEN a.window_ends_at <= statement_timestamp() THEN 1
                               ELSE a.failures + 1 END,
               window_ends_at = CASE WHEN a.window_ends_at <= statement_timestamp()
                                     THEN statement_timestamp() + interval '15 minutes'
                                     ELSE a.window_ends_at END
        ",
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(scope.as_str())
    .bind(subject)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}
