//! Tenant-scoped non-human principal and API-key queries.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::types::{Json, Uuid};
use wyrd_spec::reference::CardRef;

use crate::TenantConn;

/// Resolve an active Card-bound principal from the Card identity a client can
/// express.
///
/// Containment, not equality: registering a Card-bound principal stores a
/// `uid`-bearing `card_ref` — the projection at `queries::cards::auth_projection`
/// writes `space: Some(..)` and `uid: Some(..)` — while a caller can only name
/// `space/Kind/name@version`, so `card_ref = $3` matched no registered principal
/// at all.
///
/// Containment relaxes *every* optional `CardRef` field, `space` included: a ref
/// with no space matches a row in any space. What bounds this to one intended
/// row is not the predicate but two facts outside it — the table's
/// `UNIQUE (data_tenant_id, name)` and `auth_projection` keeping the `name`
/// column equal to `card_ref->>'name'`. `ORDER BY created_at, id LIMIT 1` exists
/// because none of that chain is enforced here: relax the unique constraint or
/// decouple the name projection, and this predicate starts matching more rows on
/// a credential-issuing path. The stable oldest-first pick is then the
/// difference between a bounded anomaly and an arbitrary one — narrow the
/// predicate rather than lean on that fallback.
const SERVICE_ACCOUNT_BY_CARD_REF_SQL: &str = r#"
        SELECT id, principal_kind, card_ref, status
          FROM wyrd.auth_service_accounts
         WHERE data_tenant_id = $1
           AND principal_kind = $2
           AND card_ref @> $3
           AND status = 'active'
         ORDER BY created_at, id
         LIMIT 1
        "#;

const INSERT_REFRESH_TOKEN_SQL: &str = r#"
        INSERT INTO wyrd.auth_refresh_tokens (
            id, data_tenant_id, principal_kind, principal_id, token_hash, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6)
        "#;

/// Active tenant-scope machine principal row.
///
/// Covers every kind `wyrd.auth_service_accounts` holds: a tenant
/// administrator, a Card-free automation identity, and a Card-bound Service or
/// Agent workload.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceAccountPrincipalRow {
    /// Principal id.
    pub id: Uuid,
    /// Principal kind label: `tenant_admin`, `service`, or `agent`.
    pub principal_kind: String,
    /// Structured card reference, absent for a principal that binds no Card.
    pub card_ref: Option<Json<CardRef>>,
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
    /// Structured card reference, absent for principals that bind no Card.
    pub card_ref: Option<Json<CardRef>>,
    /// Principal status.
    pub status: String,
}

/// API-key status derived from row presence and lifecycle timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyStatus {
    /// Row exists, `revoked_at IS NULL`, and the credential is unexpired —
    /// either `expires_at IS NULL` or `expires_at > now()`.
    Active,
    /// Row exists and `revoked_at IS NOT NULL`.
    Revoked,
    /// Row exists, is not revoked, and `expires_at <= now()`.
    Expired,
    /// No row matches the prefix.
    Missing,
}

/// Insert a Service or Agent principal row.
///
/// The database check constraint enforces the allowed `principal_kind` and
/// `card_kind` combinations.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn insert_service_account(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    principal_kind: &str,
    card_ref: Option<&CardRef>,
    name: &str,
    description: Option<&str>,
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    // Card binding is a property of a deployable machine principal, so every
    // Card-derived column travels with the Card or is absent with it. A tenant
    // administrator or Card-free automation principal stores none of them.
    let card_kind = card_ref.map(|card_ref| card_ref.kind.wire_name());
    let card_uid = card_ref.map(|card_ref| {
        card_ref.uid.as_ref().map_or_else(Uuid::nil, |uid| {
            Uuid::parse_str(uid.as_str()).expect("CardUid invariant: stored value is a valid UUID")
        })
    });
    let space = card_ref.map(|card_ref| {
        card_ref
            .space
            .as_ref()
            .expect("invariant: card-bound principal CardRef has resolved space")
            .as_str()
    });
    let version = card_ref.map(|card_ref| card_ref.version.as_str());

    sqlx::query(
        r#"
        INSERT INTO wyrd.auth_service_accounts (
            id, data_tenant_id, principal_kind, card_kind, card_uid,
            card_ref, space, name, version, description, status, created_by
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'active', $11)
        "#,
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(principal_kind)
    .bind(card_kind)
    .bind(card_uid)
    .bind(card_ref.map(Json))
    .bind(space)
    .bind(name)
    .bind(version)
    .bind(description)
    .bind(created_by)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// Soft-delete a service account by marking `status = 'deleted'`.
///
/// Returns `Ok(true)` when a row was updated, `Ok(false)` when no row matched.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn delete_service_account(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE wyrd.auth_service_accounts
            SET status = 'deleted', updated_at = now()
          WHERE data_tenant_id = wyrd.current_tenant()
            AND id = $1",
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Find an active Service/Agent principal by card ref.
///
/// Binds the caller's ref as JSONB for a `card_ref @> $3` containment
/// predicate, so a ref carrying no `uid` matches a stored ref that has one and
/// a ref carrying no space matches a row in any space. The containment
/// predicate alone does not bound the match to one row: the table's
/// `UNIQUE (data_tenant_id, name)` and the `auth_projection` trigger keeping
/// `name` equal to `card_ref->>'name'` do, and `ORDER BY created_at, id
/// LIMIT 1` is the stable fallback if either is relaxed. The durable key
/// remains `(card_kind, card_uid)`; this is the lookup for the identity a
/// client can express, and the GIN index on `card_ref` serves it.
///
/// # Errors
/// Returns the database error when the read fails.
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

/// Find the tenant's administrative principal.
///
/// A tenant has exactly one, created during provisioning. Recovery needs it by
/// identity rather than by Card or name, because the whole point is that the
/// principal outlives the credentials that used to reach it. The oldest active
/// one wins so a tenant that somehow acquired two resolves deterministically.
///
/// # Errors
/// Returns the database error when the read fails.
pub async fn tenant_admin_principal_id(
    conn: &mut TenantConn<'_>,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT id
          FROM wyrd.auth_service_accounts
         WHERE data_tenant_id = $1
           AND principal_kind = 'tenant_admin'
           AND status = 'active'
         ORDER BY created_at
         LIMIT 1
        "#,
    )
    .bind(conn.data_tenant_id().as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Insert a hashed credential row for a tenant-scope principal.
///
/// Stores only the Argon2 verifier and non-secret lookup metadata; the
/// plaintext is returned once by the issuing caller and never persisted.
/// `expires_at` is optional: an administrative credential issued during tenant
/// provisioning or recovery has no natural lifetime, while a workload key keeps
/// the bounded expiry its issuance path supplies.
///
/// # Errors
/// Returns the database error when the insert fails, including when `prefix`
/// collides within the tenant or `principal_id` names no principal.
pub async fn insert_api_key(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    principal_id: Uuid,
    prefix: &str,
    key_hash: &str,
    created_by: Uuid,
    expires_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO wyrd.auth_api_keys (
            id, data_tenant_id, principal_id, prefix, key_hash, created_by, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(principal_id)
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
           AND sa.id = k.principal_id
         WHERE k.data_tenant_id = $1
           AND k.prefix = $2
           AND k.revoked_at IS NULL
           AND (k.expires_at IS NULL OR k.expires_at > now())
         LIMIT 1
        "#,
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(prefix)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Resolve an API key status by prefix without filtering invalid states.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query or row decoding fails.
pub async fn api_key_status_by_prefix(
    conn: &mut TenantConn<'_>,
    prefix: &str,
) -> Result<ApiKeyStatus, sqlx::Error> {
    let row: Option<(Option<DateTime<Utc>>, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT revoked_at, expires_at
          FROM wyrd.auth_api_keys
         WHERE data_tenant_id = $1
           AND prefix = $2
         LIMIT 1
        "#,
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(prefix)
    .fetch_optional(&mut **conn.transaction())
    .await?;

    Ok(match row {
        None => ApiKeyStatus::Missing,
        Some((Some(_), _)) => ApiKeyStatus::Revoked,
        Some((None, expires_at)) if expires_at <= Utc::now() => ApiKeyStatus::Expired,
        Some((None, _)) => ApiKeyStatus::Active,
    })
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
#[deprecated(
    since = "0.0.1",
    note = "use role_assignments::list_service_account_roles"
)]
pub async fn service_account_roles(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    super::role_assignments::list_service_account_roles(conn, principal_id).await
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

#[cfg(test)]
mod tests {
    use sqlx::types::Json;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    use super::{ApiKeyStatus, INSERT_REFRESH_TOKEN_SQL, SERVICE_ACCOUNT_BY_CARD_REF_SQL};

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
           AND sa.id = k.principal_id
         WHERE k.data_tenant_id = $1
           AND k.prefix = $2
           AND k.revoked_at IS NULL
           AND (k.expires_at IS NULL OR k.expires_at > now())
         LIMIT 1
        "#;

        assert!(sql.contains("k.prefix = $2"));
        assert!(sql.contains("k.revoked_at IS NULL"));
        assert!(sql.contains("(k.expires_at IS NULL OR k.expires_at > now())"));
        assert!(sql.contains("sa.id = k.principal_id"));
        assert!(sql.contains("sa.data_tenant_id = k.data_tenant_id"));
    }

    /// The shipped predicate matches a Card identity and returns one row.
    ///
    /// Pinned as text because the widening this guards is invisible at the call
    /// site: `=` would match no registered principal, and dropping the ordered
    /// `LIMIT 1` would make a multi-row match arbitrary rather than bounded.
    #[test]
    fn service_account_by_card_ref_uses_jsonb_card_ref_binding() {
        let card_ref = CardRef {
            kind: CardKind::Agent,
            name: CardName::new("runtime").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        };
        let Json(bound) = Json(card_ref.clone());

        assert_eq!(bound, card_ref);
        assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref @> $3"));
        assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("principal_kind = $2"));
        assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("ORDER BY created_at, id"));
        assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("LIMIT 1"));
        assert!(!SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref::text"));
    }

    #[test]
    fn refresh_token_insert_is_principal_generic() {
        assert!(INSERT_REFRESH_TOKEN_SQL.contains("principal_kind, principal_id"));
        assert!(INSERT_REFRESH_TOKEN_SQL.contains("token_hash"));
        assert!(!INSERT_REFRESH_TOKEN_SQL.contains("user_id"));
        assert!(!INSERT_REFRESH_TOKEN_SQL.contains("service_account_id"));
    }

    #[test]
    fn api_key_status_enum_covers_invalid_states() {
        let statuses = [
            ApiKeyStatus::Active,
            ApiKeyStatus::Revoked,
            ApiKeyStatus::Expired,
            ApiKeyStatus::Missing,
        ];
        assert_eq!(statuses.len(), 4);
        assert_eq!(ApiKeyStatus::Active, ApiKeyStatus::Active);
    }
}
