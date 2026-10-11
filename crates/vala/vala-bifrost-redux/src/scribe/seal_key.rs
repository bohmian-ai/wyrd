//! Seal key — `(DataTenantId, TableRef, TimePartition)` routing for WAL and
//! memtable.
//!
//! Every WAL segment, memtable slot, manifest entry, and Parquet file is keyed by
//! seal-key. A `ScribeIngressFrame` that spans multiple partitions is split into
//! per-key slices before WAL writes; a seal-key never crosses a partition.
//!
//! The time axis — the time column and its partition granularity — is not a
//! property of this module: it comes from the table's canonical
//! `PhysicalLayout` and is passed in by the admission path, so every table uses
//! exactly the same code with its own column and bucket boundaries.
//!
//! `TableRef` lives in `crate::catalog::table_ref` and is re-exported through
//! `crate::catalog` — this module only owns the seal-key composition.

use arrow::array::UInt32Array;
use arrow::compute::take;
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::catalog::TenantTableBinding;
pub use crate::catalog::layout::TimePartition;
use crate::catalog::layout::{TimeAxis, TimeColumnValues, TimeGranularity};
use crate::contracts::ScribeError;

/// Durable identity shared by every object one assembly claim publishes.
///
/// A claim spans several shard lanes, so it
/// carries no shard: the claim digest is what distinguishes one published set
/// from another. That digest is derived from the claim's exact members, so the
/// base is stable across a retry of the same publication and an interrupted
/// upload converges on the identical object instead of leaving a second copy
/// of the same rows behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScribeClaimIdentity {
    /// Tenant-qualified object prefix for the logical table.
    object_prefix: String,
    /// Exact partition every member of the claim represents.
    partition: TimePartition,
    /// Producing Scribe node without UUID punctuation.
    node_id: String,
    /// Producing Scribe writer epoch.
    writer_epoch: i64,
    /// Claim digest distinguishing this member set from any other.
    claim: String,
    /// Inclusive minimum WAL LSN the claim's members cover.
    wal_lsn_min: u64,
    /// Inclusive maximum WAL LSN the claim's members cover.
    wal_lsn_max: u64,
}

impl ScribeClaimIdentity {
    /// Constructs the durable claim identity after validating its node.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when `node_id` is not a UUID or when the WAL
    /// range the claim's members union to is reversed.
    pub(crate) fn new(
        binding: &TenantTableBinding,
        partition: TimePartition,
        node_id: &str,
        writer_epoch: i64,
        claim: &crate::scribe::assembly::StagingClaimId,
        wal_lsn_min: u64,
        wal_lsn_max: u64,
    ) -> Result<Self, ScribeError> {
        let node_id = uuid::Uuid::parse_str(node_id).map_err(|error| ScribeError::Internal {
            detail: format!("node_id is not a valid UUID: {error}"),
        })?;
        if wal_lsn_min > wal_lsn_max {
            return Err(ScribeError::Internal {
                detail: "Scribe claim WAL range is reversed".to_owned(),
            });
        }
        Ok(Self {
            object_prefix: binding.object_prefix.clone(),
            partition,
            node_id: node_id.simple().to_string(),
            writer_epoch,
            claim: claim.to_string(),
            wal_lsn_min,
            wal_lsn_max,
        })
    }

    /// Returns the deterministic base shared by every object of the claim.
    #[must_use]
    pub(crate) fn object_base(&self) -> String {
        format!(
            "{}/{}/scribe-{}-epoch-{}-claim-{}-wal-{}-{}",
            self.object_prefix,
            self.partition.as_path_components(),
            self.node_id,
            self.writer_epoch,
            self.claim,
            self.wal_lsn_min,
            self.wal_lsn_max,
        )
    }
}

/// Seal key — `(DataTenantId, TableRef, TimePartition)` identifying one
/// WAL/memtable/seal scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SealKey {
    /// Tenant ID for this seal scope.
    pub tenant: DataTenantId,
    /// Table reference for this seal scope.
    pub table: TableRef,
    /// Exact partition for this seal scope.
    pub partition: TimePartition,
}

impl SealKey {
    /// Construct a new seal key.
    #[must_use]
    pub fn new(tenant: DataTenantId, table: TableRef, partition: TimePartition) -> Self {
        Self {
            tenant,
            table,
            partition,
        }
    }

    /// Format as a directory path component:
    /// `{namespace}/{table}/tenant={tenant}/partition_granularity={g}/partition_start={s}`
    #[must_use]
    pub fn as_path_components(&self) -> String {
        format!(
            "{}/{}/tenant={}/{}",
            self.table.namespace.as_str(),
            self.table.name,
            self.tenant.as_uuid(),
            self.partition.as_path_components()
        )
    }
}

impl std::fmt::Display for SealKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_path_components())
    }
}

/// Maximum distinct time partitions accepted from one ingest source.
///
/// The bound is on *partitions*, not days: an hourly table admits at most 32
/// distinct hours from one source, and a daily table at most 32 distinct days.
/// This is the single accounting path — Gate configuration, the fixed material
/// plan, and telemetry all report this same unit.
pub(crate) const MAX_TIME_PARTITIONS: usize = crate::gate::limits::OTLP_WIRE_LIMITS.time_partitions;

/// Immutable fixed-capacity partition count plan for one current source.
///
/// The plan is built by one non-retaining counting pass so the producer can
/// materialize partitions lazily, one at a time, without holding a per-partition
/// row collection.
#[derive(Debug, Clone)]
pub(crate) struct TimePartitionPlan {
    /// Sorted partition descriptors.
    partitions: [Option<TimePartition>; MAX_TIME_PARTITIONS],
    /// Exact rows assigned to each descriptor.
    counts: [usize; MAX_TIME_PARTITIONS],
    /// Live descriptor count.
    partition_count: usize,
    /// Time axis the counting pass bucketed by, reused by materialization.
    axis: TimeAxis,
}

impl TimePartitionPlan {
    /// Returns the exact number of distinct planned partitions.
    #[must_use]
    pub(crate) const fn len(&self) -> usize {
        self.partition_count
    }

    /// Materializes one planned partition from its retained source.
    ///
    /// The single-partition case aliases the source's Arrow buffers instead of
    /// copying; every other case takes exactly the rows the counting pass
    /// assigned and fails if the two passes disagree.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when `index` is outside the plan, the source no
    /// longer matches the count pass, or Arrow cannot take the planned rows.
    pub(crate) fn materialize(
        &self,
        batch: &RecordBatch,
        index: usize,
    ) -> Result<(TimePartition, RecordBatch), ScribeError> {
        let partition = self
            .partitions
            .get(index)
            .copied()
            .flatten()
            .ok_or(ScribeError::InvalidFrame)?;
        let expected_rows = self.counts[index];
        if self.partition_count == 1 {
            return Ok((partition, batch.clone()));
        }
        let values = time_values(batch, &self.axis)?;
        let mut indices = Vec::with_capacity(expected_rows);
        for row in 0..values.len() {
            if bucket(partition.granularity(), values, row)? == partition {
                indices.push(u32::try_from(row).map_err(|_| ScribeError::Internal {
                    detail: "record batch row index exceeds u32::MAX".to_owned(),
                })?);
            }
        }
        if indices.len() != expected_rows || indices.capacity() != expected_rows {
            return Err(ScribeError::Internal {
                detail: "time-partition materialization diverged from its count pass".to_owned(),
            });
        }
        let indices = UInt32Array::from(indices);
        let columns = batch
            .columns()
            .iter()
            .map(|column| {
                take(column.as_ref(), &indices, None).map_err(|error| ScribeError::Internal {
                    detail: format!("Arrow partition split failed: {error}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sliced = RecordBatch::try_new(batch.schema(), columns).map_err(|error| {
            ScribeError::Internal {
                detail: format!("Arrow partition slice construction failed: {error}"),
            }
        })?;
        Ok((partition, sliced))
    }
}

/// Current-only iterator over deterministic time-partition slices.
pub(crate) struct TimePartitionSlices<'a> {
    /// Retained source whose buffers remain alive for single-partition aliasing.
    batch: &'a RecordBatch,
    /// Immutable descriptors shared with lazy native production.
    plan: TimePartitionPlan,
    /// Descriptor produced by the next iterator call.
    current: usize,
}

impl Iterator for TimePartitionSlices<'_> {
    type Item = Result<(TimePartition, RecordBatch), ScribeError>;

    /// Materializes only the next partition and drops its exact index workspace
    /// on the following call.
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.plan.len() {
            return None;
        }
        let result = self.plan.materialize(self.batch, self.current);
        self.current += 1;
        Some(result)
    }

    /// Reports the exact unmaterialized partition count.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.plan.len().saturating_sub(self.current);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for TimePartitionSlices<'_> {}

/// Plans deterministic time-partition slices without per-partition row
/// collections.
///
/// # Errors
///
/// Returns [`ScribeError`] for a missing, null, mistyped, or invalid event-time
/// column, a row index beyond `u32`, or more than
/// [`MAX_TIME_PARTITIONS`] distinct partitions.
pub(crate) fn split_batch_by_time_partition<'a>(
    batch: &'a RecordBatch,
    axis: &TimeAxis,
) -> Result<TimePartitionSlices<'a>, ScribeError> {
    Ok(TimePartitionSlices {
        batch,
        plan: plan_time_partitions(batch, axis)?,
        current: 0,
    })
}

/// Borrows the required time column of one admitted batch.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the column is absent or is not a
/// microsecond or nanosecond timestamp, which a stamped batch of a registered
/// table never is.
fn time_values<'a>(
    batch: &'a RecordBatch,
    axis: &TimeAxis,
) -> Result<TimeColumnValues<'a>, ScribeError> {
    axis.values(batch).ok_or_else(|| ScribeError::Internal {
        detail: format!(
            "time column {} is missing or not a microsecond or nanosecond timestamp",
            axis.column()
        ),
    })
}

/// Buckets row `index` of the time column into its exact partition.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for a null instant and for an instant
/// whose partition is not representable — its boundaries outside chrono's
/// range or its Iceberg transform value outside `i32` — so such a request is
/// refused whole rather than written.
fn bucket(
    granularity: TimeGranularity,
    values: TimeColumnValues<'_>,
    index: usize,
) -> Result<TimePartition, ScribeError> {
    let micros = values
        .floor_micros(index)
        .ok_or(ScribeError::InvalidFrame)?;
    let instant =
        DateTime::<Utc>::from_timestamp_micros(micros).ok_or(ScribeError::InvalidFrame)?;
    granularity
        .bucket(instant)
        .map_err(|_| ScribeError::InvalidFrame)
}

/// Counts one current source into an owned fixed-capacity partition plan.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] for a missing or mistyped time column and
/// checked row-count overflow, and [`ScribeError::InvalidFrame`] for a null or
/// unrepresentable instant or more than [`MAX_TIME_PARTITIONS`] distinct
/// partitions, so nothing of the request is written.
pub(crate) fn plan_time_partitions(
    batch: &RecordBatch,
    axis: &TimeAxis,
) -> Result<TimePartitionPlan, ScribeError> {
    let values = time_values(batch, axis)?;

    let mut partitions: [Option<TimePartition>; MAX_TIME_PARTITIONS] = [None; MAX_TIME_PARTITIONS];
    let mut counts = [0_usize; MAX_TIME_PARTITIONS];
    let mut partition_count = 0_usize;
    for index in 0..values.len() {
        let partition = bucket(axis.granularity(), values, index)?;
        let position = partitions[..partition_count]
            .binary_search_by(|candidate| {
                candidate.map_or(std::cmp::Ordering::Less, |value| value.cmp(&partition))
            })
            .unwrap_or_else(|position| position);
        if partitions.get(position).copied().flatten() == Some(partition) {
            counts[position] =
                counts[position]
                    .checked_add(1)
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "time-partition row count overflow".to_owned(),
                    })?;
            continue;
        }
        if partition_count == MAX_TIME_PARTITIONS {
            return Err(ScribeError::InvalidFrame);
        }
        for move_index in (position..partition_count).rev() {
            partitions[move_index + 1] = partitions[move_index];
            counts[move_index + 1] = counts[move_index];
        }
        partitions[position] = Some(partition);
        counts[position] = 1;
        partition_count += 1;
    }
    if partition_count == 0 {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(TimePartitionPlan {
        partitions,
        counts,
        partition_count,
        axis: axis.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;
    use arrow::array::{Int64Array, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;

    /// Builds one exact partition from an RFC 3339 boundary.
    fn partition(granularity: TimeGranularity, rfc3339: &str) -> TimePartition {
        let start = DateTime::parse_from_rfc3339(rfc3339)
            .expect("fixture timestamp parses")
            .with_timezone(&Utc);
        TimePartition::new(granularity, start).expect("fixture timestamp is a boundary")
    }

    /// Time column every fixture batch partitions on.
    const TIME_COLUMN: &str = "started_at";

    /// Builds the fixture time axis at `granularity`.
    fn axis(granularity: TimeGranularity) -> TimeAxis {
        TimeAxis::new(TIME_COLUMN, granularity)
    }

    /// Builds a two-column batch whose time-column values are the supplied micros.
    fn batch_with_event_times(micros: Vec<i64>) -> RecordBatch {
        let values: Vec<i64> = (0..micros.len().try_into().expect("row count fits i64")).collect();
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                TIME_COLUMN,
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(micros)),
                Arc::new(Int64Array::from(values)),
            ],
        )
        .expect("fixture batch builds")
    }

    /// Bucketing places an instant in the exact partition of its granularity.
    #[test]
    fn bucketing_is_exact_for_both_granularities() {
        let instant = DateTime::parse_from_rfc3339("2026-07-14T12:34:56.123456Z")
            .expect("fixture timestamp parses")
            .with_timezone(&Utc);
        assert_eq!(
            TimeGranularity::Hour.bucket(instant).expect("hour bucket"),
            partition(TimeGranularity::Hour, "2026-07-14T12:00:00Z")
        );
        assert_eq!(
            TimeGranularity::Day.bucket(instant).expect("day bucket"),
            partition(TimeGranularity::Day, "2026-07-14T00:00:00Z")
        );
    }

    /// The seal-key path carries the typed partition, not a bare day segment.
    #[test]
    fn seal_key_path_components() {
        let tenant = crate::test_support::tenant();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let seal_key = SealKey::new(
            tenant,
            table,
            partition(TimeGranularity::Hour, "2026-07-14T09:00:00Z"),
        );

        let path = seal_key.as_path_components();
        assert!(path.contains("vala.bifrost"));
        assert!(path.contains("events"));
        assert!(path.contains("partition_granularity=hour/partition_start=2026-07-14T09Z"));
        assert!(!path.contains("day="));
    }

    /// A single-partition source aliases its Arrow value buffers instead of
    /// copying them.
    #[test]
    fn one_partition_split_reuses_arrow_value_buffers() {
        let source = batch_with_event_times(vec![1_784_040_000_000_000, 1_784_040_060_000_000]);

        let slices = split_batch_by_time_partition(&source, &axis(TimeGranularity::Hour))
            .expect("split plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("split");
        assert_eq!(slices.len(), 1);
        assert!(Arc::ptr_eq(source.column(0), slices[0].1.column(0)));
        assert!(Arc::ptr_eq(source.column(1), slices[0].1.column(1)));
    }

    /// The same source splits into one day but two hours, proving granularity —
    /// not the data — decides the partition count.
    #[test]
    fn granularity_decides_the_partition_count() {
        let source = batch_with_event_times(vec![
            1_784_016_000_000_000,
            1_784_016_060_000_000,
            1_784_019_600_000_000,
            1_784_019_660_000_000,
        ]);

        let hourly = split_batch_by_time_partition(&source, &axis(TimeGranularity::Hour))
            .expect("hourly plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("hourly split");
        assert_eq!(hourly.len(), 2);
        assert_eq!(hourly[0].1.num_rows(), 2);
        assert_eq!(hourly[1].1.num_rows(), 2);
        assert_ne!(hourly[0].0, hourly[1].0);
        assert!(hourly[0].0 < hourly[1].0);

        let daily = split_batch_by_time_partition(&source, &axis(TimeGranularity::Day))
            .expect("daily plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("daily split");
        assert_eq!(daily.len(), 1);
        assert_eq!(daily[0].1.num_rows(), 4);
    }

    /// Exactly 32 distinct partitions are admitted from one source and the
    /// thirty-third refuses the whole request before any WAL work.
    #[test]
    fn thirty_three_partitions_refuse_the_request() {
        let hour_micros = 3_600_000_000_i64;
        let base = 1_784_016_000_000_000_i64;

        let admitted = batch_with_event_times(
            (0..MAX_TIME_PARTITIONS)
                .map(|index| base + hour_micros * i64::try_from(index).expect("index fits i64"))
                .collect(),
        );
        let plan = plan_time_partitions(&admitted, &axis(TimeGranularity::Hour))
            .expect("32 partitions admitted");
        assert_eq!(plan.len(), MAX_TIME_PARTITIONS);

        let rejected = batch_with_event_times(
            (0..=MAX_TIME_PARTITIONS)
                .map(|index| base + hour_micros * i64::try_from(index).expect("index fits i64"))
                .collect(),
        );
        assert!(matches!(
            plan_time_partitions(&rejected, &axis(TimeGranularity::Hour)),
            Err(ScribeError::InvalidFrame)
        ));
    }

    /// Any representable instant is accepted at either precision, years from
    /// now in both directions, while an instant whose partition cannot be
    /// represented is refused as an invalid frame instead of panicking.
    #[test]
    fn any_representable_instant_is_bucketed_and_extremes_are_refused() {
        let year_micros = 365 * 86_400 * 1_000_000_i64;
        let now = 1_784_016_000_000_000_i64;
        let far = batch_with_event_times(vec![now - 40 * year_micros, now + 40 * year_micros]);
        let plan = plan_time_partitions(&far, &axis(TimeGranularity::Hour))
            .expect("instants decades away are accepted");
        assert_eq!(plan.len(), 2);

        let nanos = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                TIME_COLUMN,
                DataType::Timestamp(TimeUnit::Nanosecond, Some("+00:00".into())),
                false,
            )])),
            vec![Arc::new(
                arrow::array::TimestampNanosecondArray::from(vec![-1_i64, 0])
                    .with_timezone("+00:00"),
            )],
        )
        .expect("nanosecond batch builds");
        let plan = plan_time_partitions(&nanos, &axis(TimeGranularity::Hour))
            .expect("nanosecond instants are accepted");
        assert_eq!(
            plan.len(),
            2,
            "one nanosecond before the epoch is the prior hour"
        );

        for extreme in [i64::MIN, i64::MAX] {
            assert!(matches!(
                plan_time_partitions(
                    &batch_with_event_times(vec![extreme]),
                    &axis(TimeGranularity::Hour)
                ),
                Err(ScribeError::InvalidFrame)
            ));
        }
    }
}
