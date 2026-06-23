use std::sync::Arc;

use arrow::array::RecordBatch;
use iceberg::spec::DataFile;
use iceberg::table::Table;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use iceberg::writer::base_writer::data_file_writer::DataFileWriterBuilder;
use iceberg::writer::file_writer::ParquetWriterBuilder;
use iceberg::writer::file_writer::location_generator::{
    DefaultFileNameGenerator, DefaultLocationGenerator,
};
use iceberg::writer::file_writer::rolling_writer::RollingFileWriterBuilder;
use iceberg::writer::partitioning::unpartitioned_writer::UnpartitionedWriter;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;
use wyrd_spec::ids::DataTenantId;

use crate::error::BifrostError;
use crate::types::TableUid;
use crate::writer::file_writer::bifrost_writer_properties;

/// Execute one 2PC commit of `batches` under `batch_id` for `table`.
///
/// Two phases, each with a clear short-circuit:
/// 1. [`claim_batch`] inspects the prior 2PC anchor. An already-committed batch
///    replays its recorded snapshot with no write — returning `(snapshot, None)`
///    so the caller keeps its current table ref. A prior failure or an in-flight
///    precommit is rejected.
/// 2. [`write_and_finalize`] writes Parquet, fast-appends to Iceberg, and records
///    the terminal FSM transition (`committed`, or `failed` on write error).
///
/// On a fresh write the returned [`Table`] is the post-append snapshot the caller
/// must adopt for its next commit; on replay it is `None`.
///
/// # Errors
/// Returns [`BifrostError`] when the control-plane txn, Parquet write, or Iceberg
/// append fails, or when the `batch_id` collides with a failed/in-flight anchor.
pub async fn run_commit(
    pool: &PgPool,
    catalog: &SqlCatalog,
    table: &Table,
    table_uid: &TableUid,
    batches: Vec<RecordBatch>,
    batch_id: [u8; 16],
    tenant: DataTenantId,
) -> Result<(i64, Option<Table>), BifrostError> {
    let table_fqn = table.identifier().to_string();

    if let Some(snapshot_id) = claim_batch(pool, table_uid, &batch_id, tenant, &table_fqn).await? {
        return Ok((snapshot_id, None));
    }

    let (snapshot_id, updated_table) =
        write_and_finalize(pool, catalog, table, table_uid, batches, &batch_id, tenant).await?;

    Ok((snapshot_id, Some(updated_table)))
}

/// Phase 1 of [`run_commit`]: dispatch on the prior 2PC anchor, then claim the
/// batch for a fresh write.
///
/// - `Ok(Some(snapshot_id))` — the batch already committed; the caller must
///   replay this snapshot and write nothing.
/// - `Ok(None)` — no prior anchor; a `precommit` row has been written and the
///   caller should proceed with the write.
/// - `Err(..)` — the `batch_id` collides with a prior failure
///   ([`BifrostError::DuplicateFailedBatch`]) or an in-flight precommit
///   ([`BifrostError::CommitConflict`]); the caller must retry with a new id.
///
/// # Errors
/// Returns [`BifrostError::Sql`] on control-plane failure,
/// [`BifrostError::MetadataMismatch`] when a `committed` row has a NULL
/// `snapshot_id` (control-table corruption, not a 0-snapshot success), or the
/// collision errors above.
async fn claim_batch(
    pool: &PgPool,
    table_uid: &TableUid,
    batch_id: &[u8; 16],
    tenant: DataTenantId,
    table_fqn: &str,
) -> Result<Option<i64>, BifrostError> {
    let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
        .await
        .map_err(BifrostError::Sql)?;

    let existing = vala_sql::queries::olap_catalog::lookup_idempotent(
        &mut conn,
        table_uid.as_bytes(),
        batch_id,
    )
    .await
    .map_err(BifrostError::Sql)?;

    match existing {
        Some(row) if row.state == "committed" => {
            conn.commit().await.map_err(BifrostError::Sql)?;
            let snapshot_id = row.snapshot_id.ok_or_else(|| {
                BifrostError::MetadataMismatch(format!(
                    "committed olap_commits row for {table_fqn} has NULL snapshot_id"
                ))
            })?;
            Ok(Some(snapshot_id))
        }
        Some(row) if row.state == "failed" => {
            conn.commit().await.map_err(BifrostError::Sql)?;
            Err(BifrostError::DuplicateFailedBatch(
                uuid::Uuid::from_bytes(*batch_id).to_string(),
            ))
        }
        Some(_) => {
            conn.commit().await.map_err(BifrostError::Sql)?;
            Err(BifrostError::CommitConflict(table_fqn.to_string()))
        }
        None => {
            vala_sql::queries::olap_catalog::precommit(&mut conn, table_uid.as_bytes(), batch_id)
                .await
                .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;
            Ok(None)
        }
    }
}

/// Phase 2 of [`run_commit`]: write the claimed `batches` and record the terminal
/// FSM transition.
///
/// Writes Parquet and fast-appends to Iceberg. On success the anchor moves to
/// `committed` with the discovered snapshot id and the post-append [`Table`] is
/// returned. On write failure the anchor moves to `failed` (best-effort — the
/// original error is preserved and returned).
///
/// # Errors
/// Returns the underlying [`BifrostError`] from the Parquet write or Iceberg
/// append, or [`BifrostError::Sql`] when the `committed` finalize fails.
async fn write_and_finalize(
    pool: &PgPool,
    catalog: &SqlCatalog,
    table: &Table,
    table_uid: &TableUid,
    batches: Vec<RecordBatch>,
    batch_id: &[u8; 16],
    tenant: DataTenantId,
) -> Result<(i64, Table), BifrostError> {
    let (snapshot_id, updated_table) = match write_batches(table, batches).await {
        Err(e) => {
            let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            let _ = vala_sql::queries::olap_catalog::finalize_failed(
                &mut conn,
                table_uid.as_bytes(),
                batch_id,
                "WYRD_VALA_500_BIFROST_INTERNAL",
                &e.to_string(),
            )
            .await;
            let _ = conn.commit().await;
            return Err(e);
        }
        Ok(files) => commit_to_iceberg(catalog, table, files).await?,
    };

    let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
        .await
        .map_err(BifrostError::Sql)?;

    vala_sql::queries::olap_catalog::finalize_committed(
        &mut conn,
        table_uid.as_bytes(),
        batch_id,
        snapshot_id,
    )
    .await
    .map_err(BifrostError::Sql)?;

    conn.commit().await.map_err(BifrostError::Sql)?;

    Ok((snapshot_id, updated_table))
}

/// Write `batches` to Parquet data files under `table`, returning the resulting
/// [`DataFile`] descriptors for the Iceberg append.
///
/// Casts each batch to the authoritative Arrow schema derived from the table's
/// Iceberg schema (which carries the `PARQUET:field_id` metadata and exact column
/// types the Iceberg writer requires) before writing.
///
/// # Errors
/// Returns [`BifrostError`] when schema derivation, a column cast, or the
/// underlying Iceberg/Parquet write fails.
async fn write_batches(
    table: &Table,
    batches: Vec<RecordBatch>,
) -> Result<Vec<DataFile>, BifrostError> {
    let file_io = table.file_io().clone();
    let table_metadata = table.metadata();
    let iceberg_schema = table_metadata.current_schema().clone();

    // The Iceberg writer requires:
    // 1. Arrow field metadata contains `PARQUET:field_id` so the NaN visitor
    //    can match fields by ID (FieldMatchMode::Id).
    // 2. Column types exactly match the Iceberg-derived Arrow schema (e.g.
    //    timestamps must use "+00:00" not "UTC").
    //
    // We derive the authoritative typed Arrow schema from the Iceberg schema
    // (which carries both constraints), then cast each batch's columns to
    // that schema before writing.
    let typed_schema = Arc::new(
        iceberg::arrow::schema_to_arrow_schema(&iceberg_schema).map_err(BifrostError::Iceberg)?,
    );

    let location_gen =
        DefaultLocationGenerator::new(table_metadata).map_err(BifrostError::Iceberg)?;
    let file_name_gen = DefaultFileNameGenerator::new(
        "bifrost".to_string(),
        Some(uuid::Uuid::now_v7().simple().to_string()),
        iceberg::spec::DataFileFormat::Parquet,
    );

    let writer_props = bifrost_writer_properties();
    let parquet_builder = ParquetWriterBuilder::new(writer_props, iceberg_schema);

    let rolling_builder = RollingFileWriterBuilder::new_with_default_file_size(
        parquet_builder,
        file_io,
        location_gen,
        file_name_gen,
    );

    let data_file_builder = DataFileWriterBuilder::new(rolling_builder);
    let mut writer = UnpartitionedWriter::new(data_file_builder);

    for batch in batches {
        let cast_columns: Vec<arrow::array::ArrayRef> = batch
            .columns()
            .iter()
            .zip(typed_schema.fields().iter())
            .map(|(col, field)| {
                arrow::compute::cast(col, field.data_type()).map_err(|e| {
                    BifrostError::Internal(format!("cast column {}: {e}", field.name()))
                })
            })
            .collect::<Result<_, _>>()?;
        let typed = RecordBatch::try_new(typed_schema.clone(), cast_columns)
            .map_err(|e| BifrostError::Internal(format!("retype batch: {e}")))?;
        writer.write(typed).await.map_err(BifrostError::Iceberg)?;
    }

    writer.close().await.map_err(BifrostError::Iceberg)
}

/// Fast-append `data_files` to `table` as a single Iceberg transaction and return
/// the new snapshot id alongside the updated [`Table`].
///
/// # Errors
/// Returns [`BifrostError::Iceberg`] when the transaction build or commit fails.
async fn commit_to_iceberg(
    catalog: &SqlCatalog,
    table: &Table,
    data_files: Vec<DataFile>,
) -> Result<(i64, Table), BifrostError> {
    let tx = Transaction::new(table);
    let action = tx.fast_append().add_data_files(data_files);
    let tx = action.apply(tx).map_err(BifrostError::Iceberg)?;
    let committed = tx.commit(catalog).await.map_err(BifrostError::Iceberg)?;
    let snapshot_id = committed.metadata().current_snapshot_id().unwrap_or(0);
    Ok((snapshot_id, committed))
}
