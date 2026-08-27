//! Staged compaction over files a real Scribe seal produced.
//!
//! Module of the `forge` group; the supervisor lifecycle lives in `support.rs`.

use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::basic::Encoding;
use std::collections::BTreeSet;
use wyrd_testing::bifrost::seed_forge_group;

use super::support::{SupervisedForge, start_engine_fixture_server};

/// One durable `vala.file_list` row as Scribe wrote it.
#[derive(Debug)]
struct SealedFile {
    /// Object path Scribe chose for the sealed file.
    path: String,
    /// File size Scribe measured from the object it encoded.
    size: i64,
    /// Row count Scribe measured from the batch it encoded.
    rows: i64,
}

/// Read every `file_list` row of the fixture table in a stable order.
async fn sealed_files(fixture: &wyrd_testing::bifrost::ForgeFixture) -> Vec<SealedFile> {
    sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT file_path, file_size, row_count FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
         ORDER BY file_path",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("sealed file-list rows")
    .into_iter()
    .map(|(path, size, rows)| SealedFile { path, size, rows })
    .collect()
}

/// Collect the live data-file paths of the table's current Iceberg snapshot.
async fn current_live_paths(table: &iceberg::table::Table) -> BTreeSet<String> {
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("compacted table snapshot");
    let manifest_list = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("current manifest list");
    let mut paths = BTreeSet::new();
    for manifest_file in manifest_list.entries() {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("current manifest");
        paths.extend(
            manifest
                .entries()
                .iter()
                .filter(|entry| entry.is_alive())
                .map(|entry| entry.file_path().to_owned()),
        );
    }
    paths
}

/// Staged compaction folds real Scribe-sealed files into one live Iceberg data
/// file, preserving every row and the writer recipe's per-column encoding.
///
/// This is the fixture-parity proof as much as the Forge proof: every input the
/// scheduler plans over was published by a real Scribe seal, so the sizes, row
/// counts, LSN ranges, and partitions it reasons about are values production
/// would actually produce. The assertions below check each durable row against
/// the object it names before Forge ever sees it, which is exactly what a
/// fabricated fixture row could not survive.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn staged_compaction_folds_scribe_sealed_files() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "forge_staged_compaction").await;

    let before = sealed_files(&fixture).await;
    assert_eq!(
        before.len(),
        2,
        "a real seal per fixture append: {before:?}"
    );
    let mut expected_rows = 0_i64;
    for file in &before {
        let object = fixture
            .staging
            .read(&file.path)
            .await
            .expect("Scribe-sealed object exists at the path its row names");
        assert_eq!(
            i64::try_from(object.len()).expect("object length"),
            file.size,
            "file_list size must be the size Scribe measured, not a fixture guess"
        );
        assert!(file.rows > 0, "sealed file carries rows: {file:?}");
        expected_rows += file.rows;
    }

    let mut config = fixture.config.clone();
    config.max_files_per_bin = 2;
    config.max_files_per_tick = 2;
    let mut lifecycle = SupervisedForge::start_default(&fixture, config);
    lifecycle.run_one_success().await;
    lifecycle.shutdown().await;

    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1,
        "exactly one staged compaction commits"
    );
    assert!(
        sealed_files(&fixture).await.is_empty(),
        "a committed staged compaction consumes its file-list inputs"
    );

    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("compacted table");
    let live = current_live_paths(&table).await;
    assert_eq!(live.len(), 1, "two sealed files fold into one: {live:?}");

    let catalog_path = live.iter().next().expect("one live path");
    let object_path = catalog_path
        .find(&fixture.binding.object_prefix)
        .map(|offset| &catalog_path[offset..])
        .expect("live catalog path belongs to the fixture table");
    let bytes = fixture
        .staging
        .read(object_path)
        .await
        .expect("compacted object")
        .to_bytes();
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes).expect("compacted reader");
    let metadata = builder.metadata().clone();
    let mut observed_rows = 0_i64;
    for batch in builder.build().expect("compacted batch reader") {
        observed_rows +=
            i64::try_from(batch.expect("compacted batch").num_rows()).expect("batch rows");
    }
    assert_eq!(
        observed_rows, expected_rows,
        "compaction preserves every sealed row"
    );

    let schema = metadata.file_metadata().schema_descr();
    let event_time = (0..schema.num_columns())
        .find(|index| {
            schema.column(*index).name() == wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME
        })
        .expect("compacted file carries the managed event-time column");
    for group in metadata.row_groups() {
        let encodings = group
            .column(event_time)
            .encodings()
            .collect::<Vec<Encoding>>();
        assert!(
            encodings.contains(&Encoding::DELTA_BINARY_PACKED),
            "the writer recipe keeps wyrd_event_time delta-packed through compaction: {encodings:?}"
        );
        assert!(
            !encodings.contains(&Encoding::RLE_DICTIONARY)
                && !encodings.contains(&Encoding::PLAIN_DICTIONARY),
            "wyrd_event_time is the one column the recipe excludes from dictionary encoding: {encodings:?}"
        );
    }
}
