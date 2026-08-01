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
use crate::schema::managed_columns::with_managed_columns;
use crate::tables::{DeclaredIndex, DomainTable};
use crate::types::{PartitionTransform, SchemaFingerprint, TableScope, TableUid};
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

/// Inputs for [`WyrdCatalog::create_table`].
///
/// `audit`, when present, appends one hash-chained `AuditEvent` row into the
/// transactional outbox in the same tx as the `vala.bifrost_tables`
/// registration.
pub struct CreateTableRequest<'a> {
    /// Bifrost namespace under which the table lives.
    pub ns: BifrostNamespace,
    /// Table name (unique within the namespace).
    pub name: &'a str,
    /// Arrow schema fields for the user payload (system columns are added by the catalog).
    pub user_fields: Vec<Field>,
    /// Tenancy scope: `SystemShared` or `TenantOwned`.
    pub scope: TableScope,
    /// Registering tenant.
    pub tenant: wyrd_spec::ids::DataTenantId,
    /// Optional Iceberg partition transforms `(source_field_name, transform)`.
    pub partition_columns: &'a [(String, PartitionTransform)],
    /// Optional audit event appended in the same tx as the control-table row.
    pub audit: Option<wyrd_spec::vala::api::AuditEvent>,
}

impl WyrdCatalog {
    /// Borrow the Iceberg catalog used by Bifrost readers and registration.
    ///
    /// The returned trait object is the same catalog instance assembled during
    /// server boot. Callers that need a data-plane worker must reuse it rather
    /// than constructing a second SQL catalog over the same warehouse.
    #[must_use]
    pub fn iceberg_catalog(&self) -> Arc<dyn iceberg::Catalog> {
        self.catalog.clone()
    }

    /// Construct the catalog used by the current read and registration surfaces.
    pub async fn new(
        catalog_uri: &str,
        backend: &BackendConfig,
        pool: Arc<PgPool>,
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
            storage_factory,
            storage_props,
            warehouse,
            registry,
        };

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
    /// Used for engine-owned warehouse tables that must be present before the
    /// first write.
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
        self.create_table(CreateTableRequest {
            ns,
            name,
            user_fields,
            scope: TableScope::SystemShared,
            tenant: wyrd_spec::ids::DataTenantId::SYSTEM_OWNER,
            partition_columns,
            audit: None,
        })
        .await?;
        Ok(())
    }

    /// `audit`, when present, appends one hash-chained `AuditEvent` row into the
    /// transactional outbox **in the same tx as the `vala.bifrost_tables`
    /// registration** (S3.C5): the control-table row and its audit record commit
    /// atomically, and an audit-append failure fails the registration closed.
    pub async fn create_table(
        &self,
        request: CreateTableRequest<'_>,
    ) -> Result<TableUid, BifrostError> {
        let CreateTableRequest {
            ns,
            name,
            user_fields,
            scope,
            tenant,
            partition_columns,
            audit,
        } = request;
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

        let all_fields = with_managed_columns(user_fields);
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

        let row = self
            .lookup_table_row(&fqn, tenant)
            .await?
            .ok_or_else(|| BifrostError::TableNotFound(fqn.clone()))?;
        let owner = tenant;

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

    /// Return the registered user-schema fingerprint for a table visible to
    /// `tenant`, using the same tenant/system lookup as [`Self::describe_table`].
    pub async fn table_schema_fingerprint(
        &self,
        ns: BifrostNamespace,
        name: &str,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<[u8; 32], BifrostError> {
        let meta = self.get(ns, name, tenant).await?;
        meta.row
            .fingerprint
            .as_slice()
            .try_into()
            .map_err(|_| BifrostError::Internal("schema fingerprint length mismatch".to_owned()))
    }

    /// Open a read provider for `tenant` (the authenticated **data tenant**).
    ///
    /// The caller supplies the authenticated tenant and the catalog lookup is
    /// tenant-scoped; the provider therefore cannot cross tenant boundaries.
    pub async fn provider(
        &self,
        ns: BifrostNamespace,
        name: &str,
        tenant: wyrd_spec::ids::DataTenantId,
    ) -> Result<WyrdTableProvider, BifrostError> {
        let meta = self.get(ns, name, tenant).await?;
        let table = (*meta.iceberg_table).clone();
        WyrdTableProvider::try_new(table, TableScope::TenantOwned, tenant)
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
    ///
    /// The physical schema is read back from Iceberg, so declared Arrow types that
    /// Iceberg does not store natively (e.g. `UInt32` → `Int64`) round-trip to a
    /// different Arrow type than declared. Comparing the raw declared schema would
    /// therefore report drift on every re-boot of a healthy table. To stay
    /// idempotent, normalize the declared schema through the same
    /// arrow → iceberg → arrow round-trip `create_domain_table` performs, then
    /// compare that expected physical shape against the actual physical schema.
    pub fn physical_matches_declared<T: DomainTable>(&self, physical: &SchemaRef) -> bool {
        let Some(expected) = expected_physical_schema::<T>() else {
            return false;
        };
        physical_schema_matches_expected(physical, &expected)
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
}

/// The Arrow schema a healthy Iceberg table for `T` reads back as.
///
/// `create_domain_table` writes `arrow_schema_to_schema_auto_assign_ids(T::schema())`;
/// `iceberg_physical_schema` reads back `schema_to_arrow_schema(current_schema())`.
/// Applying that same arrow → iceberg → arrow round-trip to the declared schema
/// yields the exact physical shape to compare against, so a re-boot of a healthy
/// table matches instead of reporting spurious `PhysicalDrift` on Iceberg type
/// normalizations (e.g. `UInt32` → `Int64`). Returns `None` only if the declared
/// schema cannot map into Iceberg at all, which is a real, non-idempotency failure.
fn expected_physical_schema<T: DomainTable>() -> Option<SchemaRef> {
    let declared = T::schema();
    let iceberg_schema =
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(declared.as_ref()).ok()?;
    let roundtripped = iceberg::arrow::schema_to_arrow_schema(&iceberg_schema).ok()?;
    Some(Arc::new(roundtripped))
}

/// Compare the complete ordered physical shape after Iceberg normalization.
///
/// Registration and split-brain repair use this validator before accepting a
/// persisted table. Names, normalized Arrow types, order, and nullability all
/// participate so an old nullable identity column or a missing request column
/// fails closed instead of being silently coerced.
fn physical_schema_matches_expected(physical: &SchemaRef, expected: &SchemaRef) -> bool {
    physical.fields().len() == expected.fields().len()
        && physical
            .fields()
            .iter()
            .zip(expected.fields().iter())
            .all(|(actual, declared)| {
                actual.name() == declared.name()
                    && actual.data_type() == declared.data_type()
                    && actual.is_nullable() == declared.is_nullable()
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::traces::SpansTable;
    use arrow::datatypes::Schema;

    #[test]
    /// A nullable principal is rejected even when every other physical field matches.
    ///
    /// # Panics
    ///
    /// Panics when the declared schema cannot be normalized or the mismatch is accepted.
    fn physical_schema_rejects_nullable_principal_id() {
        let expected = expected_physical_schema::<SpansTable>().expect("schema normalizes");
        let fields: Vec<_> = expected
            .fields()
            .iter()
            .map(|field| {
                let field = if field.name() == wyrd_spec::vala::PRINCIPAL_ID {
                    field.as_ref().clone().with_nullable(true)
                } else {
                    field.as_ref().clone()
                };
                Arc::new(field)
            })
            .collect();
        let physical = Arc::new(Schema::new(fields));

        assert!(!physical_schema_matches_expected(&physical, &expected));
    }

    #[test]
    /// A physical schema missing request correlation is rejected before repair.
    ///
    /// # Panics
    ///
    /// Panics when the declared schema cannot be normalized or the mismatch is accepted.
    fn physical_schema_rejects_missing_request_id() {
        let expected = expected_physical_schema::<SpansTable>().expect("schema normalizes");
        let fields: Vec<_> = expected
            .fields()
            .iter()
            .filter(|field| field.name() != wyrd_spec::vala::WYRD_REQUEST_ID)
            .cloned()
            .collect();
        let physical = Arc::new(Schema::new(fields));

        assert!(!physical_schema_matches_expected(&physical, &expected));
    }
}
