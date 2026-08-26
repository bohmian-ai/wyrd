//! Current-snapshot manifest discovery for deterministic Forge rewrite plans.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use iceberg::spec::{
    DataContentType, DataFileFormat, ManifestContentType, PrimitiveLiteral, PrimitiveType,
};
use iceberg::table::Table;
use wyrd_spec::vala::WYRD_EVENT_TIME;

use super::Forge;
use super::error::ForgeError;
use super::path::catalog_path_to_object_key;
use super::right_size::{
    ForgeRightSizePolicy, IcebergCandidateFile, IcebergConvergence, IcebergRewriteGroup,
    IcebergRewriteReason, IcebergTablePlan, validate_supported_layout,
};
use crate::catalog::TenantTableBinding;
use crate::catalog::TimeGranularity;
use crate::catalog::layout::TimePartition;

/// Checked count and byte ceilings for one live Iceberg manifest projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ForgeDiscoveryLimits {
    /// Maximum live data files accepted from one current snapshot.
    pub(super) max_files: usize,
    /// Maximum checked sum of live data-file bytes accepted from one snapshot.
    pub(super) max_bytes: u64,
}

impl ForgeDiscoveryLimits {
    /// Constructs the discovery envelope from the already validated Forge configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when either ceiling is zero.
    pub(super) fn from_config(config: &super::ForgeConfig) -> Result<Self, ForgeError> {
        if config.max_files_per_tick == 0 || config.max_bytes_per_tick == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge live discovery limits must be positive".to_owned(),
            });
        }
        Ok(Self {
            max_files: config.max_files_per_tick,
            max_bytes: config.max_bytes_per_tick,
        })
    }
}

impl Forge {
    /// Inspect the current live data files and production right-size bounds.
    ///
    /// This test-support projection runs the same manifest validation used by
    /// production discovery and performs no catalog or object-store writes.
    ///
    /// # Errors
    ///
    /// Returns catalog, layout, metadata, or manifest invariant errors from
    /// production discovery.
    #[cfg(feature = "test-support")]
    pub async fn inspect_live_files_for_test(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
    ) -> Result<(i64, ForgeRightSizePolicy, Vec<IcebergCandidateFile>), ForgeError> {
        let snapshot =
            table
                .metadata()
                .current_snapshot()
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "Forge inspection requires a current snapshot".to_owned(),
                })?;
        let schema_id = snapshot.schema_id().ok_or_else(|| ForgeError::Invariant {
            detail: "current snapshot lacks a schema identity".to_owned(),
        })?;
        let schema =
            table
                .metadata()
                .schema_by_id(schema_id)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "current snapshot schema is absent from metadata".to_owned(),
                })?;
        let granularity = validate_supported_layout(
            schema,
            table.metadata().default_partition_spec(),
        )?;
        let target = u64::try_from(
            table
                .metadata()
                .table_properties()
                .map_err(ForgeError::Catalog)?
                .write_target_file_size_bytes,
        )
        .map_err(|_| ForgeError::Invariant {
            detail: "Iceberg target file size exceeds u64".to_owned(),
        })?;
        let policy = ForgeRightSizePolicy::from_table_threshold(
            target,
            self.core.config.small_file_threshold_bytes,
            schema_id,
            table.metadata().default_partition_spec_id(),
            table.metadata().default_sort_order_id(),
        )?;
        let limits = ForgeDiscoveryLimits::from_config(&self.core.config)?;
        let mut files = self
            .live_candidates_bounded(binding, table, snapshot, granularity, limits)
            .await?;
        files.sort_by(|left, right| left.catalog_path().cmp(right.catalog_path()));
        Ok((snapshot.snapshot_id(), policy, files))
    }

    /// Load one current Iceberg snapshot and produce its deterministic live-file plan.
    ///
    /// Manifest IO is bounded to the snapshot selected by `table`; all candidate
    /// validation completes before the pure right-size policy receives a group.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] for manifest reads and
    /// [`ForgeError::Invariant`] for malformed live manifest identities, bounds,
    /// partition values, or paths. This method performs no catalog or object write.
    pub(super) async fn discover_live_rewrites(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        now: DateTime<Utc>,
    ) -> Result<IcebergTablePlan, ForgeError> {
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Ok(IcebergTablePlan {
                base_snapshot_id: 0,
                groups: Vec::new(),
                convergence: IcebergConvergence::Converged,
            });
        };
        let schema_id = snapshot.schema_id().ok_or_else(|| ForgeError::Invariant {
            detail: "current snapshot lacks a schema identity".to_owned(),
        })?;
        let policy_schema =
            table
                .metadata()
                .schema_by_id(schema_id)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: format!(
                        "current snapshot schema {schema_id} is absent from table metadata"
                    ),
                })?;
        let granularity = validate_supported_layout(
            policy_schema,
            table.metadata().default_partition_spec(),
        )?;
        let target = u64::try_from(
            table
                .metadata()
                .table_properties()
                .map_err(ForgeError::Catalog)?
                .write_target_file_size_bytes,
        )
        .map_err(|_| ForgeError::Invariant {
            detail: "Iceberg target file size exceeds u64".to_owned(),
        })?;
        let policy = ForgeRightSizePolicy::from_table_threshold(
            target,
            self.core.config.small_file_threshold_bytes,
            schema_id,
            table.metadata().default_partition_spec_id(),
            table.metadata().default_sort_order_id(),
        )?;
        let limits = ForgeDiscoveryLimits::from_config(&self.core.config)?;
        Ok(plan_candidates(
            &policy,
            self.live_candidates_bounded(binding, table, snapshot, granularity, limits)
                .await?,
            snapshot.snapshot_id(),
            now,
        ))
    }

    /// Reconstructs one exact durable live-rewrite input set from its base snapshot.
    ///
    /// This path does not rerun usefulness grouping with a later wall-clock day.
    /// The durable input paths are authoritative, while current-snapshot
    /// manifests prove that every file remains live and shares one physical
    /// schema, partition specification, day, and sort recipe.
    ///
    /// # Errors
    ///
    /// Returns catalog or invariant errors when inputs are missing, duplicated,
    /// unsafe, or no longer form one compatible rewrite group.
    pub(super) async fn discover_exact_live_group(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        inputs: &[String],
    ) -> Result<IcebergRewriteGroup, ForgeError> {
        let snapshot =
            table
                .metadata()
                .current_snapshot()
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: "exact live rewrite has no current snapshot".to_owned(),
                })?;
        let schema_id = snapshot.schema_id().ok_or_else(|| ForgeError::Invariant {
            detail: "exact live rewrite snapshot lacks a schema identity".to_owned(),
        })?;
        let schema =
            table
                .metadata()
                .schema_by_id(schema_id)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "exact live rewrite schema is absent from table metadata".to_owned(),
                })?;
        let granularity = validate_supported_layout(
            schema,
            table.metadata().default_partition_spec(),
        )?;
        let candidates = self
            .live_candidates(binding, table, snapshot, granularity)
            .await?;
        let planned = inputs.iter().map(String::as_str).collect::<BTreeSet<_>>();
        if planned.len() != inputs.len() {
            return Err(ForgeError::Invariant {
                detail: "exact live rewrite inputs contain duplicates".to_owned(),
            });
        }
        let files = candidates
            .into_iter()
            .filter(|file| planned.contains(file.catalog_path.as_str()))
            .collect::<Vec<_>>();
        if files.len() != inputs.len() {
            return Err(ForgeError::Reconciliation {
                detail: "exact live rewrite inputs changed before execution".to_owned(),
            });
        }
        let first = files.first().ok_or_else(|| ForgeError::Invariant {
            detail: "exact live rewrite input set is empty".to_owned(),
        })?;
        if files.iter().any(|file| {
            file.schema_id != first.schema_id
                || file.partition_spec_id != first.partition_spec_id
                || file.partition != first.partition
                || file.sort_order_id != first.sort_order_id
                || file.writer_recipe_version != first.writer_recipe_version
        }) {
            return Err(ForgeError::Reconciliation {
                detail: "exact live rewrite inputs no longer share one physical group".to_owned(),
            });
        }
        Ok(IcebergRewriteGroup {
            files,
            reason: IcebergRewriteReason::Undersized,
        })
    }

    /// Load one current-snapshot rewrite plan for catalog integration tests.
    ///
    /// This route exists only with the existing `test-support` feature. It
    /// preserves production encapsulation while allowing the real catalog
    /// fixture to assert manifest-derived planning facts.
    ///
    /// # Errors
    ///
    /// Returns the same catalog and invariant errors as the private discovery
    /// workflow when the observed snapshot cannot form a valid table plan.
    #[cfg(feature = "test-support")]
    pub async fn discover_live_rewrites_for_test(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        now: DateTime<Utc>,
    ) -> Result<IcebergTablePlan, ForgeError> {
        self.discover_live_rewrites(binding, table, now).await
    }

    /// Convert each alive data entry in one observed snapshot into a candidate.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] for manifest reads and
    /// [`ForgeError::Invariant`] for malformed manifest-owned data identities.
    async fn live_candidates(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        snapshot: &iceberg::spec::SnapshotRef,
        granularity: TimeGranularity,
    ) -> Result<Vec<IcebergCandidateFile>, ForgeError> {
        self.live_candidates_with_limits(binding, table, snapshot, granularity, None)
            .await
    }

    /// Converts alive entries while enforcing one checked discovery envelope.
    ///
    /// # Errors
    ///
    /// Returns capacity, catalog, or manifest invariant errors. Cancellation
    /// leaves no durable state or retained partial projection.
    pub(super) async fn live_candidates_bounded(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        snapshot: &iceberg::spec::SnapshotRef,
        granularity: TimeGranularity,
        limits: ForgeDiscoveryLimits,
    ) -> Result<Vec<IcebergCandidateFile>, ForgeError> {
        self.live_candidates_with_limits(binding, table, snapshot, granularity, Some(limits))
            .await
    }

    /// Implements optionally bounded manifest projection for planning and recovery.
    ///
    /// # Errors
    ///
    /// Returns capacity, catalog, or manifest invariant errors. Cancellation
    /// performs no durable write.
    async fn live_candidates_with_limits(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        snapshot: &iceberg::spec::SnapshotRef,
        granularity: TimeGranularity,
        limits: Option<ForgeDiscoveryLimits>,
    ) -> Result<Vec<IcebergCandidateFile>, ForgeError> {
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        let mut candidates = Vec::new();
        let mut seen_paths = BTreeSet::new();
        let mut live_bytes = 0_u64;
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .map_err(ForgeError::Catalog)?;
            if reject_live_delete_entries(
                *manifest.metadata().content(),
                manifest.entries().iter().any(|entry| entry.is_alive()),
            )? {
                continue;
            }
            let writer_schema_id = manifest.metadata().schema_id();
            let writer_schema =
                table
                    .metadata()
                    .schema_by_id(writer_schema_id)
                    .ok_or_else(|| ForgeError::Invariant {
                        detail: format!(
                            "manifest schema {writer_schema_id} is absent from table metadata"
                        ),
                    })?;
            let event_time_field_id = writer_schema
                .field_by_name(WYRD_EVENT_TIME)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "manifest writer schema lacks wyrd_event_time".to_owned(),
                })?
                .id;
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                if entry.content_type() != DataContentType::Data
                    || entry.file_format() != DataFileFormat::Parquet
                {
                    return Err(ForgeError::Invariant {
                        detail: format!("unsupported live Iceberg file: {}", entry.file_path()),
                    });
                }
                let catalog_path = entry.file_path().to_owned();
                if !seen_paths.insert(catalog_path.clone()) {
                    return Err(ForgeError::Invariant {
                        detail: format!("duplicate live Iceberg catalog path: {catalog_path}"),
                    });
                }
                let data_file = entry.data_file();
                live_bytes = advance_discovery_envelope(
                    candidates.len(),
                    live_bytes,
                    data_file.file_size_in_bytes(),
                    limits,
                )?;
                let partition = decode_partition(data_file.partition().fields(), granularity)?;
                let min_event_time =
                    event_time_bound(data_file.lower_bounds(), event_time_field_id)?;
                let max_event_time =
                    event_time_bound(data_file.upper_bounds(), event_time_field_id)?;
                validate_event_time_range(min_event_time, max_event_time, &catalog_path)?;
                candidates.push(IcebergCandidateFile {
                    object_path: catalog_path_to_object_key(
                        table.metadata().location(),
                        binding,
                        &self.core.staging,
                        &catalog_path,
                    )?,
                    catalog_path,
                    file_size_bytes: data_file.file_size_in_bytes(),
                    record_count: data_file.record_count(),
                    schema_id: writer_schema_id,
                    partition_spec_id: manifest.metadata().partition_spec().spec_id(),
                    partition,
                    sort_order_id: data_file.sort_order_id().map(i64::from),
                    writer_recipe_version: writer_recipe_version(data_file.file_path()),
                    min_event_time,
                    max_event_time,
                    source_snapshot_id: entry.snapshot_id().ok_or_else(|| {
                        ForgeError::Invariant {
                            detail: "live manifest entry lacks source snapshot identity".to_owned(),
                        }
                    })?,
                    data_sequence_number: entry.sequence_number(),
                    file_sequence_number: entry.file_sequence_number,
                });
            }
        }
        Ok(candidates)
    }
}

/// Advances the checked live-file envelope before retaining one candidate.
///
/// # Errors
///
/// Returns an invariant error for count overflow and capacity pressure for byte
/// overflow or either configured ceiling.
fn advance_discovery_envelope(
    current_files: usize,
    current_bytes: u64,
    file_bytes: u64,
    limits: Option<ForgeDiscoveryLimits>,
) -> Result<u64, ForgeError> {
    let next_files = current_files
        .checked_add(1)
        .ok_or_else(|| ForgeError::Invariant {
            detail: "Forge live discovery file count overflowed".to_owned(),
        })?;
    let next_bytes = current_bytes
        .checked_add(file_bytes)
        .ok_or_else(|| ForgeError::Capacity {
            detail: "Forge live discovery byte sum overflowed".to_owned(),
        })?;
    if limits.is_some_and(|limit| next_files > limit.max_files || next_bytes > limit.max_bytes) {
        return Err(ForgeError::Capacity {
            detail: "Forge live snapshot exceeds the configured discovery envelope".to_owned(),
        });
    }
    Ok(next_bytes)
}

/// Reject a current snapshot that retains a live delete-file entry.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when a delete manifest contains a live
/// entry; dead delete entries are historical and therefore safe to skip.
fn reject_live_delete_entries(
    content: ManifestContentType,
    has_live_entries: bool,
) -> Result<bool, ForgeError> {
    if content != ManifestContentType::Deletes {
        return Ok(false);
    }
    if has_live_entries {
        return Err(ForgeError::Invariant {
            detail: "current snapshot contains a live delete manifest entry".to_owned(),
        });
    }
    Ok(true)
}

/// Sort and group a fully validated snapshot candidate set without performing IO.
fn plan_candidates(
    policy: &ForgeRightSizePolicy,
    mut candidates: Vec<IcebergCandidateFile>,
    base_snapshot_id: i64,
    now: DateTime<Utc>,
) -> IcebergTablePlan {
    candidates.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    let mut by_partition = BTreeMap::<(i32, TimePartition), Vec<IcebergCandidateFile>>::new();
    for candidate in candidates {
        by_partition
            .entry((candidate.partition_spec_id, candidate.partition))
            .or_default()
            .push(candidate);
    }
    let mut groups = Vec::new();
    for ((_, partition), candidates) in by_partition {
        groups.extend(
            policy
                .plan_partition(candidates, partition.is_open_at(now))
                .groups,
        );
    }
    IcebergTablePlan {
        base_snapshot_id,
        convergence: if groups.is_empty() {
            IcebergConvergence::Converged
        } else {
            IcebergConvergence::Pending
        },
        groups,
    }
}

/// Decode the required time-partition value from Forge's single-field layout.
///
/// `granularity` comes from the table's own default partition spec, so an
/// hourly table decodes hours-since-epoch and a daily table decodes
/// days-since-epoch from the same integer literal.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the partition tuple is absent, is not
/// Iceberg's integer transform representation, or names an unrepresentable
/// instant.
fn decode_partition(
    fields: &[Option<iceberg::spec::Literal>],
    granularity: TimeGranularity,
) -> Result<TimePartition, ForgeError> {
    let Some(Some(iceberg::spec::Literal::Primitive(PrimitiveLiteral::Int(value)))) =
        fields.first()
    else {
        return Err(ForgeError::Invariant {
            detail: "live manifest entry has an invalid time partition".to_owned(),
        });
    };
    TimePartition::from_iceberg_transform_value(granularity, *value).map_err(|error| {
        ForgeError::Invariant {
            detail: format!("live manifest entry has an out-of-range time partition: {error}"),
        }
    })
}

/// Decode one timestamp-with-time-zone metric bound in microseconds.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the metric is absent, non-timestamp,
/// or outside Chrono's representable UTC range.
fn event_time_bound(
    bounds: &std::collections::HashMap<i32, iceberg::spec::Datum>,
    field_id: i32,
) -> Result<DateTime<Utc>, ForgeError> {
    let datum = bounds.get(&field_id).ok_or_else(|| ForgeError::Invariant {
        detail: "live manifest entry lacks wyrd_event_time bounds".to_owned(),
    })?;
    if datum.data_type() != &PrimitiveType::Timestamptz {
        return Err(ForgeError::Invariant {
            detail: "live manifest entry has non-timestamp wyrd_event_time bounds".to_owned(),
        });
    }
    let PrimitiveLiteral::Long(micros) = datum.literal() else {
        return Err(ForgeError::Invariant {
            detail: "live manifest entry has non-timestamp wyrd_event_time bounds".to_owned(),
        });
    };
    DateTime::from_timestamp_micros(*micros).ok_or_else(|| ForgeError::Invariant {
        detail: "live manifest entry has out-of-range wyrd_event_time bounds".to_owned(),
    })
}

/// Ensure decoded event-time bounds describe a non-empty chronological range.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the lower bound follows the upper
/// bound, preventing an invalid candidate from reaching the pure planner.
fn validate_event_time_range(
    min_event_time: DateTime<Utc>,
    max_event_time: DateTime<Utc>,
    catalog_path: &str,
) -> Result<(), ForgeError> {
    if min_event_time > max_event_time {
        return Err(ForgeError::Invariant {
            detail: format!("Iceberg event-time bounds are inverted: {catalog_path}"),
        });
    }
    Ok(())
}

/// Extract the Forge recipe only from the established rewrite object layout.
fn writer_recipe_version(path: &str) -> Option<String> {
    let (_, recipe_and_file) = path.split_once("/data/forge/")?;
    let (recipe, file) = recipe_and_file.split_once('/')?;
    (!recipe.is_empty() && file.ends_with(".parquet")).then(|| recipe.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::parquet::writer_properties::BIFROST_WRITER_RECIPE_VERSION;

    /// Builds the fixture hour partition shared by the discovery regressions.
    fn hour(epoch_hour: i64) -> TimePartition {
        TimePartition::new(
            TimeGranularity::Hour,
            DateTime::from_timestamp(epoch_hour * 3_600, 0).expect("fixture hour is representable"),
        )
        .expect("fixture hour is an exact hour boundary")
    }

    /// Build a complete candidate so equality proves every manifest identity field.
    fn candidate(path: &str, partition: TimePartition, schema_id: i32) -> IcebergCandidateFile {
        IcebergCandidateFile {
            catalog_path: path.to_owned(),
            object_path: path.to_owned(),
            file_size_bytes: 10,
            record_count: 3,
            schema_id,
            partition_spec_id: 1,
            partition,
            sort_order_id: Some(1),
            writer_recipe_version: Some("bifrost-writer-v1".to_owned()),
            min_event_time: DateTime::from_timestamp(1, 0).expect("fixed timestamp is valid"),
            max_event_time: DateTime::from_timestamp(2, 0).expect("fixed timestamp is valid"),
            source_snapshot_id: 42,
            data_sequence_number: Some(7),
            file_sequence_number: Some(8),
        }
    }

    /// Proves manifest enumeration order cannot change the complete table plan.
    #[test]
    fn manifest_candidates_are_stable_across_manifest_order() {
        let day = hour(400_000);
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy is valid");
        let first = plan_candidates(
            &policy,
            vec![
                candidate("b.parquet", day, 1),
                candidate("a.parquet", day, 0),
            ],
            42,
            day.start_utc(),
        );
        let second = plan_candidates(
            &policy,
            vec![
                candidate("a.parquet", day, 0),
                candidate("b.parquet", day, 1),
            ],
            42,
            day.start_utc(),
        );

        assert_eq!(first, second);
        assert_eq!(first.groups[0].files[0].catalog_path, "a.parquet");
        assert_eq!(first.groups[0].files[0].source_snapshot_id, 42);
        assert!(matches!(
            first.groups[0].reason,
            super::super::right_size::IcebergRewriteReason::ObsoleteSchema
        ));
    }

    /// Proves open-partition treatment follows the supplied wall clock against
    /// each partition's own exclusive end, so an hourly table closes hourly.
    #[test]
    fn open_partition_uses_supplied_wall_clock() {
        let current_hour = hour(400_000);
        let now = current_hour.start_utc();
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy is valid");
        let previous = hour(399_999);
        let next = hour(400_001);
        let current = |path: &str, partition| {
            let mut file = candidate(path, partition, 1);
            file.writer_recipe_version = Some(BIFROST_WRITER_RECIPE_VERSION.to_owned());
            file
        };

        assert_eq!(
            plan_candidates(
                &policy,
                vec![current("a", previous), current("b", previous)],
                1,
                now
            )
            .groups
            .len(),
            1
        );
        assert_eq!(
            plan_candidates(
                &policy,
                vec![current("a", current_hour), current("b", current_hour)],
                1,
                now
            )
            .groups
            .len(),
            0
        );
        assert_eq!(
            plan_candidates(
                &policy,
                vec![current("a", next), current("b", next)],
                1,
                now
            )
            .groups
            .len(),
            0
        );
    }

    /// Proves required timestamp metrics reject missing, malformed, and out-of-range values.
    #[test]
    fn event_time_bounds_reject_missing_and_malformed_values() {
        let bounds = HashMap::new();
        assert!(event_time_bound(&bounds, 7).is_err());
        let malformed =
            iceberg::spec::Datum::try_from_bytes(&1_i32.to_le_bytes(), PrimitiveType::Int)
                .expect("integer datum parses");
        assert!(event_time_bound(&HashMap::from([(7, malformed)]), 7).is_err());
        let out_of_range = iceberg::spec::Datum::try_from_bytes(
            &i64::MAX.to_le_bytes(),
            PrimitiveType::Timestamptz,
        )
        .expect("timestamp datum parses");
        assert!(event_time_bound(&HashMap::from([(7, out_of_range)]), 7).is_err());
        assert!(decode_partition(&[], TimeGranularity::Hour).is_err());
    }

    /// Proves inverted decoded bounds fail before candidate planning.
    #[test]
    fn event_time_bounds_reject_inverted_range() {
        let min = DateTime::from_timestamp(2, 0).expect("fixed timestamp is valid");
        let max = DateTime::from_timestamp(1, 0).expect("fixed timestamp is valid");
        assert!(validate_event_time_range(min, max, "data/part.parquet").is_err());
    }

    /// Proves only a live delete entry blocks current-snapshot optimization.
    #[test]
    fn live_delete_manifest_blocks_optimization() {
        assert!(reject_live_delete_entries(ManifestContentType::Deletes, true).is_err());
        assert!(
            reject_live_delete_entries(ManifestContentType::Deletes, false)
                .expect("dead delete entries are skipped")
        );
        assert!(
            !reject_live_delete_entries(ManifestContentType::Data, true)
                .expect("data manifests remain eligible")
        );
    }
}
