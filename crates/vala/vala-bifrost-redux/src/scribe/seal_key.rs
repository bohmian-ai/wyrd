//! Seal key — `(DataTenantId, TableRef, EventDay)` routing for WAL and memtable.
//!
//! Every WAL segment, memtable slot, manifest entry, and Parquet file is keyed by
//! seal-key. A `ScribeAppend` that spans multiple event days is split into per-key
//! slices before WAL writes; a seal-key never crosses a partition day.
//!
//! `TableRef` lives in `crate::catalog::table_ref` and is re-exported through
//! `crate::catalog` — this module only owns the seal-key composition.

use arrow::array::{Array, TimestampMicrosecondArray, UInt32Array};
use arrow::compute::take;
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, NaiveDate, Utc};
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;

/// Durable identity shared by every producer of one Scribe artifact set.
///
/// The identity deliberately excludes process-local generation and seal counters.
/// Its complete tuple matches the durable file-list conflict scope while also
/// retaining the partition and shard needed to keep object paths inspectable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScribeArtifactIdentity {
    /// Tenant-qualified object prefix for the logical table.
    object_prefix: String,
    /// Event-day partition represented by the artifact set.
    partition_day: EventDay,
    /// Producing Scribe node without UUID punctuation.
    node_id: String,
    /// Producing Scribe writer epoch.
    writer_epoch: i64,
    /// Pod-local shard lane that produced the artifact set.
    shard_id: usize,
    /// Inclusive minimum WAL LSN represented by the artifact set.
    wal_lsn_min: u64,
    /// Inclusive maximum WAL LSN represented by the artifact set.
    wal_lsn_max: u64,
}

impl ScribeArtifactIdentity {
    /// Constructs the durable identity after validating its node identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when `node_id` is not a UUID or when the WAL
    /// range is reversed.
    pub(crate) fn new(
        binding: &TenantTableBinding,
        partition_day: EventDay,
        node_id: &str,
        writer_epoch: i64,
        shard_id: usize,
        wal_lsn_min: u64,
        wal_lsn_max: u64,
    ) -> Result<Self, ScribeError> {
        let node_id = uuid::Uuid::parse_str(node_id).map_err(|error| ScribeError::Internal {
            detail: format!("node_id is not a valid UUID: {error}"),
        })?;
        if wal_lsn_min > wal_lsn_max {
            return Err(ScribeError::Internal {
                detail: "Scribe artifact WAL range is reversed".to_owned(),
            });
        }
        Ok(Self {
            object_prefix: binding.object_prefix.clone(),
            partition_day,
            node_id: node_id.simple().to_string(),
            writer_epoch,
            shard_id,
            wal_lsn_min,
            wal_lsn_max,
        })
    }

    /// Returns the deterministic base shared by every ordinal in the set.
    #[must_use]
    pub(crate) fn object_base(&self) -> String {
        format!(
            "{}/day={}/scribe-{}-epoch-{}-shard-{}-wal-{}-{}",
            self.object_prefix,
            self.partition_day,
            self.node_id,
            self.writer_epoch,
            self.shard_id,
            self.wal_lsn_min,
            self.wal_lsn_max,
        )
    }
}

/// Event day — the partition day extracted from `wyrd_event_time`.
///
/// Stored as a `NaiveDate` (year-month-day, no timezone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventDay(NaiveDate);

impl EventDay {
    /// Construct an `EventDay` from a `NaiveDate`.
    #[must_use]
    pub const fn new(date: NaiveDate) -> Self {
        Self(date)
    }

    /// Extract the event day from a `wyrd_event_time` timestamp.
    #[must_use]
    pub fn from_timestamp(dt: DateTime<Utc>) -> Self {
        Self(dt.date_naive())
    }

    /// Get a reference to the underlying `NaiveDate`.
    #[must_use]
    pub const fn as_date(&self) -> &NaiveDate {
        &self.0
    }

    /// Get the underlying `NaiveDate` value (for SQL bindings).
    #[must_use]
    pub const fn as_naive_date(&self) -> NaiveDate {
        self.0
    }

    /// Format as `YYYY-MM-DD`.
    #[must_use]
    pub fn as_string(&self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }
}

impl std::fmt::Display for EventDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

/// Seal key — `(DataTenantId, TableRef, EventDay)` identifying one WAL/memtable/seal scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SealKey {
    /// Tenant ID for this seal scope.
    pub tenant: DataTenantId,
    /// Table reference for this seal scope.
    pub table: TableRef,
    /// Event day (partition day) for this seal scope.
    pub day: EventDay,
}

impl SealKey {
    /// Construct a new seal key.
    #[must_use]
    pub fn new(tenant: DataTenantId, table: TableRef, day: EventDay) -> Self {
        Self { tenant, table, day }
    }

    /// Format as a directory path component:
    /// `{namespace}/{table}/tenant={tenant}/day={YYYY-MM-DD}`
    #[must_use]
    pub fn as_path_components(&self) -> String {
        format!(
            "{}/{}/tenant={}/day={}",
            self.table.namespace.as_str(),
            self.table.name,
            self.tenant.as_uuid(),
            self.day.as_string()
        )
    }
}

impl std::fmt::Display for SealKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_path_components())
    }
}

/// Maximum distinct UTC event days accepted in one ingest request.
const MAX_EVENT_DAYS: usize = 32;

/// Current-only iterator over deterministic UTC event-day slices.
pub(crate) struct EventDaySlices<'a> {
    /// Retained source whose buffers remain alive for one-day aliasing.
    batch: &'a RecordBatch,
    /// Sorted fixed-capacity day descriptors.
    days: [Option<NaiveDate>; MAX_EVENT_DAYS],
    /// Exact row count for each live day descriptor.
    counts: [usize; MAX_EVENT_DAYS],
    /// Number of live descriptors.
    day_count: usize,
    /// Descriptor produced by the next iterator call.
    current: usize,
}

impl Iterator for EventDaySlices<'_> {
    type Item = Result<(EventDay, RecordBatch), ScribeError>;

    /// Materializes only the next day and drops its exact index workspace on
    /// the following call.
    fn next(&mut self) -> Option<Self::Item> {
        let day = self.days.get(self.current).copied().flatten()?;
        let expected_rows = self.counts[self.current];
        self.current += 1;
        if self.day_count == 1 {
            return Some(Ok((EventDay::new(day), self.batch.clone())));
        }
        let result = (|| {
            let ts_col = self
                .batch
                .column_by_name("wyrd_event_time")
                .ok_or_else(|| ScribeError::Internal {
                    detail: "missing wyrd_event_time column".to_owned(),
                })?;
            let ts_array = ts_col
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "wyrd_event_time must be TimestampMicrosecond".to_owned(),
                })?;
            let mut indices = Vec::with_capacity(expected_rows);
            for index in 0..ts_array.len() {
                let timestamp = chrono::DateTime::from_timestamp_micros(ts_array.value(index))
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "invalid event timestamp during planned split".to_owned(),
                    })?;
                if timestamp.date_naive() == day {
                    indices.push(u32::try_from(index).map_err(|_| ScribeError::Internal {
                        detail: "record batch row index exceeds u32::MAX".to_owned(),
                    })?);
                }
            }
            if indices.len() != expected_rows || indices.capacity() != expected_rows {
                return Err(ScribeError::Internal {
                    detail: "event-day materialization diverged from its count pass".to_owned(),
                });
            }
            let indices = UInt32Array::from(indices);
            let columns = self
                .batch
                .columns()
                .iter()
                .map(|column| {
                    take(column.as_ref(), &indices, None).map_err(|error| ScribeError::Internal {
                        detail: format!("Arrow day split failed: {error}"),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let sliced = RecordBatch::try_new(self.batch.schema(), columns).map_err(|error| {
                ScribeError::Internal {
                    detail: format!("Arrow day slice construction failed: {error}"),
                }
            })?;
            Ok((EventDay::new(day), sliced))
        })();
        Some(result)
    }

    /// Reports the exact unmaterialized day count.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.day_count.saturating_sub(self.current);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for EventDaySlices<'_> {}

/// Plans deterministic UTC event-day slices without per-day row collections.
///
/// # Errors
///
/// Returns [`ScribeError`] for a missing, null, mistyped, or invalid event-time
/// column, a row index beyond `u32`, or more than 32 distinct days.
pub(crate) fn split_batch_by_event_day(
    batch: &RecordBatch,
) -> Result<EventDaySlices<'_>, ScribeError> {
    let ts_col = batch
        .column_by_name("wyrd_event_time")
        .ok_or_else(|| ScribeError::Internal {
            detail: "missing wyrd_event_time column".to_string(),
        })?;
    let ts_array = ts_col
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: "wyrd_event_time must be TimestampMicrosecond".to_string(),
        })?;

    let mut days: [Option<NaiveDate>; MAX_EVENT_DAYS] = [None; MAX_EVENT_DAYS];
    let mut counts = [0_usize; MAX_EVENT_DAYS];
    let mut day_count = 0_usize;
    for index in 0..ts_array.len() {
        if ts_array.is_null(index) {
            return Err(ScribeError::Internal {
                detail: "wyrd_event_time cannot be null".to_string(),
            });
        }
        let micros = ts_array.value(index);
        let timestamp = chrono::DateTime::from_timestamp_micros(micros).ok_or_else(|| {
            ScribeError::Internal {
                detail: format!("invalid timestamp micros: {micros}"),
            }
        })?;
        let day = timestamp.date_naive();
        let position = days[..day_count]
            .binary_search_by(|candidate| {
                candidate.map_or(std::cmp::Ordering::Less, |v| v.cmp(&day))
            })
            .unwrap_or_else(|position| position);
        if days.get(position).copied().flatten() == Some(day) {
            counts[position] =
                counts[position]
                    .checked_add(1)
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "event-day row count overflow".to_owned(),
                    })?;
            continue;
        }
        if day_count == MAX_EVENT_DAYS {
            return Err(ScribeError::InvalidFrame);
        }
        for move_index in (position..day_count).rev() {
            days[move_index + 1] = days[move_index];
            counts[move_index + 1] = counts[move_index];
        }
        days[position] = Some(day);
        counts[position] = 1;
        day_count += 1;
    }
    if day_count == 0 {
        return Err(ScribeError::InvalidFrame);
    }
    Ok(EventDaySlices {
        batch,
        days,
        counts,
        day_count,
        current: 0,
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

    /// Builds one tenant-qualified binding for durable identity tests.
    fn artifact_binding() -> TenantTableBinding {
        TenantTableBinding::resolve((
            crate::test_support::tenant(),
            TableRef::new(BifrostNamespace::Bifrost, "artifact_identity"),
        ))
        .expect("artifact identity binding")
    }

    /// Durable Scribe identity changes for every conflict-key dimension and
    /// remains stable for an exact retry and its ordered artifact ordinals.
    #[test]
    fn scribe_artifact_identity_matrix_is_durable_and_retry_stable() {
        let binding = artifact_binding();
        let day = EventDay::new(NaiveDate::from_ymd_opt(2026, 8, 15).expect("valid day"));
        let node = "018f7ca2-7a4d-7cc1-98a7-97fdd1f15101";
        let identity = ScribeArtifactIdentity::new(&binding, day, node, 7, 3, 101, 109)
            .expect("durable identity");
        let retry = ScribeArtifactIdentity::new(&binding, day, node, 7, 3, 101, 109)
            .expect("retry identity");
        let base = identity.object_base();
        assert_eq!(base, retry.object_base());
        assert_eq!(
            format!("{base}-{:05}.parquet", 0),
            format!("{}-{:05}.parquet", retry.object_base(), 0)
        );
        assert_ne!(
            format!("{base}-{:05}.parquet", 0),
            format!("{base}-{:05}.parquet", 1)
        );

        for changed in [
            ScribeArtifactIdentity::new(&binding, day, node, 8, 3, 101, 109),
            ScribeArtifactIdentity::new(&binding, day, node, 7, 4, 101, 109),
            ScribeArtifactIdentity::new(&binding, day, node, 7, 3, 100, 109),
            ScribeArtifactIdentity::new(&binding, day, node, 7, 3, 101, 110),
        ] {
            assert_ne!(base, changed.expect("changed identity").object_base());
        }
    }

    /// Reversed WAL ranges fail before an object path can be constructed.
    #[test]
    fn scribe_artifact_identity_rejects_reversed_wal_range() {
        let binding = artifact_binding();
        let error = ScribeArtifactIdentity::new(
            &binding,
            EventDay::new(NaiveDate::from_ymd_opt(2026, 8, 15).expect("valid day")),
            "018f7ca2-7a4d-7cc1-98a7-97fdd1f15101",
            7,
            3,
            110,
            109,
        )
        .expect_err("reversed range must fail");
        assert!(matches!(error, ScribeError::Internal { .. }));
    }

    #[test]
    fn event_day_from_timestamp() {
        let dt = DateTime::parse_from_rfc3339("2026-07-14T12:34:56Z")
            .unwrap()
            .with_timezone(&Utc);
        let day = EventDay::from_timestamp(dt);
        assert_eq!(day.as_string(), "2026-07-14");
    }

    #[test]
    fn seal_key_path_components() {
        let tenant = crate::test_support::tenant();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let day = EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap());
        let seal_key = SealKey::new(tenant, table, day);

        let path = seal_key.as_path_components();
        assert!(path.contains("vala.bifrost"));
        assert!(path.contains("events"));
        assert!(path.contains("2026-07-14"));
    }

    #[test]
    fn one_day_split_reuses_arrow_value_buffers() {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ]));
        let source = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(vec![
                    1_784_040_000_000_000,
                    1_784_043_600_000_000,
                ])),
                Arc::new(Int64Array::from(vec![10, 20])),
            ],
        )
        .expect("one-day batch");

        let slices = split_batch_by_event_day(&source)
            .expect("split plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("split");
        assert_eq!(slices.len(), 1);
        assert!(Arc::ptr_eq(source.column(0), slices[0].1.column(0)));
        assert!(Arc::ptr_eq(source.column(1), slices[0].1.column(1)));
    }

    #[test]
    fn cross_day_split_materializes_each_day_once() {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ]));
        let source = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(vec![
                    1_783_929_600_000_000,
                    1_783_933_200_000_000,
                    1_784_016_000_000_000,
                    1_784_019_600_000_000,
                ])),
                Arc::new(Int64Array::from(vec![1, 2, 3, 4])),
            ],
        )
        .expect("cross-day batch");

        let slices = split_batch_by_event_day(&source)
            .expect("split plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("split");
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].1.num_rows(), 2);
        assert_eq!(slices[1].1.num_rows(), 2);
        assert_eq!(
            slices
                .iter()
                .map(|(_, batch)| batch.num_rows())
                .sum::<usize>(),
            4
        );
        assert_ne!(slices[0].0, slices[1].0);
    }
}
