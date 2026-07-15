//! `file_list` writer — atomic INSERT + audit fan-out per CONTRACTS §11.

use sha2::{Digest, Sha256};
use uuid::Uuid;
use vala_sql::{SqlError, TenantConn};
use wyrd_spec::vala::api::AuditEvent;

use crate::contracts::ScribeError;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::ParquetEncoded;

/// File list INSERT row matching the 14 `vala.file_list` columns.
pub struct FileListInsert<'a> {
    pub id: Uuid,
    pub namespace: &'a str,
    pub table_name: &'a str,
    pub file_path: &'a str,
    pub file_size: i64,
    pub row_count: i64,
    pub min_event_time: chrono::DateTime<chrono::Utc>,
    pub max_event_time: chrono::DateTime<chrono::Utc>,
    pub partition_day: chrono::NaiveDate,
    pub tenant_bucket: i32,
    pub node_id: Uuid,
    pub writer_epoch: i64,
    pub wal_lsn_min: i64,
    pub wal_lsn_max: i64,
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

/// Hash the tenant UUID into a bucket (0..1023) for partition pruning.
fn compute_tenant_bucket(frozen: &FrozenMemtable) -> i32 {
    let mut hasher = Sha256::new();
    hasher.update(frozen.seal_key.tenant.as_uuid().as_bytes());
    let hash = hasher.finalize();
    let bucket_u32 = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
    i32::try_from(bucket_u32 % 1024).expect("tenant_bucket in 0..1024 fits in i32")
}

/// Build the `FileListInsert` row from freeze metadata.
pub fn build_insert<'a>(
    frozen: &'a FrozenMemtable,
    encoded: &'a ParquetEncoded,
    node_id: &str,
    writer_epoch: i64,
    file_path: &'a str,
) -> Result<FileListInsert<'a>, ScribeError> {
    let (wal_lsn_min, wal_lsn_max) = extract_lsn_range(encoded)?;
    let (min_event_time, max_event_time) = extract_event_time_range(encoded);

    // Row count and file size are usize; the Postgres columns are BIGINT (i64).
    // A single Parquet file cannot approach i64::MAX rows or bytes on any real
    // machine (i64::MAX bytes = 8 EiB), so try_from failing IS an invariant
    // violation — surface it explicitly instead of a silent `as` wrap.
    let row_count = i64::try_from(frozen.batch.num_rows()).map_err(|_| ScribeError::Internal {
        detail: "row_count exceeds i64::MAX (invariant violation)".to_string(),
    })?;
    let file_size = i64::try_from(encoded.bytes.len()).map_err(|_| ScribeError::Internal {
        detail: "file_size exceeds i64::MAX (invariant violation)".to_string(),
    })?;

    let tenant_bucket = compute_tenant_bucket(frozen);
    let node_uuid = Uuid::parse_str(node_id).map_err(|e| ScribeError::Internal {
        detail: format!("invalid node_id UUID: {e}"),
    })?;

    Ok(FileListInsert {
        id: Uuid::now_v7(),
        namespace: &frozen.seal_key.table.namespace,
        table_name: &frozen.seal_key.table.name,
        file_path,
        file_size,
        row_count,
        min_event_time,
        max_event_time,
        partition_day: encoded.partition_day.as_naive_date(),
        tenant_bucket,
        node_id: node_uuid,
        writer_epoch,
        wal_lsn_min,
        wal_lsn_max,
    })
}

/// Insert one `vala.file_list` row + N `vala.audit_outbox` rows in one transaction.
///
/// Executes `INSERT vala.file_list` with ON CONFLICT DO NOTHING on the
/// `(node_id, writer_epoch, wal_lsn_min, wal_lsn_max)` unique key. If the insert
/// is a no-op (replay-driven re-seal), returns `Ok(())` without emitting audit
/// rows — the audit rows for that range were already durably written in the prior
/// seal.
///
/// # Errors
/// Returns `vala_sql::SqlError` on transaction failure.
pub async fn insert_and_audit(
    conn: &mut TenantConn<'_>,
    row: &FileListInsert<'_>,
    events: &[AuditEvent],
) -> Result<(), SqlError> {
    // 1. INSERT vala.file_list with ON CONFLICT DO NOTHING
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
 tenant_bucket,
 node_id,
 writer_epoch,
 wal_lsn_min,
 wal_lsn_max
 )
 VALUES (
 $1,
 wyrd.current_tenant(),
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
 ON CONFLICT (node_id, writer_epoch, wal_lsn_min, wal_lsn_max) DO NOTHING
 ",
    )
    .bind(row.id)
    .bind(row.namespace)
    .bind(row.table_name)
    .bind(row.file_path)
    .bind(row.file_size)
    .bind(row.row_count)
    .bind(row.min_event_time)
    .bind(row.max_event_time)
    .bind(row.partition_day)
    .bind(row.tenant_bucket)
    .bind(row.node_id)
    .bind(row.writer_epoch)
    .bind(row.wal_lsn_min)
    .bind(row.wal_lsn_max)
    .execute(&mut **conn.transaction())
    .await?;

    // Early return if the row was a replay no-op
    if result.rows_affected() == 0 {
        tracing::debug!(
        node_id = %row.node_id,
        writer_epoch = row.writer_epoch,
        wal_lsn_min = row.wal_lsn_min,
        wal_lsn_max = row.wal_lsn_max,
        "replay-driven re-seal: file_list row already exists; skipping audit fan-out",
        );
        return Ok(());
    }

    // 2. Append audit events (one per staged AuditEvent)
    for event in events {
        vala_sql::queries::audit_outbox::append_audit(conn, event).await?;
    }

    Ok(())
}
