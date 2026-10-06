//! Redux-owned tenant-qualified Bifrost catalog.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use arrow::datatypes::{Field, Schema};
use iceberg::io::object_cache::ObjectCache;
use iceberg::io::{FileIO, FileIOBuilder};
use iceberg::spec::{FormatVersion, TableMetadata, TableProperties, Transform};
use iceberg::{Catalog as _, Error as IcebergError, TableCreation};
use iceberg_catalog_sql::SqlCatalog;
use sha2::{Digest as _, Sha256};
use vala_sql::ValaPostgres;
use vala_sql::queries::file_list::HotFileCatalog;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::WYRD_EVENT_TIME;
use wyrd_spec::vala::api::{BifrostTableDescription, BifrostTableEntry, PhysicalLayoutWire};

use crate::catalog::error::BifrostCatalogError;
use crate::catalog::event_time::{EventTimeBoundsDefect, EventTimeStatistics};
use crate::catalog::iceberg_sql;
use crate::catalog::layout::PhysicalLayout;
use iceberg::io::StorageFactory;

use crate::catalog::iceberg_storage::BifrostIcebergStorageFactory;
use crate::catalog::storage::{iceberg_catalog_properties, warehouse_uri};
use crate::catalog::wire::{
    described_fields_from_stored_schema, entry_from_row, layout_wire_from_row,
    reject_reserved_field_names,
};
use crate::catalog::{TableRef, TenantTableBinding};
use crate::namespaces::BifrostNamespace;
use crate::schema::{SchemaFingerprint, with_managed_columns};
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

    /// Wraps a stable 16-byte identity already issued by the catalog.
    ///
    /// Used where a registered UID crosses a boundary as raw bytes, such as a
    /// Scribe test double answering Gate's destination resolution.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
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
    /// Stable digest of the ordered, post-subtraction hot manifest.
    pub hot_manifest_digest: String,
    /// Bounded sealed-byte estimate used by Oracle classification.
    pub estimated_bytes: u64,
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
    /// Normalized `wyrd_event_time` interval decoded from the manifest entry's
    /// typed lower/upper bounds, or the exact reason the file must be retained.
    pub event_time: EventTimeStatistics,
}

impl PinnedIcebergFile {
    /// Projects one pinned manifest entry into immutable pruning statistics.
    ///
    /// `event_time_field_id` is the `wyrd_event_time` field identity resolved
    /// from the *manifest's own writer schema*, not the current table schema:
    /// a rewrite output committed under an older schema keeps that schema's
    /// field id, and resolving against the newest schema would read the wrong
    /// bound. `None` means the writer schema had no such field at all, which is
    /// a missing-bounds fail-open rather than an error.
    ///
    /// Decoding never fails the pin. Every defect normalizes into
    /// [`EventTimeStatistics::Unusable`] so the file is retained and executed
    /// through the residual predicate and footer tenant proof.
    #[must_use]
    fn from_manifest_entry(
        file_path: String,
        data_file: &iceberg::spec::DataFile,
        event_time_field_id: Option<i32>,
    ) -> Self {
        let event_time = match event_time_field_id {
            None => EventTimeStatistics::Unusable(EventTimeBoundsDefect::Missing),
            Some(field_id) => manifest_event_time_statistics(data_file, field_id),
        };
        Self {
            file_path,
            file_size: data_file.file_size_in_bytes(),
            row_count: data_file.record_count(),
            event_time,
        }
    }
}

/// Decodes one manifest entry's `wyrd_event_time` interval by field identity.
///
/// A bound that is absent is [`EventTimeBoundsDefect::Missing`]; one that is
/// present but not a `timestamptz` long literal, or not representable as a UTC
/// microsecond instant, is [`EventTimeBoundsDefect::Invalid`]. Both classes
/// retain the file.
fn manifest_event_time_statistics(
    data_file: &iceberg::spec::DataFile,
    field_id: i32,
) -> EventTimeStatistics {
    let decode = |bounds: &HashMap<i32, iceberg::spec::Datum>| match bounds.get(&field_id) {
        None => Err(EventTimeBoundsDefect::Missing),
        Some(datum) => {
            if datum.data_type() != &iceberg::spec::PrimitiveType::Timestamptz {
                return Err(EventTimeBoundsDefect::Invalid);
            }
            let iceberg::spec::PrimitiveLiteral::Long(micros) = datum.literal() else {
                return Err(EventTimeBoundsDefect::Invalid);
            };
            if chrono::DateTime::from_timestamp_micros(*micros).is_none() {
                return Err(EventTimeBoundsDefect::Invalid);
            }
            Ok(*micros)
        }
    };
    match (
        decode(data_file.lower_bounds()),
        decode(data_file.upper_bounds()),
    ) {
        (Ok(min_micros), Ok(max_micros)) => {
            EventTimeStatistics::normalize(Some(min_micros), Some(max_micros))
        }
        (Err(defect), _) | (_, Err(defect)) => EventTimeStatistics::Unusable(defect),
    }
}

/// Immutable Iceberg snapshot metadata collected before hot-manifest reconciliation.
struct PinnedIcebergState {
    /// Current snapshot identity, when the table has committed data.
    snapshot_id: Option<i64>,
    /// Validated Forge publication operation recorded by the current snapshot.
    forge_publication_operation_id: Option<uuid::Uuid>,
    /// Canonical paths used to exclude hot-manifest overlap.
    file_paths: BTreeSet<String>,
    /// Exact immutable metadata keyed by canonical path.
    files: BTreeMap<String, PinnedIcebergFile>,
    /// Checked sum of immutable object bytes.
    estimated_bytes: u64,
}

/// Everything one reader needs to protect a cut before it opens anything.
///
/// Produced by [`BifrostCatalog::prepare_reader_identity`] and consumed by
/// [`BifrostCatalog::materialize_reader_cut`]. It is deliberately inert: it
/// names a snapshot and carries the immutable metadata that names it, and holds
/// no `FileIO`, provider, or open object of its own.
#[derive(Debug, Clone)]
pub struct PreparedReaderIdentity {
    /// Authenticated tenant the cut belongs to.
    pub tenant: DataTenantId,
    /// Resolved tenant/table binding for the physical table.
    pub binding: TenantTableBinding,
    /// Durable registered identity protection is keyed by.
    pub table_uid: TableUid,
    /// Physical Iceberg identifier the table was loaded under.
    pub identifier: iceberg::TableIdent,
    /// Immutable metadata document naming this cut.
    pub metadata: iceberg::spec::TableMetadataRef,
    /// Immutable location the metadata document was read from.
    ///
    /// Every catalog commit writes a new `<version>-<uuid>.metadata.json` and
    /// swaps the pointer to it, so this location names exactly one document.
    pub metadata_location: String,
    /// Current snapshot, or `None` for a table that has never committed.
    pub snapshot_id: Option<i64>,
    /// That snapshot's own recorded commit timestamp in milliseconds.
    pub snapshot_timestamp_ms: Option<i64>,
    /// Ancestry from the current snapshot to the oldest reachable parent.
    pub ancestry_path: Vec<i64>,
}

/// Walks one snapshot's parent ancestry from the immutable metadata document.
///
/// Bounded by the retained snapshot count: an ancestry longer than the
/// metadata's own snapshot list is a cycle, not deep history, and following it
/// would not terminate.
///
/// # Errors
/// Returns [`BifrostCatalogError::MetadataMismatch`] when the ancestry does not
/// terminate.
fn ancestry_path(
    metadata: &iceberg::spec::TableMetadataRef,
    snapshot: &iceberg::spec::Snapshot,
) -> Result<Vec<i64>, BifrostCatalogError> {
    let mut path = vec![snapshot.snapshot_id()];
    let mut cursor = snapshot.parent_snapshot_id();
    let bound = metadata.snapshots().count().saturating_add(1);
    while let Some(parent) = cursor {
        if path.len() > bound || path.contains(&parent) {
            return Err(BifrostCatalogError::MetadataMismatch(
                "pinned snapshot ancestry does not terminate".to_owned(),
            ));
        }
        path.push(parent);
        cursor = metadata
            .snapshot_by_id(parent)
            .and_then(|snapshot| snapshot.parent_snapshot_id());
    }
    Ok(path)
}

/// Redux catalog shared by Gate, Forge, Oracle, and server catalog routes.
#[derive(Clone)]
pub struct BifrostCatalog {
    /// The one Iceberg SQL catalog, held concretely so a reader can recheck
    /// its authoritative metadata pointer without reloading the document.
    catalog: Arc<SqlCatalog>,
    postgres: ValaPostgres,
    warehouse: String,
    file_io: FileIO,
    /// The node's storage owner, retained so a query can build its own
    /// epoch-gated `FileIO` instead of borrowing this catalog's ungated one.
    storage: Arc<BifrostStorage>,
    /// Backend properties every built `FileIO` must share with the catalog.
    storage_properties: std::collections::HashMap<String, String>,
    /// Node-wide decoded manifest and manifest-list cache shared by every
    /// permit-scoped read table, so the snapshot pin and `plan_files` decode
    /// each immutable manifest once per node instead of once per consumer per
    /// query. Seeded lazily because iceberg only builds a cache with a table.
    manifest_cache: Arc<std::sync::OnceLock<Arc<ObjectCache>>>,
    /// Registered UIDs this node has already read, by tenant and table name.
    ///
    /// A registration row is immutable once written: its UID is minted once,
    /// `(tenant, fqn)` is unique, and no production path updates or deletes
    /// it. Only found rows are cached, so a table registered later is still
    /// seen on its first lookup. Grows with registered tables, never with
    /// queries.
    table_uids: Arc<std::sync::RwLock<HashMap<(DataTenantId, String), TableUid>>>,
}

/// Estimated-weight eviction limit of the node-wide decoded manifest cache.
///
/// Manifest and manifest-list paths embed a commit UUID, so entries are
/// immutable and never need invalidation; eviction is size-only LRU over
/// iceberg's estimated entry weight. This is not a hard memory ceiling: a
/// query still holding an evicted `Arc`, or loads in flight, can briefly
/// exceed it. It sits outside the Oracle memory root because decoded
/// manifests are small and shared across every query. A cache hit performs no
/// storage IO and therefore consults no reader IO permit.
const MANIFEST_CACHE_BYTES: u64 = 64 * 1024 * 1024;

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

/// Reader identities prepared by production code paths in serialized tests.
///
/// A restart after catalog promotion has to re-prepare every table, not only
/// the one that drifted, so the count is what distinguishes a complete restart
/// from a partial one.
#[cfg(any(test, feature = "test-support"))]
static TEST_PREPARED_IDENTITY_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Authoritative metadata-pointer reads observed by one catalog test.
#[cfg(test)]
static TEST_METADATA_POINTER_READS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Reader-identity metadata documents read by one catalog test.
#[cfg(test)]
static TEST_METADATA_DOCUMENT_READS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Revalidations still to be failed before the next one is allowed to succeed.
#[cfg(any(test, feature = "test-support"))]
static TEST_REVALIDATION_FAULTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Resets and returns the observed prepared-identity count for one test.
#[cfg(any(test, feature = "test-support"))]
pub fn reset_prepared_identity_count_for_test() -> usize {
    TEST_PREPARED_IDENTITY_COUNT.swap(0, std::sync::atomic::Ordering::SeqCst)
}

/// Returns prepared reader identities observed since the last reset.
#[must_use]
#[cfg(any(test, feature = "test-support"))]
pub fn prepared_identity_count_for_test() -> usize {
    TEST_PREPARED_IDENTITY_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Makes the next `count` revalidations report authoritative metadata drift.
///
/// A real promotion between preparation and materialization is what this
/// stands in for. Committing one at that exact point from outside the call is
/// not reachable, because preparation, protection, revalidation, and
/// materialization are one operation by construction.
#[cfg(any(test, feature = "test-support"))]
pub fn inject_revalidation_faults_for_test(count: usize) {
    TEST_REVALIDATION_FAULTS.store(count, std::sync::atomic::Ordering::SeqCst);
}

/// Returns how many injected revalidation faults remain unconsumed.
#[must_use]
#[cfg(any(test, feature = "test-support"))]
pub fn pending_revalidation_faults_for_test() -> usize {
    TEST_REVALIDATION_FAULTS.load(std::sync::atomic::Ordering::SeqCst)
}

impl BifrostCatalog {
    /// Resolves only the identity of the cut a reader is about to protect.
    ///
    /// This is deliberately the whole of what may happen before protection: a
    /// registration lookup (served from [`BifrostCatalog::table_uid`]'s cache
    /// after the node's first write or query of the table), one authoritative
    /// metadata-pointer read, one read of
    /// the immutable metadata document it names, and the facts derived from
    /// that document. Reading that document is the allowed
    /// identity step because there is no other way to name the snapshot that
    /// protection has to cover. Nothing here loads a manifest list, enumerates
    /// data files, queries the hot manifest, builds a provider, or opens any
    /// object the snapshot names — all of that is snapshot-dependent IO, and it
    /// belongs after [`BifrostCatalog::materialize_reader_cut`] takes a permit.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the table is not
    /// registered for this tenant or has no catalog pointer, an invalid-binding
    /// error when the tenant and table cannot be bound, and a catalog error when
    /// the pointer or the Iceberg metadata document cannot be read.
    pub async fn prepare_reader_identity(
        &self,
        table: &TableRef,
        tenant: DataTenantId,
    ) -> Result<PreparedReaderIdentity, BifrostCatalogError> {
        #[cfg(any(test, feature = "test-support"))]
        TEST_PREPARED_IDENTITY_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let table_uid = self.table_uid(table, tenant).await?;
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let identifier = binding.table_ident();
        let metadata_location = self.metadata_pointer(&identifier).await?;
        #[cfg(test)]
        TEST_METADATA_DOCUMENT_READS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let metadata = Arc::new(TableMetadata::read_from(&self.file_io, &metadata_location).await?);
        let snapshot = metadata.current_snapshot();
        Ok(PreparedReaderIdentity {
            tenant,
            binding,
            table_uid,
            identifier,
            metadata_location,
            snapshot_id: snapshot.map(|snapshot| snapshot.snapshot_id()),
            snapshot_timestamp_ms: snapshot.map(|snapshot| snapshot.timestamp_ms()),
            ancestry_path: snapshot
                .map(|snapshot| ancestry_path(&metadata, snapshot))
                .transpose()?
                .unwrap_or_default(),
            metadata,
        })
    }

    /// Proves the authoritative catalog still holds the prepared identity.
    ///
    /// Protection is taken against the metadata document preparation read, and
    /// that document is then reused for the whole cut, so nothing downstream
    /// can notice that the table was promoted in between. This is the one place
    /// that asks the catalog again: one authoritative pointer read per prepared
    /// identifier. The pointer alone is sufficient because the Iceberg SQL
    /// catalog, the only publication path Wyrd uses, commits by writing a new
    /// `<version>-<uuid>.metadata.json` and compare-and-swapping the pointer to
    /// it; no supported writer changes a document in place. An unchanged
    /// pointer therefore names the same immutable document preparation read.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::MetadataMismatch`] when the authoritative
    /// pointer differs from the prepared one,
    /// [`BifrostCatalogError::TableNotFound`] when the table's pointer is gone,
    /// and a catalog error when the pointer cannot be read.
    pub async fn revalidate_reader_identity(
        &self,
        prepared: &PreparedReaderIdentity,
    ) -> Result<(), BifrostCatalogError> {
        #[cfg(any(test, feature = "test-support"))]
        if TEST_REVALIDATION_FAULTS
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |remaining| remaining.checked_sub(1),
            )
            .is_ok()
        {
            return Err(BifrostCatalogError::MetadataMismatch(
                "injected authoritative reader-identity drift".to_owned(),
            ));
        }
        if self.metadata_pointer(&prepared.identifier).await? != prepared.metadata_location {
            return Err(BifrostCatalogError::MetadataMismatch(
                "the authoritative table metadata pointer moved under the prepared identity"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    /// Reads one table's authoritative metadata pointer and nothing else.
    ///
    /// The read runs through the Iceberg SQL catalog's own primary connection
    /// and catalog-owner credential, with the exact catalog, namespace, and
    /// table predicates. The request role is deliberately denied the catalog
    /// schema, so this is never issued from a tenant connection.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::TableNotFound`] when the catalog has no
    /// row for the identifier, and a catalog error when the read fails.
    async fn metadata_pointer(
        &self,
        identifier: &iceberg::TableIdent,
    ) -> Result<String, BifrostCatalogError> {
        #[cfg(test)]
        TEST_METADATA_POINTER_READS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.catalog
            .load_metadata_location(identifier)
            .await?
            .ok_or_else(|| BifrostCatalogError::TableNotFound(identifier.to_string()))
    }

    /// Materializes the pinned cut through storage gated by one reader permit.
    ///
    /// The table is rebuilt here rather than reused from
    /// [`BifrostCatalog::prepare_reader_identity`] for one reason: the catalog's
    /// table carries the catalog's own ungated `FileIO`, and every manifest,
    /// data, and delete object this cut opens must go through the permit
    /// instead. Reusing the loaded table would leave an ungated route to
    /// exactly the objects the protection was taken for.
    ///
    /// Drift between preparation and this call is not detectable here: this
    /// method builds its table from the prepared metadata, so comparing the
    /// result against that same document proves nothing. The authoritative
    /// check is [`BifrostCatalog::revalidate_reader_identity`], which the
    /// caller runs for every prepared table before materializing any.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::MetadataMismatch`] when the table moved
    /// under the prepared identity, [`BifrostCatalogError::AmbiguousPublication`]
    /// or [`BifrostCatalogError::UnstableCut`] from the underlying cut, a
    /// catalog or SQL error when the manifest or hot cut cannot be read, and
    /// [`BifrostCatalogError::Iceberg`] when the permit no longer authorizes
    /// exposing the cut.
    pub async fn materialize_reader_cut(
        &self,
        prepared: PreparedReaderIdentity,
        permit: &crate::oracle::reader_pins::ReaderIoPermit,
    ) -> Result<PinnedSealedTable, BifrostCatalogError> {
        #[cfg(any(test, feature = "test-support"))]
        TEST_SEALED_PIN_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let tenant = prepared.tenant;
        let binding = prepared.binding;
        let table_uid = prepared.table_uid;
        let gated = self.permit_scoped_table(
            &prepared.identifier,
            prepared.metadata.clone(),
            Some(prepared.metadata_location.clone()),
            permit,
        )?;
        let (iceberg_table, pinned, cut) = self
            .acquire_stable_cut_from(gated, &binding, tenant)
            .await?;
        // A cut served wholly from the node-wide manifest cache opened nothing
        // through the gated `FileIO`, so the permit is checked once more before
        // the cut is exposed: an epoch that lost authority returns no cut.
        permit
            .expose_result()
            .map_err(|error| crate::catalog::iceberg_storage::permit_error(&error))?;
        let snapshot_id = pinned.snapshot_id;
        let iceberg_file_paths = pinned.file_paths;
        let iceberg_files = pinned.files;
        let mut estimated_bytes = pinned.estimated_bytes;
        if cut.ambiguous_publication {
            return Err(BifrostCatalogError::AmbiguousPublication);
        }
        let hot_files = cut.hot_files;
        for row in &hot_files {
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
        metrics::counter!(
            "bifrost_oracle_files_pruned_total",
            "source" => "hot_sealed",
            "reason" => "snapshot_overlap"
        )
        .increment(cut.represented as u64);
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
            table_uid,
            iceberg_table,
            snapshot_id,
            snapshot_digest,
            iceberg_file_paths,
            iceberg_files: iceberg_files.into_values().collect(),
            hot_files,
            hot_manifest_digest,
            estimated_bytes,
        })
    }

    /// Builds a `FileIO` whose every read is gated by one reader permit.
    ///
    /// This is the only place Oracle constructs read storage for a protected
    /// cut. Writes and deletes are refused by the storage itself rather than by
    /// convention, so a query path cannot mutate warehouse objects even if it
    /// reaches an Iceberg API that would.
    #[must_use]
    pub fn gated_file_io(
        &self,
        permit: &crate::oracle::reader_pins::ReaderIoPermit,
    ) -> iceberg::io::FileIO {
        let factory = Arc::new(
            crate::catalog::iceberg_storage::EpochGatedIcebergStorageFactory::new(
                Arc::clone(&self.storage),
                &self.warehouse,
                permit.clone(),
            ),
        ) as Arc<dyn StorageFactory>;
        FileIOBuilder::new(factory)
            .with_props(self.storage_properties.clone())
            .build()
    }

    /// Rebuilds one immutable table over storage gated by a reader permit.
    ///
    /// Uses the prepared metadata document rather than reloading it, so this
    /// step opens nothing: the table it returns is the same immutable metadata
    /// with a `FileIO` that refuses every object read the permit no longer
    /// authorizes. The table shares the node-wide manifest cache; a cache miss
    /// still reads through this table's gated `FileIO`, and a hit can only be a
    /// manifest under this tenant's own table prefix because cache keys are
    /// full object paths.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::Iceberg`] when the table cannot be built
    /// from the prepared identity and metadata.
    fn permit_scoped_table(
        &self,
        identifier: &iceberg::TableIdent,
        metadata: iceberg::spec::TableMetadataRef,
        metadata_location: Option<String>,
        permit: &crate::oracle::reader_pins::ReaderIoPermit,
    ) -> Result<iceberg::table::Table, BifrostCatalogError> {
        let manifest_cache = self.manifest_cache(identifier, &metadata)?;
        let mut builder = iceberg::table::Table::builder()
            .file_io(self.gated_file_io(permit))
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
    /// first caller builds a throwaway table over the catalog's ungated
    /// `FileIO` with [`MANIFEST_CACHE_BYTES`] and keeps its cache. The seed's
    /// `FileIO` is never used for reads: every consumer attaches the entries
    /// with `Table::with_object_cache`, which rebinds misses to that table's
    /// own gated `FileIO`. A racing first caller builds a redundant seed and
    /// discards it.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::Iceberg`] when the seed table cannot be
    /// built from the supplied identity and metadata.
    fn manifest_cache(
        &self,
        identifier: &iceberg::TableIdent,
        metadata: &iceberg::spec::TableMetadataRef,
    ) -> Result<Arc<ObjectCache>, BifrostCatalogError> {
        if let Some(cache) = self.manifest_cache.get() {
            return Ok(Arc::clone(cache));
        }
        let seed = iceberg::table::Table::builder()
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

    /// Acquires one stable Iceberg/SQL/Iceberg cut for a tenant table.
    ///
    /// Every reload uses the same permit-scoped table the caller supplied, so
    /// the stability retry can never fall back to the catalog's ungated
    /// storage part-way through.
    ///
    /// # Errors
    ///
    /// Returns a catalog or SQL error for one failed read, or `UnstableCut`
    /// after three complete snapshot identity mismatches. Cancellation drops
    /// the in-flight attempt without exposing partial state.
    async fn acquire_stable_cut_from(
        &self,
        gated: iceberg::table::Table,
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
        let pinned = self.pin_iceberg_snapshot(&gated, binding).await?;
        let cut = async {
            let hot_file_catalog =
                HotFileCatalog::new(&binding.logical_namespace, &binding.table_name);
            let mut conn = self.postgres.tenant_conn(tenant).await?;
            let cut = hot_file_catalog
                .unresolved_for_cut(
                    &mut conn,
                    &pinned.file_paths,
                    pinned.forge_publication_operation_id,
                )
                .await?;
            conn.commit().await?;
            Ok::<_, BifrostCatalogError>(cut)
        }
        .await;
        Ok((gated, pinned, cut?))
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
                uuid::Uuid::parse_str(value).map_err(|_| {
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
            // Resolved from the manifest's own writer schema, once per manifest.
            // A file committed under an older schema keeps that schema's field
            // ids, so resolving against the table's current schema would read
            // the bounds of whichever column now happens to hold that id.
            let event_time_field_id = manifest
                .metadata()
                .schema()
                .field_by_name(WYRD_EVENT_TIME)
                .map(|field| field.id);
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
                    event_time_field_id,
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
        let backend = storage.handle().backend_config();
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
            warehouse,
            file_io,
            storage,
            storage_properties,
            manifest_cache: Arc::new(std::sync::OnceLock::new()),
            table_uids: Arc::default(),
        })
    }

    /// Borrows the node's one storage owner this catalog binds Iceberg to.
    ///
    /// Oracle needs the owner itself, not only the `FileIO` built over it,
    /// because the hot-footer path asks the owner to decode metadata under its
    /// cache, admission, and retry policy rather than reading ranges blindly.
    #[must_use]
    pub fn storage(&self) -> &Arc<BifrostStorage> {
        &self.storage
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
        self.create_table_locked(request, None, None).await
    }

    /// Register a caller-owned dataset in the tenant-qualified dataset namespace.
    ///
    /// `compaction_target_file_size_bytes`, when supplied, becomes the new
    /// table's explicit `write.target-file-size-bytes` property; omitted, the
    /// table follows Forge's deployment default. On an existing table it must
    /// match the stored explicit target or be omitted.
    ///
    /// # Errors
    /// Returns a typed catalog error when the dataset name, schema, physical table,
    /// compaction target, or control row is invalid.
    pub async fn register_dataset(
        &self,
        tenant: DataTenantId,
        table: TableRef,
        user_fields: Vec<Field>,
        physical_layout: Option<PhysicalLayoutWire>,
        compaction_target_file_size_bytes: Option<u64>,
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
            },
            None,
            compaction_target_file_size_bytes,
        )
        .await
    }

    /// Ensure one canonical built-in exists for a tenant.
    ///
    /// This is the only path that creates a built-in, and tenant provisioning
    /// and server boot are its only production callers: the server ensures the whole
    /// canonical inventory when it provisions a tenant and again for every
    /// active tenant at startup. Ingest and describe never create one and
    /// treat a missing built-in as an unregistered table.
    /// Repeating it for an existing built-in returns the same [`TableUid`].
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::MetadataMismatch`] for a definition in an
    /// unknown namespace, and the registration, SQL, or Iceberg failure of the
    /// underlying create otherwise.
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
            },
            Some((definition.schema)()),
            None,
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
    /// `compaction_target_file_size_bytes` is checked for intrinsic shape
    /// before the transaction, compared under the advisory lock against an
    /// existing physical table's explicit target, and otherwise written as the
    /// new table's `write.target-file-size-bytes` in its create transaction.
    ///
    /// # Errors
    /// Returns [`BifrostCatalogError::ReservedColumn`] when a declaration
    /// names a server-owned column,
    /// [`BifrostCatalogError::Registration`] for an invalid or
    /// conflicting layout or compaction target,
    /// [`BifrostCatalogError::FingerprintMismatch`] for a schema conflict, and
    /// metadata, Iceberg, SQL, or audit errors otherwise.
    async fn create_table_locked(
        &self,
        request: CreateTableRequest,
        canonical_schema: Option<arrow::datatypes::SchemaRef>,
        compaction_target_file_size_bytes: Option<u64>,
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
        if let Some(bytes) = compaction_target_file_size_bytes
            && !crate::forge::managed::policy::registrable_target_file_size_bytes(bytes)
        {
            return Err(BifrostCatalogError::Registration(
                wyrd_spec::vala::BifrostError::InvalidCompactionTarget { table: fqn, bytes },
            ));
        }
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
            assert_compaction_target(physical.metadata(), compaction_target_file_size_bytes, &fqn)?;
            conn.commit().await?;
            return TableUid::from_row(&row.table_uid, &row.fqn);
        }

        self.ensure_namespace(binding.physical_namespace()).await?;
        if physical_exists {
            let physical = self.catalog.load_table(&table_ident).await?;
            self.validate_physical_table(&physical, &binding, &arrow_schema, &layout)?;
            assert_compaction_target(physical.metadata(), compaction_target_file_size_bytes, &fqn)?;
        } else {
            self.create_physical_table(
                &binding,
                &arrow_schema,
                &layout,
                compaction_target_file_size_bytes,
            )
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
        conn.commit().await?;
        Ok(table_uid)
    }

    /// Create the physical Iceberg table for one canonical layout.
    ///
    /// Called only when registration has established that no physical table
    /// exists yet. Every physical decision — schema ids, partition spec, sort
    /// order, location, and the Bloom and Forge data-path properties — is
    /// derived from `layout` and `binding`, so the canonical layout stays the
    /// single authority for the table's shape. The Forge data path is written
    /// as `write.data.path` so the managed rewrite core roots its outputs under
    /// the recipe segment instead of the default data root. A supplied
    /// `compaction_target_file_size_bytes` is written as the table's explicit
    /// `write.target-file-size-bytes`; omitted, no target property is written
    /// so Forge resolves its deployment default at planning time.
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
        compaction_target_file_size_bytes: Option<u64>,
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
        if let Some(bytes) = compaction_target_file_size_bytes {
            properties.insert(TARGET_FILE_SIZE_PROPERTY.to_owned(), bytes.to_string());
        }
        let creation = TableCreation::builder()
            .name(binding.table_name.clone())
            .location(location)
            .schema(iceberg_schema)
            .format_version(FormatVersion::V2)
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
        let key = (tenant, table.fqn());
        let cached = self
            .table_uids
            .read()
            .expect("the registration cache lock is never poisoned")
            .get(&key)
            .copied();
        if let Some(table_uid) = cached {
            return Ok(table_uid);
        }
        let fqn = &key.1;
        let row = self
            .lookup_table_row(fqn, tenant)
            .await?
            .ok_or_else(|| BifrostCatalogError::TableNotFound(fqn.clone()))?;
        let table_uid = TableUid::from_row(&row.table_uid, fqn)?;
        self.table_uids
            .write()
            .expect("the registration cache lock is never poisoned")
            .insert(key, table_uid);
        Ok(table_uid)
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
        // Schema only: the provider is built from the loaded metadata document
        // and dropped here, so this path opens no snapshot object and needs no
        // reader permit to gate one.
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
            .ok_or_else(|| BifrostCatalogError::TableNotFound(fqn))?;
        // A canonical built-in is described from its own declaration, not from
        // the Iceberg round trip. The catalog assigns its own sequential field
        // ids at table creation, so the stored schema's ids diverge from the
        // ledger's after the first nested column — and it is the ledger's ids
        // that Scribe enforces on every stamped canonical batch. Describing the
        // stored ids would hand a writer a schema its own batches fail against.
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
            user_fields: described.user_fields,
            correlation_fields: described.correlation_fields,
            managed_candidates: described.managed_candidates,
            canonical_physical_fingerprint: builtin
                .and_then(|definition| (definition.canonical_physical_fingerprint)())
                .map(crate::tables::CanonicalPhysicalFingerprint::to_hex),
            physical_layout: layout_wire_from_row(&row)?,
            compaction_target_file_size_bytes: explicit_compaction_target(
                iceberg_table.metadata(),
            )?,
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
        permit: &crate::oracle::reader_pins::ReaderIoPermit,
    ) -> Result<IcebergStaticTableProvider, BifrostCatalogError> {
        let fqn = table.fqn();
        let Some(_row) = self.lookup_table_row(&fqn, tenant).await? else {
            return Err(BifrostCatalogError::TableNotFound(fqn));
        };
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let identifier = binding.table_ident();
        let loaded = self.catalog.load_table(&identifier).await?;
        let iceberg_table = self.permit_scoped_table(
            &identifier,
            loaded.metadata_ref(),
            loaded.metadata_location().map(ToOwned::to_owned),
            permit,
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
        permit: &crate::oracle::reader_pins::ReaderIoPermit,
    ) -> Result<IcebergStaticTableProvider, BifrostCatalogError> {
        let fqn = table.fqn();
        let Some(_row) = self.lookup_table_row(&fqn, tenant).await? else {
            return Err(BifrostCatalogError::TableNotFound(fqn));
        };
        let binding = TenantTableBinding::resolve((tenant, table.clone()))
            .map_err(|error| BifrostCatalogError::InvalidBinding(error.to_string()))?;
        let identifier = binding.table_ident();
        let loaded = self.catalog.load_table(&identifier).await?;
        let iceberg_table = self.permit_scoped_table(
            &identifier,
            loaded.metadata_ref(),
            loaded.metadata_location().map(ToOwned::to_owned),
            permit,
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
            .all(|(expected, actual)| field_shape_matches(expected, actual))
}

/// Whether two fields describe the same column name, nullability, and layout.
fn field_shape_matches(expected: &Field, actual: &Field) -> bool {
    expected.name() == actual.name()
        && expected.is_nullable() == actual.is_nullable()
        && crate::tables::arrow_type_shape_matches(expected.data_type(), actual.data_type())
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
        || Schema::new(with_managed_columns(user_fields.to_vec())),
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

/// Checks a re-registration's compaction target against the stored one.
///
/// Omission always matches and leaves the stored property untouched; a
/// supplied value must equal the table's explicit target.
///
/// # Errors
/// Returns [`BifrostCatalogError::Registration`] carrying
/// `CompactionTargetMismatch` when a supplied target differs from the stored
/// explicit target (including when none is stored), and a metadata mismatch
/// when the stored property is malformed.
fn assert_compaction_target(
    metadata: &TableMetadata,
    supplied: Option<u64>,
    fqn: &str,
) -> Result<(), BifrostCatalogError> {
    match supplied {
        Some(bytes) if explicit_compaction_target(metadata)? != Some(bytes) => {
            Err(BifrostCatalogError::Registration(
                wyrd_spec::vala::BifrostError::CompactionTargetMismatch {
                    table: fqn.to_owned(),
                },
            ))
        }
        _ => Ok(()),
    }
}

/// Maps an Iceberg scan-provider construction failure into the catalog error
/// its callers document.
fn provider_error(error: IcebergError) -> BifrostCatalogError {
    BifrostCatalogError::DataFusion(datafusion::error::DataFusionError::External(Box::new(
        error,
    )))
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

    /// The same columns in a different order are a different physical shape.
    ///
    /// Position is the whole comparison. Nothing consults a declared Iceberg
    /// field id to call two orderings the same table, so a reordered canonical
    /// schema is a mismatch here and a `MetadataMismatch` at registration.
    #[test]
    fn schema_shape_rejects_reordered_columns() {
        let declared = Schema::new(vec![
            Field::new("seq", DataType::Int64, false),
            Field::new("entry_hash", DataType::Utf8, false),
        ]);
        let reordered = Schema::new(vec![
            Field::new("entry_hash", DataType::Utf8, false),
            Field::new("seq", DataType::Int64, false),
        ]);

        assert!(!schema_shape_matches(&declared, &reordered));
        assert!(!schema_shape_matches(&reordered, &declared));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use arrow::datatypes::{DataType, Field};

    use super::{MAX_PHYSICAL_LEAF_COLUMNS, PinnedIcebergFile, resolve_registration_layout};
    use crate::catalog::event_time::{EventTimeBoundsDefect, EventTimeStatistics};

    /// Builds one Forge-style rewrite output's manifest `DataFile` with typed
    /// `wyrd_event_time` bounds under the given field id.
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
    use wyrd_spec::vala::WYRD_EVENT_TIME;

    use crate::storage::BifrostStorage;

    use std::sync::Arc;

    use super::BifrostCatalog;
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
    /// The catalog now runs its Iceberg I/O through the node's storage owner,
    /// so a catalog fixture must compose the same owner production does rather
    /// than hand the catalog a bare backend description.
    ///
    /// # Panics
    /// Panics when the signer, resource observation, or storage policy the
    /// fixture asks for is invalid.
    pub(super) fn local_storage_owner(root: &Path) -> Arc<BifrostStorage> {
        let signer = wyrd_storage::signer::BackendSigner::Local(
            wyrd_storage::local::LocalSigner::new(root.to_path_buf())
                .expect("fixture local signer"),
        );
        Arc::new(crate::storage::BifrostStorage::new(
            Arc::new(wyrd_storage::handle::StorageHandle::new(signer)),
            crate::storage::BifrostStoragePolicy::resolve(
                crate::storage::BifrostStorageConfig::default(),
                2 * 1024 * 1024 * 1024,
                false,
            )
            .expect("the fixture storage policy is valid"),
            None,
        ))
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

    /// One stable protected cut reads the immutable metadata document once.
    ///
    /// Preparation reads the authoritative pointer and the document it names;
    /// the post-protection recheck reads only the pointer again; and
    /// materialization reuses the prepared document. The pointer is read
    /// through the catalog-owner credential, while the request role's tenant
    /// connection is still refused the catalog schema outright.
    ///
    /// # Panics
    /// Panics when the fixture, registration, or cut fails, when the cut reads
    /// the document or the pointer a different number of times, or when the
    /// request role can read the catalog table.
    #[test]
    fn protected_cut_reuses_immutable_metadata() {
        use std::sync::atomic::Ordering::SeqCst;
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
            let table = TableRef::new(BifrostNamespace::Datasets, "immutable_metadata");
            catalog
                .register_dataset(
                    tenant,
                    table.clone(),
                    vec![arrow::datatypes::Field::new(
                        "value",
                        arrow::datatypes::DataType::Int64,
                        true,
                    )],
                    None,
                    None,
                )
                .await
                .expect("dataset registers");

            super::TEST_METADATA_POINTER_READS.store(0, SeqCst);
            super::TEST_METADATA_DOCUMENT_READS.store(0, SeqCst);
            let permit = crate::oracle::reader_pins::ReaderIoPermit::unfenced_for_test();
            let prepared = catalog
                .prepare_reader_identity(&table, tenant)
                .await
                .expect("the registered table prepares");
            catalog
                .revalidate_reader_identity(&prepared)
                .await
                .expect("an unchanged pointer revalidates");
            catalog
                .materialize_reader_cut(prepared, &permit)
                .await
                .expect("the cut materializes");
            assert_eq!(super::TEST_METADATA_POINTER_READS.load(SeqCst), 2);
            assert_eq!(super::TEST_METADATA_DOCUMENT_READS.load(SeqCst), 1);

            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            let denied =
                sqlx::query("SELECT metadata_location FROM iceberg_catalog.iceberg_tables")
                    .execute(&mut **conn.transaction())
                    .await
                    .expect_err("the request role cannot read the catalog pointer");
            assert!(
                denied.to_string().contains("permission denied"),
                "the request role is refused by privilege, not by absence: {denied}"
            );
        });
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
                    table.clone(),
                    vec![arrow::datatypes::Field::new(
                        "value",
                        arrow::datatypes::DataType::Int64,
                        true,
                    )],
                    None,
                    None,
                )
                .await
                .expect("dataset registers");
            for _ in 0..2 {
                assert_eq!(
                    catalog
                        .table_uid(&table, tenant)
                        .await
                        .expect("the registered table resolves"),
                    registered
                );
            }
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
                    .ensure_builtin(tenant, definition)
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

    /// Plans the current snapshot through a fresh permit-scoped table and
    /// returns how many file tasks it produced.
    ///
    /// # Panics
    /// Panics when the table cannot be reloaded, built, or planned.
    async fn planned_file_count(
        catalog: &BifrostCatalog,
        identifier: &iceberg::TableIdent,
        permit: &crate::oracle::reader_pins::ReaderIoPermit,
    ) -> usize {
        use futures_util::TryStreamExt as _;
        let loaded = catalog
            .iceberg_catalog()
            .load_table(identifier)
            .await
            .expect("physical table reloads");
        let tasks: Vec<_> = catalog
            .permit_scoped_table(
                identifier,
                loaded.metadata_ref(),
                loaded.metadata_location().map(ToOwned::to_owned),
                permit,
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
    /// permit-scoped table must still plan the committed file, which it can
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
            catalog
                .register_dataset(
                    tenant,
                    table.clone(),
                    vec![arrow::datatypes::Field::new(
                        "value",
                        arrow::datatypes::DataType::Int64,
                        true,
                    )],
                    None,
                    None,
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
            let applied =
                ApplyTransactionAction::apply(action, transaction).expect("append applies");
            applied
                .commit(catalog.iceberg_catalog().as_ref())
                .await
                .expect("fast append commits");

            let permit = crate::oracle::reader_pins::ReaderIoPermit::unfenced_for_test();
            let prepared = catalog
                .prepare_reader_identity(&table, tenant)
                .await
                .expect("the registered table prepares");
            let pinned = catalog
                .materialize_reader_cut(prepared, &permit)
                .await
                .expect("the committed snapshot pins");
            let snapshot_id = pinned
                .snapshot_id
                .expect("a committed table has a snapshot");
            delete_manifest_objects(warehouse.path());
            assert_eq!(
                planned_file_count(&catalog, &binding.table_ident(), &permit).await,
                1,
                "the pin's decoded manifests plan the file"
            );
            catalog
                .pinned_provider(&table, tenant, snapshot_id, &permit)
                .await
                .expect("the published snapshot resolves");
            assert!(
                catalog
                    .pinned_provider(&table, tenant, snapshot_id.wrapping_add(1), &permit)
                    .await
                    .is_err(),
                "an unpublished snapshot must fail to resolve rather than serve the current one"
            );
        });
    }

    /// Real production pinning resolves `wyrd_event_time` from the manifest's
    /// own writer schema and decodes each committed file's bounds.
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
    fn pinned_snapshot_decodes_manifest_event_time_by_writer_field_id() {
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
                    table.clone(),
                    vec![arrow::datatypes::Field::new(
                        "value",
                        arrow::datatypes::DataType::Int64,
                        true,
                    )],
                    None,
                    None,
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
            let field_id = physical
                .metadata()
                .current_schema()
                .field_by_name(WYRD_EVENT_TIME)
                .expect("the physical schema carries the event-time column")
                .id;

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

            let permit = crate::oracle::reader_pins::ReaderIoPermit::unfenced_for_test();
            let prepared = catalog
                .prepare_reader_identity(&table, tenant)
                .await
                .expect("the registered table prepares");
            let pinned = catalog
                .materialize_reader_cut(prepared, &permit)
                .await
                .expect("the committed snapshot pins");
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
