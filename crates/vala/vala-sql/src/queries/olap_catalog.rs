//! Tenant-scoped reads + writes for the Vala OLAP catalog control table
//! `vala.bifrost_tables`.
//! All callers must pass a [`TenantConn`] — the wyrd-sql RLS bind enforces
//! tenant scope for wyrd_app-role paths.
// raw-query grep allowlist: olap control tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::queries::oracle_reader_authority::{
    BIFROST_CATALOG_NAME, BifrostTableMaintenanceAuthority,
};
use crate::row_types::olap_catalog::BifrostTableRow;
use crate::row_types::oracle_reader_authority::TableAuthorityIdentity;

// ── vala.bifrost_tables ──────────────────────────────────────────────────────

/// Insert or update a Bifrost table registration for the current tenant.
///
/// Registration also writes the table's
/// `vala.bifrost_table_maintenance_authority` row in the same transaction, so
/// the one serialization boundary reader protection and snapshot expiration
/// both lock exists from the moment the table does. That is what lets every
/// later consumer treat a missing authority row as an identity failure rather
/// than falling back to an advisory lock.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or an RLS policy rejects the row,
/// and [`SqlError::InvariantViolation`] when `fqn` carries no namespace segment
/// and therefore cannot name a table inside one.
pub async fn upsert_table(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    fqn: &str,
    fingerprint: &[u8; 32],
    physical_layout: &serde_json::Value,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.bifrost_tables
            (data_tenant_id, table_uid, fqn, fingerprint, physical_layout, origin, actor)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, 'system', 'system')
        ON CONFLICT (data_tenant_id, table_uid)
        DO UPDATE SET
            fqn             = EXCLUDED.fqn,
            fingerprint     = EXCLUDED.fingerprint,
            physical_layout = EXCLUDED.physical_layout,
            updated_at      = now()
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(fqn)
    .bind(fingerprint.as_slice())
    .bind(physical_layout)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    let Some((namespace_name, table_name)) = fqn.rsplit_once('.') else {
        return Err(SqlError::InvariantViolation {
            detail: "Bifrost table registration has no namespace segment".to_owned(),
        });
    };
    let identity = TableAuthorityIdentity {
        tenant: conn.data_tenant_id(),
        table_uid: *table_uid,
        catalog_name: BIFROST_CATALOG_NAME.to_owned(),
        namespace_name: namespace_name.to_owned(),
        table_name: table_name.to_owned(),
    };
    BifrostTableMaintenanceAuthority::new(conn)
        .register(&identity)
        .await?;
    Ok(())
}

/// Look up a Bifrost table registration by FQN for the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn get_by_fqn(
    conn: &mut TenantConn<'_>,
    fqn: &str,
) -> Result<Option<BifrostTableRow>, SqlError> {
    sqlx::query_as::<_, BifrostTableRow>(
        r#"
        SELECT data_tenant_id, table_uid, fqn, fingerprint, status,
               physical_layout, registered_at, updated_at, origin, actor
          FROM vala.bifrost_tables
         WHERE fqn = $1
        "#,
    )
    .bind(fqn)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// List all `vala.bifrost_tables` rows visible to the current tenant bind.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn list_tables_for_tenant(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<BifrostTableRow>, SqlError> {
    sqlx::query_as::<_, BifrostTableRow>(
        r#"
        SELECT data_tenant_id, table_uid, fqn, fingerprint, status,
               physical_layout, registered_at, updated_at, origin, actor
          FROM vala.bifrost_tables
        "#,
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}
