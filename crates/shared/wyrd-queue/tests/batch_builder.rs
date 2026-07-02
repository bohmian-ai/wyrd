//! `BatchBuilder` proof: user cols + `card_ref`/`run_id`, reserved/type-mismatch
//! rejection, and Arrow IPC round-trip.

use std::sync::Arc;

use arrow::array::{Array, Int64Array, StringArray};
use arrow::ipc::reader::StreamReader;
use arrow_schema::{DataType, Field, Schema};
use wyrd_queue::BatchBuilder;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

fn user_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, true),
    ]))
}

fn card(name: &str) -> CardRef {
    format!("prod/Service/{name}@1.0.0")
        .parse()
        .expect("valid card ref")
}

#[test]
fn builds_user_columns_plus_correlation_columns() {
    let mut builder = BatchBuilder::new(user_schema());
    builder
        .append_json_row(
            r#"{"id": 1, "name": "a"}"#,
            &card("alpha"),
            Some(&RunId::from_string("run-1".to_owned())),
        )
        .expect("row appends");
    builder
        .append_json_row(r#"{"id": 2, "name": null}"#, &card("beta"), None)
        .expect("row appends");

    let batch = builder.finish().expect("finish");

    // user columns + card_ref + run_id
    assert_eq!(batch.num_columns(), 4);
    assert_eq!(batch.num_rows(), 2);
    let schema = batch.schema();
    assert_eq!(schema.field(2).name(), "card_ref");
    assert_eq!(schema.field(3).name(), "run_id");
    assert!(!schema.field(2).is_nullable(), "card_ref is non-null");
    assert!(schema.field(3).is_nullable(), "run_id is nullable");

    let ids = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("int64");
    assert_eq!(ids.value(0), 1);
    assert_eq!(ids.value(1), 2);

    let names = batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("utf8");
    assert_eq!(names.value(0), "a");
    assert!(names.is_null(1), "explicit null preserved");

    let card_refs = batch
        .column(2)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("utf8");
    assert_eq!(card_refs.value(0), "prod/Service/alpha@1.0.0");
    assert_eq!(card_refs.value(1), "prod/Service/beta@1.0.0");

    let run_ids = batch
        .column(3)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("utf8");
    assert_eq!(run_ids.value(0), "run-1");
    assert!(run_ids.is_null(1), "absent run_id is null");
}

#[test]
fn reserved_payload_key_is_rejected() {
    let mut builder = BatchBuilder::new(user_schema());

    let err = builder
        .append_json_row(r#"{"id": 1, "card_ref": "x"}"#, &card("alpha"), None)
        .unwrap_err();
    assert_eq!(err.code(), "WYRD_VALA_400_BIFROST_RESERVED_COLUMN");

    let err = builder
        .append_json_row(r#"{"id": 1, "wyrd_ts": 1}"#, &card("alpha"), None)
        .unwrap_err();
    assert_eq!(err.code(), "WYRD_VALA_400_BIFROST_RESERVED_COLUMN");
}

#[test]
fn type_mismatch_fails_at_build() {
    let mut builder = BatchBuilder::new(user_schema());
    builder
        .append_json_row(r#"{"id": "not-an-int", "name": "a"}"#, &card("alpha"), None)
        .expect("appends deferred");

    let err = builder.finish().unwrap_err();
    assert_eq!(err.code(), "WYRD_VALA_400_SCHEMA_PARSE");
}

#[test]
fn non_nullable_absent_value_fails() {
    let mut builder = BatchBuilder::new(user_schema());
    builder
        .append_json_row(r#"{"name": "a"}"#, &card("alpha"), None)
        .expect("appends deferred");

    let err = builder.finish().unwrap_err();
    assert_eq!(err.code(), "WYRD_VALA_400_SCHEMA_PARSE");
}

#[test]
fn ipc_round_trips() {
    let mut builder = BatchBuilder::new(user_schema());
    builder
        .append_json_row(r#"{"id": 7, "name": "seven"}"#, &card("alpha"), None)
        .expect("appends");

    let bytes = builder.finish_ipc().expect("ipc");

    let mut reader = StreamReader::try_new(bytes.as_slice(), None).expect("reader");
    let decoded = reader.next().expect("one batch").expect("ok");
    assert_eq!(decoded.num_rows(), 1);
    assert_eq!(decoded.num_columns(), 4);
    let ids = decoded
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("int64");
    assert_eq!(ids.value(0), 7);
    assert!(reader.next().is_none(), "single batch stream");
}
