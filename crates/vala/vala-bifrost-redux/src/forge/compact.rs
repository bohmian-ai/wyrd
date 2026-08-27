//! Forge compaction from staged Scribe files into Iceberg snapshots.
//!
//! This module owns the compaction state machine. It selects bounded,
//! tenant-scoped groups from `vala.file_list`, validates and sorts their Arrow
//! rows, writes one Iceberg data file, and records each durable transition in
//! the audit outbox. Prepared audit records make a crash between the SQL and
//! Iceberg commits observable; the next Forge tick reconciles that state before
//! selecting more files.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Array, StringArray};
use arrow::compute::cast;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use futures_util::future::ready;
use futures_util::stream::{self, BoxStream};
use iceberg::spec::DataFile;
use iceberg::table::Table;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use opendal::{Buffer, Entry, Metadata};
use sqlx::{Row, postgres::PgRow};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
use vala_sql::row_types::forge_tasks::ForgeTaskStrategy;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeCompactionPhase,
    StoragePath,
};

use super::Forge;
use super::binpack::{CandidateFile, ForgeGroupKey, RewriteBin};
use super::error::ForgeError;
use super::lease::ForgeLease;
use super::metrics::{ForgeCatalogCommitStrategy, ForgeTaskMetricStrategy, ForgeTelemetry};
use super::planner::ForgePlanCandidate;
use super::rewrite::{ForgeAttemptGeneration, RewriteOutput, RewriteRequest, RewriteSourceFile};
use super::right_size::{
    ForgeRightSizePolicy, IcebergCandidateFile, IcebergRewriteGroup, IcebergRewriteReason,
    validate_supported_layout,
};
use crate::catalog::TenantTableBinding;
use crate::catalog::layout::TimePartition;

const DEFAULT_MAX_CONCURRENT_READS: usize = 4;
const DEFAULT_SPILL_LIMIT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DEFAULT_MAX_OPEN_OPERATIONS_PER_TABLE: usize = 256;
const DEFAULT_MAX_RETAINED_SNAPSHOTS_PER_TABLE: usize = 256;
/// Small-file candidacy threshold.
pub(crate) const DEFAULT_SMALL_FILE_THRESHOLD_BYTES: u64 = 64 * 1024 * 1024;
/// Target size for one manifest rewrite bin.
pub(crate) const DEFAULT_MANIFEST_REWRITE_TARGET_SIZE_BYTES: u64 = 8 * 1024 * 1024;
/// Minimum count for an under-filled manifest bin.
pub(crate) const DEFAULT_MANIFEST_REWRITE_MIN_COUNT: usize = 100;
/// Default commit count past `retain_last` that makes snapshot expiry due on
/// its own. Chosen well above ordinary per-tick compaction commit counts so a
/// table under steady ingest still accrues history before maintenance fires,
/// but far below `max_retained_snapshots_per_table` so history never wedges.
const DEFAULT_MAINTENANCE_TRIGGER_SNAPSHOT_COUNT: usize = 32;
/// Default oldest-snapshot age that makes snapshot expiry due when at least one
/// commit exists past `retain_last`. Bounds retained-history age for a
/// low-commit table that never reaches the count trigger.
const DEFAULT_MAINTENANCE_TRIGGER_INTERVAL: Duration = Duration::from_hours(1);
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
    /// Minimum number of staged files that makes a group eligible.
    pub min_files: i64,
    /// Maximum number of files in one rewrite bin.
    pub max_files_per_bin: usize,
    /// Maximum number of input files processed by one tick.
    pub max_files_per_tick: usize,
    /// Maximum input bytes processed by one tick.
    pub max_bytes_per_tick: u64,
    /// Maximum peak memory estimate admitted for either Forge execution lane.
    pub max_memory_bytes: u64,
    /// Hard ceiling for one oversized singleton admitted outside the ordinary byte lane.
    pub max_large_task_bytes: u64,
    /// Maximum bins committed by one tick.
    pub max_bins_per_tick: usize,
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
    pub manifest_rewrite_enabled: bool,
    /// Independent small-file candidacy threshold used by live planning.
    pub small_file_threshold_bytes: u64,
    /// Maximum bytes packed into one selected manifest rewrite bin.
    pub manifest_rewrite_target_size_bytes: u64,
    /// Minimum count required for the newest under-filled manifest bin.
    pub manifest_rewrite_min_count: usize,
    /// Age after which an unreferenced object may be deleted.
    pub orphan_gc_ttl: Duration,
    /// Maximum orphan candidates considered in one GC batch.
    pub max_gc_candidates_per_batch: usize,
    /// Maximum concurrent staged-object reads during rewrite.
    pub max_concurrent_reads: usize,
    /// `DataFusion` spill ceiling for Forge rewrites.
    pub spill_limit_bytes: u64,
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
            min_files: 2,
            max_files_per_bin: 256,
            max_files_per_tick: 1_024,
            max_bytes_per_tick: 2 * 512 * 1024 * 1024,
            max_memory_bytes: 4 * 512 * 1024 * 1024,
            max_large_task_bytes: 4 * 512 * 1024 * 1024,
            max_bins_per_tick: 64,
            lease_ttl: Duration::from_mins(15),
            iceberg_total_retry_timeout: Duration::from_mins(5),
            catalog_request_timeout: Duration::from_secs(30),
            uncertainty_margin: Duration::from_secs(30),
            uncertainty_bound: Duration::from_mins(2),
            audit_page_size: 256,
            snapshot_retention: Duration::from_hours(24),
            retain_last: 1,
            snapshot_expiry_enabled: true,
            manifest_rewrite_enabled: false,
            small_file_threshold_bytes: DEFAULT_SMALL_FILE_THRESHOLD_BYTES,
            manifest_rewrite_target_size_bytes: DEFAULT_MANIFEST_REWRITE_TARGET_SIZE_BYTES,
            manifest_rewrite_min_count: DEFAULT_MANIFEST_REWRITE_MIN_COUNT,
            orphan_gc_ttl: Duration::from_hours(24),
            max_gc_candidates_per_batch: 256,
            max_concurrent_reads: DEFAULT_MAX_CONCURRENT_READS,
            spill_limit_bytes: DEFAULT_SPILL_LIMIT_BYTES,
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
        if self.min_files < 2
            || self.max_files_per_bin < 2
            || self.max_files_per_tick == 0
            || self.max_bytes_per_tick == 0
            || self.max_memory_bytes == 0
            || self.max_large_task_bytes == 0
            || self.max_bins_per_tick == 0
            || self.lease_ttl.is_zero()
            || self.iceberg_total_retry_timeout.is_zero()
            || self.catalog_request_timeout.is_zero()
            || self.uncertainty_margin.is_zero()
            || self.uncertainty_bound.is_zero()
            || self.audit_page_size <= 0
            || self.snapshot_retention.is_zero()
            || self.retain_last == 0
            || self.small_file_threshold_bytes == 0
            || self.manifest_rewrite_target_size_bytes == 0
            || self.manifest_rewrite_min_count == 0
            || self.orphan_gc_ttl.is_zero()
            || self.max_gc_candidates_per_batch == 0
            || self.max_concurrent_reads == 0
            || self.spill_limit_bytes == 0
            || self.max_hints_per_wake == 0
            || self.max_open_operations_per_table == 0
            || self.max_retained_snapshots_per_table == 0
            || self.maintenance_trigger_snapshot_count == 0
            || self.maintenance_trigger_interval.is_zero()
            || self.orphan_gc_max_list_pages == 0
            || self.orphan_gc_run_budget.is_zero()
        {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge limits must be positive and min_files/max_files_per_bin must be at least two".to_owned(),
            });
        }
        if self.max_files_per_bin > self.max_files_per_tick
            || self.max_large_task_bytes < self.max_bytes_per_tick
        {
            return Err(ForgeError::InvalidConfig {
                detail: "max_files_per_bin must not exceed max_files_per_tick and max_large_task_bytes must cover the ordinary byte limit".to_owned(),
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

    /// Pause or fail a rewrite immediately before its lease fence is checked.
    ///
    /// Production implementations return successfully. Test implementations
    /// use this seam to deterministically interleave lease takeover with an
    /// output PUT while preserving the production write path.
    ///
    /// # Errors
    ///
    /// Returns an injected object-store error when a test deliberately rejects
    /// the output boundary.
    async fn before_output_put(&self, _path: &str) -> opendal::Result<()> {
        Ok(())
    }

    /// Notifies test-support wrappers after one rewrite output is durable.
    ///
    /// Production implementations use this infallible default no-op.
    /// Implementations may delay return for deterministic tests but cannot
    /// reject the already successful write or mutate its ownership.
    async fn after_output_put(&self, _path: &str) {}

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
    /// time budget is reached. The default body adapts [`Self::list`] into a
    /// single page, which preserves behavior for every non-production
    /// implementation. The production adapter overrides this with true
    /// incremental pagination so a large prefix never materializes at once;
    /// riding this default in production would defeat the scan bound.
    ///
    /// # Errors
    ///
    /// Returns the backend error when recursive listing cannot begin. Errors
    /// encountered mid-walk surface as a failed item in the returned stream.
    async fn list_pages(&self, prefix: &str) -> opendal::Result<ForgeObjectPages> {
        let entries = self.list(prefix).await?;
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
pub(crate) struct ForgeTableKey {
    /// Tenant whose physical table is being maintained.
    pub(crate) tenant: DataTenantId,
    /// Logical table identity resolved at the storage boundary.
    pub(crate) table_ref: crate::catalog::TableRef,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
/// Counters collected from one Forge maintenance tick.
pub struct ForgeTickOutcome {
    /// Candidate groups discovered for compaction.
    pub groups_seen: usize,
    /// Rewrite bins committed to Iceberg.
    pub bins_committed: usize,
    /// Bins skipped because a budget or lease window was exhausted.
    pub bins_skipped: usize,
    /// Durable operations recovered from audit state.
    pub reconciled: usize,
    /// Registrations discovered by the periodic catalog roster.
    pub tables_discovered: usize,
    /// Registrations that reached a terminal local classification.
    pub tables_examined: usize,
    /// Tables whose stages completed successfully.
    pub tables_succeeded: usize,
    /// Tables skipped due to lease contention or cancellation.
    pub tables_skipped: usize,
    /// Tables that encountered a failure.
    pub tables_failed: usize,
    /// Compaction operations recovered during reconciliation.
    pub reconciliation_recovered: usize,
    /// Snapshot-expiry operations recovered or completed.
    pub expiry_reconciled: usize,
    /// Orphan-GC operations recovered.
    pub gc_reconciled: usize,
    /// Objects considered by orphan GC.
    pub gc_candidates: usize,
    /// Objects deleted by orphan GC.
    pub gc_deleted: usize,
    /// Objects skipped after a live-set or fence recheck.
    pub gc_skipped: usize,
    /// Work skipped because a per-tick budget was reached.
    pub budget_skips: usize,
    /// Tables skipped because another Forge owner held the lease.
    pub lease_contention: usize,
    /// Leases obtained by replacing expired owners.
    pub lease_takeovers: usize,
    /// Operations stopped after losing the table fence.
    pub fence_losses: usize,
    /// Stage-level failures that did not abort discovery of other tables.
    pub stage_failures: usize,
    /// Peak spill bytes observed across successful rewrites in this tick.
    pub spill_bytes: u64,
    /// Rows accepted from staged inputs.
    pub input_rows: u64,
    /// Rows encoded into committed outputs.
    pub output_rows: u64,
    /// Number of rotated output files committed to Iceberg.
    pub outputs_committed: usize,
    /// Prepared live replacements proven committed by fresh manifest evidence.
    pub live_recovered: usize,
    /// Prepared live replacements proven abandoned and safely reset.
    pub live_reset: usize,
    /// Prepared live replacements still inside the uncertainty window.
    pub live_pending: usize,
    /// Prepared live replacements lacking terminal proof.
    pub live_unresolved: usize,
    /// Open-operation pages that exceeded their configured cap.
    pub open_operation_overflows: usize,
    /// Staging files that remain for a later fold.
    pub staging_pending_files: usize,
    /// Current-snapshot live files seen by right-size planning.
    pub live_candidates: usize,
    /// Live rewrite groups planned by the tick.
    pub live_groups_planned: usize,
    /// Live rewrite groups committed by the tick.
    pub live_groups_committed: usize,
    /// Live plans invalidated by snapshot change.
    pub live_snapshot_changes: usize,
    /// Staging inputs committed by fold work.
    pub staging_input_files: usize,
    /// Staging input bytes committed by fold work.
    pub staging_input_bytes: u64,
    /// Staging outputs committed by fold work.
    pub staging_output_files: usize,
    /// Staging output bytes committed by fold work.
    pub staging_output_bytes: u64,
    /// Live replacement input files committed by the tick.
    pub live_input_files: usize,
    /// Live replacement input bytes committed by the tick.
    pub live_input_bytes: u64,
    /// Live replacement output files committed by the tick.
    pub live_output_files: usize,
    /// Live replacement output bytes committed by the tick.
    pub live_output_bytes: u64,
    /// Whether the periodic owner examined its complete roster.
    pub tick_complete: bool,
    /// Whether conservative work remains after this outcome.
    pub pending_work: bool,
}

impl ForgeTickOutcome {
    /// Returns true only for a complete, observed no-work periodic pass.
    #[must_use]
    pub fn is_converged(&self) -> bool {
        self.tick_complete
            && self.tables_examined == self.tables_discovered
            && !self.pending_work
            && [
                self.groups_seen,
                self.bins_committed,
                self.bins_skipped,
                self.reconciled,
                self.tables_skipped,
                self.tables_failed,
                self.reconciliation_recovered,
                self.expiry_reconciled,
                self.gc_reconciled,
                self.gc_candidates,
                self.gc_deleted,
                self.gc_skipped,
                self.budget_skips,
                self.lease_contention,
                self.lease_takeovers,
                self.fence_losses,
                self.stage_failures,
                self.staging_pending_files,
                self.live_candidates,
                self.live_groups_planned,
                self.live_groups_committed,
                self.live_snapshot_changes,
                self.live_recovered,
                self.live_reset,
                self.live_pending,
                self.live_unresolved,
                self.open_operation_overflows,
                self.staging_input_files,
                self.staging_output_files,
                self.live_input_files,
                self.live_output_files,
                self.outputs_committed,
            ]
            .into_iter()
            .all(|value| value == 0)
            && [
                self.staging_input_bytes,
                self.staging_output_bytes,
                self.live_input_bytes,
                self.live_output_bytes,
                self.spill_bytes,
                self.input_rows,
                self.output_rows,
            ]
            .into_iter()
            .all(|value| value == 0)
    }
}

/// One decoded SQL row used to reconstruct an interrupted staging projection.
struct InterruptedStagingRow {
    /// Candidate file data needed by exact reset and retry.
    file: CandidateFile,
    /// Exact time partition that must agree across the exact set.
    partition: TimePartition,
    /// Whether the prepared projection marked this input compacted.
    compacted: bool,
    /// Prepared operation identity stamped on the input.
    operation_id: Option<Uuid>,
}

impl InterruptedStagingRow {
    /// Decodes one catalog row without applying cross-row invariants.
    ///
    /// # Errors
    ///
    /// Returns a typed group or reconciliation error for malformed SQL values.
    fn decode(row: &PgRow) -> Result<Self, ForgeError> {
        let size = row
            .try_get::<i64, _>("file_size")
            .map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })?;
        Ok(Self {
            file: CandidateFile {
                id: row.try_get("id").map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })?,
                path: row
                    .try_get("file_path")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?,
                size: u64::try_from(size).map_err(|_| ForgeError::Group {
                    detail: "interrupted staging file size is negative".to_owned(),
                })?,
                min_event_time: row.try_get("min_event_time").map_err(|error| {
                    ForgeError::Group {
                        detail: error.to_string(),
                    }
                })?,
                max_event_time: row.try_get("max_event_time").map_err(|error| {
                    ForgeError::Group {
                        detail: error.to_string(),
                    }
                })?,
            },
            partition: row_time_partition(row, ForgeError::reconciliation)?,
            compacted: row
                .try_get("compacted")
                .map_err(|error| ForgeError::Reconciliation {
                    detail: error.to_string(),
                })?,
            operation_id: row.try_get("publication_operation_id").map_err(|error| {
                ForgeError::Reconciliation {
                    detail: error.to_string(),
                }
            })?,
        })
    }
}

/// Exact invariant-bearing projection left between SQL prepare and acknowledgement.
struct InterruptedStagingTask {
    /// Tenant/table/partition identity shared by all prepared inputs.
    key: ForgeGroupKey,
    /// Ordered files and checked byte total needed for a later retry.
    bin: RewriteBin,
    /// One prepared operation identity shared by all inputs.
    operation_id: Uuid,
}

impl InterruptedStagingTask {
    /// Reconstructs one exact interrupted projection from decoded SQL rows.
    ///
    /// A complete uncompacted set is ordinary work and returns `None`. Mixed
    /// partition, compacted, or operation state fails closed.
    ///
    /// # Errors
    ///
    /// Returns group or reconciliation errors for malformed or inconsistent rows.
    fn decode(
        binding: &TenantTableBinding,
        expected_files: usize,
        rows: Vec<PgRow>,
    ) -> Result<Option<Self>, ForgeError> {
        if rows.len() != expected_files || rows.is_empty() {
            return Ok(None);
        }
        let rows = rows
            .into_iter()
            .map(|row| InterruptedStagingRow::decode(&row))
            .collect::<Result<Vec<_>, _>>()?;
        let first = &rows[0];
        if rows.iter().any(|row| row.partition != first.partition) {
            return Err(ForgeError::Reconciliation {
                detail: "interrupted staging task crosses a time partition".to_owned(),
            });
        }
        if rows.iter().any(|row| row.compacted != first.compacted) {
            return Err(ForgeError::Reconciliation {
                detail: "interrupted staging task has mixed compacted state".to_owned(),
            });
        }
        if rows
            .iter()
            .any(|row| row.operation_id != first.operation_id)
        {
            return Err(ForgeError::Reconciliation {
                detail: "interrupted staging task has mixed operation identity".to_owned(),
            });
        }
        if !first.compacted {
            return Ok(None);
        }
        let partition = first.partition;
        let operation_id = first
            .operation_id
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "interrupted staging task lost its operation identity".to_owned(),
            })?;
        let files = rows.into_iter().map(|row| row.file).collect::<Vec<_>>();
        let total_bytes = files.iter().try_fold(0_u64, |total, file| {
            total
                .checked_add(file.size)
                .ok_or_else(|| ForgeError::Group {
                    detail: "interrupted staging task byte total overflows".to_owned(),
                })
        })?;
        Ok(Some(Self {
            key: ForgeGroupKey {
                tenant: binding.tenant,
                table_ref: binding.table_ref.clone(),
                partition,
            },
            bin: RewriteBin { files, total_bytes },
            operation_id,
        }))
    }

    /// Validates that the operation ledger describes this exact projection.
    ///
    /// # Errors
    ///
    /// Returns reconciliation failure when the prepared detail names another operation.
    fn validate_prepared_detail(&self, detail: &AuditDetail) -> Result<(), ForgeError> {
        if matches!(
            detail,
            AuditDetail::ForgeCompaction { operation_id, .. } if *operation_id == self.operation_id
        ) {
            return Ok(());
        }
        Err(ForgeError::Reconciliation {
            detail: "interrupted staging task operation identity changed".to_owned(),
        })
    }

    /// Finds the snapshot committed by this exact operation, when present.
    #[must_use]
    fn committed_snapshot_id(&self, table: &Table) -> Option<i64> {
        let operation_id = self.operation_id.to_string();
        table.metadata().snapshots().find_map(|snapshot| {
            (snapshot
                .summary()
                .additional_properties
                .get("forge.operation_id")
                == Some(&operation_id))
            .then_some(snapshot.snapshot_id())
        })
    }

    /// Returns the exact input row identities owned by this projection.
    #[must_use]
    fn input_file_ids(&self) -> Vec<Uuid> {
        self.bin.files.iter().map(|file| file.id).collect()
    }
}

/// Convert the configured staging-discovery count into the durable query domain.
///
/// The returned positive count preserves the validated `usize` limit exactly
/// for binding to a `PostgreSQL` `bigint` query parameter.
///
/// # Errors
///
/// Returns invalid configuration when the count exceeds `PostgreSQL` `bigint`.
fn staging_discovery_limit(limit: usize) -> Result<i64, ForgeError> {
    i64::try_from(limit).map_err(|_| ForgeError::InvalidConfig {
        detail: "max_files_per_tick exceeds PostgreSQL bigint".to_owned(),
    })
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

    /// Resolve the table property that controls Forge packing and output rotation.
    ///
    /// # Errors
    ///
    /// Returns catalog or invalid-property errors when the table does not
    /// provide a target representable as `u64`.
    async fn table_right_size_policy(
        &self,
        binding: &TenantTableBinding,
    ) -> Result<ForgeRightSizePolicy, ForgeError> {
        let table = self.load_table(&binding.table_ident()).await?;
        let target = u64::try_from(
            table
                .metadata()
                .table_properties()
                .map_err(ForgeError::Catalog)?
                .write_target_file_size_bytes,
        )
        .map_err(|_| ForgeError::InvalidConfig {
            detail: "write.target-file-size-bytes exceeds u64".to_owned(),
        })?;
        if target == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "write.target-file-size-bytes must be positive".to_owned(),
            });
        }
        validate_supported_layout(
            table.metadata().current_schema(),
            table.metadata().default_partition_spec(),
        )?;
        ForgeRightSizePolicy::from_table_threshold(
            target,
            self.core.config.small_file_threshold_bytes,
            table.metadata().current_schema_id(),
            table.metadata().default_partition_spec_id(),
            table.metadata().default_sort_order_id(),
        )
    }

    /// Discovers bounded per-partition staging bins for durable task planning.
    ///
    /// Each returned candidate is executable by one task and one Iceberg
    /// transaction. Cross-partition staging rows therefore never enter a single
    /// durable payload that the worker could only publish partially. `capacity`
    /// is the scheduler's live governor-clamped envelope authority.
    ///
    /// # Errors
    ///
    /// Returns SQL, row decoding, layout-policy, size, or bound failures.
    pub(super) async fn discover_staging_task_candidates(
        &self,
        binding: &TenantTableBinding,
        now: chrono::DateTime<chrono::Utc>,
        capacity: super::planner::ForgeCapacity,
    ) -> Result<Vec<ForgePlanCandidate>, ForgeError> {
        let rows = sqlx::query(
            r"SELECT id,file_path,file_size,min_event_time,max_event_time,
                      partition_granularity,partition_start
                FROM vala.file_list
               WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND NOT compacted
               ORDER BY partition_granularity,partition_start,min_event_time,max_event_time,id
               LIMIT $4",
        )
        .bind(binding.tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .bind(staging_discovery_limit(
            self.core.config.max_files_per_tick,
        )?)
        .fetch_all(self.core.operator_pool.pool())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        let mut grouped = BTreeMap::<TimePartition, Vec<CandidateFile>>::new();
        for row in rows {
            let partition = row_time_partition(&row, ForgeError::group)?;
            let size: i64 = row
                .try_get("file_size")
                .map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })?;
            grouped.entry(partition).or_default().push(CandidateFile {
                id: row.try_get("id").map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })?,
                path: row
                    .try_get("file_path")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?,
                size: u64::try_from(size).map_err(|_| ForgeError::Group {
                    detail: "negative staging file size".to_owned(),
                })?,
                min_event_time: row.try_get("min_event_time").map_err(|error| {
                    ForgeError::Group {
                        detail: error.to_string(),
                    }
                })?,
                max_event_time: row.try_get("max_event_time").map_err(|error| {
                    ForgeError::Group {
                        detail: error.to_string(),
                    }
                })?,
            });
        }
        let policy = self.table_right_size_policy(binding).await?;
        let mut candidates = Vec::new();
        for (partition, files) in grouped {
            for bin in plan_staging_bins(
                &policy,
                &files,
                self.core.config.max_files_per_bin,
                partition,
                now,
            ) {
                let total_bytes = bin.total_bytes;
                let mut input_terms = bin
                    .files
                    .into_iter()
                    .map(|file| (file.path, file.size))
                    .collect::<Vec<_>>();
                input_terms.sort_by(|left, right| left.0.cmp(&right.0));
                let (inputs, input_bytes): (Vec<_>, Vec<_>) = input_terms.into_iter().unzip();
                let envelope = super::planner::ForgeEnvelopeSizer::size(
                    total_bytes,
                    inputs.len(),
                    self.core.config.max_concurrent_reads,
                    capacity,
                )?;
                candidates.push(ForgePlanCandidate {
                    strategy: ForgeTaskStrategy::StagingFold,
                    parallelism: envelope.reader_permits,
                    memory_bytes: envelope.memory_bytes().map_err(|error| {
                        ForgeError::Invariant {
                            detail: error.to_string(),
                        }
                    })?,
                    spill_bytes: envelope.scratch_bytes().map_err(|error| {
                        ForgeError::Invariant {
                            detail: error.to_string(),
                        }
                    })?,
                    inputs,
                    input_bytes,
                    bytes: total_bytes,
                    parameters: serde_json::json!({"kind":"staging_fold"}),
                });
            }
        }
        Ok(candidates)
    }

    /// Executes one exact durable staging-fold payload under the task attempt.
    ///
    /// The persisted input list is authoritative: every path must still be an
    /// uncompacted row for the same tenant/table and all rows must share one
    /// day partition. The current T13 planner emits one bounded staging task;
    /// rejecting a changed or cross-partition set prevents the worker from
    /// silently selecting replacement work outside that exact payload.
    ///
    /// # Errors
    ///
    /// Returns SQL, identity, stale-plan, capacity, rewrite, fencing, catalog,
    /// or durable-bookkeeping errors. No rewrite begins until the exact set is
    /// reconstructed and validated.
    pub(super) async fn execute_staging_task(
        &self,
        request: StagingTaskRequest<'_>,
    ) -> Result<iceberg::table::Table, ForgeError> {
        let StagingTaskRequest {
            rewrite,
            lease,
            binding,
            inputs,
            task_id,
            attempt_id,
            stop,
        } = request;
        let (key, bin) = self.load_exact_staging_bin(binding, inputs).await?;
        let policy = self.table_right_size_policy(binding).await?;
        let commit = self
            .compact_bin(StagingRewriteRequest {
                rewrite,
                lease,
                key: &key,
                binding,
                bin: &bin,
                right_size_policy: &policy,
                attempt_generation: ForgeAttemptGeneration::from_attempt(attempt_id),
                task_identity: Some((task_id, attempt_id)),
                stop,
            })
            .await?;
        self.core
            .telemetry
            .record_task_spill(ForgeTaskMetricStrategy::StagingFold, commit.spill_bytes);
        Ok(commit.committed_table)
    }

    /// Reconstructs and validates the exact durable staging payload from SQL.
    ///
    /// # Errors
    ///
    /// Returns SQL, row decoding, stale-set, cross-partition, negative-size,
    /// or byte-overflow errors before any rewrite begins.
    async fn load_exact_staging_bin(
        &self,
        binding: &TenantTableBinding,
        inputs: &[String],
    ) -> Result<(ForgeGroupKey, RewriteBin), ForgeError> {
        if inputs.is_empty() || inputs.len() > self.core.config.max_files_per_bin {
            return Err(ForgeError::Group {
                detail: "exact staging task must contain one bounded non-empty bin".to_owned(),
            });
        }
        let rows = sqlx::query(
            r"SELECT id,file_path,file_size,min_event_time,max_event_time,
                      partition_granularity,partition_start
                 FROM vala.file_list
                WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3
                  AND file_path=ANY($4) AND NOT compacted
                ORDER BY partition_granularity,partition_start,min_event_time,max_event_time,id",
        )
        .bind(binding.tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .bind(inputs)
        .fetch_all(self.core.operator_pool.pool())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        if rows.len() != inputs.len() {
            return Err(ForgeError::Reconciliation {
                detail: "exact staging task inputs changed before execution".to_owned(),
            });
        }
        let mut files = Vec::with_capacity(rows.len());
        let mut partition = None;
        for row in rows {
            let observed = row_time_partition(&row, ForgeError::group)?;
            if partition
                .replace(observed)
                .is_some_and(|current| current != observed)
            {
                return Err(ForgeError::Group {
                    detail: "exact staging task crosses a time partition".to_owned(),
                });
            }
            let size: i64 = row
                .try_get("file_size")
                .map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })?;
            files.push(CandidateFile {
                id: row.try_get("id").map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })?,
                path: row
                    .try_get("file_path")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?,
                size: u64::try_from(size).map_err(|_| ForgeError::Group {
                    detail: "negative exact staging file size".to_owned(),
                })?,
                min_event_time: row.try_get("min_event_time").map_err(|error| {
                    ForgeError::Group {
                        detail: error.to_string(),
                    }
                })?,
                max_event_time: row.try_get("max_event_time").map_err(|error| {
                    ForgeError::Group {
                        detail: error.to_string(),
                    }
                })?,
            });
        }
        let actual = files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<BTreeSet<_>>();
        let planned = inputs.iter().map(String::as_str).collect::<BTreeSet<_>>();
        if actual != planned {
            return Err(ForgeError::Reconciliation {
                detail: "exact staging task path set changed before execution".to_owned(),
            });
        }
        let partition = partition.ok_or_else(|| ForgeError::Group {
            detail: "exact staging task lost its time partition".to_owned(),
        })?;
        let total_bytes = files.iter().try_fold(0_u64, |total, file| {
            total
                .checked_add(file.size)
                .ok_or_else(|| ForgeError::Group {
                    detail: "exact staging task byte total overflow".to_owned(),
                })
        })?;
        let key = ForgeGroupKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
            partition,
        };
        Ok((key, RewriteBin { files, total_bytes }))
    }

    /// Recovers an interrupted staging publication before an exact-task replay.
    ///
    /// A process panic can leave every planned input compacted under the
    /// deterministic operation identity while the task remains `running`. If
    /// Iceberg contains that identity, this stamps the recovered snapshot. If
    /// it does not, this resets only that exact prepared projection so the same
    /// durable task can rewrite it. Ordinary uncompacted tasks are unchanged.
    ///
    /// # Errors
    ///
    /// Returns SQL, malformed-set, ambiguous-operation, fence, catalog, or
    /// exact reset/stamp failures. Mixed compacted state fails closed.
    pub(super) async fn recover_interrupted_staging_task(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        inputs: &[String],
        table: &Table,
    ) -> Result<Option<Table>, ForgeError> {
        let Some(interrupted) = self.load_interrupted_staging_task(binding, inputs).await? else {
            return Ok(None);
        };
        let input_paths = inputs.iter().cloned().collect::<BTreeSet<_>>();
        let detail = self
            .prepared_staging_detail(binding.tenant, &interrupted.key, &input_paths)
            .await?;
        interrupted.validate_prepared_detail(&detail)?;
        if let Some(snapshot_id) = interrupted.committed_snapshot_id(table) {
            self.stamp_recovered_staging_task(lease, binding, inputs, snapshot_id)
                .await?;
            return Ok(Some(table.clone()));
        }
        self.reset_reconciled(
            lease,
            &interrupted.key,
            &interrupted.input_file_ids(),
            &detail,
        )
        .await?;
        tracing::info!(operation_id=%interrupted.operation_id, prepared_detail=?detail, "reset interrupted pre-commit Forge staging projection");
        Ok(None)
    }

    /// Loads the exact persisted projection that may have survived an interrupted staging task.
    ///
    /// # Errors
    ///
    /// Returns SQL, group, or reconciliation errors when the persisted rows
    /// cannot be read or do not form one unambiguous interrupted projection.
    async fn load_interrupted_staging_task(
        &self,
        binding: &TenantTableBinding,
        inputs: &[String],
    ) -> Result<Option<InterruptedStagingTask>, ForgeError> {
        let rows = sqlx::query(
            r"SELECT id,file_path,file_size,min_event_time,max_event_time,
                      partition_granularity,partition_start,compacted,publication_operation_id
                 FROM vala.file_list
                WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND file_path=ANY($4)
                ORDER BY partition_granularity,partition_start,min_event_time,max_event_time,id",
        )
        .bind(binding.tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .bind(inputs)
        .fetch_all(self.core.operator_pool.pool())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        InterruptedStagingTask::decode(binding, inputs.len(), rows)
    }

    /// Finalizes the exact staging projection after task-tagged commit recovery.
    ///
    /// The persisted task input paths remain authoritative. Recovery accepts
    /// only compacted rows from that exact set, requires one time partition,
    /// and transitions the one matching prepared compaction operation. A
    /// replay after the projection and audit transaction committed is a no-op.
    ///
    /// # Errors
    ///
    /// Returns SQL, identity, exact-set, operation-state, audit, or fence
    /// errors. Missing, cross-partition, differently stamped, or ambiguously
    /// prepared inputs fail closed without changing durable state.
    pub(super) async fn stamp_recovered_staging_task(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        inputs: &[String],
        snapshot_id: i64,
    ) -> Result<(), ForgeError> {
        if inputs.is_empty() || inputs.len() > self.core.config.max_files_per_bin {
            return Err(ForgeError::Reconciliation {
                detail: "recovered staging task must contain one bounded non-empty bin".to_owned(),
            });
        }
        let rows = sqlx::query(
            r"SELECT id,file_path,partition_granularity,partition_start,committed_snapshot_id
                 FROM vala.file_list
                WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3
                  AND file_path=ANY($4) AND compacted
                ORDER BY partition_granularity,partition_start,id",
        )
        .bind(binding.tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .bind(inputs)
        .fetch_all(self.core.operator_pool.pool())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        if rows.len() != inputs.len() {
            return Err(ForgeError::Reconciliation {
                detail: "recovered staging task inputs do not match the exact compacted set"
                    .to_owned(),
            });
        }
        let mut partition = None;
        let mut file_ids = Vec::with_capacity(rows.len());
        let mut actual_paths = BTreeSet::new();
        let mut already_stamped = true;
        for row in rows {
            let observed = row_time_partition(&row, ForgeError::reconciliation)?;
            if partition
                .replace(observed)
                .is_some_and(|current| current != observed)
            {
                return Err(ForgeError::Reconciliation {
                    detail: "recovered staging task crosses a time partition".to_owned(),
                });
            }
            file_ids.push(
                row.try_get("id")
                    .map_err(|error| ForgeError::Reconciliation {
                        detail: error.to_string(),
                    })?,
            );
            actual_paths.insert(row.try_get::<String, _>("file_path").map_err(|error| {
                ForgeError::Reconciliation {
                    detail: error.to_string(),
                }
            })?);
            let committed: Option<i64> = row.try_get("committed_snapshot_id").map_err(|error| {
                ForgeError::Reconciliation {
                    detail: error.to_string(),
                }
            })?;
            match committed {
                Some(id) if id == snapshot_id => {}
                None => already_stamped = false,
                Some(_) => {
                    return Err(ForgeError::Reconciliation {
                        detail: "recovered staging input is stamped to another snapshot".to_owned(),
                    });
                }
            }
        }
        let planned_paths = inputs.iter().cloned().collect::<BTreeSet<_>>();
        if actual_paths != planned_paths {
            return Err(ForgeError::Reconciliation {
                detail: "recovered staging task path set changed".to_owned(),
            });
        }
        if already_stamped {
            return Ok(());
        }
        let key = ForgeGroupKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
            partition: partition.ok_or_else(|| ForgeError::Reconciliation {
                detail: "recovered staging task lost its time partition".to_owned(),
            })?,
        };
        let detail = self
            .prepared_staging_detail(binding.tenant, &key, &planned_paths)
            .await?;
        self.stamp_reconciled(lease, &key, &file_ids, &detail, snapshot_id)
            .await
    }

    /// Resolve the one open staging operation matching an exact recovered path set.
    ///
    /// # Errors
    ///
    /// Returns SQL or reconciliation errors when bounded operation evidence is
    /// malformed, overflowed, absent, or ambiguous.
    async fn prepared_staging_detail(
        &self,
        tenant: DataTenantId,
        key: &ForgeGroupKey,
        planned_paths: &BTreeSet<String>,
    ) -> Result<AuditDetail, ForgeError> {
        let resource = key.audit_resource();
        let mut conn = self
            .core
            .vala
            .tenant_conn(tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = ForgeOperations::new(&resource, ForgeOperationFamily::StagingFold)
            .map_err(ForgeError::Sql)?
            .list_open(&mut conn, self.core.config.max_open_operations_per_table)
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        if page.overflowed {
            return Err(ForgeError::Reconciliation {
                detail: "open staging operations exceeded the recovery bound".to_owned(),
            });
        }
        let matching = page
            .operations
            .into_iter()
            .filter(|operation| match &operation.prepared_detail {
                AuditDetail::ForgeCompaction { input_paths, .. } => input_paths
                    .iter()
                    .map(ToString::to_string)
                    .collect::<BTreeSet<_>>()
                    .eq(planned_paths),
                _ => false,
            })
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "expected exactly one prepared staging operation for recovered inputs; matched {}",
                    matching.len()
                ),
            });
        }
        Ok(matching
            .into_iter()
            .next()
            .expect("exactly one matching prepared staging operation was validated")
            .prepared_detail)
    }
}

/// Decodes the exact time partition carried by one `vala.file_list` row.
///
/// The durable representation is the `(partition_granularity, partition_start)`
/// column pair; `wrap` selects the failure variant the calling workflow must
/// surface, because planning refuses as a grouping error while recovery refuses
/// as a reconciliation mismatch.
///
/// # Errors
///
/// Returns `wrap`-built errors when either column is absent or mistyped, or
/// when the pair does not name a canonical partition boundary.
fn row_time_partition(
    row: &PgRow,
    wrap: fn(String) -> ForgeError,
) -> Result<TimePartition, ForgeError> {
    let granularity: String = row
        .try_get("partition_granularity")
        .map_err(|error| wrap(error.to_string()))?;
    let start: chrono::DateTime<chrono::Utc> = row
        .try_get("partition_start")
        .map_err(|error| wrap(error.to_string()))?;
    TimePartition::from_durable_columns(&granularity, start).map_err(|error| {
        wrap(format!(
            "file_list row has an invalid time partition: {error}"
        ))
    })
}

/// Map durable staging facts into the live-file policy without reusing its
/// separate [`CandidateFile`] domain, then map selected groups back to rewrite
/// bins by their stable object paths.
fn plan_staging_bins(
    policy: &ForgeRightSizePolicy,
    files: &[CandidateFile],
    max_files: usize,
    partition: TimePartition,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<RewriteBin> {
    if max_files == 0 {
        return Vec::new();
    }
    let mut by_path = HashMap::with_capacity(files.len());
    let live = files
        .iter()
        .map(|file| {
            by_path.insert(file.path.clone(), file.clone());
            IcebergCandidateFile {
                catalog_path: file.path.clone(),
                object_path: file.path.clone(),
                file_size_bytes: file.size,
                record_count: 0,
                schema_id: policy.schema_id(),
                partition_spec_id: policy.partition_spec_id(),
                partition,
                sort_order_id: Some(policy.sort_order_id()),
                min_event_time: file.min_event_time,
                max_event_time: file.max_event_time,
                source_snapshot_id: 0,
                data_sequence_number: None,
                file_sequence_number: None,
            }
        })
        .collect();
    let partition_is_open = partition.is_open_at(now);
    let plan = policy.plan_partition(live, partition_is_open);
    let groups = plan
        .groups
        .into_iter()
        .flat_map(|group| {
            let IcebergRewriteGroup { files, reason } = group;
            files
                .chunks(max_files)
                .filter(|chunk| {
                    reason != IcebergRewriteReason::Undersized
                        || chunk.len() >= 2
                        || !partition_is_open
                })
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut bins = groups
        .into_iter()
        .filter_map(|group| {
            let files = group
                .into_iter()
                .filter_map(|file| by_path.remove(&file.catalog_path))
                .collect::<Vec<_>>();
            (!files.is_empty()).then(|| RewriteBin {
                total_bytes: files.iter().map(|file| file.size).sum(),
                files,
            })
        })
        .collect::<Vec<_>>();
    if !partition_is_open && !by_path.is_empty() {
        let mut remaining = by_path.into_values().collect::<Vec<_>>();
        remaining.sort_by(|left, right| left.path.cmp(&right.path));
        bins.extend(remaining.chunks(max_files).map(|files| RewriteBin {
            total_bytes: files.iter().map(|file| file.size).sum(),
            files: files.to_vec(),
        }));
    }
    bins
}

/// Borrowed authority and exact payload for one durable staging-fold task.
///
/// The worker owns the attempt's rewrite pipeline and publication fence, so
/// this request carries them by reference rather than letting the staging
/// stage reach back into shared Forge state for either.
pub(super) struct StagingTaskRequest<'a> {
    /// Attempt-local pipeline bound to the retained operation lease.
    pub(super) rewrite: &'a super::rewrite::ForgeRewritePipeline,
    /// Mutable publication fence retained through every external effect.
    pub(super) lease: &'a mut ForgeLease,
    /// Tenant/table identity the persisted payload must belong to.
    pub(super) binding: &'a TenantTableBinding,
    /// Authoritative persisted input paths for this exact task.
    pub(super) inputs: &'a [String],
    /// Durable task identity used for commit bookkeeping.
    pub(super) task_id: Uuid,
    /// Attempt generation fencing this execution.
    pub(super) attempt_id: Uuid,
    /// Cooperative cancellation observed before any committed effect.
    pub(super) stop: &'a CancellationToken,
}

/// Borrowed authority and exact payload for one staging rewrite transaction.
struct StagingRewriteRequest<'a> {
    /// Attempt-local pipeline bound to the retained operation lease.
    rewrite: &'a super::rewrite::ForgeRewritePipeline,
    /// Mutable publication fence retained through every external effect.
    lease: &'a mut ForgeLease,
    /// Tenant/table/day identity for the exact staging bin.
    key: &'a ForgeGroupKey,
    /// Server-resolved physical table binding.
    binding: &'a TenantTableBinding,
    /// Exact planned staging input set.
    bin: &'a RewriteBin,
    /// Table-derived packing and output-size policy.
    right_size_policy: &'a ForgeRightSizePolicy,
    /// Attempt-owned output generation.
    attempt_generation: ForgeAttemptGeneration,
    /// Durable task and attempt stamped into snapshot properties.
    task_identity: Option<(Uuid, Uuid)>,
    /// Cancellation source observed around each effect boundary.
    stop: &'a CancellationToken,
}

impl Forge {
    /// Execute one fenced compaction operation from staged files to Iceberg.
    ///
    /// The sequence is read and validate, write the output, mark inputs prepared,
    /// commit the Iceberg transaction, and stamp the committed snapshot. Every
    /// external boundary is fenced so a stale lease cannot finish the operation.
    ///
    /// # Errors
    ///
    /// Returns catalog, rewrite, object-store, fence, audit, or SQL errors at
    /// the exact durable boundary where execution stopped.
    async fn compact_bin(
        &self,
        request: StagingRewriteRequest<'_>,
    ) -> Result<CompactCommit, ForgeError> {
        let StagingRewriteRequest {
            rewrite,
            lease,
            key,
            binding,
            bin,
            right_size_policy,
            attempt_generation,
            task_identity,
            stop,
        } = request;
        let operation_generation =
            task_identity.map_or(attempt_generation.as_uuid(), |(_, attempt_id)| attempt_id);
        let operation_id = operation_id(key, bin, operation_generation);
        let table = self.load_table(&binding.table_ident()).await?;
        let source_files = bin
            .files
            .iter()
            .map(|file| RewriteSourceFile {
                catalog_path: file.path.clone(),
                object_path: file.path.clone(),
                file_size_bytes: file.size,
                record_count: 0,
            })
            .collect::<Vec<_>>();
        let schema = Arc::new(
            iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema())
                .map_err(ForgeError::Catalog)?,
        );
        let rewrite = rewrite
            .rewrite(
                RewriteRequest {
                    attempt_generation,
                    binding,
                    schema,
                    iceberg_schema: table.metadata().current_schema().clone(),
                    source_files: &source_files,
                    partition: key.partition,
                    bloom_columns: crate::forge::rewrite::table_bloom_columns(table.metadata())?,
                    table_location: table.metadata().location(),
                    partition_spec_id: table.metadata().default_partition_spec_id(),
                    sort_order_id: i32::try_from(table.metadata().default_sort_order_id())
                        .map_err(|_| ForgeError::InvalidConfig {
                            detail: "Iceberg default sort-order ID exceeds i32".to_owned(),
                        })?,
                    target_file_size_bytes: right_size_policy.target_file_size_bytes(),
                },
                stop,
                lease,
                &self.core.operator_pool,
            )
            .await?;
        tracing::debug!(
            operation_id = %operation_id,
            input_rows = rewrite.input_rows,
            output_rows = rewrite.output_rows,
            output_files = rewrite.object_paths.len(),
            spill_bytes = rewrite.spill_bytes,
            "Forge streaming rewrite completed"
        );
        let output_bytes = rewrite_output_bytes(&rewrite.files)?;
        lease.require_fence(&self.core.operator_pool).await?;
        let prepared = forge_detail(
            key,
            bin,
            &rewrite.files,
            operation_id,
            ForgeCompactionPhase::Prepared,
            None,
        )?;
        self.prepare_inputs(lease, key, bin, operation_id, prepared)
            .await?;
        let committed_table = self
            .commit_rewrite(
                lease,
                CommitRewriteRequest {
                    key,
                    binding,
                    bin,
                    operation_id,
                    rewrite: &rewrite,
                    task_identity,
                    stop,
                },
            )
            .await?;
        self.core.telemetry.record_rewrite_volume(
            super::metrics::ForgeMetricSource::Staging,
            bin.files.len(),
            bin.total_bytes,
            rewrite.files.len(),
            output_bytes,
        );
        Ok(CompactCommit {
            committed_table,
            spill_bytes: rewrite.spill_bytes,
        })
    }
}

/// Sum exact output-file sizes for the production rewrite-volume counter.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the committed byte total exceeds `u64`.
fn rewrite_output_bytes(files: &[iceberg::spec::DataFile]) -> Result<u64, ForgeError> {
    files.iter().try_fold(0_u64, |total, file| {
        total
            .checked_add(file.file_size_in_bytes())
            .ok_or_else(|| ForgeError::Invariant {
                detail: "staging rewrite output bytes overflowed u64".to_owned(),
            })
    })
}

/// One staging rewrite commit retaining exact returned catalog evidence.
struct CompactCommit {
    /// Exact table returned by the Iceberg transaction.
    committed_table: iceberg::table::Table,
    /// Final spill accounting observed by the completed rewrite.
    spill_bytes: u64,
}

/// Immutable inputs for one fenced Iceberg commit and its durable bookkeeping.
struct CommitRewriteRequest<'a> {
    /// Physical candidate group whose staged inputs become visible.
    key: &'a ForgeGroupKey,
    /// Server-resolved physical table binding for the group.
    binding: &'a TenantTableBinding,
    /// Exact staged inputs prepared for this operation.
    bin: &'a RewriteBin,
    /// Stable operation identity shared by Iceberg and audit records.
    operation_id: Uuid,
    /// Rewritten outputs awaiting Iceberg publication.
    rewrite: &'a RewriteOutput,
    /// Optional durable task identity recorded in the committed snapshot.
    task_identity: Option<(Uuid, Uuid)>,
    /// Cancellation source observed while the catalog commit is unresolved.
    stop: &'a CancellationToken,
}

impl Forge {
    /// Commit one completed rewrite and stamp its shared terminal snapshot.
    ///
    /// All rotated outputs enter one `RewriteFilesAction`. A definite catalog
    /// failure resets the prepared inputs only while this owner still holds the
    /// fence; uncertain failures remain prepared for reconciliation.
    /// Cancellation while the catalog future is unresolved returns shutdown
    /// without resetting inputs, allowing the normal lease-release path and a
    /// successor reconciliation tick to decide the terminal state. The catalog
    /// future runs inside the shared Forge commit span with only its closed
    /// strategy/result/role fields and scrubbed durable task UUIDs; tenant,
    /// table, SQL, object-path, and error details never enter the span.
    ///
    /// # Errors
    ///
    /// Returns fence, catalog, timeout, reconciliation, SQL, or audit failures.
    async fn commit_rewrite(
        &self,
        lease: &mut ForgeLease,
        request: CommitRewriteRequest<'_>,
    ) -> Result<iceberg::table::Table, ForgeError> {
        let CommitRewriteRequest {
            key,
            binding,
            bin,
            operation_id,
            rewrite,
            task_identity,
            stop,
        } = request;
        lease.require_fence(&self.core.operator_pool).await?;
        if !lease.commit_window_fits(self.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let table = self.load_table(&binding.table_ident()).await?;
        let mut properties = HashMap::new();
        properties.insert("forge.workflow".to_owned(), "staging-fold".to_owned());
        properties.insert("forge.operation_id".to_owned(), operation_id.to_string());
        properties.insert("forge.group".to_owned(), key.audit_resource());
        if let Some((task_id, attempt_id)) = task_identity {
            properties.insert("forge.task_id".to_owned(), task_id.to_string());
            properties.insert("forge.task_attempt".to_owned(), attempt_id.to_string());
        }
        let tx = Transaction::new(&table);
        let action = tx
            .rewrite_files()
            .delete_files(Vec::<String>::new())
            .add_data_files(rewrite.files.iter().cloned())
            .set_commit_uuid(operation_id)
            .set_snapshot_properties(properties);
        let transaction = ApplyTransactionAction::apply(action, tx).map_err(ForgeError::Catalog)?;
        lease.require_fence(&self.core.operator_pool).await?;
        if !lease.commit_window_fits(self.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let span = ForgeTelemetry::catalog_commit_span(
            ForgeCatalogCommitStrategy::StagingFold,
            task_identity,
        );
        let commit = transaction.commit_once(self.core.catalog.as_ref());
        tokio::pin!(commit);
        let response = tokio::select! {
            response = tracing::Instrument::instrument(
                tokio::time::timeout(
                    self.core.config.iceberg_total_retry_timeout,
                    &mut commit,
                ),
                span.clone(),
            ) => response,
            () = stop.cancelled() => {
                span.record("result", "cancelled");
                return Err(ForgeError::Shutdown);
            },
        };
        let committed_table = match response {
            Ok(Ok(committed_table)) => {
                span.record("result", "succeeded");
                committed_table
            }
            Ok(Err(error)) => {
                span.record("result", "failed");
                if Self::is_retryable(&error)
                    && let Some(recovered) = self
                        .recover_uncertain_staging_commit(binding, operation_id)
                        .await?
                {
                    recovered
                } else {
                    if !Self::is_retryable(&error) {
                        if lease.require_fence(&self.core.operator_pool).await.is_ok() {
                            self.reset_inputs(lease, key, bin, &rewrite.files, operation_id)
                                .await?;
                        } else {
                            tracing::warn!(
                                lease_key = %lease.lease_key,
                                operation_id = %operation_id,
                                "definite Iceberg failure could not be reset after fence loss"
                            );
                        }
                    }
                    return Err(ForgeError::Catalog(error));
                }
            }
            Err(_) => {
                span.record("result", "timed_out");
                return Err(ForgeError::Timeout {
                    operation: "Iceberg commit",
                });
            }
        };
        self.finalize_staging_commit(lease, key, bin, rewrite, operation_id, committed_table)
            .await
    }

    /// Stamp SQL terminal state after a staging catalog commit has succeeded.
    ///
    /// The Iceberg mutation has already committed before this method runs. A
    /// failure therefore leaves the prepared SQL state for reconciliation by a
    /// successor owner rather than attempting to reverse the catalog commit.
    ///
    /// # Errors
    ///
    /// Returns reconciliation errors when the committed table has no current
    /// snapshot, or SQL, audit, and fencing errors from the terminal stamp.
    async fn finalize_staging_commit(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        bin: &RewriteBin,
        rewrite: &RewriteOutput,
        operation_id: Uuid,
        committed_table: iceberg::table::Table,
    ) -> Result<iceberg::table::Table, ForgeError> {
        let snapshot_id = committed_table
            .metadata()
            .current_snapshot_id()
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Iceberg replace committed without a current snapshot".to_owned(),
            })?;
        self.stamp_committed(lease, key, bin, &rewrite.files, operation_id, snapshot_id)
            .await?;
        Ok(committed_table)
    }

    /// Reload an accepted staging commit after its catalog response was uncertain.
    ///
    /// Recovery accepts any retained snapshot tagged with the deterministic
    /// operation identity, so a later metadata commit cannot trigger replay.
    ///
    /// # Errors
    ///
    /// Returns catalog errors when the table cannot be reloaded.
    async fn recover_uncertain_staging_commit(
        &self,
        binding: &TenantTableBinding,
        operation_id: Uuid,
    ) -> Result<Option<iceberg::table::Table>, ForgeError> {
        let recovered = self.load_table(&binding.table_ident()).await?;
        let max = self
            .core
            .config
            .max_retained_snapshots_per_table
            .checked_add(1)
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "Forge retained snapshot recovery bound overflowed".to_owned(),
            })?;
        let snapshots = recovered.metadata().snapshots().collect::<Vec<_>>();
        if snapshots.len() > max {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "retained Forge snapshot search has {} snapshots above limit {max}",
                    snapshots.len()
                ),
            });
        }
        let operation = operation_id.to_string();
        let matches = snapshots.into_iter().any(|snapshot| {
            snapshot
                .summary()
                .additional_properties
                .get("forge.operation_id")
                == Some(&operation)
        });
        Ok(matches.then_some(recovered))
    }

    /// Return whether Iceberg classified an error as safe to retry.
    fn is_retryable(error: &iceberg::Error) -> bool {
        error.retryable()
    }
}

/// Reject a staging batch containing a missing, malformed, or foreign tenant.
pub(crate) fn validate_tenant_column(
    batch: &RecordBatch,
    tenant: DataTenantId,
) -> Result<(), ForgeError> {
    let index = batch
        .schema()
        .index_of("data_tenant_id")
        .map_err(|_| ForgeError::Schema {
            detail: "staging file lacks data_tenant_id".to_owned(),
        })?;
    let values = batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ForgeError::Schema {
            detail: "data_tenant_id must be Utf8".to_owned(),
        })?;
    let expected = tenant.to_string();
    if (0..values.len()).any(|index| values.is_null(index) || values.value(index) != expected) {
        return Err(ForgeError::Schema {
            detail: format!("staging file contains a row outside tenant `{tenant}`"),
        });
    }
    Ok(())
}

/// Project source columns into the registered Iceberg schema by field name.
///
/// Matching types are reused; compatible differences are cast through Arrow.
/// Missing fields and failed casts are schema errors rather than positional
/// guesses.
pub(crate) fn project_by_name(
    source: &RecordBatch,
    target: Arc<arrow::datatypes::Schema>,
) -> Result<RecordBatch, ForgeError> {
    let mut columns = Vec::with_capacity(target.fields().len());
    for field in target.fields() {
        let index = source
            .schema()
            .index_of(field.name())
            .map_err(|_| ForgeError::Schema {
                detail: format!("staging schema lacks Iceberg field `{}`", field.name()),
            })?;
        let column = source.column(index);
        let column = if column.data_type() == field.data_type() {
            column.clone()
        } else {
            cast(column, field.data_type()).map_err(|error| ForgeError::Schema {
                detail: format!(
                    "field `{}` cannot be cast to {:?}: {error}",
                    field.name(),
                    field.data_type()
                ),
            })?
        };
        columns.push(column);
    }
    RecordBatch::try_new(target, columns).map_err(|error| ForgeError::Schema {
        detail: error.to_string(),
    })
}

impl Forge {
    /// Mark input rows prepared and append the matching prepared audit event.
    ///
    /// The SQL transition and audit append share one tenant transaction, which is
    /// fenced immediately before commit.
    ///
    /// # Errors
    ///
    /// Returns a lease, SQL, reconciliation, operation-state, audit, or fence
    /// error. Dropping the caller-owned transaction rolls back every mutation.
    async fn prepare_inputs(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        bin: &RewriteBin,
        operation_id: Uuid,
        detail: AuditDetail,
    ) -> Result<(), ForgeError> {
        if !lease.renew(&self.core.operator_pool).await? {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let ids: Vec<Uuid> = bin.files.iter().map(|file| file.id).collect();
        let result = sqlx::query(
            r"UPDATE vala.file_list
              SET compacted = true, publication_operation_id = $1
            WHERE data_tenant_id = $2 AND namespace = $3 AND table_name = $4
              AND partition_granularity = $5 AND partition_start = $6 AND id = ANY($7)
              AND committed_snapshot_id IS NULL",
        )
        .bind(operation_id)
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition.granularity_str())
        .bind(key.partition.start_utc())
        .bind(&ids)
        .execute(&mut **conn.transaction())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        if result.rows_affected() != u64::try_from(ids.len()).unwrap_or(u64::MAX) {
            return Err(ForgeError::Reconciliation {
                detail: "prepared transition did not claim every input file".to_owned(),
            });
        }
        lease.require_fence(&self.core.operator_pool).await?;
        self.append_system_audit(&mut conn, key, "forge.file_compact.prepared", detail)
            .await?;
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }
}

impl Forge {
    /// Stamp the Iceberg snapshot ID on prepared input rows and audit the commit.
    ///
    /// # Errors
    ///
    /// Returns a lease, SQL, reconciliation, operation-state, audit, or fence
    /// error. Dropping the caller-owned transaction rolls back every mutation.
    async fn stamp_committed(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        bin: &RewriteBin,
        outputs: &[DataFile],
        operation_id: Uuid,
        snapshot_id: i64,
    ) -> Result<(), ForgeError> {
        lease.require_fence(&self.core.operator_pool).await?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let ids: Vec<Uuid> = bin.files.iter().map(|file| file.id).collect();
        let result = sqlx::query(
            r"UPDATE vala.file_list
              SET committed_snapshot_id = $1
            WHERE data_tenant_id = $2 AND namespace = $3 AND table_name = $4
              AND partition_granularity = $5 AND partition_start = $6 AND id = ANY($7)
              AND compacted AND publication_operation_id = $8
              AND (committed_snapshot_id IS NULL OR committed_snapshot_id = $1)",
        )
        .bind(snapshot_id)
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition.granularity_str())
        .bind(key.partition.start_utc())
        .bind(&ids)
        .bind(operation_id)
        .execute(&mut **conn.transaction())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        if result.rows_affected() != u64::try_from(ids.len()).unwrap_or(u64::MAX) {
            return Err(ForgeError::Reconciliation {
                detail: "committed transition did not stamp every input file".to_owned(),
            });
        }
        let detail = forge_detail(
            key,
            bin,
            outputs,
            operation_id,
            ForgeCompactionPhase::Committed,
            Some(snapshot_id),
        )?;
        lease.require_fence(&self.core.operator_pool).await?;
        self.append_system_audit(&mut conn, key, "forge.file_compact.committed", detail)
            .await?;
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }

    /// Restore prepared input rows to the uncompacted state after a definite
    /// Iceberg failure.
    ///
    /// # Errors
    ///
    /// Returns a lease, SQL, reconciliation, operation-state, audit, or fence
    /// error. Dropping the caller-owned transaction rolls back every mutation.
    async fn reset_inputs(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        bin: &RewriteBin,
        outputs: &[DataFile],
        operation_id: Uuid,
    ) -> Result<(), ForgeError> {
        lease.require_fence(&self.core.operator_pool).await?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let ids: Vec<Uuid> = bin.files.iter().map(|file| file.id).collect();
        let result = sqlx::query(
            r"UPDATE vala.file_list
              SET compacted = false, committed_snapshot_id = NULL,
                  publication_operation_id = NULL
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_granularity = $4 AND partition_start = $5 AND id = ANY($6)
              AND committed_snapshot_id IS NULL
              AND publication_operation_id = $7",
        )
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition.granularity_str())
        .bind(key.partition.start_utc())
        .bind(&ids)
        .bind(operation_id)
        .execute(&mut **conn.transaction())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        if result.rows_affected() != u64::try_from(ids.len()).unwrap_or(u64::MAX) {
            return Err(ForgeError::Reconciliation {
                detail: "reset transition did not restore every input file".to_owned(),
            });
        }
        let detail = forge_detail(
            key,
            bin,
            outputs,
            operation_id,
            ForgeCompactionPhase::Reset,
            None,
        )?;
        lease.require_fence(&self.core.operator_pool).await?;
        self.append_system_audit(&mut conn, key, "forge.file_compact.reset", detail)
            .await?;
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }
}

impl Forge {
    /// Stamp a recovered compaction with the snapshot that already contains its
    /// output.
    ///
    /// # Errors
    ///
    /// Returns a lease, SQL, reconciliation, operation-state, audit, or fence
    /// error. Dropping the caller-owned transaction rolls back every mutation.
    async fn stamp_reconciled(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        input_file_ids: &[Uuid],
        detail: &AuditDetail,
        snapshot_id: i64,
    ) -> Result<(), ForgeError> {
        lease.require_fence(&self.core.operator_pool).await?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let result = sqlx::query(
            r"UPDATE vala.file_list
              SET committed_snapshot_id = $1
            WHERE data_tenant_id = $2 AND namespace = $3 AND table_name = $4
              AND partition_granularity = $5 AND partition_start = $6 AND id = ANY($7)
              AND compacted
              AND (committed_snapshot_id IS NULL OR committed_snapshot_id = $1)",
        )
        .bind(snapshot_id)
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition.granularity_str())
        .bind(key.partition.start_utc())
        .bind(input_file_ids)
        .execute(&mut **conn.transaction())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        if result.rows_affected() != u64::try_from(input_file_ids.len()).unwrap_or(u64::MAX) {
            return Err(ForgeError::Reconciliation {
                detail: "recovery transition did not stamp every input file".to_owned(),
            });
        }
        self.append_system_audit(
            &mut conn,
            key,
            "forge.file_compact.recovered",
            Self::terminal_detail(detail, ForgeCompactionPhase::Recovered, Some(snapshot_id)),
        )
        .await?;
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }

    /// Reset a recovered compaction whose output was never committed.
    ///
    /// # Errors
    ///
    /// Returns a lease, SQL, reconciliation, operation-state, audit, or fence
    /// error. Dropping the caller-owned transaction rolls back every mutation.
    async fn reset_reconciled(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        input_file_ids: &[Uuid],
        detail: &AuditDetail,
    ) -> Result<(), ForgeError> {
        lease.require_fence(&self.core.operator_pool).await?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let result = sqlx::query(
            r"UPDATE vala.file_list
              SET compacted = false, committed_snapshot_id = NULL,
                  publication_operation_id = NULL
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_granularity = $4 AND partition_start = $5 AND id = ANY($6)
              AND committed_snapshot_id IS NULL",
        )
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition.granularity_str())
        .bind(key.partition.start_utc())
        .bind(input_file_ids)
        .execute(&mut **conn.transaction())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        if result.rows_affected() != u64::try_from(input_file_ids.len()).unwrap_or(u64::MAX) {
            return Err(ForgeError::Reconciliation {
                detail: "recovery reset did not restore every input file".to_owned(),
            });
        }
        self.append_system_audit(
            &mut conn,
            key,
            "forge.file_compact.reset",
            Self::terminal_detail(detail, ForgeCompactionPhase::Reset, None),
        )
        .await?;
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }

    /// Drive the production recovered-stamping writer from DB integration tests.
    ///
    /// # Errors
    ///
    /// Returns the same lease, SQL, reconciliation, transition, audit, fence,
    /// and commit errors as the production recovery path.
    #[cfg(feature = "test-support")]
    pub async fn stamp_reconciled_for_test(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        partition: TimePartition,
        input_file_ids: &[Uuid],
        detail: &AuditDetail,
        snapshot_id: i64,
    ) -> Result<(), ForgeError> {
        let key = ForgeGroupKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
            partition,
        };
        self.stamp_reconciled(lease, &key, input_file_ids, detail, snapshot_id)
            .await
    }

    /// Drive the production reconciliation-reset writer from DB integration tests.
    ///
    /// # Errors
    ///
    /// Returns the same lease, SQL, reconciliation, transition, audit, fence,
    /// and commit errors as the production reset path.
    #[cfg(feature = "test-support")]
    pub async fn reset_reconciled_for_test(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        partition: TimePartition,
        input_file_ids: &[Uuid],
        detail: &AuditDetail,
    ) -> Result<(), ForgeError> {
        let key = ForgeGroupKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
            partition,
        };
        self.reset_reconciled(lease, &key, input_file_ids, detail)
            .await
    }

    /// Drive the production staging-fold transition writer from integration tests.
    ///
    /// # Errors
    ///
    /// Returns the same lease, SQL, transition, audit, fence, and commit errors
    /// as the production staging-fold transition path.
    #[cfg(feature = "test-support")]
    pub async fn append_compaction_transition_for_test(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        partition: TimePartition,
        detail: AuditDetail,
        operation: &str,
    ) -> Result<(), ForgeError> {
        let key = ForgeGroupKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
            partition,
        };
        lease.require_fence(&self.core.operator_pool).await?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        self.append_system_audit(&mut conn, &key, operation, detail)
            .await?;
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }

    /// Convert a prepared compaction detail into a terminal audit phase.
    fn terminal_detail(
        detail: &AuditDetail,
        phase: ForgeCompactionPhase,
        snapshot_id: Option<i64>,
    ) -> AuditDetail {
        match detail {
            AuditDetail::ForgeCompaction {
                operation_id,
                group,
                input_file_ids,
                input_paths,
                output_paths,
                ..
            } => AuditDetail::ForgeCompaction {
                operation_id: *operation_id,
                phase,
                group: group.clone(),
                input_file_ids: input_file_ids.clone(),
                input_paths: input_paths.clone(),
                output_paths: output_paths.clone(),
                snapshot_id,
            },
            _ => detail.clone(),
        }
    }

    /// Append one staging-fold transition to audit and projection in the caller's transaction.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when event validation, audit persistence, or
    /// the bounded operation-state transition fails.
    async fn append_system_audit(
        &self,
        conn: &mut vala_sql::TenantConn<'_>,
        key: &ForgeGroupKey,
        operation: &str,
        detail: AuditDetail,
    ) -> Result<(), ForgeError> {
        let resource = key.audit_resource();
        let event = forge_transition_event(operation, resource.clone(), detail);
        let operations = ForgeOperations::new(&resource, ForgeOperationFamily::StagingFold)
            .map_err(ForgeError::Sql)?;
        let transition = if operation == "forge.file_compact.prepared" {
            operations.append_prepared(conn, &event).await
        } else {
            operations.append_terminal(conn, &event).await
        }
        .map_err(ForgeError::Sql)?;
        match transition {
            ForgeOperationTransition::Applied { .. }
            | ForgeOperationTransition::AlreadyApplied { .. } => Ok(()),
        }
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

/// Build the canonical audit detail for one compaction operation.
///
/// # Errors
///
/// Returns [`ForgeError::Group`] when the group storage path (built from
/// `key.audit_resource()`) or any output file's storage path fails to
/// validate, and [`ForgeError::Invariant`] when `outputs` is empty, since a
/// compaction audit detail must record at least one output.
fn forge_detail(
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    outputs: &[DataFile],
    operation_id: Uuid,
    phase: ForgeCompactionPhase,
    snapshot_id: Option<i64>,
) -> Result<AuditDetail, ForgeError> {
    let group = StoragePath::new(key.audit_resource()).map_err(|error| ForgeError::Group {
        detail: error.to_string(),
    })?;
    let output_paths = outputs
        .iter()
        .map(|output| {
            StoragePath::new(output.file_path().to_owned()).map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if output_paths.is_empty() {
        return Err(ForgeError::Invariant {
            detail: "Forge audit detail requires at least one output".to_owned(),
        });
    }
    let mut inputs = bin
        .files
        .iter()
        .map(|file| {
            StoragePath::new(file.path.clone())
                .map(|path| (file.id, path))
                .map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    inputs.sort_by_key(|(id, _)| *id);
    let (input_file_ids, input_paths): (Vec<_>, Vec<_>) = inputs.into_iter().unzip();
    Ok(AuditDetail::ForgeCompaction {
        operation_id,
        phase,
        group: group.to_string(),
        input_file_ids,
        input_paths,
        output_paths,
        snapshot_id,
    })
}

/// Derive a stable operation ID from the group and ordered input identity.
fn operation_id(key: &ForgeGroupKey, bin: &RewriteBin, generation: Uuid) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.audit_resource());
    hasher.update(key.partition.granularity_str());
    hasher.update(key.partition.start_unix_micros().to_le_bytes());
    hasher.update(generation.as_bytes());
    for file in &bin.files {
        hasher.update(file.id.as_bytes());
        hasher.update(file.path.as_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    /// Minimal object-store implementation used to exercise trait defaults.
    #[derive(Debug)]
    struct DefaultHookObjectStore;

    #[async_trait]
    impl ForgeObjectStore for DefaultHookObjectStore {
        /// Reject reads because this unit exercises only the default notification.
        async fn read(&self, _path: &str) -> opendal::Result<Buffer> {
            unreachable!("default hook unit does not read objects")
        }

        /// Reject ranged reads because this unit exercises only the default notification.
        async fn read_range(
            &self,
            _path: &str,
            _range: std::ops::Range<u64>,
        ) -> opendal::Result<Buffer> {
            unreachable!("default hook unit does not read object ranges")
        }

        /// Reject listings because this unit exercises only the default notification.
        async fn list(&self, _prefix: &str) -> opendal::Result<Vec<Entry>> {
            unreachable!("default hook unit does not list objects")
        }

        /// Reject metadata reads because this unit exercises only the default notification.
        async fn stat(&self, _path: &str) -> opendal::Result<Metadata> {
            unreachable!("default hook unit does not inspect objects")
        }

        /// Reject deletes because this unit exercises only the default notification.
        async fn delete(&self, _path: &str) -> opendal::Result<()> {
            unreachable!("default hook unit does not delete objects")
        }
    }

    /// The production default post-PUT notification completes without failure or side effects.
    #[tokio::test]
    async fn after_output_put_default_is_infallible_noop() {
        DefaultHookObjectStore
            .after_output_put("durable/output.parquet")
            .await;
    }

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

        let mut direct = store
            .list("")
            .await
            .expect("direct listing")
            .into_iter()
            .map(|entry| entry.path().to_owned())
            .collect::<Vec<_>>();
        direct.sort();

        let mut pages = store.list_pages("").await.expect("paged listing");
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
            "the single page equals the flat listing"
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

    /// Staging folds publish new data files without deleting Iceberg files.
    #[test]
    fn rewrite_action_uses_empty_delete_set_for_staging_fold() {
        let _ = Transaction::new;
        let config = ForgeConfig::default();
        assert!(config.validate().is_ok());
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

    /// Forge rejects a zero planner memory ceiling before composing scheduler or workers.
    #[test]
    fn forge_config_rejects_zero_memory_capacity() {
        let config = ForgeConfig {
            max_memory_bytes: 0,
            ..ForgeConfig::default()
        };
        assert!(config.validate().is_err());
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

    /// Builds the fixture hour partition shared by the staging-planner regressions.
    fn staging_hour() -> TimePartition {
        TimePartition::new(
            crate::catalog::TimeGranularity::Hour,
            DateTime::from_timestamp(1_767_312_000, 0).expect("fixture hour is representable"),
        )
        .expect("fixture hour is an exact hour boundary")
    }

    /// Closed staging debt keeps a healthy singleton actionable beside policy groups.
    #[test]
    fn staging_planner_maps_policy_groups_without_healthy_fillers() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy");
        let day = staging_hour();
        let timestamp = DateTime::from_timestamp(1, 0).expect("timestamp");
        let file = |id: u128, path: &str, size| CandidateFile {
            id: Uuid::from_u128(id),
            path: path.to_owned(),
            size,
            min_event_time: timestamp,
            max_event_time: timestamp,
        };
        let bins = plan_staging_bins(
            &policy,
            &[
                file(1, "small-a", 20),
                file(2, "healthy", 100),
                file(3, "small-b", 20),
            ],
            256,
            day,
            day.end_utc(),
        );
        assert_eq!(bins.len(), 2);
        assert_eq!(
            bins[0]
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["small-a", "small-b"]
        );
        assert_eq!(bins[1].files[0].path, "healthy");
    }

    /// Forge transition writers cannot bypass the operation-state owner.
    ///
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

    /// A closed partition retains a bounded singleton remainder as staging debt.
    #[test]
    fn staging_planner_discards_undersized_singleton_remainder() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy");
        let day = staging_hour();
        let timestamp = DateTime::from_timestamp(1, 0).expect("timestamp");
        let file = |id: u128, path: &str| CandidateFile {
            id: Uuid::from_u128(id),
            path: path.to_owned(),
            size: 20,
            min_event_time: timestamp,
            max_event_time: timestamp,
        };
        let bins = plan_staging_bins(
            &policy,
            &[file(1, "small-a"), file(2, "small-b"), file(3, "small-c")],
            2,
            day,
            day.end_utc(),
        );
        assert_eq!(bins.len(), 2);
        assert_eq!(bins[0].files.len(), 2);
        assert_eq!(bins[0].total_bytes, 40);
        assert_eq!(bins[1].files.len(), 1);
        assert_eq!(bins[1].files[0].path, "small-c");
    }

    /// An accepted open-partition tail creates no actionable bin or pending work.
    #[test]
    fn accepted_open_partition_tail_is_not_pending_work() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy");
        let day = staging_hour();
        let timestamp = DateTime::from_timestamp(1, 0).expect("timestamp");
        let tail = CandidateFile {
            id: Uuid::from_u128(1),
            path: "open-tail".to_owned(),
            size: 20,
            min_event_time: timestamp,
            max_event_time: timestamp,
        };
        let bins = plan_staging_bins(&policy, &[tail], 256, day, day.start_utc());
        assert!(bins.is_empty());
        let outcome = ForgeTickOutcome {
            tables_discovered: 1,
            tables_examined: 1,
            tables_succeeded: 1,
            tick_complete: true,
            ..ForgeTickOutcome::default()
        };
        assert_eq!(outcome.staging_pending_files, 0);
        assert!(!outcome.pending_work);
        assert!(outcome.is_converged());
    }

    /// A closed partition publishes its final staging singleton instead of stranding debt.
    #[test]
    fn closed_partition_staging_singleton_is_actionable() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy");
        let day = staging_hour();
        let timestamp = DateTime::from_timestamp(1, 0).expect("timestamp");
        let tail = CandidateFile {
            id: Uuid::from_u128(1),
            path: "closed-tail".to_owned(),
            size: 20,
            min_event_time: timestamp,
            max_event_time: timestamp,
        };

        let bins = plan_staging_bins(&policy, &[tail], 256, day, day.end_utc());

        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0].files.len(), 1);
        assert_eq!(bins[0].total_bytes, 20);
    }
}
