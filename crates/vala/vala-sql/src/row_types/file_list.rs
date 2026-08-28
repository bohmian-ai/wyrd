//! Typed rows returned from the tenant-scoped `vala.file_list` manifest.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// One validated sealed Parquet manifest entry.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HotFileRow {
    /// Durable file-list identity.
    pub id: Uuid,
    /// Authenticated owning tenant.
    pub data_tenant_id: Uuid,
    /// Logical table namespace.
    pub namespace: String,
    /// Logical table name.
    pub table_name: String,
    /// Canonical object-store path.
    pub file_path: String,
    /// Zero-based position within the producing generation's artifact set.
    pub file_ordinal: i16,
    /// Lowercase SHA-256 object checksum for writer-v2 rows.
    pub file_checksum: Option<String>,
    /// Encoded file size.
    pub file_size: i64,
    /// Number of rows in the file.
    pub row_count: i64,
    /// Partition granularity token: `hour` or `day`.
    pub partition_granularity: String,
    /// Exact UTC start boundary of the partition.
    pub partition_start: DateTime<Utc>,
    /// Whether Forge has published the file into Iceberg.
    pub compacted: bool,
    /// Iceberg snapshot that contains this file, when published.
    pub committed_snapshot_id: Option<i64>,
    /// Stable Forge operation that prepared and published this input.
    pub forge_publication_operation_id: Option<Uuid>,
    /// Producing Scribe node.
    pub node_id: Uuid,
    /// Producing writer epoch.
    pub writer_epoch: i64,
    /// Inclusive WAL lower bound.
    pub wal_lsn_min: i64,
    /// Inclusive WAL upper bound.
    pub wal_lsn_max: i64,
    /// Manifest insertion time.
    pub created_at: DateTime<Utc>,
}
