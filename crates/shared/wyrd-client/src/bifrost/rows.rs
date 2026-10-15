//! Typed cell readers over query result batches, and the decoder from a
//! `vala.traces.spans` row to the canonical [`SpanRecord`].
//!
//! Cell readers index each column in its stored Arrow type rather than casting
//! it, and report a missing or mistyped column as a description the caller
//! wraps in its own error. The trace timeline read and the server's Eval
//! evidence read decode spans through the one [`span_record`].

use arrow::array::{Array, ArrayRef, AsArray as _};
use arrow::datatypes::{DataType, Int32Type, Int64Type, TimestampNanosecondType};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use wyrd_queue::variant::variant_cell_to_json;
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::trace::{
    InstrumentationScope, Resource, SpanEvent, SpanKind, SpanLink, SpanRecord, SpanStatus,
};

/// The `vala.traces.spans` columns [`span_record`] reads, as a SELECT list.
pub const SPAN_COLUMNS: &str = "trace_id, span_id, parent_span_id, trace_state, flags, name, kind, \
     start_time_unix_nano, end_time_unix_nano, status_code, status_message, \
     attributes, dropped_attributes_count, events, dropped_events_count, \
     links, dropped_links_count, scope_name, service_name";

/// Decode row `row` of a `vala.traces.spans` result into the canonical span
/// record, including its persisted events, links, and dropped counts.
///
/// The batch must select at least [`SPAN_COLUMNS`] by name.
///
/// # Errors
/// Returns a description of a missing or malformed column.
// ponytail: resource attributes and scope version are not projected; decode
// them when a trace assertion needs to select them.
pub fn span_record(batch: &RecordBatch, row: usize) -> Result<SpanRecord, String> {
    let trace_id = bytes(batch, "trace_id", row)?
        .ok_or("trace_id is missing")
        .and_then(|id| <[u8; 16]>::try_from(id).map_err(|_| "trace_id is not 16 bytes"))?;
    let trace_id = TraceId::from_bytes(trace_id).map_err(|error| error.to_string())?;
    let span_id = |name| -> Result<Option<SpanId>, String> {
        bytes(batch, name, row)?
            .map(|id| {
                <[u8; 8]>::try_from(id)
                    .map_err(|_| format!("{name} is not 8 bytes"))
                    .and_then(|id| SpanId::from_bytes(id).map_err(|error| error.to_string()))
            })
            .transpose()
    };
    let nanos = |name| -> Result<DateTime<Utc>, String> {
        instant(batch, name, row)?.ok_or_else(|| format!("{name} is missing"))
    };
    let (start_time, end_time) = (nanos("start_time_unix_nano")?, nanos("end_time_unix_nano")?);
    let status = match int32(batch, "status_code", row)? {
        Some(1) => SpanStatus::Ok,
        Some(2) => SpanStatus::Error {
            description: text(batch, "status_message", row)?,
        },
        _ => SpanStatus::Unset,
    };
    let kind = match int32(batch, "kind", row)? {
        Some(2) => SpanKind::Server,
        Some(3) => SpanKind::Client,
        Some(4) => SpanKind::Producer,
        Some(5) => SpanKind::Consumer,
        _ => SpanKind::Internal,
    };
    Ok(SpanRecord {
        trace_id,
        span_id: span_id("span_id")?.ok_or("span_id is missing")?,
        parent_span_id: span_id("parent_span_id")?,
        flags: u32::try_from(int64(batch, "flags", row)?.unwrap_or_default()).unwrap_or_default(),
        trace_state: text(batch, "trace_state", row)?.unwrap_or_default(),
        name: text(batch, "name", row)?.unwrap_or_default(),
        kind,
        start_time,
        end_time,
        duration_ms: u64::try_from((end_time - start_time).num_milliseconds()).unwrap_or(0),
        status,
        attributes: attribute_column(batch, "attributes", row)?,
        dropped_attributes_count: count(batch, "dropped_attributes_count", row)?,
        events: nested(batch, "events", row, event)?,
        dropped_events_count: count(batch, "dropped_events_count", row)?,
        links: nested(batch, "links", row, link)?,
        dropped_links_count: count(batch, "dropped_links_count", row)?,
        scope: InstrumentationScope {
            name: text(batch, "scope_name", row)?.unwrap_or_default(),
            version: None,
            attributes: Map::new(),
        },
        resource: Resource {
            service_name: text(batch, "service_name", row)?.unwrap_or_default(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: Map::new(),
        },
    })
}

/// Decode one persisted span event from row `row` of its element batch.
///
/// # Errors
/// Returns a description of a missing or malformed field.
fn event(items: &RecordBatch, row: usize) -> Result<SpanEvent, String> {
    Ok(SpanEvent {
        timestamp: instant(items, "time_unix_nano", row)?
            .ok_or("event time_unix_nano is missing")?,
        name: text(items, "name", row)?.unwrap_or_default(),
        attributes: attribute_column(items, "attributes", row)?,
        dropped_attributes_count: count(items, "dropped_attributes_count", row)?,
    })
}

/// Decode one persisted span link from row `row` of its element batch.
///
/// # Errors
/// Returns a description of a missing or malformed field or identifier.
fn link(items: &RecordBatch, row: usize) -> Result<SpanLink, String> {
    let trace_id = bytes(items, "trace_id", row)?.ok_or("link trace_id is missing")?;
    let span_id = bytes(items, "span_id", row)?.ok_or("link span_id is missing")?;
    Ok(SpanLink {
        trace_id: <[u8; 16]>::try_from(trace_id)
            .map_err(|_| "link trace_id is not 16 bytes".to_owned())
            .and_then(|id| TraceId::from_bytes(id).map_err(|error| error.to_string()))?,
        span_id: <[u8; 8]>::try_from(span_id)
            .map_err(|_| "link span_id is not 8 bytes".to_owned())
            .and_then(|id| SpanId::from_bytes(id).map_err(|error| error.to_string()))?,
        trace_state: text(items, "trace_state", row)?.unwrap_or_default(),
        flags: count(items, "flags", row)?,
        attributes: attribute_column(items, "attributes", row)?,
        dropped_attributes_count: count(items, "dropped_attributes_count", row)?,
    })
}

/// Decode every element of list column `name` at `row` with `decode`; a
/// null list is empty.
///
/// The list's struct elements are viewed as one batch so the scalar column
/// readers decode their fields.
///
/// # Errors
/// Returns a description when the column is absent, is not a list of
/// non-null structs, or an element does not decode.
fn nested<T>(
    batch: &RecordBatch,
    name: &str,
    row: usize,
    decode: fn(&RecordBatch, usize) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let list = batch
        .column_by_name(name)
        .ok_or_else(|| format!("column {name} is missing"))?
        .as_list_opt::<i32>()
        .ok_or_else(|| format!("column {name} is not a list"))?;
    if list.is_null(row) {
        return Ok(Vec::new());
    }
    let elements = list.value(row);
    let elements = elements
        .as_struct_opt()
        .filter(|elements| elements.null_count() == 0)
        .ok_or_else(|| format!("column {name} does not hold non-null structs"))?;
    let elements = RecordBatch::from(elements.clone());
    (0..elements.num_rows())
        .map(|element| decode(&elements, element))
        .collect()
}

/// The Variant attribute object in column `name` at `row` as JSON; a null
/// value is empty.
///
/// Bytes values render as base64 text and integers keep their exact value.
///
/// # Errors
/// Returns a description when the column is absent, is not Variant storage,
/// the value does not decode, or the value is not an object.
fn attribute_column(
    batch: &RecordBatch,
    name: &str,
    row: usize,
) -> Result<Map<String, Value>, String> {
    let column = batch
        .column_by_name(name)
        .ok_or_else(|| format!("column {name} is missing"))?;
    let ordinal = u64::try_from(row).unwrap_or(u64::MAX);
    match variant_cell_to_json(column.as_ref(), row)
        .map_err(|violation| violation.into_error(name, ordinal).to_string())?
    {
        Value::Null => Ok(Map::new()),
        Value::Object(attributes) => Ok(attributes),
        _ => Err(format!("column {name} does not hold an attribute object")),
    }
}

/// Non-negative 32-bit count in 64-bit column `name` at `row`; null is zero.
///
/// # Errors
/// Returns the column failure or a value outside `u32`.
fn count(batch: &RecordBatch, name: &str, row: usize) -> Result<u32, String> {
    u32::try_from(int64(batch, name, row)?.unwrap_or_default())
        .map_err(|_| format!("column {name} is not a 32-bit count"))
}

/// Column `name` of `batch`.
///
/// Cell readers index the column in its stored Arrow type rather than casting
/// it: a cast copies the whole column, and a span decode reads every cell.
///
/// # Errors
/// Returns a description when the column is absent.
pub fn column<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a ArrayRef, String> {
    batch
        .column_by_name(name)
        .ok_or_else(|| format!("column {name} is missing"))
}

/// Text value of column `name` at `row`, `None` when null.
///
/// # Errors
/// Returns the column failure or a column that is not text.
pub fn text(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<String>, String> {
    let column = column(batch, name)?;
    if column.is_null(row) {
        return Ok(None);
    }
    let value = match column.data_type() {
        DataType::Utf8 => column.as_string::<i32>().value(row),
        DataType::LargeUtf8 => column.as_string::<i64>().value(row),
        DataType::Utf8View => column.as_string_view().value(row),
        _ => return Err(format!("column {name} is not text")),
    };
    Ok(Some(value.to_owned()))
}

/// Byte value of column `name` at `row`, `None` when null.
///
/// # Errors
/// Returns the column failure or a column that is not binary.
pub fn bytes(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<Vec<u8>>, String> {
    let column = column(batch, name)?;
    if column.is_null(row) {
        return Ok(None);
    }
    let value = match column.data_type() {
        DataType::Binary => column.as_binary::<i32>().value(row),
        DataType::LargeBinary => column.as_binary::<i64>().value(row),
        DataType::BinaryView => column.as_binary_view().value(row),
        DataType::FixedSizeBinary(_) => column.as_fixed_size_binary().value(row),
        _ => return Err(format!("column {name} is not binary")),
    };
    Ok(Some(value.to_vec()))
}

/// 64-bit integer value of column `name` at `row`, `None` when null.
///
/// # Errors
/// Returns the column failure or a column that is not `Int64`.
fn int64(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<i64>, String> {
    let column = column(batch, name)?
        .as_primitive_opt::<Int64Type>()
        .ok_or_else(|| format!("column {name} is not a 64-bit integer"))?;
    Ok(column.is_valid(row).then(|| column.value(row)))
}

/// UTC instant of the nanosecond timestamp column `name` at `row`, `None`
/// when null.
///
/// Span start, end and event times are stored as OTLP instants
/// (`Timestamp(Nanosecond, "UTC")`), not as raw integers.
///
/// # Errors
/// Returns the column failure or a column that is not a nanosecond timestamp.
fn instant(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<DateTime<Utc>>, String> {
    let column = column(batch, name)?
        .as_primitive_opt::<TimestampNanosecondType>()
        .ok_or_else(|| format!("column {name} is not a nanosecond timestamp"))?;
    Ok(column
        .is_valid(row)
        .then(|| DateTime::<Utc>::from_timestamp_nanos(column.value(row))))
}

/// 32-bit integer value of column `name` at `row`, `None` when null.
///
/// # Errors
/// Returns the column failure or a column that is not `Int32`.
fn int32(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<i32>, String> {
    let column = column(batch, name)?
        .as_primitive_opt::<Int32Type>()
        .ok_or_else(|| format!("column {name} is not a 32-bit integer"))?;
    Ok(column.is_valid(row).then(|| column.value(row)))
}
