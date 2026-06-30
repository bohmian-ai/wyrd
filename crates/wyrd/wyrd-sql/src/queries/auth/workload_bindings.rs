//! Query slots for `wyrd.auth_workload_bindings`.
//!
//! All functions take `&mut TenantConn<'_>`. Postgres RLS enforces tenant
//! isolation via `data_tenant_id = wyrd.current_tenant()`.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

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

#[cfg(test)]
mod tests {
    use super::WORKLOAD_BINDING_BY_SUBJECT_SQL;

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
