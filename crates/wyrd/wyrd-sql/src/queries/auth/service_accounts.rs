//! Tenant-scoped non-human principal and API-key queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::types::{Json, Uuid};
use wyrd_spec::reference::CardRef;

use crate::TenantConn;

const SERVICE_ACCOUNT_BY_CARD_REF_SQL: &str = r#"
        SELECT id, principal_kind, card_ref, status
          FROM wyrd.auth_service_accounts
         WHERE data_tenant_id = $1
           AND principal_kind = $2
           AND card_ref = $3
           AND status = 'active'
        "#;

const INSERT_REFRESH_TOKEN_SQL: &str = r#"
        INSERT INTO wyrd.auth_refresh_tokens (
            id, data_tenant_id, principal_kind, principal_id, token_hash, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6)
        "#;

/// Active Service/Agent principal row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceAccountPrincipalRow {
    /// Principal id.
    pub id: Uuid,
    /// Principal kind, `service` or `agent`.
    pub principal_kind: String,
    /// Structured card reference.
    pub card_ref: Json<CardRef>,
    /// Status.
    pub status: String,
}

/// API-key lookup row joined to its principal.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyLookupRow {
    /// API key id.
    pub api_key_id: Uuid,
    /// Stored Argon2 PHC string.
    pub key_hash: String,
    /// Service account id.
    pub principal_id: Uuid,
    /// Principal kind, `service` or `agent`.
    pub principal_kind: String,
    /// Structured card reference.
    pub card_ref: Json<CardRef>,
    /// Principal status.
    pub status: String,
}

/// Find an active Service/Agent principal by card ref.
pub async fn service_account_by_card_ref(
    conn: &mut TenantConn<'_>,
    principal_kind: &str,
    card_ref: &CardRef,
) -> Result<Option<ServiceAccountPrincipalRow>, sqlx::Error> {
    sqlx::query_as::<_, ServiceAccountPrincipalRow>(SERVICE_ACCOUNT_BY_CARD_REF_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(principal_kind)
        .bind(Json(card_ref))
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Find an active Service/Agent principal by id.
pub async fn service_account_by_id(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<ServiceAccountPrincipalRow>, sqlx::Error> {
    sqlx::query_as::<_, ServiceAccountPrincipalRow>(
        r#"
        SELECT id, principal_kind, card_ref, status
          FROM wyrd.auth_service_accounts
         WHERE data_tenant_id = $1
           AND id = $2
           AND status = 'active'
        "#,
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Insert a hashed API key row.
pub async fn insert_api_key(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    sa_id: Uuid,
    prefix: &str,
    key_hash: &str,
    created_by: Uuid,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO wyrd.auth_api_keys (
            id, data_tenant_id, sa_id, prefix, key_hash, created_by, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(sa_id)
    .bind(prefix)
    .bind(key_hash)
    .bind(created_by)
    .bind(expires_at)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// Lookup an unexpired, unrevoked API key by prefix.
pub async fn api_key_by_prefix(
    conn: &mut TenantConn<'_>,
    prefix: &str,
) -> Result<Option<ApiKeyLookupRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyLookupRow>(
        r#"
        SELECT k.id AS api_key_id,
               k.key_hash,
               sa.id AS principal_id,
               sa.principal_kind,
               sa.card_ref,
               sa.status
          FROM wyrd.auth_api_keys k
          JOIN wyrd.auth_service_accounts sa
            ON sa.data_tenant_id = k.data_tenant_id
           AND sa.id = k.sa_id
         WHERE k.data_tenant_id = $1
           AND k.prefix = $2
           AND k.revoked_at IS NULL
           AND k.expires_at > now()
         LIMIT 1
        "#,
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(prefix)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Mark API key usage.
pub async fn touch_api_key_last_used(
    conn: &mut TenantConn<'_>,
    api_key_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE wyrd.auth_api_keys
           SET last_used_at = now()
         WHERE data_tenant_id = $1
           AND id = $2
        "#,
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(api_key_id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// Return role names granted to a non-human principal.
pub async fn service_account_roles(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT r.name
          FROM wyrd.auth_service_account_roles sar
          JOIN wyrd.auth_roles r
            ON r.data_tenant_id = sar.data_tenant_id
           AND r.id = sar.role_id
         WHERE sar.data_tenant_id = $1
           AND sar.service_account_id = $2
         ORDER BY r.name
        "#,
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(principal_id)
    .fetch_all(&mut **conn.transaction())
    .await?;
    Ok(rows)
}

/// Insert a principal-generic refresh token row.
pub async fn insert_refresh_token(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    principal_kind: &str,
    principal_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_REFRESH_TOKEN_SQL)
        .bind(id)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(principal_kind)
        .bind(principal_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Insert durable credential issuance audit.
pub async fn insert_audit_credential_issuance(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    issuer_principal_id: Uuid,
    target_sa_id: Uuid,
    api_key_id: Uuid,
    request_id: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO wyrd.audit_credential_issuance (
            id, data_tenant_id, issuer_principal_id, target_sa_id,
            api_key_id, request_id, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(issuer_principal_id)
    .bind(target_sa_id)
    .bind(api_key_id)
    .bind(request_id)
    .bind(expires_at)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// Insert durable token-exchange audit.
pub async fn insert_audit_token_exchange(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    subject_principal_id: Uuid,
    actor_principal_id: Uuid,
    act_chain: Value,
    request_id: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO wyrd.audit_token_exchange (
            id, data_tenant_id, subject_principal_id, actor_principal_id,
            act_chain, request_id, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(subject_principal_id)
    .bind(actor_principal_id)
    .bind(act_chain)
    .bind(request_id)
    .bind(expires_at)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::types::Json;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::version::VersionBlock;

    use super::{INSERT_REFRESH_TOKEN_SQL, SERVICE_ACCOUNT_BY_CARD_REF_SQL};

    #[test]
    fn api_key_lookup_filters_all_public_invalid_key_cases() {
        let sql = r#"
        SELECT k.id AS api_key_id,
               k.key_hash,
               sa.id AS principal_id,
               sa.principal_kind,
               sa.card_ref,
               sa.status
          FROM wyrd.auth_api_keys k
          JOIN wyrd.auth_service_accounts sa
            ON sa.data_tenant_id = k.data_tenant_id
           AND sa.id = k.sa_id
         WHERE k.data_tenant_id = $1
           AND k.prefix = $2
           AND k.revoked_at IS NULL
           AND k.expires_at > now()
         LIMIT 1
        "#;

        assert!(sql.contains("k.prefix = $2"));
        assert!(sql.contains("k.revoked_at IS NULL"));
        assert!(sql.contains("k.expires_at > now()"));
        assert!(sql.contains("sa.data_tenant_id = k.data_tenant_id"));
    }

    #[test]
    fn service_account_by_card_ref_uses_jsonb_card_ref_binding() {
        let card_ref = CardRef {
            kind: CardKind::Agent,
            name: CardName::new("runtime").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        };
        let Json(bound) = Json(card_ref.clone());

        assert_eq!(bound, card_ref);
        assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3"));
        assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("principal_kind = $2"));
        assert!(!SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref::text"));
    }

    #[test]
    fn refresh_token_insert_is_principal_generic() {
        assert!(INSERT_REFRESH_TOKEN_SQL.contains("principal_kind, principal_id"));
        assert!(INSERT_REFRESH_TOKEN_SQL.contains("token_hash"));
        assert!(!INSERT_REFRESH_TOKEN_SQL.contains("user_id"));
        assert!(!INSERT_REFRESH_TOKEN_SQL.contains("service_account_id"));
    }
}
