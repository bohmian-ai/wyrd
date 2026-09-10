//! One fixed canonical trace/log/metric dataset, built as public Arrow batches.
//!
//! The canonical signal tables declare binary, fixed-size-binary and nested
//! list/struct columns, so a caller writes them through the public Arrow batch
//! door rather than the JSON row path. Every batch here is assembled against a
//! schema the caller obtained from `describe_table`, never against a second
//! hard-coded copy of the ledger: the fixture supplies values by column name
//! and lets the described schema decide which columns exist, in what order,
//! and with what type.
//!
//! The logical values are fixed constants so that independent write paths -
//! a stock OTLP exporter and a hand-built canonical batch - can be asserted
//! against the same expected numbers through public SQL.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BinaryBuilder, BooleanBuilder, FixedSizeBinaryBuilder, Float64Builder, Int32Builder,
    Int64Builder, ListArray, StringBuilder, StructArray, new_empty_array,
};
use arrow::buffer::OffsetBuffer;
use arrow::datatypes::{DataType, Field, SchemaRef};
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::tables::signal::encode_attributes;
use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue};

/// The trace every fixture span, and the correlated error log, belong to.
pub const TRACE_ID: [u8; 16] = [
    0xc1, 0xa0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
];
/// The parent span: the GenAI chat operation.
pub const PARENT_SPAN_ID: [u8; 8] = [0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8];
/// The child span: the tool call the chat operation made.
pub const CHILD_SPAN_ID: [u8; 8] = [0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8];
/// The span the parent links to, standing in for a prior turn.
pub const LINKED_SPAN_ID: [u8; 8] = [0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8];

/// The name of the one event the parent span carries.
pub const EVENT_NAME: &str = "gen_ai.choice";
/// The trace state of the one link the parent span carries.
pub const LINK_TRACE_STATE: &str = "wyrd=fixture";

/// The GenAI model both spans report, and the SQL filter selects on.
pub const MODEL: &str = "claude-opus-5";
/// Input tokens the parent span reports.
pub const INPUT_TOKENS: i64 = 1_280;
/// Output tokens the parent span reports.
pub const OUTPUT_TOKENS: i64 = 320;
/// The structured GenAI input payload the payload gate protects.
pub const INPUT_MESSAGES: &str =
    r#"[{"role":"user","parts":[{"type":"text","content":"summarize the incident"}]}]"#;
/// The structured GenAI output payload the payload gate protects.
pub const OUTPUT_MESSAGES: &str =
    r#"[{"role":"assistant","parts":[{"type":"text","content":"the writer stalled"}]}]"#;
/// The body of the trace-correlated error log.
pub const LOG_BODY: &str = "tool call exhausted its retry budget";
/// The counter metric's fixed integer value.
pub const COUNTER_VALUE: i64 = 7;
/// The gauge metric's fixed double value.
pub const GAUGE_VALUE: f64 = 0.75;
/// The histogram metric's fixed observation count.
pub const HISTOGRAM_COUNT: i64 = 4;
/// The histogram metric's fixed observation sum.
pub const HISTOGRAM_SUM: f64 = 12.5;

/// One column value the fixture supplies by name.
///
/// Anything a fixture does not name is filled from the described column's own
/// type, so this carries only the values a journey actually asserts.
#[derive(Clone, Debug)]
pub enum Cell {
    /// A UTF-8 value.
    Text(String),
    /// A 32-bit integer value.
    Int32(i32),
    /// A 64-bit integer value.
    Int64(i64),
    /// A double value.
    Float64(f64),
    /// A boolean value.
    Bool(bool),
    /// Opaque bytes, already in their canonical encoding.
    Bytes(Vec<u8>),
    /// A present but empty collection.
    Empty,
    /// A present collection of nested rows, for a `List<Struct<..>>` column.
    Rows(Vec<Row>),
    /// An explicit null, for a nullable column the fixture leaves absent.
    Null,
}

/// One fixture row: the columns it names, keyed by canonical column name.
pub type Row = BTreeMap<&'static str, Cell>;

/// Encode one attribute collection of string values as canonical bytes.
///
/// The canonical encoding is the server's own, reached through the published
/// helper rather than restated here, so a fixture cannot drift from the
/// encoding ingress verifies.
#[must_use]
pub fn attributes(pairs: &[(&str, &str)]) -> Vec<u8> {
    let values: Vec<KeyValue> = pairs
        .iter()
        .map(|(key, value)| KeyValue {
            key: (*key).to_owned(),
            value: Some(AnyValue {
                value: Some(Value::StringValue((*value).to_owned())),
            }),
        })
        .collect();
    encode_attributes(&values)
}

/// Build one Arrow batch over `schema` from rows that name only some columns.
///
/// Every described column is produced: a named one takes the supplied value,
/// an unnamed nullable one becomes null, and an unnamed non-null one takes the
/// canonical empty value for its type - empty text, zero, `false`, zero bytes,
/// or an empty list. That keeps a fixture to the handful of values its
/// assertions depend on while still satisfying the table's full ledger.
///
/// # Panics
///
/// Panics when a supplied cell does not fit its described column's type, or
/// when the assembled columns do not form a batch under `schema`, both of
/// which are fixture authoring errors rather than product behavior.
#[must_use]
pub fn batch(schema: &SchemaRef, rows: &[Row]) -> RecordBatch {
    let columns: Vec<ArrayRef> = schema
        .fields()
        .iter()
        .map(|field| column(field, rows))
        .collect();
    RecordBatch::try_new(Arc::clone(schema), columns).expect("the fixture rows assemble")
}

/// Build one described column across every fixture row.
fn column(field: &Field, rows: &[Row]) -> ArrayRef {
    let cells: Vec<Cell> = rows
        .iter()
        .map(|row| {
            row.get(field.name().as_str())
                .cloned()
                .unwrap_or_else(|| default_cell(field))
        })
        .collect();
    match field.data_type() {
        DataType::Utf8 => {
            let mut builder = StringBuilder::new();
            for cell in &cells {
                match cell {
                    Cell::Text(value) => builder.append_value(value),
                    Cell::Null => builder.append_null(),
                    other => panic!("column {} cannot take {other:?}", field.name()),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Int32 => {
            let mut builder = Int32Builder::new();
            for cell in &cells {
                match cell {
                    Cell::Int32(value) => builder.append_value(*value),
                    Cell::Null => builder.append_null(),
                    other => panic!("column {} cannot take {other:?}", field.name()),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Int64 => {
            let mut builder = Int64Builder::new();
            for cell in &cells {
                match cell {
                    Cell::Int64(value) => builder.append_value(*value),
                    Cell::Null => builder.append_null(),
                    other => panic!("column {} cannot take {other:?}", field.name()),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Float64 => {
            let mut builder = Float64Builder::new();
            for cell in &cells {
                match cell {
                    Cell::Float64(value) => builder.append_value(*value),
                    Cell::Null => builder.append_null(),
                    other => panic!("column {} cannot take {other:?}", field.name()),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Boolean => {
            let mut builder = BooleanBuilder::new();
            for cell in &cells {
                match cell {
                    Cell::Bool(value) => builder.append_value(*value),
                    Cell::Null => builder.append_null(),
                    other => panic!("column {} cannot take {other:?}", field.name()),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Binary => {
            let mut builder = BinaryBuilder::new();
            for cell in &cells {
                match cell {
                    Cell::Bytes(value) => builder.append_value(value),
                    Cell::Null => builder.append_null(),
                    other => panic!("column {} cannot take {other:?}", field.name()),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::FixedSizeBinary(width) => {
            let mut builder = FixedSizeBinaryBuilder::new(*width);
            for cell in &cells {
                match cell {
                    Cell::Bytes(value) => builder
                        .append_value(value)
                        .expect("a fixture identifier is the declared width"),
                    Cell::Null => builder.append_null(),
                    other => panic!("column {} cannot take {other:?}", field.name()),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::List(element) => {
            if cells.iter().any(|cell| matches!(cell, Cell::Rows(_))) {
                nested_rows(element, &cells)
            } else {
                empty_lists(element, &cells)
            }
        }
        DataType::Struct(children) => {
            assert!(
                cells.iter().all(|cell| matches!(cell, Cell::Null)),
                "column {} is a nested struct the fixture only leaves absent",
                field.name()
            );
            Arc::new(StructArray::new(
                children.clone(),
                children.iter().map(|child| column(child, rows)).collect(),
                Some(arrow::buffer::NullBuffer::new_null(cells.len())),
            ))
        }
        other => panic!("column {} has unsupported type {other}", field.name()),
    }
}

/// Build a `List<Struct<..>>` column from the nested rows each cell names.
///
/// The element struct's own children are produced by the same column builder
/// as a top-level column, so a nested row names only the fields its assertion
/// depends on and the rest take their canonical empty value.
///
/// # Panics
///
/// Panics when the element is not a struct, or when a cell carries a scalar
/// where the described column declares a nested collection.
fn nested_rows(element: &Arc<Field>, cells: &[Cell]) -> ArrayRef {
    let DataType::Struct(children) = element.data_type() else {
        panic!(
            "nested rows require a struct element, not {}",
            element.data_type()
        )
    };
    let mut offsets: Vec<i32> = Vec::with_capacity(cells.len() + 1);
    let mut flat: Vec<Row> = Vec::new();
    offsets.push(0);
    for cell in cells {
        match cell {
            Cell::Rows(rows) => flat.extend(rows.iter().cloned()),
            Cell::Empty | Cell::Null => {}
            other => panic!("a nested collection cannot take {other:?}"),
        }
        offsets.push(i32::try_from(flat.len()).expect("a fixture holds few nested rows"));
    }
    let values = Arc::new(StructArray::new(
        children.clone(),
        children.iter().map(|child| column(child, &flat)).collect(),
        None,
    ));
    let nulls: Vec<bool> = cells
        .iter()
        .map(|cell| !matches!(cell, Cell::Null))
        .collect();
    Arc::new(ListArray::new(
        Arc::clone(element),
        OffsetBuffer::new(offsets.into()),
        values,
        Some(arrow::buffer::NullBuffer::from(nulls.as_slice())),
    ))
}

/// Build an all-empty list column, the canonical absent-collection value.
///
/// A nullable list keeps its null where the fixture named none; a non-null one
/// is a present empty collection, which is what the canonical ledger means by
/// an unnamed nested column.
fn empty_lists(element: &Arc<Field>, cells: &[Cell]) -> ArrayRef {
    let offsets = OffsetBuffer::new_zeroed(cells.len());
    let nulls: Vec<bool> = cells
        .iter()
        .map(|cell| !matches!(cell, Cell::Null))
        .collect();
    let values = new_empty_array(element.data_type());
    Arc::new(ListArray::new(
        Arc::clone(element),
        offsets,
        values,
        Some(arrow::buffer::NullBuffer::from(nulls.as_slice())),
    ))
}

/// The value an unnamed column takes, decided by its described type.
fn default_cell(field: &Field) -> Cell {
    if field.is_nullable() {
        return Cell::Null;
    }
    match field.data_type() {
        DataType::Utf8 => Cell::Text(String::new()),
        DataType::Int32 => Cell::Int32(0),
        DataType::Int64 => Cell::Int64(0),
        DataType::Float64 => Cell::Float64(0.0),
        DataType::Boolean => Cell::Bool(false),
        DataType::Binary | DataType::FixedSizeBinary(_) => Cell::Bytes(Vec::new()),
        DataType::List(_) => Cell::Empty,
        other => panic!("column {} has unsupported type {other}", field.name()),
    }
}

/// Build the two-span parent/child trace batch over a described span schema.
///
/// The parent is the GenAI chat operation carrying promoted operation,
/// provider, model, conversation and token columns plus the structured
/// input/output message payloads; the child is the tool call it made, which
/// ends in error status. Both carry resource and scope metadata so a reader
/// can attribute them to a service.
///
/// # Panics
///
/// Panics when the described schema does not assemble the fixture rows.
#[must_use]
pub fn spans(schema: &SchemaRef, scope: &str, anchor_nanos: i64) -> RecordBatch {
    let parent: Row = BTreeMap::from([
        ("trace_id", Cell::Bytes(TRACE_ID.to_vec())),
        ("span_id", Cell::Bytes(PARENT_SPAN_ID.to_vec())),
        ("name", Cell::Text("chat claude-opus-5".to_owned())),
        ("kind", Cell::Int32(3)),
        ("start_time_unix_nano", Cell::Int64(anchor_nanos)),
        ("end_time_unix_nano", Cell::Int64(anchor_nanos + 2_000_000)),
        ("duration_nano", Cell::Int64(2_000_000)),
        ("status_present", Cell::Bool(true)),
        ("status_code", Cell::Int32(1)),
        ("status_message", Cell::Text("ok".to_owned())),
        (
            "attributes",
            Cell::Bytes(attributes(&[
                ("gen_ai.input.messages", INPUT_MESSAGES),
                ("gen_ai.output.messages", OUTPUT_MESSAGES),
            ])),
        ),
        ("resource_present", Cell::Bool(true)),
        (
            "resource_attributes",
            Cell::Bytes(attributes(&[("service.name", "wyrd.fixture.service")])),
        ),
        ("scope_present", Cell::Bool(true)),
        ("scope_name", Cell::Text(scope.to_owned())),
        ("scope_version", Cell::Text("1.0.0".to_owned())),
        (
            "service_name",
            Cell::Text("wyrd.fixture.service".to_owned()),
        ),
        ("gen_ai_operation_name", Cell::Text("chat".to_owned())),
        ("gen_ai_provider_name", Cell::Text("anthropic".to_owned())),
        ("gen_ai_request_model", Cell::Text(MODEL.to_owned())),
        (
            "gen_ai_conversation_id",
            Cell::Text("conversation-fixture".to_owned()),
        ),
        ("gen_ai_usage_input_tokens", Cell::Int64(INPUT_TOKENS)),
        ("gen_ai_usage_output_tokens", Cell::Int64(OUTPUT_TOKENS)),
        (
            "events",
            Cell::Rows(vec![BTreeMap::from([
                ("time_unix_nano", Cell::Int64(anchor_nanos + 1_000_000)),
                ("name", Cell::Text(EVENT_NAME.to_owned())),
                (
                    "attributes",
                    Cell::Bytes(attributes(&[("gen_ai.finish_reason", "stop")])),
                ),
            ])]),
        ),
        (
            "links",
            Cell::Rows(vec![BTreeMap::from([
                ("trace_id", Cell::Bytes(TRACE_ID.to_vec())),
                ("span_id", Cell::Bytes(LINKED_SPAN_ID.to_vec())),
                ("trace_state", Cell::Text(LINK_TRACE_STATE.to_owned())),
                ("flags", Cell::Int64(1)),
                (
                    "attributes",
                    Cell::Bytes(attributes(&[("link.kind", "follows_from")])),
                ),
            ])]),
        ),
    ]);
    let child: Row = BTreeMap::from([
        ("trace_id", Cell::Bytes(TRACE_ID.to_vec())),
        ("span_id", Cell::Bytes(CHILD_SPAN_ID.to_vec())),
        ("parent_span_id", Cell::Bytes(PARENT_SPAN_ID.to_vec())),
        ("name", Cell::Text("execute_tool search".to_owned())),
        ("kind", Cell::Int32(1)),
        ("start_time_unix_nano", Cell::Int64(anchor_nanos + 100_000)),
        ("end_time_unix_nano", Cell::Int64(anchor_nanos + 900_000)),
        ("duration_nano", Cell::Int64(800_000)),
        ("status_present", Cell::Bool(true)),
        ("status_code", Cell::Int32(2)),
        (
            "status_message",
            Cell::Text("tool call exhausted its retry budget".to_owned()),
        ),
        (
            "attributes",
            Cell::Bytes(attributes(&[("gen_ai.tool.name", "search")])),
        ),
        ("resource_present", Cell::Bool(true)),
        (
            "resource_attributes",
            Cell::Bytes(attributes(&[("service.name", "wyrd.fixture.service")])),
        ),
        ("scope_present", Cell::Bool(true)),
        ("scope_name", Cell::Text(scope.to_owned())),
        ("scope_version", Cell::Text("1.0.0".to_owned())),
        (
            "service_name",
            Cell::Text("wyrd.fixture.service".to_owned()),
        ),
        (
            "gen_ai_operation_name",
            Cell::Text("execute_tool".to_owned()),
        ),
        ("gen_ai_provider_name", Cell::Text("anthropic".to_owned())),
        ("gen_ai_request_model", Cell::Text(MODEL.to_owned())),
        (
            "gen_ai_conversation_id",
            Cell::Text("conversation-fixture".to_owned()),
        ),
        ("gen_ai_usage_input_tokens", Cell::Int64(64)),
        ("gen_ai_usage_output_tokens", Cell::Int64(16)),
    ]);
    batch(schema, &[parent, child])
}

/// Build the trace-correlated error log batch over a described log schema.
///
/// # Panics
///
/// Panics when the described schema does not assemble the fixture row.
#[must_use]
pub fn logs(schema: &SchemaRef, scope: &str, anchor_nanos: i64) -> RecordBatch {
    let record: Row = BTreeMap::from([
        ("time_unix_nano", Cell::Int64(anchor_nanos + 800_000)),
        (
            "observed_time_unix_nano",
            Cell::Int64(anchor_nanos + 850_000),
        ),
        ("severity_number", Cell::Int32(17)),
        ("severity_text", Cell::Text("ERROR".to_owned())),
        ("event_name", Cell::Text("tool.retry.exhausted".to_owned())),
        (
            "body",
            Cell::Bytes(vala_bifrost_redux::tables::signal::encode_any_value(
                &AnyValue {
                    value: Some(Value::StringValue(LOG_BODY.to_owned())),
                },
            )),
        ),
        ("trace_id", Cell::Bytes(TRACE_ID.to_vec())),
        ("span_id", Cell::Bytes(CHILD_SPAN_ID.to_vec())),
        (
            "attributes",
            Cell::Bytes(attributes(&[("gen_ai.tool.name", "search")])),
        ),
        ("resource_present", Cell::Bool(true)),
        (
            "resource_attributes",
            Cell::Bytes(attributes(&[("service.name", "wyrd.fixture.service")])),
        ),
        ("scope_present", Cell::Bool(true)),
        ("scope_name", Cell::Text(scope.to_owned())),
        ("scope_version", Cell::Text("1.0.0".to_owned())),
    ]);
    batch(schema, &[record])
}

/// Build the counter, gauge and histogram point batch over a described schema.
///
/// Scenario 1 owns exhaustive metric-kind fidelity; these three are the
/// representative kinds a reader aggregates over.
///
/// # Panics
///
/// Panics when the described schema does not assemble the fixture rows.
#[must_use]
pub fn points(schema: &SchemaRef, scope: &str, anchor_nanos: i64) -> RecordBatch {
    let common = |name: &str, kind: &str| -> Row {
        BTreeMap::from([
            ("metric_name", Cell::Text(name.to_owned())),
            ("description", Cell::Text(format!("fixture {kind}"))),
            ("unit", Cell::Text("1".to_owned())),
            ("metric_type", Cell::Text(kind.to_owned())),
            ("time_unix_nano", Cell::Int64(anchor_nanos)),
            ("start_time_unix_nano", Cell::Int64(anchor_nanos)),
            (
                "attributes",
                Cell::Bytes(attributes(&[("gen_ai.request.model", MODEL)])),
            ),
            ("resource_present", Cell::Bool(true)),
            (
                "resource_attributes",
                Cell::Bytes(attributes(&[("service.name", "wyrd.fixture.service")])),
            ),
            ("scope_present", Cell::Bool(true)),
            ("scope_name", Cell::Text(scope.to_owned())),
            ("scope_version", Cell::Text("1.0.0".to_owned())),
        ])
    };
    let mut counter = common("wyrd.fixture.requests", "sum");
    counter.insert("int_value", Cell::Int64(COUNTER_VALUE));
    counter.insert("aggregation_temporality", Cell::Int32(2));
    counter.insert("is_monotonic", Cell::Bool(true));

    let mut gauge = common("wyrd.fixture.saturation", "gauge");
    gauge.insert("double_value", Cell::Float64(GAUGE_VALUE));

    let mut histogram = common("wyrd.fixture.latency", "histogram");
    histogram.insert("aggregation_temporality", Cell::Int32(2));
    histogram.insert("histogram_count", Cell::Int64(HISTOGRAM_COUNT));
    histogram.insert("histogram_sum", Cell::Float64(HISTOGRAM_SUM));
    histogram.insert("histogram_min", Cell::Float64(1.0));
    histogram.insert("histogram_max", Cell::Float64(6.0));

    batch(schema, &[counter, gauge, histogram])
}

/// A seeded canonical dataset plus the credentials an agent surface reads it
/// with.
#[derive(Debug, Clone)]
pub struct SeededCanonicalSignals {
    /// Bound Wyrd HTTP endpoint an MCP or CLI client connects to.
    pub endpoint: String,
    /// Short-lived bearer exchanged through the real auth route.
    pub token: String,
    /// Instrumentation scope every seeded row carries, which isolates this
    /// dataset from anything else the server holds.
    pub scope: String,
}

/// Provision the canonical ledgers and write one complete signal dataset.
///
/// The three tables are server-owned, so the harness provisions them, then
/// writes through the same public Arrow batch door a caller has: each batch is
/// built over the schema the table's own description publishes. After the
/// Scribe flush the rows are readable through public SQL, which is what an
/// agent surface journey needs in front of it.
///
/// # Errors
///
/// Returns a [`WyrdTestServerError`] when provisioning, bootstrap, describe,
/// the batch write, the flush, endpoint discovery, or token exchange fails.
pub async fn seed_canonical_signals(
    server: &crate::server::WyrdTestServer,
    name: &str,
) -> Result<SeededCanonicalSignals, crate::server::WyrdTestServerError> {
    use crate::server::WyrdTestServerError;

    for (namespace, table) in [
        ("traces", "spans"),
        ("logs", "records"),
        ("metrics", "points"),
    ] {
        server
            .ensure_builtin_table_for_test(server.data_tenant_id(), namespace, table)
            .await?;
    }
    let bootstrap = server.bootstrap_service(name, &["admin"]).await?;
    let api_key = bootstrap
        .api_key()
        .ok_or_else(|| {
            WyrdTestServerError::Auth("the canonical fixture requires a service key".into())
        })?
        .clone();
    let card_ref = bootstrap
        .card_ref()
        .ok_or_else(|| {
            WyrdTestServerError::Auth("the canonical fixture requires a machine principal".into())
        })?
        .clone();
    let writer = crate::bifrost::write::BifrostWriter::connect(
        wyrd_client::config::ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: server
                    .grpc_url()
                    .ok_or_else(|| WyrdTestServerError::Start("missing gRPC URL".into()))?,
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            http: wyrd_client::transport::HttpConfig {
                base_url: server
                    .base_url()
                    .ok_or_else(|| WyrdTestServerError::Start("missing HTTP URL".into()))?
                    .to_owned(),
                ..wyrd_client::transport::HttpConfig::default()
            },
            credential: Some(api_key.clone()),
            ..wyrd_client::config::ClientConfig::default()
        },
        card_ref,
    )
    .await
    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;

    let scope = format!("wyrd.canonical.{}", uuid::Uuid::now_v7().simple());
    let anchor = 1_760_000_000_000_000_000_i64;
    for (fqn, build) in [
        (
            "vala.traces.spans",
            spans as fn(&SchemaRef, &str, i64) -> RecordBatch,
        ),
        ("vala.logs.records", logs),
        ("vala.metrics.points", points),
    ] {
        let described = vala_sdk::TableConfig::describe(writer.client(), fqn)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        writer
            .write_batch(fqn, &build(described.user_schema(), &scope, anchor))
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
    }
    server.flush_bifrost().await?;
    let token = server.exchange_api_key(&api_key).await?;
    let endpoint = server
        .base_url()
        .ok_or_else(|| WyrdTestServerError::Start("missing HTTP URL".into()))?
        .to_owned();
    Ok(SeededCanonicalSignals {
        endpoint,
        token,
        scope,
    })
}
