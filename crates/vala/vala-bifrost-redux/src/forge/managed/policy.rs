//! Immutable rewrite policy extracted from one registered table.
//!
//! A managed rewrite has exactly two sources of truth for *how* it should
//! rewrite: the table's own registered geometry, and the operator limits this
//! Forge owner was validated with. [`ForgeTablePolicy`] is where those two meet
//! and are checked against each other once, before any attempt is admitted, so
//! every later step — selection, grouping, writing — runs against terms that
//! were already proven consistent rather than clamped silently at the point of
//! use.

use std::collections::HashMap;
use std::sync::Arc;

use iceberg::spec::{TableMetadata, TableProperties};
use iceberg::table::Table;
use iceberg_compaction_core::compaction::{CompactionPlan, CompactionPlanner};
use iceberg_compaction_core::config::{
    CompactionConfig, CompactionExecutionConfig, CompactionExecutionConfigBuilder,
    CompactionPlanningConfig, FileGroupScope, FilesWithDeletesConfig, FullCompactionConfig,
    GroupingStrategy, SmallFilesConfig,
};
use iceberg_compaction_core::file_selection::{FileSelector, PlanStrategy};
use iceberg_compaction_core::managed::{SelectedFile, SelectionReport, SelectionStrategyKind};
use wyrd_spec::DataTenantId;

use crate::catalog::layout::{FORGE_WRITER_RECIPE, forge_data_location};
use crate::forge::compact::ForgeConfig;
use crate::forge::error::ForgeError;
use crate::forge::settings::ForgeCompactionType;

/// `RisingWave`'s compactor runner `max_parallelism` default.
const RUNNER_MAX_PARALLELISM: usize = 4;

/// `RisingWave`'s compactor runner `min_size_per_partition` default (1 GiB).
const RUNNER_MIN_SIZE_PER_PARTITION: u64 = 1024 * 1024 * 1024;

/// `RisingWave`'s compactor runner `max_file_count_per_partition` default.
const RUNNER_MAX_FILE_COUNT_PER_PARTITION: usize = 32;

/// `RisingWave`'s Iceberg sink `delete_files_count_threshold` default.
const DELETE_FILES_COUNT_THRESHOLD: usize = 256;

/// Upstream Auto's minimum small-file count for a small-file plan.
const AUTO_MIN_SMALL_FILES: usize = 5;

/// Upstream Auto's minimum delete-heavy file count for a delete plan.
const AUTO_MIN_DELETE_HEAVY_FILES: usize = 1;

/// Iceberg property naming the encoded row-group target for Parquet writers.
///
/// Held separately from the file target on purpose: a row group is the unit a
/// reader prunes and buffers, while a file is the unit the catalog tracks.
/// Conflating them either produces one enormous row group per file or forces a
/// file to roll at every row group.
const ROW_GROUP_TARGET_PROPERTY: &str = TableProperties::PROPERTY_PARQUET_ROW_GROUP_SIZE_BYTES;

/// Default encoded row-group target when the table declares none.
///
/// The same soft 128 MiB target Scribe's staged writer uses, so a table's row
/// groups keep one shape from staging through compaction.
const ROW_GROUP_TARGET_DEFAULT: u64 =
    crate::parquet::writer_properties::BIFROST_ROW_GROUP_TARGET_BYTES as u64;

/// Iceberg property naming the target size of a newly written data file.
const FILE_TARGET_PROPERTY: &str = TableProperties::PROPERTY_WRITE_TARGET_FILE_SIZE_BYTES;

/// Multiplier the managed core applies to derive its oversized ceiling.
///
/// Mirrored here only so this owner can refuse a target whose ceiling would
/// overflow *before* the core is constructed, which is the difference between a
/// refusal an operator can read and a failure deep inside planning.
const OVERSIZED_CEILING_PERCENT: u64 = 180;

/// Reports whether `bytes` is a file target a table may declare at registration.
///
/// Registration cannot see a worker's operator limits, so it checks only what
/// holds under every deployment: the target is representable on this platform,
/// its oversized ceiling does not overflow, and it is at least the default
/// row-group target a newly registered table writes with, which also places it
/// above the default small-file threshold. [`ForgeTablePolicy::extract`] re-checks the
/// full table and worker combination at planning.
#[must_use]
pub(crate) fn registrable_target_file_size_bytes(bytes: u64) -> bool {
    bytes >= ROW_GROUP_TARGET_DEFAULT
        && bytes.checked_mul(OVERSIZED_CEILING_PERCENT).is_some()
        && usize::try_from(bytes).is_ok()
}

/// The complete, already-consistent terms one managed rewrite executes under.
///
/// Every field is derived, never defaulted at the point of use: the identity
/// triple and both size targets come from the table, the small-file threshold
/// and plan budget come from validated operator limits, and the data location
/// and writer recipe come from the registered Forge path layout. Construction
/// is the only place these can disagree, so construction is where they are
/// checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeTablePolicy {
    /// Target size of one rewritten data file, passed through unchanged.
    pub(crate) target_file_size_bytes: u64,
    /// Independent encoded row-group target applied to the Parquet writer.
    pub(crate) row_group_target_bytes: u64,
    /// Size at or below which a live file is small enough to be reselected.
    pub(crate) small_file_threshold_bytes: u64,
    /// Current schema a produced file must carry to be considered settled.
    pub(crate) schema_id: i32,
    /// Current partition spec a produced file must carry.
    pub(crate) partition_spec_id: i32,
    /// Current sort order a produced file must carry.
    pub(crate) sort_order_id: i32,
    /// Writer recipe segment produced paths carry and selection resolves back.
    pub(crate) writer_recipe: String,
    /// Base location beneath which every rewrite output is written.
    pub(crate) data_location: String,
}

impl ForgeTablePolicy {
    /// Derives one attempt's policy from a loaded table and validated limits.
    ///
    /// The file target is the table's `write.target-file-size-bytes` property
    /// when declared, otherwise the deployment default in
    /// [`ForgeConfig::default_target_file_size_bytes`]. It is resolved here
    /// once; execution, publication, and audit all read this policy's value.
    ///
    /// There is no admitted-memory term. Execution charges the shared Bifrost
    /// memory root as it grows, so a row-group target is never checked
    /// against a per-plan grant.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when a declared size is
    /// unparseable or zero, when the small-file threshold is not strictly below
    /// the file target, when the row-group target exceeds the file target, when
    /// the file target cannot be represented on this platform or would overflow
    /// the core's oversized ceiling, or when the table's declared data location
    /// is not the registered Forge recipe location.
    pub(crate) fn extract(
        metadata: &TableMetadata,
        config: &ForgeConfig,
    ) -> Result<Self, ForgeError> {
        let properties = metadata.properties();
        let target_file_size_bytes = declared_bytes(
            properties,
            FILE_TARGET_PROPERTY,
            config.default_target_file_size_bytes,
        )?;
        let row_group_target_bytes = declared_bytes(
            properties,
            ROW_GROUP_TARGET_PROPERTY,
            ROW_GROUP_TARGET_DEFAULT,
        )?;
        let expected_location = forge_data_location(metadata.location());
        let declared_location = properties
            .get(crate::catalog::layout::WRITE_DATA_PATH_PROPERTY)
            .map(String::as_str)
            .unwrap_or_default();
        if declared_location != expected_location {
            return Err(ForgeError::InvalidConfig {
                detail: format!(
                    "table declares rewrite data location {declared_location:?}, \
                     but the registered Forge recipe location is {expected_location:?}"
                ),
            });
        }
        let policy = Self {
            target_file_size_bytes,
            row_group_target_bytes,
            small_file_threshold_bytes: config.small_file_threshold_bytes,
            schema_id: metadata.current_schema_id(),
            partition_spec_id: metadata.default_partition_spec_id(),
            sort_order_id: i32::try_from(metadata.default_sort_order().order_id).map_err(|_| {
                ForgeError::InvalidConfig {
                    detail: format!(
                        "table declares sort order id {} outside the Iceberg data-file range",
                        metadata.default_sort_order().order_id
                    ),
                }
            })?,
            writer_recipe: FORGE_WRITER_RECIPE.to_owned(),
            data_location: expected_location,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Rejects every geometry a rewrite could not honestly execute under.
    ///
    /// Separate from [`Self::extract`] so the invariants are stated once and
    /// can be exercised directly against a constructed policy rather than only
    /// through a table.
    ///
    /// # Errors
    ///
    /// See [`Self::extract`]; this method raises exactly those conditions.
    fn validate(&self) -> Result<(), ForgeError> {
        let refuse = |detail: String| Err(ForgeError::InvalidConfig { detail });
        if self.target_file_size_bytes == 0 {
            return refuse("rewrite target file size must be positive".to_owned());
        }
        if self.row_group_target_bytes == 0 {
            return refuse("rewrite row-group target must be positive".to_owned());
        }
        if self.small_file_threshold_bytes == 0 {
            return refuse("rewrite small-file threshold must be positive".to_owned());
        }
        if self.small_file_threshold_bytes >= self.target_file_size_bytes {
            return refuse(format!(
                "small-file threshold {} must be below the target file size {}",
                self.small_file_threshold_bytes, self.target_file_size_bytes
            ));
        }
        if self.row_group_target_bytes > self.target_file_size_bytes {
            return refuse(format!(
                "row-group target {} must not exceed the target file size {}",
                self.row_group_target_bytes, self.target_file_size_bytes
            ));
        }
        if self
            .target_file_size_bytes
            .checked_mul(OVERSIZED_CEILING_PERCENT)
            .is_none()
        {
            return refuse(format!(
                "target file size {} overflows the oversized ceiling",
                self.target_file_size_bytes
            ));
        }
        if usize::try_from(self.target_file_size_bytes).is_err() {
            return refuse(format!(
                "target file size {} exceeds this platform",
                self.target_file_size_bytes
            ));
        }
        Ok(())
    }

    /// Builds `RisingWave`'s planning configuration for one task type.
    ///
    /// Mirrors `build_task_planning_config`
    /// (`iceberg_compactor_runner.rs:477-590` at e23ddf95) with its runner
    /// defaults: parallelism four, 1 GiB minimum per partition, 32 files per
    /// partition, heuristic output parallelism off, single grouping, and a
    /// delete-file threshold of 256. The file target and small-file threshold
    /// are this policy's. Copy-on-write plans `Full` over the whole table, as
    /// `RisingWave` does for a copy-on-write sink.
    #[must_use]
    pub(crate) fn planning(
        &self,
        compaction_type: ForgeCompactionType,
        copy_on_write: bool,
    ) -> ForgeTaskPlanning {
        let compaction_type = if copy_on_write {
            ForgeCompactionType::Full
        } else {
            compaction_type
        };
        match compaction_type {
            ForgeCompactionType::Full => {
                ForgeTaskPlanning::Explicit(CompactionPlanningConfig::Full(FullCompactionConfig {
                    target_file_size_bytes: self.target_file_size_bytes,
                    min_size_per_partition: RUNNER_MIN_SIZE_PER_PARTITION,
                    max_file_count_per_partition: RUNNER_MAX_FILE_COUNT_PER_PARTITION,
                    max_input_parallelism: RUNNER_MAX_PARALLELISM,
                    max_output_parallelism: RUNNER_MAX_PARALLELISM,
                    enable_heuristic_output_parallelism: false,
                    grouping_strategy: GroupingStrategy::Single,
                    file_group_scope: if copy_on_write {
                        FileGroupScope::Table
                    } else {
                        FileGroupScope::Partition
                    },
                    max_file_sequence_number: None,
                }))
            }
            ForgeCompactionType::SmallFiles => ForgeTaskPlanning::Explicit(
                CompactionPlanningConfig::SmallFiles(self.small_files()),
            ),
            ForgeCompactionType::FilesWithDelete => ForgeTaskPlanning::Explicit(
                CompactionPlanningConfig::FilesWithDeletes(self.files_with_deletes()),
            ),
            ForgeCompactionType::Auto => ForgeTaskPlanning::Auto {
                files_with_deletes: self.files_with_deletes(),
                small_files: self.small_files(),
            },
        }
    }

    /// `RisingWave`'s `SmallFiles` configuration under this policy.
    fn small_files(&self) -> SmallFilesConfig {
        SmallFilesConfig {
            target_file_size_bytes: self.target_file_size_bytes,
            min_size_per_partition: RUNNER_MIN_SIZE_PER_PARTITION,
            max_file_count_per_partition: RUNNER_MAX_FILE_COUNT_PER_PARTITION,
            max_input_parallelism: RUNNER_MAX_PARALLELISM,
            max_output_parallelism: RUNNER_MAX_PARALLELISM,
            enable_heuristic_output_parallelism: false,
            small_file_threshold_bytes: self.small_file_threshold_bytes,
            grouping_strategy: GroupingStrategy::Single,
            file_group_scope: FileGroupScope::Partition,
            max_file_sequence_number: None,
            group_filters: None,
        }
    }

    /// `RisingWave`'s `FilesWithDelete` configuration under this policy.
    fn files_with_deletes(&self) -> FilesWithDeletesConfig {
        FilesWithDeletesConfig {
            target_file_size_bytes: self.target_file_size_bytes,
            min_size_per_partition: RUNNER_MIN_SIZE_PER_PARTITION,
            max_file_count_per_partition: RUNNER_MAX_FILE_COUNT_PER_PARTITION,
            max_input_parallelism: RUNNER_MAX_PARALLELISM,
            max_output_parallelism: RUNNER_MAX_PARALLELISM,
            enable_heuristic_output_parallelism: false,
            grouping_strategy: GroupingStrategy::Single,
            file_group_scope: FileGroupScope::Partition,
            max_file_sequence_number: None,
            min_delete_file_count_threshold: DELETE_FILES_COUNT_THRESHOLD,
            group_filters: None,
        }
    }

    /// Builds the complete core configuration one plan executes under.
    ///
    /// `data_file_prefix` is the attempt identity, which is what makes every
    /// object an attempt produced attributable to it by path alone. `tenant`
    /// is the table binding's tenant, stamped into every output footer.
    /// `planning` is carried only because the core's configuration requires
    /// one; a rewrite reads the execution terms alone.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the derived execution
    /// configuration cannot be built, which can only happen if a required
    /// builder term is left unset.
    pub(crate) fn to_core_config(
        &self,
        planning: CompactionPlanningConfig,
        data_file_prefix: String,
        bloom_columns: &[String],
        max_concurrent_closes: usize,
        tenant: DataTenantId,
    ) -> Result<Arc<CompactionConfig>, ForgeError> {
        let execution: CompactionExecutionConfig = CompactionExecutionConfigBuilder::default()
            .target_file_size_bytes(self.target_file_size_bytes)
            .data_file_prefix(data_file_prefix)
            .max_concurrent_closes(max_concurrent_closes)
            .write_parquet_properties(crate::parquet::bifrost_rewrite_writer_properties(
                self.row_group_target_bytes,
                bloom_columns,
                tenant,
            ))
            // One runner executes exactly one plan, so the core's own
            // multi-plan concurrency is never used. Memory and spill are left
            // unset here because they come from the attempt's governed
            // `ManagedExecutionContext`, not from this configuration.
            .max_concurrent_compaction_plans(1)
            .build()
            .map_err(|error| ForgeError::InvalidConfig {
                detail: format!("Forge rewrite execution configuration is incomplete: {error}"),
            })?;
        Ok(Arc::new(CompactionConfig::new(planning, execution)))
    }
}

/// `RisingWave`'s per-task planning choice: one explicit mode, or Auto.
///
/// Auto is not planned by the pinned fork, whose Auto selector diverges from
/// upstream; [`Self::plan`] ports upstream's `AutoCompactionPlanner` instead.
#[derive(Debug, Clone)]
pub(crate) enum ForgeTaskPlanning {
    /// Full, `SmallFiles` or `FilesWithDeletes`, planned by the core as is.
    Explicit(CompactionPlanningConfig),
    /// Upstream Auto: delete-heavy plans first, else small-file plans.
    Auto {
        /// Configuration used when enough delete-heavy files exist.
        files_with_deletes: FilesWithDeletesConfig,
        /// Configuration used when enough small files exist.
        small_files: SmallFilesConfig,
    },
}

impl ForgeTaskPlanning {
    /// Plans one loaded table's branch and reports what was selected.
    ///
    /// Auto follows upstream `AutoCompactionPlanner` at nimtable `74bdc45`
    /// (`compaction/auto.rs:130-300`): one scan; nothing for a table with at
    /// most one data file; a delete-heavy candidate when at least one file
    /// carries the delete threshold, a small-file candidate when at least five
    /// files are small; the delete-heavy plans win when nonempty, otherwise
    /// the small-file plans. Every plan is bound to `branch` at its snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the branch has no snapshot, the
    /// manifests cannot be read, or the core refuses its configuration.
    pub(crate) async fn plan(
        &self,
        table: &Table,
        branch: &str,
    ) -> Result<(Vec<CompactionPlan>, SelectionReport), ForgeError> {
        let core_error = |error: iceberg_compaction_core::CompactionError| {
            ForgeError::Catalog(iceberg::Error::new(
                iceberg::ErrorKind::Unexpected,
                format!("Forge managed planning failed: {error}"),
            ))
        };
        let (files_with_deletes, small_files) = match self {
            Self::Explicit(config) => {
                return CompactionPlanner::new(config.clone())
                    .plan_compaction_with_report(table, branch)
                    .await
                    .map_err(core_error);
            }
            Self::Auto {
                files_with_deletes,
                small_files,
            } => (files_with_deletes, small_files),
        };
        let snapshot_id = table
            .metadata()
            .snapshot_for_ref(branch)
            .map(|snapshot| snapshot.snapshot_id())
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!("branch '{branch}' has no snapshot to plan from"),
            })?;
        let tasks = FileSelector::scan_data_files(table, snapshot_id)
            .await
            .map_err(core_error)?;
        let small = tasks
            .iter()
            .filter(|task| task.length < small_files.small_file_threshold_bytes)
            .count();
        let delete_heavy = tasks
            .iter()
            .filter(|task| task.deletes.len() >= files_with_deletes.min_delete_file_count_threshold)
            .count();
        let mut candidates = Vec::new();
        if tasks.len() > 1 {
            if delete_heavy >= AUTO_MIN_DELETE_HEAVY_FILES {
                candidates.push((
                    CompactionPlanningConfig::FilesWithDeletes(files_with_deletes.clone()),
                    SelectionStrategyKind::UpstreamFilesWithDeletes,
                ));
            }
            if small >= AUTO_MIN_SMALL_FILES {
                candidates.push((
                    CompactionPlanningConfig::SmallFiles(small_files.clone()),
                    SelectionStrategyKind::UpstreamSmallFiles,
                ));
            }
        }
        for (config, kind) in candidates {
            let plans = FileSelector::group_tasks_with_strategy(
                tasks.clone(),
                PlanStrategy::from(&config),
                &config,
            )
            .map_err(core_error)?
            .into_iter()
            .map(|group| CompactionPlan::new(group, branch.to_owned(), snapshot_id))
            .filter(CompactionPlan::has_files)
            .collect::<Vec<_>>();
            if !plans.is_empty() {
                return Ok((plans.clone(), selection_report(kind, snapshot_id, &plans)?));
            }
        }
        Ok((
            Vec::new(),
            selection_report(SelectionStrategyKind::UpstreamAuto, snapshot_id, &[])?,
        ))
    }
}

/// Builds the selection report one upstream strategy's plans describe.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the core refuses the report.
fn selection_report(
    kind: SelectionStrategyKind,
    snapshot_id: i64,
    plans: &[CompactionPlan],
) -> Result<SelectionReport, ForgeError> {
    let reason = kind.uniform_reason().ok_or_else(|| ForgeError::Invariant {
        detail: format!("strategy {kind} has no uniform selection reason"),
    })?;
    let selected = plans
        .iter()
        .flat_map(|plan| plan.file_group.data_files.iter())
        .map(|task| SelectedFile {
            file_path: task.data_file_path.clone(),
            reason,
        })
        .collect();
    SelectionReport::new(kind, snapshot_id, None, selected).map_err(|error| ForgeError::Invariant {
        detail: format!("Forge selection report is not canonical: {error}"),
    })
}

/// Reads one declared byte-valued table property, or its documented default.
///
/// # Errors
///
/// Returns [`ForgeError::InvalidConfig`] when the property is present but is
/// not a base-ten unsigned integer, because silently falling back to the
/// default would execute under geometry the table did not declare.
fn declared_bytes(
    properties: &HashMap<String, String>,
    key: &str,
    default: u64,
) -> Result<u64, ForgeError> {
    match properties.get(key) {
        None => Ok(default),
        Some(raw) => raw
            .parse::<u64>()
            .map_err(|error| ForgeError::InvalidConfig {
                detail: format!("table property {key}={raw:?} is not a byte count: {error}"),
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds table metadata carrying an explicit Forge geometry.
    ///
    /// Registration is reproduced rather than mocked: the location, the recipe
    /// data path, and both size properties are the exact strings the catalog
    /// writes, so a policy that reads them wrongly fails here rather than in a
    /// tier-2 run.
    ///
    /// # Panics
    ///
    /// Panics when the fixture schema or metadata cannot be built, which is a
    /// construction invariant of the test rather than an input.
    fn metadata_with(properties: Vec<(&str, &str)>) -> TableMetadata {
        let schema = iceberg::spec::Schema::builder()
            .with_schema_id(0)
            .with_fields(vec![
                iceberg::spec::NestedField::required(
                    1,
                    "value",
                    iceberg::spec::Type::Primitive(iceberg::spec::PrimitiveType::Long),
                )
                .into(),
            ])
            .build()
            .expect("fixture schema");
        let location = "file:///warehouse/tenant/table";
        let mut declared: HashMap<String, String> = properties
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        declared
            .entry(crate::catalog::layout::WRITE_DATA_PATH_PROPERTY.to_owned())
            .or_insert_with(|| forge_data_location(location));
        iceberg::spec::TableMetadataBuilder::new(
            schema,
            iceberg::spec::PartitionSpec::unpartition_spec(),
            iceberg::spec::SortOrder::unsorted_order(),
            location.to_owned(),
            iceberg::spec::FormatVersion::V2,
            declared,
        )
        .expect("fixture metadata builder")
        .build()
        .expect("fixture metadata")
        .metadata
    }

    /// A validated Forge owner configuration with a usable small-file floor.
    fn limits() -> ForgeConfig {
        ForgeConfig {
            small_file_threshold_bytes: 32 * 1024 * 1024,
            ..ForgeConfig::default()
        }
    }

    /// The declared file target survives every hop, and stays its own term.
    ///
    /// This is the pass-through claim: the target the table declared reaches
    /// both the selection policy and the execution configuration byte for byte,
    /// and the row-group target stays a separate, smaller term rather than
    /// being conflated with it. Clamping the target to the admitted memory, or
    /// reusing one term for both, would change the selection boundary and the
    /// file geometry without any operator asking for it. An undeclared table
    /// falls back to the deployment file target — 1 GiB unless the operator
    /// moved it, never Iceberg's 512 MiB — and the Iceberg row-group default.
    #[test]
    fn forge_table_policy_preserves_declared_target_geometry() {
        let metadata = metadata_with(vec![
            (FILE_TARGET_PROPERTY, "268435456"),
            (ROW_GROUP_TARGET_PROPERTY, "134217728"),
        ]);
        let policy = ForgeTablePolicy::extract(&metadata, &limits())
            .expect("declared geometry is admissible");

        assert_eq!(policy.target_file_size_bytes, 268_435_456);
        assert_eq!(policy.row_group_target_bytes, 134_217_728);
        let ForgeTaskPlanning::Explicit(CompactionPlanningConfig::SmallFiles(selection)) =
            policy.planning(ForgeCompactionType::SmallFiles, false)
        else {
            panic!("small-files planning is explicit");
        };
        assert_eq!(
            selection.target_file_size_bytes, 268_435_456,
            "the declared target reaches selection unchanged"
        );
        let config = policy
            .to_core_config(
                CompactionPlanningConfig::default(),
                "attempt".to_owned(),
                &["value".to_owned()],
                2,
                wyrd_spec::DataTenantId::new_v7(),
            )
            .expect("core configuration builds");
        assert_eq!(
            config.execution.target_file_size_bytes, 268_435_456,
            "the declared target reaches execution unchanged"
        );
        assert_eq!(
            config
                .execution
                .write_parquet_properties
                .max_row_group_bytes(),
            Some(134_217_728),
            "the row-group target stays an independent writer term"
        );
        assert_ne!(
            u64::try_from(
                config
                    .execution
                    .write_parquet_properties
                    .max_row_group_bytes()
                    .expect("a row-group ceiling is set")
            )
            .expect("row-group ceiling fits"),
            config.execution.target_file_size_bytes,
            "the file target and the row-group target are not one term"
        );

        let unset = ForgeTablePolicy::extract(&metadata_with(Vec::new()), &limits())
            .expect("undeclared geometry falls back to the deployment defaults");
        assert_eq!(unset.target_file_size_bytes, 1024 * 1024 * 1024);
        assert_eq!(unset.row_group_target_bytes, 128 * 1024 * 1024);
        let moved = ForgeConfig {
            default_target_file_size_bytes: 256 * 1024 * 1024,
            ..limits()
        };
        assert_eq!(
            ForgeTablePolicy::extract(&metadata_with(Vec::new()), &moved)
                .expect("a moved deployment default is admissible")
                .target_file_size_bytes,
            256 * 1024 * 1024,
            "an undeclared table follows the configured deployment default"
        );
        assert_eq!(
            ForgeTablePolicy::extract(&metadata, &moved)
                .expect("declared geometry is admissible")
                .target_file_size_bytes,
            268_435_456,
            "a declared table property wins over the deployment default"
        );
    }

    /// Each task type maps to `RisingWave`'s runner configuration, and
    /// copy-on-write forces `Full` over the whole table.
    ///
    /// # Panics
    /// Panics when a task type maps to the wrong planner, scope, or runner term.
    #[test]
    fn forge_table_policy_plans_risingwave_task_types() {
        let policy = ForgeTablePolicy::extract(&metadata_with(Vec::new()), &limits())
            .expect("default geometry is admissible");
        let ForgeTaskPlanning::Explicit(CompactionPlanningConfig::Full(full)) =
            policy.planning(ForgeCompactionType::Full, false)
        else {
            panic!("full planning is explicit");
        };
        assert_eq!(full.file_group_scope, FileGroupScope::Partition);
        assert_eq!(full.max_output_parallelism, RUNNER_MAX_PARALLELISM);
        assert!(!full.enable_heuristic_output_parallelism);
        for requested in [
            ForgeCompactionType::Auto,
            ForgeCompactionType::SmallFiles,
            ForgeCompactionType::FilesWithDelete,
            ForgeCompactionType::Full,
        ] {
            let ForgeTaskPlanning::Explicit(CompactionPlanningConfig::Full(cow)) =
                policy.planning(requested, true)
            else {
                panic!("copy-on-write forces full planning for {requested:?}");
            };
            assert_eq!(cow.file_group_scope, FileGroupScope::Table);
        }
        let ForgeTaskPlanning::Explicit(CompactionPlanningConfig::FilesWithDeletes(deletes)) =
            policy.planning(ForgeCompactionType::FilesWithDelete, false)
        else {
            panic!("files-with-delete planning is explicit");
        };
        assert_eq!(
            deletes.min_delete_file_count_threshold,
            DELETE_FILES_COUNT_THRESHOLD
        );
        assert!(matches!(
            policy.planning(ForgeCompactionType::Auto, false),
            ForgeTaskPlanning::Auto { .. }
        ));
    }

    /// Registration admits exactly the targets planning can honor under the
    /// default threshold and row-group target.
    ///
    /// # Panics
    /// Panics when a boundary target is classified wrongly.
    #[test]
    fn registrable_targets_clear_the_row_group_target_and_ceiling() {
        assert!(!registrable_target_file_size_bytes(0));
        assert!(!registrable_target_file_size_bytes(
            ROW_GROUP_TARGET_DEFAULT - 1
        ));
        assert!(registrable_target_file_size_bytes(ROW_GROUP_TARGET_DEFAULT));
        assert!(registrable_target_file_size_bytes(1 << 30));
        assert!(!registrable_target_file_size_bytes(u64::MAX));
        let registered = metadata_with(vec![(
            FILE_TARGET_PROPERTY,
            &ROW_GROUP_TARGET_DEFAULT.to_string(),
        )]);
        ForgeTablePolicy::extract(&registered, &ForgeConfig::default())
            .expect("the smallest registrable target plans under default limits");
    }

    /// Every impossible geometry is refused before any planning happens.
    ///
    /// One case per way the geometry can be impossible: a zero target, a
    /// non-numeric target, a threshold that is not below the target, a row-group
    /// target above the file target, a target whose oversized ceiling overflows,
    /// and a data location that is not the registered recipe location. Each is
    /// a condition under which an admitted attempt could only produce wrong or
    /// no work, so the refusal belongs at extraction rather than mid-rewrite.
    #[test]
    fn forge_table_policy_rejects_impossible_geometry() {
        assert!(matches!(
            ForgeTablePolicy::extract(&metadata_with(vec![(FILE_TARGET_PROPERTY, "0")]), &limits(),),
            Err(ForgeError::InvalidConfig { .. })
        ));
        assert!(matches!(
            ForgeTablePolicy::extract(
                &metadata_with(vec![(FILE_TARGET_PROPERTY, "not-a-number")]),
                &limits(),
            ),
            Err(ForgeError::InvalidConfig { .. })
        ));
        assert!(
            matches!(
                ForgeTablePolicy::extract(
                    &metadata_with(vec![(FILE_TARGET_PROPERTY, "1024")]),
                    &limits(),
                ),
                Err(ForgeError::InvalidConfig { .. })
            ),
            "a threshold at or above the target selects every file forever"
        );
        assert!(
            matches!(
                ForgeTablePolicy::extract(
                    &metadata_with(vec![
                        (FILE_TARGET_PROPERTY, "67108864"),
                        (ROW_GROUP_TARGET_PROPERTY, "134217728"),
                    ]),
                    &limits(),
                ),
                Err(ForgeError::InvalidConfig { .. })
            ),
            "a row group larger than the file forces a roll per row group"
        );
        assert!(
            matches!(
                ForgeTablePolicy::extract(
                    &metadata_with(vec![(FILE_TARGET_PROPERTY, &u64::MAX.to_string())]),
                    &limits(),
                ),
                Err(ForgeError::InvalidConfig { .. })
            ),
            "a target whose oversized ceiling overflows is refused before planning"
        );
        assert!(
            matches!(
                ForgeTablePolicy::extract(
                    &metadata_with(vec![(
                        crate::catalog::layout::WRITE_DATA_PATH_PROPERTY,
                        "file:///warehouse/tenant/table/data"
                    )]),
                    &limits(),
                ),
                Err(ForgeError::InvalidConfig { .. })
            ),
            "outputs written outside the recipe location can never settle"
        );
    }
}
