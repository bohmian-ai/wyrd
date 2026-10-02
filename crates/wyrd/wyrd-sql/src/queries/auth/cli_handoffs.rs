//! Tenant-scoped CLI login handoff queries.
//!
//! A handoff row binds a server-issued handoff id to the tenant, the Active
//! connection the login began through, and the SHA-256 of the CLI-held
//! verifier. [`insert_cli_handoff`] writes it, [`cli_handoff_is_open`] admits
//! it as a login's initiation binding, [`lock_cli_handoff`] proves the
//! verifier before a claim redeems the login's sealed completion, and
//! [`delete_cli_handoff`] ends it together with any login state still bound
//! to it. Forced RLS is the only tenant selection and `PostgreSQL` owns every
//! expiry: callers bind lifetimes, never instants.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wyrd_spec::auth::Sha256Hex;

use crate::TenantConn;

/// Drop this tenant's handoffs that can no longer be claimed.
const PURGE_EXPIRED_CLI_HANDOFFS_SQL: &str = r#"
    DELETE FROM wyrd.auth_cli_handoffs
     WHERE expires_at <= statement_timestamp()
"#;

/// Record one handoff owned by the RLS tenant; `PostgreSQL` derives its
/// expiry from the bound lifetime in seconds and returns it.
const INSERT_CLI_HANDOFF_SQL: &str = r#"
    INSERT INTO wyrd.auth_cli_handoffs (
        handoff_id, data_tenant_id, connection_id, verifier_hash, expires_at
    ) VALUES ($1, $2, $3, $4, statement_timestamp() + ($5 * interval '1 second'))
    RETURNING expires_at
"#;

/// Whether an unexpired handoff with this id is visible to the RLS tenant.
const CLI_HANDOFF_IS_OPEN_SQL: &str = r#"
    SELECT EXISTS (
        SELECT 1 FROM wyrd.auth_cli_handoffs
         WHERE handoff_id = $1
           AND expires_at > statement_timestamp()
    )
"#;

/// Lock the unexpired handoff this id and verifier hash name, returning the
/// connection it was begun through.
const LOCK_CLI_HANDOFF_SQL: &str = r#"
    SELECT connection_id FROM wyrd.auth_cli_handoffs
     WHERE handoff_id = $1
       AND verifier_hash = $2
       AND expires_at > statement_timestamp()
       FOR UPDATE
"#;

/// Delete the handoff this id and verifier hash name, with any login state
/// still bound to it, and report whether a handoff was deleted.
const DELETE_CLI_HANDOFF_SQL: &str = r#"
    WITH handoff AS (
        DELETE FROM wyrd.auth_cli_handoffs
         WHERE handoff_id = $1
           AND verifier_hash = $2
        RETURNING handoff_id
    ), login AS (
        DELETE FROM wyrd.auth_login_state
         WHERE cli_handoff_id IN (SELECT handoff_id FROM handoff)
    )
    SELECT EXISTS (SELECT 1 FROM handoff)
"#;

/// Insert a handoff for `connection_id` whose expiry `PostgreSQL` derives
/// from `ttl`, and return that expiry.
///
/// First purges this tenant's expired handoffs, so abandoned logins do not
/// accumulate. The table refuses a lifetime above five minutes.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the purge or the insert,
/// including a reused handoff id or a refused lifetime.
pub async fn insert_cli_handoff(
    conn: &mut TenantConn<'_>,
    handoff_id: Uuid,
    connection_id: Uuid,
    verifier_hash: &Sha256Hex,
    ttl: Duration,
) -> Result<DateTime<Utc>, sqlx::Error> {
    let tenant = conn.data_tenant_id().as_uuid();
    sqlx::query(PURGE_EXPIRED_CLI_HANDOFFS_SQL)
        .execute(&mut **conn.transaction())
        .await?;
    sqlx::query_scalar::<_, DateTime<Utc>>(INSERT_CLI_HANDOFF_SQL)
        .bind(handoff_id)
        .bind(tenant)
        .bind(connection_id)
        .bind(verifier_hash.as_bytes().as_slice())
        .bind(ttl.as_secs_f64())
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Whether this tenant has an unexpired handoff `handoff_id`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read.
pub async fn cli_handoff_is_open(
    conn: &mut TenantConn<'_>,
    handoff_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(CLI_HANDOFF_IS_OPEN_SQL)
        .bind(handoff_id)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Lock this tenant's unexpired handoff `handoff_id` whose verifier hashes to
/// `verifier_hash` until the transaction ends, returning its connection id.
///
/// A missing, expired, or other tenant's handoff and a wrong verifier all
/// return `None`, so a caller cannot tell them apart.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the read.
pub async fn lock_cli_handoff(
    conn: &mut TenantConn<'_>,
    handoff_id: Uuid,
    verifier_hash: &Sha256Hex,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(LOCK_CLI_HANDOFF_SQL)
        .bind(handoff_id)
        .bind(verifier_hash.as_bytes().as_slice())
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Delete this tenant's handoff `handoff_id` whose verifier hashes to
/// `verifier_hash`, expired or not, and any login state still bound to it.
///
/// Returns whether a handoff was deleted.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn delete_cli_handoff(
    conn: &mut TenantConn<'_>,
    handoff_id: Uuid,
    verifier_hash: &Sha256Hex,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(DELETE_CLI_HANDOFF_SQL)
        .bind(handoff_id)
        .bind(verifier_hash.as_bytes().as_slice())
        .fetch_one(&mut **conn.transaction())
        .await
}
