//! Tenant-scoped device authorization (RFC 8628) queries.
//!
//! A row binds a device id to the tenant, the SHA-256 of the device code the
//! CLI polls with, and the user code the person approves.
//! [`insert_device_authorization`] writes it, [`pending_device_authorization`]
//! finds it by user code on the verification page,
//! [`deny_device_authorization`] records a denial,
//! [`approve_device_authorization`] records the signed-in principal and
//! connection once the provider callback verified the sign-in,
//! [`poll_device_authorization`] locks it for a token poll and enforces the
//! poll interval, and [`delete_device_authorization`] ends it together with
//! any login state still bound to it. Forced RLS is the only tenant selection and `PostgreSQL` owns
//! every expiry and poll time: callers bind durations, never instants.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use uuid::Uuid;
use wyrd_spec::auth::Sha256Hex;

use crate::TenantConn;
use crate::row_types::auth::HumanConnectionBinding;

/// Drop this tenant's device authorizations that can no longer be redeemed.
const PURGE_EXPIRED_DEVICE_AUTHORIZATIONS_SQL: &str = r#"
    DELETE FROM wyrd.auth_device_authorizations
     WHERE expires_at <= statement_timestamp()
"#;

/// Record one device authorization owned by the RLS tenant; `PostgreSQL`
/// derives its expiry from the bound lifetime in seconds.
const INSERT_DEVICE_AUTHORIZATION_SQL: &str = r#"
    INSERT INTO wyrd.auth_device_authorizations (
        device_id, data_tenant_id, device_code_hash, user_code, expires_at
    ) VALUES ($1, $2, $3, $4, statement_timestamp() + ($5 * interval '1 second'))
"#;

/// The id of the unexpired, undecided device authorization with this user
/// code.
const PENDING_DEVICE_AUTHORIZATION_SQL: &str = r#"
    SELECT device_id FROM wyrd.auth_device_authorizations
     WHERE user_code = $1
       AND NOT denied
       AND principal_id IS NULL
       AND expires_at > statement_timestamp()
"#;

/// Mark the unexpired, unapproved device authorization with this user code
/// denied.
const DENY_DEVICE_AUTHORIZATION_SQL: &str = r#"
    UPDATE wyrd.auth_device_authorizations
       SET denied = true
     WHERE user_code = $1
       AND principal_id IS NULL
       AND expires_at > statement_timestamp()
"#;

/// Record the approval of the unexpired, undecided device authorization with
/// this id.
const APPROVE_DEVICE_AUTHORIZATION_SQL: &str = r#"
    UPDATE wyrd.auth_device_authorizations
       SET principal_id = $2,
           connection_id = $3,
           connection_revision = $4
     WHERE device_id = $1
       AND NOT denied
       AND principal_id IS NULL
       AND expires_at > statement_timestamp()
"#;

/// Lock the device authorization this code hash names and record this poll,
/// returning its state and whether the previous poll was within the interval.
const POLL_DEVICE_AUTHORIZATION_SQL: &str = r#"
    WITH device AS (
        SELECT device_id, denied, principal_id, connection_id, connection_revision,
               expires_at <= statement_timestamp() AS expired,
               COALESCE(last_polled_at > statement_timestamp() - ($2 * interval '1 second'),
                        false) AS too_fast
          FROM wyrd.auth_device_authorizations
         WHERE device_code_hash = $1
           FOR UPDATE
    )
    UPDATE wyrd.auth_device_authorizations AS polled
       SET last_polled_at = statement_timestamp()
      FROM device
     WHERE polled.device_id = device.device_id
    RETURNING device.device_id, device.denied, device.principal_id, device.connection_id,
              device.connection_revision, device.expired, device.too_fast
"#;

/// Delete the device authorization with this id and any login state still
/// bound to it.
const DELETE_DEVICE_AUTHORIZATION_SQL: &str = r#"
    WITH device AS (
        DELETE FROM wyrd.auth_device_authorizations
         WHERE device_id = $1
        RETURNING device_id
    )
    DELETE FROM wyrd.auth_login_state
     WHERE device_id IN (SELECT device_id FROM device)
"#;

/// A device authorization as one token poll found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct DevicePoll {
    /// The device id its login is bound to.
    pub device_id: Uuid,
    /// The person denied the user code.
    pub denied: bool,
    /// The principal that approved the user code by signing in; `None` while
    /// pending.
    pub principal_id: Option<Uuid>,
    /// The connection id that sign-in went through, set with `principal_id`.
    pub connection_id: Option<Uuid>,
    /// That connection's revision, set with `principal_id`.
    pub connection_revision: Option<i64>,
    /// The device code has expired.
    pub expired: bool,
    /// The previous poll was less than the interval ago.
    pub too_fast: bool,
}

impl DevicePoll {
    /// The approving principal and the connection revision its sign-in went
    /// through, once the device authorization is approved.
    #[must_use]
    pub fn approval(&self) -> Option<(Uuid, HumanConnectionBinding)> {
        match (
            self.principal_id,
            self.connection_id,
            self.connection_revision,
        ) {
            (Some(principal_id), Some(connection_id), Some(connection_revision)) => Some((
                principal_id,
                HumanConnectionBinding {
                    connection_id,
                    connection_revision,
                },
            )),
            _ => None,
        }
    }
}

/// Insert a device authorization whose expiry `PostgreSQL` derives from
/// `ttl`.
///
/// First purges this tenant's expired device authorizations, so abandoned
/// logins do not accumulate. The table refuses a lifetime above ten minutes.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or the insert,
/// including a reused id, device code, or live user code, or a refused
/// lifetime.
pub async fn insert_device_authorization(
    conn: &mut TenantConn<'_>,
    device_id: Uuid,
    device_code_hash: &Sha256Hex,
    user_code: &str,
    ttl: Duration,
) -> Result<(), sqlx::Error> {
    let tenant = conn.data_tenant_id().as_uuid();
    sqlx::query(PURGE_EXPIRED_DEVICE_AUTHORIZATIONS_SQL)
        .execute(&mut **conn.transaction())
        .await?;
    sqlx::query(INSERT_DEVICE_AUTHORIZATION_SQL)
        .bind(device_id)
        .bind(tenant)
        .bind(device_code_hash.as_bytes().as_slice())
        .bind(user_code)
        .bind(ttl.as_secs_f64())
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// The id of this tenant's unexpired device authorization with `user_code`
/// that is neither denied nor approved, if any.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read.
pub async fn pending_device_authorization(
    conn: &mut TenantConn<'_>,
    user_code: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(PENDING_DEVICE_AUTHORIZATION_SQL)
        .bind(user_code)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Deny this tenant's unexpired, unapproved device authorization with
/// `user_code`, and return whether one was found.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn deny_device_authorization(
    conn: &mut TenantConn<'_>,
    user_code: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(DENY_DEVICE_AUTHORIZATION_SQL)
        .bind(user_code)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Record that `principal_id` approved this tenant's device authorization
/// `device_id` by signing in through `connection`, and return whether it was
/// still unexpired and undecided.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn approve_device_authorization(
    conn: &mut TenantConn<'_>,
    device_id: Uuid,
    principal_id: Uuid,
    connection: HumanConnectionBinding,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(APPROVE_DEVICE_AUTHORIZATION_SQL)
        .bind(device_id)
        .bind(principal_id)
        .bind(connection.connection_id)
        .bind(connection.connection_revision)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Lock this tenant's device authorization whose device code hashes to
/// `device_code_hash` until the transaction ends, record this poll, and
/// return what the poll found; `None` for an unknown code.
///
/// `too_fast` is whether the previous poll was less than `interval` ago.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn poll_device_authorization(
    conn: &mut TenantConn<'_>,
    device_code_hash: &Sha256Hex,
    interval: Duration,
) -> Result<Option<DevicePoll>, sqlx::Error> {
    sqlx::query_as::<_, DevicePoll>(POLL_DEVICE_AUTHORIZATION_SQL)
        .bind(device_code_hash.as_bytes().as_slice())
        .bind(interval.as_secs_f64())
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Delete this tenant's device authorization `device_id` and any login state
/// still bound to it.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn delete_device_authorization(
    conn: &mut TenantConn<'_>,
    device_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(DELETE_DEVICE_AUTHORIZATION_SQL)
        .bind(device_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}
