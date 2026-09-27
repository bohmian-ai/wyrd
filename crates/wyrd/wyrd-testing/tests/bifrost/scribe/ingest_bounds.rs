//! Ingest size boundaries, proven end to end on public routes.
//!
//! One wire ceiling and its derived four-times expanded-data ceiling decide
//! every size refusal before WAL. Row count is not a limit, and an accepted
//! request — however wide or however large one row is — stages, publishes, and
//! reads back without a later size refusal.

use std::sync::Arc;

use arrow::array::{ArrayRef, BinaryArray, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::catalog::MAX_PHYSICAL_LEAF_COLUMNS;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::gate::limits::IngestLimits;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::bifrost::{BifrostClientError, TableConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::RegisterOutcome;
use wyrd_testing::WyrdTestServer;

use super::support::{
    append_batch, append_values, await_persistence_drained, published_object_count, read_sql,
    register_table, start_scribe_server, tenant_client, unique_table,
};

/// Stable public refusal for a request above either ingest ceiling.
const PAYLOAD_TOO_LARGE: &str = "WYRD_VALA_413_PAYLOAD_TOO_LARGE";

/// Starts one bound server whose ingest wire ceiling is `wire_bytes`.
///
/// The expanded-data ceiling is derived from it, never configured.
///
/// # Panics
///
/// Panics when the server cannot start with that ceiling.
async fn start_with_wire_ceiling(wire_bytes: usize) -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_scribe_ingest_limits_for_test(IngestLimits {
            max_frame_bytes: wire_bytes,
            ..IngestLimits::default()
        })
        .start_bound()
        .await
        .expect("the Scribe production harness starts with the requested wire ceiling")
}

/// Stages every acknowledged row, reads it, then publishes and reads it again.
///
/// Both reads run `sql` through the public query route and must return
/// `expected`; the published read also proves at least one hot object exists.
///
/// # Panics
///
/// Panics when the freeze, publication, or either read fails or differs.
async fn assert_stages_and_publishes(
    server: &WyrdTestServer,
    client: &wyrd_client::WyrdClient,
    tenant: DataTenantId,
    name: &str,
    sql: &str,
    expected: &[i64],
) {
    let scribe = server.bifrost_scribe().expect("the server owns a Scribe");
    scribe
        .flush_writable_for_test()
        .await
        .expect("every writable bucket freezes");
    await_persistence_drained(&scribe).await;
    assert_eq!(read_sql(client, sql).await, expected, "the staged read");
    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");
    assert!(
        published_object_count(server, tenant, BifrostNamespace::Datasets, name).await > 0,
        "publication commits a hot object"
    );
    assert_eq!(read_sql(client, sql).await, expected, "the published read");
}

/// Registers `fields` as a dataset table through the public client.
///
/// # Errors
///
/// Returns the public registration refusal.
async fn register_public(
    client: &wyrd_client::WyrdClient,
    name: &str,
    fields: Vec<Field>,
) -> Result<RegisterOutcome, BifrostClientError> {
    let config = TableConfig::from_arrow(
        &format!("vala.datasets.{name}"),
        Arc::new(Schema::new(fields)),
    )?;
    wyrd_client::Bifrost::connect_with_table(client, config)
        .await?
        .register()
        .await
}

/// A native request is bounded by expanded bytes, not rows or wire alone.
///
/// At the default 16 MiB wire ceiling (64 MiB expanded), 140,000 tiny rows —
/// more than the retired 131,072-row cap — are acknowledged. A request still
/// within the wire ceiling whose managed-column projection expands past
/// 64 MiB, and a request past the wire ceiling itself, are both refused as
/// payload too large. Only the accepted rows are ever readable.
///
/// # Panics
///
/// Panics when the many-row request is refused, an oversized request is
/// accepted or refused with another code, or the readable count differs.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn native_ingest_bounds_expanded_bytes_not_rows() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let name = unique_table("expanded_ceiling");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;

    let many: Vec<i64> = (0..140_000).collect();
    append_values(&client, &table, uuid::Uuid::now_v7(), &many)
        .await
        .expect("more than 131,072 tiny rows within both ceilings are acknowledged");

    for (rows, reason) in [
        (
            400_000,
            "within the wire ceiling but expanding past four times it",
        ),
        (2_200_000, "above the wire ceiling"),
    ] {
        let values: Vec<i64> = (0..rows).collect();
        let refusal = append_values(&client, &table, uuid::Uuid::now_v7(), &values)
            .await
            .expect_err(reason);
        assert_eq!(
            (refusal.code(), refusal.status()),
            (PAYLOAD_TOO_LARGE, 413),
            "a request {reason} is refused as payload too large"
        );
    }

    assert_eq!(
        read_sql(&client, &format!("SELECT count(*) AS value FROM {table}")).await,
        vec![140_000],
        "only the acknowledged request is readable"
    );
}

/// A table of exactly 256 physical Parquet leaves registers, stages,
/// publishes, and reads back at the default wire ceiling; 257 is refused.
///
/// The managed-column leaf count is read from a probe table's registered
/// Iceberg schema rather than restated, so the wide table sits exactly on the
/// limit whatever the managed columns are.
///
/// # Panics
///
/// Panics when the boundary table is refused, the next one is admitted, or the
/// wide rows do not read back from the staged and published sources.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn widest_registrable_table_stages_publishes_and_reads_back() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let client = tenant_client(&server, tenant).await;
    let user_field = |index: usize| Field::new(format!("c{index}"), DataType::Int64, false);

    let probe = unique_table("leaf_probe");
    register_public(&client, &probe, vec![user_field(0)])
        .await
        .expect("the probe table registers");
    let iceberg = server
        .bifrost_catalog()
        .iceberg_catalog()
        .load_table(
            &TenantTableBinding::resolve((
                tenant,
                TableRef::new(BifrostNamespace::Datasets, &probe),
            ))
            .expect("the probe resolves")
            .table_ident(),
        )
        .await
        .expect("the probe table loads");
    let probe_leaves = parquet::arrow::ArrowSchemaConverter::new()
        .convert(
            &iceberg::arrow::schema_to_arrow_schema(iceberg.metadata().current_schema())
                .expect("the probe schema maps to Arrow"),
        )
        .expect("the probe schema maps to Parquet")
        .num_columns();
    let user_columns = MAX_PHYSICAL_LEAF_COLUMNS - (probe_leaves - 1);

    let over = unique_table("leaves_257");
    let refusal = register_public(&client, &over, (0..=user_columns).map(user_field).collect())
        .await
        .expect_err("257 physical leaves are refused");
    let BifrostClientError::Transport(refusal) = refusal else {
        panic!("the refusal is a typed server error: {refusal}");
    };
    assert_eq!(refusal.code(), "WYRD_VALA_400_SCHEMA_PARSE");

    let name = unique_table("leaves_256");
    let fields: Vec<Field> = (0..user_columns).map(user_field).collect();
    assert_eq!(
        register_public(&client, &name, fields.clone())
            .await
            .expect("256 physical leaves register"),
        RegisterOutcome::Created
    );
    let rows: Vec<i64> = (0..64).collect();
    let columns: Vec<ArrayRef> = (0..user_columns)
        .map(|column| {
            let offset = i64::try_from(column).expect("a column index fits i64") * 1_000;
            Arc::new(Int64Array::from_iter_values(
                rows.iter().map(|row| row + offset),
            )) as ArrayRef
        })
        .collect();
    let batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).expect("wide batch");
    append_batch(
        &client,
        &format!("vala.datasets.{name}"),
        uuid::Uuid::now_v7(),
        &batch,
    )
    .await
    .expect("the widest accepted request is acknowledged");

    let last = user_columns - 1;
    let offset = i64::try_from(last).expect("a column index fits i64") * 1_000;
    assert_stages_and_publishes(
        &server,
        &client,
        tenant,
        &name,
        &format!("SELECT c{last} AS value FROM vala.datasets.{name} ORDER BY value"),
        &rows.iter().map(|row| row + offset).collect::<Vec<_>>(),
    )
    .await;
}

/// One 40 MiB row accepted under a 48 MiB wire ceiling stages, publishes, and
/// reads back: no post-ACK row-group or file policy can refuse its shape.
///
/// # Panics
///
/// Panics when the row is refused, or its length and identity do not read
/// back from the staged and published sources.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn forty_mib_row_stages_publishes_and_reads_back_at_48_mib_wire() {
    let server = start_with_wire_ceiling(48 * 1024 * 1024).await;
    let tenant = server.data_tenant_id();
    let client = tenant_client(&server, tenant).await;
    let name = unique_table("large_row");
    let fields = vec![
        Field::new("value", DataType::Int64, false),
        Field::new("payload", DataType::Binary, false),
    ];
    register_public(&client, &name, fields.clone())
        .await
        .expect("the large-row table registers");
    let payload = vec![0x5a_u8; 40 * 1024 * 1024];
    let batch = RecordBatch::try_new(
        Arc::new(Schema::new(fields)),
        vec![
            Arc::new(Int64Array::from(vec![7])),
            Arc::new(BinaryArray::from_iter_values([payload.as_slice()])),
        ],
    )
    .expect("large-row batch");
    append_batch(
        &client,
        &format!("vala.datasets.{name}"),
        uuid::Uuid::now_v7(),
        &batch,
    )
    .await
    .expect("one 40 MiB row within both ceilings is acknowledged");

    assert_stages_and_publishes(
        &server,
        &client,
        tenant,
        &name,
        &format!(
            "SELECT value + CAST(octet_length(CAST(payload AS VARCHAR)) AS BIGINT) AS value \
             FROM vala.datasets.{name}"
        ),
        &[7 + 40 * 1024 * 1024],
    )
    .await;
}
