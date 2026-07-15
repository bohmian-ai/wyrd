//! Seal key — `(DataTenantId, TableRef, EventDay)` routing for WAL and memtable.
//!
//! Every WAL segment, memtable slot, manifest entry, and Parquet file is keyed by
//! seal-key. A `ScribeAppend` that spans multiple event days is split into per-key
//! slices before WAL writes; a seal-key never crosses a partition day.

use chrono::{DateTime, NaiveDate, Utc};
use wyrd_spec::ids::DataTenantId;

use crate::contracts::ScribeError;

/// Check if a string is a safe identifier for object-store paths.
///
/// Allows only `[A-Za-z0-9_-]`, max 63 characters, prevents path traversal
/// attacks on opendal-fs backends.
fn is_safe_identifier(s: &str) -> bool {
    if s.is_empty() || s.len() > 63 {
        return false;
    }

    // Reject path traversal patterns
    if s.contains("..") || s.contains('/') || s.contains('\\') {
        return false;
    }

    // Allow only alphanumeric, underscore, hyphen
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Table reference — namespace + name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableRef {
    /// Namespace (e.g., "vala.bifrost").
    pub namespace: String,
    /// Table name (e.g., "events").
    pub name: String,
}

impl TableRef {
    /// Construct a new table reference.
    #[must_use]
    pub fn new(namespace: String, name: String) -> Self {
        Self { namespace, name }
    }

    /// Parse a fully-qualified table name into `(namespace, name)`.
    ///
    /// Returns `None` for names that don't have at least one dot separator,
    /// if the table name part contains dots, or if either part is unsafe for
    /// object-store path construction (prevents path traversal on opendal-fs).
    pub fn parse_fqn(fqn: &str) -> Option<Self> {
        let mut parts = fqn.rsplitn(2, '.');
        let name = parts.next()?.to_string();
        let namespace = parts.next()?.to_string();

        // Reject if either part is empty, if the table name contains dots,
        // or if either part is not a safe identifier
        if name.is_empty()
            || namespace.is_empty()
            || name.contains('.')
            || !is_safe_identifier(&namespace)
            || !is_safe_identifier(&name)
        {
            return None;
        }

        Some(Self::new(namespace, name))
    }

    /// Format as fully-qualified name.
    #[must_use]
    pub fn fqn(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }
}

impl std::fmt::Display for TableRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.fqn())
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
            self.table.namespace,
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

/// Per-day slice of a batch — one partition of a cross-day `ScribeAppend`.
#[derive(Debug, Clone)]
pub struct DaySlice {
    /// The seal key for this slice.
    pub seal_key: SealKey,
    /// Row indices from the original batch that belong to this day.
    pub row_indices: Vec<usize>,
}

/// Split a batch by `wyrd_event_time` into per-day slices.
///
/// This function is a placeholder for the real Arrow-based implementation in .
/// The real version will:
/// - Take an Arrow `RecordBatch` and extract the `wyrd_event_time` column
/// - Partition rows by `Day(wyrd_event_time)`
/// - Return one `DaySlice` per day with row indices
///
/// For (this PR), we only need the signature and the contract test; the
/// memtable integration happens in .
///
/// # Errors
/// Returns [`ScribeError::Internal`] if:
/// - The batch has no `wyrd_event_time` column
/// - The `wyrd_event_time` column has an incompatible type
pub fn split_batch_by_event_day(
    _batch: &[u8], // Placeholder — real signature is `RecordBatch`
    _tenant: DataTenantId,
    _table: TableRef,
) -> Result<Vec<DaySlice>, ScribeError> {
    // Placeholder implementation for  // Real implementation in when Arrow batch handling lands
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_ref_parse_and_format() {
        let table_ref = TableRef::parse_fqn("vala.bifrost.events").unwrap();
        assert_eq!(table_ref.namespace, "vala.bifrost");
        assert_eq!(table_ref.name, "events");
        assert_eq!(table_ref.fqn(), "vala.bifrost.events");

        assert!(TableRef::parse_fqn("no_dot").is_none());

        // Namespace can have dots, only table name must not
        let multi_dot = TableRef::parse_fqn("too.many.dots").unwrap();
        assert_eq!(multi_dot.namespace, "too.many");
        assert_eq!(multi_dot.name, "dots");
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
        let tenant = DataTenantId::SYSTEM_OWNER;
        let table = TableRef::new("vala.bifrost".to_string(), "events".to_string());
        let day = EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap());
        let seal_key = SealKey::new(tenant, table, day);

        let path = seal_key.as_path_components();
        assert!(path.contains("vala.bifrost"));
        assert!(path.contains("events"));
        assert!(path.contains("2026-07-14"));
    }

    #[test]
    #[ignore = "split_batch_by_event_day moved to mod.rs"]
    fn split_batch_by_event_day_partitions_correctly() {
        // This test is obsolete - split_batch_by_event_day is now implemented
        // in mod.rs and tested via integration tests in pg_scribe_seal.rs
        // (pg_scribe_cross_day_batch_produces_two_files).
    }
}
