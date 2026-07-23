//! `file_list` writer — atomic INSERT + audit fan-out per CONTRACTS §11.

use uuid::Uuid;
use vala_sql::{SqlError, TenantConn};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::ParquetEncoded;

/// The unique replay key on `vala.file_list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileListConflictKey {
    /// Producing Scribe node.
    pub node_id: Uuid,
    /// Producing Scribe writer epoch.
    pub writer_epoch: i64,
    /// Inclusive lower WAL LSN.
    pub wal_lsn_min: i64,
    /// Inclusive upper WAL LSN.
    pub wal_lsn_max: i64,
}

/// The full identity that must match when a conflict key is replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListCommitKey {
    /// Organization stamped on the physical Parquet file.
    pub data_tenant_id: DataTenantId,
    /// Logical namespace stored in `vala.file_list`.
    pub namespace: String,
    /// Local logical table name.
    pub table_name: String,
    /// Producing Scribe node.
    pub node_id: Uuid,
    /// Producing Scribe writer epoch.
    pub writer_epoch: i64,
    /// Inclusive lower WAL LSN.
    pub wal_lsn_min: i64,
    /// Inclusive upper WAL LSN.
    pub wal_lsn_max: i64,
}

/// Result of a file-list write or an already-validated replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListInsertOutcome {
    /// Durable `vala.file_list` row identity.
    pub id: Uuid,
    /// Full identity validated against the durable row.
    pub commit_key: FileListCommitKey,
    /// Whether the unique replay key already existed.
    pub replayed: bool,
}

/// File-list INSERT row matching the `vala.file_list` columns.
pub struct FileListInsert<'a> {
    pub id: Uuid,
    pub data_tenant_id: DataTenantId,
    pub namespace: &'a str,
    pub table_name: &'a str,
    pub file_path: &'a str,
    pub file_size: i64,
    pub row_count: i64,
    pub min_event_time: chrono::DateTime<chrono::Utc>,
    pub max_event_time: chrono::DateTime<chrono::Utc>,
    pub partition_day: chrono::NaiveDate,
    pub node_id: Uuid,
    pub writer_epoch: i64,
    pub wal_lsn_min: i64,
    pub wal_lsn_max: i64,
}

impl FileListInsert<'_> {
    /// Return the full commit identity for this row.
    #[must_use]
    pub fn commit_key(&self) -> FileListCommitKey {
        FileListCommitKey {
            data_tenant_id: self.data_tenant_id,
            namespace: self.namespace.to_owned(),
            table_name: self.table_name.to_owned(),
            node_id: self.node_id,
            writer_epoch: self.writer_epoch,
            wal_lsn_min: self.wal_lsn_min,
            wal_lsn_max: self.wal_lsn_max,
        }
    }

    /// Return the unique replay key for this row.
    #[must_use]
    pub const fn conflict_key(&self) -> FileListConflictKey {
        FileListConflictKey {
            node_id: self.node_id,
            writer_epoch: self.writer_epoch,
            wal_lsn_min: self.wal_lsn_min,
            wal_lsn_max: self.wal_lsn_max,
        }
    }
}

/// Extract LSN range from append metadata.
///
/// LSN values are u64 but real-world Postgres LSNs fit in i64 (PG column is BIGINT).
fn extract_lsn_range(encoded: &ParquetEncoded) -> Result<(i64, i64), ScribeError> {
    let min = encoded
        .append_metas
        .iter()
        .map(|m| m.wal_lsn_min.as_u64())
        .min()
        .unwrap_or(0);
    let max = encoded
        .append_metas
        .iter()
        .map(|m| m.wal_lsn_max.as_u64())
        .max()
        .unwrap_or(0);

    let min_i = i64::try_from(min).map_err(|_| ScribeError::Internal {
        detail: format!("wal_lsn_min {min} exceeds i64::MAX (invariant violation)"),
    })?;
    let max_i = i64::try_from(max).map_err(|_| ScribeError::Internal {
        detail: format!("wal_lsn_max {max} exceeds i64::MAX (invariant violation)"),
    })?;

    Ok((min_i, max_i))
}

/// Extract min/max event time from row group stats.
fn extract_event_time_range(
    encoded: &ParquetEncoded,
) -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let epoch = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
    match encoded.row_group_stats.first() {
        Some(rg) => (
            rg.min_event_time
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros)
                .unwrap_or(epoch),
            rg.max_event_time
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros)
                .unwrap_or(epoch),
        ),
        None => (epoch, epoch),
    }
}

/// Build the `FileListInsert` row from freeze metadata and its canonical binding.
pub fn build_insert<'a>(
    frozen: &'a FrozenMemtable,
    encoded: &'a ParquetEncoded,
    binding: &'a TenantTableBinding,
    node_id: &str,
    writer_epoch: i64,
    file_path: &'a str,
) -> Result<FileListInsert<'a>, ScribeError> {
    if binding.tenant != frozen.seal_key.tenant || binding.table_ref != frozen.seal_key.table {
        return Err(ScribeError::Internal {
            detail: format!(
                "tenant-table binding does not match seal key: binding tenant/table=({},{}) seal tenant/table=({},{})",
                binding.tenant, binding.table_ref, frozen.seal_key.tenant, frozen.seal_key.table
            ),
        });
    }

    let (wal_lsn_min, wal_lsn_max) = extract_lsn_range(encoded)?;
    let (min_event_time, max_event_time) = extract_event_time_range(encoded);

    // Row count and file size are usize; the Postgres columns are BIGINT (i64).
    // A single Parquet file cannot approach i64::MAX rows or bytes on any real
    // machine, so a failed conversion is an invariant violation.
    let row_count = i64::try_from(frozen.batch.num_rows()).map_err(|_| ScribeError::Internal {
        detail: "row_count exceeds i64::MAX (invariant violation)".to_string(),
    })?;
    let file_size = i64::try_from(encoded.bytes.len()).map_err(|_| ScribeError::Internal {
        detail: "file_size exceeds i64::MAX (invariant violation)".to_string(),
    })?;

    let node_uuid = Uuid::parse_str(node_id).map_err(|e| ScribeError::Internal {
        detail: format!("invalid node_id UUID: {e}"),
    })?;

    Ok(FileListInsert {
        id: Uuid::now_v7(),
        data_tenant_id: binding.tenant,
        namespace: &binding.logical_namespace,
        table_name: &binding.table_name,
        file_path,
        file_size,
        row_count,
        min_event_time,
        max_event_time,
        partition_day: encoded.partition_day.as_naive_date(),
        node_id: node_uuid,
        writer_epoch,
        wal_lsn_min,
        wal_lsn_max,
    })
}

/// Validate and return the row already stored for a replay conflict.
async fn validate_replay(
    conn: &mut TenantConn<'_>,
    row: &FileListInsert<'_>,
    commit_key: FileListCommitKey,
) -> Result<FileListInsertOutcome, SqlError> {
    let conflict = row.conflict_key();
    let existing: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        r"
 SELECT id, data_tenant_id, namespace, table_name
   FROM vala.file_list
  WHERE node_id = $1
    AND writer_epoch = $2
    AND wal_lsn_min = $3
    AND wal_lsn_max = $4
  FOR UPDATE
 ",
    )
    .bind(conflict.node_id)
    .bind(conflict.writer_epoch)
    .bind(conflict.wal_lsn_min)
    .bind(conflict.wal_lsn_max)
    .fetch_optional(&mut **conn.transaction())
    .await?;

    let Some((id, data_tenant_id, namespace, table_name)) = existing else {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list replay conflict has no RLS-visible row for stream {}:{}:{}-{}",
                conflict.node_id, conflict.writer_epoch, conflict.wal_lsn_min, conflict.wal_lsn_max
            ),
        });
    };

    if data_tenant_id != row.data_tenant_id.as_uuid()
        || namespace != row.namespace
        || table_name != row.table_name
    {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list replay conflict identity mismatch: expected ({},{},{}) found ({},{},{})",
                row.data_tenant_id,
                row.namespace,
                row.table_name,
                data_tenant_id,
                namespace,
                table_name
            ),
        });
    }

    tracing::debug!(
        node_id = %row.node_id,
        writer_epoch = row.writer_epoch,
        wal_lsn_min = row.wal_lsn_min,
        wal_lsn_max = row.wal_lsn_max,
        "replay-driven re-seal: validated existing file_list row; skipping audit fan-out",
    );
    Ok(FileListInsertOutcome {
        id,
        commit_key,
        replayed: true,
    })
}

/// Insert one `vala.file_list` row + N `vala.audit_outbox` rows in one transaction.
///
/// The insert uses `FileListConflictKey` for replay detection. A conflict is
/// successful only after the existing row's complete `FileListCommitKey` is
/// visible under the same RLS-bound transaction and matches the attempted row.
///
/// # Errors
/// Returns [`vala_sql::SqlError`] on transaction failure or an unvalidated replay.
pub async fn insert_and_audit(
    conn: &mut TenantConn<'_>,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
) -> Result<FileListInsertOutcome, SqlError> {
    if row.data_tenant_id != conn.data_tenant_id() {
        return Err(SqlError::InvariantViolation {
            detail: format!(
                "file_list tenant mismatch: row tenant `{}` does not match TenantConn tenant `{}`",
                row.data_tenant_id,
                conn.data_tenant_id()
            ),
        });
    }

    let result = sqlx::query(
        r"
 INSERT INTO vala.file_list (
 id,
 data_tenant_id,
 namespace,
 table_name,
 file_path,
 file_size,
 row_count,
 min_event_time,
 max_event_time,
 partition_day,
 node_id,
 writer_epoch,
 wal_lsn_min,
 wal_lsn_max
 )
 VALUES (
 $1,
 $2,
 $3,
 $4,
 $5,
 $6,
 $7,
 $8,
 $9,
 $10,
 $11,
 $12,
 $13,
 $14
 )
 ON CONFLICT (data_tenant_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) DO NOTHING
 ",
    )
    .bind(row.id)
    .bind(row.data_tenant_id.as_uuid())
    .bind(row.namespace)
    .bind(row.table_name)
    .bind(row.file_path)
    .bind(row.file_size)
    .bind(row.row_count)
    .bind(row.min_event_time)
    .bind(row.max_event_time)
    .bind(row.partition_day)
    .bind(row.node_id)
    .bind(row.writer_epoch)
    .bind(row.wal_lsn_min)
    .bind(row.wal_lsn_max)
    .execute(&mut **conn.transaction())
    .await?;

    let commit_key = row.commit_key();
    if result.rows_affected() == 0 {
        return validate_replay(conn, row, commit_key).await;
    }

    for event in events {
        vala_sql::queries::audit_outbox::append_audit(conn, event).await?;
    }

    Ok(FileListInsertOutcome {
        id: row.id,
        commit_key,
        replayed: false,
    })
}
