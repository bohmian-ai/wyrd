//! Redux-owned tenant-qualified Bifrost catalog.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use arrow::datatypes::{Field, Schema};
use iceberg::TableCreation;
use iceberg::io::{FileIO, FileIOBuilder};
use iceberg::spec::{FormatVersion, Transform};
use sha2::{Digest as _, Sha256};
use vala_sql::ValaPostgres;
use vala_sql::queries::file_list::HotFileCatalog;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    AuditEvent, BifrostTableDescription, BifrostTableEntry, PersistedWalRange, PhysicalLayoutWire,
    persisted_wal_ranges_are_valid,
};
use wyrd_storage::settings::BackendConfig;

use crate::catalog::error::BifrostCatalogError;
use crate::catalog::iceberg_sql;
use crate::catalog::layout::PhysicalLayout;
use crate::catalog::storage::{iceberg_storage_factory, warehouse_uri};
use crate::catalog::wire::{
    entry_from_row, fields_from_stored_schema, layout_wire_from_row, reject_reserved_field_names,
};
use crate::catalog::{TableRef, TenantTableBinding};
use crate::namespaces::BifrostNamespace;
use crate::provider::ReduxTableProvider;
use crate::schema::{SchemaFingerprint, with_managed_columns};
use crate::tables::{BuiltinTableDefinition, builtin_table};

/// Hashes an ordered metadata identity projection for immutable cut auditing.
fn digest_strings(values: impl IntoIterator<Item = String>) -> String {
    let mut digest = Sha256::new();
    for value in values {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    hex::encode(digest.finalize())
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
    /// Optional caller-declared physical layout.
    ///
    /// `None` means "no declaration": the catalog resolves the canonical
    /// default layout. `Some` is canonicalized and validated against the
    /// complete physical schema before any durable mutation.
    pub physical_layout: Option<PhysicalLayoutWire>,
    /// Optional audit event committed with the tenant-scoped control row.
    pub audit: Option<AuditEvent>,
}

/// Immutable metadata cut for one tenant-qualified sealed table.
#[derive(Debug)]
pub struct PinnedSealedTable {
    /// Authenticated tenant/table binding.
    pub binding: TenantTableBinding,
    /// Iceberg table metadata loaded once for this cut.
    pub iceberg_table: iceberg::table::Table,
    /// Exact current snapshot identity selected from the immutable table metadata.
    pub snapshot_id: Option<i64>,
    /// Stable digest of the selected snapshot identity and live data-file set.
    pub snapshot_digest: String,
    /// Canonical live data-file identities in the pinned Iceberg snapshot.
    pub iceberg_file_paths: BTreeSet<String>,
    /// Ordered immutable data-file work from the pinned Iceberg snapshot.
    pub iceberg_files: Vec<PinnedIcebergFile>,
    /// Ordered tenant hot rows absent from the exact pinned snapshot manifest.
    pub hot_files: Vec<vala_sql::row_types::file_list::HotFileRow>,
    /// Complete validated sealed manifest retained for live-tail watermarks.
    pub sealed_manifest: Vec<vala_sql::row_types::file_list::HotFileRow>,
    /// Stable digest of the ordered, post-subtraction hot manifest.
    pub hot_manifest_digest: String,
    /// Bounded sealed-byte estimate used by Oracle classification.
    pub estimated_bytes: u64,
}

impl PinnedSealedTable {
    /// Projects complete writer-v2 generations into canonical persisted WAL ranges.
    ///
    /// # Errors
    ///
    /// Returns a metadata mismatch when a generation has missing ordinals,
    /// incompatible bounds, overlapping WAL ownership, or straddles the cursor.
    pub fn persisted_wal_ranges(
        &self,
        persisted_cursor: u64,
    ) -> Result<Vec<PersistedWalRange>, BifrostCatalogError> {
        project_persisted_wal_ranges(&self.sealed_manifest, persisted_cursor)
    }
}

/// Projects a sealed manifest into one inclusive WAL range per complete generation.
///
/// # Errors
///
/// Returns a metadata mismatch for invalid bounds, incomplete ordinals,
/// overlapping generations, or cursor straddling.
pub fn project_persisted_wal_ranges(
    rows: &[vala_sql::row_types::file_list::HotFileRow],
    persisted_cursor: u64,
) -> Result<Vec<PersistedWalRange>, BifrostCatalogError> {
    let mut generations: BTreeMap<(uuid::Uuid, i64, i64, i64), BTreeSet<i16>> = BTreeMap::new();
    for row in rows {
        if row.writer_epoch < 0
            || row.wal_lsn_min < 0
            || row.wal_lsn_max < row.wal_lsn_min
            || row.file_ordinal < 0
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "writer-v2 generation contains invalid bounds or ordinal".to_owned(),
            ));
        }
        let ordinals = generations
            .entry((
                row.node_id,
                row.writer_epoch,
                row.wal_lsn_min,
                row.wal_lsn_max,
            ))
            .or_default();
        if !ordinals.insert(row.file_ordinal) {
            return Err(BifrostCatalogError::MetadataMismatch(
                "writer-v2 generation contains a duplicate ordinal".to_owned(),
            ));
        }
    }
    let mut ranges = Vec::with_capacity(generations.len());
    for ((_, _, start, end), ordinals) in generations {
        if ordinals
            .iter()
            .copied()
            .enumerate()
            .any(|(expected, actual)| usize::try_from(actual) != Ok(expected))
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "writer-v2 generation ordinals are not contiguous".to_owned(),
            ));
        }
        let start_lsn = u64::try_from(start).map_err(|_| {
            BifrostCatalogError::MetadataMismatch("persisted WAL lower bound is invalid".to_owned())
        })?;
        let end_lsn = u64::try_from(end).map_err(|_| {
            BifrostCatalogError::MetadataMismatch("persisted WAL upper bound is invalid".to_owned())
        })?;
        ranges.push(PersistedWalRange { start_lsn, end_lsn });
    }
    ranges.sort_unstable_by_key(|range| (range.start_lsn, range.end_lsn));
    if !persisted_wal_ranges_are_valid(persisted_cursor, &ranges) {
        return Err(BifrostCatalogError::MetadataMismatch(
            "persisted WAL ranges violate cursor, order, overlap, or signed bounds".to_owned(),
        ));
    }
    Ok(ranges)
}

/// Immutable file metadata retained from one pinned Iceberg manifest entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedIcebergFile {
    /// Canonical tenant-qualified object path.
    pub file_path: String,
    /// Exact object byte size from the pinned manifest.
    pub file_size: u64,
    /// Exact record count from the pinned manifest.
    pub row_count: u64,
}

/// Immutable Iceberg snapshot metadata collected before hot-manifest reconciliation.
struct PinnedIcebergState {
    /// Current snapshot identity, when the table has committed data.
    snapshot_id: Option<i64>,
    /// Validated Forge publication operation recorded by the current snapshot.
    publication_operation_id: Option<uuid::Uuid>,
    /// Canonical paths used to exclude hot-manifest overlap.
    file_paths: BTreeSet<String>,
    /// Exact immutable metadata keyed by canonical path.
    files: BTreeMap<String, PinnedIcebergFile>,
    /// Checked sum of immutable object bytes.
    estimated_bytes: u64,
}

impl PinnedIcebergState {
    /// Returns the canonical identity digest used to stabilize a cross-system cut.
    fn digest(&self) -> String {
        digest_strings(
            self.snapshot_id
                .map(|id| id.to_string())
                .into_iter()
                .chain(self.file_paths.iter().cloned()),
        )
    }
}

/// Redux catalog shared by Gate, Forge, Oracle, and server catalog routes.
#[derive(Clone)]
pub struct BifrostCatalog {
    catalog: Arc<dyn iceberg::Catalog>,
    postgres: ValaPostgres,
    warehouse: String,
    file_io: FileIO,
}

/// Catalog pins observed by production code paths during serialized tests.
///
/// Classification and execution each used to pin the catalog, costing every
/// query two round trips for one file list. This counter lets a test assert
/// that a locally led query pins exactly once per table.
#[cfg(any(test, feature = "test-support"))]
static TEST_SEALED_PIN_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Resets and returns the observed catalog-pin count for a serialized test.
#[cfg(any(test, feature = "test-support"))]
pub fn reset_sealed_pin_count_for_test() -> usize {
    TEST_SEALED_PIN_COUNT.swap(0, std::sync::atomic::Ordering::SeqCst)
}

/// Returns catalog pins observed since the last reset in a serialized test.
#[must_use]
#[cfg(any(test, feature = "test-support"))]
pub fn sealed_pin_count_for_test() -> usize {
    TEST_SEALED_PIN_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

impl BifrostCatalog {
    /// Pins one Iceberg table and its tenant-scoped hot manifest without reading rows.
    ///
    /// # Errors
    /// Returns a catalog or SQL error when the binding, Iceberg metadata, or
    /// tenant-scoped manifest cannot be loaded.
    pub async fn pin_sealed_table(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<PinnedSealedTable, BifrostCatalogError> {
        #[cfg(any(test, feature = "test-support"))]
        TEST_SEALED_PIN_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let fqn = table.fqn();
        let Some(_row) = self.lookup_table_row(&fqn, tenant).await? else {
            return Err(BifrostCatalogError::TableNotFound(fqn));
        };
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let (iceberg_table, pinned, cut) = self.acquire_stable_cut(&binding, tenant).await?;
        let snapshot_id = pinned.snapshot_id;
        let iceberg_file_paths = pinned.file_paths;
        let iceberg_files = pinned.files;
        let mut estimated_bytes = pinned.estimated_bytes;
        if cut.ambiguous_publication {
            return Err(BifrostCatalogError::AmbiguousPublication);
        }
        let hot_files = cut.hot_files;
        let sealed_manifest = cut.sealed_manifest;
        for row in &sealed_manifest {
            let valid_identity = row.data_tenant_id == tenant.as_uuid()
                && row.namespace == binding.logical_namespace
                && row.table_name == binding.table_name
                && row.file_size > 0
                && row.row_count >= 0
                && row.writer_epoch >= 0
                && row.wal_lsn_min >= 0
                && row.wal_lsn_max >= row.wal_lsn_min
                && binding.validate_object_path(&row.file_path).is_some();
            if !valid_identity {
                return Err(BifrostCatalogError::MetadataMismatch(
                    "sealed manifest row violates its tenant/table binding".to_owned(),
                ));
            }
        }
        let hot_file_count = sealed_manifest.len();
        metrics::counter!(
            "bifrost_oracle_files_pruned_total",
            "source" => "hot_sealed",
            "reason" => "snapshot_overlap"
        )
        .increment(hot_file_count.saturating_sub(hot_files.len()) as u64);
        for row in &hot_files {
            estimated_bytes = estimated_bytes
                .checked_add(u64::try_from(row.file_size).map_err(|_| {
                    BifrostCatalogError::MetadataMismatch(
                        "hot manifest file size is invalid".to_owned(),
                    )
                })?)
                .ok_or_else(|| {
                    BifrostCatalogError::MetadataMismatch(
                        "sealed byte estimate overflow".to_owned(),
                    )
                })?;
        }
        let snapshot_digest = digest_strings(
            snapshot_id
                .map(|id| id.to_string())
                .into_iter()
                .chain(iceberg_file_paths.iter().cloned()),
        );
        let hot_manifest_digest = digest_strings(hot_files.iter().map(|row| {
            format!(
                "{}:{}:{}:{}",
                row.file_path, row.file_size, row.wal_lsn_min, row.wal_lsn_max
            )
        }));
        Ok(PinnedSealedTable {
            binding,
            iceberg_table,
            snapshot_id,
            snapshot_digest,
            iceberg_file_paths,
            iceberg_files: iceberg_files.into_values().collect(),
            hot_files,
            sealed_manifest,
            hot_manifest_digest,
            estimated_bytes,
        })
    }

    /// Acquires one stable Iceberg/SQL/Iceberg cut for a tenant table.
    ///
    /// # Errors
    ///
    /// Returns a catalog or SQL error for one failed read, or `UnstableCut`
    /// after three complete snapshot identity mismatches. Cancellation drops
    /// the in-flight attempt without exposing partial state.
    async fn acquire_stable_cut(
        &self,
        binding: &TenantTableBinding,
        tenant: DataTenantId,
    ) -> Result<
        (
            iceberg::table::Table,
            PinnedIcebergState,
            vala_sql::queries::file_list::HotFileCut,
        ),
        BifrostCatalogError,
    > {
        for _attempt in 1..=3_u8 {
            let iceberg_a = self.catalog.load_table(&binding.table_ident()).await?;
            let pinned_a = self.pin_iceberg_snapshot(&iceberg_a, binding).await?;
            let hot_file_catalog =
                HotFileCatalog::new(&binding.logical_namespace, &binding.table_name);
            let mut conn = self.postgres.tenant_conn(tenant).await?;
            let cut = hot_file_catalog
                .unresolved_for_cut(
                    &mut conn,
                    &pinned_a.file_paths,
                    pinned_a.publication_operation_id,
                )
                .await?;
            conn.commit().await?;
            let iceberg_b = self.catalog.load_table(&binding.table_ident()).await?;
            let pinned_b = self.pin_iceberg_snapshot(&iceberg_b, binding).await?;
            if pinned_a.snapshot_id == pinned_b.snapshot_id
                && pinned_a.digest() == pinned_b.digest()
            {
                return Ok((iceberg_b, pinned_b, cut));
            }
        }
        Err(BifrostCatalogError::UnstableCut { attempts: 3 })
    }

    /// Collects and validates the immutable files of one current Iceberg snapshot.
    ///
    /// # Errors
    ///
    /// Returns a catalog error when manifests cannot be read, a path escapes the
    /// tenant binding, file metadata conflicts, or byte accounting overflows.
    async fn pin_iceberg_snapshot(
        &self,
        iceberg_table: &iceberg::table::Table,
        binding: &TenantTableBinding,
    ) -> Result<PinnedIcebergState, BifrostCatalogError> {
        let snapshot_id = iceberg_table.metadata().current_snapshot_id();
        let mut file_paths = BTreeSet::new();
        let mut files = BTreeMap::new();
        let mut estimated_bytes = 0_u64;
        let Some(snapshot) = iceberg_table.metadata().current_snapshot() else {
            return Ok(PinnedIcebergState {
                snapshot_id,
                publication_operation_id: None,
                file_paths,
                files,
                estimated_bytes,
            });
        };
        let publication_operation_id = snapshot
            .summary()
            .additional_properties
            .get("forge.operation_id")
            .map(|value| {
                uuid::Uuid::parse_str(value).map_err(|_| {
                    BifrostCatalogError::MetadataMismatch(
                        "current snapshot has malformed forge.operation_id".to_owned(),
                    )
                })
            })
            .transpose()?;
        let manifests = iceberg_table.manifest_list_reader(snapshot).load().await?;
        for manifest_file in manifests.entries() {
            let manifest = manifest_file.load_manifest(iceberg_table.file_io()).await?;
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                let path = entry.data_file().file_path().to_owned();
                let canonical = path
                    .strip_prefix(iceberg_table.metadata().location())
                    .and_then(|suffix| suffix.strip_prefix('/'))
                    .map(|suffix| format!("{}/{suffix}", binding.object_prefix))
                    .unwrap_or(path);
                if binding.validate_object_path(&canonical).is_none() {
                    return Err(BifrostCatalogError::MetadataMismatch(
                        "pinned snapshot contains a path outside the tenant/table binding"
                            .to_owned(),
                    ));
                }
                estimated_bytes = estimated_bytes
                    .checked_add(entry.data_file().file_size_in_bytes())
                    .ok_or_else(|| {
                        BifrostCatalogError::MetadataMismatch(
                            "sealed byte estimate overflow".to_owned(),
                        )
                    })?;
                let pinned = PinnedIcebergFile {
                    file_path: canonical.clone(),
                    file_size: entry.data_file().file_size_in_bytes(),
                    row_count: entry.data_file().record_count(),
                };
                if files
                    .insert(canonical.clone(), pinned.clone())
                    .is_some_and(|existing| existing != pinned)
                {
                    return Err(BifrostCatalogError::MetadataMismatch(
                        "pinned snapshot repeats a data path with conflicting metadata".to_owned(),
                    ));
                }
                file_paths.insert(canonical);
            }
        }
        Ok(PinnedIcebergState {
            snapshot_id,
            publication_operation_id,
            file_paths,
            files,
            estimated_bytes,
        })
    }

    /// Converts one already validated relative table object path into a storage URI.
    ///
    /// # Errors
    ///
    /// Returns a binding error when the path escapes the tenant-qualified table prefix.
    pub(crate) fn object_location(
        &self,
        binding: &TenantTableBinding,
        path: &str,
    ) -> Result<String, BifrostCatalogError> {
        let path = binding.validate_object_path(path).ok_or_else(|| {
            BifrostCatalogError::InvalidBinding(
                "object path escapes the tenant-qualified table prefix".to_owned(),
            )
        })?;
        Ok(format!("{}/{}", self.warehouse.trim_end_matches('/'), path))
    }
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
        let file_io = FileIOBuilder::new(Arc::clone(&storage_factory))
            .with_props(storage_properties.clone())
            .build();
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
            file_io,
        })
    }

    /// Borrow the exact Iceberg catalog used by Redux physical table registration.
    #[must_use]
    pub fn iceberg_catalog(&self) -> Arc<dyn iceberg::Catalog> {
        Arc::clone(&self.catalog)
    }

    /// Clones the exact storage adapter used by Redux Iceberg tables.
    #[must_use]
    pub fn file_io(&self) -> FileIO {
        self.file_io.clone()
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
        physical_layout: Option<PhysicalLayoutWire>,
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
                physical_layout,
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
                physical_layout: Some((definition.physical_layout)()),
                audit: None,
            },
            Some((definition.schema)()),
        )
        .await
    }

    /// Resolve one canonical layout and register the tenant-qualified table.
    ///
    /// The order is fixed and fail-closed: caller input is rejected, the
    /// binding and physical schema are resolved, the layout is canonicalized —
    /// all before a transaction opens. Inside the tenant transaction the
    /// advisory lock serializes concurrent registrations of the same FQN; an
    /// existing row is compared on schema fingerprint first, then on layout, so
    /// a schema conflict never masquerades as a layout conflict. An exact
    /// repeat of a prior registration is a no-op returning the same
    /// [`TableUid`].
    ///
    /// `canonical_schema` is the built-in's own complete physical schema when
    /// this registration comes from [`Self::ensure_builtin`], and `None` for a
    /// caller registration, whose physical schema is derived from its user
    /// fields. It selects no resolver: every declaration, built-in or caller,
    /// resolves through the one [`PhysicalLayout::resolve`] entry point.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::Layout`] for an invalid or conflicting
    /// declaration, [`BifrostCatalogError::FingerprintMismatch`] for a schema
    /// conflict, and metadata, Iceberg, SQL, or audit errors otherwise.
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
        let (arrow_schema, layout) = resolve_registration_layout(
            &fqn,
            &request.user_fields,
            request.physical_layout.as_ref(),
            canonical_schema.as_deref(),
        )?;
        let layout_wire = layout.to_wire();
        let layout_json = serde_json::to_value(&layout_wire).map_err(|error| {
            BifrostCatalogError::MetadataMismatch(format!(
                "canonical physical layout for {fqn} is not encodable: {error}"
            ))
        })?;

        let mut conn = self.postgres.tenant_conn(request.tenant).await?;
        acquire_table_advisory_lock(&mut conn, request.tenant, &fqn).await?;
        let row = vala_sql::queries::olap_catalog::get_by_fqn(&mut conn, &fqn).await?;
        let table_ident = binding.table_ident();
        let physical_exists = self.catalog.table_exists(&table_ident).await?;

        if let Some(row) = row {
            if row.fingerprint.as_slice() != fingerprint.as_ref() {
                return Err(BifrostCatalogError::FingerprintMismatch(fqn));
            }
            if layout_wire_from_row(&row)? != layout_wire {
                return Err(BifrostCatalogError::Layout(
                    wyrd_spec::vala::BifrostError::PhysicalLayoutMismatch { table: fqn },
                ));
            }
            if !physical_exists {
                return Err(BifrostCatalogError::MetadataMismatch(format!(
                    "control registration exists without physical table: {table_ident}"
                )));
            }
            let physical = self.catalog.load_table(&table_ident).await?;
            self.validate_physical_table(&physical, &binding, &arrow_schema, &layout)?;
            conn.commit().await?;
            return TableUid::from_row(&row.table_uid, &row.fqn);
        }

        self.ensure_namespace(binding.physical_namespace()).await?;
        if physical_exists {
            let physical = self.catalog.load_table(&table_ident).await?;
            self.validate_physical_table(&physical, &binding, &arrow_schema, &layout)?;
        } else {
            let iceberg_schema =
                iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&arrow_schema)?;
            let partition_spec = layout
                .iceberg_partition_spec(&iceberg_schema)
                .map_err(BifrostCatalogError::MetadataMismatch)?;
            let sort_order = layout
                .iceberg_sort_order(&iceberg_schema)
                .map_err(BifrostCatalogError::MetadataMismatch)?;
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
                .properties(std::collections::HashMap::from([(
                    crate::catalog::layout::BLOOM_COLUMNS_PROPERTY.to_owned(),
                    layout.bloom_columns_property(),
                )]))
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
            &layout_json,
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
    /// physical state, preventing a control-plane row from silently pointing at
    /// a table with a different location, schema, time partition, or Forge sort
    /// recipe. Every physical assertion is derived from `layout`, so the
    /// canonical layout is the single authority for what "correct" means.
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
        layout: &PhysicalLayout,
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
        let schema = table.metadata().current_schema();
        let partition_source_id = schema
            .field_by_name(layout.partition().column())
            .map(|field| field.id)
            .ok_or_else(|| {
                BifrostCatalogError::MetadataMismatch(format!(
                    "physical schema lacks partition column {}",
                    layout.partition().column()
                ))
            })?;
        let expected_partition_name = layout.iceberg_partition_field_name();
        let fields = table.metadata().default_partition_spec().fields();
        if fields.len() != 1
            || fields[0].source_id != partition_source_id
            || fields[0].name != expected_partition_name
            || fields[0].transform != layout.iceberg_transform()
        {
            return Err(BifrostCatalogError::MetadataMismatch(format!(
                "physical table partition spec mismatch: expected one {expected_partition_name} field"
            )));
        }
        let expected_sort_fields = layout
            .sort_keys()
            .iter()
            .map(|key| {
                schema.field_by_name(&key.column).map(|field| {
                    (
                        field.id,
                        key.direction.to_iceberg(),
                        key.null_order.to_iceberg(),
                    )
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                BifrostCatalogError::MetadataMismatch(
                    "physical schema lacks Forge sort columns".to_owned(),
                )
            })?;
        let sort_fields = &table.metadata().default_sort_order().fields;
        if sort_fields.len() != expected_sort_fields.len()
            || sort_fields.iter().zip(expected_sort_fields).any(
                |(field, (source_id, direction, null_order))| {
                    field.source_id != source_id
                        || field.transform != Transform::Identity
                        || field.direction != direction
                        || field.null_order != null_order
                },
            )
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "physical table sort order mismatch".to_owned(),
            ));
        }
        Ok(())
    }

    /// Return the registered schema fingerprint and canonical physical layout
    /// for one tenant/logical table in a single control-row read.
    ///
    /// Scribe calls this once per admitted frame to resolve the table's
    /// partition granularity and Bloom recipe before planning, so no ingest
    /// path invents or defaults a layout of its own.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the tenant does not
    /// own the registration, or a metadata error for a malformed fingerprint or
    /// an undecodable stored layout.
    pub async fn table_registration(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<(SchemaFingerprint, PhysicalLayoutWire), BifrostCatalogError> {
        let fqn = table.fqn();
        let row = self
            .lookup_table_row(&fqn, tenant)
            .await?
            .ok_or_else(|| BifrostCatalogError::TableNotFound(fqn.clone()))?;
        let fingerprint: [u8; 32] = row.fingerprint.as_slice().try_into().map_err(|_| {
            BifrostCatalogError::MetadataMismatch(format!(
                "schema fingerprint length mismatch for {fqn}"
            ))
        })?;
        Ok((SchemaFingerprint(fingerprint), layout_wire_from_row(&row)?))
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

    /// Return the physical Arrow schema a follower scan assignment must be
    /// built against for one tenant-qualified table.
    ///
    /// This is the provider's schema including managed columns, not the
    /// registered user schema. Both the assignment's projection closure and its
    /// fingerprint are derived from it, and the follower re-derives the same
    /// schema after resolution, so a caller that guesses either one is rejected.
    ///
    /// # Errors
    /// Returns a catalog error when the tenant does not own the registration or
    /// the physical provider cannot be built.
    pub async fn assignment_schema(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<arrow::datatypes::SchemaRef, BifrostCatalogError> {
        let provider = self.provider(table, tenant).await?;
        Ok(datafusion::datasource::TableProvider::schema(&provider))
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
            physical_layout: layout_wire_from_row(&row)?,
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

/// Compares physical schema shapes across Arrow and Iceberg representations.
///
/// The sole spelling alias is the UTC timestamp timezone emitted as `UTC` by
/// Arrow and `+00:00` by Iceberg. Field order, names, nullability, units, and
/// every other data-type detail remain exact.
pub(crate) fn schema_shape_matches(expected: &Schema, actual: &Schema) -> bool {
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

/// Resolves the physical schema and canonical layout one registration writes.
///
/// The physical schema is `canonical_schema` when the caller supplied one — the
/// built-in's own complete schema — and `user_fields` plus the managed column
/// set otherwise.
///
/// There is one resolver. Built-in and caller declarations both run
/// [`PhysicalLayout::resolve`], so a built-in and a dynamic table declaring the
/// same layout over the same schema canonicalize byte-identically and the
/// engine can never refuse a declaration a caller could have made.
///
/// # Errors
///
/// Returns [`BifrostCatalogError::Layout`] when the declaration carries more
/// than [`MAX_SORT_KEYS`](crate::catalog::layout::MAX_SORT_KEYS) sort keys, or
/// names a column absent from the resolved schema or repeated within one list.
fn resolve_registration_layout(
    fqn: &str,
    user_fields: &[Field],
    declared: Option<&PhysicalLayoutWire>,
    canonical_schema: Option<&Schema>,
) -> Result<(Schema, PhysicalLayout), BifrostCatalogError> {
    let arrow_schema = canonical_schema.map_or_else(
        || Schema::new(with_managed_columns(user_fields.to_vec())),
        Clone::clone,
    );
    let layout = PhysicalLayout::resolve(fqn, &arrow_schema, declared)
        .map_err(BifrostCatalogError::Layout)?;
    Ok((arrow_schema, layout))
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

#[cfg(test)]
mod schema_shape_tests {
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    use super::schema_shape_matches;

    /// UTC and Iceberg's equivalent offset spelling have the same physical shape.
    #[test]
    fn schema_shape_accepts_utc_offset_alias() {
        let utc = Schema::new(vec![Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        )]);
        let offset = Schema::new(vec![Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
            false,
        )]);

        assert!(schema_shape_matches(&utc, &offset));
        assert!(schema_shape_matches(&offset, &utc));
    }

    /// A non-UTC timezone remains a different physical schema shape.
    #[test]
    fn schema_shape_rejects_non_utc_timezone() {
        let utc = Schema::new(vec![Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        )]);
        let other = Schema::new(vec![Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("America/New_York".into())),
            false,
        )]);

        assert!(!schema_shape_matches(&utc, &other));
    }
}
