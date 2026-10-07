//! Declare every Iceberg column type from an Arrow schema, write it, and read it back.
//!
//! `TableConfig::from_arrow` declares the types a derived model cannot, such
//! as `Decimal128`, `Int32`, maps, and nanosecond timestamps. Types with no
//! Iceberg column are refused before any table exists. Every SDK reads the
//! same `fixtures/bifrost` batches, written by
//! `fixtures/bifrost/every_iceberg_type.py`.

use std::fs::File;
use std::sync::Arc;

use arrow::array::RecordBatch;
use arrow::compute::{cast, concat_batches};
use arrow::ipc::reader::FileReader;
use arrow_schema::{DataType, Field, IntervalUnit, Schema, TimeUnit, UnionFields, UnionMode};
use wyrd_client::WyrdClient;
use wyrd_client::bifrost::{Bifrost, TableConfig};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::RegisterOutcome;
use wyrd_testing::server::WyrdTestServer;

use crate::pg_tests::admin_client;

/// The table every Iceberg type is registered and written to.
const ALL_TYPES: &str = "vala.datasets.all_types";
/// The table the narrower Arrow spellings are registered and written to.
const NARROW_TYPES: &str = "vala.datasets.narrow_types";
/// The catalog code for a type Bifrost cannot store.
const UNSUPPORTED_TYPE: &str = "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE";

/// Read one shared fixture batch from `fixtures/bifrost`.
///
/// # Panics
///
/// Panics when the fixture is missing or is not one Arrow IPC file batch.
fn fixture(name: &str) -> RecordBatch {
    let path = format!(
        "{}/../../../fixtures/bifrost/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    FileReader::try_new(File::open(&path).expect("the fixture exists"), None)
        .expect("the fixture is an Arrow IPC file")
        .next()
        .expect("the fixture holds a batch")
        .expect("the fixture batch decodes")
}

/// Register `fqn` from `batch`'s schema as `client`, write `batch`, publish
/// it, and read every column back ordered by `order_by`.
///
/// # Panics
///
/// Panics when registration, the write, publication, or the read fails.
async fn round_trip(
    srv: &WyrdTestServer,
    client: &WyrdClient,
    fqn: &str,
    batch: &RecordBatch,
    order_by: &str,
) -> RecordBatch {
    let table = Bifrost::connect_with_table(
        client,
        TableConfig::from_arrow(fqn, batch.schema()).expect("the fixture declares a table"),
    )
    .await
    .expect("the writer connects");
    table.register().await.expect("the table registers");
    table
        .write_batch(fqn, batch)
        .await
        .expect("the batch writes");
    srv.flush_bifrost().await.expect("the rows publish");
    let read = table
        .sql(&format!("SELECT * FROM {fqn} ORDER BY {order_by}"))
        .await
        .expect("the rows read back");
    concat_batches(read.schema(), read.batches()).expect("one read batch")
}

/// Declare a one-column table `fqn` holding `column`.
///
/// # Errors
///
/// Returns the SDK refusal when `column` has no wire form.
fn one_column(fqn: &str, column: Field) -> Result<TableConfig, WyrdError> {
    TableConfig::from_arrow(fqn, Arc::new(Schema::new(vec![column])))
        .map_err(|error| WyrdError::from(&error))
}

/// A map column of string keys to `value` entries.
fn map_of(value: DataType) -> Field {
    Field::new_map(
        "c",
        "entries",
        Field::new("key", DataType::Utf8, false),
        Field::new("value", value, true),
        false,
        true,
    )
}

/// Every column of `every_iceberg_type.arrow` reads back with its Arrow type
/// and values, nested nulls included.
///
/// # Panics
///
/// Panics when a column's type or values differ.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn every_iceberg_type_round_trips_with_nested_nulls() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let client = admin_client(&srv, "every-iceberg-column-type").await;
    let written = fixture("every_iceberg_type.arrow");

    let read = round_trip(&srv, &client, ALL_TYPES, &written, "id").await;

    for field in written.schema().fields() {
        let sent = written
            .column_by_name(field.name())
            .expect("written column");
        let got = read.column_by_name(field.name()).expect("read column");
        assert!(
            got.data_type().equals_datatype(sent.data_type()),
            "{} wrote {} and read {}",
            field.name(),
            sent.data_type(),
            got.data_type()
        );
        let got = cast(got, sent.data_type()).expect("the read column takes the written names");
        assert_eq!(
            got.as_ref(),
            sent.as_ref(),
            "{} keeps its values",
            field.name()
        );
    }
    srv.shutdown().await.expect("server shutdown");
}

/// Every column of `narrow_types.arrow` reads back with the values written.
///
/// # Panics
///
/// Panics when a column's values differ.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn a_narrower_type_reads_back_the_same_values() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let client = admin_client(&srv, "every-iceberg-column-type").await;
    let written = fixture("narrow_types.arrow");

    let read = round_trip(&srv, &client, NARROW_TYPES, &written, "int8").await;

    for field in written.schema().fields() {
        let sent = written
            .column_by_name(field.name())
            .expect("written column");
        let got = read.column_by_name(field.name()).expect("read column");
        let got = cast(got, sent.data_type()).expect("the stored type casts back");
        assert_eq!(
            got.as_ref(),
            sent.as_ref(),
            "{} keeps its values",
            field.name()
        );
    }
    srv.shutdown().await.expect("server shutdown");
}

/// Declaring the table the server describes for narrower spellings finds the
/// same table.
///
/// # Panics
///
/// Panics when the described declaration is not `AlreadyExists`.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn a_narrower_declaration_is_the_table_it_describes() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let client = admin_client(&srv, "every-iceberg-column-type").await;
    round_trip(
        &srv,
        &client,
        NARROW_TYPES,
        &fixture("narrow_types.arrow"),
        "int8",
    )
    .await;

    let described = Bifrost::connect_with_table(
        &client,
        TableConfig::describe(&client, NARROW_TYPES)
            .await
            .expect("the table describes"),
    )
    .await
    .expect("the writer connects");

    assert_eq!(
        described
            .register()
            .await
            .expect("the described table registers"),
        RegisterOutcome::AlreadyExists
    );
    srv.shutdown().await.expect("server shutdown");
}

/// A type with no wire form, or one nested in a Map where Iceberg cannot
/// store it, is refused when the table is declared.
///
/// # Panics
///
/// Panics when a type is accepted or refused with another code.
#[test]
fn a_type_bifrost_cannot_store_is_refused_when_declared() {
    let union = UnionFields::try_new(vec![0], vec![Field::new("i", DataType::Int32, true)])
        .expect("union fields");
    for column in [
        Field::new("c", DataType::Union(union, UnionMode::Dense), true),
        Field::new("c", DataType::Duration(TimeUnit::Second), true),
        Field::new("c", DataType::Interval(IntervalUnit::MonthDayNano), true),
        Field::new("c", DataType::Float16, true),
        map_of(DataType::Float16),
        map_of(DataType::UInt64),
    ] {
        let data_type = column.data_type().clone();

        let refused = one_column("vala.datasets.no_wire_form", column)
            .expect_err("a type Bifrost cannot store is refused");

        assert_eq!(refused.code(), UNSUPPORTED_TYPE, "{data_type}");
    }
}
