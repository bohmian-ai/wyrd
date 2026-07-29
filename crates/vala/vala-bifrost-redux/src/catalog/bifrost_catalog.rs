//! Redux-owned tenant-qualified Bifrost catalog.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::datatypes::{Field, Schema};
use iceberg::TableCreation;
use iceberg::spec::{FormatVersion, NullOrder, SortDirection, SortField, SortOrder, Transform};
use vala_sql::ValaPostgres;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditEvent, BifrostTableDescription, BifrostTableEntry};
use wyrd_storage::settings::BackendConfig;

use crate::catalog::error::BifrostCatalogError;
use crate::catalog::iceberg_sql;
use crate::catalog::storage::{iceberg_storage_factory, warehouse_uri};
use crate::catalog::wire::{
    entry_from_row, fields_from_stored_schema, reject_reserved_field_names,
};
use crate::catalog::{TableRef, TenantTableBinding, build_partition_spec};
use crate::namespaces::BifrostNamespace;
use crate::provider::ReduxTableProvider;
use crate::schema::{SchemaFingerprint, with_managed_columns};
use crate::tables::{BuiltinTableDefinition, builtin_table};

/// Build the fixed ascending sort order used by every Redux Bifrost table.
///
/// # Errors
/// Returns a metadata mismatch when either system sort column is absent or
/// Iceberg rejects the bound sort fields for the physical schema.
fn forge_sort_order(schema: &iceberg::spec::Schema) -> Result<SortOrder, BifrostCatalogError> {
    let tenant_id = schema
        .field_by_name("data_tenant_id")
        .ok_or_else(|| {
            BifrostCatalogError::MetadataMismatch("physical schema lacks data_tenant_id".to_owned())
        })?
        .id;
    let event_time_id = schema
        .field_by_name("wyrd_event_time")
        .ok_or_else(|| {
            BifrostCatalogError::MetadataMismatch(
                "physical schema lacks wyrd_event_time".to_owned(),
            )
        })?
        .id;
    SortOrder::builder()
        .with_order_id(1)
        .with_sort_field(SortField {
            source_id: tenant_id,
            transform: Transform::Identity,
            direction: SortDirection::Ascending,
            null_order: NullOrder::Last,
        })
        .with_sort_field(SortField {
            source_id: event_time_id,
            transform: Transform::Identity,
            direction: SortDirection::Ascending,
            null_order: NullOrder::Last,
        })
        .build(schema)
        .map_err(|error| BifrostCatalogError::MetadataMismatch(error.to_string()))
}

/// Opaque 16-byte table identity stored in `vala.bifrost_tables`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableUid([u8; 16]);

impl TableUid {
    fn new_v7() -> Self {
        Self(*uuid::Uuid::now_v7().as_bytes())
    }

    fn from_row(bytes: &[u8], table: &str) -> Result<Self, BifrostCatalogError> {
        bytes.try_into().map(Self).map_err(|_| {
            BifrostCatalogError::MetadataMismatch(format!("table_uid length mismatch for {table}"))
        })
    }

    /// Borrow the stable 16-byte representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Inputs for one tenant-qualified physical table registration.
pub struct CreateTableRequest {
    /// Tenant-free logical table identity.
    pub table: TableRef,
    /// User fields before Redux appends server-owned columns.
    pub user_fields: Vec<Field>,
    /// Authenticated organization that owns the registration and physical table.
    pub tenant: DataTenantId,
    /// Optional audit event committed with the tenant-scoped control row.
    pub audit: Option<AuditEvent>,
}

/// Redux catalog shared by Gate, Forge, Oracle, and server catalog routes.
#[derive(Clone)]
pub struct BifrostCatalog {
    catalog: Arc<dyn iceberg::Catalog>,
    postgres: ValaPostgres,
    warehouse: String,
}

impl BifrostCatalog {
    /// Build the Redux Iceberg SQL catalog and tenant-scoped control-plane handle.
    ///
    /// # Errors
    /// Returns a catalog error when the Iceberg SQL catalog cannot be loaded.
    pub async fn new(
        catalog_uri: &str,
        backend: &BackendConfig,
        postgres: ValaPostgres,
    ) -> Result<Self, BifrostCatalogError> {
        let (storage_factory, storage_properties) = iceberg_storage_factory(backend);
        let warehouse = warehouse_uri(backend);
        let catalog = iceberg_sql::build_catalog(
            catalog_uri,
            &warehouse,
            storage_factory,
            storage_properties,
        )
        .await?;
        Ok(Self {
            catalog: Arc::new(catalog),
            postgres,
            warehouse,
        })
    }

    /// Borrow the exact Iceberg catalog used by Redux physical table registration.
    #[must_use]
    pub fn iceberg_catalog(&self) -> Arc<dyn iceberg::Catalog> {
        Arc::clone(&self.catalog)
    }

    /// Register one logical table as a tenant-qualified physical Iceberg table.
    ///
    /// The logical FQN remains identical across organizations. The authenticated
    /// tenant is encoded only in the physical namespace and object prefix.
    ///
    /// # Errors
    /// Returns a typed catalog error for invalid fields/bindings, metadata drift,
    /// Iceberg failures, SQL failures, or audit failures.
    pub async fn create_table(
        &self,
        request: CreateTableRequest,
    ) -> Result<TableUid, BifrostCatalogError> {
        if builtin_table(
            request
                .table
                .namespace
                .as_str()
                .strip_prefix("vala.")
                .unwrap_or_default(),
            &request.table.name,
        )
        .is_some()
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "built-in tables must be provisioned with ensure_builtin".to_owned(),
            ));
        }
        self.create_table_locked(request, None).await
    }

    /// Register a caller-owned dataset in the tenant-qualified dataset namespace.
    ///
    /// # Errors
    /// Returns a typed catalog error when the dataset name, schema, physical table,
    /// control row, or audit event is invalid.
    pub async fn register_dataset(
        &self,
        tenant: DataTenantId,
        table: TableRef,
        user_fields: Vec<Field>,
        audit: Option<AuditEvent>,
    ) -> Result<TableUid, BifrostCatalogError> {
        if table.namespace != BifrostNamespace::Datasets {
            return Err(BifrostCatalogError::MetadataMismatch(
                "caller-owned registrations must use vala.datasets".to_owned(),
            ));
        }
        self.create_table_locked(
            CreateTableRequest {
                table,
                user_fields,
                tenant,
                audit,
            },
            None,
        )
        .await
    }

    /// Ensure one canonical built-in exists for a tenant.
    ///
    /// Built-ins are lazy: this method is the only path that creates one, and
    /// callers choose when a tenant first needs the table.
    pub async fn ensure_builtin(
        &self,
        tenant: DataTenantId,
        definition: &'static BuiltinTableDefinition,
    ) -> Result<TableUid, BifrostCatalogError> {
        let namespace =
            BifrostNamespace::from_domain_namespace(definition.namespace).ok_or_else(|| {
                BifrostCatalogError::MetadataMismatch(format!(
                    "unknown built-in namespace: {}",
                    definition.namespace
                ))
            })?;
        self.create_table_locked(
            CreateTableRequest {
                table: TableRef::new(namespace, definition.name),
                user_fields: (definition.arrow_fields)(),
                tenant,
                audit: None,
            },
            Some((definition.schema)()),
        )
        .await
    }

    async fn create_table_locked(
        &self,
        request: CreateTableRequest,
        canonical_schema: Option<arrow::datatypes::SchemaRef>,
    ) -> Result<TableUid, BifrostCatalogError> {
        reject_reserved_field_names(&request.user_fields)?;
        let binding = TenantTableBinding::resolve((request.tenant, request.table))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let fqn = binding.table_ref.fqn();
        let fingerprint =
            SchemaFingerprint::from_arrow_schema(&Schema::new(request.user_fields.clone()));

        let mut conn = self.postgres.tenant_conn(request.tenant).await?;
        acquire_table_advisory_lock(&mut conn, request.tenant, &fqn).await?;
        let row = vala_sql::queries::olap_catalog::get_by_fqn(&mut conn, &fqn).await?;
        let table_ident = binding.table_ident();
        let physical_exists = self.catalog.table_exists(&table_ident).await?;

        if let Some(row) = row {
            if row.fingerprint.as_slice() != fingerprint.as_ref() {
                return Err(BifrostCatalogError::FingerprintMismatch(fqn));
            }
            if !physical_exists {
                return Err(BifrostCatalogError::MetadataMismatch(format!(
                    "control registration exists without physical table: {table_ident}"
                )));
            }
            let physical = self.catalog.load_table(&table_ident).await?;
            self.validate_physical_table(
                &physical,
                &binding,
                canonical_schema
                    .as_deref()
                    .unwrap_or(&Schema::new(with_managed_columns(
                        request.user_fields.clone(),
                    ))),
            )?;
            conn.commit().await?;
            return TableUid::from_row(&row.table_uid, &row.fqn);
        }

        self.ensure_namespace(binding.physical_namespace()).await?;
        if physical_exists {
            let physical = self.catalog.load_table(&table_ident).await?;
            self.validate_physical_table(
                &physical,
                &binding,
                canonical_schema
                    .as_deref()
                    .unwrap_or(&Schema::new(with_managed_columns(
                        request.user_fields.clone(),
                    ))),
            )?;
        } else {
            let arrow_schema = canonical_schema
                .as_deref()
                .cloned()
                .unwrap_or_else(|| Schema::new(with_managed_columns(request.user_fields.clone())));
            let iceberg_schema =
                iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&arrow_schema)?;
            let partition_columns = binding.partition_columns();
            let partition_spec = build_partition_spec(&iceberg_schema, &partition_columns)
                .map_err(BifrostCatalogError::MetadataMismatch)?;
            let sort_order = forge_sort_order(&iceberg_schema)?;
            let location = format!(
                "{}/{}",
                self.warehouse.trim_end_matches('/'),
                binding.object_prefix
            );
            let creation = TableCreation::builder()
                .name(binding.table_name.clone())
                .location(location)
                .schema(iceberg_schema)
                .format_version(FormatVersion::V2)
                .partition_spec(partition_spec)
                .sort_order(sort_order)
                .build();
            self.catalog
                .create_table(binding.physical_namespace(), creation)
                .await?;
        }

        let table_uid = TableUid::new_v7();
        vala_sql::queries::olap_catalog::upsert_table(
            &mut conn,
            table_uid.as_bytes(),
            &fqn,
            &fingerprint.0,
            &["wyrd_event_time".to_owned()],
        )
        .await?;
        if let Some(event) = request.audit.as_ref() {
            vala_sql::queries::audit_outbox::append_audit(&mut conn, event)
                .await
                .map_err(|error| {
                    tracing::error!(error = %error, table = %fqn, "Redux catalog audit append failed");
                    BifrostCatalogError::AuditUnavailable(
                        "audit outbox append failed".to_owned(),
                    )
                })?;
        }
        conn.commit().await?;
        Ok(table_uid)
    }

    /// Verify that a registered table still carries Bifrost's complete physical recipe.
    ///
    /// Registration and reconciliation call this before accepting existing
    /// physical state, preventing a control-plane row from silently pointing
    /// at a table with a different location, schema, day partition, or Forge
    /// sort recipe.
    ///
    /// # Errors
    ///
    /// Returns a metadata mismatch when the location, schema, partition, or
    /// sort recipe diverges, or an Iceberg error when its schema cannot be
    /// converted for shape validation.
    fn validate_physical_table(
        &self,
        table: &iceberg::table::Table,
        binding: &TenantTableBinding,
        expected_schema: &Schema,
    ) -> Result<(), BifrostCatalogError> {
        let expected_location = format!(
            "{}/{}",
            self.warehouse.trim_end_matches('/'),
            binding.object_prefix
        );
        if table.metadata().location() != expected_location {
            return Err(BifrostCatalogError::MetadataMismatch(format!(
                "physical table location mismatch: expected {expected_location}, found {}",
                table.metadata().location()
            )));
        }
        let actual_schema =
            iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema())?;
        if !schema_shape_matches(expected_schema, &actual_schema) {
            return Err(BifrostCatalogError::MetadataMismatch(format!(
                "physical table schema mismatch: expected {expected_schema:?}, actual {actual_schema:?}"
            )));
        }
        let fields = table.metadata().default_partition_spec().fields();
        if fields.len() != 1
            || fields[0].source_id
                != table
                    .metadata()
                    .current_schema()
                    .field_by_name("wyrd_event_time")
                    .map(|field| field.id)
                    .unwrap_or_default()
            || fields[0].name != "wyrd_event_time_day"
            || fields[0].transform != iceberg::spec::Transform::Day
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "physical table partition spec mismatch".to_owned(),
            ));
        }
        let schema = table.metadata().current_schema();
        let expected_sort_fields = ["data_tenant_id", "wyrd_event_time"]
            .into_iter()
            .map(|name| schema.field_by_name(name).map(|field| field.id))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                BifrostCatalogError::MetadataMismatch(
                    "physical schema lacks Forge sort columns".to_owned(),
                )
            })?;
        let sort_fields = &table.metadata().default_sort_order().fields;
        if sort_fields.len() != expected_sort_fields.len()
            || sort_fields
                .iter()
                .zip(expected_sort_fields)
                .any(|(field, source_id)| {
                    field.source_id != source_id
                        || field.transform != Transform::Identity
                        || field.direction != SortDirection::Ascending
                        || field.null_order != NullOrder::Last
                })
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "physical table sort order mismatch".to_owned(),
            ));
        }
        Ok(())
    }

    /// Return the registered user-schema fingerprint for one tenant/logical table.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the tenant does not own
    /// the registration, or a metadata error for a malformed fingerprint.
    pub async fn table_schema_fingerprint(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<SchemaFingerprint, BifrostCatalogError> {
        let fqn = table.fqn();
        let row = self
            .lookup_table_row(&fqn, tenant)
            .await?
            .ok_or_else(|| BifrostCatalogError::TableNotFound(fqn.clone()))?;
        let fingerprint = row.fingerprint.as_slice().try_into().map_err(|_| {
            BifrostCatalogError::MetadataMismatch(format!(
                "schema fingerprint length mismatch for {fqn}"
            ))
        })?;
        Ok(SchemaFingerprint(fingerprint))
    }

    /// List registrations visible under the exact tenant RLS bind.
    ///
    /// # Errors
    /// Returns a catalog error when SQL or row projection fails.
    pub async fn list_tables(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<BifrostTableEntry>, BifrostCatalogError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let rows = vala_sql::queries::olap_catalog::list_tables_for_tenant(&mut conn).await?;
        conn.commit().await?;
        rows.iter().map(entry_from_row).collect()
    }

    /// Describe one tenant-qualified table and its stored physical schema.
    ///
    /// # Errors
    /// Returns a catalog error for a missing registration, invalid binding,
    /// Iceberg load failure, or malformed stored schema.
    pub async fn describe_table(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<BifrostTableDescription, BifrostCatalogError> {
        let fqn = table.fqn();
        let row = self
            .lookup_table_row(&fqn, tenant)
            .await?
            .ok_or_else(|| BifrostCatalogError::TableNotFound(fqn))?;
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let iceberg_table = self.catalog.load_table(&binding.table_ident()).await?;
        let arrow_schema =
            iceberg::arrow::schema_to_arrow_schema(iceberg_table.metadata().current_schema())?;
        Ok(BifrostTableDescription {
            entry: entry_from_row(&row)?,
            fields: fields_from_stored_schema(&arrow_schema)?,
        })
    }

    /// Build an execution provider for one authenticated tenant's table.
    ///
    /// The control-plane lookup and physical Iceberg binding both use the
    /// authenticated tenant. The returned provider adds an execution-time
    /// tenant predicate as a second fail-closed boundary.
    pub async fn provider(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<ReduxTableProvider, BifrostCatalogError> {
        self.provider_with_hot_batches(table, tenant, Vec::new())
            .await
    }

    /// Build a tenant-qualified provider with shallow Scribe hot batches.
    pub async fn provider_with_hot_batches(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
        hot_batches: Vec<arrow::record_batch::RecordBatch>,
    ) -> Result<ReduxTableProvider, BifrostCatalogError> {
        let fqn = table.fqn();
        let Some(_row) = self.lookup_table_row(&fqn, tenant).await? else {
            return Err(BifrostCatalogError::TableNotFound(fqn));
        };
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let iceberg_table = self.catalog.load_table(&binding.table_ident()).await?;
        ReduxTableProvider::try_new_with_hot_batches(iceberg_table, tenant, hot_batches)
            .await
            .map_err(BifrostCatalogError::DataFusion)
    }

    async fn ensure_namespace(
        &self,
        namespace: &iceberg::NamespaceIdent,
    ) -> Result<(), BifrostCatalogError> {
        if !self.catalog.namespace_exists(namespace).await? {
            self.catalog
                .create_namespace(namespace, HashMap::new())
                .await?;
        }
        Ok(())
    }

    async fn lookup_table_row(
        &self,
        fqn: &str,
        tenant: DataTenantId,
    ) -> Result<Option<vala_sql::row_types::olap_catalog::BifrostTableRow>, BifrostCatalogError>
    {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let row = vala_sql::queries::olap_catalog::get_by_fqn(&mut conn, fqn).await?;
        conn.commit().await?;
        Ok(row)
    }
}

fn schema_shape_matches(expected: &Schema, actual: &Schema) -> bool {
    expected.fields().len() == actual.fields().len()
        && expected
            .fields()
            .iter()
            .zip(actual.fields())
            .all(|(expected, actual)| {
                expected.name() == actual.name()
                    && expected.is_nullable() == actual.is_nullable()
                    && data_type_shape_matches(expected.data_type(), actual.data_type())
            })
}

fn data_type_shape_matches(
    expected: &arrow::datatypes::DataType,
    actual: &arrow::datatypes::DataType,
) -> bool {
    if expected.equals_datatype(actual) {
        return true;
    }
    match (expected, actual) {
        (
            arrow::datatypes::DataType::Timestamp(expected_unit, Some(expected_timezone)),
            arrow::datatypes::DataType::Timestamp(actual_unit, Some(actual_timezone)),
        ) => {
            expected_unit == actual_unit
                && ((expected_timezone.as_ref() == "UTC" && actual_timezone.as_ref() == "+00:00")
                    || (expected_timezone.as_ref() == "+00:00"
                        && actual_timezone.as_ref() == "UTC"))
        }
        _ => false,
    }
}

async fn acquire_table_advisory_lock(
    conn: &mut wyrd_sql::TenantConn<'_>,
    tenant: DataTenantId,
    fqn: &str,
) -> Result<(), BifrostCatalogError> {
    let lock_key = format!("{}:{fqn}", tenant.as_uuid());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(lock_key)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
        .map_err(vala_sql::SqlError::from)
        .map_err(BifrostCatalogError::Sql)
}
