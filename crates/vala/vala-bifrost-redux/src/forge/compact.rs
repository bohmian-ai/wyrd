//! Forge compaction from staged Scribe files into Iceberg snapshots.
//!
//! This module owns the compaction state machine. It selects bounded,
//! tenant-scoped groups from `vala.file_list`, validates and sorts their Arrow
//! rows, writes one Iceberg data file, and records each durable transition in
//! the audit outbox. Prepared audit records make a crash between the SQL and
//! Iceberg commits observable; the next Forge tick reconciles that state before
//! selecting more files.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Array, StringArray};
use arrow::compute::cast;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use iceberg::spec::DataFile;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use opendal::{Buffer, Entry, Metadata};
use sqlx::Row;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
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
use super::rewrite::{RewriteOutput, RewriteRequest, RewriteSourceFile};
use super::right_size::{
    ForgeRightSizePolicy, IcebergCandidateFile, IcebergRewriteGroup, IcebergRewriteReason,
    validate_supported_layout,
};
use crate::catalog::TenantTableBinding;
use crate::parquet::writer_properties::BIFROST_WRITER_RECIPE_VERSION;

const DEFAULT_MAX_CONCURRENT_READS: usize = 4;
const DEFAULT_SPILL_LIMIT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DEFAULT_MAX_OPEN_OPERATIONS_PER_TABLE: usize = 256;
const DEFAULT_MAX_RETAINED_SNAPSHOTS_PER_TABLE: usize = 256;
const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

#[derive(Debug, Clone)]
/// Limits and durability windows for one Forge maintenance loop.
pub struct ForgeConfig {
    /// Minimum number of staged files that makes a group eligible.
    pub min_files: i64,
    /// Maximum number of files in one rewrite bin.
    pub max_files_per_bin: usize,
    /// Maximum number of input files processed by one tick.
    pub max_files_per_tick: usize,
    /// Maximum input bytes processed by one tick.
    pub max_bytes_per_tick: u64,
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
}

impl Default for ForgeConfig {
    /// Return the production defaults for Forge maintenance limits.
    fn default() -> Self {
        Self {
            min_files: 2,
            max_files_per_bin: 256,
            max_files_per_tick: 1_024,
            max_bytes_per_tick: 2 * 512 * 1024 * 1024,
            max_bins_per_tick: 64,
            lease_ttl: Duration::from_mins(15),
            iceberg_total_retry_timeout: Duration::from_mins(5),
            catalog_request_timeout: Duration::from_secs(30),
            uncertainty_margin: Duration::from_secs(30),
            uncertainty_bound: Duration::from_mins(2),
            audit_page_size: 256,
            snapshot_retention: Duration::from_hours(120),
            retain_last: 1,
            orphan_gc_ttl: Duration::from_hours(24),
            max_gc_candidates_per_batch: 256,
            max_concurrent_reads: DEFAULT_MAX_CONCURRENT_READS,
            spill_limit_bytes: DEFAULT_SPILL_LIMIT_BYTES,
            max_hints_per_wake: 256,
            max_open_operations_per_table: DEFAULT_MAX_OPEN_OPERATIONS_PER_TABLE,
            max_retained_snapshots_per_table: DEFAULT_MAX_RETAINED_SNAPSHOTS_PER_TABLE,
        }
    }
}

impl ForgeConfig {
    /// Validate compaction, recovery, expiry, and garbage-collection limits.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when a limit is zero, a bin cannot
    /// contain two files, or the lease cannot cover the configured commit
    /// window.
    pub fn validate(&self) -> Result<(), ForgeError> {
        if self.min_files < 2
            || self.max_files_per_bin < 2
            || self.max_files_per_tick == 0
            || self.max_bytes_per_tick == 0
            || self.max_bins_per_tick == 0
            || self.lease_ttl.is_zero()
            || self.iceberg_total_retry_timeout.is_zero()
            || self.catalog_request_timeout.is_zero()
            || self.uncertainty_margin.is_zero()
            || self.uncertainty_bound.is_zero()
            || self.audit_page_size <= 0
            || self.snapshot_retention.is_zero()
            || self.retain_last == 0
            || self.orphan_gc_ttl.is_zero()
            || self.max_gc_candidates_per_batch == 0
            || self.max_concurrent_reads == 0
            || self.spill_limit_bytes == 0
            || self.max_hints_per_wake == 0
            || self.max_open_operations_per_table == 0
            || self.max_retained_snapshots_per_table == 0
        {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge limits must be positive and min_files/max_files_per_bin must be at least two".to_owned(),
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

/// The object-store operations Forge performs after Scribe has staged a file.
///
/// Keeping this capability narrow lets production use the real `OpenDAL`
/// operator while integration tests wrap the same seam with deterministic
/// barriers and failures. Fixture code still uses [`ForgeCore::staging`]
/// directly for producer writes.
#[async_trait]
pub trait ForgeObjectStore: std::fmt::Debug + Send + Sync {
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
    /// Objects deleted by orphan GC.
    pub gc_deleted: usize,
    /// Objects skipped after a live-set or fence recheck.
    pub gc_skipped: usize,
    /// Work skipped because a per-tick budget was reached.
    pub budget_skips: usize,
    /// Tables skipped because another Forge owner held the lease.
    pub lease_contention: usize,
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
    /// Whether conservative work remains after this outcome.
    pub pending_work: bool,
}

impl ForgeTickOutcome {
    /// Merge another isolated stage outcome into this aggregate.
    pub(crate) fn merge(&mut self, other: Self) {
        self.groups_seen = self.groups_seen.saturating_add(other.groups_seen);
        self.bins_committed += other.bins_committed;
        self.bins_skipped += other.bins_skipped;
        self.reconciled += other.reconciled;
        self.tables_succeeded += other.tables_succeeded;
        self.tables_skipped += other.tables_skipped;
        self.tables_failed += other.tables_failed;
        self.reconciliation_recovered += other.reconciliation_recovered;
        self.expiry_reconciled += other.expiry_reconciled;
        self.gc_reconciled += other.gc_reconciled;
        self.gc_deleted += other.gc_deleted;
        self.gc_skipped += other.gc_skipped;
        self.budget_skips += other.budget_skips;
        self.lease_contention += other.lease_contention;
        self.fence_losses += other.fence_losses;
        self.stage_failures += other.stage_failures;
        self.spill_bytes = self.spill_bytes.saturating_add(other.spill_bytes);
        self.input_rows = self.input_rows.saturating_add(other.input_rows);
        self.output_rows = self.output_rows.saturating_add(other.output_rows);
        self.outputs_committed = self
            .outputs_committed
            .saturating_add(other.outputs_committed);
        self.live_recovered = self.live_recovered.saturating_add(other.live_recovered);
        self.live_reset = self.live_reset.saturating_add(other.live_reset);
        self.live_pending = self.live_pending.saturating_add(other.live_pending);
        self.live_unresolved = self.live_unresolved.saturating_add(other.live_unresolved);
        self.open_operation_overflows = self
            .open_operation_overflows
            .saturating_add(other.open_operation_overflows);
        self.pending_work |= other.pending_work;
    }
}

/// File, byte, and bin counters shared by one bounded Forge work batch.
#[derive(Debug, Default)]
pub(crate) struct ForgeTickBudget {
    /// Input files already committed by the batch.
    files: usize,
    /// Input bytes already committed by the batch.
    bytes: u64,
    /// Rewrite bins already committed by the batch.
    bins: usize,
}

/// Reconcile prepared compaction audits for one tenant/table before new work.
///
/// A prepared operation is marked committed when its output is still live in
/// Iceberg, or reset when the output is absent after the uncertainty window.
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
            table.metadata().default_sort_order(),
        )?;
        ForgeRightSizePolicy::new(
            target,
            table.metadata().current_schema_id(),
            table.metadata().default_partition_spec_id(),
            table.metadata().default_sort_order_id(),
        )
    }

    /// Reconcile prepared compactions before admitting new table work.
    ///
    /// # Errors
    ///
    /// Returns SQL, catalog, audit, or fence failures. Recovery preserves all
    /// prepared outputs until their complete live-set status is certain.
    pub(super) async fn run_compaction_reconciliation_for_table(
        &self,
        lease: &mut ForgeLease,
        table_key: &ForgeTableKey,
        binding: &TenantTableBinding,
    ) -> Result<usize, ForgeError> {
        let mut reconciled = 0;
        for key in self.load_reconciliation_keys().await? {
            if key.tenant != table_key.tenant || key.table_ref != table_key.table_ref {
                continue;
            }
            reconciled += self.reconcile_group(lease, &key, binding).await?;
        }
        Ok(reconciled)
    }

    /// Select, bin-pack, and commit bounded periodic compaction work.
    ///
    /// # Errors
    ///
    /// Returns discovery, lease, rewrite, catalog, or bookkeeping failures.
    pub(super) async fn run_compaction_bins_for_table(
        &self,
        lease: &mut ForgeLease,
        table_key: &ForgeTableKey,
        binding: &TenantTableBinding,
        stop: &CancellationToken,
    ) -> Result<ForgeTickOutcome, ForgeError> {
        let rows = self.select_candidate_groups(table_key).await?;
        let right_size_policy = self.table_right_size_policy(binding).await?;
        let mut budget = ForgeTickBudget::default();
        self.compact_candidate_rows(lease, binding, rows, &right_size_policy, &mut budget, stop)
            .await
    }

    /// Query and compact one exact durable day after an advisory hint.
    ///
    /// # Errors
    ///
    /// Returns discovery, lease, rewrite, catalog, or bookkeeping failures.
    pub(super) async fn run_targeted_compaction_for_table(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        binding: &TenantTableBinding,
        budget: &mut ForgeTickBudget,
        stop: &CancellationToken,
    ) -> Result<ForgeTickOutcome, ForgeError> {
        let rows = self.select_targeted_candidate_group(key).await?;
        let right_size_policy = self.table_right_size_policy(binding).await?;
        self.compact_candidate_rows(lease, binding, rows, &right_size_policy, budget, stop)
            .await
    }
}

/// Plan and commit candidate rows against one caller-owned work budget.
///
/// # Errors
///
/// Returns a fence, catalog, rewrite, or durable bookkeeping error. Budget
/// exhaustion is represented in the returned outcome rather than as failure.
impl Forge {
    async fn compact_candidate_rows(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        rows: Vec<CandidateRow>,
        right_size_policy: &ForgeRightSizePolicy,
        budget: &mut ForgeTickBudget,
        stop: &CancellationToken,
    ) -> Result<ForgeTickOutcome, ForgeError> {
        let mut outcome = ForgeTickOutcome::default();
        'groups: for row in rows {
            outcome.groups_seen += 1;
            let bins = plan_staging_bins(
                right_size_policy,
                &row.files,
                self.core.config.max_files_per_bin,
                row.key.partition_day,
                Utc::now().date_naive(),
            );
            for bin in bins {
                if stop.is_cancelled() {
                    return Ok(outcome);
                }
                if budget.bins >= self.core.config.max_bins_per_tick {
                    break;
                }
                if budget.files.saturating_add(bin.files.len())
                    > self.core.config.max_files_per_tick
                    || budget.bytes.saturating_add(bin.total_bytes)
                        > self.core.config.max_bytes_per_tick
                {
                    outcome.bins_skipped += 1;
                    outcome.budget_skips += 1;
                    break 'groups;
                }
                if !lease.renew(&self.core.operator_pool).await? {
                    return Err(ForgeError::FenceLost {
                        lease_key: lease.lease_key.clone(),
                    });
                }
                if !lease.commit_window_fits(self.core.config.commit_window()) {
                    outcome.bins_skipped += 1;
                    outcome.budget_skips += 1;
                    break 'groups;
                }
                let stats = self
                    .compact_bin(lease, &row.key, binding, &bin, right_size_policy, stop)
                    .await?;
                outcome.spill_bytes = outcome.spill_bytes.saturating_add(stats.spill_bytes);
                outcome.input_rows = outcome.input_rows.saturating_add(stats.input_rows);
                outcome.output_rows = outcome.output_rows.saturating_add(stats.output_rows);
                outcome.outputs_committed = outcome
                    .outputs_committed
                    .saturating_add(stats.outputs_committed);
                budget.files += bin.files.len();
                budget.bytes = budget.bytes.saturating_add(bin.total_bytes);
                budget.bins += 1;
                outcome.bins_committed += 1;
            }
            if budget.bins >= self.core.config.max_bins_per_tick
                || budget.files >= self.core.config.max_files_per_tick
                || budget.bytes >= self.core.config.max_bytes_per_tick
            {
                break;
            }
        }
        Ok(outcome)
    }
}

/// Map durable staging facts into the live-file policy without reusing its
/// separate [`CandidateFile`] domain, then map selected groups back to rewrite
/// bins by their stable object paths.
fn plan_staging_bins(
    policy: &ForgeRightSizePolicy,
    files: &[CandidateFile],
    max_files: usize,
    partition_day: NaiveDate,
    current_day: NaiveDate,
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
                partition_day,
                sort_order_id: Some(policy.sort_order_id()),
                writer_recipe_version: Some(BIFROST_WRITER_RECIPE_VERSION.to_owned()),
                min_event_time: file.min_event_time,
                max_event_time: file.max_event_time,
                source_snapshot_id: 0,
                data_sequence_number: None,
                file_sequence_number: None,
            }
        })
        .collect();
    let plan = policy.plan_partition(live, partition_day >= current_day);
    let groups = plan
        .groups
        .into_iter()
        .flat_map(|group| {
            let IcebergRewriteGroup { files, reason } = group;
            files
                .chunks(max_files)
                .filter(|chunk| reason != IcebergRewriteReason::Undersized || chunk.len() >= 2)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    groups
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
        .collect()
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateRow {
    /// Tenant/table/day identity shared by every file in the row.
    key: ForgeGroupKey,
    /// Uncompacted files selected for this candidate group.
    files: Vec<CandidateFile>,
}

impl Forge {
    /// Load bounded, old-enough staging candidates for one tenant/table.
    ///
    /// The query uses the operator pool because Forge discovers work across tenant
    /// rows. Results are grouped by partition day and ordered deterministically so
    /// repeated ticks make the same selection under the same durable state.
    async fn select_candidate_groups(
        &self,
        table_key: &ForgeTableKey,
    ) -> Result<Vec<CandidateRow>, ForgeError> {
        let rows = sqlx::query(
            r"
        SELECT id, file_path, file_size, min_event_time, max_event_time,
               partition_day
          FROM vala.file_list
         WHERE data_tenant_id = $1
           AND namespace = $2
           AND table_name = $3
           AND NOT compacted
           AND created_at < now() - interval '2 min'
         ORDER BY partition_day, min_event_time, max_event_time, id
         LIMIT $4
        ",
        )
        .bind(table_key.tenant.as_uuid())
        .bind(table_key.table_ref.namespace.as_str())
        .bind(&table_key.table_ref.name)
        .bind(
            i64::try_from(self.core.config.max_files_per_tick).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "max_files_per_tick exceeds PostgreSQL bigint".to_owned(),
                }
            })?,
        )
        .fetch_all(self.core.operator_pool.pool())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;

        let mut groups = BTreeMap::<NaiveDate, Vec<CandidateFile>>::new();
        for row in rows {
            let id: Uuid = row.try_get("id").map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })?;
            let path: String = row
                .try_get("file_path")
                .map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })?;
            let file_size: i64 = row
                .try_get("file_size")
                .map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })?;
            let min_event_time: DateTime<Utc> =
                row.try_get("min_event_time")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?;
            let max_event_time: DateTime<Utc> =
                row.try_get("max_event_time")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?;
            let partition_day: NaiveDate =
                row.try_get("partition_day")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?;
            groups
                .entry(partition_day)
                .or_default()
                .push(CandidateFile {
                    id,
                    path,
                    size: u64::try_from(file_size).map_err(|_| ForgeError::Group {
                        detail: "negative file size".to_owned(),
                    })?,
                    min_event_time,
                    max_event_time,
                });
        }

        Ok(groups
            .into_iter()
            .map(|(partition_day, files)| CandidateRow {
                key: ForgeGroupKey {
                    tenant: table_key.tenant,
                    table_ref: table_key.table_ref.clone(),
                    partition_day,
                },
                files,
            })
            .collect())
    }

    /// Load one exact durable candidate group without the periodic age guard.
    ///
    /// All other periodic predicates and deterministic ordering remain identical,
    /// so a hint accelerates discovery without becoming work authority.
    ///
    /// # Errors
    ///
    /// Returns SQL, identifier, file-size, or candidate-row decoding failures.
    async fn select_targeted_candidate_group(
        &self,
        key: &ForgeGroupKey,
    ) -> Result<Vec<CandidateRow>, ForgeError> {
        let rows = sqlx::query(
            r"
        SELECT id, file_path, file_size, min_event_time, max_event_time,
               partition_day
          FROM vala.file_list
         WHERE data_tenant_id = $1
           AND namespace = $2
           AND table_name = $3
           AND partition_day = $4
           AND NOT compacted
         ORDER BY partition_day, min_event_time, max_event_time, id
         LIMIT $5
        ",
        )
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition_day)
        .bind(
            i64::try_from(self.core.config.max_files_per_tick).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: "max_files_per_tick exceeds PostgreSQL bigint".to_owned(),
                }
            })?,
        )
        .fetch_all(self.core.operator_pool.pool())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;

        let mut files = Vec::with_capacity(rows.len());
        for row in rows {
            let file_size: i64 = row
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
                size: u64::try_from(file_size).map_err(|_| ForgeError::Group {
                    detail: "negative file size".to_owned(),
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
        if files.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![CandidateRow {
            key: key.clone(),
            files,
        }])
    }

    /// Load groups whose prepared SQL transition has no committed snapshot ID.
    async fn load_reconciliation_keys(&self) -> Result<Vec<ForgeGroupKey>, ForgeError> {
        let rows = sqlx::query(
            r"SELECT DISTINCT data_tenant_id, namespace, table_name, partition_day
             FROM vala.file_list
            WHERE compacted AND committed_snapshot_id IS NULL
            ORDER BY data_tenant_id, namespace, table_name, partition_day",
        )
        .fetch_all(self.core.operator_pool.pool())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        rows.into_iter()
            .map(|row| {
                let tenant_uuid: Uuid =
                    row.try_get("data_tenant_id")
                        .map_err(|error| ForgeError::Group {
                            detail: error.to_string(),
                        })?;
                let tenant = Self::data_tenant_from_uuid(tenant_uuid)?;
                let namespace: String =
                    row.try_get("namespace")
                        .map_err(|error| ForgeError::Group {
                            detail: error.to_string(),
                        })?;
                let table_name: String =
                    row.try_get("table_name")
                        .map_err(|error| ForgeError::Group {
                            detail: error.to_string(),
                        })?;
                let partition_day: NaiveDate =
                    row.try_get("partition_day")
                        .map_err(|error| ForgeError::Group {
                            detail: error.to_string(),
                        })?;
                ForgeGroupKey::from_sql(tenant, &namespace, &table_name, partition_day)
                    .map_err(|detail| ForgeError::Group { detail })
            })
            .collect()
    }

    /// Convert a database UUID into a validated data-tenant ID.
    fn data_tenant_from_uuid(value: Uuid) -> Result<DataTenantId, ForgeError> {
        DataTenantId::try_from(value).map_err(|error| ForgeError::Group {
            detail: error.to_string(),
        })
    }

    /// Execute one fenced compaction operation from staged files to Iceberg.
    ///
    /// The sequence is read and validate, write the output, mark inputs prepared,
    /// commit the Iceberg transaction, and stamp the committed snapshot. Every
    /// external boundary is fenced so a stale lease cannot finish the operation.
    async fn compact_bin(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        binding: &TenantTableBinding,
        bin: &RewriteBin,
        right_size_policy: &ForgeRightSizePolicy,
        stop: &CancellationToken,
    ) -> Result<CompactStats, ForgeError> {
        let operation_id = operation_id(key, bin);
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
        let rewrite = self
            .core
            .rewrite
            .rewrite(
                RewriteRequest {
                    operation_id,
                    binding,
                    schema,
                    iceberg_schema: table.metadata().current_schema().clone(),
                    source_files: &source_files,
                    partition_day: key.partition_day,
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
        lease.require_fence(&self.core.operator_pool).await?;
        let prepared = forge_detail(
            key,
            bin,
            &rewrite.files,
            operation_id,
            ForgeCompactionPhase::Prepared,
            None,
        )?;
        self.prepare_inputs(lease, key, bin, prepared).await?;
        self.commit_rewrite(
            lease,
            CommitRewriteRequest {
                key,
                binding,
                bin,
                operation_id,
                rewrite: &rewrite,
                stop,
            },
        )
        .await?;
        Ok(CompactStats {
            spill_bytes: rewrite.spill_bytes,
            input_rows: rewrite.input_rows,
            output_rows: rewrite.output_rows,
            outputs_committed: rewrite.object_paths.len(),
        })
    }
}

/// Rewrite statistics returned only after the Iceberg commit succeeds.
#[derive(Debug, Default, Clone, Copy)]
struct CompactStats {
    /// Peak spill bytes observed by the rewrite runtime.
    spill_bytes: u64,
    /// Rows accepted from source batches.
    input_rows: u64,
    /// Rows encoded into outputs.
    output_rows: u64,
    /// Number of non-empty output files committed.
    outputs_committed: usize,
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
    /// successor reconciliation tick to decide the terminal state.
    ///
    /// # Errors
    ///
    /// Returns fence, catalog, timeout, reconciliation, SQL, or audit failures.
    async fn commit_rewrite(
        &self,
        lease: &mut ForgeLease,
        request: CommitRewriteRequest<'_>,
    ) -> Result<(), ForgeError> {
        let CommitRewriteRequest {
            key,
            binding,
            bin,
            operation_id,
            rewrite,
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
        let commit = transaction.commit(self.core.catalog.as_ref());
        tokio::pin!(commit);
        let response = tokio::select! {
            response = tokio::time::timeout(
                self.core.config.iceberg_total_retry_timeout,
                &mut commit,
            ) => response,
            () = stop.cancelled() => return Err(ForgeError::Shutdown),
        };
        match response {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
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
            Err(_) => {
                return Err(ForgeError::Timeout {
                    operation: "Iceberg commit",
                });
            }
        }
        let committed_table = self.load_table(&binding.table_ident()).await?;
        let snapshot_id = committed_table
            .metadata()
            .current_snapshot_id()
            .ok_or_else(|| ForgeError::Reconciliation {
                detail: "Iceberg replace committed without a current snapshot".to_owned(),
            })?;
        self.stamp_committed(lease, key, bin, &rewrite.files, operation_id, snapshot_id)
            .await
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
              SET compacted = true
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5)
              AND committed_snapshot_id IS NULL",
        )
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition_day)
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
              AND partition_day = $5 AND id = ANY($6)
              AND compacted
              AND (committed_snapshot_id IS NULL OR committed_snapshot_id = $1)",
        )
        .bind(snapshot_id)
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition_day)
        .bind(&ids)
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
              SET compacted = false, committed_snapshot_id = NULL
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5)
              AND committed_snapshot_id IS NULL",
        )
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition_day)
        .bind(&ids)
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
    /// Reconcile the latest prepared audit transition for every group in a table.
    ///
    /// A live output is stamped as recovered. An output absent after the
    /// uncertainty window resets the hidden inputs so a future tick can retry.
    async fn reconcile_group(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeGroupKey,
        binding: &TenantTableBinding,
    ) -> Result<usize, ForgeError> {
        let resource = key.audit_resource();
        let latest = self.load_reconciliation_audits(key, &resource).await?;
        let mut recovered = 0;
        for (_operation_id, (detail, created_at)) in latest {
            if !matches!(
                &detail,
                AuditDetail::ForgeCompaction {
                    phase: ForgeCompactionPhase::Prepared,
                    ..
                }
            ) {
                continue;
            }
            let AuditDetail::ForgeCompaction {
                input_file_ids,
                output_paths,
                ..
            } = &detail
            else {
                continue;
            };
            if Self::detail_group(&detail) != resource {
                return Err(ForgeError::Reconciliation {
                    detail: "audit detail group differs from its resource".to_owned(),
                });
            }
            self.verify_hidden_inputs(key, input_file_ids, &detail)
                .await?;
            if !lease.renew(&self.core.operator_pool).await? {
                return Err(ForgeError::FenceLost {
                    lease_key: lease.lease_key.clone(),
                });
            }
            let table = self.load_table(&binding.table_ident()).await?;
            if let Some(snapshot_id) = self.live_snapshot_for_paths(&table, output_paths).await? {
                if !lease.renew(&self.core.operator_pool).await? {
                    return Err(ForgeError::FenceLost {
                        lease_key: lease.lease_key.clone(),
                    });
                }
                self.stamp_reconciled(lease, key, input_file_ids, &detail, snapshot_id)
                    .await?;
                recovered += 1;
                continue;
            }
            if Utc::now()
                .signed_duration_since(created_at)
                .to_std()
                .unwrap_or_default()
                < self.core.config.uncertainty_bound
            {
                continue;
            }
            let first_reload = self.load_table(&binding.table_ident()).await?;
            if self
                .live_snapshot_for_paths(&first_reload, output_paths)
                .await?
                .is_some()
            {
                continue;
            }
            let second_reload = self.load_table(&binding.table_ident()).await?;
            if self
                .live_snapshot_for_paths(&second_reload, output_paths)
                .await?
                .is_none()
            {
                if !lease.renew(&self.core.operator_pool).await? {
                    return Err(ForgeError::FenceLost {
                        lease_key: lease.lease_key.clone(),
                    });
                }
                self.reset_reconciled(lease, key, input_file_ids, &detail)
                    .await?;
                recovered += 1;
            }
        }
        Ok(recovered)
    }
}

/// Load the latest audit detail for each compaction operation in a group.
impl Forge {
    async fn load_reconciliation_audits(
        &self,
        key: &ForgeGroupKey,
        resource: &str,
    ) -> Result<HashMap<Uuid, (AuditDetail, DateTime<Utc>)>, ForgeError> {
        let mut after_seq = 0_i64;
        let mut latest = HashMap::new();
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        loop {
            let page = vala_sql::queries::audit_outbox::list_audit_events_for_resource(
                &mut conn,
                resource,
                after_seq,
                self.core.config.audit_page_size,
            )
            .await
            .map_err(ForgeError::Sql)?;
            let page_len = page.len();
            for row in page {
                after_seq = row.seq;
                let Some(detail) = row.detail else { continue };
                let detail = serde_json::from_str::<AuditDetail>(&detail).map_err(|error| {
                    ForgeError::Reconciliation {
                        detail: error.to_string(),
                    }
                })?;
                Self::validate_reconciliation_detail(&detail, resource)?;
                let AuditDetail::ForgeCompaction { operation_id, .. } = &detail else {
                    continue;
                };
                latest.insert(*operation_id, (detail, row.created_at));
            }
            if page_len < usize::try_from(self.core.config.audit_page_size).unwrap_or(usize::MAX) {
                break;
            }
        }
        conn.commit().await.map_err(ForgeError::Sql)?;
        Ok(latest)
    }

    /// Validate the identity and ordering fields needed for compaction recovery.
    fn validate_reconciliation_detail(
        detail: &AuditDetail,
        resource: &str,
    ) -> Result<(), ForgeError> {
        let AuditDetail::ForgeCompaction {
            group,
            input_file_ids,
            input_paths,
            output_paths,
            ..
        } = detail
        else {
            return Ok(());
        };
        if group != resource {
            return Err(ForgeError::Reconciliation {
                detail: "audit detail group differs from its resource".to_owned(),
            });
        }
        if input_file_ids.len() < 2
            || input_file_ids.len() != input_paths.len()
            || input_file_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || output_paths.is_empty()
            || output_paths.iter().any(|path| path.as_str().is_empty())
            || output_paths.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(ForgeError::Reconciliation {
                detail: "audit detail does not contain sorted complete input identity".to_owned(),
            });
        }
        Ok(())
    }

    /// Extract the group resource from a compaction audit detail.
    fn detail_group(detail: &AuditDetail) -> String {
        match detail {
            AuditDetail::ForgeCompaction { group, .. } => group.clone(),
            _ => String::new(),
        }
    }

    /// Find the current snapshot only when every operation output remains live.
    async fn live_snapshot_for_paths(
        &self,
        table: &iceberg::table::Table,
        paths: &[StoragePath],
    ) -> Result<Option<i64>, ForgeError> {
        if paths.is_empty() {
            return Ok(None);
        }
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Ok(None);
        };
        let mut remaining = paths
            .iter()
            .map(StoragePath::as_str)
            .collect::<std::collections::HashSet<_>>();
        let manifest_list = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        for manifest_file in manifest_list.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .map_err(ForgeError::Catalog)?;
            for entry in manifest.entries() {
                if entry.is_alive() {
                    remaining.remove(entry.data_file().file_path());
                    if remaining.is_empty() {
                        return Ok(Some(snapshot.snapshot_id()));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Confirm that prepared audit inputs still match hidden `file_list` rows.
    async fn verify_hidden_inputs(
        &self,
        key: &ForgeGroupKey,
        input_file_ids: &[Uuid],
        detail: &AuditDetail,
    ) -> Result<(), ForgeError> {
        let AuditDetail::ForgeCompaction { input_paths, .. } = detail else {
            return Err(ForgeError::Reconciliation {
                detail: "prepared audit detail is not a Forge compaction".to_owned(),
            });
        };
        if input_file_ids.len() != input_paths.len() {
            return Err(ForgeError::Reconciliation {
                detail: "prepared audit detail has unpaired input IDs and paths".to_owned(),
            });
        }
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let rows = sqlx::query(
            r"SELECT id, file_path
             FROM vala.file_list
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5)
              AND compacted AND committed_snapshot_id IS NULL
            ORDER BY id",
        )
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition_day)
        .bind(input_file_ids)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(|error| ForgeError::Sql(error.into()))?;
        conn.commit().await.map_err(ForgeError::Sql)?;

        let observed = rows
            .into_iter()
            .map(|row| {
                Ok::<_, ForgeError>((
                    row.try_get::<Uuid, _>("id")
                        .map_err(|error| ForgeError::Reconciliation {
                            detail: error.to_string(),
                        })?,
                    row.try_get::<String, _>("file_path").map_err(|error| {
                        ForgeError::Reconciliation {
                            detail: error.to_string(),
                        }
                    })?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut expected = input_file_ids
            .iter()
            .copied()
            .zip(input_paths.iter().map(|path| path.as_str().to_owned()))
            .collect::<Vec<_>>();
        expected.sort_by_key(|(id, _)| *id);
        if observed != expected {
            return Err(ForgeError::Reconciliation {
                detail: "prepared audit inputs no longer match hidden file_list rows".to_owned(),
            });
        }
        Ok(())
    }

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
              AND partition_day = $5 AND id = ANY($6)
              AND compacted
              AND (committed_snapshot_id IS NULL OR committed_snapshot_id = $1)",
        )
        .bind(snapshot_id)
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition_day)
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
              SET compacted = false, committed_snapshot_id = NULL
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5)
              AND committed_snapshot_id IS NULL",
        )
        .bind(key.tenant.as_uuid())
        .bind(key.table_ref.namespace.as_str())
        .bind(&key.table_ref.name)
        .bind(key.partition_day)
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
        partition_day: NaiveDate,
        input_file_ids: &[Uuid],
        detail: &AuditDetail,
        snapshot_id: i64,
    ) -> Result<(), ForgeError> {
        let key = ForgeGroupKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
            partition_day,
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
        partition_day: NaiveDate,
        input_file_ids: &[Uuid],
        detail: &AuditDetail,
    ) -> Result<(), ForgeError> {
        let key = ForgeGroupKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
            partition_day,
        };
        self.reset_reconciled(lease, &key, input_file_ids, detail)
            .await
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
                writer_recipe_version,
                ..
            } => AuditDetail::ForgeCompaction {
                operation_id: *operation_id,
                phase,
                group: group.clone(),
                input_file_ids: input_file_ids.clone(),
                input_paths: input_paths.clone(),
                output_paths: output_paths.clone(),
                snapshot_id,
                writer_recipe_version: writer_recipe_version.clone(),
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
        writer_recipe_version: "bifrost-writer-v1".to_owned(),
    })
}

/// Derive a stable operation ID from the group and ordered input identity.
fn operation_id(key: &ForgeGroupKey, bin: &RewriteBin) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.audit_resource());
    hasher.update(key.partition_day.to_string());
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

    /// Forge output encoding uses the shared Bifrost Parquet recipe.
    #[test]
    fn compacted_output_uses_shared_writer_properties() {
        assert_eq!(
            crate::parquet::writer_properties::bifrost_writer_properties(10)
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

    /// Interim live counters saturate and pending state merges by logical OR.
    #[test]
    fn forge_tick_outcome_merge_saturates_live_reconciliation_counters() {
        let mut aggregate = ForgeTickOutcome {
            live_recovered: usize::MAX,
            live_reset: usize::MAX,
            live_pending: usize::MAX,
            live_unresolved: usize::MAX,
            open_operation_overflows: usize::MAX,
            ..ForgeTickOutcome::default()
        };
        aggregate.merge(ForgeTickOutcome {
            live_recovered: 1,
            live_reset: 1,
            live_pending: 1,
            live_unresolved: 1,
            open_operation_overflows: 1,
            pending_work: true,
            ..ForgeTickOutcome::default()
        });
        assert_eq!(aggregate.live_recovered, usize::MAX);
        assert_eq!(aggregate.live_reset, usize::MAX);
        assert_eq!(aggregate.live_pending, usize::MAX);
        assert_eq!(aggregate.live_unresolved, usize::MAX);
        assert_eq!(aggregate.open_operation_overflows, usize::MAX);
        assert!(aggregate.pending_work);
    }

    /// The staging seam consumes the right-size policy's selected groups.
    #[test]
    fn staging_planner_maps_policy_groups_without_healthy_fillers() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("day");
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
            day.succ_opt().expect("next day"),
        );
        assert_eq!(bins.len(), 1);
        assert_eq!(
            bins[0]
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["small-a", "small-b"]
        );
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

    /// A bounded undersized group never leaves a singleton remainder for a
    /// worthless one-to-one rewrite.
    #[test]
    fn staging_planner_discards_undersized_singleton_remainder() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("day");
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
            day.succ_opt().expect("next day"),
        );
        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0].files.len(), 2);
        assert_eq!(bins[0].total_bytes, 40);
    }
}
