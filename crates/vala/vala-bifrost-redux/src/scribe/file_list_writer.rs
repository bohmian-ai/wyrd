//! `file_list` writer — atomic INSERT + audit fan-out per CONTRACTS §11.

use sha2::{Digest, Sha256};
use uuid::Uuid;
use vala_sql::TenantConn;

use crate::contracts::ScribeError;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::ParquetEncoded;

/// Metadata extracted from encoded Parquet for the `file_list` INSERT.
struct FileListMetadata {
    wal_lsn_min: i64,
    wal_lsn_max: i64,
    min_event_time: chrono::DateTime<chrono::Utc>,
    max_event_time: chrono::DateTime<chrono::Utc>,
    row_count: i64,
    file_size: i64,
    tenant_bucket: i32,
    node_uuid: Uuid,
}

/// Extract LSN range from append metadata.
///
/// LSN values are u64 but real-world Postgres LSNs fit in i64 (PG column is BIGINT).
fn extract_lsn_range(encoded: &ParquetEncoded) -> (i64, i64) {
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

    (min.cast_signed(), max.cast_signed())
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
    (bucket_u32 % 1024).cast_signed()
}

/// Gather all metadata needed for the `file_list` INSERT.
fn extract_file_list_metadata(
    frozen: &FrozenMemtable,
    encoded: &ParquetEncoded,
    node_id: &str,
) -> Result<FileListMetadata, ScribeError> {
    let (wal_lsn_min, wal_lsn_max) = extract_lsn_range(encoded);
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

    Ok(FileListMetadata {
        wal_lsn_min,
        wal_lsn_max,
        min_event_time,
        max_event_time,
        row_count,
        file_size,
        tenant_bucket,
        node_uuid,
    })
}

/// Insert one `vala.file_list` row + N `vala.audit_outbox` rows in one transaction.
///
/// Executes `INSERT vala.file_list` (stamped with `node_id`, `writer_epoch`,
/// `wal_lsn_min`, `wal_lsn_max`, `partition_day`) + one
/// `vala_sql::queries::audit_outbox::append_audit(&mut conn, &event)` call per
/// staged `wyrd_spec::vala::api::AuditEvent` within a single `TenantConn`
/// transaction.
///
/// # Errors
/// Returns [`ScribeError::Internal`] wrapping `vala_sql::SqlError` on transaction failure.
pub async fn insert_and_audit(
    conn: &mut TenantConn<'_>,
    frozen: &FrozenMemtable,
    encoded: &ParquetEncoded,
    file_path: &str,
    node_id: &str,
    writer_epoch: i64,
) -> Result<(), ScribeError> {
    let meta = extract_file_list_metadata(frozen, encoded, node_id)?;

    // 1. INSERT vala.file_list
    sqlx::query(
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
    .bind(Uuid::now_v7())
    .bind(&frozen.seal_key.table.namespace)
    .bind(&frozen.seal_key.table.name)
    .bind(file_path)
    .bind(meta.file_size)
    .bind(meta.row_count)
    .bind(meta.min_event_time)
    .bind(meta.max_event_time)
    .bind(encoded.partition_day.as_naive_date())
    .bind(meta.tenant_bucket)
    .bind(meta.node_uuid)
    .bind(writer_epoch)
    .bind(meta.wal_lsn_min)
    .bind(meta.wal_lsn_max)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|e| ScribeError::Internal {
        detail: format!("file_list INSERT failed: {e}"),
    })?;

    // 2. Append audit events (one per staged AuditEvent)
    for event in &encoded.audit_events {
        vala_sql::queries::audit_outbox::append_audit(conn, event)
            .await
            .map_err(|e| ScribeError::Internal {
                detail: format!("audit append failed: {e}"),
            })?;
    }

    Ok(())
}
