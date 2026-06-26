//! Tenant-scoped federated human identity queries.
//!
//! Functions here take `&mut TenantConn<'_>` and rely on Postgres RLS for
//! tenant isolation.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;

use crate::TenantConn;

const UPSERT_USER_IDENTITY_SQL: &str = r#"
    INSERT INTO wyrd.auth_user_identities (
        data_tenant_id, issuer, subject, user_id
    ) VALUES ($1, $2, $3, $4)
    ON CONFLICT (data_tenant_id, issuer, subject) DO UPDATE
        SET user_id = wyrd.auth_user_identities.user_id
    RETURNING user_id
"#;

const USER_ID_BY_IDENTITY_SQL: &str = r#"
    SELECT user_id
      FROM wyrd.auth_user_identities
     WHERE data_tenant_id = wyrd.current_tenant()
       AND issuer = $1
       AND subject = $2
"#;

/// Insert or retain a federated identity mapping.
///
/// Returns the canonical `user_id` linked to `(issuer, subject)` after the
/// insert or conflict no-op.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn upsert_user_identity(
    conn: &mut TenantConn<'_>,
    issuer: &str,
    subject: &str,
    user_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(UPSERT_USER_IDENTITY_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(issuer)
        .bind(subject)
        .bind(user_id)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Look up the user id for a federated identity.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn user_id_by_identity(
    conn: &mut TenantConn<'_>,
    issuer: &str,
    subject: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid,)>(USER_ID_BY_IDENTITY_SQL)
        .bind(issuer)
        .bind(subject)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map(|row| row.map(|(user_id,)| user_id))
}

#[cfg(test)]
mod tests {
    use super::{UPSERT_USER_IDENTITY_SQL, USER_ID_BY_IDENTITY_SQL};

    #[test]
    fn upsert_is_tenant_scoped_and_returns_user_id() {
        assert!(UPSERT_USER_IDENTITY_SQL.contains("data_tenant_id"));
        assert!(UPSERT_USER_IDENTITY_SQL.contains("issuer"));
        assert!(UPSERT_USER_IDENTITY_SQL.contains("subject"));
        assert!(UPSERT_USER_IDENTITY_SQL.contains("RETURNING user_id"));
    }

    #[test]
    fn lookup_scopes_by_current_tenant_and_identity_key() {
        assert!(USER_ID_BY_IDENTITY_SQL.contains("data_tenant_id = wyrd.current_tenant()"));
        assert!(USER_ID_BY_IDENTITY_SQL.contains("issuer = $1"));
        assert!(USER_ID_BY_IDENTITY_SQL.contains("subject = $2"));
    }
}
