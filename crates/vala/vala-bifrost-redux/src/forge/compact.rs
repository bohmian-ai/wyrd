//! Forge compaction from staged Scribe files into Iceberg snapshots.
//!
//! This module owns the compaction state machine. It selects bounded,
//! tenant-scoped groups from `vala.file_list`, validates and sorts their Arrow
//! rows, writes one Iceberg data file, and records each durable transition in
//! the audit outbox. Prepared audit records make a crash between the SQL and
//! Iceberg commits observable; the next Forge tick reconciles that state before
//! selecting more files.

use std::time::Duration;

use async_trait::async_trait;
use futures_util::future::ready;
use futures_util::stream::{self, BoxStream};
use opendal::{Buffer, Entry, Metadata};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod};

use super::Forge;
use super::error::ForgeError;

const DEFAULT_MAX_CONCURRENT_READS: usize = 4;
const DEFAULT_MAX_OPEN_OPERATIONS_PER_TABLE: usize = 256;
const DEFAULT_MAX_RETAINED_SNAPSHOTS_PER_TABLE: usize = 256;
/// Small-file candidacy threshold.
pub(crate) const DEFAULT_SMALL_FILE_THRESHOLD_BYTES: u64 = 64 * 1024 * 1024;
/// Default commit count past `retain_last` that makes snapshot expiry due on
/// its own. Chosen well above ordinary per-tick compaction commit counts so a
/// table under steady ingest still accrues history before maintenance fires,
/// but far below `max_retained_snapshots_per_table` so history never wedges.
const DEFAULT_MAINTENANCE_TRIGGER_SNAPSHOT_COUNT: usize = 32;
/// Default oldest-snapshot age that makes snapshot expiry due when at least one
/// commit exists past `retain_last`. Bounds retained-history age for a
/// low-commit table that never reaches the count trigger.
const DEFAULT_MAINTENANCE_TRIGGER_INTERVAL: Duration = Duration::from_hours(1);
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
const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

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
    /// Age after which old Iceberg snapshots become eligible for expiry.
    pub snapshot_retention: Duration,
    /// Number of snapshots retained along each current/ref ancestry.
    pub retain_last: usize,
    /// Whether periodic snapshot expiry is enabled for this Forge owner.
    pub snapshot_expiry_enabled: bool,
    /// Whether periodic fragmented-manifest rewrite is enabled.
    /// Independent small-file candidacy threshold used by live planning.
    pub small_file_threshold_bytes: u64,
    /// Maximum bytes packed into one selected manifest rewrite bin.
    /// Minimum count required for the newest under-filled manifest bin.
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
    /// Count of accumulated commits past `retain_last` that makes snapshot
    /// expiry due on its own, independent of compaction backlog. Evaluated
    /// every planning tick so maintenance can never be starved by compaction
    /// load; see [`super::planning_scheduler`].
    pub maintenance_trigger_snapshot_count: usize,
    /// Age of the oldest retained snapshot past which snapshot expiry becomes
    /// due, provided at least one commit exists past `retain_last`. Paired with
    /// `maintenance_trigger_snapshot_count` as a count-OR-interval trigger.
    pub maintenance_trigger_interval: Duration,
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
            snapshot_retention: Duration::from_hours(24),
            retain_last: 1,
            snapshot_expiry_enabled: false,
            small_file_threshold_bytes: DEFAULT_SMALL_FILE_THRESHOLD_BYTES,
            orphan_gc_ttl: Duration::from_hours(24),
            max_gc_candidates_per_batch: 256,
            max_maintenance_items_per_tick: DEFAULT_MAX_MAINTENANCE_ITEMS_PER_TICK,
            max_maintenance_bytes_per_tick: DEFAULT_MAX_MAINTENANCE_BYTES_PER_TICK,
            max_concurrent_reads: DEFAULT_MAX_CONCURRENT_READS,
            max_hints_per_wake: 256,
            max_open_operations_per_table: DEFAULT_MAX_OPEN_OPERATIONS_PER_TABLE,
            max_retained_snapshots_per_table: DEFAULT_MAX_RETAINED_SNAPSHOTS_PER_TABLE,
            maintenance_trigger_snapshot_count: DEFAULT_MAINTENANCE_TRIGGER_SNAPSHOT_COUNT,
            maintenance_trigger_interval: DEFAULT_MAINTENANCE_TRIGGER_INTERVAL,
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
    /// contain two files, the lease cannot cover the configured commit window,
    /// or `retain_last` exceeds the retained-snapshot traversal cap
    /// (`max_retained_snapshots_per_table`), which would make reconciliation
    /// unable to see every snapshot the expiry policy is asked to retain.
    pub fn validate(&self) -> Result<(), ForgeError> {
        if self.lease_ttl.is_zero()
            || self.iceberg_total_retry_timeout.is_zero()
            || self.catalog_request_timeout.is_zero()
            || self.uncertainty_margin.is_zero()
            || self.uncertainty_bound.is_zero()
            || self.audit_page_size <= 0
            || self.snapshot_retention.is_zero()
            || self.retain_last == 0
            || self.small_file_threshold_bytes == 0
            || self.orphan_gc_ttl.is_zero()
            || self.max_gc_candidates_per_batch == 0
            || self.max_maintenance_items_per_tick == 0
            || self.max_maintenance_bytes_per_tick == 0
            || self.max_concurrent_reads == 0
            || self.max_hints_per_wake == 0
            || self.max_open_operations_per_table == 0
            || self.max_retained_snapshots_per_table == 0
            || self.maintenance_trigger_snapshot_count == 0
            || self.maintenance_trigger_interval.is_zero()
            || self.orphan_gc_max_list_pages == 0
            || self.orphan_gc_run_budget.is_zero()
        {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge limits must be positive".to_owned(),
            });
        }
        if self.retain_last > self.max_retained_snapshots_per_table {
            return Err(ForgeError::InvalidConfig {
                detail: "retain_last must not exceed max_retained_snapshots_per_table".to_owned(),
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

/// A stream of object-listing pages under one prefix.
///
/// Each item is one bounded page of entries or the backend error that ended
/// the walk. The stream owns its listing state, so it is `'static` and does not
/// borrow the object store that produced it. Orphan GC pulls pages one at a
/// time so it can stop at a page boundary once its page cap or run budget is
/// reached without having materialized the whole prefix.
pub type ForgeObjectPages = BoxStream<'static, opendal::Result<Vec<Entry>>>;

/// The object-store operations Forge performs after Scribe has staged a file.
///
/// Keeping this capability narrow lets production use the real `OpenDAL`
/// operator while integration tests wrap the same seam with deterministic
/// barriers and failures. Fixture code still uses [`ForgeCore::staging`]
/// directly for producer writes.
#[async_trait]
pub trait ForgeObjectStore: std::fmt::Debug + Send + Sync {
    /// Opens the bounded streaming writer used for one rewritten output.
    ///
    /// # Errors
    ///
    /// Returns the backend error when multipart or streaming upload cannot start.
    async fn output_writer(
        &self,
        operator: &opendal::Operator,
        path: &str,
        chunk_bytes: usize,
    ) -> opendal::Result<opendal::Writer> {
        operator.writer_with(path).chunk(chunk_bytes).await
    }

    /// Writes one already-bounded chunk to an open rewrite output.
    ///
    /// This narrow continuation of [`Self::output_writer`] lets integration
    /// stores observe and fail individual chunks without replacing production
    /// upload behavior.
    ///
    /// # Errors
    ///
    /// Returns the backend error when the chunk cannot be accepted.
    async fn write_output_chunk(
        &self,
        writer: &mut opendal::Writer,
        chunk: bytes::Bytes,
    ) -> opendal::Result<()> {
        writer.write(chunk).await
    }

    /// Aborts an incomplete rewrite output after a chunk failure.
    ///
    /// # Errors
    ///
    /// Returns the backend error when multipart cleanup cannot be confirmed.
    async fn abort_output_writer(&self, writer: &mut opendal::Writer) -> opendal::Result<()> {
        writer.abort().await
    }

    /// Read one staged or Iceberg-owned object.
    ///
    /// # Errors
    ///
    /// Returns the backend error when the object cannot be read completely.
    async fn read(&self, path: &str) -> opendal::Result<Buffer>;

    /// Read one bounded byte range from a staged object.
    ///
    /// Implementations must issue a native ranged request. Falling back to a
    /// whole-object read would defeat Parquet footer/row-group admission and
    /// can exhaust the rewrite memory budget before `DataFusion` sees a batch.
    ///
    /// # Errors
    ///
    /// Returns the backend read error when the object cannot be fetched.
    async fn read_range(&self, path: &str, range: std::ops::Range<u64>) -> opendal::Result<Buffer>;

    /// List all objects below a table-owned prefix.
    ///
    /// # Errors
    ///
    /// Returns the backend error when recursive listing cannot complete.
    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>>;

    /// List objects below a table-owned prefix as a stream of bounded pages.
    ///
    /// This is the listing path orphan GC uses so it can consume the prefix
    /// incrementally and stop at a page boundary once its per-run page cap or
    /// time budget is reached. `start_after` resumes the walk strictly after
    /// one already-processed key in lexicographic order, which is what keeps a
    /// leading page of protected objects from starving the pages behind it.
    ///
    /// The default body adapts [`Self::list`] into a single page and applies
    /// the same exclusive ordering itself, which preserves behavior for every
    /// non-production implementation. The production adapter overrides this
    /// with true incremental pagination and the backend's own cursor so a large
    /// prefix never materializes at once; riding this default in production
    /// would defeat the scan bound.
    ///
    /// # Errors
    ///
    /// Returns the backend error when recursive listing cannot begin. Errors
    /// encountered mid-walk surface as a failed item in the returned stream.
    async fn list_pages(
        &self,
        prefix: &str,
        start_after: Option<&str>,
    ) -> opendal::Result<ForgeObjectPages> {
        let mut entries = self.list(prefix).await?;
        entries.retain(|entry| entry.metadata().is_file());
        if let Some(cursor) = start_after {
            entries.retain(|entry| entry.path() > cursor);
        }
        entries.sort_unstable_by(|left, right| left.path().cmp(right.path()));
        Ok(Box::pin(stream::once(ready(Ok(entries)))))
    }

    /// Read object metadata before a destructive decision.
    ///
    /// # Errors
    ///
    /// Returns the backend error when metadata cannot be read.
    async fn stat(&self, path: &str) -> opendal::Result<Metadata>;

    /// Delete one object after the final live-set and fence checks.
    ///
    /// # Errors
    ///
    /// Returns the backend error when deletion cannot complete.
    async fn delete(&self, path: &str) -> opendal::Result<()>;
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

/// Construct system-owned metadata for one Forge transition event.
///
/// Family and phase validation remain owned by [`ForgeOperations`]; this pure
/// helper only preserves the established event envelope.
pub(super) fn forge_transition_event(
    operation: &str,
    resource: String,
    detail: AuditDetail,
) -> AuditEvent {
    AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: operation.to_owned(),
        resource,
        card_ref: None,
        principal_id: SYSTEM_PRINCIPAL,
        principal_kind: PrincipalKindTag::Service,
        auth_method: AuthMethod::Internal,
        permission: "bifrost:forge".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: operation.to_owned(),
        detail: Some(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Object store whose `list` delegates to a real filesystem operator.
    ///
    /// Backs the default `list_pages` adaptation test with genuine
    /// [`opendal::Entry`] values, which cannot be constructed directly, while
    /// leaving `list_pages` unimplemented so the trait default is exercised.
    #[derive(Debug)]
    struct FsListObjectStore {
        /// Filesystem operator rooted at a temporary directory.
        operator: opendal::Operator,
    }

    #[async_trait]
    impl ForgeObjectStore for FsListObjectStore {
        /// Reject reads because this unit exercises only listing adaptation.
        async fn read(&self, _path: &str) -> opendal::Result<Buffer> {
            unreachable!("listing unit does not read objects")
        }

        /// Reject ranged reads because this unit exercises only listing adaptation.
        async fn read_range(
            &self,
            _path: &str,
            _range: std::ops::Range<u64>,
        ) -> opendal::Result<Buffer> {
            unreachable!("listing unit does not read object ranges")
        }

        /// Delegate the flat recursive listing to the backing filesystem operator.
        async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>> {
            self.operator.list_with(prefix).recursive(true).await
        }

        /// Reject metadata reads because this unit exercises only listing adaptation.
        async fn stat(&self, _path: &str) -> opendal::Result<Metadata> {
            unreachable!("listing unit does not inspect objects")
        }

        /// Reject deletes because this unit exercises only listing adaptation.
        async fn delete(&self, _path: &str) -> opendal::Result<()> {
            unreachable!("listing unit does not delete objects")
        }
    }

    /// The default `list_pages` body adapts `list` into exactly one page.
    #[tokio::test]
    async fn list_pages_default_yields_single_page() {
        use futures_util::StreamExt;

        let dir = tempfile::tempdir().expect("temporary listing root");
        let operator = opendal::Operator::new(
            opendal::services::Fs::default().root(dir.path().to_str().expect("root path")),
        )
        .expect("filesystem operator")
        .finish();
        for name in ["a.parquet", "b.parquet", "c.parquet"] {
            operator
                .write(name, vec![0_u8])
                .await
                .expect("seed listing object");
        }
        let store = FsListObjectStore { operator };

        // A flat listing also names the directories it walked; a page carries
        // addressable objects only, because a resumable scan advances its
        // cursor over object keys.
        let mut direct = store
            .list("")
            .await
            .expect("direct listing")
            .into_iter()
            .filter(|entry| entry.metadata().is_file())
            .map(|entry| entry.path().to_owned())
            .collect::<Vec<_>>();
        direct.sort();

        let mut pages = store.list_pages("", None).await.expect("paged listing");
        let first = pages
            .next()
            .await
            .expect("default body yields one page")
            .expect("page listing succeeds");
        assert!(
            pages.next().await.is_none(),
            "the default adaptation yields exactly one page"
        );
        let mut page_paths = first
            .into_iter()
            .map(|entry| entry.path().to_owned())
            .collect::<Vec<_>>();
        page_paths.sort();

        assert_eq!(
            page_paths, direct,
            "the single page equals every object of the flat listing"
        );
    }

    /// Forge output encoding uses the shared Bifrost Parquet recipe.
    #[test]
    fn compacted_output_uses_shared_writer_properties() {
        assert_eq!(
            crate::parquet::writer_properties::bifrost_writer_properties(10, &[])
                .max_row_group_row_count(),
            Some(131_072)
        );
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

    /// `retain_last` may not exceed the retained-snapshot traversal cap.
    ///
    /// The default (`retain_last` 1, cap 256) is well within bound, and equal
    /// values are accepted; only a `retain_last` above the traversal cap fails,
    /// because reconciliation could then never observe every retained snapshot.
    #[test]
    fn forge_config_rejects_retain_last_above_traversal_cap() {
        let at_bound = ForgeConfig {
            retain_last: 4,
            max_retained_snapshots_per_table: 4,
            ..ForgeConfig::default()
        };
        assert!(at_bound.validate().is_ok());
        let over_bound = ForgeConfig {
            retain_last: 5,
            max_retained_snapshots_per_table: 4,
            ..ForgeConfig::default()
        };
        assert!(over_bound.validate().is_err());
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
        let forbidden_call = ["audit_outbox::append_", "audit"].concat();

        for (module, source) in modules {
            assert!(
                !source.contains(&forbidden_call),
                "{module} bypasses ForgeOperations"
            );
        }
    }
}
