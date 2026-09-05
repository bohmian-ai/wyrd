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
use iceberg_compaction_core::config::{
    CompactionConfig, CompactionExecutionConfig, CompactionExecutionConfigBuilder,
    CompactionPlanningConfig, WyrdIdentityAwareConfig,
};
use iceberg_compaction_core::managed::{
    OpenPartitionPolicy, WriterRecipeResolver, WyrdSelectionPolicy,
};

use crate::catalog::layout::{FORGE_WRITER_RECIPE, forge_data_location};
use crate::forge::compact::ForgeConfig;
use crate::forge::error::ForgeError;

/// Iceberg property naming the encoded row-group target for Parquet writers.
///
/// Held separately from the file target on purpose: a row group is the unit a
/// reader prunes and buffers, while a file is the unit the catalog tracks.
/// Conflating them either produces one enormous row group per file or forces a
/// file to roll at every row group.
const ROW_GROUP_TARGET_PROPERTY: &str = TableProperties::PROPERTY_PARQUET_ROW_GROUP_SIZE_BYTES;

/// Default encoded row-group target when the table declares none.
///
/// Matches the pinned Iceberg default so an undeclared table behaves the same
/// under Forge as it would under any other Iceberg writer.
const ROW_GROUP_TARGET_DEFAULT: u64 =
    TableProperties::PROPERTY_PARQUET_ROW_GROUP_SIZE_BYTES_DEFAULT as u64;

/// Iceberg property naming the target size of a newly written data file.
const FILE_TARGET_PROPERTY: &str = TableProperties::PROPERTY_WRITE_TARGET_FILE_SIZE_BYTES;

/// Default target file size when the table declares none.
const FILE_TARGET_DEFAULT: u64 =
    TableProperties::PROPERTY_WRITE_TARGET_FILE_SIZE_BYTES_DEFAULT as u64;

/// Multiplier the managed core applies to derive its oversized ceiling.
///
/// Mirrored here only so this owner can refuse a target whose ceiling would
/// overflow *before* the core is constructed, which is the difference between a
/// refusal an operator can read and a failure deep inside planning.
const OVERSIZED_CEILING_PERCENT: u64 = 180;

/// The complete, already-consistent terms one managed rewrite executes under.
///
/// Every field is derived, never defaulted at the point of use: the identity
/// triple and both size targets come from the table, the small-file threshold
/// and plan budget come from validated operator limits, and the data location
/// and writer recipe come from the registered Forge path layout. Construction
/// is the only place these can disagree, so construction is where they are
/// checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForgeTablePolicy {
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
    /// Maximum core plans one attempt may execute before it must yield.
    pub(crate) max_plans_per_attempt: usize,
}

/// Reads the target output size one table declares for its data files.
///
/// Exposed separately from [`ForgeTablePolicy::extract`] because the audit row
/// for a publication records the same target the writer used, and reconstructing
/// the whole policy there would demand an admitted memory grant the publication
/// no longer holds. Both paths read the one property, so they cannot disagree.
///
/// # Errors
///
/// Returns [`ForgeError::InvalidConfig`] when the declared value is not a
/// positive integer byte count.
pub(crate) fn declared_target_file_size_bytes(metadata: &TableMetadata) -> Result<u64, ForgeError> {
    declared_bytes(
        metadata.properties(),
        FILE_TARGET_PROPERTY,
        FILE_TARGET_DEFAULT,
    )
}

impl ForgeTablePolicy {
    /// Derives one attempt's policy from a loaded table and validated limits.
    ///
    /// `admitted_memory_bytes` is the memory the root governor actually granted
    /// this attempt, not a configured ceiling: a rewrite cannot buffer a row
    /// group it was never admitted the memory to hold, and finding that out
    /// here is what keeps it from being discovered as an out-of-memory failure
    /// mid-write.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when a declared size is
    /// unparseable or zero, when the small-file threshold is not strictly below
    /// the file target, when the row-group target exceeds the file target, when
    /// the file target cannot be represented on this platform or would overflow
    /// the core's oversized ceiling, when the plan budget is zero, or when the
    /// table's declared data location is not the registered Forge recipe
    /// location. Returns [`ForgeError::Capacity`] when the admitted memory is
    /// below one row-group target.
    pub(crate) fn extract(
        metadata: &TableMetadata,
        config: &ForgeConfig,
        admitted_memory_bytes: u64,
    ) -> Result<Self, ForgeError> {
        let properties = metadata.properties();
        let target_file_size_bytes =
            declared_bytes(properties, FILE_TARGET_PROPERTY, FILE_TARGET_DEFAULT)?;
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
            max_plans_per_attempt: config.rewrite_max_plans_per_attempt,
        };
        policy.validate(admitted_memory_bytes)?;
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
    fn validate(&self, admitted_memory_bytes: u64) -> Result<(), ForgeError> {
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
        if self.max_plans_per_attempt == 0 {
            return refuse("rewrite plan budget must admit at least one plan".to_owned());
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
        if admitted_memory_bytes < self.row_group_target_bytes {
            return Err(ForgeError::Capacity {
                detail: format!(
                    "admitted rewrite memory {admitted_memory_bytes} is below one \
                     row-group target of {}",
                    self.row_group_target_bytes
                ),
            });
        }
        Ok(())
    }

    /// Projects this policy onto the core's canonical selection policy.
    ///
    /// Both size terms cross unchanged: the core derives its own oversized
    /// ceiling from the target, so clamping or rounding here would move the
    /// selection boundary away from the geometry the table declared.
    ///
    /// Open-partition suppression is left off deliberately. Bifrost's demand
    /// side decides *which* tables are worth an attempt; the selection policy's
    /// job is only to describe what a settled file looks like, and a
    /// still-filling partition that is genuinely fragmented is not exempt from
    /// that. Repeated no-op churn is prevented by the semantic-debt refusal,
    /// not by hiding partitions from the selector.
    pub(crate) fn to_selection_policy(&self) -> WyrdSelectionPolicy {
        WyrdSelectionPolicy {
            schema_id: self.schema_id,
            partition_spec_id: self.partition_spec_id,
            sort_order_id: self.sort_order_id,
            writer_recipe: self.writer_recipe.clone(),
            recipe_resolver: WriterRecipeResolver::forge(),
            target_file_size_bytes: self.target_file_size_bytes,
            small_file_threshold_bytes: self.small_file_threshold_bytes,
            open_partitions: OpenPartitionPolicy::AllClosed,
            emit_open_partition_tail: false,
            event_time_field_id: None,
        }
    }

    /// Builds the complete core configuration one attempt executes under.
    ///
    /// `data_file_prefix` is the attempt identity, which is what makes every
    /// object an attempt produced attributable to it by path alone.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the derived execution
    /// configuration cannot be built, which can only happen if a required
    /// builder term is left unset.
    pub(crate) fn to_core_config(
        &self,
        data_file_prefix: String,
        bloom_columns: &[String],
        max_concurrent_closes: usize,
        memory_bytes: usize,
        spill_dir: std::path::PathBuf,
    ) -> Result<Arc<CompactionConfig>, ForgeError> {
        let execution: CompactionExecutionConfig = CompactionExecutionConfigBuilder::default()
            .target_file_size_bytes(self.target_file_size_bytes)
            .data_file_prefix(data_file_prefix)
            .max_concurrent_closes(max_concurrent_closes)
            .write_parquet_properties(crate::parquet::bifrost_rewrite_writer_properties(
                self.row_group_target_bytes,
                bloom_columns,
            ))
            .max_concurrent_compaction_plans(self.max_plans_per_attempt)
            .max_memory_bytes(Some(memory_bytes))
            .spill_dir(Some(spill_dir))
            .build()
            .map_err(|error| ForgeError::InvalidConfig {
                detail: format!("Forge rewrite execution configuration is incomplete: {error}"),
            })?;
        Ok(Arc::new(CompactionConfig::new(
            CompactionPlanningConfig::WyrdIdentityAware(
                WyrdIdentityAwareConfig::new(self.to_selection_policy())
                    .with_max_selection_plans(self.max_plans_per_attempt),
            ),
            execution,
        )))
    }
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
    use iceberg::scan::FileScanTask;
    use iceberg::spec::DataFileFormat;
    use iceberg_compaction_core::file_selection::FileGroup;

    /// Native rolling receives each selected group without premature stream residue.
    ///
    /// # Panics
    /// Panics if valid policy projection or native planning fails, or selected
    /// rows are split between writers before the encoded file target can roll.
    #[test]
    fn forge_table_policy_keeps_rolling_stream_whole() {
        let metadata = metadata_with(vec![(FILE_TARGET_PROPERTY, "1073741824")]);
        let policy = ForgeTablePolicy::extract(
            &metadata,
            &ForgeConfig {
                small_file_threshold_bytes: 768 * 1024 * 1024,
                ..limits()
            },
            1024 * 1024 * 1024,
        ).expect("valid production geometry");
        let config = policy.to_core_config(
            "attempt".to_owned(), &[], 2, 1024 * 1024 * 1024,
            std::path::PathBuf::from("/tmp"),
        ).expect("native configuration");
        for sizes_mib in [&[700_u64, 700][..], &[256, 256], &[2048]] {
            let files = sizes_mib.iter().enumerate().map(|(index, size)| FileScanTask {
                start: 0,
                length: size * 1024 * 1024,
                record_count: Some(100),
                first_row_id: None,
                data_sequence_number: None,
                data_file_path: format!("file:///warehouse/input-{index}.parquet"),
                data_file_format: DataFileFormat::Parquet,
                schema: Arc::clone(metadata.current_schema()),
                project_field_ids: vec![1],
                predicate: None,
                deletes: vec![],
                sequence_number: 1,
                file_size_in_bytes: size * 1024 * 1024,
                partition: None,
                partition_spec: None,
                name_mapping: None,
                unified_partition_type: None,
                case_sensitive: true,
                key_metadata: None,
            }).collect();
            let group = FileGroup::with_parallelism(files, &config.planning)
                .expect("native group parallelism");
            assert_eq!(group.output_parallelism, 1,
                "{sizes_mib:?} MiB must reach one rolling stream, allowing target files and a remainder");
        }
    }

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
    /// falls back to the pinned Iceberg defaults rather than to nothing.
    #[test]
    fn forge_table_policy_preserves_declared_target_geometry() {
        let metadata = metadata_with(vec![
            (FILE_TARGET_PROPERTY, "268435456"),
            (ROW_GROUP_TARGET_PROPERTY, "134217728"),
        ]);
        let policy = ForgeTablePolicy::extract(&metadata, &limits(), 1024 * 1024 * 1024)
            .expect("declared geometry is admissible");

        assert_eq!(policy.target_file_size_bytes, 268_435_456);
        assert_eq!(policy.row_group_target_bytes, 134_217_728);
        assert_eq!(
            policy.to_selection_policy().target_file_size_bytes,
            268_435_456,
            "the declared target reaches selection unchanged"
        );
        let config = policy
            .to_core_config(
                "attempt".to_owned(),
                &["value".to_owned()],
                2,
                512 * 1024 * 1024,
                std::path::PathBuf::from("/tmp"),
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

        let unset = ForgeTablePolicy::extract(&metadata_with(Vec::new()), &limits(), 1 << 30)
            .expect("undeclared geometry falls back to the pinned Iceberg defaults");
        assert_eq!(unset.target_file_size_bytes, FILE_TARGET_DEFAULT);
        assert_eq!(unset.row_group_target_bytes, ROW_GROUP_TARGET_DEFAULT);
    }

    /// Every impossible geometry is refused before any planning happens.
    ///
    /// One case per way the geometry can be impossible: a zero target, a
    /// non-numeric target, a threshold that is not below the target, a row-group
    /// target above the file target, a target whose oversized ceiling overflows,
    /// a data location that is not the registered recipe location, admitted
    /// memory too small to hold one row group, and a zero plan budget. Each is
    /// a condition under which an admitted attempt could only produce wrong or
    /// no work, so the refusal belongs at extraction rather than mid-rewrite.
    #[test]
    fn forge_table_policy_rejects_impossible_geometry() {
        let metadata = metadata_with(vec![
            (FILE_TARGET_PROPERTY, "268435456"),
            (ROW_GROUP_TARGET_PROPERTY, "134217728"),
        ]);
        assert!(matches!(
            ForgeTablePolicy::extract(
                &metadata_with(vec![(FILE_TARGET_PROPERTY, "0")]),
                &limits(),
                1 << 30
            ),
            Err(ForgeError::InvalidConfig { .. })
        ));
        assert!(matches!(
            ForgeTablePolicy::extract(
                &metadata_with(vec![(FILE_TARGET_PROPERTY, "not-a-number")]),
                &limits(),
                1 << 30
            ),
            Err(ForgeError::InvalidConfig { .. })
        ));
        assert!(
            matches!(
                ForgeTablePolicy::extract(
                    &metadata_with(vec![(FILE_TARGET_PROPERTY, "1024")]),
                    &limits(),
                    1 << 30
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
                    1 << 30
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
                    1 << 30
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
                    1 << 30
                ),
                Err(ForgeError::InvalidConfig { .. })
            ),
            "outputs written outside the recipe location can never settle"
        );
        assert!(
            matches!(
                ForgeTablePolicy::extract(&metadata, &limits(), 1024),
                Err(ForgeError::Capacity { .. })
            ),
            "an attempt that cannot hold one row group is refused as capacity"
        );
        assert!(
            matches!(
                ForgeTablePolicy::extract(
                    &metadata,
                    &ForgeConfig {
                        rewrite_max_plans_per_attempt: 0,
                        ..limits()
                    },
                    1 << 30
                ),
                Err(ForgeError::InvalidConfig { .. })
            ),
            "a zero plan budget would admit an attempt that can do nothing"
        );
    }
}
