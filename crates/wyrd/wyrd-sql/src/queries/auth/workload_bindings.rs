//! Query slots for `wyrd.auth_workload_bindings`.
//!
//! All functions take `&mut TenantConn<'_>`. Postgres RLS enforces tenant
//! isolation via `data_tenant_id = wyrd.current_tenant()`.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use serde_json::Value;

use crate::TenantConn;
use crate::row_types::auth::WorkloadBindingRow;

const WORKLOAD_BINDING_BY_SUBJECT_SQL: &str = r#"
    SELECT data_tenant_id, issuer_url, subject, audience, card_ref,
           created_at, updated_at
      FROM wyrd.auth_workload_bindings
     WHERE issuer_url = $1
       AND subject = $2
       AND (audience = $3 OR audience IS NULL)
     ORDER BY CASE WHEN audience IS NOT NULL THEN 0 ELSE 1 END
     LIMIT 1
"#;

const UPSERT_WORKLOAD_BINDING_SQL: &str = r#"
    INSERT INTO wyrd.auth_workload_bindings (
        data_tenant_id, issuer_url, subject, audience, card_ref
    ) VALUES ($1, $2, $3, $4, $5)
    ON CONFLICT (data_tenant_id, issuer_url, subject) DO UPDATE SET
        audience = EXCLUDED.audience,
        card_ref = EXCLUDED.card_ref,
        updated_at = now()
"#;

/// Owned column values for an upsert into `wyrd.auth_workload_bindings`.
///
/// `data_tenant_id` is bound from the [`TenantConn`], never from this struct.
/// `card_ref` is a pre-encoded structured JSONB `serde_json::Value`.
#[derive(Debug, Clone)]
pub struct WorkloadBindingWrite {
    /// OIDC issuer URL (FK composite with `data_tenant_id`).
    pub issuer_url: String,
    /// Token subject claim that identifies this workload.
    pub subject: String,
    /// Optional audience override; `None` acts as a wildcard fallback.
    pub audience: Option<String>,
    /// Structured Card reference serialized to JSONB.
    pub card_ref: Value,
}

/// Look up a workload binding for the given `(issuer, subject)` pair.
///
/// When `audience` is `Some`, an audience-specific binding is preferred; if
/// none exists the row with a `NULL` audience is returned as a fallback. When
/// `audience` is `None`, only a `NULL`-audience row matches. This mirrors the
/// `WorkloadBindingRegistry::binding` precedence.
///
/// Returns `None` when no matching binding exists for the current tenant.
/// RLS on `TenantConn` ensures cross-tenant rows are never visible.
pub async fn workload_binding_by_subject(
    conn: &mut TenantConn<'_>,
    issuer: &str,
    subject: &str,
    audience: Option<&str>,
) -> Result<Option<WorkloadBindingRow>, sqlx::Error> {
    sqlx::query_as::<_, WorkloadBindingRow>(WORKLOAD_BINDING_BY_SUBJECT_SQL)
        .bind(issuer)
        .bind(subject)
        .bind(audience)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Insert or update a workload binding for the current tenant.
///
/// `data_tenant_id` is taken from the bound [`TenantConn`]. On
/// `(data_tenant_id, issuer_url, subject)` conflict the audience and card
/// reference are overwritten so re-seeding from config is idempotent. The
/// referenced trusted issuer row must already exist (FK `ON DELETE RESTRICT`).
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the upsert (including FK
/// violations when the issuer has not been seeded first).
pub async fn upsert_workload_binding(
    conn: &mut TenantConn<'_>,
    binding: &WorkloadBindingWrite,
) -> Result<(), sqlx::Error> {
    sqlx::query(UPSERT_WORKLOAD_BINDING_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(&binding.issuer_url)
        .bind(&binding.subject)
        .bind(&binding.audience)
        .bind(&binding.card_ref)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{UPSERT_WORKLOAD_BINDING_SQL, WORKLOAD_BINDING_BY_SUBJECT_SQL};

    #[test]
    fn upsert_targets_composite_key_and_overwrites_columns() {
        assert!(UPSERT_WORKLOAD_BINDING_SQL.contains("INSERT INTO wyrd.auth_workload_bindings"));
        assert!(
            UPSERT_WORKLOAD_BINDING_SQL
                .contains("ON CONFLICT (data_tenant_id, issuer_url, subject)")
        );
        assert!(UPSERT_WORKLOAD_BINDING_SQL.contains("card_ref = EXCLUDED.card_ref"));
        assert!(UPSERT_WORKLOAD_BINDING_SQL.contains("audience = EXCLUDED.audience"));
    }

    #[test]
    fn workload_binding_by_subject_prefers_audience_specific_row() {
        assert!(WORKLOAD_BINDING_BY_SUBJECT_SQL.contains("issuer_url = $1"));
        assert!(WORKLOAD_BINDING_BY_SUBJECT_SQL.contains("subject = $2"));
        // audience = $3 OR audience IS NULL — covers both exact and fallback.
        assert!(WORKLOAD_BINDING_BY_SUBJECT_SQL.contains("audience = $3"));
        assert!(WORKLOAD_BINDING_BY_SUBJECT_SQL.contains("audience IS NULL"));
        // ORDER BY puts the audience-specific row first.
        assert!(WORKLOAD_BINDING_BY_SUBJECT_SQL.contains("ORDER BY"));
        assert!(WORKLOAD_BINDING_BY_SUBJECT_SQL.contains("LIMIT 1"));
    }
}
