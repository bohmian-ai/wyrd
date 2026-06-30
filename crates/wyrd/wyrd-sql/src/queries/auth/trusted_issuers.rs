//! Query slots for `wyrd.auth_trusted_issuers`.
//!
//! All functions take `&mut TenantConn<'_>`. Postgres RLS enforces tenant
//! isolation; no per-query `data_tenant_id` predicate is needed because the
//! RLS policy (`data_tenant_id = wyrd.current_tenant()`) on `TenantConn`
//! scopes every read to the bound tenant.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use serde_json::Value;

use crate::TenantConn;
use crate::row_types::auth::TrustedIssuerRow;

const TRUSTED_ISSUERS_FOR_TENANT_SQL: &str = r#"
    SELECT data_tenant_id, issuer_url, jwks_uri, expected_audience, client_id,
           client_auth, claim_mapping, group_role_map, default_roles,
           principal_kind, jwks_ttl_secs, client_secret_enc,
           created_at, updated_at
      FROM wyrd.auth_trusted_issuers
"#;

const UPSERT_TRUSTED_ISSUER_SQL: &str = r#"
    INSERT INTO wyrd.auth_trusted_issuers (
        data_tenant_id, issuer_url, jwks_uri, expected_audience, client_id,
        client_auth, claim_mapping, group_role_map, default_roles,
        principal_kind, jwks_ttl_secs, client_secret_enc
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
    ON CONFLICT (data_tenant_id, issuer_url) DO UPDATE SET
        jwks_uri = EXCLUDED.jwks_uri,
        expected_audience = EXCLUDED.expected_audience,
        client_id = EXCLUDED.client_id,
        client_auth = EXCLUDED.client_auth,
        claim_mapping = EXCLUDED.claim_mapping,
        group_role_map = EXCLUDED.group_role_map,
        default_roles = EXCLUDED.default_roles,
        principal_kind = EXCLUDED.principal_kind,
        jwks_ttl_secs = EXCLUDED.jwks_ttl_secs,
        client_secret_enc = EXCLUDED.client_secret_enc,
        updated_at = now()
"#;

const TRUSTED_ISSUER_EXISTS_SQL: &str = r#"
    SELECT EXISTS (
        SELECT 1
          FROM wyrd.auth_trusted_issuers
         WHERE data_tenant_id = wyrd.current_tenant()
           AND issuer_url = $1
    )
"#;

const INSERT_TRUSTED_ISSUER_SQL: &str = r#"
    INSERT INTO wyrd.auth_trusted_issuers (
        data_tenant_id, issuer_url, jwks_uri, expected_audience, client_id,
        client_auth, claim_mapping, group_role_map, default_roles,
        principal_kind, jwks_ttl_secs, client_secret_enc
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
"#;

const TRUSTED_ISSUER_BY_URL_SQL: &str = r#"
    SELECT data_tenant_id, issuer_url, jwks_uri, expected_audience, client_id,
           client_auth, claim_mapping, group_role_map, default_roles,
           principal_kind, jwks_ttl_secs, client_secret_enc,
           created_at, updated_at
      FROM wyrd.auth_trusted_issuers
     WHERE issuer_url = $1
"#;

const DELETE_TRUSTED_ISSUER_SQL: &str = r#"
    DELETE FROM wyrd.auth_trusted_issuers
     WHERE issuer_url = $1
"#;

/// Owned column values for an upsert into `wyrd.auth_trusted_issuers`.
///
/// `data_tenant_id` is bound from the [`TenantConn`], never from this struct, so
/// a write can only ever target the bound tenant. JSONB columns are pre-encoded
/// `serde_json::Value`s; `client_secret_enc` is the raw `nonce ‖ ciphertext`
/// AES-GCM payload (or `None` for secret-free client auth).
#[derive(Debug, Clone)]
pub struct TrustedIssuerWrite {
    /// OIDC issuer URL (natural composite key with `data_tenant_id`).
    pub issuer_url: String,
    /// Resolved JWKS endpoint URI.
    pub jwks_uri: String,
    /// Expected `aud` claim value for tokens from this issuer.
    pub expected_audience: String,
    /// Wyrd's OAuth 2.0 client identifier at this IdP.
    pub client_id: String,
    /// `ClientAuth` discriminant.
    pub client_auth: String,
    /// Lossless `ClaimMapping` serialized to JSONB.
    pub claim_mapping: Value,
    /// IdP group → Wyrd role mapping serialized to JSONB.
    pub group_role_map: Value,
    /// Baseline roles serialized to JSONB.
    pub default_roles: Value,
    /// Principal kind discriminant: `Human` or `Workload`.
    pub principal_kind: String,
    /// JWKS key cache TTL in seconds.
    pub jwks_ttl_secs: i64,
    /// Encrypted client secret (`nonce ‖ ciphertext`), or `None`.
    pub client_secret_enc: Option<Vec<u8>>,
}

/// Return all trusted OIDC issuer rows for the current tenant.
///
/// RLS on `TenantConn` scopes the result to the bound tenant. An empty `Vec`
/// means the tenant has no configured issuers; this is not an error.
///
/// `client_secret_enc` bytes are returned verbatim — the 12-byte AES-GCM
/// nonce is the leading prefix; callers must not trim or text-decode the value.
pub async fn trusted_issuers_for_tenant(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<TrustedIssuerRow>, sqlx::Error> {
    sqlx::query_as::<_, TrustedIssuerRow>(TRUSTED_ISSUERS_FOR_TENANT_SQL)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Insert or update a trusted issuer for the current tenant.
///
/// `data_tenant_id` is taken from the bound [`TenantConn`]; the write targets
/// the bound tenant only. On `(data_tenant_id, issuer_url)` conflict every
/// mutable column is overwritten so re-seeding from config is idempotent.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the upsert.
pub async fn upsert_trusted_issuer(
    conn: &mut TenantConn<'_>,
    issuer: &TrustedIssuerWrite,
) -> Result<(), sqlx::Error> {
    sqlx::query(UPSERT_TRUSTED_ISSUER_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(&issuer.issuer_url)
        .bind(&issuer.jwks_uri)
        .bind(&issuer.expected_audience)
        .bind(&issuer.client_id)
        .bind(&issuer.client_auth)
        .bind(&issuer.claim_mapping)
        .bind(&issuer.group_role_map)
        .bind(&issuer.default_roles)
        .bind(&issuer.principal_kind)
        .bind(issuer.jwks_ttl_secs)
        .bind(issuer.client_secret_enc.as_deref())
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Insert a trusted issuer for the current tenant, failing on conflict.
///
/// Unlike [`upsert_trusted_issuer`] this is a plain `INSERT`: a duplicate
/// `(data_tenant_id, issuer_url)` raises a unique-violation (`23505`) so the
/// admin CRUD path can map it to a `409` conflict rather than silently
/// overwriting an existing issuer. `data_tenant_id` is bound from the
/// [`TenantConn`]; the write targets the bound tenant only.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert (unique violation on a
/// duplicate issuer, or any other database error).
pub async fn insert_trusted_issuer(
    conn: &mut TenantConn<'_>,
    issuer: &TrustedIssuerWrite,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_TRUSTED_ISSUER_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(&issuer.issuer_url)
        .bind(&issuer.jwks_uri)
        .bind(&issuer.expected_audience)
        .bind(&issuer.client_id)
        .bind(&issuer.client_auth)
        .bind(&issuer.claim_mapping)
        .bind(&issuer.group_role_map)
        .bind(&issuer.default_roles)
        .bind(&issuer.principal_kind)
        .bind(issuer.jwks_ttl_secs)
        .bind(issuer.client_secret_enc.as_deref())
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Fetch one trusted issuer row by URL for the current tenant.
///
/// RLS on `TenantConn` scopes the lookup to the bound tenant. Returns `None`
/// when no issuer with that URL exists for the tenant. `client_secret_enc` is
/// returned byte-for-byte; callers must not trim or text-decode it.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn trusted_issuer_by_url(
    conn: &mut TenantConn<'_>,
    issuer_url: &str,
) -> Result<Option<TrustedIssuerRow>, sqlx::Error> {
    sqlx::query_as::<_, TrustedIssuerRow>(TRUSTED_ISSUER_BY_URL_SQL)
        .bind(issuer_url)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Delete a trusted issuer by URL for the current tenant.
///
/// Returns the number of rows removed: `0` means no such issuer existed for the
/// tenant (the admin path maps this to `404`). A delete blocked by a live
/// workload binding raises a foreign-key violation (`23503`, `ON DELETE
/// RESTRICT` from migration 02), which the admin path maps to `409`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete (including the FK
/// restrict when bindings still reference the issuer).
pub async fn delete_trusted_issuer(
    conn: &mut TenantConn<'_>,
    issuer_url: &str,
) -> Result<u64, sqlx::Error> {
    sqlx::query(DELETE_TRUSTED_ISSUER_SQL)
        .bind(issuer_url)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected())
}

/// Report whether a trusted issuer row exists for the current tenant.
///
/// Used by the boot seeder to keep an unreachable IdP non-fatal on restart when
/// the issuer was already seeded.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn trusted_issuer_exists(
    conn: &mut TenantConn<'_>,
    issuer_url: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(TRUSTED_ISSUER_EXISTS_SQL)
        .bind(issuer_url)
        .fetch_one(&mut **conn.transaction())
        .await
}

#[cfg(test)]
mod tests {
    use super::{
        DELETE_TRUSTED_ISSUER_SQL, INSERT_TRUSTED_ISSUER_SQL, TRUSTED_ISSUER_BY_URL_SQL,
        TRUSTED_ISSUER_EXISTS_SQL, TRUSTED_ISSUERS_FOR_TENANT_SQL, UPSERT_TRUSTED_ISSUER_SQL,
    };

    #[test]
    fn plain_insert_has_no_on_conflict_clause() {
        // A duplicate must raise 23505, not silently upsert.
        assert!(INSERT_TRUSTED_ISSUER_SQL.contains("INSERT INTO wyrd.auth_trusted_issuers"));
        assert!(!INSERT_TRUSTED_ISSUER_SQL.contains("ON CONFLICT"));
    }

    #[test]
    fn delete_and_get_are_scoped_by_issuer_url_only() {
        // RLS supplies the tenant predicate; the statement keys on issuer_url.
        assert!(DELETE_TRUSTED_ISSUER_SQL.contains("DELETE FROM wyrd.auth_trusted_issuers"));
        assert!(DELETE_TRUSTED_ISSUER_SQL.contains("issuer_url = $1"));
        assert!(!DELETE_TRUSTED_ISSUER_SQL.contains("data_tenant_id ="));
        assert!(TRUSTED_ISSUER_BY_URL_SQL.contains("client_secret_enc"));
        assert!(TRUSTED_ISSUER_BY_URL_SQL.contains("issuer_url = $1"));
    }

    #[test]
    fn upsert_targets_composite_key_and_overwrites_columns() {
        assert!(UPSERT_TRUSTED_ISSUER_SQL.contains("INSERT INTO wyrd.auth_trusted_issuers"));
        assert!(UPSERT_TRUSTED_ISSUER_SQL.contains("ON CONFLICT (data_tenant_id, issuer_url)"));
        assert!(
            UPSERT_TRUSTED_ISSUER_SQL.contains("client_secret_enc = EXCLUDED.client_secret_enc")
        );
        assert!(UPSERT_TRUSTED_ISSUER_SQL.contains("claim_mapping = EXCLUDED.claim_mapping"));
        assert!(UPSERT_TRUSTED_ISSUER_SQL.contains("updated_at = now()"));
    }

    #[test]
    fn exists_is_scoped_to_current_tenant() {
        assert!(TRUSTED_ISSUER_EXISTS_SQL.contains("data_tenant_id = wyrd.current_tenant()"));
        assert!(TRUSTED_ISSUER_EXISTS_SQL.contains("issuer_url = $1"));
    }

    #[test]
    fn trusted_issuers_for_tenant_selects_all_columns() {
        assert!(TRUSTED_ISSUERS_FOR_TENANT_SQL.contains("client_secret_enc"));
        assert!(TRUSTED_ISSUERS_FOR_TENANT_SQL.contains("claim_mapping"));
        assert!(TRUSTED_ISSUERS_FOR_TENANT_SQL.contains("group_role_map"));
        assert!(TRUSTED_ISSUERS_FOR_TENANT_SQL.contains("default_roles"));
        assert!(TRUSTED_ISSUERS_FOR_TENANT_SQL.contains("principal_kind"));
        assert!(TRUSTED_ISSUERS_FOR_TENANT_SQL.contains("jwks_ttl_secs"));
        // No per-query tenant predicate — RLS handles isolation.
        assert!(!TRUSTED_ISSUERS_FOR_TENANT_SQL.contains("data_tenant_id ="));
    }
}
