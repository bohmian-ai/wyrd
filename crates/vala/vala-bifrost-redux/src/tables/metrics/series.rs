//! The `series_id` column of `bifrost.metrics.points` and the
//! `bifrost.metrics.series` table indexed by it.
//!
//! A series is one metric's identity fields plus its resource, scope, and
//! point attribute sets. [`with_series_ids`] derives each row's id from the
//! canonical Arrow columns alone, so the OTLP projection and a raw Arrow
//! writer's batch reach the same id for the same series: the projection calls
//! it on the batch it assembles, and the table's validator calls it on every
//! untrusted batch, replacing whatever `series_id` the writer sent.
//!
//! The id is the first 16 bytes of SHA-256 over version byte
//! [`ENCODING_VERSION`] followed by these fields in order: `metric_name`,
//! `metric_type`, `unit`, `aggregation_temporality`, `is_monotonic`,
//! `explicit_bounds`, `resource_attributes`, `scope_name`, `scope_version`,
//! `scope_attributes`, `attributes`. Each field is a one-byte tag, a
//! big-endian `u32` byte length, and the bytes; an absent value is
//! [`Tag::Null`] with length 0. Object keys are sorted bytewise at every
//! depth, arrays keep their order, every integer width is a big-endian `i64`,
//! and floats are IEEE-754 bits with `-0.0` written as `0.0` and every NaN as
//! one canonical NaN. Changing any of this takes a new version byte.

use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, AsArray, BooleanArray, FixedSizeBinaryBuilder, Float64Array, Int32Array,
    StringArray, UInt32Array,
};
use arrow::datatypes::{Field, Schema, TimestampNanosecondType};
use arrow::record_batch::RecordBatch;
use arrow::row::{RowConverter, SortField};
use chrono::{Datelike as _, Timelike as _};
use parquet_variant::Variant;
use sha2::{Digest, Sha256};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::managed_columns::{
    ACT_PRINCIPAL_ID, RUN_ID, SUB_PRINCIPAL_ID, SUBJECT_CARD_UID, WORKLOAD_CARD_UID,
};

use super::points::EXPLICIT_BOUND_ELEMENT;
use crate::tables::fields::{
    CanonicalField, CanonicalField as F, CanonicalType as T, canonical_arrow_fields,
};
use crate::tables::signal::{self, stored_variant};
use crate::tables::{DomainTable, PayloadClass, attributed_layout, hourly_layout};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// Name of the derived series identity column.
pub const SERIES_ID: &str = "series_id";

/// Byte width of one series id.
const SERIES_ID_BYTES: i32 = 16;

/// Leading byte of the current identity encoding.
pub const ENCODING_VERSION: u8 = 1;

/// The one NaN every NaN is written as.
const CANONICAL_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;

/// One-byte type tag of an encoded identity value.
#[derive(Clone, Copy)]
#[repr(u8)]
enum Tag {
    /// An absent value; always length 0.
    Null = 0,
    /// One byte, 0 or 1.
    Bool = 1,
    /// Any integer width, as a big-endian `i64`.
    Int = 2,
    /// `f32` or `f64`, as the big-endian bits of the normalized `f64`.
    Float = 3,
    /// UTF-8 bytes.
    String = 4,
    /// Raw bytes.
    Binary = 5,
    /// The encoded elements, in order.
    Array = 6,
    /// Sorted key/value pairs, each an encoded string then an encoded value.
    Object = 7,
    /// Days since the Unix epoch, as a big-endian `i64`.
    Date = 8,
    /// UTC microseconds since the epoch.
    TimestampMicros = 9,
    /// Zone-less microseconds since the epoch.
    TimestampNtzMicros = 10,
    /// UTC nanoseconds since the epoch.
    TimestampNanos = 11,
    /// Zone-less nanoseconds since the epoch.
    TimestampNtzNanos = 12,
    /// A big-endian `i128` unscaled value followed by its one-byte scale.
    Decimal = 13,
    /// Microseconds since midnight.
    Time = 14,
    /// The 16 UUID bytes.
    Uuid = 15,
}

/// Return `batch` with its `series_id` column recomputed for every row.
///
/// `batch` is a canonical point user block: it carries every identity column
/// and a `series_id` column, whose supplied values are discarded.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when an identity column is missing or
/// not its canonical type, a Variant does not decode, or an encoded value
/// exceeds a `u32` length.
pub fn with_series_ids(batch: &RecordBatch) -> Result<RecordBatch, BifrostError> {
    let schema = batch.schema();
    let at = schema.index_of(SERIES_ID).map_err(|_| missing(SERIES_ID))?;
    let mut columns = batch.columns().to_vec();
    columns[at] = series_ids(batch)?;
    RecordBatch::try_new(schema, columns).map_err(|error| BifrostError::SchemaParse {
        detail: format!("series_id does not assemble: {error}"),
    })
}

/// Compute the series id of every row of `batch`.
///
/// # Errors
///
/// Returns the [`with_series_ids`] refusals.
fn series_ids(batch: &RecordBatch) -> Result<ArrayRef, BifrostError> {
    let text = |name: &'static str| {
        batch
            .column_by_name(name)
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| missing(name))
    };
    let column = |name: &'static str| batch.column_by_name(name).ok_or_else(|| missing(name));
    let metric_name = text("metric_name")?;
    let metric_type = text("metric_type")?;
    let unit = text("unit")?;
    let temporality = column("aggregation_temporality")?
        .as_any()
        .downcast_ref::<Int32Array>()
        .ok_or_else(|| missing("aggregation_temporality"))?;
    let monotonic = column("is_monotonic")?
        .as_any()
        .downcast_ref::<BooleanArray>()
        .ok_or_else(|| missing("is_monotonic"))?;
    let bounds = column("explicit_bounds")?
        .as_list_opt::<i32>()
        .ok_or_else(|| missing("explicit_bounds"))?;
    let resource_attributes = column("resource_attributes")?;
    let scope_name = text("scope_name")?;
    let scope_version = text("scope_version")?;
    let scope_attributes = column("scope_attributes")?;
    let attributes = column("attributes")?;

    let mut ids = FixedSizeBinaryBuilder::with_capacity(batch.num_rows(), SERIES_ID_BYTES);
    let mut encoded = Vec::new();
    for row in 0..batch.num_rows() {
        encoded.clear();
        encoded.push(ENCODING_VERSION);
        string_cell(metric_name, row, &mut encoded)?;
        string_cell(metric_type, row, &mut encoded)?;
        string_cell(unit, row, &mut encoded)?;
        if temporality.is_valid(row) {
            int(i64::from(temporality.value(row)), &mut encoded);
        } else {
            null(&mut encoded);
        }
        if monotonic.is_valid(row) {
            boolean(monotonic.value(row), &mut encoded);
        } else {
            null(&mut encoded);
        }
        if bounds.is_valid(row) {
            let values = bounds.value(row);
            let values = values
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| missing("explicit_bounds"))?;
            framed(Tag::Array, &mut encoded, |out| {
                for value in values {
                    match value {
                        Some(value) => float(value, out),
                        None => null(out),
                    }
                }
                Ok(())
            })?;
        } else {
            null(&mut encoded);
        }
        variant_cell(resource_attributes.as_ref(), row, &mut encoded)?;
        string_cell(scope_name, row, &mut encoded)?;
        string_cell(scope_version, row, &mut encoded)?;
        variant_cell(scope_attributes.as_ref(), row, &mut encoded)?;
        variant_cell(attributes.as_ref(), row, &mut encoded)?;
        let digest = Sha256::digest(&encoded);
        ids.append_value(&digest[..16])
            .map_err(|error| BifrostError::SchemaParse {
                detail: format!("series_id does not assemble: {error}"),
            })?;
    }
    Ok(Arc::new(ids.finish()))
}

/// The refusal for an identity column that is missing or mistyped.
fn missing(name: &str) -> BifrostError {
    BifrostError::SchemaParse {
        detail: format!("canonical batch lacks the {name} column series_id derives from"),
    }
}

/// Encode row `row` of a Utf8 column, null when absent.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when the text exceeds a `u32` length.
fn string_cell(column: &StringArray, row: usize, out: &mut Vec<u8>) -> Result<(), BifrostError> {
    if column.is_valid(row) {
        bytes(Tag::String, column.value(row).as_bytes(), out)
    } else {
        null(out);
        Ok(())
    }
}

/// Encode row `row` of a stored Variant column, null when absent.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when the row does not decode or a
/// value exceeds a `u32` length.
fn variant_cell(column: &dyn Array, row: usize, out: &mut Vec<u8>) -> Result<(), BifrostError> {
    if column.is_null(row) {
        null(out);
        return Ok(());
    }
    stored_variant(column, row, |value| variant(value, out))?
}

/// Encode one Variant value, recursing through objects and lists.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when a nested value does not decode
/// or a value exceeds a `u32` length.
fn variant(value: &Variant<'_, '_>, out: &mut Vec<u8>) -> Result<(), BifrostError> {
    let malformed = |_| BifrostError::SchemaParse {
        detail: "a canonical Variant column does not decode".to_owned(),
    };
    match value {
        Variant::Null => null(out),
        Variant::BooleanTrue => boolean(true, out),
        Variant::BooleanFalse => boolean(false, out),
        Variant::Int8(number) => int(i64::from(*number), out),
        Variant::Int16(number) => int(i64::from(*number), out),
        Variant::Int32(number) => int(i64::from(*number), out),
        Variant::Int64(number) => int(*number, out),
        Variant::Float(number) => float(f64::from(*number), out),
        Variant::Double(number) => float(*number, out),
        Variant::String(text) => bytes(Tag::String, text.as_bytes(), out)?,
        Variant::ShortString(text) => bytes(Tag::String, text.as_bytes(), out)?,
        Variant::Binary(raw) => bytes(Tag::Binary, raw, out)?,
        Variant::Uuid(uuid) => bytes(Tag::Uuid, uuid.as_bytes(), out)?,
        Variant::Date(date) => {
            // 719_163 is the day number of 1970-01-01 counted from 0001-01-01.
            let days = i64::from(date.num_days_from_ce()) - 719_163;
            bytes(Tag::Date, &days.to_be_bytes(), out)?;
        }
        Variant::TimestampMicros(at) => {
            bytes(
                Tag::TimestampMicros,
                &at.timestamp_micros().to_be_bytes(),
                out,
            )?;
        }
        Variant::TimestampNtzMicros(at) => bytes(
            Tag::TimestampNtzMicros,
            &at.and_utc().timestamp_micros().to_be_bytes(),
            out,
        )?,
        Variant::TimestampNanos(at) => bytes(
            Tag::TimestampNanos,
            &nanos(at.timestamp_nanos_opt())?.to_be_bytes(),
            out,
        )?,
        Variant::TimestampNtzNanos(at) => bytes(
            Tag::TimestampNtzNanos,
            &nanos(at.and_utc().timestamp_nanos_opt())?.to_be_bytes(),
            out,
        )?,
        Variant::Time(time) => {
            let micros = i64::from(time.num_seconds_from_midnight()) * 1_000_000
                + i64::from(time.nanosecond() / 1_000);
            bytes(Tag::Time, &micros.to_be_bytes(), out)?;
        }
        Variant::Decimal4(decimal) => {
            decimal_bytes(i128::from(decimal.integer()), decimal.scale(), out)?;
        }
        Variant::Decimal8(decimal) => {
            decimal_bytes(i128::from(decimal.integer()), decimal.scale(), out)?;
        }
        Variant::Decimal16(decimal) => decimal_bytes(decimal.integer(), decimal.scale(), out)?,
        Variant::List(list) => framed(Tag::Array, out, |out| {
            for element in list.iter_try() {
                variant(&element.map_err(malformed)?, out)?;
            }
            Ok(())
        })?,
        Variant::Object(object) => {
            let mut fields = object
                .iter_try()
                .collect::<Result<Vec<_>, _>>()
                .map_err(malformed)?;
            fields.sort_unstable_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
            framed(Tag::Object, out, |out| {
                for (key, value) in &fields {
                    bytes(Tag::String, key.as_bytes(), out)?;
                    variant(value, out)?;
                }
                Ok(())
            })?;
        }
    }
    Ok(())
}

/// A nanosecond instant, refused when it falls outside `i64`.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when `value` is `None`.
fn nanos(value: Option<i64>) -> Result<i64, BifrostError> {
    value.ok_or_else(|| BifrostError::SchemaParse {
        detail: "a Variant nanosecond timestamp falls outside i64".to_owned(),
    })
}

/// Encode a decimal as its unscaled value and scale.
///
/// # Errors
///
/// Never in practice; shares the [`bytes`] signature.
fn decimal_bytes(unscaled: i128, scale: u8, out: &mut Vec<u8>) -> Result<(), BifrostError> {
    let mut raw = [0_u8; 17];
    raw[..16].copy_from_slice(&unscaled.to_be_bytes());
    raw[16] = scale;
    bytes(Tag::Decimal, &raw, out)
}

/// Write the null tag with length 0.
fn null(out: &mut Vec<u8>) {
    out.push(Tag::Null as u8);
    out.extend_from_slice(&0_u32.to_be_bytes());
}

/// Write one boolean as a single 0 or 1 byte.
fn boolean(value: bool, out: &mut Vec<u8>) {
    out.push(Tag::Bool as u8);
    out.extend_from_slice(&1_u32.to_be_bytes());
    out.push(u8::from(value));
}

/// Write one integer as a big-endian `i64`.
fn int(value: i64, out: &mut Vec<u8>) {
    out.push(Tag::Int as u8);
    out.extend_from_slice(&8_u32.to_be_bytes());
    out.extend_from_slice(&value.to_be_bytes());
}

/// Write one float as the big-endian bits of its normalized value.
///
/// `-0.0` is written as `0.0` and every NaN as [`CANONICAL_NAN_BITS`], so
/// values that compare equal, or are both NaN, encode alike.
fn float(value: f64, out: &mut Vec<u8>) {
    let bits = if value.is_nan() {
        CANONICAL_NAN_BITS
    } else if value == 0.0 {
        0
    } else {
        value.to_bits()
    };
    out.push(Tag::Float as u8);
    out.extend_from_slice(&8_u32.to_be_bytes());
    out.extend_from_slice(&bits.to_be_bytes());
}

/// Write `tag`, the big-endian `u32` length of `value`, then `value`.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when `value` exceeds a `u32` length.
fn bytes(tag: Tag, value: &[u8], out: &mut Vec<u8>) -> Result<(), BifrostError> {
    out.push(tag as u8);
    out.extend_from_slice(&length(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

/// Write `tag`, then the body `write` appends, prefixed with its length.
///
/// # Errors
///
/// Returns the refusal of `write`, or [`BifrostError::SchemaParse`] when the
/// body exceeds a `u32` length.
fn framed(
    tag: Tag,
    out: &mut Vec<u8>,
    write: impl FnOnce(&mut Vec<u8>) -> Result<(), BifrostError>,
) -> Result<(), BifrostError> {
    out.push(tag as u8);
    let at = out.len();
    out.extend_from_slice(&[0; 4]);
    write(out)?;
    let body = length(out.len() - at - 4)?;
    out[at..at + 4].copy_from_slice(&body.to_be_bytes());
    Ok(())
}

/// Narrow one encoded length to the encoding's `u32`.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when `len` exceeds `u32`.
fn length(len: usize) -> Result<u32, BifrostError> {
    u32::try_from(len).map_err(|_| BifrostError::SchemaParse {
        detail: "a series identity value exceeds a u32 length".to_owned(),
    })
}

/// Name of the series table's UTC hour, its time and partition column.
pub const HOUR_UNIX_NANO: &str = "hour_unix_nano";

/// Nanoseconds in one hour.
const HOUR_NANOS: i64 = 3_600_000_000_000;

/// The `bifrost.metrics.series` ledger.
///
/// Every column but [`HOUR_UNIX_NANO`] is declared exactly as on
/// `bifrost.metrics.points`, so a series row is a copy of one point's series
/// columns: the identity fields, the attribute sets, and the promoted and
/// attribution columns. The promoted and attribution columns are the point's
/// accepted context, which its attribute sets do not fix: a native writer
/// names its own run, and Gate stamps the accepted workload's Service and
/// environment. A series has one row per hour for each [`CONTEXT_COLUMNS`]
/// context its points were admitted under.
pub static SERIES_FIELDS: &[CanonicalField] = &[
    F::meta(SERIES_ID, T::FixedSizeBinary(16), true),
    F::meta(HOUR_UNIX_NANO, signal::OTLP_INSTANT, false),
    F::meta("metric_name", T::Utf8, false),
    F::meta("metric_type", T::Utf8, false),
    F::meta("unit", T::Utf8, false),
    F::meta("aggregation_temporality", T::Int32, true),
    F::meta("is_monotonic", T::Bool, true),
    F::payload("explicit_bounds", T::List(&EXPLICIT_BOUND_ELEMENT), true),
    F::meta("scope_name", T::Utf8, false),
    F::meta("scope_version", T::Utf8, false),
    F::sensitive("resource_attributes", T::Variant, false),
    F::sensitive("scope_attributes", T::Variant, false),
    F::sensitive("attributes", T::Variant, false),
    F::meta("service_name", T::Utf8, true),
    F::meta("service_version", T::Utf8, true),
    F::meta("deployment_environment", T::Utf8, true),
    F::meta("service_namespace", T::Utf8, true),
    F::meta("service_instance_id", T::Utf8, true),
    F::meta("process_pid", T::Int64, true),
    F::meta("telemetry_sdk_name", T::Utf8, true),
    F::meta("telemetry_sdk_language", T::Utf8, true),
    F::meta("telemetry_sdk_version", T::Utf8, true),
    F::meta("subject_kind", T::Utf8, true),
    F::meta("subject_name", T::Utf8, true),
    F::meta("subject_version", T::Utf8, true),
];

/// `bifrost.metrics.series`: one row per series per UTC hour it has points in.
///
/// Scribe alone writes it, deriving each row from an admitted points batch
/// with [`series_rows`] and making it durable before those points; no client
/// may write it. Rows repeat across replicas and restarts, so a reader
/// aggregates by `series_id` and never counts rows as series.
pub struct SeriesTable;

impl DomainTable for SeriesTable {
    const NAMESPACE: &'static str = "metrics";
    const NAME: &'static str = "series";
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] =
        &["resource_attributes", "scope_attributes", "attributes"];

    fn canonical_fields() -> Option<&'static [CanonicalField]> {
        Some(SERIES_FIELDS)
    }

    fn arrow_fields() -> Vec<Field> {
        canonical_arrow_fields(SERIES_FIELDS)
    }

    /// Hourly on [`HOUR_UNIX_NANO`], with Blooms on the attribution defaults,
    /// `series_id`, and `metric_name`.
    fn physical_layout() -> PhysicalLayoutWire {
        attributed_layout(hourly_layout, HOUR_UNIX_NANO, &[SERIES_ID, "metric_name"])
    }
}

/// The stamped point columns a series row's context is: the promoted
/// Service, environment, SDK and subject columns, then the Card, run, workload
/// and principal correlation. They can vary for one `series_id`, so each
/// distinct context is its own series row; the request id and admission
/// instant are per admission and never part of it.
pub const CONTEXT_COLUMNS: &[&str] = &[
    "service_name",
    "service_version",
    "deployment_environment",
    "service_namespace",
    "service_instance_id",
    "process_pid",
    "telemetry_sdk_name",
    "telemetry_sdk_language",
    "telemetry_sdk_version",
    "subject_kind",
    "subject_name",
    "subject_version",
    SUBJECT_CARD_UID,
    RUN_ID,
    WORKLOAD_CARD_UID,
    SUB_PRINCIPAL_ID,
    ACT_PRINCIPAL_ID,
];

/// One series row's identity: its UTC hour in Unix nanoseconds, its id, and
/// the first 16 bytes of SHA-256 over its [`CONTEXT_COLUMNS`] values in Arrow
/// row encoding.
pub type SeriesKey = (i64, [u8; 16], [u8; 16]);

/// Derive the series rows of one admitted points batch.
///
/// `points` is a stamped `bifrost.metrics.points` batch whose `series_id`
/// Scribe derived at admission. The first row of each [`SeriesKey`] for which
/// `wanted` holds becomes one series row: the [`SERIES_FIELDS`] columns
/// copied from that point with their point fields, its hour, then the
/// `carried` columns copied the same way. `wanted` is asked once per distinct
/// key, in row order. Returns `None` when no row is wanted, else the rows and
/// their keys in row order.
///
/// # Errors
///
/// Returns [`BifrostError::SchemaParse`] when `points` lacks a copied or
/// context column, `time_unix_nano` is not a nanosecond timestamp, a
/// `series_id` is null, the context does not encode, or the rows do not
/// assemble.
pub fn series_rows(
    points: &RecordBatch,
    carried: &[&str],
    mut wanted: impl FnMut(SeriesKey) -> bool,
) -> Result<Option<(RecordBatch, Vec<SeriesKey>)>, BifrostError> {
    let column = |name: &str| {
        points
            .schema()
            .index_of(name)
            .map(|at| (points.schema().field(at).clone(), points.column(at)))
            .map_err(|_| missing(name))
    };
    let times = column("time_unix_nano")?
        .1
        .as_primitive_opt::<TimestampNanosecondType>()
        .ok_or_else(|| missing("time_unix_nano"))?
        .clone();
    let ids = column(SERIES_ID)?
        .1
        .as_fixed_size_binary_opt()
        .ok_or_else(|| missing(SERIES_ID))?
        .clone();
    let context = CONTEXT_COLUMNS
        .iter()
        .map(|name| column(name).map(|(_, values)| Arc::clone(values)))
        .collect::<Result<Vec<_>, _>>()?;
    let unencodable = |error: arrow::error::ArrowError| BifrostError::SchemaParse {
        detail: format!("series context does not encode: {error}"),
    };
    let contexts = RowConverter::new(
        context
            .iter()
            .map(|values| SortField::new(values.data_type().clone()))
            .collect(),
    )
    .and_then(|converter| converter.convert_columns(&context))
    .map_err(unencodable)?;
    let mut asked = std::collections::HashSet::new();
    let (mut rows, mut keys, mut hours) = (Vec::new(), Vec::new(), Vec::new());
    for row in 0..points.num_rows() {
        let id: [u8; 16] = ids
            .is_valid(row)
            .then(|| ids.value(row).try_into().ok())
            .flatten()
            .ok_or_else(|| missing(SERIES_ID))?;
        let hour = times.value(row) - times.value(row).rem_euclid(HOUR_NANOS);
        let digest: [u8; 32] = Sha256::digest(contexts.row(row).as_ref()).into();
        let mut context = [0; 16];
        context.copy_from_slice(&digest[..16]);
        let key = (hour, id, context);
        if asked.insert(key) && wanted(key) {
            rows.push(u32::try_from(row).map_err(|_| missing(SERIES_ID))?);
            keys.push(key);
            hours.push(hour);
        }
    }
    if rows.is_empty() {
        return Ok(None);
    }
    let rows = UInt32Array::from(rows);
    let mut fields = Vec::with_capacity(SERIES_FIELDS.len() + carried.len());
    let mut columns = Vec::with_capacity(fields.capacity());
    for name in SERIES_FIELDS
        .iter()
        .map(|field| field.name)
        .chain(carried.iter().copied())
    {
        if name == HOUR_UNIX_NANO {
            fields.push(SERIES_FIELDS[1].to_arrow());
            columns.push(signal::otlp_instant_column(std::mem::take(&mut hours)));
            continue;
        }
        let (field, values) = column(name)?;
        columns.push(
            arrow::compute::take(values.as_ref(), &rows, None).map_err(|error| {
                BifrostError::SchemaParse {
                    detail: format!("series column {name} does not assemble: {error}"),
                }
            })?,
        );
        fields.push(field);
    }
    let batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).map_err(|error| {
        BifrostError::SchemaParse {
            detail: format!("series rows do not assemble: {error}"),
        }
    })?;
    Ok(Some((batch, keys)))
}

#[cfg(test)]
mod tests {
    use arrow::array::{Array, FixedSizeBinaryArray};
    use arrow::record_batch::RecordBatch;
    use serde_json::json;
    use std::sync::Arc;
    use wyrd_queue::variant::EncodedVariant;
    use wyrd_tonic::otlp::common::v1::any_value::Value;
    use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope, KeyValue, KeyValueList};
    use wyrd_tonic::otlp::metrics::v1::{
        Histogram, HistogramDataPoint, Metric, ResourceMetrics, ScopeMetrics, metric,
    };
    use wyrd_tonic::otlp::resource::v1::Resource;

    use super::SERIES_ID;
    use crate::tables::metrics::{project_resource_metrics, validate_metric_points};
    use crate::tables::signal::{variant_column, without_correlation_columns};

    /// The id of [`point`] with [`fixture_attributes`] and bounds `[0, 5, 10]`,
    /// computed independently from the documented encoding.
    const GOLDEN_SERIES_ID: &str = "f977aa8a0076fb2a48aab2815440a3b6";

    /// Build one attribute entry.
    fn attribute(key: &str, value: Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// Point attributes with a nested object, in the supplied key order.
    fn fixture_attributes(reversed: bool) -> Vec<KeyValue> {
        let mut nested = vec![
            attribute("rack", Value::IntValue(3)),
            attribute("zone", Value::StringValue("a".to_owned())),
        ];
        let mut attributes = vec![
            attribute("code", Value::IntValue(200)),
            attribute("http.route", Value::StringValue("/pay".to_owned())),
            attribute(
                "placement",
                Value::KvlistValue(KeyValueList {
                    values: nested.clone(),
                }),
            ),
        ];
        if reversed {
            nested.reverse();
            attributes.reverse();
            attributes[0] = attribute(
                "placement",
                Value::KvlistValue(KeyValueList { values: nested }),
            );
        }
        attributes
    }

    /// One cumulative histogram point with the supplied attributes and bounds.
    ///
    /// # Panics
    ///
    /// Panics when the bucket count does not fit `u64`.
    fn point(attributes: Vec<KeyValue>, explicit_bounds: Vec<f64>) -> Vec<ResourceMetrics> {
        let bucket_counts = vec![1; explicit_bounds.len() + 1];
        let count = u64::try_from(bucket_counts.len()).expect("small bucket count");
        vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![attribute(
                    "service.name",
                    Value::StringValue("checkout".to_owned()),
                )],
                ..Resource::default()
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "meter".to_owned(),
                    version: "1.0".to_owned(),
                    attributes: vec![attribute("lib", Value::BoolValue(true))],
                    ..InstrumentationScope::default()
                }),
                metrics: vec![Metric {
                    name: "http.server.duration".to_owned(),
                    description: "excluded from the id".to_owned(),
                    unit: "ms".to_owned(),
                    metadata: Vec::new(),
                    data: Some(metric::Data::Histogram(Histogram {
                        data_points: vec![HistogramDataPoint {
                            attributes,
                            start_time_unix_nano: 10,
                            time_unix_nano: 20,
                            count,
                            sum: Some(12.5),
                            bucket_counts,
                            explicit_bounds,
                            ..HistogramDataPoint::default()
                        }],
                        aggregation_temporality: 2,
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }]
    }

    /// Project `request` and return its one accepted user block.
    ///
    /// # Panics
    ///
    /// Panics when the projection fails, rejects a point, or cannot split off
    /// its correlation columns.
    fn projected(request: &[ResourceMetrics]) -> RecordBatch {
        let (batch, outcome) =
            project_resource_metrics(request, None, usize::MAX).expect("the fixture projects");
        assert_eq!(outcome.rejected_points, 0, "{outcome:?}");
        without_correlation_columns(&batch).expect("correlation columns split off")
    }

    /// The hex series id of row 0 of `batch`.
    ///
    /// # Panics
    ///
    /// Panics when `batch` has no `FixedSizeBinary` `series_id` or row 0's is
    /// null.
    fn id(batch: &RecordBatch) -> String {
        let ids = batch
            .column_by_name(SERIES_ID)
            .and_then(|column| column.as_any().downcast_ref::<FixedSizeBinaryArray>())
            .expect("series_id is FixedSizeBinary(16)");
        assert!(ids.is_valid(0), "every point carries a series_id");
        hex::encode(ids.value(0))
    }

    /// The hex series id the OTLP projection derives for `request`.
    ///
    /// # Panics
    ///
    /// Panics as [`projected`] and [`id`] do.
    fn otlp_id(request: &[ResourceMetrics]) -> String {
        id(&projected(request))
    }

    /// `batch` with column `name` replaced by `column`.
    ///
    /// # Panics
    ///
    /// Panics when `batch` has no column `name` or `column` does not fit its
    /// schema.
    fn replaced(batch: &RecordBatch, name: &str, column: Arc<dyn Array>) -> RecordBatch {
        let at = batch.schema().index_of(name).expect("column exists");
        let mut columns = batch.columns().to_vec();
        columns[at] = column;
        RecordBatch::try_new(batch.schema(), columns).expect("replacement keeps the schema")
    }

    /// The fixture point's encoding is pinned, so a change to the encoding can
    /// never silently re-identify stored series.
    ///
    /// # Panics
    ///
    /// Panics when the fixture's id differs from the pinned one.
    #[test]
    fn golden_series_id_is_pinned() {
        assert_eq!(
            otlp_id(&point(fixture_attributes(false), vec![0.0, 5.0, 10.0])),
            GOLDEN_SERIES_ID
        );
    }

    /// Attribute keys in another order, at every depth, are the same series.
    ///
    /// # Panics
    ///
    /// Panics when reordered keys reach another id.
    #[test]
    fn attribute_key_order_keeps_the_series_id() {
        assert_eq!(
            otlp_id(&point(fixture_attributes(false), vec![0.0, 5.0])),
            otlp_id(&point(fixture_attributes(true), vec![0.0, 5.0]))
        );
    }

    /// A changed attribute value or a changed bound is a different series.
    ///
    /// # Panics
    ///
    /// Panics when a changed value or bound keeps the id.
    #[test]
    fn changed_attribute_value_or_bound_changes_the_series_id() {
        let base = otlp_id(&point(fixture_attributes(false), vec![0.0, 5.0]));
        let mut attributes = fixture_attributes(false);
        attributes[0] = attribute("code", Value::IntValue(500));
        assert_ne!(base, otlp_id(&point(attributes, vec![0.0, 5.0])));
        assert_ne!(
            base,
            otlp_id(&point(fixture_attributes(false), vec![0.0, 6.0]))
        );
    }

    /// `-0.0` and `0.0` encode alike, in a bound and in an attribute.
    ///
    /// # Panics
    ///
    /// Panics when `-0.0` and `0.0` reach different ids.
    #[test]
    fn negative_zero_and_zero_share_the_series_id() {
        assert_eq!(
            otlp_id(&point(fixture_attributes(false), vec![-0.0, 5.0])),
            otlp_id(&point(fixture_attributes(false), vec![0.0, 5.0]))
        );
        let with = |zero: f64| {
            let mut attributes = fixture_attributes(false);
            attributes.push(attribute("ratio", Value::DoubleValue(zero)));
            otlp_id(&point(attributes, vec![1.0]))
        };
        assert_eq!(with(-0.0), with(0.0));
    }

    /// A raw Arrow writer that builds the same point from JSON, keys in
    /// another order and with a forged `series_id`, is admitted with the id
    /// the OTLP projection derives.
    ///
    /// # Panics
    ///
    /// Panics when the raw point fails validation or keeps its forged id.
    #[test]
    fn raw_arrow_point_gets_the_otlp_series_id_and_loses_its_own() {
        let otlp = projected(&point(fixture_attributes(false), vec![0.0, 5.0, 10.0]));
        let attributes = EncodedVariant::from_json(&json!({
            "placement": {"zone": "a", "rack": 3},
            "http.route": "/pay",
            "code": 200,
        }))
        .expect("the attributes encode");
        let raw = replaced(
            &otlp,
            "attributes",
            variant_column([Some(&attributes)].into_iter()),
        );
        let forged =
            FixedSizeBinaryArray::try_from_iter([[0xab_u8; 16]].into_iter()).expect("a forged id");
        let raw = replaced(&raw, SERIES_ID, Arc::new(forged));

        let admitted = validate_metric_points(&raw).expect("the raw point validates");

        assert_eq!(id(&admitted), id(&otlp));
        assert_ne!(id(&admitted), "ab".repeat(16));
    }
}
