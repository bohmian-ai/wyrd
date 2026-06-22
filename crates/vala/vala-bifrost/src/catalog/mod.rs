use std::collections::HashMap;
use std::sync::Arc;

use arrow::datatypes::Field;
use iceberg::Catalog as _;
use iceberg::spec::FormatVersion;
use iceberg::TableCreation;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;

use crate::catalog::namespaces::BifrostNamespace;
use crate::catalog::partition_spec::build_partition_spec;
use crate::error::BifrostError;
use crate::provider::WyrdTableProvider;
use crate::schema::system_columns::with_system_columns;
use crate::types::{PartitionTransform, SchemaFingerprint, TableScope, TableUid};
use crate::writer::coordinator::spawn_commit_coordinator;
use crate::writer::TableWriterHandle;

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
        partition_columns: &[(String, PartitionTransform)],
    ) -> Result<TableUid, BifrostError> {
        self.ensure_namespace(ns).await?;

        let all_fields = with_system_columns(user_fields, scope);
        let arrow_schema = arrow::datatypes::Schema::new(all_fields);
        let iceberg_schema =
            iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&arrow_schema)
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

        let mut conn = vala_sql::TenantConn::acquire(
            &self.pool,
            wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
        )
        .await
        .map_err(BifrostError::Sql)?;

        vala_sql::queries::olap_catalog::upsert_table(
            &mut conn,
            table_uid.as_bytes(),
            &fqn,
            &fingerprint.0,
            match scope {
                TableScope::TenantOwned => "tenant_owned",
                TableScope::SystemShared => "system_shared",
            },
            &[],
        )
        .await
        .map_err(BifrostError::Sql)?;

        conn.commit().await.map_err(BifrostError::Sql)?;

        Ok(table_uid)
    }

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

        let conn_row = {
            let mut conn = vala_sql::TenantConn::acquire(
                &self.pool,
                wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
            )
            .await
            .map_err(BifrostError::Sql)?;
            let row = vala_sql::queries::olap_catalog::get_by_fqn(&mut conn, &fqn)
                .await
                .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;
            row
        };

        let row = conn_row.ok_or_else(|| BifrostError::TableNotFound(fqn))?;
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
            scope,
            tenant,
        );

        Ok(handle)
    }

    pub async fn provider(
        &self,
        ns: BifrostNamespace,
        name: &str,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<WyrdTableProvider, BifrostError> {
        let table_ident = iceberg::TableIdent::new(ns.to_namespace_ident(), name.to_string());
        let table = self.catalog.load_table(&table_ident).await?;

        let scope = {
            let fqn = format!("{}.{}", ns.as_str(), name);
            let mut conn = vala_sql::TenantConn::acquire(
                &self.pool,
                wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
            )
            .await
            .map_err(BifrostError::Sql)?;
            let row = vala_sql::queries::olap_catalog::get_by_fqn(&mut conn, &fqn)
                .await
                .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;
            match row.as_ref().map(|r| r.scope.as_str()) {
                Some("system_shared") => TableScope::SystemShared,
                _ => TableScope::TenantOwned,
            }
        };

        WyrdTableProvider::try_new(table, scope, tenant)
            .await
            .map_err(BifrostError::DataFusion)
    }
}
