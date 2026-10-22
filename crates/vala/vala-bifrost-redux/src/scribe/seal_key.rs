//! Seal key — `(DataTenantId, TableRef, PhysicalPartition)` routing for WAL
//! and memtable.
//!
//! Every WAL segment, memtable slot, manifest entry, and Parquet file is keyed by
//! seal-key. A `ScribeIngressFrame` that spans multiple partitions is split into
//! per-key slices before WAL writes; a seal-key never crosses a partition.
//!
//! The partition scheme — the time axis plus any additional dimensions — is
//! not a property of this module: it comes from the table's canonical
//! `PhysicalLayout` and is passed in by the admission path, so every table uses
//! exactly the same code with its own columns and transforms.
//!
//! `TableRef` lives in `crate::catalog::table_ref` and is re-exported through
//! `crate::catalog` — this module only owns the seal-key composition.

use arrow::array::UInt32Array;
use arrow::compute::take;
use arrow::record_batch::RecordBatch;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::LayoutRevision;
use crate::catalog::TableRef;
use crate::catalog::TenantTableBinding;
pub use crate::catalog::layout::TimePartition;
pub use crate::catalog::{PartitionScheme, PhysicalPartition};
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
    partition: PhysicalPartition,
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
        partition: PhysicalPartition,
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
            self.partition.time().as_path_components(),
            self.node_id,
            self.writer_epoch,
            self.claim,
            self.wal_lsn_min,
            self.wal_lsn_max,
        )
    }
}

/// Seal key — `(DataTenantId, TableRef, LayoutRevision, PhysicalPartition)`
/// identifying one WAL/memtable/seal scope.
///
/// The revision is part of the scope, so batches accepted under different
/// schema or sort-order revisions never share a generation and each seals
/// under the retained metadata it was accepted with.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SealKey {
    /// Tenant ID for this seal scope.
    pub tenant: DataTenantId,
    /// Table reference for this seal scope.
    pub table: TableRef,
    /// Schema and sort-order revision every batch in the scope is bound to.
    pub revision: LayoutRevision,
    /// Exact partition for this seal scope.
    pub partition: PhysicalPartition,
}

impl SealKey {
    /// Construct a new seal key under the creation revision; a bare
    /// [`TimePartition`] is the time-only identity.
    #[must_use]
    pub fn new(
        tenant: DataTenantId,
        table: TableRef,
        partition: impl Into<PhysicalPartition>,
    ) -> Self {
        Self {
            tenant,
            table,
            revision: LayoutRevision::INITIAL,
            partition: partition.into(),
        }
    }

    /// Rebinds the key to `revision`.
    #[must_use]
    pub const fn with_revision(mut self, revision: LayoutRevision) -> Self {
        self.revision = revision;
        self
    }

    /// Returns the canonical text identity of this seal scope,
    /// `{namespace}/{table}/tenant={tenant}/{revision}/{partition}`.
    ///
    /// WAL manifests and replay indexes key on this text, so it is injective:
    /// it renders the revision ids and the partition's spec, time, and every
    /// typed value.
    #[must_use]
    pub fn canonical_key(&self) -> String {
        self.to_string()
    }
}

impl std::fmt::Display for SealKey {
    /// Renders the [`Self::canonical_key`] text.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}/tenant={}/{}/{}",
            self.table.namespace.as_str(),
            self.table.name,
            self.tenant.as_uuid(),
            self.revision,
            self.partition
        )
    }
}

/// Maximum distinct physical partitions accepted from one ingest source.
///
/// The bound is on complete partition tuples: an hourly table admits at most
/// 32 distinct hours from one source, and a table with additional dimensions
/// at most 32 distinct `(time, values…)` tuples, so a repeated tuple counts
/// once and two dimension values within one hour count twice. This is the
/// single accounting path — Gate configuration, the material plan, and
/// telemetry all report this same unit.
pub(crate) const MAX_PHYSICAL_PARTITIONS: usize =
    crate::gate::limits::OTLP_WIRE_LIMITS.time_partitions;

/// Immutable bounded partition count plan for one current source.
///
/// The plan is built by one non-retaining counting pass so the producer can
/// materialize partitions lazily, one at a time, without holding a per-partition
/// row collection.
#[derive(Debug, Clone)]
pub(crate) struct PartitionPlan {
    /// Sorted distinct partitions with the exact rows assigned to each.
    partitions: Vec<(PhysicalPartition, usize)>,
    /// Scheme the counting pass partitioned by, reused by materialization.
    scheme: PartitionScheme,
}

impl PartitionPlan {
    /// Returns the exact number of distinct planned partitions.
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.partitions.len()
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
    ) -> Result<(PhysicalPartition, RecordBatch), ScribeError> {
        let (partition, expected_rows) = self
            .partitions
            .get(index)
            .cloned()
            .ok_or(ScribeError::InvalidFrame)?;
        if self.partitions.len() == 1 {
            return Ok((partition, batch.clone()));
        }
        let rows = row_partitions(batch, &self.scheme)?;
        let mut indices = Vec::with_capacity(expected_rows);
        for row in 0..rows.len() {
            if rows.partition(row).ok_or(ScribeError::InvalidFrame)? == partition {
                indices.push(u32::try_from(row).map_err(|_| ScribeError::Internal {
                    detail: "record batch row index exceeds u32::MAX".to_owned(),
                })?);
            }
        }
        if indices.len() != expected_rows || indices.capacity() != expected_rows {
            return Err(ScribeError::Internal {
                detail: "partition materialization diverged from its count pass".to_owned(),
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

/// Current-only iterator over deterministic partition slices.
pub(crate) struct PartitionSlices<'a> {
    /// Retained source whose buffers remain alive for single-partition aliasing.
    batch: &'a RecordBatch,
    /// Immutable descriptors shared with lazy native production.
    plan: PartitionPlan,
    /// Descriptor produced by the next iterator call.
    current: usize,
}

impl Iterator for PartitionSlices<'_> {
    type Item = Result<(PhysicalPartition, RecordBatch), ScribeError>;

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

impl ExactSizeIterator for PartitionSlices<'_> {}

/// Plans deterministic partition slices without per-partition row
/// collections.
///
/// # Errors
///
/// Returns [`ScribeError`] for a missing, null, mistyped, or invalid time or
/// dimension column, a row index beyond `u32`, or more than
/// [`MAX_PHYSICAL_PARTITIONS`] distinct partitions.
pub(crate) fn split_batch_by_partition<'a>(
    batch: &'a RecordBatch,
    scheme: &PartitionScheme,
) -> Result<PartitionSlices<'a>, ScribeError> {
    Ok(PartitionSlices {
        batch,
        plan: plan_partitions(batch, scheme)?,
        current: 0,
    })
}

/// Applies `scheme` to one admitted batch.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a partition source column is absent
/// or of a type its transform cannot partition, which a stamped batch of a
/// registered table never is.
fn row_partitions<'a>(
    batch: &'a RecordBatch,
    scheme: &PartitionScheme,
) -> Result<crate::catalog::physical_partition::RowPartitions<'a>, ScribeError> {
    scheme.rows(batch).map_err(|error| ScribeError::Internal {
        detail: error.to_string(),
    })
}

/// Counts one current source into an owned bounded partition plan.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] for a missing or mistyped partition source
/// and checked row-count overflow, and [`ScribeError::InvalidFrame`] for a
/// null or unrepresentable instant or more than [`MAX_PHYSICAL_PARTITIONS`]
/// distinct partitions, so nothing of the request is written.
pub(crate) fn plan_partitions(
    batch: &RecordBatch,
    scheme: &PartitionScheme,
) -> Result<PartitionPlan, ScribeError> {
    let rows = row_partitions(batch, scheme)?;
    let mut partitions: Vec<(PhysicalPartition, usize)> =
        Vec::with_capacity(MAX_PHYSICAL_PARTITIONS.min(rows.len()));
    for index in 0..rows.len() {
        let partition = rows.partition(index).ok_or(ScribeError::InvalidFrame)?;
        match partitions.binary_search_by(|(candidate, _)| candidate.cmp(&partition)) {
            Ok(position) => {
                partitions[position].1 =
                    partitions[position]
                        .1
                        .checked_add(1)
                        .ok_or_else(|| ScribeError::Internal {
                            detail: "partition row count overflow".to_owned(),
                        })?;
            }
            Err(_) if partitions.len() == MAX_PHYSICAL_PARTITIONS => {
                return Err(ScribeError::InvalidFrame);
            }
            Err(position) => partitions.insert(position, (partition, 1)),
        }
    }
    if partitions.is_empty() {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(PartitionPlan {
        partitions,
        scheme: scheme.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{TimeAxis, TimeGranularity};
    use crate::namespaces::BifrostNamespace;
    use arrow::array::{Int64Array, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use chrono::{DateTime, Utc};
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

    /// Builds the fixture time-only scheme at `granularity`.
    fn axis(granularity: TimeGranularity) -> PartitionScheme {
        PartitionScheme::time_only(TimeAxis::new(TIME_COLUMN, granularity))
    }

    /// Builds a two-column batch whose time-column values are the supplied micros.
    fn batch_with_event_times(micros: Vec<i64>) -> RecordBatch {
        let values: Vec<i64> = (0..micros.len().try_into().expect("row count fits i64")).collect();
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                TIME_COLUMN,
                DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(micros).with_timezone("+00:00")),
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

    /// The seal-key text carries the typed partition, not a bare day segment.
    #[test]
    fn seal_key_canonical_key() {
        let tenant = crate::test_support::tenant();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let seal_key = SealKey::new(
            tenant,
            table,
            PhysicalPartition::time_only(partition(TimeGranularity::Hour, "2026-07-14T09:00:00Z")),
        );

        let key = seal_key.canonical_key();
        assert!(key.contains("vala.bifrost"));
        assert!(key.contains("events"));
        assert!(key.contains("hour:2026-07-14T09:00:00+00:00"));
        assert!(!key.contains("day"));
    }

    /// A single-partition source aliases its Arrow value buffers instead of
    /// copying them.
    #[test]
    fn one_partition_split_reuses_arrow_value_buffers() {
        let source = batch_with_event_times(vec![1_784_040_000_000_000, 1_784_040_060_000_000]);

        let slices = split_batch_by_partition(&source, &axis(TimeGranularity::Hour))
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

        let hourly = split_batch_by_partition(&source, &axis(TimeGranularity::Hour))
            .expect("hourly plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("hourly split");
        assert_eq!(hourly.len(), 2);
        assert_eq!(hourly[0].1.num_rows(), 2);
        assert_eq!(hourly[1].1.num_rows(), 2);
        assert_ne!(hourly[0].0, hourly[1].0);
        assert!(hourly[0].0 < hourly[1].0);

        let daily = split_batch_by_partition(&source, &axis(TimeGranularity::Day))
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
            (0..MAX_PHYSICAL_PARTITIONS)
                .map(|index| base + hour_micros * i64::try_from(index).expect("index fits i64"))
                .collect(),
        );
        let plan = plan_partitions(&admitted, &axis(TimeGranularity::Hour))
            .expect("32 partitions admitted");
        assert_eq!(plan.len(), MAX_PHYSICAL_PARTITIONS);

        let rejected = batch_with_event_times(
            (0..=MAX_PHYSICAL_PARTITIONS)
                .map(|index| base + hour_micros * i64::try_from(index).expect("index fits i64"))
                .collect(),
        );
        assert!(matches!(
            plan_partitions(&rejected, &axis(TimeGranularity::Hour)),
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
        let plan = plan_partitions(&far, &axis(TimeGranularity::Hour))
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
        let plan = plan_partitions(&nanos, &axis(TimeGranularity::Hour))
            .expect("nanosecond instants are accepted");
        assert_eq!(
            plan.len(),
            2,
            "one nanosecond before the epoch is the prior hour"
        );

        for extreme in [i64::MIN, i64::MAX] {
            assert!(matches!(
                plan_partitions(
                    &batch_with_event_times(vec![extreme]),
                    &axis(TimeGranularity::Hour)
                ),
                Err(ScribeError::InvalidFrame)
            ));
        }
    }

    /// Builds a one-hour batch whose rows carry `customers` and `shards`.
    fn composite_batch(customers: Vec<Option<&str>>, shards: Vec<i64>) -> RecordBatch {
        let rows = customers.len();
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                TIME_COLUMN,
                DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
                false,
            ),
            Field::new("customer", DataType::Utf8, true),
            Field::new("shard", DataType::Int64, false),
        ]));
        let micros =
            (0..rows).map(|row| 1_784_016_000_000_000 + i64::try_from(row).expect("row fits i64"));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(
                    TimestampMicrosecondArray::from_iter_values(micros).with_timezone("+00:00"),
                ),
                Arc::new(arrow::array::StringArray::from(customers)),
                Arc::new(Int64Array::from(shards)),
            ],
        )
        .expect("composite batch builds")
    }

    /// Builds the scheme of hourly `TIME_COLUMN` plus `dimensions`.
    fn composite_scheme(
        dimensions: Vec<wyrd_spec::vala::api::PartitionFieldWire>,
    ) -> PartitionScheme {
        use wyrd_spec::vala::api::{
            PartitionFieldWire, PartitionTransformWire, PhysicalLayoutWire,
        };
        let mut partition_by = vec![PartitionFieldWire::new(
            TIME_COLUMN,
            PartitionTransformWire::Hour,
        )];
        partition_by.extend(dimensions);
        PartitionScheme::of_wire(
            &PhysicalLayoutWire {
                time_column: Some(TIME_COLUMN.to_owned()),
                partition_by,
                sort_keys: Vec::new(),
                bloom_columns: Vec::new(),
            },
            crate::catalog::layout::BIFROST_PARTITION_SPEC_ID,
        )
        .expect("the fixture scheme is valid")
    }

    #[test]
    fn composite_partition_identity_and_wal() {
        use crate::catalog::PhysicalPartitionError;
        use wyrd_spec::vala::api::{
            PartitionFieldWire, PartitionTransformWire, PartitionValueWire,
        };

        let identity = PartitionFieldWire::new("customer", PartitionTransformWire::Identity);
        let bucket = PartitionFieldWire {
            buckets: Some(16),
            ..PartitionFieldWire::new("shard", PartitionTransformWire::Bucket)
        };
        let truncate = PartitionFieldWire {
            width: Some(1),
            ..PartitionFieldWire::new("customer", PartitionTransformWire::Truncate)
        };

        // Identity and Iceberg bucket split one hour; a repeated tuple counts
        // once and a null source is its own partition.
        let batch = composite_batch(vec![Some("a"), Some("a"), None, Some("b")], vec![34; 4]);
        let scheme = composite_scheme(vec![identity.clone(), bucket]);
        let rows = scheme.rows(&batch).expect("the transforms apply");
        let hour = rows.partition(0).expect("row 0 partitions").time();
        assert_eq!(
            rows.partition(0).expect("row 0 partitions").values(),
            [
                Some(PartitionValueWire::Utf8("a".to_owned())),
                // Iceberg's Murmur3 bucket of long 34 over 16 buckets is 3.
                Some(PartitionValueWire::Int32(3)),
            ]
        );
        assert_eq!(
            rows.partition(2).expect("row 2 partitions").values()[0],
            None
        );
        assert_eq!(plan_partitions(&batch, &scheme).expect("plans").len(), 3);
        assert_eq!(
            plan_partitions(&batch, &axis(TimeGranularity::Hour))
                .expect("plans")
                .len(),
            1,
            "time-only grouping merges the composite tuples"
        );

        // Truncate merges values sharing a prefix.
        let fruit = composite_batch(
            vec![Some("apple"), Some("avocado"), Some("banana")],
            vec![0; 3],
        );
        let truncated = composite_scheme(vec![truncate]);
        assert_eq!(plan_partitions(&fruit, &truncated).expect("plans").len(), 2);
        assert_eq!(
            truncated
                .rows(&fruit)
                .expect("applies")
                .partition(1)
                .expect("row 1")
                .values(),
            [Some(PartitionValueWire::Utf8("a".to_owned()))]
        );

        // 32 distinct tuples plan; a 33rd refuses the whole batch.
        let names: Vec<String> = (0..=MAX_PHYSICAL_PARTITIONS)
            .map(|index| format!("c{index}"))
            .collect();
        let customers = |count: usize| {
            names[..count]
                .iter()
                .map(|name| Some(name.as_str()))
                .collect()
        };
        let identity_only = composite_scheme(vec![identity]);
        assert_eq!(
            plan_partitions(&composite_batch(customers(32), vec![0; 32]), &identity_only)
                .expect("32 tuples plan")
                .len(),
            MAX_PHYSICAL_PARTITIONS
        );
        assert!(matches!(
            plan_partitions(&composite_batch(customers(33), vec![0; 33]), &identity_only),
            Err(ScribeError::InvalidFrame)
        ));

        // Null, literal, type, and spec are each part of the identity.
        let with = |spec_id, value| {
            PhysicalPartition::new(spec_id, hour, 0, vec![value]).expect("one dimension is valid")
        };
        let distinct = [
            with(0, None),
            with(0, Some(PartitionValueWire::Utf8(String::new()))),
            with(0, Some(PartitionValueWire::Int32(1))),
            with(0, Some(PartitionValueWire::Int64(1))),
            with(1, Some(PartitionValueWire::Int64(1))),
            PhysicalPartition::time_only(hour),
        ];
        for (index, left) in distinct.iter().enumerate() {
            assert_eq!(
                PhysicalPartition::decode(&left.to_bytes()).as_ref(),
                Ok(left)
            );
            assert_eq!(
                PhysicalPartition::from_wire(&left.to_wire()).as_ref(),
                Ok(left)
            );
            for right in &distinct[index + 1..] {
                assert_ne!(left, right);
                assert_ne!(left.to_bytes(), right.to_bytes());
                assert_ne!(left.to_string(), right.to_string());
            }
        }

        // The codec fails closed on truncation, trailing bytes, and an
        // unknown value tag.
        let encoded = distinct[2].to_bytes();
        assert!(PhysicalPartition::decode(&encoded[..encoded.len() - 1]).is_err());
        assert!(PhysicalPartition::decode(&[encoded.as_slice(), &[0]].concat()).is_err());
        let mut unknown = encoded.clone();
        unknown[15] = 9;
        assert!(matches!(
            PhysicalPartition::decode(&unknown),
            Err(PhysicalPartitionError::UnknownValueTag(9))
        ));

        // Every identity survives a real WAL round trip onto its own cohort.
        let temp_dir = tempfile::TempDir::new().expect("temp WAL directory");
        let wal = crate::scribe::wal::WalWriter::new(
            temp_dir.path(),
            *uuid::Uuid::now_v7().as_bytes(),
            1,
            crate::scribe::wal::WalConfig::default(),
        )
        .expect("WAL writer");
        let tenant = crate::test_support::tenant();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let keys: Vec<SealKey> = distinct
            .iter()
            .map(|partition| SealKey::new(tenant, table.clone(), partition.clone()))
            .collect();
        for (index, key) in keys.iter().enumerate() {
            let byte = u8::try_from(index).expect("fixture index fits u8");
            wal.append_and_commit_for_replay_test(key, [byte; 16], &[byte])
                .expect("append and commit");
        }
        let replayed = crate::scribe::replay::replay_wal_directory(temp_dir.path())
            .expect("replay the WAL directory");
        assert_eq!(replayed.len(), keys.len());
        for key in &keys {
            assert_eq!(replayed[&key.canonical_key()].seal_key, *key);
        }
    }
}
