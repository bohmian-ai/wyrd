//! Query slots for `wyrd.auth_workload_bindings`.
//!
//! All functions take `&mut TenantConn<'_>`. Postgres RLS enforces tenant
//! isolation via `data_tenant_id = wyrd.current_tenant()`; the read and delete
//! statements additionally carry that predicate explicitly as defense in depth,
//! so a query can never match a row outside the bound tenant even if RLS were
//! misconfigured. Writes bind `data_tenant_id` from the [`TenantConn`].
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use serde_json::Value;

use crate::TenantConn;
use crate::row_types::auth::WorkloadBindingRow;

const WORKLOAD_BINDING_BY_SUBJECT_SQL: &str = r#"
    SELECT data_tenant_id, issuer_url, subject, audience, card_ref,
           created_at, updated_at
      FROM wyrd.auth_workload_bindings
     WHERE data_tenant_id = wyrd.current_tenant()
       AND issuer_url = $1
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

const INSERT_WORKLOAD_BINDING_SQL: &str = r#"
    INSERT INTO wyrd.auth_workload_bindings (
        data_tenant_id, issuer_url, subject, audience, card_ref
    ) VALUES ($1, $2, $3, $4, $5)
"#;

const WORKLOAD_BINDING_BY_KEY_SQL: &str = r#"
    SELECT data_tenant_id, issuer_url, subject, audience, card_ref,
           created_at, updated_at
      FROM wyrd.auth_workload_bindings
     WHERE data_tenant_id = wyrd.current_tenant()
       AND issuer_url = $1
       AND subject = $2
"#;

const WORKLOAD_BINDINGS_FOR_TENANT_SQL: &str = r#"
    SELECT data_tenant_id, issuer_url, subject, audience, card_ref,
           created_at, updated_at
      FROM wyrd.auth_workload_bindings
     WHERE data_tenant_id = wyrd.current_tenant()
       AND ($1::text IS NULL OR issuer_url = $1)
       AND ($2::text IS NULL OR subject = $2)
     ORDER BY issuer_url, subject
"#;

const DELETE_WORKLOAD_BINDING_SQL: &str = r#"
    DELETE FROM wyrd.auth_workload_bindings
     WHERE data_tenant_id = wyrd.current_tenant()
       AND issuer_url = $1
       AND subject = $2
"#;

const DELETE_WORKLOAD_BINDINGS_FOR_ISSUER_SQL: &str = r#"
    DELETE FROM wyrd.auth_workload_bindings
     WHERE data_tenant_id = wyrd.current_tenant()
       AND issuer_url = $1
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

/// Return workload binding rows for the current tenant, newest filter wins.
///
/// `issuer` and `subject` are optional exact-match filters: a `None` filter
/// matches every row (the `$N::text IS NULL OR ...` guard). RLS on `TenantConn`
/// scopes the result to the bound tenant; an empty `Vec` means the tenant has no
/// matching bindings and is not an error.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn workload_bindings_for_tenant(
    conn: &mut TenantConn<'_>,
    issuer: Option<&str>,
    subject: Option<&str>,
) -> Result<Vec<WorkloadBindingRow>, sqlx::Error> {
    sqlx::query_as::<_, WorkloadBindingRow>(WORKLOAD_BINDINGS_FOR_TENANT_SQL)
        .bind(issuer)
        .bind(subject)
        .fetch_all(&mut **conn.transaction())
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

/// Insert a workload binding for the current tenant, failing on conflict.
///
/// Unlike [`upsert_workload_binding`] this is a plain `INSERT`: a duplicate
/// `(data_tenant_id, issuer_url, subject)` raises a unique-violation (`23505`)
/// so the admin CRUD path can map it to a `409` conflict. The referenced
/// trusted issuer must already exist or Postgres raises a foreign-key violation
/// (`23503`). `data_tenant_id` is bound from the [`TenantConn`].
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert (unique violation on a
/// duplicate binding, or FK violation when the issuer has not been created).
pub async fn insert_workload_binding(
    conn: &mut TenantConn<'_>,
    binding: &WorkloadBindingWrite,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_WORKLOAD_BINDING_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(&binding.issuer_url)
        .bind(&binding.subject)
        .bind(&binding.audience)
        .bind(&binding.card_ref)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Fetch one workload binding by its `(issuer, subject)` key for the tenant.
///
/// The natural primary key is `(data_tenant_id, issuer_url, subject)`, so this
/// returns at most one row. Unlike [`workload_binding_by_subject`] there is no
/// audience precedence: the admin GET addresses an exact binding. Returns
/// `None` when no binding exists for the tenant.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn workload_binding_by_key(
    conn: &mut TenantConn<'_>,
    issuer_url: &str,
    subject: &str,
) -> Result<Option<WorkloadBindingRow>, sqlx::Error> {
    sqlx::query_as::<_, WorkloadBindingRow>(WORKLOAD_BINDING_BY_KEY_SQL)
        .bind(issuer_url)
        .bind(subject)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Delete one workload binding by its `(issuer, subject)` key for the tenant.
///
/// Returns the number of rows removed: `0` means no such binding existed (the
/// admin path maps this to `404`).
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn delete_workload_binding(
    conn: &mut TenantConn<'_>,
    issuer_url: &str,
    subject: &str,
) -> Result<u64, sqlx::Error> {
    sqlx::query(DELETE_WORKLOAD_BINDING_SQL)
        .bind(issuer_url)
        .bind(subject)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected())
}

/// Delete every workload binding for an issuer in the current tenant.
///
/// This is the `--cascade` path: removing the bindings that reference an issuer
/// so the issuer itself can then be deleted without tripping the FK `ON DELETE
/// RESTRICT`. Returns the number of bindings removed.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn delete_workload_bindings_for_issuer(
    conn: &mut TenantConn<'_>,
    issuer_url: &str,
) -> Result<u64, sqlx::Error> {
    sqlx::query(DELETE_WORKLOAD_BINDINGS_FOR_ISSUER_SQL)
        .bind(issuer_url)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::{
        DELETE_WORKLOAD_BINDING_SQL, DELETE_WORKLOAD_BINDINGS_FOR_ISSUER_SQL,
        INSERT_WORKLOAD_BINDING_SQL, UPSERT_WORKLOAD_BINDING_SQL, WORKLOAD_BINDING_BY_KEY_SQL,
        WORKLOAD_BINDING_BY_SUBJECT_SQL, WORKLOAD_BINDINGS_FOR_TENANT_SQL,
    };

    #[test]
    fn list_filters_are_optional_via_null_guard() {
        // Each filter is an exact match only when bound; a NULL bind matches all.
        assert!(WORKLOAD_BINDINGS_FOR_TENANT_SQL.contains("$1::text IS NULL OR issuer_url = $1"));
        assert!(WORKLOAD_BINDINGS_FOR_TENANT_SQL.contains("$2::text IS NULL OR subject = $2"));
        assert!(WORKLOAD_BINDINGS_FOR_TENANT_SQL.contains("ORDER BY issuer_url, subject"));
    }

    #[test]
    fn plain_insert_has_no_on_conflict_clause() {
        assert!(INSERT_WORKLOAD_BINDING_SQL.contains("INSERT INTO wyrd.auth_workload_bindings"));
        assert!(!INSERT_WORKLOAD_BINDING_SQL.contains("ON CONFLICT"));
    }

    #[test]
    fn by_key_addresses_exact_binding_without_audience_precedence() {
        assert!(WORKLOAD_BINDING_BY_KEY_SQL.contains("issuer_url = $1"));
        assert!(WORKLOAD_BINDING_BY_KEY_SQL.contains("subject = $2"));
        assert!(!WORKLOAD_BINDING_BY_KEY_SQL.contains("ORDER BY"));
    }

    #[test]
    fn delete_statements_key_on_issuer_and_optional_subject() {
        assert!(DELETE_WORKLOAD_BINDING_SQL.contains("issuer_url = $1"));
        assert!(DELETE_WORKLOAD_BINDING_SQL.contains("subject = $2"));
        assert!(DELETE_WORKLOAD_BINDINGS_FOR_ISSUER_SQL.contains("issuer_url = $1"));
        assert!(!DELETE_WORKLOAD_BINDINGS_FOR_ISSUER_SQL.contains("subject"));
    }

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
    fn read_and_delete_statements_carry_explicit_tenant_predicate() {
        // Defense in depth on top of RLS: every filtering statement anchors on
        // the current tenant so a row outside the bound tenant can never match.
        for sql in [
            WORKLOAD_BINDING_BY_SUBJECT_SQL,
            WORKLOAD_BINDING_BY_KEY_SQL,
            WORKLOAD_BINDINGS_FOR_TENANT_SQL,
            DELETE_WORKLOAD_BINDING_SQL,
            DELETE_WORKLOAD_BINDINGS_FOR_ISSUER_SQL,
        ] {
            assert!(sql.contains("data_tenant_id = wyrd.current_tenant()"));
        }
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
