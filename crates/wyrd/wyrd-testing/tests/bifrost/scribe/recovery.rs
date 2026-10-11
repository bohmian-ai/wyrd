//! Staged recovery: restored runs publish once as one final object.

use std::sync::Arc;

use arrow::array::{ArrayRef, AsArray as _, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use uuid::Uuid;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_testing::WyrdTestServer;

use super::support::{
    append_batch, await_persistence_drained, published_object_count, published_rows, tenant_client,
    unique_table,
};

/// Builds one ingest batch of `value` ids and their Variant `v` documents.
///
/// # Panics
///
/// Panics when a document is not JSON or the batch cannot be assembled.
fn variant_batch(rows: &[(i64, &str)]) -> RecordBatch {
    let json: ArrayRef = Arc::new(StringArray::from(
        rows.iter().map(|(_, json)| *json).collect::<Vec<_>>(),
    ));
    let variant: ArrayRef = parquet_variant_compute::json_to_variant(&json)
        .expect("documents are JSON")
        .into();
    let variant = arrow::compute::cast(&variant, &wyrd_types::variant::variant_storage_type())
        .expect("Variant storage casts to the registered shape");
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false),
            wyrd_types::variant::variant_field("v", true),
        ])),
        vec![
            Arc::new(Int64Array::from(
                rows.iter().map(|(value, _)| *value).collect::<Vec<_>>(),
            )),
            variant,
        ],
    )
    .expect("variant ingest batch")
}

/// Reads every `(value, to_json(v))` pair of `table` through the public query route.
///
/// # Panics
///
/// Panics when the query fails or does not settle successfully.
async fn read_documents(client: &wyrd_client::WyrdClient, table: &str) -> Vec<(i64, String)> {
    let sql = format!("SELECT value, to_json(v) AS v FROM {table} ORDER BY value");
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&wyrd_spec::vala::api::BifrostQueryRequest {
            sql: sql.clone(),
            deadline_ms: Some(120_000),
        })
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` starts: {error}"));
    let mut rows = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` streams: {error}"))
    {
        let values = batch
            .column(0)
            .as_primitive::<arrow::datatypes::Int64Type>();
        let documents =
            arrow::compute::cast(batch.column(1), &DataType::Utf8).expect("to_json renders text");
        let documents = documents.as_string::<i32>();
        rows.extend(
            (0..batch.num_rows()).map(|row| (values.value(row), documents.value(row).to_owned())),
        );
    }
    assert_eq!(
        stream.terminal().map(|terminal| terminal.outcome),
        Some(wyrd_spec::vala::api::QueryTerminalOutcome::Success),
        "an authority read of `{sql}` must see every source"
    );
    rows
}

/// Lists the `.wal` segment files under `root`.
///
/// # Panics
///
/// Panics when the WAL tree cannot be read.
fn wal_segments(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut segments = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("the WAL tree is readable") {
            let path = entry.expect("a WAL entry is readable").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "wal") {
                segments.push(path);
            }
        }
    }
    segments
}

/// Waits until every acknowledged WAL segment has been retired from disk.
///
/// Staged generations are retirement-eligible at once, but the shard retires
/// them on its lifecycle tick and deletes a segment only once it is closed, so
/// this polls the production outcome rather than assuming a schedule.
///
/// # Panics
///
/// Panics when any segment remains after the case deadline.
async fn await_wal_retired(acknowledged: &[std::path::PathBuf]) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let retained: Vec<_> = acknowledged.iter().filter(|path| path.exists()).collect();
        if retained.is_empty() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "staged durability retires the WAL that backed the run: {retained:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Staged Variant runs restored after a stop publish once as one final object.
///
/// Two staged runs hold Variant documents that alone would infer different
/// layouts (`{a: int}` and `{b: string}`). Each run is staged durably and the
/// WAL that backed it is retired, so after the stop the staged runs are the
/// only copy. The pod stops before publication and restarts on its retained
/// root; the restored runs merge under their one unshredded schema and publish
/// exactly once as one final object, and every document reads back unchanged.
///
/// # Panics
///
/// Panics when the server cannot start or restart, an append or read fails,
/// the WAL is not retired after staging, anything publishes before the stop,
/// or the restored rows do not publish once as one object with their values.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn variant_staging_restores_and_publishes_once() {
    let data_root = tempfile::tempdir().expect("durable Bifrost data root");
    let builder =
        || WyrdTestServer::builder().with_durable_bifrost_data_root(data_root.path().to_path_buf());
    let server = builder()
        .start_bound()
        .await
        .expect("the pod starts on a durable data root");
    let tenant = server.data_tenant_id();
    let name = unique_table("variant_staging");
    server
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Datasets, &name),
            user_fields: vec![
                Field::new("value", DataType::Int64, false),
                wyrd_types::variant::variant_field("v", true),
            ],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("the catalog registers the Variant table");
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
    let client = tenant_client(&server, tenant).await;
    let wal_root = server
        .scribe_wal_root_for_test()
        .expect("the pod composes a WAL")
        .to_path_buf();
    let scribe = server.bifrost_scribe().expect("the pod owns a Scribe");

    let runs: [&[(i64, &str)]; 2] = [
        &[(1, r#"{"a":1}"#), (2, r#"{"a":2}"#)],
        &[(3, r#"{"b":"x"}"#), (4, r#"{"b":"y"}"#)],
    ];
    for run in runs {
        append_batch(&client, &table, Uuid::now_v7(), &variant_batch(run))
            .await
            .expect("the Variant append is acknowledged");
        let acknowledged = wal_segments(&wal_root);
        assert!(
            !acknowledged.is_empty(),
            "the acknowledgement is WAL-backed"
        );
        scribe
            .flush_writable_for_test()
            .await
            .expect("every writable bucket freezes");
        await_persistence_drained(&scribe).await;
        await_wal_retired(&acknowledged).await;
    }
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        0,
        "nothing publishes before the stop"
    );
    drop(scribe);

    let server = server
        .restart_bound(builder())
        .await
        .expect("the pod restarts on its retained data root");
    let client = tenant_client(&server, tenant).await;
    server
        .flush_bifrost()
        .await
        .expect("the restored runs publish");
    assert_eq!(
        published_object_count(&server, tenant, BifrostNamespace::Datasets, &name).await,
        1,
        "the restored runs publish as one final object"
    );
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        4,
        "every row publishes exactly once"
    );
    assert_eq!(
        read_documents(&client, &table).await,
        runs.iter()
            .flat_map(|run| run.iter())
            .map(|(value, json)| (*value, (*json).to_owned()))
            .collect::<Vec<_>>(),
        "every Variant document reads back once and unchanged"
    );

    server.shutdown().await.expect("the server drains cleanly");
}
