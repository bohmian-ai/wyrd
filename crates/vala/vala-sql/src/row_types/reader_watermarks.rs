//! Row mirror for the snapshots live Bifrost readers still depend on.

use chrono::{DateTime, Utc};

use crate::SqlError;
use crate::row_types::forge_tasks::SnapshotWatermark;

/// One table a reader node currently depends on, and the snapshot it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderWatermarkEntry {
    /// Bifrost namespace owning the table.
    pub namespace: String,
    /// Physical table name inside that namespace.
    pub table_name: String,
    /// Oldest snapshot this node still reads for the table.
    pub watermark: SnapshotWatermark,
}

/// One reader node's complete current dependency set, as published.
///
/// The set is whole rather than incremental because a reader releasing its last
/// cut on a table has to be able to say so: an incremental publication cannot
/// distinguish "no longer needed" from "not mentioned this round", and the
/// difference is a table that never becomes maintainable again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderWatermarkPublication {
    /// Every table this node currently pins, in any order.
    pub entries: Vec<ReaderWatermarkEntry>,
    /// Time the publishing node observed this dependency set.
    pub published_at: DateTime<Utc>,
    /// Time after which this publication no longer protects anything.
    pub expires_at: DateTime<Utc>,
}

impl ReaderWatermarkPublication {
    /// Validates identity, timestamp, and liveness shape before any statement.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the publication does not expire
    /// after it was published, when a table identity is blank, when a snapshot
    /// identifier is not positive, when a snapshot timestamp is negative, or
    /// when one table appears twice.
    pub fn validate(&self) -> Result<(), SqlError> {
        let conflict = |detail: &str| SqlError::Conflict {
            detail: detail.to_owned(),
        };
        if self.expires_at <= self.published_at {
            return Err(conflict("reader watermark publication expires immediately"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.entries {
            if entry.namespace.trim().is_empty() || entry.table_name.trim().is_empty() {
                return Err(conflict("reader watermark names a blank table identity"));
            }
            if entry.watermark.snapshot_id <= 0 {
                return Err(conflict("reader watermark snapshot id must be positive"));
            }
            entry.watermark.validate()?;
            if !seen.insert((entry.namespace.as_str(), entry.table_name.as_str())) {
                return Err(conflict("reader watermark publication repeats a table"));
            }
        }
        Ok(())
    }

    /// Projects the entries into the four parallel arrays the statements bind.
    ///
    /// Postgres `unnest` over parallel arrays is what lets one publication be
    /// one round trip regardless of how many tables a reader currently pins.
    #[must_use]
    pub fn columns(&self) -> (Vec<String>, Vec<String>, Vec<i64>, Vec<i64>) {
        let mut namespaces = Vec::with_capacity(self.entries.len());
        let mut tables = Vec::with_capacity(self.entries.len());
        let mut snapshot_ids = Vec::with_capacity(self.entries.len());
        let mut timestamps = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            namespaces.push(entry.namespace.clone());
            tables.push(entry.table_name.clone());
            snapshot_ids.push(entry.watermark.snapshot_id);
            timestamps.push(entry.watermark.timestamp_ms);
        }
        (namespaces, tables, snapshot_ids, timestamps)
    }
}
