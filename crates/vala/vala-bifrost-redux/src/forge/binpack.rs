//! Deterministic candidate grouping for Forge compaction.
//!
//! Forge groups staged files by tenant, table, and partition day before it
//! reads any object-store data. [`stable_pack`] keeps files in event-time order,
//! respects both byte and file-count limits, and drops singleton bins because
//! rewriting one file does not reduce file cardinality.

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wyrd_spec::DataTenantId;

use crate::catalog::layout::TimePartition;
use crate::catalog::table_ref::TableRef;
#[cfg(test)]
use crate::catalog::table_ref::is_safe_name;
#[cfg(test)]
use crate::namespaces::BifrostNamespace;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// The tenant, table, and exact time partition that define one compaction group.
pub struct ForgeGroupKey {
    /// Tenant that owns the staged files.
    pub tenant: DataTenantId,
    /// Registered Bifrost table being compacted.
    pub table_ref: TableRef,
    /// Exact time partition shared by every file in the group.
    pub partition: TimePartition,
}

impl ForgeGroupKey {
    /// Builds a group key from the SQL representation used by parser regressions.
    ///
    /// # Errors
    ///
    /// Returns an error when the namespace is unknown or the table name is
    /// unsafe for a server-owned table reference.
    #[cfg(test)]
    fn from_sql(
        tenant: DataTenantId,
        namespace: &str,
        table_name: &str,
        partition: TimePartition,
    ) -> Result<Self, String> {
        let namespace = BifrostNamespace::from_wire(namespace)
            .ok_or_else(|| format!("unknown Bifrost namespace `{namespace}`"))?;
        if !is_safe_name(table_name) {
            return Err(format!("unsafe table name `{table_name}`"));
        }
        Ok(Self {
            tenant,
            table_ref: TableRef::new(namespace, table_name),
            partition,
        })
    }

    /// Return the stable audit resource URI for one tenant/table pair.
    ///
    /// The resource identifies the table, never the partition, so table-scoped
    /// callers such as live reconciliation can address the same audit stream
    /// without inventing a placeholder partition.
    #[must_use]
    pub fn table_audit_resource(tenant: DataTenantId, table_ref: &TableRef) -> String {
        format!(
            "bifrost://{}/{}/{}",
            tenant, table_ref.namespace, table_ref.name
        )
    }

    /// Return the stable audit resource URI for this group.
    #[must_use]
    pub fn audit_resource(&self) -> String {
        Self::table_audit_resource(self.tenant, &self.table_ref)
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
#[cfg(test)]
pub(crate) struct IncrementalCompactionPlan {
    /// Complete bins eligible for one fenced rewrite operation each.
    pub(crate) rewrite_bins: Vec<RewriteBin>,
    /// Files intentionally retained for a later tick.
    pub(crate) retained_files: Vec<CandidateFile>,
}

/// Plan bounded incremental bins while preserving every input file.
#[cfg(test)]
pub(crate) fn plan_incremental_bins(
    mut files: Vec<CandidateFile>,
    target_bytes: u64,
    max_files: usize,
    partition: TimePartition,
    now: DateTime<Utc>,
) -> IncrementalCompactionPlan {
    files.sort_by_key(|file| (file.min_event_time, file.max_event_time, file.id));
    if target_bytes == 0 || max_files == 0 {
        return IncrementalCompactionPlan {
            rewrite_bins: Vec::new(),
            retained_files: files,
        };
    }
    let closed = !partition.is_open_at(now);
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
    use crate::catalog::TimeGranularity;
    use crate::namespaces::BifrostNamespace;

    /// Builds the fixture hour partition every planner regression shares.
    fn hour(epoch_hour: i64) -> TimePartition {
        TimePartition::new(
            TimeGranularity::Hour,
            DateTime::from_timestamp(epoch_hour * 3_600, 0).expect("fixture hour is representable"),
        )
        .expect("fixture hour is an exact hour boundary")
    }

    /// Returns an instant inside `partition`, so the partition reads as open.
    fn inside(partition: TimePartition) -> DateTime<Utc> {
        partition.start_utc()
    }

    /// Returns an instant after `partition`, so the partition reads as closed.
    fn after(partition: TimePartition) -> DateTime<Utc> {
        partition.end_utc()
    }

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
    fn group_key_is_exactly_tenant_table_partition() {
        let tenant = DataTenantId::new_v7();
        let partition = hour(400_000);
        let key = ForgeGroupKey::from_sql(tenant, "vala.traces", "spans", partition).expect("key");
        assert_eq!(key.tenant, tenant);
        assert_eq!(
            key.table_ref,
            TableRef::new(BifrostNamespace::Traces, "spans")
        );
        assert_eq!(key.partition, partition);
    }

    #[test]
    fn group_key_rejects_tenant_namespace_table_or_partition_mismatch() {
        let tenant = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        let partition = hour(400_000);
        let key = ForgeGroupKey::from_sql(tenant, "vala.traces", "spans", partition).expect("key");
        for observed in [
            (other, "vala.traces", "spans", partition),
            (tenant, "vala.logs", "spans", partition),
            (tenant, "vala.traces", "events", partition),
            (tenant, "vala.traces", "spans", hour(400_001)),
        ] {
            assert_ne!(
                ForgeGroupKey::from_sql(observed.0, observed.1, observed.2, observed.3),
                Ok(key.clone())
            );
        }
    }

    /// A same-granularity partition at a different instant is a distinct group,
    /// which is what keeps hourly compaction from merging across hours.
    #[test]
    fn group_key_separates_adjacent_hours() {
        let tenant = DataTenantId::new_v7();
        let first = ForgeGroupKey::from_sql(tenant, "vala.traces", "spans", hour(400_000));
        let second = ForgeGroupKey::from_sql(tenant, "vala.traces", "spans", hour(400_001));
        assert_ne!(first, second);
    }

    #[test]
    fn binpack_is_stable_and_never_exceeds_target() {
        let files = vec![file(3, 4, 2, 3), file(1, 4, 0, 1), file(2, 4, 1, 2)];
        let partition = hour(400_000);
        let bins = plan_incremental_bins(files, 8, 10, partition, after(partition)).rewrite_bins;
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
        let partition = hour(400_000);
        let bins = plan_incremental_bins(files, 8, 2, partition, after(partition)).rewrite_bins;
        assert_eq!(bins.len(), 1);
        assert_eq!(
            bins[0].files.iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(2), Uuid::from_u128(3)]
        );
    }

    /// An open partition retains a trailing bin that is below both planner
    /// thresholds so the next hint can add more files before rewriting it.
    #[test]
    fn open_partition_retains_incomplete_trailing_bin() {
        let partition = hour(400_000);
        let plan = plan_incremental_bins(
            vec![file(1, 3, 0, 1), file(2, 3, 2, 3)],
            10,
            10,
            partition,
            inside(partition),
        );
        assert!(plan.rewrite_bins.is_empty());
        assert_eq!(plan.retained_files.len(), 2);
    }

    /// An open partition rewrites a trailing bin as soon as it reaches the
    /// byte target, even though the partition has not closed yet.
    #[test]
    fn open_partition_emits_exact_target_trailing_bin() {
        let partition = hour(400_000);
        let plan = plan_incremental_bins(
            vec![file(1, 5, 0, 1), file(2, 5, 2, 3)],
            10,
            10,
            partition,
            inside(partition),
        );
        assert_eq!(plan.rewrite_bins.len(), 1);
    }

    /// A closed partition rewrites a multi-file trailing remainder even when
    /// it remains below the byte and file-count targets.
    #[test]
    fn closed_partition_emits_multi_file_remainder() {
        let partition = hour(400_000);
        let plan = plan_incremental_bins(
            vec![file(1, 3, 0, 1), file(2, 3, 2, 3)],
            10,
            10,
            partition,
            after(partition),
        );
        assert_eq!(plan.rewrite_bins.len(), 1);
    }

    /// The planner retains oversized inputs and never emits a singleton bin,
    /// preserving every file for a future eligible compaction.
    #[test]
    fn planner_never_rewrites_singleton_or_drops_oversized_file() {
        let partition = hour(400_000);
        let plan = plan_incremental_bins(
            vec![file(1, 11, 0, 1), file(2, 3, 2, 3)],
            10,
            10,
            partition,
            inside(partition),
        );
        assert!(plan.rewrite_bins.is_empty());
        assert_eq!(plan.retained_files.len(), 2);
    }

    /// Sorting by event-time bounds and row identity makes planning independent
    /// of the order in which candidates were discovered.
    #[test]
    fn planner_is_stable_across_input_order() {
        let partition = hour(400_000);
        let a = vec![file(2, 5, 2, 3), file(1, 5, 0, 1)];
        let b = vec![a[1].clone(), a[0].clone()];
        assert_eq!(
            plan_incremental_bins(a, 10, 10, partition, inside(partition)),
            plan_incremental_bins(b, 10, 10, partition, inside(partition))
        );
    }
}
