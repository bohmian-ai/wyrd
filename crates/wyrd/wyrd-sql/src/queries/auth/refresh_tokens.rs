//! Query slots for `wyrd.auth_refresh_tokens`.
//!
//! All functions take `&mut TenantConn<'_>`. Postgres RLS enforces tenant
//! isolation; selected queries also include explicit `data_tenant_id` predicates
//! where the index benefits.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

use crate::TenantConn;
use crate::row_types::auth::RefreshTokenRow;

const CONSUME_ACTIVE_REFRESH_SQL: &str = r#"
    UPDATE wyrd.auth_refresh_tokens
       SET revoked_at = statement_timestamp(),
           revoked_reason = 'rotated'
     WHERE token_hash = $1
       AND data_tenant_id = $2
       AND revoked_at IS NULL
       AND expires_at > statement_timestamp()
    RETURNING id, data_tenant_id, principal_kind, principal_id, token_hash,
              issued_at, expires_at, rotated_from, revoked_at, revoked_reason
"#;

const REFRESH_ISSUANCE_INSTANT_SQL: &str = r#"
    SELECT date_trunc('second', statement_timestamp())
"#;

const REFRESH_BY_HASH_SQL: &str = r#"
    SELECT id, data_tenant_id, principal_kind, principal_id, token_hash,
           issued_at, expires_at, rotated_from, revoked_at, revoked_reason
      FROM wyrd.auth_refresh_tokens
     WHERE token_hash = $1
       AND data_tenant_id = $2
     LIMIT 1
"#;

const INSERT_REFRESH_TOKEN_ROTATED_SQL: &str = r#"
    INSERT INTO wyrd.auth_refresh_tokens (
        id, data_tenant_id, principal_kind, principal_id,
        token_hash, expires_at, rotated_from
    ) VALUES ($1, $2, $3, $4, $5, $6, $7)
"#;

/// Sample the `PostgreSQL` issuance instant for a refresh token, truncated to a
/// whole second.
///
/// The signed `exp` claim carries only whole seconds, so truncating here makes
/// the JWT expiry and the durable row expiry derived from this instant exactly
/// equal rather than equal-to-the-second. The caller mints and inserts inside
/// the same transaction, and the consume predicate that later evaluates the
/// stored expiry uses the same database clock.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the statement.
pub async fn refresh_issuance_instant(
    conn: &mut TenantConn<'_>,
) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar::<_, DateTime<Utc>>(REFRESH_ISSUANCE_INSTANT_SQL)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Atomically revoke the active row and return it.
///
/// Executes a single `UPDATE … RETURNING` so two concurrent callers presenting
/// the same token race on the same row write. Postgres serialises the write:
/// exactly one caller receives `Some(row)` and wins the rotation; the other
/// receives `None` and falls through to reuse detection (F07).
///
/// The row is immediately marked `revoked_reason = 'rotated'` inside the
/// caller's transaction. The caller must commit after issuing the successor
/// tokens.
pub async fn consume_active_refresh(
    conn: &mut TenantConn<'_>,
    token_hash: &str,
) -> Result<Option<RefreshTokenRow>, sqlx::Error> {
    let tenant_id = conn.data_tenant_id().as_uuid();
    sqlx::query_as::<_, RefreshTokenRow>(CONSUME_ACTIVE_REFRESH_SQL)
        .bind(token_hash)
        .bind(tenant_id)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Look up any row by hash regardless of its lifecycle state.
///
/// Used in the reuse-detection path: if `consume_active_refresh` returns
/// `None`, this query determines whether the presented token was ever valid
/// (rotated or explicitly revoked) or is entirely unknown.
pub async fn refresh_by_hash(
    conn: &mut TenantConn<'_>,
    token_hash: &str,
) -> Result<Option<RefreshTokenRow>, sqlx::Error> {
    let tenant_id = conn.data_tenant_id().as_uuid();
    sqlx::query_as::<_, RefreshTokenRow>(REFRESH_BY_HASH_SQL)
        .bind(token_hash)
        .bind(tenant_id)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Mark a single token row revoked with an explicit reason.
///
/// Not used on the hot rotation path — `consume_active_refresh` revokes
/// atomically. Intended for the admin revoke operation (commit 05) and
/// direct revocation tooling.
pub async fn revoke_refresh(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE wyrd.auth_refresh_tokens
           SET revoked_at = now(),
               revoked_reason = $2
         WHERE id = $1
           AND revoked_at IS NULL
        "#,
    )
    .bind(id)
    .bind(reason)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// Revoke every active token in the family owned by a principal.
///
/// Bulk-revokes all rows where `(principal_kind, principal_id)` match and
/// `revoked_at IS NULL`. Used as a theft response when reuse of an
/// already-rotated token is detected.
///
/// Returns the number of rows updated.
pub async fn revoke_refresh_family(
    conn: &mut TenantConn<'_>,
    principal_kind: &str,
    principal_id: Uuid,
    reason: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.auth_refresh_tokens
           SET revoked_at = now(),
               revoked_reason = $3
         WHERE principal_kind = $1
           AND principal_id = $2
           AND revoked_at IS NULL
        "#,
    )
    .bind(principal_kind)
    .bind(principal_id)
    .bind(reason)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(result.rows_affected())
}

/// Insert a new refresh token row with a `rotated_from` back-link.
///
/// Called during rotation to record the successor token alongside the id of
/// the token it replaced. The predecessor row must already be revoked (via
/// `consume_active_refresh`) before this insert runs in the same transaction.
pub async fn insert_refresh_token_rotated(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    principal_kind: &str,
    principal_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
    rotated_from: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_REFRESH_TOKEN_ROTATED_SQL)
        .bind(id)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(principal_kind)
        .bind(principal_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(rotated_from)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CONSUME_ACTIVE_REFRESH_SQL, INSERT_REFRESH_TOKEN_ROTATED_SQL, REFRESH_BY_HASH_SQL,
    };

    #[test]
    fn consume_active_refresh_is_atomic_update_returning() {
        assert!(CONSUME_ACTIVE_REFRESH_SQL.contains("UPDATE wyrd.auth_refresh_tokens"));
        assert!(CONSUME_ACTIVE_REFRESH_SQL.contains("revoked_at IS NULL"));
        assert!(CONSUME_ACTIVE_REFRESH_SQL.contains("expires_at > statement_timestamp()"));
        assert!(CONSUME_ACTIVE_REFRESH_SQL.contains("revoked_reason = 'rotated'"));
        assert!(CONSUME_ACTIVE_REFRESH_SQL.contains("data_tenant_id = $2"));
        assert!(CONSUME_ACTIVE_REFRESH_SQL.contains("RETURNING"));
    }

    #[test]
    fn refresh_by_hash_selects_all_lifecycle_states() {
        assert!(REFRESH_BY_HASH_SQL.contains("FROM wyrd.auth_refresh_tokens"));
        assert!(REFRESH_BY_HASH_SQL.contains("token_hash = $1"));
        assert!(REFRESH_BY_HASH_SQL.contains("data_tenant_id = $2"));
        // No revoked_at filter — finds rows in any state.
        assert!(!REFRESH_BY_HASH_SQL.contains("revoked_at IS NULL"));
        assert!(!REFRESH_BY_HASH_SQL.contains("expires_at >"));
    }

    #[test]
    fn insert_refresh_token_rotated_includes_rotated_from() {
        assert!(INSERT_REFRESH_TOKEN_ROTATED_SQL.contains("rotated_from"));
        assert!(INSERT_REFRESH_TOKEN_ROTATED_SQL.contains("principal_kind"));
        assert!(INSERT_REFRESH_TOKEN_ROTATED_SQL.contains("principal_id"));
        assert!(INSERT_REFRESH_TOKEN_ROTATED_SQL.contains("token_hash"));
    }
}
