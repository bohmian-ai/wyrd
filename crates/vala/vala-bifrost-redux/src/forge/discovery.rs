//! Current-snapshot manifest discovery for deterministic Forge rewrite plans.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use iceberg::spec::{
    DataContentType, DataFileFormat, ManifestContentType, PrimitiveLiteral, PrimitiveType,
};
use iceberg::table::Table;
use wyrd_spec::vala::WYRD_EVENT_TIME;

use super::Forge;
use super::error::ForgeError;
use super::path::catalog_path_to_object_key;
use super::right_size::{
    ForgeRightSizePolicy, IcebergCandidateFile, IcebergConvergence, IcebergTablePlan,
    validate_supported_layout,
};
use crate::catalog::TenantTableBinding;

impl Forge {
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
    async fn discover_live_rewrites(
        &self,
        binding: &TenantTableBinding,
        table: &Table,
        current_day: NaiveDate,
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
        validate_supported_layout(
            policy_schema,
            table.metadata().default_partition_spec(),
            table.metadata().default_sort_order(),
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
        let policy = ForgeRightSizePolicy::new(
            target,
            schema_id,
            table.metadata().default_partition_spec_id(),
            table.metadata().default_sort_order_id(),
        )?;
        Ok(plan_candidates(
            &policy,
            self.live_candidates(binding, table, snapshot).await?,
            snapshot.snapshot_id(),
            current_day,
        ))
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
        current_day: NaiveDate,
    ) -> Result<IcebergTablePlan, ForgeError> {
        self.discover_live_rewrites(binding, table, current_day)
            .await
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
    ) -> Result<Vec<IcebergCandidateFile>, ForgeError> {
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        let mut candidates = Vec::new();
        let mut seen_paths = BTreeSet::new();
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
                let partition_day = partition_day(data_file.partition().fields())?;
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
                    partition_day,
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
    current_day: NaiveDate,
) -> IcebergTablePlan {
    candidates.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    let mut by_partition = BTreeMap::<(i32, NaiveDate), Vec<IcebergCandidateFile>>::new();
    for candidate in candidates {
        by_partition
            .entry((candidate.partition_spec_id, candidate.partition_day))
            .or_default()
            .push(candidate);
    }
    let mut groups = Vec::new();
    for ((_, partition_day), candidates) in by_partition {
        groups.extend(
            policy
                .plan_partition(candidates, partition_day >= current_day)
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

/// Decode the required day partition value from Forge's single-field layout.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the partition tuple is absent or is
/// not Iceberg's integer days-since-epoch representation.
fn partition_day(fields: &[Option<iceberg::spec::Literal>]) -> Result<NaiveDate, ForgeError> {
    let Some(Some(iceberg::spec::Literal::Primitive(PrimitiveLiteral::Int(days)))) = fields.first()
    else {
        return Err(ForgeError::Invariant {
            detail: "live manifest entry has an invalid day partition".to_owned(),
        });
    };
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|epoch| epoch.checked_add_signed(chrono::Duration::days(i64::from(*days))))
        .ok_or_else(|| ForgeError::Invariant {
            detail: "live manifest entry has an out-of-range day partition".to_owned(),
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

    /// Build a complete candidate so equality proves every manifest identity field.
    fn candidate(path: &str, day: NaiveDate, schema_id: i32) -> IcebergCandidateFile {
        IcebergCandidateFile {
            catalog_path: path.to_owned(),
            object_path: path.to_owned(),
            file_size_bytes: 10,
            record_count: 3,
            schema_id,
            partition_spec_id: 1,
            partition_day: day,
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
        let day = NaiveDate::from_ymd_opt(2026, 7, 29).expect("fixed day is valid");
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy is valid");
        let first = plan_candidates(
            &policy,
            vec![
                candidate("b.parquet", day, 1),
                candidate("a.parquet", day, 0),
            ],
            42,
            day,
        );
        let second = plan_candidates(
            &policy,
            vec![
                candidate("a.parquet", day, 0),
                candidate("b.parquet", day, 1),
            ],
            42,
            day,
        );

        assert_eq!(first, second);
        assert_eq!(first.groups[0].files[0].catalog_path, "a.parquet");
        assert_eq!(first.groups[0].files[0].source_snapshot_id, 42);
        assert!(matches!(
            first.groups[0].reason,
            super::super::right_size::IcebergRewriteReason::ObsoleteSchema
        ));
    }

    /// Proves open-day treatment is determined only by the supplied current day.
    #[test]
    fn open_partition_uses_supplied_current_day() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 29).expect("fixed day is valid");
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("policy is valid");
        let yesterday = today.pred_opt().expect("fixed day has predecessor");
        let tomorrow = today.succ_opt().expect("fixed day has successor");

        assert_eq!(
            plan_candidates(
                &policy,
                vec![candidate("a", yesterday, 1), candidate("b", yesterday, 1)],
                1,
                today
            )
            .groups
            .len(),
            1
        );
        assert!(
            plan_candidates(
                &policy,
                vec![candidate("a", today, 1), candidate("b", today, 1)],
                1,
                today
            )
            .groups
            .is_empty()
        );
        assert!(
            plan_candidates(
                &policy,
                vec![candidate("a", tomorrow, 1), candidate("b", tomorrow, 1)],
                1,
                today
            )
            .groups
            .is_empty()
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
        assert!(partition_day(&[]).is_err());
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
