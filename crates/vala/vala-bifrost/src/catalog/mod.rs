use std::collections::HashMap;
use std::sync::Arc;

use arrow::datatypes::{Field, SchemaRef};
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
use crate::schema::fingerprint::fingerprint_user_fields;
use crate::schema::system_columns::with_system_columns;
use crate::tables::{DeclaredIndex, DomainTable, PayloadClass};
use crate::types::{PartitionTransform, SchemaFingerprint, TableScope, TableUid};
use crate::writer::TableWriterHandle;
use crate::writer::coordinator::spawn_commit_coordinator;
use wyrd_storage::settings::BackendConfig;

pub mod iceberg_sql;
pub mod namespaces;
pub mod partition_spec;
pub mod storage;
mod wire;

#[allow(dead_code)]
pub struct WyrdCatalog {
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    recovery_pool: Option<Arc<PgPool>>,
    storage_factory: Arc<dyn iceberg::io::StorageFactory>,
    storage_props: HashMap<String, String>,
    warehouse: String,
    registry: Arc<Registry>,
}

/// RAII guard that holds a Postgres transaction containing a transaction-scoped
/// advisory lock. The lock is released automatically when this guard drops and
/// sqlx rolls back the open transaction.
pub struct AdvisoryLockGuard {
    _tx: sqlx::Transaction<'static, sqlx::Postgres>,
}

impl WyrdCatalog {
    /// Construct the catalog and run a best-effort startup recovery pass.
    ///
    /// `recovery_pool` must be authenticated as `vala_recovery` (the role granted
    /// EXECUTE on the SECURITY DEFINER recovery routines). When `None`, recovery is
    /// disabled and startup proceeds without scanning stale precommit rows — the
    /// `pool` (`wyrd_app` in production) lacks EXECUTE on those routines, so it is
    /// never used for recovery.
    pub async fn new(
        catalog_uri: &str,
        backend: &BackendConfig,
        pool: Arc<PgPool>,
        recovery_pool: Option<Arc<PgPool>>,
    ) -> Result<Self, BifrostError> {
        let (storage_factory, storage_props) = storage::iceberg_storage_factory(backend);
        let warehouse = storage::warehouse_uri(backend);
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
            recovery_pool,
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

    /// Idempotently ensure a privileged `SystemShared` table exists.
    ///
    /// Used for engine-owned warehouse tables (e.g. the S3.C5 audit relay's
    /// `vala.system.audit_log`) that must be present before the first write.
    /// A no-op when the Iceberg table already exists; otherwise creates and
    /// registers it under `SYSTEM_OWNER`. Safe to call on every boot.
    pub async fn ensure_system_table(
        &self,
        ns: BifrostNamespace,
        name: &str,
        user_fields: Vec<Field>,
        partition_columns: &[(String, PartitionTransform)],
    ) -> Result<(), BifrostError> {
        self.ensure_namespace(ns).await?;
        let table_ident = iceberg::TableIdent::new(ns.to_namespace_ident(), name.to_string());
        if self.catalog.table_exists(&table_ident).await? {
            return Ok(());
        }
        self.create_table(
            ns,
            name,
            user_fields,
            TableScope::SystemShared,
            wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
            partition_columns,
            None,
        )
        .await?;
        Ok(())
    }

    /// `audit`, when present, appends one hash-chained `AuditEvent` row into the
    /// transactional outbox **in the same tx as the `vala.bifrost_tables`
    /// registration** (S3.C5): the control-table row and its audit record commit
    /// atomically, and an audit-append failure fails the registration closed.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_table(
        &self,
        ns: BifrostNamespace,
        name: &str,
        user_fields: Vec<Field>,
        scope: TableScope,
        tenant: wyrd_spec::ids::DataTenantId,
        partition_columns: &[(String, PartitionTransform)],
        audit: Option<wyrd_spec::vala::api::AuditEvent>,
    ) -> Result<TableUid, BifrostError> {
        // M6 reserved-name guard: a user field may not take a reserved system
        // (`wyrd_*`/`data_tenant_id`) or correlation (`card_uid`/`run_id`/`principal_id`) name —
        // the server stamps the former and carries the latter as cell values.
        wire::reject_reserved_field_names(&user_fields)?;

        self.ensure_namespace(ns).await?;

        // The fingerprint is user-fields-only (06 §3): the correlation
        // (`run_id`/`card_uid`/`principal_id`) and system columns are stripped before hashing,
        // so their presence in the physical schema does not perturb the value the
        // client caches and the server compares.
        let fingerprint = SchemaFingerprint::from_arrow_schema(&arrow::datatypes::Schema::new(
            user_fields.clone(),
        ));

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

        if let Some(event) = audit.as_ref() {
            vala_sql::queries::audit_outbox::append_audit(&mut conn, event)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "audit outbox append failed; refusing register");
                    BifrostError::AuditUnavailable("audit outbox append failed".to_string())
                })?;
        }

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

    /// List tables visible to `tenant` — both `TenantOwned` and `SystemShared` rows —
    /// as the Arrow-free public wire entries. Each stored row is mapped via
    /// [`wire::entry_from_row`]; a corrupt scope/status/fqn surfaces as a
    /// `WYRD_VALA_500_*`, never a silently dropped listing.
    pub async fn list_tables(
        &self,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<Vec<wyrd_spec::vala::api::BifrostTableEntry>, BifrostError> {
        let rows = self.registry.list_for_tenant(tenant).await?;
        rows.iter().map(wire::entry_from_row).collect()
    }

    /// Describe a single table for `tenant` — the public wire entry plus its
    /// projected field list.
    ///
    /// The `fields` surface user columns and the universal `card_uid`/`run_id`/`principal_id`
    /// correlation columns (the latter flagged `wyrd:column_class = correlation`,
    /// Decision E), while the server-stamped `wyrd_*`/`data_tenant_id` system
    /// columns are excluded — so `card_uid` is distinct from both user and system
    /// columns. Resolves the same two-step (`TenantOwned` then `SystemShared`) bind
    /// as `get`.
    pub async fn describe_table(
        &self,
        ns: BifrostNamespace,
        name: &str,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<wyrd_spec::vala::api::BifrostTableDescription, BifrostError> {
        let meta = self.get(ns, name, tenant).await?;
        let entry = wire::entry_from_row(&meta.row)?;

        let iceberg_schema = meta.iceberg_table.metadata().current_schema();
        let arrow_schema = iceberg::arrow::schema_to_arrow_schema(iceberg_schema)
            .map_err(BifrostError::Iceberg)?;
        let fields = wire::fields_from_stored_schema(&arrow_schema)?;

        Ok(wyrd_spec::vala::api::BifrostTableDescription { entry, fields })
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
            PayloadClass::Standard,
            &[],
            None,
        );

        Ok(handle)
    }

    /// Open a write handle for a pre-declared domain table `T`.
    ///
    /// Carries the table's `PayloadClass`, `SENSITIVE_PAYLOAD_COLUMNS`, and
    /// `entity_bounds_mapping` into the coordinator so redaction and best-effort
    /// entity-time-bounds upserts fire automatically on each commit.
    pub async fn typed_writer<T: DomainTable>(
        &self,
        scope: TableScope,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<TableWriterHandle, BifrostError> {
        let ns = BifrostNamespace::from_domain_namespace(T::NAMESPACE).ok_or_else(|| {
            BifrostError::Internal(format!("unknown namespace: {}", T::NAMESPACE))
        })?;
        let table_ident = iceberg::TableIdent::new(ns.to_namespace_ident(), T::NAME.to_string());
        let table = self.catalog.load_table(&table_ident).await?;
        let fqn = format!("{}.{}", ns.as_str(), T::NAME);

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
            T::PAYLOAD_CLASS,
            T::SENSITIVE_PAYLOAD_COLUMNS,
            T::entity_bounds_mapping(),
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

    // ─────────────────────────────────────────────────────────────────────
    // DomainTable registration support (task 02)
    // ─────────────────────────────────────────────────────────────────────

    /// Acquire a transaction-scoped Postgres advisory lock keyed by the table FQN
    /// hash. The lock is held for the duration of the guard's lifetime and released
    /// automatically when the guard drops (transaction rolled back).
    pub async fn advisory_lock_for(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<AdvisoryLockGuard, BifrostError> {
        let fqn = format!("{namespace}.{name}");
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BifrostError::Internal(format!("advisory lock begin: {e}")))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(&fqn)
            .execute(&mut *tx)
            .await
            .map_err(|e| BifrostError::Internal(format!("advisory lock: {e}")))?;
        Ok(AdvisoryLockGuard { _tx: tx })
    }

    /// Look up the stored schema fingerprint for a pre-declared domain table.
    /// Returns `None` when no control row exists (first-boot or split-brain).
    pub async fn domain_table_fingerprint(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<Option<[u8; 32]>, BifrostError> {
        let ns = BifrostNamespace::from_domain_namespace(namespace).ok_or_else(|| {
            BifrostError::MetadataMismatch(format!("unknown namespace: {namespace}"))
        })?;
        let fqn = format!("{}.{name}", ns.as_str());
        let mut conn =
            vala_sql::TenantConn::acquire(&self.pool, wyrd_spec::ids::DataTenantId::SYSTEM_OWNER)
                .await
                .map_err(BifrostError::Sql)?;
        let row = vala_sql::queries::olap_catalog::get_by_fqn(&mut conn, &fqn)
            .await
            .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        let Some(row) = row else { return Ok(None) };
        let fp: [u8; 32] = row
            .fingerprint
            .as_slice()
            .try_into()
            .map_err(|_| BifrostError::Internal("fingerprint length mismatch".to_string()))?;
        Ok(Some(fp))
    }

    /// Check whether the Iceberg table exists in the catalog.
    pub async fn iceberg_table_exists(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<bool, BifrostError> {
        let ns = BifrostNamespace::from_domain_namespace(namespace).ok_or_else(|| {
            BifrostError::MetadataMismatch(format!("unknown namespace: {namespace}"))
        })?;
        let ident = iceberg::TableIdent::new(ns.to_namespace_ident(), name.to_string());
        Ok(self.catalog.table_exists(&ident).await?)
    }

    /// Load the physical Arrow schema of an existing Iceberg table.
    pub async fn iceberg_physical_schema(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<SchemaRef, BifrostError> {
        let ns = BifrostNamespace::from_domain_namespace(namespace).ok_or_else(|| {
            BifrostError::MetadataMismatch(format!("unknown namespace: {namespace}"))
        })?;
        let ident = iceberg::TableIdent::new(ns.to_namespace_ident(), name.to_string());
        let table = self.catalog.load_table(&ident).await?;
        let iceberg_schema = table.metadata().current_schema();
        let arrow_schema = iceberg::arrow::schema_to_arrow_schema(iceberg_schema)
            .map_err(BifrostError::Iceberg)?;
        Ok(Arc::new(arrow_schema))
    }

    /// Check whether a loaded physical schema matches the declared schema for `T`.
    pub fn physical_matches_declared<T: DomainTable>(&self, physical: &SchemaRef) -> bool {
        let declared = T::schema();
        // Compare field names and types (order-sensitive).
        physical.fields().len() == declared.fields().len()
            && physical
                .fields()
                .iter()
                .zip(declared.fields().iter())
                .all(|(p, d)| p.name() == d.name() && p.data_type() == d.data_type())
    }

    /// Compute the fingerprint of a physical schema over user fields only.
    /// Strips system and universal correlation columns (`wyrd_*`, `data_tenant_id`,
    /// `run_id`, `card_uid`, `principal_id`) to approximate a table's declared user
    /// fields from a *physical* schema, for the `actual:` field of a `PhysicalDrift`
    /// diagnostic only. This is NOT equal to `T::schema_fingerprint()` for tables
    /// whose policy declares `run_id`/`principal_id` as content columns
    /// (`dev.agent_traces`, `system.audit_log`), and is never a gate.
    pub fn fingerprint_of_user_fields(&self, schema: &SchemaRef) -> [u8; 32] {
        fingerprint_user_fields(schema)
    }

    /// Create the Iceberg table for a pre-declared domain table.
    pub async fn create_domain_table<T: DomainTable>(&self) -> Result<(), BifrostError> {
        let ns = BifrostNamespace::from_domain_namespace(T::NAMESPACE).ok_or_else(|| {
            BifrostError::MetadataMismatch(format!("unknown namespace: {}", T::NAMESPACE))
        })?;
        self.ensure_namespace(ns).await?;

        let schema = T::schema();
        let iceberg_schema =
            iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(schema.as_ref())
                .map_err(BifrostError::Iceberg)?;

        let partition_cols = T::partition_columns();
        let partition_spec = if partition_cols.is_empty() {
            None
        } else {
            Some(build_partition_spec(&iceberg_schema, &partition_cols)?)
        };

        let location = format!("{}/{}/{}", self.warehouse, T::NAMESPACE, T::NAME);

        let creation = match partition_spec {
            Some(spec) => TableCreation::builder()
                .name(T::NAME.to_string())
                .location(location)
                .schema(iceberg_schema)
                .format_version(FormatVersion::V2)
                .partition_spec(spec)
                .build(),
            None => TableCreation::builder()
                .name(T::NAME.to_string())
                .location(location)
                .schema(iceberg_schema)
                .format_version(FormatVersion::V2)
                .build(),
        };

        let ns_ident = ns.to_namespace_ident();
        // Check first to give callers a clean IcebergAlreadyExists error.
        let table_ident_check = iceberg::TableIdent::new(ns_ident.clone(), T::NAME.to_string());
        if self.catalog.table_exists(&table_ident_check).await? {
            return Err(BifrostError::IcebergAlreadyExists {
                namespace: T::NAMESPACE,
                name: T::NAME,
            });
        }
        self.catalog.create_table(&ns_ident, creation).await?;
        Ok(())
    }

    /// Register the SQL control row for a pre-declared domain table.
    pub async fn register_domain_control_row<T: DomainTable>(
        &self,
        fingerprint: [u8; 32],
    ) -> Result<(), BifrostError> {
        let ns = BifrostNamespace::from_domain_namespace(T::NAMESPACE).ok_or_else(|| {
            BifrostError::Internal(format!("unknown namespace: {}", T::NAMESPACE))
        })?;
        let fqn = format!("{}.{}", ns.as_str(), T::NAME);
        let table_uid = TableUid::new_v7();
        let mut conn =
            vala_sql::TenantConn::acquire(&self.pool, wyrd_spec::ids::DataTenantId::SYSTEM_OWNER)
                .await
                .map_err(BifrostError::Sql)?;
        vala_sql::queries::olap_catalog::upsert_table(
            &mut conn,
            table_uid.as_bytes(),
            &fqn,
            &fingerprint,
            TableScope::SystemShared.as_db_str(),
            &[],
        )
        .await
        .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        Ok(())
    }

    /// Declare one index for a pre-declared domain table in `vala.olap_indexes`.
    pub fn declare_domain_index(
        &self,
        namespace: &str,
        name: &str,
        _idx: &DeclaredIndex,
    ) -> Result<(), BifrostError> {
        // Index registration uses vala.olap_indexes (task 03).
        // Stubbed until olap_indexes migration and query functions land.
        tracing::debug!(namespace, name, "declare_domain_index (stub)");
        Ok(())
    }

    /// Ensure all declared indexes for `T` exist idempotently.
    pub fn ensure_domain_indexes<T: DomainTable>(&self) -> Result<(), BifrostError> {
        for idx in T::declared_indexes() {
            self.declare_domain_index(T::NAMESPACE, T::NAME, &idx)?;
        }
        Ok(())
    }

    /// Borrow the optional recovery pool (`vala_recovery` SECURITY DEFINER).
    ///
    /// Returns `None` when no recovery pool was provisioned. Production boot
    /// that requires the commit-recovery sweep must fail hard when this is
    /// `None` (see `wyrd-server` `ServerBootError::RecoveryPoolRequired`).
    #[must_use]
    pub fn recovery_pool(&self) -> Option<&PgPool> {
        self.recovery_pool.as_deref()
    }

    /// Best-effort startup recovery pass.
    ///
    /// Claims stale `precommit` rows (lease absent/expired) via the SECURITY
    /// DEFINER `vala.claim_stale_precommits` function and reconciles each by
    /// scanning Iceberg snapshot summaries for `wyrd_batch_id`. Recovery runs only
    /// when a `recovery_pool` (authenticated as `vala_recovery`) was provided; with
    /// no recovery pool it is disabled and returns immediately.
    async fn startup_recovery(&self) -> Result<(), BifrostError> {
        use crate::writer::commit::WRITER_INSTANCE;

        let Some(recovery_pool) = self.recovery_pool.as_ref() else {
            tracing::info!("startup recovery disabled — no vala_recovery pool configured");
            return Ok(());
        };

        let engine_owner = *WRITER_INSTANCE;

        let mut conn = vala_sql::TenantConn::acquire(
            recovery_pool,
            wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
        )
        .await
        .map_err(BifrostError::Sql)?;
        let claimed =
            vala_sql::queries::olap_catalog::claim_stale_precommits(&mut conn, engine_owner, 100)
                .await
                .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;

        for row in claimed {
            if let Err(e) = self.recover_claimed_row(recovery_pool, &row).await {
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
        recovery_pool: &PgPool,
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
                let mut conn = vala_sql::TenantConn::acquire(recovery_pool, recovery_bind)
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

        let mut conn = vala_sql::TenantConn::acquire(recovery_pool, recovery_bind)
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
    let namespaces = BifrostNamespace::ALL;
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
