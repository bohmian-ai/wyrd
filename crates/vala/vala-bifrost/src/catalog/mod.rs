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
use crate::registry::{CachedMeta, Registry, RegistryKey};
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
    registry: Arc<Registry>,
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

        let registry = Arc::new(Registry::new(pool.clone()));

        let this = Self {
            catalog: Arc::new(catalog),
            pool,
            storage_factory,
            storage_props,
            warehouse,
            registry,
        };

        if let Err(e) = this.startup_recovery().await {
            tracing::warn!(error = %e, "startup recovery pass failed (best-effort)");
        }

        Ok(this)
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

    /// Resolve a table's cached Iceberg metadata.
    ///
    /// Cheap when the `refresh_epochs` entry matches the cached entry's epoch
    /// (one narrow PK-indexed SELECT + no Iceberg load). On a miss the catalog
    /// loads the table from Iceberg and stores it in the registry.
    pub(crate) async fn get(
        &self,
        ns: BifrostNamespace,
        name: &str,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<Arc<CachedMeta>, BifrostError> {
        let fqn = format!("{}.{}", ns.as_str(), name);

        // Two-step lookup: try tenant bind (TenantOwned), then SYSTEM_OWNER (SystemShared).
        let (row, owner) = if let Some(row) = self.lookup_table_row(&fqn, tenant).await? {
            let scope = TableScope::from_db_str(&row.scope)?;
            let owner = scope.control_bind(tenant);
            (row, owner)
        } else {
            let row = self
                .lookup_table_row(&fqn, wyrd_spec::ids::DataTenantId::SYSTEM_OWNER)
                .await?
                .ok_or_else(|| BifrostError::TableNotFound(fqn.clone()))?;
            (row, wyrd_spec::ids::DataTenantId::SYSTEM_OWNER)
        };

        let table_uid = TableUid(
            row.table_uid
                .as_slice()
                .try_into()
                .map_err(|_| BifrostError::Internal("table_uid length mismatch".to_string()))?,
        );
        let key = RegistryKey { owner, table_uid };

        let epoch = self.registry.current_epoch(&key).await?;

        if let Some(cached) = self.registry.lookup_cached(&key, epoch) {
            return Ok(cached);
        }

        let table_ident = iceberg::TableIdent::new(ns.to_namespace_ident(), name.to_string());
        let table = self.catalog.load_table(&table_ident).await?;

        let meta = Arc::new(CachedMeta {
            row,
            iceberg_table: Arc::new(table),
            refresh_epoch: epoch,
        });
        self.registry.store(key, Arc::clone(&meta));
        Ok(meta)
    }

    /// List tables visible to `tenant` — both `TenantOwned` and `SystemShared` rows.
    /// Engine-internal; no stable public contract yet.
    #[allow(dead_code)]
    pub(crate) async fn list_tables(
        &self,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<Vec<vala_sql::row_types::olap_catalog::BifrostTableRow>, BifrostError> {
        self.registry.list_for_tenant(tenant).await
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
            Arc::clone(&self.registry),
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
        let meta = self.get(ns, name, tenant).await?;
        let scope = TableScope::from_db_str(&meta.row.scope)?;
        let table = (*meta.iceberg_table).clone();
        WyrdTableProvider::try_new(table, scope, tenant)
            .await
            .map_err(BifrostError::DataFusion)
    }

    /// Best-effort startup recovery pass.
    ///
    /// Claims stale `precommit` rows (lease absent/expired) via the SECURITY
    /// DEFINER `vala.claim_stale_precommits` function and reconciles each by
    /// scanning Iceberg snapshot summaries for `wyrd_batch_id`. Runs on the
    /// existing pool; in tests the pool is a superuser and can call the DEFINER
    /// functions. In production, the `wyrd_app` role is not granted EXECUTE, so
    /// this pass will silently skip (logged at WARN).
    async fn startup_recovery(&self) -> Result<(), BifrostError> {
        use crate::writer::commit::WRITER_INSTANCE;

        let engine_owner = *WRITER_INSTANCE;

        let mut conn =
            vala_sql::TenantConn::acquire(&self.pool, wyrd_spec::ids::DataTenantId::SYSTEM_OWNER)
                .await
                .map_err(BifrostError::Sql)?;
        let claimed =
            vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, engine_owner, 100)
                .await
                .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;

        for row in claimed {
            if let Err(e) = self.recover_claimed_row(&row).await {
                tracing::warn!(
                    fqn = %row.fqn,
                    error = %e,
                    "recovery scan failed for claimed precommit row"
                );
            }
        }

        Ok(())
    }

    async fn recover_claimed_row(
        &self,
        row: &vala_sql::row_types::olap_catalog::ClaimedPrecommitRow,
    ) -> Result<(), BifrostError> {
        let table_uid: [u8; 16] =
            row.table_uid.as_slice().try_into().map_err(|_| {
                BifrostError::Internal("recovery: table_uid length mismatch".into())
            })?;
        let batch_id: [u8; 16] = row
            .batch_id
            .as_slice()
            .try_into()
            .map_err(|_| BifrostError::Internal("recovery: batch_id length mismatch".into()))?;
        let fencing_token = row.fencing_token;
        let batch_id_hex = uuid::Uuid::from_bytes(batch_id).simple().to_string();

        let table_ident = fqn_to_table_ident(&row.fqn)?;

        // SECURITY DEFINER recovery routines bypass RLS regardless of the bind tenant.
        // Use SYSTEM_OWNER as a stable, always-valid bind for all recovery connections.
        let recovery_bind = wyrd_spec::ids::DataTenantId::SYSTEM_OWNER;

        let load_result = self.catalog.load_table(&table_ident).await;

        let table = match load_result {
            Err(e) => {
                tracing::warn!(fqn = %row.fqn, error = %e, "recovery: iceberg load failed");
                let mut conn = vala_sql::TenantConn::acquire(&self.pool, recovery_bind)
                    .await
                    .map_err(BifrostError::Sql)?;
                vala_sql::queries::olap_catalog::mark_recovery_scan_failed(
                    &mut conn,
                    &table_uid,
                    &batch_id,
                    fencing_token,
                    &e.to_string(),
                )
                .await
                .map_err(BifrostError::Sql)?;
                conn.commit().await.map_err(BifrostError::Sql)?;
                return Ok(());
            }
            Ok(t) => t,
        };

        let snapshot_id = find_snapshot_by_batch_id(&table, &batch_id_hex);

        let mut conn = vala_sql::TenantConn::acquire(&self.pool, recovery_bind)
            .await
            .map_err(BifrostError::Sql)?;

        match snapshot_id {
            Some(sid) => {
                vala_sql::queries::olap_catalog::finalize_recovered_committed(
                    &mut conn,
                    &table_uid,
                    &batch_id,
                    sid,
                    fencing_token,
                )
                .await
                .map_err(BifrostError::Sql)?;
            }
            None => {
                vala_sql::queries::olap_catalog::finalize_recovered_aborted(
                    &mut conn,
                    &table_uid,
                    &batch_id,
                    fencing_token,
                    "snapshot_absent",
                )
                .await
                .map_err(BifrostError::Sql)?;
            }
        }

        conn.commit().await.map_err(BifrostError::Sql)?;
        Ok(())
    }
}

/// Parse a Bifrost FQN into an Iceberg `TableIdent`.
///
/// FQN format: `{ns.as_str()}.{table_name}`, where `ns.as_str()` may contain
/// dots (e.g. "vala.bifrost"). Match the known namespace prefix to extract the
/// table name — avoids relying on the buggy `split_part('.', N)` in the SQL.
fn fqn_to_table_ident(fqn: &str) -> Result<iceberg::TableIdent, BifrostError> {
    let namespaces = [
        BifrostNamespace::System,
        BifrostNamespace::Bifrost,
        BifrostNamespace::Traces,
        BifrostNamespace::Eval,
    ];
    for ns in namespaces {
        let prefix = format!("{}.", ns.as_str());
        if let Some(table_name) = fqn.strip_prefix(&prefix)
            && !table_name.is_empty()
            && !table_name.contains('.')
        {
            return Ok(iceberg::TableIdent::new(
                ns.to_namespace_ident(),
                table_name.to_string(),
            ));
        }
    }
    Err(BifrostError::MetadataMismatch(format!(
        "cannot parse fqn into known namespace: {fqn}"
    )))
}

/// Scan an Iceberg table's snapshot history for a snapshot whose summary
/// carries `wyrd_batch_id == batch_id_hex`. Returns the first matching
/// `snapshot_id`, or `None` if no snapshot matches.
fn find_snapshot_by_batch_id(table: &iceberg::table::Table, batch_id_hex: &str) -> Option<i64> {
    for snapshot in table.metadata().snapshots() {
        let props = &snapshot.summary().additional_properties;
        if props.get("wyrd_batch_id").map(String::as_str) == Some(batch_id_hex) {
            return Some(snapshot.snapshot_id());
        }
    }
    None
}
