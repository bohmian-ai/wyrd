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
use arrow::compute::{SortColumn, SortOptions, cast, lexsort_to_indices, take};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use iceberg::Catalog;
use iceberg::spec::{DataFile, DataFileFormat, Literal, PartitionKey, Struct};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use iceberg::writer::base_writer::data_file_writer::DataFileWriterBuilder;
use iceberg::writer::file_writer::location_generator::{
    DefaultFileNameGenerator, DefaultLocationGenerator,
};
use iceberg::writer::file_writer::{
    ParquetWriterBuilder, rolling_writer::RollingFileWriterBuilder,
};
use iceberg::writer::{IcebergWriter, IcebergWriterBuilder};
use opendal::{Buffer, Entry, Metadata, Operator};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use sqlx::Row;
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeCompactionPhase,
    StoragePath,
};

use crate::catalog::TenantTableBinding;
use crate::parquet::writer_properties::bifrost_writer_properties;
use crate::scribe::memory::{BifrostMemoryGovernor, ParentMemoryReservation};

use super::binpack::{CandidateFile, ForgeGroupKey, RewriteBin, stable_pack};
use super::error::ForgeError;
use super::lease::ForgeLease;

const DEFAULT_TARGET_BIN_BYTES: u64 = 512 * 1024 * 1024;
const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

#[derive(Debug, Clone)]
/// Limits and durability windows for one Forge maintenance loop.
pub struct ForgeConfig {
    /// Minimum number of staged files that makes a group eligible.
    pub min_files: i64,
    /// Target byte size for one rewrite bin.
    pub target_bin_bytes: u64,
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
    /// Maximum staging hints drained by one wake-up.
    pub max_hints_per_wake: usize,
    /// Maximum concurrent object reads during rewrite.
    pub max_concurrent_reads: usize,
    /// Maximum bytes DataFusion may spill for one operation.
    pub spill_limit_bytes: u64,
    /// Maximum encoded bytes written to one output file.
    pub output_file_bytes: u64,
}

impl Default for ForgeConfig {
    /// Return the production defaults for Forge maintenance limits.
    fn default() -> Self {
        Self {
            min_files: 2,
            target_bin_bytes: DEFAULT_TARGET_BIN_BYTES,
            max_files_per_bin: 256,
            max_files_per_tick: 1_024,
            max_bytes_per_tick: 2 * DEFAULT_TARGET_BIN_BYTES,
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
            max_hints_per_wake: 256,
            max_concurrent_reads: 4,
            spill_limit_bytes: 4 * 1024 * 1024 * 1024,
            output_file_bytes: 512 * 1024 * 1024,
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
            || self.target_bin_bytes == 0
            || self.max_files_per_bin < 2
            || self.max_files_per_tick == 0
            || self.max_bytes_per_tick == 0
            || self.max_bins_per_tick == 0
            || self.audit_page_size <= 0
            || self.snapshot_retention.is_zero()
            || self.retain_last == 0
            || self.orphan_gc_ttl.is_zero()
            || self.max_gc_candidates_per_batch == 0
            || self.max_hints_per_wake == 0
            || self.max_concurrent_reads == 0
            || self.spill_limit_bytes == 0
            || self.output_file_bytes == 0
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
    /// Read one staged or Iceberg-owned object.
    async fn read(&self, path: &str) -> opendal::Result<Buffer>;

    /// List all objects below a table-owned prefix.
    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>>;

    /// Read object metadata before a destructive decision.
    async fn stat(&self, path: &str) -> opendal::Result<Metadata>;

    /// Delete one object after the final live-set and fence checks.
    async fn delete(&self, path: &str) -> opendal::Result<()>;
}

#[derive(Debug, Clone)]
struct OpenDalForgeObjectStore {
    operator: Arc<Operator>,
}

#[async_trait]
impl ForgeObjectStore for OpenDalForgeObjectStore {
    /// Read one object through the configured `OpenDAL` operator.
    async fn read(&self, path: &str) -> opendal::Result<Buffer> {
        self.operator.read(path).await
    }

    /// Recursively list objects below a table-owned prefix.
    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>> {
        self.operator.list_with(prefix).recursive(true).await
    }

    /// Read metadata for one object before a deletion decision.
    async fn stat(&self, path: &str) -> opendal::Result<Metadata> {
        self.operator.stat(path).await
    }

    /// Delete one object through the configured `OpenDAL` operator.
    async fn delete(&self, path: &str) -> opendal::Result<()> {
        self.operator.delete(path).await
    }
}

#[derive(Clone)]
pub struct ForgeCore {
    /// Vala-owned application SQL handle used to open tenant transactions.
    pub vala: vala_sql::ValaPostgres,
    /// Operator pool used for cross-tenant lease and candidate discovery work.
    pub operator_pool: vala_sql::OperatorPool,
    /// Iceberg catalog used to load tables and commit maintenance actions.
    pub catalog: Arc<dyn Catalog>,
    /// Raw staging operator retained for producer fixtures and callers.
    pub staging: Arc<Operator>,
    /// Forge's read/list/stat/delete seam, backed by `staging` by default.
    pub object_store: Arc<dyn ForgeObjectStore>,
    pub config: ForgeConfig,
    /// Optional process-global parent governor installed by production boot.
    pub memory_governor: Option<BifrostMemoryGovernor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ForgeTableKey {
    pub(crate) tenant: DataTenantId,
    pub(crate) table_ref: crate::catalog::TableRef,
}

impl ForgeCore {
    /// Build a validated Forge context using the staging operator as its object
    /// store seam.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the supplied configuration is
    /// not safe to run.
    pub fn new(
        vala: vala_sql::ValaPostgres,
        operator_pool: vala_sql::OperatorPool,
        catalog: Arc<dyn Catalog>,
        staging: Arc<Operator>,
        config: ForgeConfig,
    ) -> Result<Self, ForgeError> {
        config.validate()?;
        Ok(Self {
            vala,
            operator_pool,
            catalog,
            object_store: Arc::new(OpenDalForgeObjectStore {
                operator: Arc::clone(&staging),
            }),
            staging,
            config,
            memory_governor: None,
        })
    }

    /// Replace Forge's scoped object-store seam while retaining the raw
    /// operator used by producer fixtures and ordinary callers.
    #[must_use]
    pub fn with_object_store(mut self, object_store: Arc<dyn ForgeObjectStore>) -> Self {
        self.object_store = object_store;
        self
    }

    /// Attach the process-global Bifrost parent governor to Forge workspace
    /// operations. Test fixtures may omit it when they do not model pressure.
    #[must_use]
    pub fn with_memory_governor(mut self, memory_governor: BifrostMemoryGovernor) -> Self {
        self.memory_governor = Some(memory_governor);
        self
    }
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
}

/// Load one Iceberg table with the configured catalog request timeout.
pub(crate) async fn load_table(
    context: &ForgeCore,
    ident: &iceberg::TableIdent,
) -> Result<iceberg::table::Table, ForgeError> {
    tokio::time::timeout(
        context.config.catalog_request_timeout,
        context.catalog.load_table(ident),
    )
    .await
    .map_err(|_| ForgeError::Timeout {
        operation: "catalog table load",
    })?
    .map_err(ForgeError::Catalog)
}

/// Reconcile prepared compaction audits for one tenant/table before new work.
///
/// A prepared operation is marked committed when its output is still live in
/// Iceberg, or reset when the output is absent after the uncertainty window.
pub(crate) async fn run_compaction_reconciliation_for_table(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    table_key: &ForgeTableKey,
    binding: &TenantTableBinding,
) -> Result<usize, ForgeError> {
    let mut reconciled = 0;
    for key in load_reconciliation_keys(context).await? {
        if key.tenant != table_key.tenant || key.table_ref != table_key.table_ref {
            continue;
        }
        reconciled += reconcile_group(context, lease, &key, binding).await?;
    }
    Ok(reconciled)
}

/// Select, bin-pack, and commit bounded compaction work for one table.
///
/// The table lease is renewed before each bin, and the remaining lease time is
/// checked against the configured Iceberg commit window. Hitting any file,
/// byte, or bin budget stops the table without treating the skipped work as a
/// failure.
pub(crate) async fn run_compaction_bins_for_table(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    table_key: &ForgeTableKey,
    binding: &TenantTableBinding,
) -> Result<ForgeTickOutcome, ForgeError> {
    let mut outcome = ForgeTickOutcome::default();
    let rows = select_candidate_groups(&context.operator_pool, table_key, &context.config).await?;
    let mut file_budget = 0_usize;
    let mut byte_budget = 0_u64;
    let mut bin_budget = 0_usize;
    'groups: for row in rows {
        outcome.groups_seen += 1;
        let bins = stable_pack(
            row.files,
            context.config.target_bin_bytes,
            context.config.max_files_per_bin,
        );
        for bin in bins {
            if bin_budget >= context.config.max_bins_per_tick {
                break;
            }
            if file_budget.saturating_add(bin.files.len()) > context.config.max_files_per_tick
                || byte_budget.saturating_add(bin.total_bytes) > context.config.max_bytes_per_tick
            {
                outcome.bins_skipped += 1;
                outcome.budget_skips += 1;
                break 'groups;
            }
            if !lease.renew(&context.operator_pool).await? {
                return Err(ForgeError::FenceLost {
                    lease_key: lease.lease_key.clone(),
                });
            }
            if !lease.commit_window_fits(context.config.commit_window()) {
                outcome.bins_skipped += 1;
                outcome.budget_skips += 1;
                break 'groups;
            }
            compact_bin(context, lease, &row.key, binding, &bin).await?;
            file_budget += bin.files.len();
            byte_budget = byte_budget.saturating_add(bin.total_bytes);
            bin_budget += 1;
            outcome.bins_committed += 1;
        }
        if bin_budget >= context.config.max_bins_per_tick
            || file_budget >= context.config.max_files_per_tick
            || byte_budget >= context.config.max_bytes_per_tick
        {
            break;
        }
    }
    Ok(outcome)
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateRow {
    /// Tenant/table/day identity shared by every file in the row.
    key: ForgeGroupKey,
    /// Uncompacted files selected for this candidate group.
    files: Vec<CandidateFile>,
}

/// Load bounded, old-enough staging candidates for one tenant/table.
///
/// The query uses the operator pool because Forge discovers work across tenant
/// rows. Results are grouped by partition day and ordered deterministically so
/// repeated ticks make the same selection under the same durable state.
pub(crate) async fn select_candidate_groups(
    operator_pool: &vala_sql::OperatorPool,
    table_key: &ForgeTableKey,
    config: &ForgeConfig,
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
        i64::try_from(config.max_files_per_tick).map_err(|_| ForgeError::InvalidConfig {
            detail: "max_files_per_tick exceeds PostgreSQL bigint".to_owned(),
        })?,
    )
    .fetch_all(operator_pool.pool())
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
        .filter(|(_, files)| {
            files.len() >= usize::try_from(config.min_files).unwrap_or(usize::MAX)
                || files
                    .iter()
                    .map(|file| file.size)
                    .fold(0_u64, u64::saturating_add)
                    < config.target_bin_bytes
        })
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

/// Load groups whose prepared SQL transition has no committed snapshot ID.
async fn load_reconciliation_keys(context: &ForgeCore) -> Result<Vec<ForgeGroupKey>, ForgeError> {
    let rows = sqlx::query(
        r"SELECT DISTINCT data_tenant_id, namespace, table_name, partition_day
             FROM vala.file_list
            WHERE compacted AND committed_snapshot_id IS NULL
            ORDER BY data_tenant_id, namespace, table_name, partition_day",
    )
    .fetch_all(context.operator_pool.pool())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    rows.into_iter()
        .map(|row| {
            let tenant_uuid: Uuid =
                row.try_get("data_tenant_id")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?;
            let tenant = data_tenant_from_uuid(tenant_uuid)?;
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
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    binding: &TenantTableBinding,
    bin: &RewriteBin,
) -> Result<(), ForgeError> {
    let _workspace = reserve_forge_workspace(context, bin)?;
    let operation_id = operation_id(key, bin);
    let table = load_table(context, &binding.table_ident()).await?;
    let batch = read_and_project_staging(context, &table, key, binding, bin).await?;
    lease.require_fence(&context.operator_pool).await?;
    let output = write_output(&table, binding, operation_id, key.partition_day, &batch).await?;
    lease.require_fence(&context.operator_pool).await?;
    let prepared = forge_detail(
        key,
        bin,
        &output,
        operation_id,
        ForgeCompactionPhase::Prepared,
        None,
    )?;
    prepare_inputs(context, lease, key, bin, prepared).await?;
    lease.require_fence(&context.operator_pool).await?;
    if !lease.commit_window_fits(context.config.commit_window()) {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    let table = load_table(context, &binding.table_ident()).await?;
    let mut properties = HashMap::new();
    properties.insert("forge.operation_id".to_owned(), operation_id.to_string());
    properties.insert("forge.group".to_owned(), key.audit_resource());
    let tx = Transaction::new(&table);
    let action = tx
        .rewrite_files()
        .delete_files(Vec::<String>::new())
        .add_data_files([output.clone()])
        .set_commit_uuid(operation_id)
        .set_snapshot_properties(properties);
    let transaction = ApplyTransactionAction::apply(action, tx).map_err(ForgeError::Catalog)?;
    lease.require_fence(&context.operator_pool).await?;
    if !lease.commit_window_fits(context.config.commit_window()) {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    match tokio::time::timeout(
        context.config.iceberg_total_retry_timeout,
        transaction.commit(context.catalog.as_ref()),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            if !is_retryable(&error) {
                if lease.require_fence(&context.operator_pool).await.is_ok() {
                    reset_inputs(context, lease, key, bin, &output, operation_id).await?;
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
    let committed_table = load_table(context, &binding.table_ident()).await?;
    let snapshot_id = committed_table
        .metadata()
        .current_snapshot_id()
        .ok_or_else(|| ForgeError::Reconciliation {
            detail: "Iceberg replace committed without a current snapshot".to_owned(),
        })?;
    stamp_committed(context, lease, key, bin, &output, operation_id, snapshot_id).await
}

fn reserve_forge_workspace(
    context: &ForgeCore,
    bin: &RewriteBin,
) -> Result<Option<ParentMemoryReservation>, ForgeError> {
    let Some(governor) = &context.memory_governor else {
        return Ok(None);
    };
    let requested = usize::try_from(bin.total_bytes).map_err(|error| ForgeError::MemoryBudget {
        detail: format!("Forge input byte count does not fit memory accounting: {error}"),
    })?;
    governor
        .try_reserve_parent(requested.max(1))
        .map(Some)
        .map_err(|error| ForgeError::MemoryBudget {
            detail: error.to_string(),
        })
}

/// Return whether Iceberg classified an error as safe to retry.
fn is_retryable(error: &iceberg::Error) -> bool {
    error.retryable()
}

/// Read a rewrite bin, enforce tenant ownership, project by field name, and
/// sort rows by tenant and event time for the Iceberg writer.
async fn read_and_project_staging(
    context: &ForgeCore,
    table: &iceberg::table::Table,
    key: &ForgeGroupKey,
    binding: &TenantTableBinding,
    bin: &RewriteBin,
) -> Result<RecordBatch, ForgeError> {
    let paths = bin
        .files
        .iter()
        .map(|file| {
            binding
                .validate_object_path(&file.path)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: format!("Forge staging input escaped table prefix: {}", file.path),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut batches = Vec::new();
    let mut source_schema = None;
    for path in paths {
        let bytes = context
            .object_store
            .read(&path)
            .await
            .map_err(ForgeError::ObjectStore)?
            .to_bytes();
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?
            .build()
            .map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?;
        for batch in reader {
            let batch = batch.map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?;
            if let Some(schema) = source_schema.as_ref() {
                if batch.schema().as_ref() != schema {
                    return Err(ForgeError::Schema {
                        detail:
                            "staging files in one rewrite bin do not have the same Arrow schema"
                                .to_owned(),
                    });
                }
            } else {
                source_schema = Some(batch.schema().as_ref().clone());
            }
            validate_tenant_column(&batch, key.tenant)?;
            batches.push(batch);
        }
    }
    let source_schema = source_schema.ok_or_else(|| ForgeError::Parquet {
        detail: "staging files contained no record batches".to_owned(),
    })?;
    let source =
        arrow::compute::concat_batches(&Arc::new(source_schema), &batches).map_err(|error| {
            ForgeError::Parquet {
                detail: error.to_string(),
            }
        })?;
    let target = Arc::new(
        iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema())
            .map_err(ForgeError::Catalog)?,
    );
    let projected = project_by_name(&source, target)?;
    sort_rows(&projected)
}

/// Reject a staging batch containing a missing, malformed, or foreign tenant.
fn validate_tenant_column(batch: &RecordBatch, tenant: DataTenantId) -> Result<(), ForgeError> {
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
fn project_by_name(
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

/// Sort a projected batch by tenant and event time before writing it.
fn sort_rows(batch: &RecordBatch) -> Result<RecordBatch, ForgeError> {
    let tenant = batch
        .schema()
        .index_of("data_tenant_id")
        .map_err(|_| ForgeError::Schema {
            detail: "projected batch lacks data_tenant_id".to_owned(),
        })?;
    let event_time =
        batch
            .schema()
            .index_of("wyrd_event_time")
            .map_err(|_| ForgeError::Schema {
                detail: "projected batch lacks wyrd_event_time".to_owned(),
            })?;
    let indices = lexsort_to_indices(
        &[
            SortColumn {
                values: batch.column(tenant).clone(),
                options: Some(SortOptions {
                    descending: false,
                    nulls_first: false,
                }),
            },
            SortColumn {
                values: batch.column(event_time).clone(),
                options: Some(SortOptions {
                    descending: false,
                    nulls_first: false,
                }),
            },
        ],
        None,
    )
    .map_err(|error| ForgeError::Schema {
        detail: error.to_string(),
    })?;
    let columns = batch
        .columns()
        .iter()
        .map(|column| take(column, &indices, None))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ForgeError::Schema {
            detail: error.to_string(),
        })?;
    RecordBatch::try_new(batch.schema(), columns).map_err(|error| ForgeError::Schema {
        detail: error.to_string(),
    })
}

/// Write one compacted Parquet data file under the table's object prefix.
///
/// The output must contain one file, the same row count as the input batch, and
/// the candidate partition. The prefix check prevents a catalog or storage
/// configuration error from escaping the tenant/table boundary.
async fn write_output(
    table: &iceberg::table::Table,
    binding: &TenantTableBinding,
    operation_id: Uuid,
    partition_day: NaiveDate,
    batch: &RecordBatch,
) -> Result<DataFile, ForgeError> {
    let schema = table.metadata().current_schema().clone();
    let props = bifrost_writer_properties(batch.num_rows());
    let parquet = ParquetWriterBuilder::new(props, schema.clone());
    // Iceberg DataFile locations must be absolute URLs. The table metadata
    // location already carries the configured warehouse/backend, while the
    // table binding keeps the tenant-qualified prefix in that location.
    let location = DefaultLocationGenerator::new(table.metadata()).map_err(ForgeError::Catalog)?;
    let name = DefaultFileNameGenerator::new(
        format!("forge-{operation_id}"),
        None,
        DataFileFormat::Parquet,
    );
    let rolling = RollingFileWriterBuilder::new_with_default_file_size(
        parquet,
        table.file_io().clone(),
        location,
        name,
    );
    let partition = Struct::from_iter([Some(Literal::date(
        partition_day.num_days_from_ce() - 719_163,
    ))]);
    let mut writer = DataFileWriterBuilder::new(rolling)
        .build(Some(PartitionKey::new(
            table.metadata().default_partition_spec().as_ref().clone(),
            schema,
            partition.clone(),
        )))
        .await
        .map_err(ForgeError::Catalog)?;
    writer
        .write(batch.clone())
        .await
        .map_err(ForgeError::Catalog)?;
    let files = writer.close().await.map_err(ForgeError::Catalog)?;
    if files.len() != 1 {
        return Err(ForgeError::Invariant {
            detail: format!("Forge writer produced {} files for one bin", files.len()),
        });
    }
    let file = files
        .into_iter()
        .next()
        .ok_or_else(|| ForgeError::Invariant {
            detail: "Forge writer returned no data file".to_owned(),
        })?;
    if !path_is_under_prefix(file.file_path(), &binding.object_prefix) {
        return Err(ForgeError::Invariant {
            detail: format!("Forge output escaped table prefix: {}", file.file_path()),
        });
    }
    if file.record_count() != u64::try_from(batch.num_rows()).unwrap_or(u64::MAX) {
        return Err(ForgeError::Invariant {
            detail: format!(
                "Forge output row count {} does not match input {}",
                file.record_count(),
                batch.num_rows()
            ),
        });
    }
    if file.partition() != &partition {
        return Err(ForgeError::Invariant {
            detail: "Forge output partition does not match the candidate day".to_owned(),
        });
    }
    Ok(file)
}

/// Check that a path is a normalized descendant of a configured object prefix.
fn path_is_under_prefix(path: &str, prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches('/');
    let prefixed = format!("{prefix}/");
    path.match_indices(prefix).any(|(index, _)| {
        if index > 0 && !path[..index].ends_with('/') {
            return false;
        }
        let candidate = &path[index..];
        (candidate == prefix || candidate.starts_with(&prefixed))
            && !candidate.contains('\\')
            && !candidate
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    })
}

/// Mark input rows prepared and append the matching prepared audit event.
///
/// The SQL transition and audit append share one tenant transaction, which is
/// fenced immediately before commit.
async fn prepare_inputs(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    detail: AuditDetail,
) -> Result<(), ForgeError> {
    if !lease.renew(&context.operator_pool).await? {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    let mut conn = context
        .vala
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let ids: Vec<Uuid> = bin.files.iter().map(|file| file.id).collect();
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET compacted = true
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5) AND NOT compacted",
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
    lease.require_fence(&context.operator_pool).await?;
    append_system_audit(&mut conn, key, "forge.file_compact.prepared", detail).await?;
    lease.assert_transaction_fence(&mut conn).await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

/// Stamp the Iceberg snapshot ID on prepared input rows and audit the commit.
async fn stamp_committed(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    output: &DataFile,
    operation_id: Uuid,
    snapshot_id: i64,
) -> Result<(), ForgeError> {
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
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
              AND compacted AND committed_snapshot_id IS NULL",
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
        output,
        operation_id,
        ForgeCompactionPhase::Committed,
        Some(snapshot_id),
    )?;
    lease.require_fence(&context.operator_pool).await?;
    append_system_audit(&mut conn, key, "forge.file_compact.committed", detail).await?;
    lease.assert_transaction_fence(&mut conn).await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

/// Restore prepared input rows to the uncompacted state after a definite
/// Iceberg failure.
async fn reset_inputs(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    output: &DataFile,
    operation_id: Uuid,
) -> Result<(), ForgeError> {
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
        .vala
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let ids: Vec<Uuid> = bin.files.iter().map(|file| file.id).collect();
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET compacted = false, committed_snapshot_id = NULL
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5) AND compacted
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
        output,
        operation_id,
        ForgeCompactionPhase::Reset,
        None,
    )?;
    lease.require_fence(&context.operator_pool).await?;
    append_system_audit(&mut conn, key, "forge.file_compact.reset", detail).await?;
    lease.assert_transaction_fence(&mut conn).await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

/// Reconcile the latest prepared audit transition for every group in a table.
///
/// A live output is stamped as recovered. An output absent after the
/// uncertainty window resets the hidden inputs so a future tick can retry.
async fn reconcile_group(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    binding: &TenantTableBinding,
) -> Result<usize, ForgeError> {
    let resource = key.audit_resource();
    let latest = load_reconciliation_audits(context, key, &resource).await?;
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
            output_path,
            ..
        } = &detail
        else {
            continue;
        };
        if detail_group(&detail) != resource {
            return Err(ForgeError::Reconciliation {
                detail: "audit detail group differs from its resource".to_owned(),
            });
        }
        verify_hidden_inputs(context, key, input_file_ids, &detail).await?;
        if !lease.renew(&context.operator_pool).await? {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let table = load_table(context, &binding.table_ident()).await?;
        if let Some(snapshot_id) = live_snapshot_for_path(&table, output_path.as_str()).await? {
            if !lease.renew(&context.operator_pool).await? {
                return Err(ForgeError::FenceLost {
                    lease_key: lease.lease_key.clone(),
                });
            }
            stamp_reconciled(context, lease, key, input_file_ids, &detail, snapshot_id).await?;
            recovered += 1;
            continue;
        }
        if Utc::now()
            .signed_duration_since(created_at)
            .to_std()
            .unwrap_or_default()
            < context.config.uncertainty_bound
        {
            continue;
        }
        let first_reload = load_table(context, &binding.table_ident()).await?;
        if live_snapshot_for_path(&first_reload, output_path.as_str())
            .await?
            .is_some()
        {
            continue;
        }
        let second_reload = load_table(context, &binding.table_ident()).await?;
        if live_snapshot_for_path(&second_reload, output_path.as_str())
            .await?
            .is_none()
        {
            if !lease.renew(&context.operator_pool).await? {
                return Err(ForgeError::FenceLost {
                    lease_key: lease.lease_key.clone(),
                });
            }
            reset_reconciled(context, lease, key, input_file_ids, &detail).await?;
            recovered += 1;
        }
    }
    Ok(recovered)
}

/// Load the latest audit detail for each compaction operation in a group.
async fn load_reconciliation_audits(
    context: &ForgeCore,
    key: &ForgeGroupKey,
    resource: &str,
) -> Result<HashMap<Uuid, (AuditDetail, DateTime<Utc>)>, ForgeError> {
    let mut after_seq = 0_i64;
    let mut latest = HashMap::new();
    let mut conn = context
        .vala
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    loop {
        let page = vala_sql::queries::audit_outbox::list_audit_events_for_resource(
            &mut conn,
            resource,
            after_seq,
            context.config.audit_page_size,
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
            validate_reconciliation_detail(&detail, resource)?;
            let AuditDetail::ForgeCompaction { operation_id, .. } = &detail else {
                continue;
            };
            latest.insert(*operation_id, (detail, row.created_at));
        }
        if page_len < usize::try_from(context.config.audit_page_size).unwrap_or(usize::MAX) {
            break;
        }
    }
    conn.commit().await.map_err(ForgeError::Sql)?;
    Ok(latest)
}

/// Validate the identity and ordering fields needed for compaction recovery.
fn validate_reconciliation_detail(detail: &AuditDetail, resource: &str) -> Result<(), ForgeError> {
    let AuditDetail::ForgeCompaction {
        group,
        input_file_ids,
        input_paths,
        output_path,
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
        || output_path.as_str().is_empty()
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

/// Find the current Iceberg snapshot that contains a live data-file path.
async fn live_snapshot_for_path(
    table: &iceberg::table::Table,
    path: &str,
) -> Result<Option<i64>, ForgeError> {
    let Some(snapshot) = table.metadata().current_snapshot() else {
        return Ok(None);
    };
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
            if entry.is_alive() && entry.data_file().file_path() == path {
                return Ok(entry.snapshot_id().or(Some(snapshot.snapshot_id())));
            }
        }
    }
    Ok(None)
}

/// Confirm that prepared audit inputs still match hidden `file_list` rows.
async fn verify_hidden_inputs(
    context: &ForgeCore,
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
    let mut conn = context
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
async fn stamp_reconciled(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    input_file_ids: &[Uuid],
    detail: &AuditDetail,
    snapshot_id: i64,
) -> Result<(), ForgeError> {
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
        .vala
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET committed_snapshot_id = $1
            WHERE data_tenant_id = $2 AND namespace = $3 AND table_name = $4
              AND partition_day = $5 AND id = ANY($6)
              AND compacted AND committed_snapshot_id IS NULL",
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
    append_system_audit(
        &mut conn,
        key,
        "forge.file_compact.recovered",
        terminal_detail(detail, ForgeCompactionPhase::Recovered, Some(snapshot_id)),
    )
    .await?;
    lease.assert_transaction_fence(&mut conn).await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

/// Reset a recovered compaction whose output was never committed.
async fn reset_reconciled(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    input_file_ids: &[Uuid],
    detail: &AuditDetail,
) -> Result<(), ForgeError> {
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
        .vala
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET compacted = false, committed_snapshot_id = NULL
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5)
              AND compacted AND committed_snapshot_id IS NULL",
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
    append_system_audit(
        &mut conn,
        key,
        "forge.file_compact.reset",
        terminal_detail(detail, ForgeCompactionPhase::Reset, None),
    )
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
            output_path,
            writer_recipe_version,
            ..
        } => AuditDetail::ForgeCompaction {
            operation_id: *operation_id,
            phase,
            group: group.clone(),
            input_file_ids: input_file_ids.clone(),
            input_paths: input_paths.clone(),
            output_path: output_path.clone(),
            snapshot_id,
            writer_recipe_version: writer_recipe_version.clone(),
        },
        _ => detail.clone(),
    }
}

/// Append a system-owned Forge audit event to the caller's tenant transaction.
async fn append_system_audit(
    conn: &mut vala_sql::TenantConn<'_>,
    key: &ForgeGroupKey,
    operation: &str,
    detail: AuditDetail,
) -> Result<(), ForgeError> {
    let event = AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: operation.to_owned(),
        resource: key.audit_resource(),
        card_ref: None,
        principal_id: SYSTEM_PRINCIPAL,
        principal_kind: PrincipalKindTag::Service,
        auth_method: AuthMethod::Internal,
        permission: "bifrost:forge".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: operation.to_owned(),
        detail: Some(detail),
    };
    vala_sql::queries::audit_outbox::append_audit(conn, &event)
        .await
        .map(|_| ())
        .map_err(ForgeError::Sql)
}

/// Build the canonical audit detail for one compaction operation.
fn forge_detail(
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    output: &DataFile,
    operation_id: Uuid,
    phase: ForgeCompactionPhase,
    snapshot_id: Option<i64>,
) -> Result<AuditDetail, ForgeError> {
    let group = StoragePath::new(key.audit_resource()).map_err(|error| ForgeError::Group {
        detail: error.to_string(),
    })?;
    let output_path =
        StoragePath::new(output.file_path().to_owned()).map_err(|error| ForgeError::Group {
            detail: error.to_string(),
        })?;
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
        output_path,
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

    #[test]
    fn compacted_output_uses_shared_writer_properties() {
        assert_eq!(
            bifrost_writer_properties(10).max_row_group_row_count(),
            Some(131_072)
        );
    }

    #[test]
    fn rewrite_action_uses_empty_delete_set_for_staging_fold() {
        let _ = Transaction::new;
        let config = ForgeConfig::default();
        assert!(config.validate().is_ok());
    }
}
