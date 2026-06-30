//! Query slots for `wyrd.auth_trusted_issuers`.
//!
//! All functions take `&mut TenantConn<'_>`. Postgres RLS enforces tenant
//! isolation; no per-query `data_tenant_id` predicate is needed because the
//! RLS policy (`data_tenant_id = wyrd.current_tenant()`) on `TenantConn`
//! scopes every read to the bound tenant.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use crate::TenantConn;
use crate::row_types::auth::TrustedIssuerRow;

const TRUSTED_ISSUERS_FOR_TENANT_SQL: &str = r#"
    SELECT data_tenant_id, issuer_url, jwks_uri, expected_audience, client_id,
           client_auth, claim_mapping, group_role_map, default_roles,
           principal_kind, jwks_ttl_secs, client_secret_enc,
           created_at, updated_at
      FROM wyrd.auth_trusted_issuers
"#;

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

#[cfg(test)]
mod tests {
    use super::TRUSTED_ISSUERS_FOR_TENANT_SQL;

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
