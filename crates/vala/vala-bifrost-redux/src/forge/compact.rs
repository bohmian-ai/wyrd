//! Forge compaction from staged Scribe files into Iceberg snapshots.
//!
//! This module owns the compaction state machine. It selects bounded,
//! tenant-scoped groups from `vala.file_list`, validates and sorts their Arrow
//! rows, writes one Iceberg data file, and records each durable transition in
//! `vala.forge_operation_state`. Prepared operation state makes a crash between
//! the SQL and Iceberg commits observable; the next Forge tick reconciles that
//! state before selecting more files.

use std::time::Duration;

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::{self, BoxStream};
use futures_util::{StreamExt as _, TryStreamExt as _};
use object_store::path::Path as ObjectPath;
use object_store::{ObjectMeta, ObjectStore, ObjectStoreExt as _};
use wyrd_spec::DataTenantId;
use wyrd_storage::BackendConfig;

use crate::storage::BifrostStorage;

use super::Forge;
use super::error::ForgeError;

const DEFAULT_MAX_CONCURRENT_READS: usize = 4;
const DEFAULT_MAX_OPEN_OPERATIONS_PER_TABLE: usize = 256;
const DEFAULT_MAX_RETAINED_SNAPSHOTS_PER_TABLE: usize = 256;
/// Default small-file threshold as a percentage of a table's file target.
///
/// At the 1 GiB default target this is 768 MiB: a 512 MiB staged file is
/// selected, while a merged output that reached the target is not.
const DEFAULT_SMALL_FILE_THRESHOLD_PERCENT: u8 = 75;
/// Deployment-wide rewrite file target for tables that declare none: 1 GiB.
///
/// Replaces Iceberg's 512 MiB writer default so every Bifrost table, built-in
/// or registered, compacts toward the same soft target unless its own
/// `write.target-file-size-bytes` property says otherwise.
pub const DEFAULT_TARGET_FILE_SIZE_BYTES: u64 = 1024 * 1024 * 1024;
/// Default cap on metadata records one maintenance pass visits. Retained from
/// the per-tick bound that previously governed every Forge pass, so a single
/// manifest rewrite or cleanup traversal stays bounded on a fragmented table.
const DEFAULT_MAX_MAINTENANCE_ITEMS_PER_TICK: usize = 1_024;
/// Default cap on declared metadata bytes one maintenance pass visits. Sized
/// for manifest and path bytes, not table data, and retained from the same
/// per-tick bound as [`DEFAULT_MAX_MAINTENANCE_ITEMS_PER_TICK`].
const DEFAULT_MAX_MAINTENANCE_BYTES_PER_TICK: u64 = 1024 * 1024 * 1024;
/// Default cap on listing pages walked by one orphan-GC candidate scan. Sized
/// well above an ordinary table's orphan-prefix page count so steady-state runs
/// complete in one pass, while still bounding a pathological prefix so one run
/// cannot walk unboundedly before yielding to a successor.
const DEFAULT_ORPHAN_GC_MAX_LIST_PAGES: usize = 1_024;
/// Default wall-clock budget for one orphan-GC run. Bounds the time a single run
/// spends listing and deleting before it yields cleanly as Partial, leaving the
/// remainder to a successor run that resumes from the committed-deletion
/// frontier.
const DEFAULT_ORPHAN_GC_RUN_BUDGET: Duration = Duration::from_mins(2);

#[derive(Debug, Clone, PartialEq, Eq)]
/// Limits and durability windows for one Forge maintenance loop.
///
/// A subset of these limits is operator-tunable through the server's `forge`
/// config section (see `wyrd-server`'s `ForgeRuntimeConfig`); the rest stay
/// internal. Whether supplied by an operator or left at the compiled default,
/// every instance passes through [`ForgeConfig::validate`] at Forge
/// construction, so the invariants below hold regardless of the source of the
/// numbers. `PartialEq`/`Eq` let boot-time resolution pin that an empty config
/// reproduces [`ForgeConfig::default`] exactly.
pub struct ForgeConfig {
    /// Lease duration used to fence one table's maintenance work.
    pub lease_ttl: Duration,
    /// Total time reserved for a retried Iceberg commit.
    pub iceberg_total_retry_timeout: Duration,
    /// Timeout for one catalog request.
    pub catalog_request_timeout: Duration,
    /// Time reserved for clock and lease uncertainty around a commit.
    pub uncertainty_margin: Duration,
    /// Minimum age before an uncertain prepared operation may be recovered.
    pub uncertainty_bound: Duration,
    /// Number of audit rows read per reconciliation page.
    pub audit_page_size: i64,
    /// Small-file threshold as a percentage of each table's resolved file
    /// target, in `1..=99`.
    ///
    /// A live file below this share of its table's target is a small-file
    /// candidate. Expressing it as a share rather than a byte count keeps the
    /// threshold below every table's own target, so an output that reached
    /// the target is never selected again.
    pub small_file_threshold_percent: u8,
    /// Soft rewrite file target for a table without its own
    /// `write.target-file-size-bytes` property.
    ///
    /// Resolved at planning time rather than written into table metadata, so
    /// changing the deployment default also moves every table that never
    /// declared an override.
    pub default_target_file_size_bytes: u64,
    /// Age after which an unreferenced object may be deleted.
    pub orphan_gc_ttl: Duration,
    /// Maximum orphan candidates considered in one GC batch.
    pub max_gc_candidates_per_batch: usize,
    /// Maximum manifests or metadata records one maintenance pass may visit.
    ///
    /// Bounds both the manifest-rewrite bin admitted by one task and the
    /// entries the catalog reads while rewriting it, so a fragmented table
    /// cannot turn a single maintenance pass into unbounded metadata work.
    pub max_maintenance_items_per_tick: usize,
    /// Maximum declared metadata bytes one maintenance pass may visit.
    ///
    /// Applies to the manifest-rewrite bin, the catalog's own rewrite
    /// traversal, and the post-expiry cleanup traversal, which all walk
    /// declared manifest and path bytes rather than table data.
    pub max_maintenance_bytes_per_tick: u64,
    /// Maximum concurrent staged-object reads during rewrite.
    pub max_concurrent_reads: usize,
    /// Maximum staging hints drained by one wake-up.
    pub max_hints_per_wake: usize,
    /// Maximum bounded open operations classified for one table and family.
    pub max_open_operations_per_table: usize,
    /// Maximum retained snapshots traversed by one reconciliation observation.
    pub max_retained_snapshots_per_table: usize,
    /// Maximum object-store listing pages an orphan-GC candidate scan consumes
    /// in one run. Bounds a single run's listing work on a table whose orphan
    /// prefix holds more pages than one run should walk; a run that hits the cap
    /// ends cleanly as Partial and a successor run resumes after the durable
    /// deletions this run committed. Operator-tunable via
    /// `forge.orphan_gc_max_list_pages`; defaults to the compiled value.
    pub orphan_gc_max_list_pages: usize,
    /// Wall-clock budget for one orphan-GC run, checked at listing-page and
    /// per-candidate-deletion boundaries. Exhausting the budget ends the run
    /// cleanly as Partial with no deletion attempted past the boundary; a
    /// successor run resumes from the durable frontier. Operator-tunable via
    /// `forge.orphan_gc_run_budget_secs`; defaults to the compiled value.
    pub orphan_gc_run_budget: Duration,
}

impl Default for ForgeConfig {
    /// Return the production defaults for Forge maintenance limits.
    fn default() -> Self {
        Self {
            lease_ttl: Duration::from_mins(15),
            iceberg_total_retry_timeout: Duration::from_mins(5),
            catalog_request_timeout: Duration::from_secs(30),
            uncertainty_margin: Duration::from_secs(30),
            uncertainty_bound: Duration::from_mins(2),
            audit_page_size: 256,
            small_file_threshold_percent: DEFAULT_SMALL_FILE_THRESHOLD_PERCENT,
            default_target_file_size_bytes: DEFAULT_TARGET_FILE_SIZE_BYTES,
            orphan_gc_ttl: Duration::from_hours(24),
            max_gc_candidates_per_batch: 256,
            max_maintenance_items_per_tick: DEFAULT_MAX_MAINTENANCE_ITEMS_PER_TICK,
            max_maintenance_bytes_per_tick: DEFAULT_MAX_MAINTENANCE_BYTES_PER_TICK,
            max_concurrent_reads: DEFAULT_MAX_CONCURRENT_READS,
            max_hints_per_wake: 256,
            max_open_operations_per_table: DEFAULT_MAX_OPEN_OPERATIONS_PER_TABLE,
            max_retained_snapshots_per_table: DEFAULT_MAX_RETAINED_SNAPSHOTS_PER_TABLE,
            orphan_gc_max_list_pages: DEFAULT_ORPHAN_GC_MAX_LIST_PAGES,
            orphan_gc_run_budget: DEFAULT_ORPHAN_GC_RUN_BUDGET,
        }
    }
}

impl ForgeConfig {
    /// Validate compaction, recovery, expiry, and garbage-collection limits.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when a limit is zero, a bin cannot
    /// contain two files, the small-file threshold percentage is not below 100,
    /// or the lease cannot cover the configured commit window.
    pub fn validate(&self) -> Result<(), ForgeError> {
        if self.lease_ttl.is_zero()
            || self.iceberg_total_retry_timeout.is_zero()
            || self.catalog_request_timeout.is_zero()
            || self.uncertainty_margin.is_zero()
            || self.uncertainty_bound.is_zero()
            || self.audit_page_size <= 0
            || self.small_file_threshold_percent == 0
            || self.default_target_file_size_bytes == 0
            || self.orphan_gc_ttl.is_zero()
            || self.max_gc_candidates_per_batch == 0
            || self.max_maintenance_items_per_tick == 0
            || self.max_maintenance_bytes_per_tick == 0
            || self.max_concurrent_reads == 0
            || self.max_hints_per_wake == 0
            || self.max_open_operations_per_table == 0
            || self.max_retained_snapshots_per_table == 0
            || self.orphan_gc_max_list_pages == 0
            || self.orphan_gc_run_budget.is_zero()
        {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge limits must be positive".to_owned(),
            });
        }
        if self.small_file_threshold_percent >= 100 {
            return Err(ForgeError::InvalidConfig {
                detail: "small_file_threshold_percent must be below 100".to_owned(),
            });
        }
        let required = self
            .iceberg_total_retry_timeout
            .saturating_add(self.catalog_request_timeout)
            .saturating_add(self.uncertainty_margin);
        if self.lease_ttl <= required {
            return Err(ForgeError::InvalidConfig {
                detail: "lease_ttl must exceed the Iceberg retry timeout, catalog timeout, and uncertainty margin".to_owned(),
            });
        }
        Ok(())
    }

    /// Return the time Forge must reserve for the Iceberg commit window.
    pub(crate) fn commit_window(&self) -> Duration {
        self.iceberg_total_retry_timeout
            .saturating_add(self.catalog_request_timeout)
            .saturating_add(self.uncertainty_margin)
    }
}

/// Number of listing entries one orphan-GC page holds.
///
/// Bounds how much of a listing materializes before orphan GC checks its page
/// cap and run budget at a page boundary. Sized so a steady-state prefix scans
/// in a handful of pages while a pathological prefix still yields promptly.
const FORGE_OBJECT_LIST_PAGE_ENTRIES: usize = 1_000;

/// A stream of object-listing pages under one prefix.
///
/// Each item is one bounded page of objects in ascending key order, or the
/// backend error that ended the walk. The stream owns its listing state, so it
/// is `'static` and does not borrow the store. Orphan GC pulls pages one at a
/// time so it can stop at a page boundary once its page cap or run budget is
/// reached without having materialized the whole prefix.
pub type ForgeObjectPages = BoxStream<'static, object_store::Result<Vec<ObjectMeta>>>;

/// The object-store operations Forge performs after Scribe has staged a file.
///
/// Production uses [`ForgeObjects`] over the node's Bifrost object store.
/// The trait exists so integration tests can wrap that same implementation
/// with deterministic barriers and failures.
#[async_trait]
pub trait ForgeObjectStore: std::fmt::Debug + Send + Sync {
    /// Read one staged or Iceberg-owned object.
    ///
    /// # Errors
    ///
    /// Returns the backend error when the object cannot be read completely.
    async fn read(&self, path: &str) -> object_store::Result<Bytes>;

    /// List objects below a table-owned prefix as ascending, bounded pages.
    ///
    /// `start_after` resumes the walk strictly after one already-processed
    /// key, which keeps a leading page of protected objects from starving the
    /// pages behind it. A backend error surfaces as a failed page.
    fn list_pages(&self, prefix: &str, start_after: Option<&str>) -> ForgeObjectPages;

    /// Read object metadata before a destructive decision.
    ///
    /// # Errors
    ///
    /// Returns the backend error, `NotFound` for an absent object.
    async fn stat(&self, path: &str) -> object_store::Result<ObjectMeta>;

    /// Delete one object after the final live-set and fence checks.
    ///
    /// # Errors
    ///
    /// Returns the backend error when deletion cannot complete.
    async fn delete(&self, path: &str) -> object_store::Result<()>;
}

/// Forge's production object operations over the node's Bifrost object store.
#[derive(Debug, Clone)]
pub struct ForgeObjects {
    /// The node's one Bifrost object store.
    objects: Arc<dyn ObjectStore>,
    /// Whether the backend lists keys in ascending order. Cloud stores and the
    /// in-memory store do; the local filesystem walks directories unordered.
    ordered_listing: bool,
}

impl ForgeObjects {
    /// Builds Forge's object operations over the node's storage owner.
    #[must_use]
    pub fn new(storage: &BifrostStorage) -> Self {
        Self {
            objects: Arc::clone(storage.objects()),
            ordered_listing: !matches!(storage.backend(), BackendConfig::Local { .. }),
        }
    }

    /// Page a prefix in ascending key order on a backend that lists unordered.
    ///
    /// Each page is one full walk that keeps only the
    /// [`FORGE_OBJECT_LIST_PAGE_ENTRIES`] smallest keys strictly after the
    /// cursor, so memory stays bounded by the page size whatever the walk
    /// order. The last key of a page becomes the next cursor; a short page
    /// ends the stream. Pages are produced lazily, so orphan GC stopping at its
    /// page cap stops the walks.
    // One full walk per page (O(pages × objects)); only the local
    // filesystem takes this path.
    fn unordered_pages(
        &self,
        prefix: ObjectPath,
        start_after: Option<ObjectPath>,
    ) -> ForgeObjectPages {
        let objects = Arc::clone(&self.objects);
        Box::pin(stream::try_unfold(Some(start_after), move |cursor| {
            let objects = Arc::clone(&objects);
            let prefix = prefix.clone();
            async move {
                let Some(cursor) = cursor else {
                    return Ok(None);
                };
                let mut smallest = BTreeMap::new();
                let mut walk = match &cursor {
                    Some(cursor) => objects.list_with_offset(Some(&prefix), cursor),
                    None => objects.list(Some(&prefix)),
                };
                while let Some(meta) = walk.try_next().await? {
                    smallest.insert(meta.location.clone(), meta);
                    if smallest.len() > FORGE_OBJECT_LIST_PAGE_ENTRIES {
                        smallest.pop_last();
                    }
                }
                let page = smallest.into_values().collect::<Vec<_>>();
                let Some(last) = page.last().map(|meta| meta.location.clone()) else {
                    return Ok(None);
                };
                let next = (page.len() == FORGE_OBJECT_LIST_PAGE_ENTRIES).then_some(Some(last));
                Ok(Some((page, next)))
            }
        }))
    }
}

#[async_trait]
impl ForgeObjectStore for ForgeObjects {
    /// Reads one whole object.
    ///
    /// # Errors
    /// Returns the backend error when the object cannot be read.
    async fn read(&self, path: &str) -> object_store::Result<Bytes> {
        self.objects
            .get(&ObjectPath::from(path))
            .await?
            .bytes()
            .await
    }

    /// Lists ascending pages, natively on an ordered backend.
    fn list_pages(&self, prefix: &str, start_after: Option<&str>) -> ForgeObjectPages {
        let prefix = ObjectPath::from(prefix);
        let start_after = start_after.map(ObjectPath::from);
        if !self.ordered_listing {
            return self.unordered_pages(prefix, start_after);
        }
        let walk = match &start_after {
            Some(cursor) => self.objects.list_with_offset(Some(&prefix), cursor),
            None => self.objects.list(Some(&prefix)),
        };
        Box::pin(
            walk.chunks(FORGE_OBJECT_LIST_PAGE_ENTRIES)
                .map(|chunk| chunk.into_iter().collect::<object_store::Result<Vec<_>>>()),
        )
    }

    /// Reads object metadata.
    ///
    /// # Errors
    /// Returns the backend error, `NotFound` for an absent object.
    async fn stat(&self, path: &str) -> object_store::Result<ObjectMeta> {
        self.objects.head(&ObjectPath::from(path)).await
    }

    /// Deletes one object.
    ///
    /// # Errors
    /// Returns the backend error when deletion cannot complete.
    async fn delete(&self, path: &str) -> object_store::Result<()> {
        self.objects.delete(&ObjectPath::from(path)).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// The tenant, table, and exact time partition that define one Forge group.
///
/// Retained from the erased grouping module because surviving live
/// reconciliation and transactional audit settlement address their durable
/// audit stream through this identity. Nothing here selects, packs, or rewrites
/// files.
pub(crate) struct ForgeGroupKey {
    /// Tenant that owns the staged files.
    pub(crate) tenant: DataTenantId,
    /// Registered Bifrost table being maintained.
    pub(crate) table_ref: crate::catalog::table_ref::TableRef,
    /// Exact time partition shared by every file in the group.
    pub(crate) partition: crate::catalog::layout::TimePartition,
}

impl ForgeGroupKey {
    /// Return the stable audit resource URI for one tenant/table pair.
    ///
    /// The resource identifies the table, never the partition, so table-scoped
    /// callers such as live reconciliation can address the same audit stream
    /// without inventing a placeholder partition.
    #[must_use]
    pub(crate) fn table_audit_resource(
        tenant: DataTenantId,
        table_ref: &crate::catalog::table_ref::TableRef,
    ) -> String {
        format!(
            "bifrost://{}/{}/{}",
            tenant, table_ref.namespace, table_ref.name
        )
    }

    /// Return the stable audit resource URI for this group.
    #[must_use]
    pub(crate) fn audit_resource(&self) -> String {
        Self::table_audit_resource(self.tenant, &self.table_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ForgeTableKey {
    /// Tenant whose physical table is being maintained.
    pub(crate) tenant: DataTenantId,
    /// Logical table identity resolved at the storage boundary.
    pub(crate) table_ref: crate::catalog::TableRef,
}

impl Forge {
    /// Loads one Iceberg table under Forge's bounded catalog timeout.
    ///
    /// # Errors
    ///
    /// Returns timeout or catalog errors; cancellation leaves durable state
    /// unchanged for the next reconciliation tick.
    pub(super) async fn load_table(
        &self,
        ident: &iceberg::TableIdent,
    ) -> Result<iceberg::table::Table, ForgeError> {
        tokio::time::timeout(
            self.core.config.catalog_request_timeout,
            self.core.catalog.load_table(ident),
        )
        .await
        .map_err(|_| ForgeError::Timeout {
            operation: "catalog table load",
        })?
        .map_err(ForgeError::Catalog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes `count` objects under `prefix` in reverse key order.
    ///
    /// # Panics
    ///
    /// Panics when the fixture store refuses a write.
    async fn seed_reversed(
        objects: &Arc<dyn ObjectStore>,
        prefix: &str,
        count: usize,
    ) -> Vec<String> {
        let mut keys = (0..count)
            .map(|index| format!("{prefix}{index:06}.parquet"))
            .collect::<Vec<_>>();
        for key in keys.iter().rev() {
            objects
                .put(
                    &ObjectPath::from(key.as_str()),
                    Bytes::from_static(b"orphan").into(),
                )
                .await
                .expect("object is written");
        }
        keys.sort();
        keys
    }

    /// Both listing paths page a prefix in bounded, ascending, exclusive pages.
    ///
    /// The local filesystem lists unordered and takes the emulated path; the
    /// in-memory store lists in key order and takes the native path. Objects
    /// are written in reverse so walk order and key order disagree, and a
    /// resume from a mid-page cursor must yield exactly the keys after it.
    ///
    /// # Panics
    ///
    /// Panics when any page is oversized, out of order, or skips or repeats a key.
    #[tokio::test]
    async fn forge_listing_pages_are_bounded_ascending_and_resumable() {
        let root = tempfile::tempdir().expect("listing root");
        let local = ForgeObjects::new(&BifrostStorage::for_test(root.path(), false));
        let memory = ForgeObjects {
            objects: Arc::new(object_store::memory::InMemory::new()),
            ordered_listing: true,
        };
        let prefix = "tenants/t/table/data/forge/v1/";
        for store in [local, memory] {
            let keys = seed_reversed(
                &store.objects,
                prefix,
                FORGE_OBJECT_LIST_PAGE_ENTRIES * 2 + 3,
            )
            .await;
            for (cursor, expected) in [(None, &keys[..]), (Some(keys[700].as_str()), &keys[701..])]
            {
                let pages = store
                    .list_pages(prefix, cursor)
                    .try_collect::<Vec<_>>()
                    .await
                    .expect("every page lists");
                assert!(
                    pages
                        .iter()
                        .all(|page| page.len() <= FORGE_OBJECT_LIST_PAGE_ENTRIES),
                    "every page stays within the page bound"
                );
                let listed = pages
                    .iter()
                    .flatten()
                    .map(|meta| meta.location.to_string())
                    .collect::<Vec<_>>();
                assert_eq!(
                    listed, expected,
                    "pages are ascending, complete, and exclusive"
                );
            }
        }
    }

    /// Forge output encoding uses the shared Bifrost Parquet recipe: the
    /// parquet-rs row cap and the soft 128 MiB encoded row-group target.
    #[test]
    fn compacted_output_uses_shared_writer_properties() {
        let properties = crate::parquet::writer_properties::bifrost_writer_properties(&[]);
        assert_eq!(
            properties.max_row_group_row_count(),
            Some(parquet::file::properties::DEFAULT_MAX_ROW_GROUP_ROW_COUNT)
        );
        assert_eq!(properties.max_row_group_bytes(), Some(128 * 1024 * 1024));
    }

    /// Both reconciliation caps default to 256 and reject zero independently.
    #[test]
    fn forge_reconciliation_default_caps_are_256_and_zero_is_invalid() {
        let config = ForgeConfig::default();
        assert_eq!(config.max_open_operations_per_table, 256);
        assert_eq!(config.max_retained_snapshots_per_table, 256);
        let mut invalid_open = config.clone();
        invalid_open.max_open_operations_per_table = 0;
        assert!(invalid_open.validate().is_err());
        let mut invalid_snapshots = config;
        invalid_snapshots.max_retained_snapshots_per_table = 0;
        assert!(invalid_snapshots.validate().is_err());
    }

    /// Both orphan-GC scan bounds default to positive values and reject zero.
    #[test]
    fn forge_config_rejects_zero_orphan_gc_bounds() {
        let config = ForgeConfig::default();
        assert_eq!(
            config.orphan_gc_max_list_pages,
            DEFAULT_ORPHAN_GC_MAX_LIST_PAGES
        );
        assert_eq!(config.orphan_gc_run_budget, DEFAULT_ORPHAN_GC_RUN_BUDGET);
        assert!(config.validate().is_ok());
        let zero_pages = ForgeConfig {
            orphan_gc_max_list_pages: 0,
            ..ForgeConfig::default()
        };
        assert!(zero_pages.validate().is_err());
        let zero_budget = ForgeConfig {
            orphan_gc_run_budget: Duration::ZERO,
            ..ForgeConfig::default()
        };
        assert!(zero_budget.validate().is_err());
    }

    /// The small-file threshold percentage defaults to 75 and stays in `1..=99`.
    ///
    /// Zero would select nothing and 100 or more would select a file that
    /// already reached its table's target, so both bounds are refused.
    ///
    /// # Panics
    /// Panics when the default or a boundary percentage is classified wrongly.
    #[test]
    fn forge_config_bounds_the_small_file_threshold_percent() {
        assert_eq!(ForgeConfig::default().small_file_threshold_percent, 75);
        for (percent, valid) in [(0, false), (1, true), (99, true), (100, false)] {
            let config = ForgeConfig {
                small_file_threshold_percent: percent,
                ..ForgeConfig::default()
            };
            assert_eq!(config.validate().is_ok(), valid, "percent {percent}");
        }
    }

    /// The history readers may continue to call the audit query module until
    /// state-backed reconciliation replaces them, but none of the four
    /// transition modules may append an audit event directly.
    #[test]
    fn forge_transition_writers_do_not_append_audit_directly() {
        let modules = [
            ("compact.rs", include_str!("compact.rs")),
            ("live_replace.rs", include_str!("live_replace.rs")),
            ("expire.rs", include_str!("expire.rs")),
            ("orphan_gc.rs", include_str!("orphan_gc.rs")),
        ];
        let forbidden_call = ["audit_staging::append_", "audit"].concat();

        for (module, source) in modules {
            assert!(
                !source.contains(&forbidden_call),
                "{module} bypasses ForgeOperations"
            );
        }
    }
}
