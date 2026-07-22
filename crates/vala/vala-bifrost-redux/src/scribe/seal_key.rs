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
use crate::contracts::ScribeError;

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

/// Split a batch into deterministic UTC event-day slices.
pub(crate) fn split_batch_by_event_day(
    batch: &RecordBatch,
) -> Result<Vec<(EventDay, RecordBatch)>, ScribeError> {
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

    let mut day_indices = std::collections::BTreeMap::<NaiveDate, Vec<u32>>::new();
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
        day_indices
            .entry(timestamp.date_naive())
            .or_default()
            .push(u32::try_from(index).map_err(|_| ScribeError::Internal {
                detail: "record batch row index exceeds u32::MAX".to_string(),
            })?);
    }

    day_indices
        .into_iter()
        .map(|(day, indices)| {
            let indices = UInt32Array::from(indices);
            let columns = batch
                .columns()
                .iter()
                .map(|column| {
                    take(column.as_ref(), &indices, None).map_err(|error| ScribeError::Internal {
                        detail: format!("Arrow day split failed: {error}"),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let sliced = RecordBatch::try_new(batch.schema(), columns).map_err(|error| {
                ScribeError::Internal {
                    detail: format!("Arrow day slice construction failed: {error}"),
                }
            })?;
            Ok((EventDay::new(day), sliced))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;

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
        let tenant = DataTenantId::SYSTEM_OWNER;
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let day = EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap());
        let seal_key = SealKey::new(tenant, table, day);

        let path = seal_key.as_path_components();
        assert!(path.contains("vala.bifrost"));
        assert!(path.contains("events"));
        assert!(path.contains("2026-07-14"));
    }
}
