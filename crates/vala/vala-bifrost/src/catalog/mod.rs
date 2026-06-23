use std::collections::HashMap;
use std::sync::Arc;

use arrow::datatypes::Field;
use iceberg::Catalog as _;
use iceberg::TableCreation;
use iceberg::spec::FormatVersion;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;

use crate::catalog::namespaces::BifrostNamespace;
use crate::catalog::partition_spec::build_partition_spec;
use crate::error::BifrostError;
use crate::provider::WyrdTableProvider;
use crate::schema::system_columns::with_system_columns;
use crate::types::{PartitionTransform, SchemaFingerprint, TableScope, TableUid};
use crate::writer::TableWriterHandle;
use crate::writer::coordinator::spawn_commit_coordinator;

pub mod iceberg_sql;
pub mod namespaces;
pub mod partition_spec;

#[allow(dead_code)]
pub struct WyrdCatalog {
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    storage_factory: Arc<dyn iceberg::io::StorageFactory>,
    storage_props: HashMap<String, String>,
    warehouse: String,
}

impl WyrdCatalog {
    pub async fn new(
        catalog_uri: &str,
        warehouse: impl Into<String>,
        pool: Arc<PgPool>,
        storage_factory: Arc<dyn iceberg::io::StorageFactory>,
        storage_props: HashMap<String, String>,
    ) -> Result<Self, BifrostError> {
        let warehouse = warehouse.into();
        let catalog = iceberg_sql::build_catalog(
            catalog_uri,
            &warehouse,
            storage_factory.clone(),
            storage_props.clone(),
        )
        .await?;

        Ok(Self {
            catalog: Arc::new(catalog),
            pool,
            storage_factory,
            storage_props,
            warehouse,
        })
    }

    pub async fn ensure_namespace(&self, ns: BifrostNamespace) -> Result<(), BifrostError> {
        let ident = ns.to_namespace_ident();
        if !self.catalog.namespace_exists(&ident).await? {
            self.catalog
                .create_namespace(&ident, HashMap::new())
                .await?;
        }
        Ok(())
    }

    pub async fn create_table(
        &self,
        ns: BifrostNamespace,
        name: &str,
        user_fields: Vec<Field>,
        scope: TableScope,
        tenant: wyrd_spec::ids::DataTenantId,
        partition_columns: &[(String, PartitionTransform)],
    ) -> Result<TableUid, BifrostError> {
        self.ensure_namespace(ns).await?;

        let all_fields = with_system_columns(user_fields, scope);
        let arrow_schema = arrow::datatypes::Schema::new(all_fields);
        let iceberg_schema = iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&arrow_schema)
            .map_err(BifrostError::Iceberg)?;

        let partition_spec = if partition_columns.is_empty() {
            None
        } else {
            Some(build_partition_spec(&iceberg_schema, partition_columns)?)
        };

        let table_uid = TableUid::new_v7();
        let fingerprint = SchemaFingerprint::from_arrow_schema(&arrow_schema);
        let fqn = format!("{}.{}", ns.as_str(), name);

        let location = format!("{}/{}/{}", self.warehouse, ns.as_str(), name);

        let creation = match partition_spec {
            Some(spec) => TableCreation::builder()
                .name(name.to_string())
                .location(location)
                .schema(iceberg_schema)
                .format_version(FormatVersion::V2)
                .partition_spec(spec)
                .build(),
            None => TableCreation::builder()
                .name(name.to_string())
                .location(location)
                .schema(iceberg_schema)
                .format_version(FormatVersion::V2)
                .build(),
        };

        let namespace_ident = ns.to_namespace_ident();
        self.catalog
            .create_table(&namespace_ident, creation)
            .await?;

        let registration_tenant = scope.control_bind(tenant);

        let mut conn = vala_sql::TenantConn::acquire(&self.pool, registration_tenant)
            .await
            .map_err(BifrostError::Sql)?;

        vala_sql::queries::olap_catalog::upsert_table(
            &mut conn,
            table_uid.as_bytes(),
            &fqn,
            &fingerprint.0,
            scope.as_db_str(),
            &[],
        )
        .await
        .map_err(BifrostError::Sql)?;

        conn.commit().await.map_err(BifrostError::Sql)?;

        Ok(table_uid)
    }

    /// Read a `vala.bifrost_tables` registration under a specific control-plane
    /// RLS bind. The bind decides row visibility: `SystemShared` rows are only
    /// visible under `SYSTEM_OWNER`, `TenantOwned` rows only under their data tenant.
    async fn lookup_table_row(
        &self,
        fqn: &str,
        bind: wyrd_spec::ids::DataTenantId,
    ) -> Result<Option<vala_sql::row_types::olap_catalog::BifrostTableRow>, BifrostError> {
        let mut conn = vala_sql::TenantConn::acquire(&self.pool, bind)
            .await
            .map_err(BifrostError::Sql)?;
        let row = vala_sql::queries::olap_catalog::get_by_fqn(&mut conn, fqn)
            .await
            .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        Ok(row)
    }

    /// Open a writer for `tenant` (the authenticated **data tenant**, for both
    /// scopes). The control-plane RLS bind for the registration lookup — and for
    /// the commit coordinator's `vala.olap_commits` precommit/finalize rows — is
    /// derived from `scope` (C2/N-M12), never from the caller: `SystemShared` binds
    /// `SYSTEM_OWNER`, `TenantOwned` binds the data tenant. The data tenant itself is
    /// what gets server-stamped into `data_tenant_id` on `SystemShared` rows; the two
    /// values must never collapse.
    pub async fn writer(
        &self,
        ns: BifrostNamespace,
        name: &str,
        scope: TableScope,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<TableWriterHandle, BifrostError> {
        let table_ident = iceberg::TableIdent::new(ns.to_namespace_ident(), name.to_string());
        let table = self.catalog.load_table(&table_ident).await?;
        let fqn = format!("{}.{}", ns.as_str(), name);

        let row = self
            .lookup_table_row(&fqn, scope.control_bind(tenant))
            .await?
            .ok_or_else(|| BifrostError::TableNotFound(fqn.clone()))?;
        let table_uid = TableUid(
            row.table_uid
                .try_into()
                .map_err(|_| BifrostError::Internal("table_uid length mismatch".to_string()))?,
        );

        let handle = spawn_commit_coordinator(
            table,
            self.catalog.clone(),
            self.pool.clone(),
            table_uid,
            fqn,
            scope,
            tenant,
        );

        Ok(handle)
    }

    /// Drop an Iceberg table from both the SQL catalog and the Wyrd control tables.
    ///
    /// Ignores not-found errors. Routes the control-table cleanup through a
    /// `TenantConn` bound to `tenant` (the registration's owner: the data tenant
    /// for `TenantOwned`, `SYSTEM_OWNER` for `SystemShared`) so RLS sees the rows, and
    /// deletes in FK order via `delete_table`.
    ///
    /// This is an unconditionally destructive, ownership-free operation, so it is
    /// gated to test and bench builds and must never be reachable in production.
    #[cfg(any(test, feature = "bench-bin"))]
    pub async fn drop_table(
        &self,
        ns: BifrostNamespace,
        name: &str,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<(), BifrostError> {
        let fqn = format!("{}.{}", ns.as_str(), name);
        let table_ident = iceberg::TableIdent::new(ns.to_namespace_ident(), name.to_string());

        // Drop from Iceberg catalog — ignore not-found.
        let _ = self.catalog.drop_table(&table_ident).await;

        let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(BifrostError::Sql)?;
        vala_sql::queries::olap_catalog::delete_table(&mut conn, &fqn)
            .await
            .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;

        Ok(())
    }

    /// Open a read provider for `tenant` (the authenticated **data tenant**).
    ///
    /// Unlike `writer()`, the caller does not supply the scope — it must be
    /// *discovered* from the registration. The lookup is therefore two-step
    /// (MAJOR-3): try the data tenant's bind first (resolves `TenantOwned` rows
    /// under RLS), then `SYSTEM_OWNER` (resolves `SystemShared` rows). The resolved
    /// `data_tenant_id` filter on a `SystemShared` scan binds this data tenant; the
    /// control-plane binds above only decide which registration row is visible.
    pub async fn provider(
        &self,
        ns: BifrostNamespace,
        name: &str,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<WyrdTableProvider, BifrostError> {
        let table_ident = iceberg::TableIdent::new(ns.to_namespace_ident(), name.to_string());
        let table = self.catalog.load_table(&table_ident).await?;
        let fqn = format!("{}.{}", ns.as_str(), name);

        let row = match self.lookup_table_row(&fqn, tenant).await? {
            Some(row) => row,
            None => self
                .lookup_table_row(&fqn, wyrd_spec::ids::DataTenantId::SYSTEM_OWNER)
                .await?
                .ok_or_else(|| BifrostError::TableNotFound(fqn.clone()))?,
        };
        let scope = TableScope::from_db_str(row.scope.as_str())?;

        WyrdTableProvider::try_new(table, scope, tenant)
            .await
            .map_err(BifrostError::DataFusion)
    }
}
