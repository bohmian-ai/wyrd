//! Deterministic candidate grouping for Forge compaction.
//!
//! Forge groups staged files by tenant, table, and partition day before it
//! reads any object-store data. [`stable_pack`] keeps files in event-time order,
//! respects both byte and file-count limits, and drops singleton bins because
//! rewriting one file does not reduce file cardinality.

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;
use wyrd_spec::DataTenantId;

use crate::catalog::table_ref::{TableRef, is_safe_name};
use crate::namespaces::BifrostNamespace;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// The tenant, table, and event day that define one compaction group.
pub struct ForgeGroupKey {
    /// Tenant that owns the staged files.
    pub tenant: DataTenantId,
    /// Registered Bifrost table being compacted.
    pub table_ref: TableRef,
    /// Day partition shared by every file in the group.
    pub partition_day: NaiveDate,
}

impl ForgeGroupKey {
    /// Build a group key from the SQL representation of a Bifrost table.
    ///
    /// # Errors
    ///
    /// Returns an error when the namespace is unknown or the table name is
    /// unsafe for use in a server-owned table reference.
    pub fn from_sql(
        tenant: DataTenantId,
        namespace: &str,
        table_name: &str,
        partition_day: NaiveDate,
    ) -> Result<Self, String> {
        let namespace = BifrostNamespace::from_wire(namespace)
            .ok_or_else(|| format!("unknown Bifrost namespace `{namespace}`"))?;
        if !is_safe_name(table_name) {
            return Err(format!("unsafe table name `{table_name}`"));
        }
        Ok(Self {
            tenant,
            table_ref: TableRef::new(namespace, table_name),
            partition_day,
        })
    }

    /// Return the stable audit resource URI for this group.
    pub fn audit_resource(&self) -> String {
        format!(
            "bifrost://{}/{}/{}",
            self.tenant, self.table_ref.namespace, self.table_ref.name
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A staged Parquet file that is eligible for compaction.
pub struct CandidateFile {
    /// Stable row identity from `vala.file_list`.
    pub id: Uuid,
    /// Object-store path for the staged file.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Earliest event time recorded in the file.
    pub min_event_time: DateTime<Utc>,
    /// Latest event time recorded in the file.
    pub max_event_time: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A bounded set of staged files written as one compacted output.
pub struct RewriteBin {
    /// Files included in the rewrite, ordered by event-time bounds and ID.
    pub files: Vec<CandidateFile>,
    /// Sum of the input file sizes in bytes.
    pub total_bytes: u64,
}

/// Deterministic incremental compaction decision for one partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncrementalCompactionPlan {
    /// Complete bins eligible for one fenced rewrite operation each.
    pub(crate) rewrite_bins: Vec<RewriteBin>,
    /// Files intentionally retained for a later tick.
    pub(crate) retained_files: Vec<CandidateFile>,
}

/// Plan bounded incremental bins while preserving every input file.
pub(crate) fn plan_incremental_bins(
    mut files: Vec<CandidateFile>,
    target_bytes: u64,
    max_files: usize,
    partition_day: NaiveDate,
    current_utc_day: NaiveDate,
) -> IncrementalCompactionPlan {
    files.sort_by_key(|file| (file.min_event_time, file.max_event_time, file.id));
    if target_bytes == 0 || max_files == 0 {
        return IncrementalCompactionPlan {
            rewrite_bins: Vec::new(),
            retained_files: files,
        };
    }
    let closed = partition_day < current_utc_day;
    let mut rewrite_bins = Vec::new();
    let mut retained_files = Vec::new();
    let mut current = Vec::new();
    let mut bytes = 0_u64;
    for file in files {
        if file.size > target_bytes {
            retained_files.push(file);
            continue;
        }
        let boundary = !current.is_empty()
            && (current.len() >= max_files || bytes.saturating_add(file.size) > target_bytes);
        if boundary {
            if current.len() >= 2 {
                rewrite_bins.push(RewriteBin {
                    files: std::mem::take(&mut current),
                    total_bytes: bytes,
                });
            } else {
                retained_files.append(&mut current);
            }
            bytes = 0;
        }
        bytes = bytes.saturating_add(file.size);
        current.push(file);
    }
    if current.len() >= 2 && (closed || bytes >= target_bytes || current.len() >= max_files) {
        rewrite_bins.push(RewriteBin {
            files: current,
            total_bytes: bytes,
        });
    } else {
        retained_files.extend(current);
    }
    retained_files.sort_by_key(|file| (file.min_event_time, file.max_event_time, file.id));
    IncrementalCompactionPlan {
        rewrite_bins,
        retained_files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;

    fn file(id: u128, size: u64, min: i64, max: i64) -> CandidateFile {
        CandidateFile {
            id: Uuid::from_u128(id),
            path: format!("file-{id}"),
            size,
            min_event_time: DateTime::from_timestamp(min, 0).expect("timestamp"),
            max_event_time: DateTime::from_timestamp(max, 0).expect("timestamp"),
        }
    }

    #[test]
    fn group_key_is_exactly_tenant_table_day() {
        let tenant = DataTenantId::new_v7();
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        let key = ForgeGroupKey::from_sql(tenant, "vala.traces", "spans", day).expect("key");
        assert_eq!(key.tenant, tenant);
        assert_eq!(
            key.table_ref,
            TableRef::new(BifrostNamespace::Traces, "spans")
        );
        assert_eq!(key.partition_day, day);
    }

    #[test]
    fn group_key_rejects_tenant_namespace_table_or_day_mismatch() {
        let tenant = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        let key = ForgeGroupKey::from_sql(tenant, "vala.traces", "spans", day).expect("key");
        for observed in [
            (other, "vala.traces", "spans", day),
            (tenant, "vala.logs", "spans", day),
            (tenant, "vala.traces", "events", day),
            (
                tenant,
                "vala.traces",
                "spans",
                day.succ_opt().expect("next day"),
            ),
        ] {
            assert_ne!(
                ForgeGroupKey::from_sql(observed.0, observed.1, observed.2, observed.3),
                Ok(key.clone())
            );
        }
    }

    #[test]
    fn binpack_is_stable_and_never_exceeds_target() {
        let files = vec![file(3, 4, 2, 3), file(1, 4, 0, 1), file(2, 4, 1, 2)];
        let bins = plan_incremental_bins(
            files,
            8,
            10,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        )
        .rewrite_bins;
        assert_eq!(bins.len(), 1);
        assert_eq!(
            bins[0].files.iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(1), Uuid::from_u128(2)]
        );
        assert!(bins.iter().all(|bin| bin.total_bytes <= 8));
    }

    #[test]
    fn binpack_skips_singletons_and_preserves_interval_order() {
        let files = vec![file(1, 9, 0, 1), file(2, 4, 2, 3), file(3, 4, 4, 5)];
        let bins = plan_incremental_bins(
            files,
            8,
            2,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        )
        .rewrite_bins;
        assert_eq!(bins.len(), 1);
        assert_eq!(
            bins[0].files.iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(2), Uuid::from_u128(3)]
        );
    }

    /// An open partition retains a trailing bin that is below both planner
    /// thresholds so the next hint can add more files before rewriting it.
    #[test]
    fn active_day_retains_incomplete_trailing_bin() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        let plan =
            plan_incremental_bins(vec![file(1, 3, 0, 1), file(2, 3, 2, 3)], 10, 10, day, day);
        assert!(plan.rewrite_bins.is_empty());
        assert_eq!(plan.retained_files.len(), 2);
    }

    /// An open partition rewrites a trailing bin as soon as it reaches the
    /// byte target, even though the partition has not closed yet.
    #[test]
    fn active_day_emits_exact_target_trailing_bin() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        let plan =
            plan_incremental_bins(vec![file(1, 5, 0, 1), file(2, 5, 2, 3)], 10, 10, day, day);
        assert_eq!(plan.rewrite_bins.len(), 1);
    }

    /// A closed partition rewrites a multi-file trailing remainder even when
    /// it remains below the byte and file-count targets.
    #[test]
    fn closed_day_emits_multi_file_remainder() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).expect("day");
        let now = day.succ_opt().expect("next");
        let plan =
            plan_incremental_bins(vec![file(1, 3, 0, 1), file(2, 3, 2, 3)], 10, 10, day, now);
        assert_eq!(plan.rewrite_bins.len(), 1);
    }

    /// The planner retains oversized inputs and never emits a singleton bin,
    /// preserving every file for a future eligible compaction.
    #[test]
    fn planner_never_rewrites_singleton_or_drops_oversized_file() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        let plan =
            plan_incremental_bins(vec![file(1, 11, 0, 1), file(2, 3, 2, 3)], 10, 10, day, day);
        assert!(plan.rewrite_bins.is_empty());
        assert_eq!(plan.retained_files.len(), 2);
    }

    /// Sorting by event-time bounds and row identity makes planning independent
    /// of the order in which candidates were discovered.
    #[test]
    fn planner_is_stable_across_input_order() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 2).expect("day");
        let a = vec![file(2, 5, 2, 3), file(1, 5, 0, 1)];
        let b = vec![a[1].clone(), a[0].clone()];
        assert_eq!(
            plan_incremental_bins(a, 10, 10, day, day),
            plan_incremental_bins(b, 10, 10, day, day)
        );
    }
}
