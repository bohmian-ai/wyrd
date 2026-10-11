//! Register a user table on its own time column and read a time range back.
//!
//! A table declares `time_column` on its physical layout. The column
//! partitions the table and bounds its files, so a time-range read returns
//! exactly the rows inside the range. `describe` reports the declared column;
//! a table declared without one reports `wyrd_ingested_at`; and a declaration
//! naming a column that cannot be a time column is refused naming the field.

use std::sync::Arc;

use arrow::array::{Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef, TimeUnit};
use wyrd_client::bifrost::{Bifrost, TableConfig};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{PhysicalLayoutWire, TimeGranularityWire, UTC_TIME_ZONE};
use wyrd_testing::server::WyrdTestServer;

use crate::pg_tests::admin_client;

/// The table partitioned on its declared `event_time` column.
const READINGS: &str = "vala.datasets.time_column_readings";
/// The table registered without a declared time column.
const DEFAULT_READINGS: &str = "vala.datasets.time_column_defaults";
/// The table whose declared time column is not a timestamp.
const REFUSED_READINGS: &str = "vala.datasets.time_column_refused";
/// The declared time column.
const EVENT_TIME: &str = "event_time";
/// One hour in microseconds.
const HOUR_MICROS: i64 = 3_600_000_000;
/// 2026-10-06T00:00:00Z in microseconds, the first written hour.
const FIRST_HOUR_MICROS: i64 = 1_791_244_800_000_000;

/// The readings schema: a non-null UTC `event_time` and an integer value.
fn readings_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new(
            EVENT_TIME,
            DataType::Timestamp(TimeUnit::Microsecond, Some(UTC_TIME_ZONE.into())),
            false,
        ),
        Field::new("value", DataType::Int64, false),
    ]))
}

/// An hourly layout partitioned on `time_column`.
fn hourly_on(time_column: Option<&str>) -> PhysicalLayoutWire {
    PhysicalLayoutWire {
        partition_granularity: TimeGranularityWire::Hour,
        time_column: time_column.map(str::to_owned),
        sort_keys: Vec::new(),
        bloom_columns: Vec::new(),
    }
}

/// One reading in each of four consecutive hours; the value is the hour index.
///
/// # Panics
///
/// Panics when the batch does not match [`readings_schema`].
fn readings() -> RecordBatch {
    let hours = 0..4_i64;
    RecordBatch::try_new(
        readings_schema(),
        vec![
            Arc::new(
                TimestampMicrosecondArray::from_iter_values(
                    hours
                        .clone()
                        .map(|hour| FIRST_HOUR_MICROS + hour * HOUR_MICROS + 1),
                )
                .with_timezone(UTC_TIME_ZONE),
            ),
            Arc::new(Int64Array::from_iter_values(hours)),
        ],
    )
    .expect("the readings match the schema")
}

/// Connect a writer for `fqn` declared with `layout`, as a service named
/// after the table so each table gets its own service identity.
///
/// # Panics
///
/// Panics when the writer cannot connect.
async fn connect(srv: &WyrdTestServer, fqn: &str, layout: PhysicalLayoutWire) -> Bifrost {
    let service = fqn.rsplit('.').next().unwrap_or(fqn).replace('_', "-");
    let client = admin_client(srv, &service).await;
    Bifrost::connect_with_table(
        &client,
        TableConfig::from_arrow(fqn, readings_schema())
            .expect("the schema declares a table")
            .with_layout(layout),
    )
    .await
    .expect("the writer connects")
}

/// A table declared on `event_time` returns exactly the rows in a time range,
/// and `describe` reports the declared column; a table declared without one
/// reports `wyrd_ingested_at`.
///
/// # Panics
///
/// Panics when registration, the write, publication, or the read fails, or
/// when the range or the described time column differs.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn user_table_time_column_prunes_a_time_range() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let readings_table = connect(&srv, READINGS, hourly_on(Some(EVENT_TIME))).await;
    readings_table
        .register()
        .await
        .expect("the table registers");
    readings_table
        .write_batch(READINGS, &readings())
        .await
        .expect("the readings are admitted");
    readings_table.flush().await.expect("the readings flush");
    srv.flush_bifrost().await.expect("the readings publish");

    let in_range: Vec<serde_json::Value> = readings_table
        .sql_as(&format!(
            "SELECT value FROM {READINGS} \
             WHERE {EVENT_TIME} >= to_timestamp_micros({lower}) \
             AND {EVENT_TIME} < to_timestamp_micros({upper}) ORDER BY value",
            lower = FIRST_HOUR_MICROS + HOUR_MICROS,
            upper = FIRST_HOUR_MICROS + 3 * HOUR_MICROS,
        ))
        .await
        .expect("the range reads back");
    let described = readings_table.describe(READINGS).await.expect("describes");

    let defaults = connect(&srv, DEFAULT_READINGS, hourly_on(None)).await;
    defaults
        .register()
        .await
        .expect("the default table registers");
    let default_described = defaults
        .describe(DEFAULT_READINGS)
        .await
        .expect("describes");

    assert_eq!(
        in_range,
        vec![
            serde_json::json!({"value": 1}),
            serde_json::json!({"value": 2})
        ]
    );
    assert_eq!(
        described.physical_layout.time_column.as_deref(),
        Some(EVENT_TIME)
    );
    assert_eq!(
        default_described.physical_layout.time_column.as_deref(),
        Some("wyrd_ingested_at")
    );
    srv.shutdown().await.expect("server shutdown");
}

/// A declared time column that is not a timestamp is refused with the
/// invalid-layout code naming the time-column field.
///
/// # Panics
///
/// Panics when registration succeeds or is refused for another reason.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn invalid_time_column_is_refused_naming_the_field() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let refused_table = connect(&srv, REFUSED_READINGS, hourly_on(Some("value"))).await;

    let refused = refused_table
        .register()
        .await
        .expect_err("a non-timestamp time column is refused");
    let error = WyrdError::from(&refused);

    assert_eq!(
        error.code(),
        "WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT"
    );
    assert!(
        refused.to_string().contains("TimeColumn"),
        "the refusal names the time-column field: {refused}"
    );
    srv.shutdown().await.expect("server shutdown");
}
