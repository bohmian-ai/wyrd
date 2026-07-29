//! Deterministic right-sizing policy for live Iceberg data files.
//!
//! This module is pure: callers provide a stable live-file view and apply the
//! resulting groups through Forge's existing fenced commit workflow.

use chrono::{DateTime, NaiveDate, Utc};

use super::error::ForgeError;
use crate::parquet::writer_properties::BIFROST_WRITER_RECIPE_VERSION;

/// Iceberg's unsorted order identifier used when a data file omits the field.
pub const ICEBERG_UNSORTED_ORDER_ID: i64 = 0;

/// Current table identity and file-size bounds for a Forge operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeRightSizePolicy {
    /// Target read from `write.target-file-size-bytes`.
    target_file_size_bytes: u64,
    /// Inclusive lower healthy bound.
    minimum_file_size_bytes: u64,
    /// Inclusive upper healthy bound.
    maximum_file_size_bytes: u64,
    /// Current Iceberg schema identity.
    schema_id: i32,
    /// Current Iceberg partition-spec identity.
    partition_spec_id: i32,
    /// Current Iceberg sort identity.
    sort_order_id: i64,
}

impl ForgeRightSizePolicy {
    /// Build one policy from the metadata captured for a rewrite operation.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the target cannot produce
    /// non-zero checked tolerance bounds.
    pub fn new(
        target_file_size_bytes: u64,
        schema_id: i32,
        partition_spec_id: i32,
        sort_order_id: i64,
    ) -> Result<Self, ForgeError> {
        let minimum_file_size_bytes = target_file_size_bytes
            .checked_mul(75)
            .map(|value| value / 100)
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "write.target-file-size-bytes overflows Forge tolerance bounds".to_owned(),
            })?;
        let maximum_file_size_bytes = target_file_size_bytes
            .checked_mul(180)
            .map(|value| value / 100)
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "write.target-file-size-bytes overflows Forge tolerance bounds".to_owned(),
            })?;
        if target_file_size_bytes == 0
            || minimum_file_size_bytes == 0
            || maximum_file_size_bytes == 0
        {
            return Err(ForgeError::InvalidConfig {
                detail: "write.target-file-size-bytes must produce non-zero Forge tolerance bounds"
                    .to_owned(),
            });
        }
        Ok(Self {
            target_file_size_bytes,
            minimum_file_size_bytes,
            maximum_file_size_bytes,
            schema_id,
            partition_spec_id,
            sort_order_id,
        })
    }

    /// Return the target shared by group planning and output rotation.
    #[must_use]
    pub const fn target_file_size_bytes(&self) -> u64 {
        self.target_file_size_bytes
    }

    /// Produce ordered worthwhile rewrite groups without catalog or manifest IO.
    #[must_use]
    pub fn plan(&self, mut files: Vec<IcebergCandidateFile>) -> IcebergRewritePlan {
        files.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        let mut groups = Vec::new();
        let mut pending = Vec::<IcebergCandidateFile>::new();
        let mut pending_bytes = 0_u64;
        for file in files {
            if !pending.is_empty()
                && (pending[0].partition_spec_id != file.partition_spec_id
                    || pending[0].partition_day != file.partition_day)
            {
                Self::finish_undersized(&mut pending, &mut pending_bytes, &mut groups);
            }
            if file.file_size_bytes > self.maximum_file_size_bytes {
                Self::finish_undersized(&mut pending, &mut pending_bytes, &mut groups);
                groups.push(IcebergRewriteGroup::singleton(
                    file,
                    IcebergRewriteReason::Oversized,
                ));
            } else if let Some(reason) = self.identity_reason(&file) {
                Self::finish_undersized(&mut pending, &mut pending_bytes, &mut groups);
                groups.push(IcebergRewriteGroup::singleton(file, reason));
            } else if file.file_size_bytes < self.minimum_file_size_bytes {
                if !pending.is_empty()
                    && pending_bytes.saturating_add(file.file_size_bytes)
                        > self.target_file_size_bytes
                    && pending.len() >= 2
                {
                    Self::finish_undersized(&mut pending, &mut pending_bytes, &mut groups);
                }
                pending_bytes = pending_bytes.saturating_add(file.file_size_bytes);
                pending.push(file);
            }
        }
        Self::finish_undersized(&mut pending, &mut pending_bytes, &mut groups);
        let convergence = if groups.is_empty() {
            IcebergConvergence::Converged
        } else {
            IcebergConvergence::Pending
        };
        IcebergRewritePlan {
            groups,
            convergence,
        }
    }

    /// Emit a merge only when it reduces multiple undersized files.
    fn finish_undersized(
        pending: &mut Vec<IcebergCandidateFile>,
        pending_bytes: &mut u64,
        groups: &mut Vec<IcebergRewriteGroup>,
    ) {
        if pending.len() >= 2 {
            groups.push(IcebergRewriteGroup {
                files: std::mem::take(pending),
                reason: IcebergRewriteReason::Undersized,
            });
        } else {
            pending.clear();
        }
        *pending_bytes = 0;
    }

    /// Return the reason a file must be rewritten to the current identity.
    fn identity_reason(&self, file: &IcebergCandidateFile) -> Option<IcebergRewriteReason> {
        if file.schema_id != self.schema_id {
            return Some(IcebergRewriteReason::ObsoleteSchema);
        }
        if file.partition_spec_id != self.partition_spec_id {
            return Some(IcebergRewriteReason::ObsoletePartitionSpec);
        }
        if file.sort_order_id.unwrap_or(ICEBERG_UNSORTED_ORDER_ID) != self.sort_order_id {
            return Some(IcebergRewriteReason::ObsoleteSortOrder);
        }
        (file.writer_recipe_version.as_deref() != Some(BIFROST_WRITER_RECIPE_VERSION))
            .then_some(IcebergRewriteReason::ObsoleteWriterRecipe)
    }
}

/// Immutable data-file facts consumed by the right-size planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcebergCandidateFile {
    /// Stable catalog path used as the final ordering tie-breaker.
    pub(crate) catalog_path: String,
    /// Compressed on-disk file size.
    pub(crate) file_size_bytes: u64,
    /// Schema identity attached to the file.
    pub(crate) schema_id: i32,
    /// Partition specification that produced the file.
    pub(crate) partition_spec_id: i32,
    /// Day partition; files never cross it.
    pub(crate) partition_day: NaiveDate,
    /// Optional Iceberg sort-order identity.
    pub(crate) sort_order_id: Option<i64>,
    /// Physical writer-recipe marker, absent when unknown.
    pub(crate) writer_recipe_version: Option<String>,
    /// Earliest event timestamp in the file.
    pub(crate) min_event_time: DateTime<Utc>,
    /// Latest event timestamp in the file.
    pub(crate) max_event_time: DateTime<Utc>,
}

impl IcebergCandidateFile {
    /// Return the canonical planner order for this file.
    fn sort_key(&self) -> (i32, NaiveDate, DateTime<Utc>, DateTime<Utc>, &str) {
        (
            self.partition_spec_id,
            self.partition_day,
            self.min_event_time,
            self.max_event_time,
            &self.catalog_path,
        )
    }
}

/// Why a rewrite group changes live Iceberg files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcebergRewriteReason {
    /// Multiple current undersized files can be combined.
    Undersized,
    /// One file must be split because it exceeds the upper bound.
    Oversized,
    /// The file has an obsolete schema.
    ObsoleteSchema,
    /// The file has an obsolete partition specification.
    ObsoletePartitionSpec,
    /// The file has an obsolete sort order.
    ObsoleteSortOrder,
    /// The file has an unknown or obsolete physical writer recipe.
    ObsoleteWriterRecipe,
}

/// One ordered, same-spec/day rewrite operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcebergRewriteGroup {
    /// Files consumed by the operation.
    pub(crate) files: Vec<IcebergCandidateFile>,
    /// Condition making the operation useful.
    pub(crate) reason: IcebergRewriteReason,
}

impl IcebergRewriteGroup {
    /// Build a forced one-file rewrite group.
    fn singleton(file: IcebergCandidateFile, reason: IcebergRewriteReason) -> Self {
        Self {
            files: vec![file],
            reason,
        }
    }
}

/// The planner's full deterministic decision for one stable table view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcebergRewritePlan {
    /// Ordered useful rewrite operations.
    pub(crate) groups: Vec<IcebergRewriteGroup>,
    /// Whether the table has no remaining useful operation.
    pub(crate) convergence: IcebergConvergence,
}

/// Convergence proof for the supplied table view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcebergConvergence {
    /// All files are healthy/current except an accepted one-file tail.
    Converged,
    /// At least one useful group remains.
    Pending,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one current-identity file with a stable ordering key.
    fn file(path: &str, bytes: u64, day: NaiveDate) -> IcebergCandidateFile {
        IcebergCandidateFile {
            catalog_path: path.to_owned(),
            file_size_bytes: bytes,
            schema_id: 1,
            partition_spec_id: 1,
            partition_day: day,
            sort_order_id: Some(1),
            writer_recipe_version: Some(BIFROST_WRITER_RECIPE_VERSION.to_owned()),
            min_event_time: DateTime::from_timestamp(1, 0).expect("fixed timestamp is valid"),
            max_event_time: DateTime::from_timestamp(2, 0).expect("fixed timestamp is valid"),
        }
    }

    /// Uses the Iceberg target to derive the locked seventy-five/one-eighty band.
    #[test]
    fn right_size_band_is_seventy_five_to_one_eighty_percent() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        assert_eq!(policy.target_file_size_bytes(), 100);
        assert_eq!(
            policy.plan(vec![file("healthy", 75, day)]).convergence,
            IcebergConvergence::Converged
        );
        assert_eq!(
            policy.plan(vec![file("large", 181, day)]).groups[0].reason,
            IcebergRewriteReason::Oversized
        );
    }

    /// The Iceberg metadata parser supplies its 512 MiB default when a table
    /// omits `write.target-file-size-bytes`; Forge preserves that authority.
    #[test]
    fn right_size_uses_table_property_and_iceberg_default() {
        let default_target = 512 * 1024 * 1024;
        let default_policy = ForgeRightSizePolicy::new(default_target, 1, 1, 1)
            .expect("Iceberg default target is valid");
        let override_policy =
            ForgeRightSizePolicy::new(1024, 1, 1, 1).expect("table property target is valid");
        assert_eq!(default_policy.target_file_size_bytes(), default_target);
        assert_eq!(override_policy.target_file_size_bytes(), 1024);
    }

    /// Combines compatible undersized files while accepting a one-file tail.
    #[test]
    fn planner_combines_compatible_undersized_files() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        let plan = policy.plan(vec![
            file("a", 20, day),
            file("b", 20, day),
            file("tail", 20, day),
        ]);
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].reason, IcebergRewriteReason::Undersized);
        assert_eq!(plan.groups[0].files.len(), 3);
    }

    /// One current undersized file is an accepted trailing tail, not a 1:1 rewrite.
    #[test]
    fn planner_accepts_one_current_undersized_tail() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        let plan = policy.plan(vec![file("tail", 20, day)]);
        assert!(plan.groups.is_empty());
        assert_eq!(plan.convergence, IcebergConvergence::Converged);
    }

    /// A file above the upper tolerance is rewritten by itself so output rotation can split it.
    #[test]
    fn planner_splits_oversized_singleton() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        let plan = policy.plan(vec![file("large", 181, day)]);
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].reason, IcebergRewriteReason::Oversized);
        assert_eq!(plan.groups[0].files.len(), 1);
    }

    /// Forces identity repair even when a file's size is healthy.
    #[test]
    fn planner_forces_obsolete_schema_spec_sort_and_recipe() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        let mut obsolete_schema = file("schema", 100, day);
        obsolete_schema.catalog_path = "1-schema".to_owned();
        obsolete_schema.schema_id = 2;
        let mut obsolete_spec = file("spec", 100, day);
        obsolete_spec.catalog_path = "2-spec".to_owned();
        obsolete_spec.partition_spec_id = 2;
        let mut obsolete_sort = file("sort", 100, day);
        obsolete_sort.catalog_path = "3-sort".to_owned();
        obsolete_sort.sort_order_id = None;
        let mut obsolete_recipe = file("recipe", 100, day);
        obsolete_recipe.catalog_path = "4-recipe".to_owned();
        obsolete_recipe.writer_recipe_version = None;
        let plan = policy.plan(vec![
            obsolete_schema,
            obsolete_spec,
            obsolete_sort,
            obsolete_recipe,
        ]);
        assert_eq!(
            plan.groups
                .iter()
                .map(|group| group.reason)
                .collect::<Vec<_>>(),
            vec![
                IcebergRewriteReason::ObsoleteSchema,
                IcebergRewriteReason::ObsoleteSortOrder,
                IcebergRewriteReason::ObsoleteWriterRecipe,
                IcebergRewriteReason::ObsoletePartitionSpec,
            ]
        );
    }

    /// Healthy current files are never selected merely to fill a rewrite bin.
    #[test]
    fn planner_excludes_healthy_files() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        let plan = policy.plan(vec![file("healthy", 100, day), file("tail", 20, day)]);
        assert!(plan.groups.is_empty());
        assert_eq!(plan.convergence, IcebergConvergence::Converged);
    }

    /// Keeps different partition days in separate rewrite groups.
    #[test]
    fn planner_never_crosses_spec_or_day() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let first = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        let second = NaiveDate::from_ymd_opt(2026, 1, 2).expect("fixed day is valid");
        assert!(
            policy
                .plan(vec![file("a", 20, first), file("b", 20, second)])
                .groups
                .is_empty()
        );
    }

    /// Sorting the same live set in a different input order cannot change its plan.
    #[test]
    fn planner_is_stable_across_input_order() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        let first = vec![file("c", 20, day), file("a", 20, day), file("b", 20, day)];
        let second = vec![file("b", 20, day), file("c", 20, day), file("a", 20, day)];
        assert_eq!(policy.plan(first), policy.plan(second));
    }

    /// A healthy file set has no useful operation and is therefore converged.
    #[test]
    fn planner_no_useful_group_is_converged() {
        let policy = ForgeRightSizePolicy::new(100, 1, 1, 1).expect("valid target");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("fixed day is valid");
        let plan = policy.plan(vec![file("healthy", 100, day)]);
        assert!(plan.groups.is_empty());
        assert_eq!(plan.convergence, IcebergConvergence::Converged);
    }
}
