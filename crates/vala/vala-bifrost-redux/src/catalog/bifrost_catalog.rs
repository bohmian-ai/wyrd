//! Redux-owned tenant-qualified Bifrost catalog.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, RwLock};

use arrow::datatypes::{Field, Schema, SchemaRef};
use iceberg::io::object_cache::ObjectCache;
use iceberg::io::{FileIO, FileIOBuilder};
use iceberg::spec::{
    self, FormatVersion, TableMetadata, TableMetadataRef, TableProperties, Transform,
};
use iceberg::table::Table;
use iceberg::{Catalog as _, Error as IcebergError, TableCreation, TableIdent};
use iceberg_catalog_sql::SqlCatalog;
use sha2::{Digest as _, Sha256};
use uuid::Uuid;
use vala_sql::ValaPostgres;
use vala_sql::queries::file_list::HotFileCut;
use vala_sql::queries::oracle_reader_authority::{CutTableRef, OracleTableCuts, TableCut};
use vala_sql::row_types::file_list::HotFileRow;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::vala::api::{
    BifrostTableDescription, BifrostTableEntry, CompactionTypeWire, PhysicalLayoutWire,
    RegisterOutcome, RegisterTableResponse,
};
use wyrd_spec::vala::ids::TableVersion;

use crate::catalog::cut_cache::{CUT_CHANNEL, CutCache};
use crate::catalog::error::BifrostCatalogError;
use crate::catalog::event_time::{EventTimeBoundsDefect, EventTimeStatistics};
use crate::catalog::iceberg_sql;
use crate::catalog::layout::PhysicalLayout;
use iceberg::io::StorageFactory;

use crate::catalog::iceberg_storage::{BifrostIcebergStorage, BifrostIcebergStorageFactory};
use crate::catalog::storage::{iceberg_catalog_properties, warehouse_uri};
use crate::catalog::wire::{
    described_fields_from_stored_schema, entry_from_row, layout_wire_from_row,
    reject_reserved_field_names,
};
use crate::catalog::{TableRef, TenantTableBinding};
use crate::forge::{COMPACTION_TYPE_PROPERTY, ForgeCompactionType};
use crate::namespaces::BifrostNamespace;
use crate::schema::SchemaFingerprint;
use crate::storage::BifrostStorage;
use crate::tables::{BuiltinTableDefinition, builtin_table};
use iceberg_datafusion::IcebergStaticTableProvider;

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
    /// Mints a fresh time-ordered identity for a table being registered.
    ///
    /// A version-7 UUID keeps newly registered table ids roughly insertion-ordered in
    /// `vala.bifrost_tables`; the identity is opaque to every other reader.
    fn new_v7() -> Self {
        Self(*Uuid::now_v7().as_bytes())
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

    /// Wraps a stable 16-byte identity already issued by the catalog.
    ///
    /// Used where a registered UID crosses a boundary as raw bytes, such as a
    /// Scribe test double answering Gate's destination resolution.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

/// What one locked registration decided.
///
/// Built-in provisioning needs only the uid; a caller registration answers
/// with all three, so the outcome and fingerprint come from the transaction
/// that decided them rather than from a separate pre-check.
struct Registration {
    /// The registered table's uid, new or existing.
    table_uid: TableUid,
    /// Whether this call created the table or matched an existing one.
    outcome: RegisterOutcome,
    /// The stored user-schema fingerprint the registration was compared on.
    fingerprint: SchemaFingerprint,
    /// The physical table's current version, `1` for a table this call created.
    table_version: TableVersion,
}

/// Inputs for one tenant-qualified physical table registration.
pub struct CreateTableRequest {
    /// Tenant-free logical table identity.
    pub table: TableRef,
    /// User fields before Redux appends server-owned columns.
    pub user_fields: Vec<Field>,
    /// Authenticated organization that owns the registration and physical table.
    pub tenant: DataTenantId,
    /// Verified subject and actor registering the table, stored once on its
    /// control row.
    pub attribution: Attribution,
    /// Optional caller-declared physical layout.
    ///
    /// `None` means "no declaration": the catalog resolves the canonical
    /// default layout. `Some` is canonicalized and validated against the
    /// complete physical schema before any durable mutation.
    pub physical_layout: Option<PhysicalLayoutWire>,
}

/// Immutable metadata cut for one tenant-qualified sealed table.
#[derive(Debug)]
pub struct PinnedSealedTable {
    /// Authenticated tenant/table binding.
    pub binding: TenantTableBinding,
    /// Durable registered table UID, which is this table's protection identity.
    ///
    /// Reader protection is keyed by the registered UID rather than by the
    /// namespace and name it happens to be reachable under, so the cut carries
    /// the UID it was resolved from instead of letting a later consumer
    /// reconstruct one.
    pub table_uid: TableUid,
    /// Iceberg table metadata loaded once for this cut.
    pub iceberg_table: Table,
    /// Exact current snapshot identity selected from the immutable table metadata.
    pub snapshot_id: Option<i64>,
    /// Stable digest of the selected snapshot identity and live data-file set.
    pub snapshot_digest: String,
    /// Canonical live data-file identities in the pinned Iceberg snapshot.
    pub iceberg_file_paths: BTreeSet<String>,
    /// Ordered immutable data-file work from the pinned Iceberg snapshot.
    pub iceberg_files: Vec<PinnedIcebergFile>,
    /// Ordered tenant hot rows absent from the exact pinned snapshot manifest.
    pub hot_files: Vec<HotFileRow>,
    /// Stable digest of the ordered, post-subtraction hot manifest.
    pub hot_manifest_digest: String,
    /// Bounded sealed-byte estimate used by Oracle classification.
    pub estimated_bytes: u64,
    /// The table's publication revision per Scribe node, read in the same
    /// statement snapshot as `hot_files`; a node absent here has revision 0.
    ///
    /// Oracle compares it with the revision each Scribe reports with its live
    /// capture and keeps the selection only when no Scribe reports more.
    pub publication_revisions: std::collections::HashMap<uuid::Uuid, i64>,
}

/// Immutable file metadata retained from one pinned Iceberg manifest entry.
///
/// The committed manifest entry is the authority for a compacted or
/// Forge-rewritten object: `vala.file_list` never receives a rewrite output, so
/// a matching row is normal to be absent and is never required to establish
/// this file's size, row count, or event-time interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedIcebergFile {
    /// Canonical tenant-qualified object path.
    pub file_path: String,
    /// Exact object byte size from the pinned manifest.
    pub file_size: u64,
    /// Exact record count from the pinned manifest.
    pub row_count: u64,
    /// Normalized time-column interval decoded from the manifest entry's typed
    /// lower/upper bounds, or the exact reason the file must be retained.
    pub event_time: EventTimeStatistics,
}

impl PinnedIcebergFile {
    /// Projects one pinned manifest entry into immutable pruning statistics.
    ///
    /// `time_field_id` is the time column's field identity: the source of the
    /// one time partition field of the manifest's *own* partition spec, not of
    /// the current table metadata, so a file written under an older spec is
    /// read by the field it was partitioned on. `None` means that spec has no
    /// time partition field, which is a missing-bounds fail-open rather than
    /// an error.
    ///
    /// Decoding never fails the pin. Every defect normalizes into
    /// [`EventTimeStatistics::Unusable`] so the file is retained and executed
    /// through the residual predicate and footer tenant proof.
    #[must_use]
    fn from_manifest_entry(
        file_path: String,
        data_file: &iceberg::spec::DataFile,
        time_field_id: Option<i32>,
    ) -> Self {
        let event_time = match time_field_id {
            None => EventTimeStatistics::Unusable(EventTimeBoundsDefect::Missing),
            Some(field_id) => EventTimeStatistics::from_manifest_bounds(data_file, field_id),
        };
        Self {
            file_path,
            file_size: data_file.file_size_in_bytes(),
            row_count: data_file.record_count(),
            event_time,
        }
    }
}

/// Immutable Iceberg snapshot metadata collected before hot-manifest reconciliation.
struct PinnedIcebergState {
    /// Current snapshot identity, when the table has committed data.
    snapshot_id: Option<i64>,
    /// Validated Forge publication operation recorded by the current snapshot.
    forge_publication_operation_id: Option<Uuid>,
    /// Canonical paths used to exclude hot-manifest overlap.
    file_paths: BTreeSet<String>,
    /// Exact immutable metadata keyed by canonical path.
    files: BTreeMap<String, PinnedIcebergFile>,
    /// Checked sum of immutable object bytes.
    estimated_bytes: u64,
}

/// One registration row's immutable facts, as the node caches them.
///
/// Filled by a single control-row read; every per-frame write resolution and
/// per-query table resolution answers from it afterwards.
#[derive(Clone)]
struct CachedRegistration {
    /// UID minted once at registration.
    table_uid: TableUid,
    /// Registered user-schema fingerprint.
    fingerprint: SchemaFingerprint,
    /// Canonical physical layout stored on the row.
    layout: PhysicalLayoutWire,
}

/// Redux catalog shared by Gate, Forge, Oracle, and server catalog routes.
#[derive(Clone)]
pub struct BifrostCatalog {
    /// The one Iceberg SQL catalog every registration and load goes through.
    catalog: Arc<SqlCatalog>,
    postgres: ValaPostgres,
    warehouse: String,
    file_io: FileIO,
    /// The node's storage owner bound to this warehouse, which `DataFusion`'s
    /// Parquet scan and Iceberg's footer loads read every Oracle object through.
    objects: Arc<BifrostIcebergStorage>,
    /// Node-wide decoded manifest and manifest-list cache shared by every
    /// read table, so the snapshot pin and `plan_files` decode
    /// each immutable manifest once per node instead of once per consumer per
    /// query. Seeded lazily because iceberg only builds a cache with a table.
    manifest_cache: Arc<std::sync::OnceLock<Arc<ObjectCache>>>,
    /// Node-wide parsed Iceberg metadata documents, keyed by their location.
    ///
    /// Every commit writes a new `<version>-<uuid>.metadata.json` and moves
    /// the catalog pointer to it, so a location's document never changes and
    /// needs no invalidation: a publication is seen because the active cut
    /// returns the new location. Concurrent misses on one location each read
    /// it once; nothing waits on another query's read.
    metadata_cache: mini_moka::sync::Cache<String, TableMetadataRef>,
    /// Registrations this node has already read, by tenant and table name.
    ///
    /// A registration's UID is minted once and `(tenant, fqn)` is unique; its
    /// fingerprint and layout change only with a new table version, after
    /// which [`BifrostCatalog::evict_current_definition`] drops the entry.
    /// Only found rows are cached, so a table registered later is still
    /// seen on its first lookup. One control-row read fills the UID,
    /// fingerprint, and layout every write and query resolves. Grows with
    /// registered tables, never with writes or queries.
    registrations: Arc<RwLock<HashMap<(DataTenantId, String), CachedRegistration>>>,
    /// Current table definitions this node has already loaded, by tenant and
    /// table name.
    ///
    /// The definition is what new admission binds: its schema is the field-id
    /// authority Scribe stamps into every Parquet object. An entry stays until
    /// [`BifrostCatalog::evict_current_definition`] drops it after a new
    /// version commits; batches already accepted are unaffected because each
    /// carries its own version. Grows with registered tables, never with
    /// writes.
    current_definitions: Arc<RwLock<CurrentDefinitions>>,
    /// Node-local Oracle cut cache, served only while [`Self::watch_cuts`]
    /// is listening for invalidations.
    cut_cache: Arc<CutCache>,
}

/// Current table definitions a node has loaded, keyed by tenant and table
/// name.
type CurrentDefinitions = HashMap<(DataTenantId, String), Arc<TableDefinition>>;

/// The complete definition one table version declares.
///
/// Rebuilt from that version's native Iceberg metadata, so every batch bound
/// to the version is admitted, sealed, recovered, and published under the
/// same schema, spec, sort order, and Bloom set.
#[derive(Debug)]
pub struct TableDefinition {
    /// Version this definition belongs to.
    pub table_version: TableVersion,
    /// Default partition-spec id the version partitions under.
    pub spec_id: i32,
    /// Default sort-order id the version writes under.
    pub sort_order_id: i32,
    /// The version's Iceberg schema, the field-id authority Scribe stamps.
    pub schema: Arc<spec::Schema>,
    /// Layout the version declares.
    pub layout: PhysicalLayout,
    /// Partition scheme the version plans slices under, bound to the version.
    pub partition_scheme: crate::catalog::PartitionScheme,
}

impl TableDefinition {
    /// Rebuilds the definition one metadata document declares.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::MetadataMismatch`] when the document has
    /// no canonical version, its sort-order id leaves the `i32` domain, or its
    /// layout or partition scheme does not rebuild.
    fn of_metadata(fqn: &str, metadata: &TableMetadata) -> Result<Self, BifrostCatalogError> {
        let mismatch = |error: wyrd_spec::vala::BifrostError| {
            BifrostCatalogError::MetadataMismatch(error.to_string())
        };
        let table_version =
            crate::catalog::layout::table_version(fqn, metadata).map_err(mismatch)?;
        let spec_id = metadata.default_partition_spec_id();
        let sort_order_id = i32::try_from(metadata.default_sort_order_id()).map_err(|_| {
            BifrostCatalogError::MetadataMismatch(format!(
                "{fqn} default sort-order id exceeds the i32 domain"
            ))
        })?;
        let (layout, schema) =
            PhysicalLayout::of_iceberg(fqn, metadata, spec_id).map_err(mismatch)?;
        let partition_scheme = crate::catalog::PartitionScheme::of_wire(&layout.to_wire(), spec_id)
            .map_err(|error| {
                BifrostCatalogError::MetadataMismatch(format!(
                    "{fqn} partition layout is invalid: {error}"
                ))
            })?
            .with_table_version(table_version);
        Ok(Self {
            table_version,
            spec_id,
            sort_order_id,
            schema,
            layout,
            partition_scheme,
        })
    }
}

/// Estimated-weight eviction limit of the node-wide decoded manifest cache.
///
/// Manifest and manifest-list paths embed a commit UUID, so entries are
/// immutable and never need invalidation; eviction is size-only LRU over
/// iceberg's estimated entry weight. This is not a hard memory ceiling: a
/// query still holding an evicted `Arc`, or loads in flight, can briefly
/// exceed it. It sits outside the Oracle memory root because decoded
/// manifests are small and shared across every query.
const MANIFEST_CACHE_BYTES: u64 = 64 * 1024 * 1024;

/// Entry limit of the node-wide parsed metadata cache.
///
/// Only each table's current document is read by new queries, so the limit
/// bounds tables a node serves concurrently, with older versions evicted by
/// LRU. ponytail: counted in entries, not bytes; weigh by document size if
/// tables with long snapshot histories make entries large.
const METADATA_CACHE_ENTRIES: u64 = 1024;

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

/// Cut-read statements observed in serialized tests.
///
/// A successful query reads its whole cut in one statement and a metadata
/// `NotFound` race in exactly two, so the count is the observable that proves
/// neither per-table nor unbounded reads.
#[cfg(any(test, feature = "test-support"))]
static TEST_CUT_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Metadata reads still to be failed as object-store `NotFound`.
#[cfg(any(test, feature = "test-support"))]
static TEST_METADATA_NOT_FOUND_FAULTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Resets and returns the observed cut-read count.
#[cfg(any(test, feature = "test-support"))]
pub fn reset_cut_reads_for_test() -> usize {
    TEST_CUT_READS.swap(0, std::sync::atomic::Ordering::SeqCst)
}

/// Makes the next `count` cut metadata reads fail the way a removed
/// document does: by reading a location that does not exist.
///
/// Stands in for a catalog move whose superseded document was deleted between
/// cut read and the metadata read, which a test cannot place at that exact
/// point from outside the query.
#[cfg(any(test, feature = "test-support"))]
pub fn inject_metadata_not_found_for_test(count: usize) {
    TEST_METADATA_NOT_FOUND_FAULTS.store(count, std::sync::atomic::Ordering::SeqCst);
}

impl BifrostCatalog {
    /// Reads one query's complete cut in one read-only tenant statement.
    ///
    /// Runs [`OracleTableCuts::read`] as one autocommit statement on a pooled
    /// connection: the statement binds the tenant itself, so the cut costs one
    /// round trip with no transaction. Nothing is recorded: the returned pointers stay readable for the
    /// query's deadline because Forge retains a replaced snapshot for at least
    /// the maximum query deadline.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::InvalidBinding`] when a table cannot be
    /// bound to `tenant`, [`BifrostCatalogError::TableNotFound`] when any table
    /// has no registration or catalog pointer visible to the tenant, and a SQL
    /// error when the connection or statement fails.
    pub async fn read_cut(
        &self,
        tenant: DataTenantId,
        tables: &[TableRef],
    ) -> Result<Vec<TableCut>, BifrostCatalogError> {
        #[cfg(any(test, feature = "test-support"))]
        TEST_CUT_READS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let bindings = tables
            .iter()
            .map(|table| {
                TenantTableBinding::resolve((tenant, table.clone()))
                    .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let refs = bindings
            .iter()
            .map(|binding| CutTableRef {
                namespace_name: &binding.logical_namespace,
                table_name: &binding.table_name,
            })
            .collect::<Vec<_>>();
        let mut conn = self.postgres.conn().await?;
        let cut = match OracleTableCuts::new(&mut conn, tenant).read(&refs).await {
            Err(vala_sql::SqlError::NoRows) => {
                let names = tables.iter().map(TableRef::fqn).collect::<Vec<_>>();
                return Err(BifrostCatalogError::TableNotFound(names.join(", ")));
            }
            other => other?,
        };
        Ok(cut)
    }

    /// Returns one query's cut from the node's cut cache, or reads it with
    /// [`Self::read_cut`] and caches it.
    ///
    /// Only cuts read at or after `not_before` are served (any when `None`);
    /// Oracle passes its deadline minus the maximum query deadline, so every
    /// served pointer outlives the query (see the `cut_cache` module). The cache serves
    /// nothing until [`Self::watch_cuts`] is listening, so without it every
    /// call reads Postgres.
    ///
    /// # Errors
    /// Returns the errors of [`Self::read_cut`]; a cache hit cannot fail.
    pub async fn read_cut_cached(
        &self,
        tenant: DataTenantId,
        tables: &[TableRef],
        not_before: Option<std::time::Instant>,
    ) -> Result<Vec<TableCut>, BifrostCatalogError> {
        let bindings = tables
            .iter()
            .map(|table| {
                TenantTableBinding::resolve((tenant, table.clone()))
                    .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let refs = bindings
            .iter()
            .map(|binding| CutTableRef {
                namespace_name: &binding.logical_namespace,
                table_name: &binding.table_name,
            })
            .collect::<Vec<_>>();
        if let Some(cut) = self.cut_cache.get(tenant, &refs, not_before) {
            return Ok(cut);
        }
        let generation = self.cut_cache.fill_generation();
        let cut = self.read_cut(tenant, tables).await?;
        if let Some(generation) = generation {
            self.cut_cache
                .insert(generation, std::time::Instant::now(), &cut);
        }
        Ok(cut)
    }

    /// Keeps the node's cut cache coherent until `shutdown`: listens on one
    /// Vala pool connection for the `vala.file_list` trigger's notifications
    /// and drops each cut they name. See [`CutCache::watch`].
    pub async fn watch_cuts(&self, shutdown: tokio_util::sync::CancellationToken) {
        self.cut_cache
            .watch(&self.postgres.listener(CUT_CHANNEL), shutdown)
            .await;
    }

    /// Drops `table`'s cached cut so the next read reaches Postgres.
    ///
    /// Oracle calls it when a captured Scribe reports a publication revision
    /// above the cut's: that cut predates rows the Scribe no longer serves
    /// live. A table that cannot be bound has no cached cut and is ignored.
    pub fn evict_cut(&self, tenant: DataTenantId, table: &TableRef) {
        if let Ok(binding) = TenantTableBinding::resolve((tenant, table.clone())) {
            self.cut_cache.evict(
                tenant,
                &CutTableRef {
                    namespace_name: &binding.logical_namespace,
                    table_name: &binding.table_name,
                },
            );
        }
    }

    /// Materializes one table of a read cut.
    ///
    /// Reads the exact metadata document the cut returned — from the
    /// node's metadata cache when this location was already parsed, never by
    /// rereading the pointer — pins its current snapshot's manifests,
    /// and reconciles the cut's unresolved hot candidates against that
    /// snapshot with the shared exact hot/Iceberg rule. The returned cut is
    /// then validated row by row against the tenant/table binding.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::InvalidBinding`] when the table cannot be
    /// bound, [`BifrostCatalogError::MetadataMismatch`] when the cut
    /// identity disagrees with the requested table or a manifest or hot row
    /// violates the binding, [`BifrostCatalogError::AmbiguousPublication`] when
    /// a candidate cannot be placed, and [`BifrostCatalogError::Iceberg`] when
    /// the metadata document or a manifest cannot be read; a missing document
    /// reports [`BifrostCatalogError::is_missing_object`].
    pub async fn materialize_cut(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        acquired: TableCut,
    ) -> Result<PinnedSealedTable, BifrostCatalogError> {
        #[cfg(any(test, feature = "test-support"))]
        TEST_SEALED_PIN_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        if acquired.identity.tenant != tenant
            || acquired.identity.namespace_name != binding.logical_namespace
            || acquired.identity.table_name != binding.table_name
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "cut identity disagrees with the requested table".to_owned(),
            ));
        }
        let table_uid = TableUid::from_bytes(acquired.identity.table_uid);
        let publication_revisions = acquired.publication_revisions;
        let identifier = binding.table_ident();
        // A fault reads a document that does not exist, through the same
        // `FileIO`, so the reacquisition sees the production error shape.
        #[cfg(any(test, feature = "test-support"))]
        if TEST_METADATA_NOT_FOUND_FAULTS
            .try_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |remaining| remaining.checked_sub(1),
            )
            .is_ok()
        {
            let missing = format!("{}.removed", acquired.metadata_location);
            TableMetadata::read_from(&self.file_io, &missing).await?;
        }
        let metadata = if let Some(metadata) = self.metadata_cache.get(&acquired.metadata_location)
        {
            metadata
        } else {
            let metadata = Arc::new(
                TableMetadata::read_from(&self.file_io, &acquired.metadata_location).await?,
            );
            self.metadata_cache
                .insert(acquired.metadata_location.clone(), Arc::clone(&metadata));
            metadata
        };
        let iceberg_table =
            self.cached_table(&identifier, metadata, Some(acquired.metadata_location))?;
        let pinned = self.pin_iceberg_snapshot(&iceberg_table, &binding).await?;
        let cut = HotFileCut::reconcile(
            acquired.hot_files,
            &pinned.file_paths,
            pinned.forge_publication_operation_id,
        );
        let snapshot_id = pinned.snapshot_id;
        let iceberg_file_paths = pinned.file_paths;
        let iceberg_files = pinned.files;
        if cut.ambiguous_publication {
            return Err(BifrostCatalogError::AmbiguousPublication);
        }
        let hot_files = cut.hot_files;
        let estimated_bytes = hot_cut_bytes(&binding, &hot_files, pinned.estimated_bytes)?;
        metrics::counter!(
            "bifrost_oracle_files_pruned_total",
            "source" => "hot_sealed",
            "reason" => "snapshot_overlap"
        )
        .increment(cut.represented as u64);
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
            table_uid,
            iceberg_table,
            snapshot_id,
            snapshot_digest,
            iceberg_file_paths,
            iceberg_files: iceberg_files.into_values().collect(),
            hot_files,
            hot_manifest_digest,
            estimated_bytes,
            publication_revisions,
        })
    }

    /// Builds one immutable read table over this catalog's storage and the
    /// node-wide manifest cache.
    ///
    /// Uses the supplied metadata rather than reloading it, so building the
    /// table opens nothing. Cache keys are full manifest paths, so a hit can
    /// only be a manifest under this tenant's own table prefix.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::Iceberg`] when the table cannot be built
    /// from the identity and metadata.
    fn cached_table(
        &self,
        identifier: &TableIdent,
        metadata: TableMetadataRef,
        metadata_location: Option<String>,
    ) -> Result<Table, BifrostCatalogError> {
        let manifest_cache = self.manifest_cache(identifier, &metadata)?;
        let mut builder = Table::builder()
            .file_io(self.file_io.clone())
            .metadata(metadata)
            .identifier(identifier.clone())
            .runtime(iceberg::Runtime::current())
            .disable_cache();
        if let Some(location) = metadata_location {
            builder = builder.metadata_location(location);
        }
        Ok(builder.build()?.with_object_cache(manifest_cache))
    }

    /// Returns the node-wide manifest cache, seeding it on first use.
    ///
    /// iceberg only constructs an `ObjectCache` as part of a table, so the
    /// first caller builds a throwaway table over the catalog's `FileIO` with
    /// [`MANIFEST_CACHE_BYTES`] and keeps its cache. Every consumer attaches
    /// the entries with `Table::with_object_cache`. A racing first caller builds a redundant seed and
    /// discards it.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::Iceberg`] when the seed table cannot be
    /// built from the supplied identity and metadata.
    fn manifest_cache(
        &self,
        identifier: &TableIdent,
        metadata: &TableMetadataRef,
    ) -> Result<Arc<ObjectCache>, BifrostCatalogError> {
        if let Some(cache) = self.manifest_cache.get() {
            return Ok(Arc::clone(cache));
        }
        let seed = Table::builder()
            .file_io(self.file_io.clone())
            .metadata(Arc::clone(metadata))
            .identifier(identifier.clone())
            .runtime(iceberg::Runtime::current())
            .cache_size_bytes(MANIFEST_CACHE_BYTES)
            .build()?;
        Ok(Arc::clone(
            self.manifest_cache.get_or_init(|| seed.object_cache()),
        ))
    }

    /// Collects and validates the immutable files of one current Iceberg snapshot.
    ///
    /// # Errors
    ///
    /// Returns a catalog error when manifests cannot be read, a path escapes the
    /// tenant binding, file metadata conflicts, or byte accounting overflows.
    async fn pin_iceberg_snapshot(
        &self,
        iceberg_table: &Table,
        binding: &TenantTableBinding,
    ) -> Result<PinnedIcebergState, BifrostCatalogError> {
        let snapshot_id = iceberg_table.metadata().current_snapshot_id();
        let mut file_paths = BTreeSet::new();
        let mut files = BTreeMap::new();
        let mut estimated_bytes = 0_u64;
        let Some(snapshot) = iceberg_table.metadata().current_snapshot() else {
            return Ok(PinnedIcebergState {
                snapshot_id,
                forge_publication_operation_id: None,
                file_paths,
                files,
                estimated_bytes,
            });
        };
        let forge_publication_operation_id = snapshot
            .summary()
            .additional_properties
            .get("forge.operation_id")
            .map(|value| {
                Uuid::parse_str(value).map_err(|_| {
                    BifrostCatalogError::MetadataMismatch(
                        "current snapshot has malformed forge.operation_id".to_owned(),
                    )
                })
            })
            .transpose()?;
        // Read through the table's shared cache so the `plan_files` that
        // follows reuses these decoded manifests instead of decoding again.
        let cache = iceberg_table.object_cache();
        let manifests = cache
            .get_manifest_list(snapshot, &iceberg_table.metadata_ref())
            .await?;
        for manifest_file in manifests.entries() {
            let manifest = cache.get_manifest(manifest_file).await?;
            // The time column is the source of the manifest's own one time
            // partition field, resolved once per manifest, so a file is read
            // by the field it was actually partitioned on.
            let time_field_id =
                crate::catalog::TimeAxis::iceberg_field_id(manifest.metadata().partition_spec());
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
                let pinned = PinnedIcebergFile::from_manifest_entry(
                    canonical.clone(),
                    entry.data_file(),
                    time_field_id,
                );
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
            forge_publication_operation_id,
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
    /// The catalog does not resolve its own backend client. It is handed this
    /// node's one storage owner and binds Iceberg to it, so catalog metadata
    /// I/O, hot-footer reads, and publication all reach the same backend under
    /// the same governance rather than through three independently configured
    /// clients that merely happen to agree.
    ///
    /// # Errors
    /// Returns a catalog error when the Iceberg SQL catalog cannot be loaded.
    pub async fn new(
        catalog_uri: &str,
        storage: Arc<BifrostStorage>,
        postgres: ValaPostgres,
    ) -> Result<Self, BifrostCatalogError> {
        let backend = storage.backend();
        let warehouse = warehouse_uri(backend);
        let storage_properties = iceberg_catalog_properties(backend);
        let storage_factory = Arc::new(BifrostIcebergStorageFactory::new(
            Arc::clone(&storage),
            &warehouse,
        )) as Arc<dyn StorageFactory>;
        let file_io = FileIOBuilder::new(Arc::clone(&storage_factory))
            .with_props(storage_properties.clone())
            .build();
        let catalog = iceberg_sql::build_catalog(
            catalog_uri,
            &warehouse,
            storage_factory,
            storage_properties.clone(),
        )
        .await?;
        Ok(Self {
            catalog: Arc::new(catalog),
            postgres,
            file_io,
            objects: Arc::new(BifrostIcebergStorage::new(storage, &warehouse)),
            warehouse,
            manifest_cache: Arc::new(std::sync::OnceLock::new()),
            metadata_cache: mini_moka::sync::Cache::new(METADATA_CACHE_ENTRIES),
            registrations: Arc::default(),
            current_definitions: Arc::default(),
            cut_cache: Arc::default(),
        })
    }

    /// Borrows the node's one storage owner this catalog binds Iceberg to.
    #[must_use]
    pub fn storage(&self) -> &Arc<BifrostStorage> {
        self.objects.storage()
    }

    /// Borrows this warehouse's object store, the one `DataFusion`'s Parquet scan
    /// and Iceberg's footer loads read every Oracle object through.
    #[must_use]
    pub(crate) fn objects(&self) -> &Arc<BifrostIcebergStorage> {
        &self.objects
    }

    /// Borrow the exact Iceberg catalog used by Redux physical table registration.
    #[must_use]
    pub fn iceberg_catalog(&self) -> Arc<dyn iceberg::Catalog> {
        Arc::clone(&self.catalog) as Arc<dyn iceberg::Catalog>
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
    /// Iceberg failures, or SQL failures.
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
        self.create_table_locked(request, None, CompactionRegistration::default())
            .await
            .map(|registration| registration.table_uid)
    }

    /// Register a caller-owned dataset in the tenant-qualified dataset namespace.
    ///
    /// Each option `compaction` declares becomes the new table's explicit
    /// Iceberg property — the file target as `write.target-file-size-bytes`,
    /// the type as `wyrd.forge.compaction.type`; an omitted option stores
    /// nothing and the table follows Forge's default. On an existing table each
    /// declared option must match the stored explicit value or be omitted.
    ///
    /// The answer is decided under the registration lock: `Created` for a new
    /// table, `AlreadyExists` for a repeat whose Iceberg schema, layout, and
    /// compaction match, and the table's uid and stored fingerprint either way.
    ///
    /// # Errors
    /// Returns a typed catalog error when the dataset name, schema, physical table,
    /// compaction target or type, or control row is invalid. A column with no
    /// Iceberg type is an Iceberg error: the caller refuses it first with
    /// `wyrd_types::schema::check_supported`.
    pub async fn register_dataset(
        &self,
        tenant: DataTenantId,
        attribution: Attribution,
        table: TableRef,
        user_fields: Vec<Field>,
        physical_layout: Option<PhysicalLayoutWire>,
        compaction: CompactionRegistration,
    ) -> Result<RegisterTableResponse, BifrostCatalogError> {
        if table.namespace != BifrostNamespace::Datasets {
            return Err(BifrostCatalogError::MetadataMismatch(
                "caller-owned registrations must use vala.datasets".to_owned(),
            ));
        }
        let registration = self
            .create_table_locked(
                CreateTableRequest {
                    table,
                    user_fields,
                    tenant,
                    attribution,
                    physical_layout,
                },
                None,
                compaction,
            )
            .await?;
        Ok(RegisterTableResponse {
            outcome: registration.outcome,
            table_uid: hex::encode(registration.table_uid.as_bytes()),
            fingerprint: hex::encode(registration.fingerprint.0),
            table_version: registration.table_version,
        })
    }

    /// Ensure one canonical built-in exists for a tenant.
    ///
    /// This is the only path that creates a built-in, and tenant provisioning
    /// and server boot are its only production callers: the server ensures the whole
    /// canonical inventory when it provisions a tenant and again for every
    /// active tenant at startup. Ingest and describe never create one and
    /// treat a missing built-in as an unregistered table.
    /// Repeating it for an existing built-in returns the same [`TableUid`].
    /// `attribution` is the verified creator pair: the platform operator that
    /// provisions the tenant, or [`Attribution::unresolved`] when no caller
    /// exists, as at startup reconciliation. Only the first registration
    /// records it; a repeat keeps the stored pair.
    ///
    /// A built-in this node has already resolved answers from the immutable
    /// registration cache; only the first ensure per tenant and table takes the
    /// advisory lock and loads Iceberg metadata, then reads the control row
    /// once into the cache.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::MetadataMismatch`] when the definition
    /// names an unknown namespace, and otherwise the registration, fingerprint,
    /// metadata, Iceberg, or SQL error from the locked create path.
    ///
    /// # Panics
    /// Panics only if a thread panicked while holding the cache lock.
    pub async fn ensure_builtin(
        &self,
        tenant: DataTenantId,
        definition: &'static BuiltinTableDefinition,
        attribution: Attribution,
    ) -> Result<TableUid, BifrostCatalogError> {
        let namespace =
            BifrostNamespace::from_domain_namespace(definition.namespace).ok_or_else(|| {
                BifrostCatalogError::MetadataMismatch(format!(
                    "unknown built-in namespace: {}",
                    definition.namespace
                ))
            })?;
        let table = TableRef::new(namespace, definition.name);
        if let Some(cached) = self.cached_registration(tenant, &table.fqn()) {
            return Ok(cached.table_uid);
        }
        self.create_table_locked(
            CreateTableRequest {
                table: table.clone(),
                user_fields: (definition.arrow_fields)(),
                tenant,
                attribution,
                physical_layout: Some((definition.physical_layout)()),
            },
            Some((definition.schema)()),
            CompactionRegistration::default(),
        )
        .await?;
        Ok(self.registration(&table, tenant).await?.table_uid)
    }

    /// Resolve one canonical layout and register the tenant-qualified table.
    ///
    /// The order is fixed and fail-closed: caller fields that name a managed
    /// column are rejected (a built-in's server-owned definition may declare
    /// one as content, as the code-axis `run_id` does), the
    /// binding and physical schema are resolved, the layout is canonicalized —
    /// all before a transaction opens. Inside the tenant transaction the
    /// advisory lock serializes concurrent registrations of the same FQN; an
    /// existing row is compared on schema fingerprint first, then on layout, so
    /// a schema conflict never masquerades as a layout conflict. An exact
    /// repeat of a prior registration is a no-op returning the same
    /// [`TableUid`] with [`RegisterOutcome::AlreadyExists`].
    ///
    /// A caller registration's fields are first replaced by their
    /// [`crate::tables::iceberg_form`], so its fingerprint, physical table, and
    /// every later description are the one Iceberg form; a spelling of the same
    /// Iceberg type therefore repeats rather than conflicts. A built-in is
    /// declared in that form already.
    ///
    /// `canonical_schema` is the built-in's own complete physical schema when
    /// this registration comes from [`Self::ensure_builtin`], and `None` for a
    /// caller registration, whose physical schema is derived from its user
    /// fields. It selects no resolver: every declaration, built-in or caller,
    /// resolves through the one [`PhysicalLayout::resolve`] entry point.
    ///
    /// `compaction` is checked for intrinsic shape before the transaction,
    /// compared under the advisory lock against an existing physical table's
    /// explicit target and type, and otherwise written as the new table's
    /// properties in its create transaction.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::ReservedColumn`] when a declaration
    /// names a server-owned column,
    /// [`BifrostCatalogError::Registration`] for an invalid or
    /// conflicting layout, compaction target, or compaction type,
    /// [`BifrostCatalogError::FingerprintMismatch`] for a schema conflict,
    /// [`BifrostCatalogError::Iceberg`] for a caller column with no Iceberg
    /// form, and metadata, SQL, or Iceberg errors otherwise.
    async fn create_table_locked(
        &self,
        request: CreateTableRequest,
        canonical_schema: Option<SchemaRef>,
        compaction: CompactionRegistration,
    ) -> Result<Registration, BifrostCatalogError> {
        let mut request = request;
        if canonical_schema.is_none() {
            reject_reserved_field_names(&request.user_fields)?;
            request.user_fields = request
                .user_fields
                .iter()
                .map(crate::tables::iceberg_form)
                .collect::<Result<_, _>>()?;
        }
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
        compaction.validate(&fqn)?;
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
                return Err(BifrostCatalogError::Registration(
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
            compaction.assert_matches(physical.metadata(), &fqn)?;
            let table_version = crate::catalog::layout::table_version(&fqn, physical.metadata())
                .map_err(BifrostCatalogError::Registration)?;
            conn.commit().await?;
            return Ok(Registration {
                table_uid: TableUid::from_row(&row.table_uid, &row.fqn)?,
                outcome: RegisterOutcome::AlreadyExists,
                fingerprint,
                table_version,
            });
        }

        self.ensure_namespace(binding.physical_namespace()).await?;
        let table_version = if physical_exists {
            let physical = self.catalog.load_table(&table_ident).await?;
            self.validate_physical_table(&physical, &binding, &arrow_schema, &layout)?;
            compaction.assert_matches(physical.metadata(), &fqn)?;
            crate::catalog::layout::table_version(&fqn, physical.metadata())
                .map_err(BifrostCatalogError::Registration)?
        } else {
            self.create_physical_table(&binding, &arrow_schema, &layout, compaction)
                .await?;
            TableVersion::INITIAL
        };

        let table_uid = TableUid::new_v7();
        vala_sql::queries::olap_catalog::upsert_table(
            &mut conn,
            table_uid.as_bytes(),
            &fqn,
            &fingerprint.0,
            &layout_json,
            request.attribution,
        )
        .await?;
        conn.commit().await?;
        Ok(Registration {
            table_uid,
            outcome: RegisterOutcome::Created,
            fingerprint,
            table_version,
        })
    }

    /// Create the physical Iceberg table for one canonical layout.
    ///
    /// Called only when registration has established that no physical table
    /// exists yet. The table is always Iceberg format v3, so appends assign
    /// row lineage through the first-row-id mechanism and Forge rewrites carry
    /// it forward. Every physical decision — schema ids, partition spec, sort
    /// order, location, and the Bloom and Forge data-path properties — is
    /// derived from `layout` and `binding`, so the canonical layout stays the
    /// single authority for the table's shape. The Forge data path is written
    /// as `write.data.path` so the managed rewrite core roots its outputs under
    /// the recipe segment instead of the default data root. Each option
    /// `compaction` declares is written as the table's explicit property;
    /// an omitted option writes nothing, so Forge resolves its default at
    /// planning time.
    ///
    /// # Errors
    ///
    /// Returns a metadata mismatch when the layout cannot produce a partition
    /// spec or sort order, and an Iceberg error when schema conversion or the
    /// catalog create fails.
    async fn create_physical_table(
        &self,
        binding: &TenantTableBinding,
        arrow_schema: &Schema,
        layout: &PhysicalLayout,
        compaction: CompactionRegistration,
    ) -> Result<(), BifrostCatalogError> {
        let iceberg_schema = crate::tables::iceberg_schema_for(arrow_schema)?;
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
        let forge_data_location = crate::catalog::layout::forge_data_location(&location);
        let mut properties = std::collections::HashMap::from([
            (
                crate::catalog::layout::BLOOM_COLUMNS_PROPERTY.to_owned(),
                layout.bloom_columns_property(),
            ),
            (
                crate::catalog::layout::WRITE_DATA_PATH_PROPERTY.to_owned(),
                forge_data_location,
            ),
        ]);
        compaction.write_properties(&mut properties);
        let creation = TableCreation::builder()
            .name(binding.table_name.clone())
            .location(location)
            .schema(iceberg_schema)
            .format_version(FormatVersion::V3)
            .partition_spec(partition_spec)
            .sort_order(sort_order)
            .properties(properties)
            .build();
        self.catalog
            .create_table(binding.physical_namespace(), creation)
            .await?;
        Ok(())
    }

    /// Verify that a registered table still carries Bifrost's complete physical recipe.
    ///
    /// Registration and reconciliation call this before accepting existing
    /// physical state, preventing a control-plane row from silently pointing at
    /// a table with a different location, schema, time partition, or Forge sort
    /// recipe. Every physical assertion is derived from `layout`, so the
    /// canonical layout is the single authority for what "correct" means.
    /// Bifrost tables exist only as Iceberg format v3, so any other format
    /// version is refused rather than migrated.
    ///
    /// # Errors
    ///
    /// Returns a metadata mismatch when the location, format version, schema,
    /// partition, or sort recipe diverges, or an Iceberg error when its schema
    /// has no Arrow form. The schema must equal the expected one exactly, by
    /// name, order, nullability, and type at every depth, because registration
    /// only ever declares Iceberg's own Arrow form.
    fn validate_physical_table(
        &self,
        table: &Table,
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
        if table.metadata().format_version() != FormatVersion::V3 {
            return Err(BifrostCatalogError::MetadataMismatch(format!(
                "physical table format version mismatch: expected v3, found {}",
                table.metadata().format_version()
            )));
        }
        let actual_schema =
            iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema())?;
        if !physical_schema_matches(expected_schema, &actual_schema) {
            return Err(BifrostCatalogError::MetadataMismatch(format!(
                "physical table schema mismatch: expected {expected_schema:?}, actual {actual_schema:?}"
            )));
        }
        let schema = table.metadata().current_schema();
        let expected_partition_fields = layout
            .partition_by()
            .iter()
            .map(|field| {
                schema.field_by_name(field.column()).map(|source| {
                    (
                        source.id,
                        field.transform().iceberg_field_name(field.column()),
                        field.transform().to_iceberg(),
                    )
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                BifrostCatalogError::MetadataMismatch(
                    "physical schema lacks a partition source column".to_owned(),
                )
            })?;
        let fields = table.metadata().default_partition_spec().fields();
        if fields.len() != expected_partition_fields.len()
            || fields.iter().zip(&expected_partition_fields).any(
                |(field, (source_id, name, transform))| {
                    field.source_id != *source_id
                        || &field.name != name
                        || field.transform != *transform
                },
            )
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "physical table partition spec mismatch".to_owned(),
            ));
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
    /// path invents or defaults a layout of its own. It answers from the
    /// node's registration cache after the first read.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the tenant does not
    /// own the registration, or a metadata error for a malformed fingerprint or
    /// an undecodable stored layout.
    ///
    /// # Panics
    /// Panics only if a thread panicked while holding the cache lock.
    pub async fn table_registration(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<(SchemaFingerprint, PhysicalLayoutWire), BifrostCatalogError> {
        let registration = self.registration(table, tenant).await?;
        Ok((registration.fingerprint, registration.layout))
    }

    /// Return the registered UID of one tenant/logical table.
    ///
    /// Answers from the node's registration cache when this table was found
    /// before, and otherwise from one control-row lookup whose result it then
    /// caches. Nothing is provisioned and no Iceberg metadata is loaded, so
    /// Gate can name a write destination's object scope on every frame and
    /// Oracle can resolve every query's tables without a Postgres round trip.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the tenant does not
    /// own the registration, or a metadata or SQL error otherwise.
    ///
    /// # Panics
    /// Panics only if a thread panicked while holding the cache lock.
    pub async fn table_uid(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<TableUid, BifrostCatalogError> {
        Ok(self.registration(table, tenant).await?.table_uid)
    }

    /// Return the current definition new admission binds for one table.
    ///
    /// The registered table, not any declaration, assigns every physical field
    /// id, nested and managed fields included; Scribe stamps exactly the
    /// definition's ids into the objects it writes so Forge and Oracle resolve
    /// its columns by the table's own numbering. Answers from the node's cache
    /// when this table was loaded before, and otherwise loads the tenant-bound
    /// Iceberg table once and caches its current definition.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::InvalidBinding`] when the tenant-qualified
    /// binding cannot be resolved, the Iceberg error when the physical table
    /// cannot be loaded, and [`BifrostCatalogError::MetadataMismatch`] when
    /// the current metadata does not rebuild a valid definition.
    ///
    /// # Panics
    /// Panics only if a thread panicked while holding the cache lock.
    pub async fn current_definition(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<Arc<TableDefinition>, BifrostCatalogError> {
        let key = (tenant, table.fqn());
        let cached = self
            .current_definitions
            .read()
            .expect("the current definition cache lock is never poisoned")
            .get(&key)
            .map(Arc::clone);
        if let Some(definition) = cached {
            return Ok(definition);
        }
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let loaded = self.catalog.load_table(&binding.table_ident()).await?;
        let definition = Arc::new(TableDefinition::of_metadata(&key.1, loaded.metadata())?);
        self.current_definitions
            .write()
            .expect("the current definition cache lock is never poisoned")
            .insert(key, Arc::clone(&definition));
        Ok(definition)
    }

    /// Resolves the definition one retained version of a table declares.
    ///
    /// Seal, staged recovery, and publication call this with the version a
    /// batch was accepted under, so the batch is written exactly as accepted
    /// whatever the table's current version is. The current version answers
    /// from the cache; an older one reads its retained metadata reference and
    /// that immutable document. ponytail: an older version re-reads its
    /// reference per call; cache it if seals behind an edit become frequent.
    ///
    /// # Errors
    /// Returns the Iceberg error when the table or reference cannot be read and
    /// [`BifrostCatalogError::MetadataMismatch`] when the version is not
    /// retained for this table or its document does not declare it.
    pub async fn retained_definition(
        &self,
        binding: &TenantTableBinding,
        table_version: TableVersion,
    ) -> Result<Arc<TableDefinition>, BifrostCatalogError> {
        let fqn = binding.table_ref.fqn();
        let current = self
            .current_definition(&binding.table_ref, binding.tenant)
            .await?;
        if current.table_version == table_version {
            return Ok(current);
        }
        let identifier = binding.table_ident();
        let loaded = self.catalog.load_table(&identifier).await?;
        let location = self
            .catalog
            .load_table_version_location(
                &identifier,
                &loaded.metadata().uuid().to_string(),
                table_version.get(),
            )
            .await?
            .ok_or_else(|| {
                BifrostCatalogError::MetadataMismatch(format!(
                    "{fqn} retains no table version {table_version}"
                ))
            })?;
        let metadata = match self.metadata_cache.get(&location) {
            Some(metadata) => metadata,
            None => {
                let metadata = Arc::new(TableMetadata::read_from(&self.file_io, &location).await?);
                self.metadata_cache
                    .insert(location.clone(), Arc::clone(&metadata));
                metadata
            }
        };
        let definition = TableDefinition::of_metadata(&fqn, &metadata)?;
        if definition.table_version != table_version {
            return Err(BifrostCatalogError::MetadataMismatch(format!(
                "{fqn} version {table_version} references a document at version {}",
                definition.table_version
            )));
        }
        Ok(Arc::new(definition))
    }

    /// Drops this node's cached registration and current definition of one
    /// table, so the next admission reads the version committed since.
    ///
    /// # Panics
    /// Panics only if a thread panicked while holding a cache lock.
    pub fn evict_current_definition(&self, table: &TableRef, tenant: DataTenantId) {
        let key = (tenant, table.fqn());
        self.current_definitions
            .write()
            .expect("the current definition cache lock is never poisoned")
            .remove(&key);
        self.registrations
            .write()
            .expect("the registration cache lock is never poisoned")
            .remove(&key);
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
    ) -> Result<SchemaRef, BifrostCatalogError> {
        // Schema only: the provider is built from the loaded metadata document
        // and dropped here, so this path opens no snapshot object.
        let fqn = table.fqn();
        let Some(_row) = self.lookup_table_row(&fqn, tenant).await? else {
            return Err(BifrostCatalogError::TableNotFound(fqn));
        };
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let iceberg_table = self.catalog.load_table(&binding.table_ident()).await?;
        let provider = IcebergStaticTableProvider::try_new_from_table(iceberg_table)
            .await
            .map_err(provider_error)?;
        Ok(datafusion::datasource::TableProvider::schema(&provider))
    }

    /// List registrations visible under the exact tenant RLS bind.
    ///
    /// A registration whose namespace is no longer a [`BifrostNamespace`] —
    /// such as a stale development row for a retired table — names nothing
    /// Bifrost can serve, so it is left out like any unknown table.
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
        rows.iter()
            .filter(|row| BifrostNamespace::split_fqn(&row.fqn).is_some())
            .map(entry_from_row)
            .collect()
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
        let builtin = builtin_table(
            table
                .namespace
                .as_str()
                .strip_prefix("vala.")
                .unwrap_or_default(),
            &table.name,
        );
        let row = self
            .lookup_table_row(&fqn, tenant)
            .await?
            .ok_or_else(|| BifrostCatalogError::TableNotFound(fqn.clone()))?;
        // A canonical built-in is described from its own declaration, not from
        // the Iceberg round trip: the stored schema drops the ledger's
        // sensitivity markers and renames nested list elements, and it is the
        // ledger's shape that Scribe enforces on every canonical batch. The
        // registered field ids are the table's and Scribe stamps them itself,
        // so the description carries none.
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let iceberg_table = self.catalog.load_table(&binding.table_ident()).await?;
        let arrow_schema = if let Some(definition) = builtin {
            (definition.schema)()
        } else {
            Arc::new(iceberg::arrow::schema_to_arrow_schema(
                iceberg_table.metadata().current_schema(),
            )?)
        };
        let described = described_fields_from_stored_schema(&arrow_schema)?;
        Ok(BifrostTableDescription {
            entry: entry_from_row(&row)?,
            table_version: crate::catalog::layout::table_version(&fqn, iceberg_table.metadata())
                .map_err(BifrostCatalogError::Registration)?,
            user_fields: described.user_fields,
            correlation_fields: described.correlation_fields,
            canonical_physical_fingerprint: builtin
                .and_then(|definition| (definition.canonical_physical_fingerprint)())
                .map(crate::tables::CanonicalPhysicalFingerprint::to_hex),
            physical_layout: layout_wire_from_row(&row)?,
            compaction_target_file_size_bytes: explicit_compaction_target(
                iceberg_table.metadata(),
            )?,
            compaction_type: explicit_compaction_type(iceberg_table.metadata())?
                .map(CompactionTypeWire::from),
        })
    }

    /// Build an execution provider for one authenticated tenant's table.
    ///
    /// The control-plane lookup and physical Iceberg binding both use the
    /// authenticated tenant. Its data-file footers are tenant-proved by the
    /// Oracle scan that reads them, never by a row predicate.
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the control-plane row
    /// is absent for this tenant, [`BifrostCatalogError::InvalidBinding`] when
    /// the tenant-qualified physical binding cannot be resolved, a catalog error
    /// when the Iceberg table cannot be loaded, and
    /// [`BifrostCatalogError::DataFusion`] when the scan provider cannot be
    /// constructed.
    pub async fn provider(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<IcebergStaticTableProvider, BifrostCatalogError> {
        let fqn = table.fqn();
        let Some(_row) = self.lookup_table_row(&fqn, tenant).await? else {
            return Err(BifrostCatalogError::TableNotFound(fqn));
        };
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let identifier = binding.table_ident();
        let loaded = self.catalog.load_table(&identifier).await?;
        let iceberg_table = self.cached_table(
            &identifier,
            loaded.metadata_ref(),
            loaded.metadata_location().map(ToOwned::to_owned),
        )?;
        IcebergStaticTableProvider::try_new_from_table(iceberg_table)
            .await
            .map_err(provider_error)
    }

    /// Resolves one registered tenant table pinned to an exact published
    /// snapshot.
    ///
    /// A follower must read the snapshot the leader planned and signed, not
    /// whichever one is current when the follower happens to resolve. Loading
    /// the table and then pinning is what turns a replaced snapshot into a
    /// resolution failure rather than a quiet read of newer files.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the tenant has no
    /// such registered table, an invalid-binding error when the tenant-qualified
    /// identity cannot be derived, a catalog error when the table cannot be
    /// loaded, and [`BifrostCatalogError::DataFusion`] when the named snapshot
    /// is absent or the provider cannot be constructed over it.
    pub async fn pinned_provider(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
        snapshot_id: i64,
    ) -> Result<IcebergStaticTableProvider, BifrostCatalogError> {
        let fqn = table.fqn();
        let Some(_row) = self.lookup_table_row(&fqn, tenant).await? else {
            return Err(BifrostCatalogError::TableNotFound(fqn));
        };
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let identifier = binding.table_ident();
        let loaded = self.catalog.load_table(&identifier).await?;
        let iceberg_table = self.cached_table(
            &identifier,
            loaded.metadata_ref(),
            loaded.metadata_location().map(ToOwned::to_owned),
        )?;
        IcebergStaticTableProvider::try_new_from_table_snapshot(iceberg_table, snapshot_id)
            .await
            .map_err(provider_error)
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

    /// Returns one table's cached registration, reading and caching its
    /// control row on this node's first lookup.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the tenant does not
    /// own the registration, a metadata error for a malformed UID,
    /// fingerprint, or stored layout, or the SQL error of the read.
    ///
    /// # Panics
    /// Panics only if a thread panicked while holding the cache lock.
    async fn registration(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<CachedRegistration, BifrostCatalogError> {
        let fqn = table.fqn();
        if let Some(cached) = self.cached_registration(tenant, &fqn) {
            return Ok(cached);
        }
        let row = self
            .lookup_table_row(&fqn, tenant)
            .await?
            .ok_or_else(|| BifrostCatalogError::TableNotFound(fqn.clone()))?;
        let fingerprint: [u8; 32] = row.fingerprint.as_slice().try_into().map_err(|_| {
            BifrostCatalogError::MetadataMismatch(format!(
                "schema fingerprint length mismatch for {fqn}"
            ))
        })?;
        let registration = CachedRegistration {
            table_uid: TableUid::from_row(&row.table_uid, &fqn)?,
            fingerprint: SchemaFingerprint(fingerprint),
            layout: layout_wire_from_row(&row)?,
        };
        self.registrations
            .write()
            .expect("the registration cache lock is never poisoned")
            .insert((tenant, fqn), registration.clone());
        Ok(registration)
    }

    /// Returns one table's registration when this node has already read it.
    ///
    /// # Panics
    /// Panics only if a thread panicked while holding the cache lock.
    fn cached_registration(&self, tenant: DataTenantId, fqn: &str) -> Option<CachedRegistration> {
        self.registrations
            .read()
            .expect("the registration cache lock is never poisoned")
            .get(&(tenant, fqn.to_owned()))
            .cloned()
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
/// The resolved schema must map to at most [`MAX_PHYSICAL_LEAF_COLUMNS`]
/// Parquet leaf columns, managed columns included. Checking here, before any
/// table exists, is what lets every accepted write stage without a later
/// shape refusal.
///
/// # Errors
///
/// Returns [`BifrostCatalogError::Registration`] carrying `SchemaParse` when
/// the physical schema cannot be mapped to Parquet or exceeds the leaf limit,
/// and carrying a layout error when the declaration carries more than
/// [`MAX_SORT_KEYS`](crate::catalog::layout::MAX_SORT_KEYS) sort keys, or
/// names a column absent from the resolved schema or repeated within one list.
fn resolve_registration_layout(
    fqn: &str,
    user_fields: &[Field],
    declared: Option<&PhysicalLayoutWire>,
    canonical_schema: Option<&Schema>,
) -> Result<(Schema, PhysicalLayout), BifrostCatalogError> {
    let arrow_schema = canonical_schema.map_or_else(
        || {
            Schema::new(crate::tables::managed_columns::ensure_managed_columns(
                user_fields.to_vec(),
                crate::tables::CorrelationPolicy::Observation,
            ))
        },
        Clone::clone,
    );
    let schema_refusal = |detail: String| {
        BifrostCatalogError::Registration(wyrd_spec::vala::BifrostError::SchemaParse { detail })
    };
    let leaves = parquet::arrow::ArrowSchemaConverter::new()
        .convert(&arrow_schema)
        .map_err(|error| schema_refusal(format!("{fqn} has no Parquet mapping: {error}")))?
        .num_columns();
    if leaves > MAX_PHYSICAL_LEAF_COLUMNS {
        return Err(schema_refusal(format!(
            "{fqn} maps to {leaves} Parquet leaf columns including managed columns; \
             at most {MAX_PHYSICAL_LEAF_COLUMNS} are allowed"
        )));
    }
    let layout = PhysicalLayout::resolve(fqn, &arrow_schema, declared)
        .map_err(BifrostCatalogError::Registration)?;
    Ok((arrow_schema, layout))
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

/// Most Parquet leaf columns one registered table may map to, managed columns
/// included.
pub const MAX_PHYSICAL_LEAF_COLUMNS: usize = 256;

/// Iceberg property naming a table's explicit Forge compaction file target.
const TARGET_FILE_SIZE_PROPERTY: &str = TableProperties::PROPERTY_WRITE_TARGET_FILE_SIZE_BYTES;

/// Reads the explicit compaction file target a physical table stores.
///
/// `None` means the table declares none and follows Forge's deployment
/// default.
///
/// # Errors
/// Returns [`BifrostCatalogError::MetadataMismatch`] when the stored property
/// is not a base-ten byte count.
fn explicit_compaction_target(
    metadata: &TableMetadata,
) -> Result<Option<u64>, BifrostCatalogError> {
    metadata
        .properties()
        .get(TARGET_FILE_SIZE_PROPERTY)
        .map(|raw| {
            raw.parse::<u64>().map_err(|error| {
                BifrostCatalogError::MetadataMismatch(format!(
                    "stored {TARGET_FILE_SIZE_PROPERTY}={raw:?} is not a byte count: {error}"
                ))
            })
        })
        .transpose()
}

/// Reads the explicit compaction type a physical table stores.
///
/// `None` means the table declares none and Forge plans its `small-files` default.
///
/// # Errors
/// Returns [`BifrostCatalogError::MetadataMismatch`] when the stored property
/// is not a known compaction type spelling.
fn explicit_compaction_type(
    metadata: &TableMetadata,
) -> Result<Option<ForgeCompactionType>, BifrostCatalogError> {
    metadata
        .properties()
        .get(COMPACTION_TYPE_PROPERTY)
        .map(|raw| {
            ForgeCompactionType::parse(raw).map_err(|error| {
                BifrostCatalogError::MetadataMismatch(format!(
                    "stored {COMPACTION_TYPE_PROPERTY}={raw:?} is invalid: {error}"
                ))
            })
        })
        .transpose()
}

/// The Forge compaction options one dataset registration declares.
///
/// Every option is optional and independent: a declared option becomes the
/// new table's explicit Iceberg property in its create transaction, and on a
/// re-registration must equal the stored explicit value. An omitted option
/// stores nothing and never conflicts, so the table follows Forge's default
/// (the deployment file target, the `small-files` type). [`Default`] declares
/// nothing, which is how built-ins register.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactionRegistration {
    /// Soft file target Forge compacts toward, stored as
    /// `write.target-file-size-bytes`.
    pub target_file_size_bytes: Option<u64>,
    /// Physical compaction type a worker plans with, stored as
    /// `wyrd.forge.compaction.type`.
    pub compaction_type: Option<ForgeCompactionType>,
}

impl CompactionRegistration {
    /// Rejects a declaration Forge could never honor, before any durable write.
    ///
    /// Only the file target has intrinsic shape: it must be registrable under
    /// Forge's file-size policy. Every compaction type is valid on its own.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::Registration`] carrying
    /// `InvalidCompactionTarget` when the declared target is not registrable.
    fn validate(&self, fqn: &str) -> Result<(), BifrostCatalogError> {
        match self.target_file_size_bytes {
            Some(bytes)
                if !crate::forge::managed::policy::registrable_target_file_size_bytes(bytes) =>
            {
                Err(BifrostCatalogError::Registration(
                    wyrd_spec::vala::BifrostError::InvalidCompactionTarget {
                        table: fqn.to_owned(),
                        bytes,
                    },
                ))
            }
            _ => Ok(()),
        }
    }

    /// Checks a re-registration's declared options against the stored table.
    ///
    /// Omission always matches and leaves the stored property untouched; a
    /// declared option must equal the table's explicit value, so declaring
    /// one for a table that stores none is a conflict. The target is checked
    /// before the type.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::Registration`] carrying
    /// `CompactionTargetMismatch` or `CompactionTypeMismatch` for a differing
    /// declaration, and a metadata mismatch when a stored property is
    /// malformed.
    fn assert_matches(
        &self,
        metadata: &TableMetadata,
        fqn: &str,
    ) -> Result<(), BifrostCatalogError> {
        if let Some(bytes) = self.target_file_size_bytes
            && explicit_compaction_target(metadata)? != Some(bytes)
        {
            return Err(BifrostCatalogError::Registration(
                wyrd_spec::vala::BifrostError::CompactionTargetMismatch {
                    table: fqn.to_owned(),
                },
            ));
        }
        if let Some(kind) = self.compaction_type
            && explicit_compaction_type(metadata)? != Some(kind)
        {
            return Err(BifrostCatalogError::Registration(
                wyrd_spec::vala::BifrostError::CompactionTypeMismatch {
                    table: fqn.to_owned(),
                },
            ));
        }
        Ok(())
    }

    /// Adds each declared option to a new table's creation properties.
    ///
    /// Writes nothing for an omitted option.
    fn write_properties(&self, properties: &mut HashMap<String, String>) {
        if let Some(bytes) = self.target_file_size_bytes {
            properties.insert(TARGET_FILE_SIZE_PROPERTY.to_owned(), bytes.to_string());
        }
        if let Some(kind) = self.compaction_type {
            properties.insert(
                COMPACTION_TYPE_PROPERTY.to_owned(),
                kind.as_str().to_owned(),
            );
        }
    }
}

/// Maps an Iceberg scan-provider construction failure into the catalog error
/// its callers document.
fn provider_error(error: IcebergError) -> BifrostCatalogError {
    BifrostCatalogError::DataFusion(datafusion::error::DataFusionError::External(Box::new(
        error,
    )))
}

/// Reports whether a physical Iceberg table's schema is the expected one.
///
/// The exact layout fingerprint — names, order, nullability, and Arrow types
/// at every depth — must match, and each column must be the same Iceberg type
/// through [`crate::tables::same_iceberg_type`], which tells a Variant from
/// its storage Struct at any depth where the metadata-free fingerprint
/// cannot. The fingerprint itself stays unchanged because sealed footers
/// persist it.
pub(crate) fn physical_schema_matches(expected: &Schema, actual: &Schema) -> bool {
    SchemaFingerprint::from_arrow_schema_exact(expected)
        == SchemaFingerprint::from_arrow_schema_exact(actual)
        && expected
            .fields()
            .iter()
            .zip(actual.fields())
            .all(|(expected, actual)| crate::tables::same_iceberg_type(expected, actual))
}

#[cfg(test)]
mod physical_schema_tests {
    //! The physical-table schema comparison registration reconciles against.

    use std::sync::Arc;

    use arrow::datatypes::{DataType, Field, Fields, Schema};
    use wyrd_types::variant::{variant_field, variant_storage_type};

    use super::physical_schema_matches;

    /// A Variant and an ordinary Struct with its storage children are
    /// different physical schemas in both directions, at the top level and
    /// nested under a Struct or a List.
    ///
    /// # Panics
    ///
    /// Panics when any placement compares a Variant equal to its storage
    /// Struct, or a Variant stops matching itself.
    #[test]
    fn physical_schema_keeps_variant_identity_at_any_depth() {
        let variant = variant_field("payload", true);
        let storage = Field::new("payload", variant_storage_type(), true);
        let struct_child = |field: &Field| {
            Field::new(
                "outer",
                DataType::Struct(Fields::from(vec![field.clone()])),
                true,
            )
        };
        let list_element =
            |field: &Field| Field::new("outer", DataType::List(Arc::new(field.clone())), true);
        let placements = [
            ("top level", variant.clone(), storage.clone()),
            (
                "struct child",
                struct_child(&variant),
                struct_child(&storage),
            ),
            (
                "list element",
                list_element(&variant),
                list_element(&storage),
            ),
        ];
        for (placement, variant, storage) in placements {
            let variant = Schema::new(vec![variant]);
            let storage = Schema::new(vec![storage]);
            assert!(physical_schema_matches(&variant, &variant), "{placement}");
            assert!(!physical_schema_matches(&variant, &storage), "{placement}");
            assert!(!physical_schema_matches(&storage, &variant), "{placement}");
        }
    }

    /// The same columns in a different order are a different physical schema.
    ///
    /// Position is the whole comparison: nothing consults a declared Iceberg
    /// field id to call two orderings the same table.
    ///
    /// # Panics
    ///
    /// Panics when a reordered schema compares equal in either direction.
    #[test]
    fn physical_schema_rejects_reordered_columns() {
        let declared = Schema::new(vec![
            Field::new("attempt", DataType::Int64, false),
            Field::new("operation", DataType::Utf8, false),
        ]);
        let reordered = Schema::new(vec![
            Field::new("operation", DataType::Utf8, false),
            Field::new("attempt", DataType::Int64, false),
        ]);

        assert!(!physical_schema_matches(&declared, &reordered));
        assert!(!physical_schema_matches(&reordered, &declared));
    }
}

/// Validates every reconciled hot row against its binding and adds its bytes.
///
/// A hot row is only readable when it names this tenant and table, carries a
/// coherent size, row count, epoch, and WAL range, and resolves inside the
/// binding's object prefix. The returned estimate is `base` plus every row's
/// file size.
///
/// # Errors
/// Returns [`BifrostCatalogError::MetadataMismatch`] when a row violates its
/// binding or the byte estimate cannot be represented.
fn hot_cut_bytes(
    binding: &TenantTableBinding,
    hot_files: &[HotFileRow],
    base: u64,
) -> Result<u64, BifrostCatalogError> {
    hot_files.iter().try_fold(base, |total, row| {
        let valid_identity = row.data_tenant_id == binding.tenant.as_uuid()
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
        total
            .checked_add(u64::try_from(row.file_size).map_err(|_| {
                BifrostCatalogError::MetadataMismatch(
                    "hot manifest file size is invalid".to_owned(),
                )
            })?)
            .ok_or_else(|| {
                BifrostCatalogError::MetadataMismatch("sealed byte estimate overflow".to_owned())
            })
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use arrow::datatypes::{DataType, Field};

    use super::{
        CompactionRegistration, MAX_PHYSICAL_LEAF_COLUMNS, PinnedIcebergFile,
        resolve_registration_layout,
    };
    use crate::catalog::event_time::{EventTimeBoundsDefect, EventTimeStatistics};
    use crate::forge::{COMPACTION_TYPE_PROPERTY, ForgeCompactionType, ForgeTableSettings};
    use wyrd_spec::vala::api::CompactionTypeWire;

    /// A declared compaction type is written under Forge's own property in
    /// the hyphenated spelling Forge parses back to the same type, every wire
    /// value round-trips through the Forge type, and an empty declaration
    /// writes nothing so Forge keeps its `small-files` default.
    ///
    /// # Panics
    /// Panics when a written property is missing, misspelled, or does not
    /// parse back into the declared type.
    #[test]
    fn compaction_registration_writes_the_forge_type_property() {
        for wire in [
            CompactionTypeWire::Auto,
            CompactionTypeWire::Full,
            CompactionTypeWire::SmallFiles,
            CompactionTypeWire::FilesWithDelete,
        ] {
            let kind = ForgeCompactionType::from(wire);
            assert_eq!(CompactionTypeWire::from(kind), wire);
            let mut properties = HashMap::new();
            CompactionRegistration {
                target_file_size_bytes: None,
                compaction_type: Some(kind),
            }
            .write_properties(&mut properties);
            assert_eq!(properties.len(), 1);
            let settings =
                ForgeTableSettings::from_properties(&properties).expect("settings parse");
            assert_eq!(settings.compaction_type, kind);
        }
        let mut properties = HashMap::new();
        CompactionRegistration {
            target_file_size_bytes: None,
            compaction_type: Some(ForgeCompactionType::SmallFiles),
        }
        .write_properties(&mut properties);
        assert_eq!(
            properties.get(COMPACTION_TYPE_PROPERTY).map(String::as_str),
            Some("small-files")
        );
        let mut empty = HashMap::new();
        CompactionRegistration::default().write_properties(&mut empty);
        assert!(empty.is_empty());
        assert_eq!(
            ForgeTableSettings::from_properties(&empty)
                .expect("defaults parse")
                .compaction_type,
            ForgeCompactionType::SmallFiles
        );
    }

    /// Builds one Forge-style rewrite output's manifest `DataFile` with typed
    /// time-column bounds under the given field id.
    ///
    /// Only the fields the pinned projection reads are populated; the rest keep
    /// their builder defaults so the fixture cannot accidentally supply
    /// evidence the projection is supposed to derive.
    fn rewrite_data_file(
        field_id: i32,
        lower: Option<iceberg::spec::Datum>,
        upper: Option<iceberg::spec::Datum>,
    ) -> iceberg::spec::DataFile {
        let mut lower_bounds = HashMap::new();
        let mut upper_bounds = HashMap::new();
        if let Some(lower) = lower {
            lower_bounds.insert(field_id, lower);
        }
        if let Some(upper) = upper {
            upper_bounds.insert(field_id, upper);
        }
        iceberg::spec::DataFileBuilder::default()
            .content(iceberg::spec::DataContentType::Data)
            .file_path("s3://warehouse/tenant/logs/records/rewrite-0.parquet".to_owned())
            .file_format(iceberg::spec::DataFileFormat::Parquet)
            .partition(iceberg::spec::Struct::empty())
            .record_count(4_096)
            .file_size_in_bytes(2_097_152)
            .lower_bounds(lower_bounds)
            .upper_bounds(upper_bounds)
            .partition_spec_id(0)
            .build()
            .expect("fixture rewrite data file builds")
    }

    /// A Forge rewrite output is discovered only through the committed Iceberg
    /// manifest: it is never inserted into `vala.file_list`, so its size, row
    /// count, and event-time interval must all come from the `DataFile` alone.
    ///
    /// The negative half is equally load-bearing: every defect class normalizes
    /// to retained-with-a-reason rather than to an exclusion or an error, so a
    /// rewrite output with unusable statistics is still scanned.
    #[test]
    fn forge_rewrite_manifest_bounds_survive_without_file_list_row() {
        let field_id = 42;
        let lower_micros = 1_787_493_600_000_000_i64;
        let upper_micros = 1_787_497_200_000_000_i64;
        let timestamptz = iceberg::spec::Datum::timestamptz_micros;

        let pinned = PinnedIcebergFile::from_manifest_entry(
            "tenant/logs/records/rewrite-0.parquet".to_owned(),
            &rewrite_data_file(
                field_id,
                Some(timestamptz(lower_micros)),
                Some(timestamptz(upper_micros)),
            ),
            Some(field_id),
        );
        assert_eq!(pinned.file_path, "tenant/logs/records/rewrite-0.parquet");
        assert_eq!(pinned.file_size, 2_097_152);
        assert_eq!(pinned.row_count, 4_096);
        assert_eq!(
            pinned.event_time,
            EventTimeStatistics::Bounded {
                min_micros: lower_micros,
                max_micros: upper_micros,
            }
        );

        // An absent upper bound is missing, not invalid: the writer recorded
        // nothing to decode.
        let half = PinnedIcebergFile::from_manifest_entry(
            "tenant/logs/records/rewrite-0.parquet".to_owned(),
            &rewrite_data_file(field_id, Some(timestamptz(lower_micros)), None),
            Some(field_id),
        );
        assert_eq!(
            half.event_time,
            EventTimeStatistics::Unusable(EventTimeBoundsDefect::Missing)
        );
        assert_eq!(half.row_count, 4_096);

        // A present bound of the wrong Iceberg type is invalid, and is still
        // retained rather than refused.
        let wrong_type = PinnedIcebergFile::from_manifest_entry(
            "tenant/logs/records/rewrite-0.parquet".to_owned(),
            &rewrite_data_file(
                field_id,
                Some(iceberg::spec::Datum::long(lower_micros)),
                Some(timestamptz(upper_micros)),
            ),
            Some(field_id),
        );
        assert_eq!(
            wrong_type.event_time,
            EventTimeStatistics::Unusable(EventTimeBoundsDefect::Invalid)
        );

        // A reversed interval is contradictory.
        let reversed = PinnedIcebergFile::from_manifest_entry(
            "tenant/logs/records/rewrite-0.parquet".to_owned(),
            &rewrite_data_file(
                field_id,
                Some(timestamptz(upper_micros)),
                Some(timestamptz(lower_micros)),
            ),
            Some(field_id),
        );
        assert_eq!(
            reversed.event_time,
            EventTimeStatistics::Unusable(EventTimeBoundsDefect::Contradictory)
        );

        // Bounds recorded under a different field id must not be read: a
        // manifest whose writer schema lacks the column fails open.
        let other_field = PinnedIcebergFile::from_manifest_entry(
            "tenant/logs/records/rewrite-0.parquet".to_owned(),
            &rewrite_data_file(
                field_id,
                Some(timestamptz(lower_micros)),
                Some(timestamptz(upper_micros)),
            ),
            Some(field_id + 1),
        );
        assert_eq!(
            other_field.event_time,
            EventTimeStatistics::Unusable(EventTimeBoundsDefect::Missing)
        );
    }

    /// Registration admits exactly 256 physical Parquet leaves, managed
    /// columns included, refuses the 257th with the typed schema refusal, and
    /// admits every built-in its eager creation resolves.
    ///
    /// # Panics
    /// Panics when the boundary schema is refused or the next one admitted.
    #[test]
    fn registration_admits_256_physical_leaves_including_managed_columns() {
        let managed = resolve_registration_layout("vala.datasets.probe", &[], None, None)
            .expect("an empty user schema resolves")
            .0;
        let managed_leaves = parquet::arrow::ArrowSchemaConverter::new()
            .convert(&managed)
            .expect("managed columns map to Parquet")
            .num_columns();
        let user = |count: usize| {
            (0..count)
                .map(|index| Field::new(format!("c{index}"), DataType::Int64, true))
                .collect::<Vec<_>>()
        };
        let at_limit = user(MAX_PHYSICAL_LEAF_COLUMNS - managed_leaves);
        resolve_registration_layout("vala.datasets.wide", &at_limit, None, None)
            .expect("256 physical leaves register");
        let over = user(MAX_PHYSICAL_LEAF_COLUMNS - managed_leaves + 1);
        let error = resolve_registration_layout("vala.datasets.wide", &over, None, None)
            .expect_err("257 physical leaves are refused");
        assert_eq!(
            error.into_public().code(),
            "WYRD_VALA_400_SCHEMA_PARSE",
            "the refusal is the typed schema error"
        );

        for definition in crate::tables::builtin_tables() {
            let fqn = format!("{}.{}", definition.namespace, definition.name);
            resolve_registration_layout(
                &fqn,
                &(definition.arrow_fields)(),
                Some(&(definition.physical_layout)()),
                Some(&(definition.schema)()),
            )
            .unwrap_or_else(|error| panic!("eagerly created built-in {fqn} registers: {error}"));
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
mod production_pin_tests {
    use std::collections::HashMap;
    use std::path::Path;

    use iceberg::transaction::{ApplyTransactionAction, Transaction};
    use secrecy::ExposeSecret as _;
    use wyrd_spec::DataTenantId;

    use crate::storage::BifrostStorage;

    use std::sync::Arc;

    use super::{BifrostCatalog, CompactionRegistration, PinnedSealedTable};
    use crate::catalog::TableRef;
    use crate::catalog::event_time::{EventTimeBoundsDefect, EventTimeStatistics};
    use crate::namespaces::BifrostNamespace;

    /// One committed data file's canonical name and the bounds it carries.
    struct ManifestCase {
        /// Object file name committed under the tenant table's location.
        name: &'static str,
        /// Lower bound the writer recorded, if any.
        lower: Option<iceberg::spec::Datum>,
        /// Upper bound the writer recorded, if any.
        upper: Option<iceberg::spec::Datum>,
        /// Statistics the pinned cut must derive from that manifest evidence.
        expected: EventTimeStatistics,
    }

    /// Builds a storage owner over one local warehouse root.
    ///
    /// The catalog runs its Iceberg I/O through the node's storage owner, so a
    /// catalog fixture composes the same owner production does.
    ///
    /// # Panics
    /// Panics when the fixture storage policy or local root is invalid.
    pub(super) fn local_storage_owner(root: &Path) -> Arc<BifrostStorage> {
        BifrostStorage::for_test(root, false)
    }

    /// Builds the manifest evidence table one committed cut must reproduce.
    ///
    /// Extracted from its test so the scenario reads as commit-then-assert:
    /// the cases are fixture data, not part of the behavior under test.
    fn manifest_cases(lower_micros: i64, upper_micros: i64) -> [ManifestCase; 4] {
        let timestamptz = iceberg::spec::Datum::timestamptz_micros;
        [
            ManifestCase {
                name: "valid.parquet",
                lower: Some(timestamptz(lower_micros)),
                upper: Some(timestamptz(upper_micros)),
                expected: EventTimeStatistics::Bounded {
                    min_micros: lower_micros,
                    max_micros: upper_micros,
                },
            },
            ManifestCase {
                name: "missing.parquet",
                lower: Some(timestamptz(lower_micros)),
                upper: None,
                expected: EventTimeStatistics::Unusable(EventTimeBoundsDefect::Missing),
            },
            ManifestCase {
                // A manifest bound is serialized as raw bytes and re-typed
                // from the writer schema on read, so a wrong-typed datum
                // cannot survive a real round trip; the projection test
                // above covers that class. What production *can* observe is
                // a well-typed value that names no representable instant.
                name: "malformed.parquet",
                lower: Some(timestamptz(i64::MAX)),
                upper: Some(timestamptz(upper_micros)),
                expected: EventTimeStatistics::Unusable(EventTimeBoundsDefect::Invalid),
            },
            ManifestCase {
                name: "contradictory.parquet",
                lower: Some(timestamptz(upper_micros)),
                upper: Some(timestamptz(lower_micros)),
                expected: EventTimeStatistics::Unusable(EventTimeBoundsDefect::Contradictory),
            },
        ]
    }

    /// Builds one committed data file per manifest case under the live table.
    ///
    /// Bounds are keyed by the physical writer field id rather than by name,
    /// which is exactly the binding the pinned cut must resolve on read.
    ///
    /// # Panics
    /// Panics when the fixture instant does not bucket or a data file does not
    /// build, both of which mean the fixture itself is wrong.
    fn manifest_data_files(
        physical: &iceberg::table::Table,
        field_id: i32,
        cases: &[ManifestCase],
        lower_micros: i64,
    ) -> Vec<iceberg::spec::DataFile> {
        let partition = crate::catalog::layout::TimeGranularity::Hour
            .bucket(
                chrono::DateTime::from_timestamp_micros(lower_micros)
                    .expect("fixture instant is representable"),
            )
            .expect("fixture instant buckets");
        let location = physical.metadata().location().to_owned();
        let data_files = cases.iter().map(|case| {
            let mut lower_bounds = HashMap::new();
            let mut upper_bounds = HashMap::new();
            if let Some(lower) = case.lower.clone() {
                lower_bounds.insert(field_id, lower);
            }
            if let Some(upper) = case.upper.clone() {
                upper_bounds.insert(field_id, upper);
            }
            iceberg::spec::DataFileBuilder::default()
                .content(iceberg::spec::DataContentType::Data)
                .file_path(format!("{location}/data/{}", case.name))
                .file_format(iceberg::spec::DataFileFormat::Parquet)
                .partition(iceberg::spec::Struct::from_iter([Some(
                    partition.iceberg_partition_literal(),
                )]))
                .record_count(4_096)
                .file_size_in_bytes(2_097_152)
                .lower_bounds(lower_bounds)
                .upper_bounds(upper_bounds)
                .partition_spec_id(physical.metadata().default_partition_spec_id())
                .sort_order_id(
                    i32::try_from(physical.metadata().default_sort_order_id())
                        .expect("fixture sort order id fits i32"),
                )
                .build()
                .expect("fixture data file builds")
        });
        data_files.collect()
    }

    /// The registration cache keeps found UIDs and never remembers a miss.
    ///
    /// A table looked up before it exists must still be found on the first
    /// lookup after it registers, and every later lookup must return the UID
    /// registration minted.
    ///
    /// # Panics
    /// Panics when the fixture or registration fails, when the unregistered
    /// lookup succeeds, or when a later lookup misses or returns another UID.
    #[test]
    fn registration_cache_keeps_found_uids_only() {
        wyrd_runtime::runtime().block_on(async {
            let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("postgres fixture starts");
            let warehouse = tempfile::tempdir().expect("warehouse directory");
            let catalog = BifrostCatalog::new(
                fixture.catalog_dsn().expose_secret(),
                local_storage_owner(warehouse.path()),
                fixture.vala_postgres().clone(),
            )
            .await
            .expect("redux catalog builds over the fixture");
            let tenant = fixture.data_tenant_id();
            let table = TableRef::new(BifrostNamespace::Datasets, "registered_later");
            assert!(matches!(
                catalog.table_uid(&table, tenant).await,
                Err(crate::catalog::BifrostCatalogError::TableNotFound(_))
            ));
            let registered = catalog
                .register_dataset(
                    tenant,
                    wyrd_spec::auth::Attribution::unresolved(),
                    table.clone(),
                    vec![arrow::datatypes::Field::new(
                        "value",
                        arrow::datatypes::DataType::Int64,
                        true,
                    )],
                    None,
                    CompactionRegistration::default(),
                )
                .await
                .expect("dataset registers");
            for _ in 0..2 {
                assert_eq!(
                    hex::encode(
                        catalog
                            .table_uid(&table, tenant)
                            .await
                            .expect("the registered table resolves")
                            .as_bytes()
                    ),
                    registered.table_uid
                );
            }
        });
    }

    /// A built-in this node has ensured is ensured again without Postgres.
    ///
    /// Only the first ensure per tenant and table may take the advisory lock
    /// and load Iceberg metadata. Closing the pool
    /// after the first ensure makes any later round trip fail.
    ///
    /// # Panics
    /// Panics when the fixture or the first ensure fails, or when the second
    /// ensure fails or returns another UID.
    #[test]
    fn ensured_builtin_answers_from_the_registration_cache() {
        wyrd_runtime::runtime().block_on(async {
            let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("postgres fixture starts");
            let warehouse = tempfile::tempdir().expect("warehouse directory");
            let catalog = BifrostCatalog::new(
                fixture.catalog_dsn().expose_secret(),
                local_storage_owner(warehouse.path()),
                fixture.vala_postgres().clone(),
            )
            .await
            .expect("redux catalog builds over the fixture");
            let tenant = fixture.data_tenant_id();
            let spans = crate::tables::builtin_table("traces", "spans").expect("spans is built in");
            let first = catalog
                .ensure_builtin(tenant, spans, wyrd_spec::auth::Attribution::unresolved())
                .await
                .expect("the first ensure registers");
            fixture.vala_postgres().pool().close().await;

            let again = catalog
                .ensure_builtin(tenant, spans, wyrd_spec::auth::Attribution::unresolved())
                .await
                .expect("the second ensure answers from the cache");

            assert_eq!(again, first);
        });
    }

    /// Every canonical built-in is created through [`BifrostCatalog::ensure_builtin`].
    ///
    /// Tenant provisioning and server startup ensure the whole
    /// [`crate::tables::builtin_tables`] inventory, so each definition must
    /// register — which also proves no built-in declares a reserved column —
    /// and must then resolve to the UID its ensure returned.
    ///
    /// # Panics
    /// Panics when the fixture fails, when any built-in cannot be ensured, or
    /// when an ensured built-in does not resolve to its returned UID.
    #[test]
    fn every_builtin_is_created_through_ensure_builtin() {
        wyrd_runtime::runtime().block_on(async {
            let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("postgres fixture starts");
            let warehouse = tempfile::tempdir().expect("warehouse directory");
            let catalog = BifrostCatalog::new(
                fixture.catalog_dsn().expose_secret(),
                local_storage_owner(warehouse.path()),
                fixture.vala_postgres().clone(),
            )
            .await
            .expect("redux catalog builds over the fixture");
            let tenant = fixture.data_tenant_id();
            for definition in crate::tables::builtin_tables() {
                let fqn = format!("{}.{}", definition.namespace, definition.name);
                let created = catalog
                    .ensure_builtin(
                        tenant,
                        definition,
                        wyrd_spec::auth::Attribution::unresolved(),
                    )
                    .await
                    .unwrap_or_else(|error| panic!("built-in {fqn} is ensured: {error}"));
                let namespace = BifrostNamespace::from_domain_namespace(definition.namespace)
                    .expect("every built-in namespace is known");
                assert_eq!(
                    catalog
                        .table_uid(&TableRef::new(namespace, definition.name), tenant)
                        .await
                        .unwrap_or_else(|error| panic!("built-in {fqn} resolves: {error}")),
                    created,
                    "built-in {fqn} resolves to the UID its ensure returned"
                );
            }
        });
    }

    /// Deletes every Avro manifest and manifest-list object under `root`.
    ///
    /// Removing them after a pin leaves the node-wide manifest cache as the
    /// only source a later plan can decode from.
    ///
    /// # Panics
    /// Panics when the warehouse cannot be listed, an object cannot be
    /// removed, or no manifest object exists to remove.
    fn delete_manifest_objects(root: &std::path::Path) {
        let mut deleted = 0_usize;
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).expect("warehouse directory lists") {
                let path = entry.expect("warehouse entry reads").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "avro") {
                    std::fs::remove_file(path).expect("manifest object deletes");
                    deleted += 1;
                }
            }
        }
        assert!(deleted > 0, "the append wrote manifests");
    }

    /// Reads and materializes one table's current cut the way Oracle does.
    ///
    /// # Panics
    /// Panics when the cut read or materialization fails.
    async fn pin_current(
        catalog: &BifrostCatalog,
        tenant: DataTenantId,
        table: &TableRef,
    ) -> PinnedSealedTable {
        let cut = catalog
            .read_cut(tenant, std::slice::from_ref(table))
            .await
            .expect("the registered table reads")
            .pop()
            .expect("one table cut");
        catalog
            .materialize_cut(tenant, table, cut)
            .await
            .expect("the committed snapshot pins")
    }

    /// Plans the current snapshot through a fresh cached table and returns how
    /// many file tasks it produced.
    ///
    /// # Panics
    /// Panics when the table cannot be reloaded, built, or planned.
    async fn planned_file_count(
        catalog: &BifrostCatalog,
        identifier: &iceberg::TableIdent,
    ) -> usize {
        use futures_util::TryStreamExt as _;
        let loaded = catalog
            .iceberg_catalog()
            .load_table(identifier)
            .await
            .expect("physical table reloads");
        let tasks: Vec<_> = catalog
            .cached_table(
                identifier,
                loaded.metadata_ref(),
                loaded.metadata_location().map(ToOwned::to_owned),
            )
            .expect("the scoped table builds")
            .scan()
            .build()
            .expect("the scan builds")
            .plan_files()
            .await
            .expect("planning reads the cached manifests")
            .try_collect()
            .await
            .expect("planning streams the cached manifests");
        tasks.len()
    }

    /// Registers `vala.datasets.<name>` with one nullable `value` column and
    /// fast-appends one fixture data file, returning the table's binding.
    ///
    /// # Panics
    /// Panics when registration, loading, or the append fails.
    async fn register_with_one_file(
        catalog: &BifrostCatalog,
        tenant: DataTenantId,
        table: &TableRef,
    ) -> crate::catalog::TenantTableBinding {
        catalog
            .register_dataset(
                tenant,
                wyrd_spec::auth::Attribution::unresolved(),
                table.clone(),
                vec![arrow::datatypes::Field::new(
                    "value",
                    arrow::datatypes::DataType::Int64,
                    true,
                )],
                None,
                CompactionRegistration::default(),
            )
            .await
            .expect("dataset registers");

        let binding = crate::catalog::TenantTableBinding::resolve((tenant, table.clone()))
            .expect("binding resolves");
        let physical = catalog
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("physical table loads");
        let lower_micros = 1_787_493_600_000_000_i64;
        let partition = crate::catalog::layout::TimeGranularity::Hour
            .bucket(
                chrono::DateTime::from_timestamp_micros(lower_micros)
                    .expect("fixture instant is representable"),
            )
            .expect("fixture instant buckets");
        let location = physical.metadata().location().to_owned();
        let data_file = iceberg::spec::DataFileBuilder::default()
            .content(iceberg::spec::DataContentType::Data)
            .file_path(format!("{location}/data/pinned.parquet"))
            .file_format(iceberg::spec::DataFileFormat::Parquet)
            .partition(iceberg::spec::Struct::from_iter([Some(
                partition.iceberg_partition_literal(),
            )]))
            .record_count(1)
            .file_size_in_bytes(1_024)
            .partition_spec_id(physical.metadata().default_partition_spec_id())
            .sort_order_id(
                i32::try_from(physical.metadata().default_sort_order_id())
                    .expect("fixture sort order id fits i32"),
            )
            .build()
            .expect("fixture data file builds");
        let transaction = Transaction::new(&physical);
        let action = transaction.fast_append().add_data_files([data_file]);
        let applied = ApplyTransactionAction::apply(action, transaction).expect("append applies");
        applied
            .commit(catalog.iceberg_catalog().as_ref())
            .await
            .expect("fast append commits");

        binding
    }

    /// A pinned provider resolves only against the snapshot it names.
    ///
    /// A follower resolves its own catalog handle, so without pinning its leaf
    /// would read whatever snapshot is current when it happens to resolve —
    /// a different file set, and a different schema, from the one the leader
    /// planned, digested, and signed. Naming a snapshot that the table does not
    /// publish must therefore fail to resolve rather than silently fall back to
    /// the current one, which is what this asserts: the committed snapshot
    /// resolves, and a neighbouring id does not.
    ///
    /// It also proves the pin and the scan share one decode: after the pin,
    /// every Avro manifest object is deleted from the warehouse, and a fresh
    /// cached table must still plan the committed file, which it can
    /// only do from the node-wide manifest cache the pin populated.
    ///
    /// # Panics
    /// Panics when the fixture, registration, or commit fails, when the
    /// committed snapshot does not resolve, when an unpublished snapshot id
    /// resolves anyway, or when planning re-reads a manifest the pin decoded.
    #[test]
    fn pinned_provider_refuses_a_snapshot_the_table_does_not_publish() {
        wyrd_runtime::runtime().block_on(async {
            let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("postgres fixture starts");
            let warehouse = tempfile::tempdir().expect("warehouse directory");
            let catalog = BifrostCatalog::new(
                fixture.catalog_dsn().expose_secret(),
                local_storage_owner(warehouse.path()),
                fixture.vala_postgres().clone(),
            )
            .await
            .expect("redux catalog builds over the fixture");

            let tenant = fixture.data_tenant_id();
            let table = TableRef::new(BifrostNamespace::Datasets, "pinned_provider");
            let binding = register_with_one_file(&catalog, tenant, &table).await;

            let pinned = pin_current(&catalog, tenant, &table).await;
            let snapshot_id = pinned
                .snapshot_id
                .expect("a committed table has a snapshot");
            delete_manifest_objects(warehouse.path());
            assert_eq!(
                planned_file_count(&catalog, &binding.table_ident()).await,
                1,
                "the pin's decoded manifests plan the file"
            );
            catalog
                .pinned_provider(&table, tenant, snapshot_id)
                .await
                .expect("the published snapshot resolves");
            assert!(
                catalog
                    .pinned_provider(&table, tenant, snapshot_id.wrapping_add(1))
                    .await
                    .is_err(),
                "an unpublished snapshot must fail to resolve rather than serve the current one"
            );
        });
    }

    /// A pin taken before a schema and partition-spec revision keeps reading
    /// the snapshot it named: its schema is the one that snapshot was written
    /// under, not the revised current schema, and its file still plans under
    /// the retained spec.
    ///
    /// # Panics
    /// Panics when the fixture, registration, commit, or pin fails, when the
    /// pinned schema shows the revised column, or when the file stops planning.
    #[test]
    fn pinned_snapshot_survives_schema_and_spec_revisions() {
        wyrd_runtime::runtime().block_on(async {
            let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("postgres fixture starts");
            let warehouse = tempfile::tempdir().expect("warehouse directory");
            let catalog = BifrostCatalog::new(
                fixture.catalog_dsn().expose_secret(),
                local_storage_owner(warehouse.path()),
                fixture.vala_postgres().clone(),
            )
            .await
            .expect("redux catalog builds over the fixture");
            let tenant = fixture.data_tenant_id();
            let table = TableRef::new(BifrostNamespace::Datasets, "pinned_revision");
            let binding = register_with_one_file(&catalog, tenant, &table).await;
            let snapshot_id = pin_current(&catalog, tenant, &table)
                .await
                .snapshot_id
                .expect("a committed table has a snapshot");

            let physical = catalog
                .iceberg_catalog()
                .load_table(&binding.table_ident())
                .await
                .expect("physical table loads");
            let metadata = physical.metadata();
            let current = metadata.current_schema();
            let schema = iceberg::spec::Schema::builder()
                .with_schema_id(current.schema_id() + 1)
                .with_fields(
                    current
                        .as_struct()
                        .fields()
                        .iter()
                        .cloned()
                        .chain([Arc::new(iceberg::spec::NestedField::optional(
                            metadata.last_column_id() + 1,
                            "note",
                            iceberg::spec::Type::Primitive(iceberg::spec::PrimitiveType::String),
                        ))]),
                )
                .build()
                .expect("revised schema builds");
            let spec = iceberg::spec::UnboundPartitionSpec::builder().build();
            let transaction = Transaction::new(&physical);
            let action = [
                iceberg::TableUpdate::AddSchema { schema },
                iceberg::TableUpdate::SetCurrentSchema { schema_id: -1 },
                iceberg::TableUpdate::AddSpec { spec },
                iceberg::TableUpdate::SetDefaultSpec { spec_id: -1 },
                iceberg::TableUpdate::SetProperties {
                    updates: [(
                        crate::catalog::TABLE_VERSION_PROPERTY.to_owned(),
                        "2".to_owned(),
                    )]
                    .into(),
                },
            ]
            .into_iter()
            .fold(
                transaction.apply_updates(),
                iceberg::transaction::ApplyUpdatesAction::update,
            );
            ApplyTransactionAction::apply(action, transaction)
                .expect("revision applies")
                .commit(catalog.iceberg_catalog().as_ref())
                .await
                .expect("revision commits");
            catalog.evict_current_definition(&table, tenant);

            let current = catalog
                .current_definition(&table, tenant)
                .await
                .expect("the current definition loads");
            assert_eq!(current.table_version.get(), 2);
            let retained = catalog
                .retained_definition(&binding, wyrd_spec::vala::ids::TableVersion::INITIAL)
                .await
                .expect("the retained reference resolves the prior version");
            assert_eq!(
                retained.table_version,
                wyrd_spec::vala::ids::TableVersion::INITIAL
            );
            assert!(
                retained.schema.field_by_name("note").is_none()
                    && current.schema.field_by_name("note").is_some(),
                "each version resolves its own schema"
            );

            let pinned = catalog
                .pinned_provider(&table, tenant, snapshot_id)
                .await
                .expect("the pinned snapshot still resolves after the revision");
            let pinned_schema = datafusion::datasource::TableProvider::schema(&pinned);
            assert!(
                pinned_schema.field_with_name("value").is_ok()
                    && pinned_schema.field_with_name("note").is_err(),
                "the pin reads the schema its snapshot was written under: {pinned_schema:?}"
            );
            assert_eq!(
                planned_file_count(&catalog, &binding.table_ident()).await,
                1,
                "the file written under the retained spec still plans"
            );
        });
    }

    /// Real production pinning resolves the time column from the manifest's
    /// own partition spec and decodes each committed file's bounds.
    ///
    /// The helper-level projection test above supplies an artificial field id
    /// and therefore cannot observe the production defect: production loads a
    /// manifest and must find the field id itself. This test commits four real
    /// data files — valid, missing-upper, wrong-type, and reversed — through an
    /// Iceberg transaction against a real catalog, then pins the sealed table
    /// and reads back exactly what production derived. None of the four has a
    /// `vala.file_list` row, which is the Forge-rewrite shape.
    ///
    /// The malformed case is an unrepresentable instant rather than a
    /// wrong-typed datum: a manifest bound is stored as raw bytes and re-typed
    /// from the writer schema on read, so the wrong-type class is unreachable
    /// through a real manifest and is proven at the projection instead.
    ///
    /// # Panics
    /// Panics when the fixture, registration, commit, or pin fails, or when a
    /// pinned file's derived statistics differ from its manifest evidence.
    #[test]
    fn pinned_snapshot_decodes_manifest_time_bounds_by_partition_source_id() {
        wyrd_runtime::runtime().block_on(async {
            let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("postgres fixture starts");
            let warehouse = tempfile::tempdir().expect("warehouse directory");
            let catalog = BifrostCatalog::new(
                fixture.catalog_dsn().expose_secret(),
                local_storage_owner(warehouse.path()),
                fixture.vala_postgres().clone(),
            )
            .await
            .expect("redux catalog builds over the fixture");

            let tenant = fixture.data_tenant_id();
            let table = TableRef::new(BifrostNamespace::Datasets, "pinned_bounds");
            catalog
                .register_dataset(
                    tenant,
                    wyrd_spec::auth::Attribution::unresolved(),
                    table.clone(),
                    vec![arrow::datatypes::Field::new(
                        "value",
                        arrow::datatypes::DataType::Int64,
                        true,
                    )],
                    None,
                    CompactionRegistration::default(),
                )
                .await
                .expect("dataset registers");

            let binding = crate::catalog::TenantTableBinding::resolve((tenant, table.clone()))
                .expect("binding resolves");
            let physical = catalog
                .iceberg_catalog()
                .load_table(&binding.table_ident())
                .await
                .expect("physical table loads");
            let field_id = crate::catalog::TimeAxis::iceberg_field_id(
                physical.metadata().default_partition_spec(),
            )
            .expect("the physical table partitions on its time column");

            let lower_micros = 1_787_493_600_000_000_i64;
            let upper_micros = 1_787_497_200_000_000_i64;
            let cases = manifest_cases(lower_micros, upper_micros);
            let data_files = manifest_data_files(&physical, field_id, &cases, lower_micros);

            let transaction = Transaction::new(&physical);
            let action = transaction.fast_append().add_data_files(data_files);
            let applied =
                ApplyTransactionAction::apply(action, transaction).expect("append applies");
            applied
                .commit(catalog.iceberg_catalog().as_ref())
                .await
                .expect("fast append commits");

            let pinned = pin_current(&catalog, tenant, &table).await;
            assert_eq!(
                pinned.iceberg_files.len(),
                cases.len(),
                "every committed file must reach the pinned cut"
            );
            for case in &cases {
                let file = pinned
                    .iceberg_files
                    .iter()
                    .find(|file| file.file_path.ends_with(case.name))
                    .unwrap_or_else(|| panic!("{} is pinned", case.name));
                assert_eq!(file.row_count, 4_096, "{} row count", case.name);
                assert_eq!(file.file_size, 2_097_152, "{} file size", case.name);
                assert_eq!(
                    file.event_time, case.expected,
                    "{} event-time statistics",
                    case.name
                );
            }
        });
    }
}
