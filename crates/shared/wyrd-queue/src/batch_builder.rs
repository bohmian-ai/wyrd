//! JSON rows → user-only `RecordBatch` (+ per-row correlation) → Arrow IPC.
//!
//! The Wyrd-native `DynamicBatchBuilder` analog, minus server-stamped column
//! injection. The client batch carries **user columns plus the two per-row
//! correlation columns `card_ref` and `run_id`** (both client-supplied); the
//! server stamps the per-request system columns. Rows are appended as deferred
//! `serde_json::Value` (parse-on-build) and driven through one typed Arrow
//! builder per column at [`finish`](BatchBuilder::finish).

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Date32Array, Float32Array, Float64Array, Int8Array, Int16Array,
    Int32Array, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray, UInt8Array,
    UInt16Array, UInt32Array, UInt64Array,
};
use arrow::ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema, SchemaRef, TimeUnit};
use serde_json::{Map, Value};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

use crate::error::WyrdQueueError;

/// Reserved per-row correlation column carrying the client's card reference.
pub const CARD_REF_COLUMN: &str = "card_ref";
/// Reserved per-row correlation column carrying the client's run identifier.
pub const RUN_ID_COLUMN: &str = "run_id";
const RESERVED_PREFIX: &str = "wyrd_";

/// Returns true when `name` is a reserved column (server-stamped `wyrd_*` prefix
/// or one of the two correlation columns presented as a payload key).
#[must_use]
pub fn is_reserved_column(name: &str) -> bool {
    name.starts_with(RESERVED_PREFIX) || name == CARD_REF_COLUMN || name == RUN_ID_COLUMN
}

struct BuiltRow {
    obj: Map<String, Value>,
    card_ref: String,
    run_id: Option<String>,
}

/// Accumulates JSON rows against a resolved user-only Arrow schema and seals them
/// into one Arrow IPC stream.
pub struct BatchBuilder {
    schema: SchemaRef,
    rows: Vec<BuiltRow>,
}

impl BatchBuilder {
    /// Construct a builder over the resolved user-only Arrow schema.
    #[must_use]
    pub fn new(schema: SchemaRef) -> Self {
        Self {
            schema,
            rows: Vec::new(),
        }
    }

    /// Number of rows buffered so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether any rows are buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Append one serialized JSON row plus its per-row correlation.
    ///
    /// # Errors
    /// - [`WyrdQueueError::SchemaParse`] if `json` is not a JSON object.
    /// - [`WyrdQueueError::ReservedColumn`] if the row carries a `wyrd_*` key or a
    ///   `card_ref`/`run_id` key (those are supplied via the arguments, never the
    ///   payload).
    pub fn append_json_row(
        &mut self,
        json: &str,
        card_ref: &CardRef,
        run_id: Option<&RunId>,
    ) -> Result<(), WyrdQueueError> {
        let value: Value = serde_json::from_str(json)
            .map_err(|e| WyrdQueueError::SchemaParse(format!("row is not valid JSON: {e}")))?;
        let Value::Object(obj) = value else {
            return Err(WyrdQueueError::SchemaParse(
                "row is not a JSON object".to_owned(),
            ));
        };
        for key in obj.keys() {
            if is_reserved_column(key) {
                return Err(WyrdQueueError::ReservedColumn(format!(
                    "payload key `{key}` is reserved"
                )));
            }
        }
        self.rows.push(BuiltRow {
            obj,
            card_ref: card_ref.to_string(),
            run_id: run_id.map(|r| r.as_str().to_owned()),
        });
        Ok(())
    }

    /// Drive one typed Arrow builder per user column plus the two reserved
    /// correlation columns, producing one `RecordBatch`.
    ///
    /// # Errors
    /// [`WyrdQueueError::SchemaParse`] when a value's JSON shape does not match its
    /// column's type, a null/absent value lands on a non-nullable column, or a
    /// column type is unsupported by the builder.
    pub fn finish(&self) -> Result<RecordBatch, WyrdQueueError> {
        let mut columns: Vec<ArrayRef> = Vec::with_capacity(self.schema.fields().len() + 2);
        for field in self.schema.fields() {
            columns.push(build_column(field, &self.rows)?);
        }
        // Reserved correlation columns: card_ref (non-null Utf8), run_id (nullable Utf8).
        columns.push(Arc::new(StringArray::from_iter(
            self.rows.iter().map(|r| Some(r.card_ref.clone())),
        )));
        columns.push(Arc::new(StringArray::from_iter(
            self.rows.iter().map(|r| r.run_id.clone()),
        )));

        let batch = RecordBatch::try_new(self.output_schema(), columns).map_err(|e| {
            WyrdQueueError::SchemaParse(format!("record batch assembly failed: {e}"))
        })?;
        Ok(batch)
    }

    /// The sealed batch schema: user fields followed by the two correlation columns.
    #[must_use]
    pub fn output_schema(&self) -> SchemaRef {
        let mut fields: Vec<Field> = self
            .schema
            .fields()
            .iter()
            .map(|f| f.as_ref().clone())
            .collect();
        fields.push(Field::new(CARD_REF_COLUMN, DataType::Utf8, false));
        fields.push(Field::new(RUN_ID_COLUMN, DataType::Utf8, true));
        Arc::new(Schema::new(fields))
    }

    /// Seal buffered rows into Arrow IPC stream bytes over the correlation-carrying
    /// output schema.
    ///
    /// # Errors
    /// Propagates [`finish`](Self::finish) errors or an IPC encode failure.
    pub fn finish_ipc(&self) -> Result<Vec<u8>, WyrdQueueError> {
        let batch = self.finish()?;
        let mut buffer = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut buffer, batch.schema_ref())
                .map_err(|e| WyrdQueueError::SchemaParse(format!("IPC writer init failed: {e}")))?;
            writer
                .write(&batch)
                .map_err(|e| WyrdQueueError::SchemaParse(format!("IPC write failed: {e}")))?;
            writer
                .finish()
                .map_err(|e| WyrdQueueError::SchemaParse(format!("IPC finish failed: {e}")))?;
        }
        Ok(buffer)
    }
}

fn collect<T>(
    name: &str,
    rows: &[BuiltRow],
    nullable: bool,
    parse: impl Fn(&Value) -> Option<T>,
) -> Result<Vec<Option<T>>, WyrdQueueError> {
    let mut out = Vec::with_capacity(rows.len());
    for (idx, row) in rows.iter().enumerate() {
        match row.obj.get(name) {
            None | Some(Value::Null) => {
                if nullable {
                    out.push(None);
                } else {
                    return Err(WyrdQueueError::SchemaParse(format!(
                        "row {idx} field `{name}`: null/absent value on a non-nullable column"
                    )));
                }
            }
            Some(value) => {
                let parsed = parse(value).ok_or_else(|| {
                    WyrdQueueError::SchemaParse(format!(
                        "row {idx} field `{name}`: value does not match the column type"
                    ))
                })?;
                out.push(Some(parsed));
            }
        }
    }
    Ok(out)
}

fn build_column(field: &Field, rows: &[BuiltRow]) -> Result<ArrayRef, WyrdQueueError> {
    let name = field.name();
    let nullable = field.is_nullable();
    let array: ArrayRef = match field.data_type() {
        DataType::Boolean => Arc::new(BooleanArray::from(collect(
            name,
            rows,
            nullable,
            Value::as_bool,
        )?)),
        DataType::Int8 => Arc::new(Int8Array::from(collect(name, rows, nullable, |v| {
            v.as_i64().and_then(|n| i8::try_from(n).ok())
        })?)),
        DataType::Int16 => Arc::new(Int16Array::from(collect(name, rows, nullable, |v| {
            v.as_i64().and_then(|n| i16::try_from(n).ok())
        })?)),
        DataType::Int32 => Arc::new(Int32Array::from(collect(name, rows, nullable, |v| {
            v.as_i64().and_then(|n| i32::try_from(n).ok())
        })?)),
        DataType::Int64 => Arc::new(Int64Array::from(collect(
            name,
            rows,
            nullable,
            Value::as_i64,
        )?)),
        DataType::UInt8 => Arc::new(UInt8Array::from(collect(name, rows, nullable, |v| {
            v.as_u64().and_then(|n| u8::try_from(n).ok())
        })?)),
        DataType::UInt16 => Arc::new(UInt16Array::from(collect(name, rows, nullable, |v| {
            v.as_u64().and_then(|n| u16::try_from(n).ok())
        })?)),
        DataType::UInt32 => Arc::new(UInt32Array::from(collect(name, rows, nullable, |v| {
            v.as_u64().and_then(|n| u32::try_from(n).ok())
        })?)),
        DataType::UInt64 => Arc::new(UInt64Array::from(collect(
            name,
            rows,
            nullable,
            Value::as_u64,
        )?)),
        DataType::Float32 => Arc::new(Float32Array::from(collect(name, rows, nullable, |v| {
            v.as_f64().map(|n| n as f32)
        })?)),
        DataType::Float64 => Arc::new(Float64Array::from(collect(
            name,
            rows,
            nullable,
            Value::as_f64,
        )?)),
        DataType::Utf8 | DataType::LargeUtf8 => {
            Arc::new(StringArray::from(collect(name, rows, nullable, |v| {
                v.as_str().map(str::to_owned)
            })?))
        }
        DataType::Date32 => Arc::new(Date32Array::from(collect(name, rows, nullable, |v| {
            v.as_str().and_then(parse_date_days)
        })?)),
        DataType::Timestamp(TimeUnit::Microsecond, tz) => {
            let values = collect(name, rows, nullable, |v| {
                v.as_str().and_then(parse_rfc3339_micros)
            })?;
            Arc::new(TimestampMicrosecondArray::from(values).with_timezone_opt(tz.clone()))
        }
        other => {
            return Err(WyrdQueueError::SchemaParse(format!(
                "batch builder does not support column type {other:?} for field `{name}`"
            )));
        }
    };
    Ok(array)
}

/// Days since the Unix epoch for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parse a `YYYY-MM-DD` date into days since the Unix epoch.
fn parse_date_days(text: &str) -> Option<i32> {
    let (date, _) = text.split_once(['T', ' ']).unwrap_or((text, ""));
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    i32::try_from(days_from_civil(year, month, day)).ok()
}

/// Parse an RFC 3339 timestamp into microseconds since the Unix epoch.
///
/// Supports `YYYY-MM-DDThh:mm:ss[.frac][Z|±hh:mm]` (also a space date/time
/// separator). Dependency-free: `chrono` is not a permitted dependency here.
fn parse_rfc3339_micros(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once(['T', ' '])?;
    let mut dparts = date.split('-');
    let year: i64 = dparts.next()?.parse().ok()?;
    let month: i64 = dparts.next()?.parse().ok()?;
    let day: i64 = dparts.next()?.parse().ok()?;
    if dparts.next().is_some() {
        return None;
    }

    // Split trailing timezone from the time-of-day.
    let (time, offset_secs) = if let Some(stripped) = rest.strip_suffix('Z') {
        (stripped, 0_i64)
    } else if let Some(idx) = rest.rfind(['+', '-']) {
        let (t, off) = rest.split_at(idx);
        (t, parse_offset_secs(off)?)
    } else {
        (rest, 0_i64)
    };

    let (hms, frac) = match time.split_once('.') {
        Some((hms, frac)) => (hms, frac),
        None => (time, ""),
    };
    let mut tparts = hms.split(':');
    let hour: i64 = tparts.next()?.parse().ok()?;
    let minute: i64 = tparts.next()?.parse().ok()?;
    let second: i64 = tparts.next()?.parse().ok()?;
    if tparts.next().is_some()
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }

    let micros_frac = parse_fraction_micros(frac)?;
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_secs;
    Some(secs * 1_000_000 + micros_frac)
}

fn parse_offset_secs(offset: &str) -> Option<i64> {
    let (sign, body) = offset.split_at(1);
    let sign = match sign {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let mut parts = body.split(':');
    let hours: i64 = parts.next()?.parse().ok()?;
    let minutes: i64 = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

fn parse_fraction_micros(frac: &str) -> Option<i64> {
    if frac.is_empty() {
        return Some(0);
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut digits = frac.to_owned();
    digits.truncate(6);
    while digits.len() < 6 {
        digits.push('0');
    }
    digits.parse().ok()
}

#[cfg(test)]
mod batch_builder_tests {
    //! `BatchBuilder` proof: user cols + `card_ref`/`run_id`, reserved/type-mismatch
    //! rejection, and Arrow IPC round-trip.

    use std::sync::Arc;

    use crate::BatchBuilder;
    use arrow::array::{Array, Int64Array, StringArray};
    use arrow::ipc::reader::StreamReader;
    use arrow_schema::{DataType, Field, Schema};
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
}
