//! Seal key — `(DataTenantId, TableRef, EventDay)` routing for WAL and memtable.
//!
//! Every WAL segment, memtable slot, manifest entry, and Parquet file is keyed by
//! seal-key. A `ScribeAppend` that spans multiple event days is split into per-key
//! slices before WAL writes; a seal-key never crosses a partition day.
//!
//! `TableRef` lives in `crate::catalog::table_ref` and is re-exported through
//! `crate::catalog` — this module only owns the seal-key composition.

use chrono::{DateTime, NaiveDate, Utc};
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;

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
