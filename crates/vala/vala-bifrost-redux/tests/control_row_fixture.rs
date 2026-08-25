//! Registers the `vala.bifrost_tables` control row an integration fixture needs
//! before Scribe will resolve a write recipe.
//!
//! The seal path re-resolves the stored `physical_layout` for every sealed
//! generation, so a fixture that drives Scribe directly — without going through
//! `BifrostCatalog::create_table` — must still publish the same canonical
//! layout the catalog would have written. This module owns that one insert so
//! no test file re-derives it.

use arrow::datatypes::Schema;
use vala_bifrost_redux::catalog::layout::PhysicalLayout;
use wyrd_spec::DataTenantId;

/// Publishes the canonical control row for `fqn` if the tenant does not already
/// have one.
///
/// Built-in tables are registered from their own declared layout so the row is
/// byte-identical to what registration would have produced; any other table
/// resolves the omitted-declaration default.
///
/// # Panics
///
/// Panics when the tenant connection, the layout canonicalization, or the
/// control-row write fails, each of which is required to establish the fixture.
pub(crate) async fn register_control_row(
    postgres: &vala_sql::ValaPostgres,
    tenant: DataTenantId,
    namespace: &str,
    table_name: &str,
    schema: &Schema,
) {
    let fqn = format!("{namespace}.{table_name}");
    let layout = match vala_bifrost_redux::tables::builtin_table(namespace, table_name) {
        Some(definition) => PhysicalLayout::builtin(&fqn, schema, &(definition.physical_layout)())
            .expect("a built-in declaration always canonicalizes against its own schema"),
        None => PhysicalLayout::canonicalize(&fqn, schema, None)
            .expect("the fixture schema canonicalizes under the omitted-declaration default"),
    };
    let mut conn = postgres
        .tenant_conn(tenant)
        .await
        .expect("fixture tenant connection");
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT fqn FROM vala.bifrost_tables WHERE data_tenant_id = $1 AND fqn = $2",
    )
    .bind(tenant.as_uuid())
    .bind(&fqn)
    .fetch_optional(&mut **conn.transaction())
    .await
    .expect("fixture control-row lookup");
    if existing.is_some() {
        conn.commit().await.expect("release the lookup transaction");
        return;
    }
    vala_sql::queries::olap_catalog::upsert_table(
        &mut conn,
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, fqn.as_bytes()).as_bytes(),
        &fqn,
        &[0_u8; 32],
        &serde_json::to_value(layout.to_wire()).expect("canonical layout encodes"),
    )
    .await
    .expect("register the fixture control row");
    conn.commit().await.expect("commit the fixture control row");
}

/// Reports whether `name` is a server-owned managed column rather than a user
/// field, so a fixture can rebuild the user half of a batch schema.
pub(crate) fn is_managed_column(name: &str) -> bool {
    use wyrd_spec::vala::managed_columns::{
        CARD_REF, CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID,
    };
    name.starts_with("wyrd_")
        || matches!(
            name,
            DATA_TENANT_ID | RUN_ID | CARD_UID | CARD_REF | PRINCIPAL_ID
        )
}
