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

    let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
        .await
        .map_err(BifrostError::Sql)?;

    let existing = vala_sql::queries::olap_catalog::lookup_idempotent(
        &mut conn,
        table_uid.as_bytes(),
        &batch_id,
    )
    .await
    .map_err(BifrostError::Sql)?;

    match existing {
        Some(row) if row.state == "committed" => {
            // Idempotent replay: this batch already committed. Return the prior
            // snapshot and no fresh table — the caller keeps its current snapshot.
            // A committed row always has a snapshot_id (finalize_committed sets it);
            // a NULL here is control-table corruption, not a 0-snapshot success.
            conn.commit().await.map_err(BifrostError::Sql)?;
            let snapshot_id = row.snapshot_id.ok_or_else(|| {
                BifrostError::MetadataMismatch(format!(
                    "committed olap_commits row for {table_fqn} has NULL snapshot_id"
                ))
            })?;
            return Ok((snapshot_id, None));
        }
        Some(row) if row.state == "failed" => {
            conn.commit().await.map_err(BifrostError::Sql)?;
            return Err(BifrostError::DuplicateFailedBatch(
                uuid::Uuid::from_bytes(batch_id).to_string(),
            ));
        }
        Some(_) => {
            // A precommit row means this batch_id is currently being written by
            // another actor. Return CommitConflict; the caller must retry with a
            // new batch_id.
            conn.commit().await.map_err(BifrostError::Sql)?;
            return Err(BifrostError::CommitConflict(table_fqn));
        }
        None => {}
    }

    vala_sql::queries::olap_catalog::precommit(&mut conn, table_uid.as_bytes(), &batch_id)
        .await
        .map_err(BifrostError::Sql)?;

    conn.commit().await.map_err(BifrostError::Sql)?;

    let data_files_result = write_batches(table, batches).await;

    let (snapshot_id, updated_table) = match data_files_result {
        Err(e) => {
            let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            let code = e.to_string();
            let _ = vala_sql::queries::olap_catalog::finalize_failed(
                &mut conn,
                table_uid.as_bytes(),
                &batch_id,
                "WYRD_VALA_500_BIFROST_INTERNAL",
                &code,
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
        &batch_id,
        snapshot_id,
    )
    .await
    .map_err(BifrostError::Sql)?;

    conn.commit().await.map_err(BifrostError::Sql)?;

    Ok((snapshot_id, Some(updated_table)))
}

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
