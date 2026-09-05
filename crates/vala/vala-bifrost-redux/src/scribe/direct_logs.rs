//! Exact two-pass projection of typed OTLP logs into one Arrow batch.
//!
//! The first pass retains only fixed counters and exact Arrow buffer facts.
//! The second pass repeats validation and writes accepted records directly
//! from borrowed prost nodes without constructing log DTOs or result vectors.

use std::sync::Arc;

use arrow::array::{ArrayRef, FixedSizeBinaryArray};
use arrow::buffer::{BooleanBuffer, Buffer, NullBuffer};
use arrow::datatypes::{Field, Int64Type, Schema, TimestampMicrosecondType};
use arrow::record_batch::RecordBatch;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::managed_columns::{CARD_UID, PRINCIPAL_ID, RUN_ID};
use wyrd_tonic::otlp::common::v1::{KeyValue, any_value};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs, SeverityNumber};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;

use super::direct_metrics::{ExactPrimitive, NullableText};
use super::direct_traces::{ByteCounter, ExactStringColumn, write_any, write_attributes};
use super::otlp_managed::OtlpProjection;
use crate::contracts::ScribeError;
use crate::otlp_contract::LogsOutcome;
use crate::tables::{DomainTable, RecordsTable};

/// Exact nullable UTF-8 capacity for one log column.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TextCapacity {
    /// Total present value bytes.
    bytes: usize,
    /// Whether any accepted row is null.
    has_null: bool,
}

/// Exact variable-width facts for log projection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LogPlan {
    /// Accepted log rows.
    accepted: usize,
    /// Rejected log rows.
    rejected: usize,
    /// Traversal ordinal of the first rejected record.
    first_rejection: Option<usize>,
    /// Severity text.
    severity_text: TextCapacity,
    /// Opaque log bodies.
    body: TextCapacity,
    /// Opaque record attributes.
    attributes: TextCapacity,
    /// Resource service names.
    service_name: TextCapacity,
    /// Instrumentation scope names.
    scope_name: TextCapacity,
    /// Instrumentation scope versions.
    scope_version: TextCapacity,
}

/// Exact fixed-size-binary values with optional packed validity.
#[derive(Debug)]
struct ExactFixedBinary {
    /// Fixed-width values including zero bytes in null slots.
    values: Vec<u8>,
    /// Packed validity bits when any row is null.
    validity: Option<Vec<u8>>,
    /// Element byte width.
    width: usize,
    /// Planned rows.
    rows: usize,
    /// Appended rows.
    len: usize,
}

impl ExactFixedBinary {
    /// Allocates exact fixed values and optional validity capacity.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when value bytes overflow.
    fn new(rows: usize, width: usize, has_null: bool) -> Result<Self, ScribeError> {
        let bytes = rows.checked_mul(width).ok_or(ScribeError::InvalidFrame)?;
        Ok(Self {
            values: vec![0_u8; bytes],
            validity: has_null.then(|| vec![0_u8; rows.saturating_add(7) / 8]),
            width,
            rows,
            len: 0,
        })
    }

    /// Appends one optional exact-width value.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on width, nullability, or row divergence.
    fn append(&mut self, value: Option<&[u8]>) -> Result<(), ScribeError> {
        if self.len == self.rows
            || value.is_some_and(|value| value.len() != self.width)
            || (value.is_none() && self.validity.is_none())
        {
            return Err(ScribeError::InvalidFrame);
        }
        if let Some(value) = value {
            let start = self.len * self.width;
            self.values[start..start + self.width].copy_from_slice(value);
            if let Some(validity) = &mut self.validity {
                validity[self.len / 8] |= 1 << (self.len % 8);
            }
        }
        self.len += 1;
        Ok(())
    }

    /// Freezes exact buffers into one fixed-size-binary array.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on row or Arrow divergence.
    fn finish(self) -> Result<FixedSizeBinaryArray, ScribeError> {
        if self.len != self.rows {
            return Err(ScribeError::InvalidFrame);
        }
        let nulls = self
            .validity
            .map(|bits| NullBuffer::new(BooleanBuffer::new(Buffer::from(bits), 0, self.rows)));
        FixedSizeBinaryArray::try_new(
            i32::try_from(self.width).map_err(|_| ScribeError::InvalidFrame)?,
            Buffer::from(self.values),
            nulls,
        )
        .map_err(|_| ScribeError::InvalidFrame)
    }
}

impl OtlpProjection<'_> {
    /// Counts final logs Arrow and IPC material without allocating row buffers.
    ///
    /// # Errors
    ///
    /// Returns validation, schema, checked-size, or configured material refusal.
    pub(crate) fn measure_logs(
        &self,
        request: &ExportLogsServiceRequest,
        material_limit: usize,
    ) -> Result<Option<super::otlp_managed::OtlpManagedMaterialPlan>, ScribeError> {
        let plan = plan(request)?;
        if plan.accepted == 0 {
            return Ok(None);
        }
        let managed = self.managed(write_schema(), plan.accepted)?;
        plan.material_plan(request, material_limit, &managed)
            .map(Some)
    }
    /// Projects one typed logs request through allocation-free planning and exact
    /// fixed-capacity Arrow construction and authoritative managed stamping. The
    /// immutable caller context is planned before source materialization so the
    /// limit covers the final persisted schema and IPC frame.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on checked arithmetic, serialization,
    /// Arrow construction, fingerprint or managed-context validation, material
    /// refusal, or pass divergence.
    pub(crate) fn project_logs(
        &self,
        request: &ExportLogsServiceRequest,
        material_limit: usize,
    ) -> Result<
        (
            Option<crate::scribe::otlp_managed::OtlpManagedBatch>,
            LogsOutcome,
        ),
        ScribeError,
    > {
        let plan = plan(request)?;
        let outcome = LogsOutcome {
            accepted_records: i64::try_from(plan.accepted).unwrap_or(i64::MAX),
            rejected_records: i64::try_from(plan.rejected).unwrap_or(i64::MAX),
            rejection_message: plan
                .first_rejection
                .map(|ordinal| rejection_reason(request, ordinal))
                .transpose()?,
        };
        if plan.accepted == 0 {
            return Ok((None, outcome));
        }
        let managed = self.managed(write_schema(), plan.accepted)?;
        let material = plan.material_plan(request, material_limit, &managed)?;
        let batch = materialize(request, plan)?;
        Ok((Some(managed.finish(batch, material)?), outcome))
    }
}

impl LogPlan {
    /// Refuses an oversized log slice from exact borrowed-source buffer facts.
    ///
    /// # Errors
    ///
    /// Returns a stable material refusal above `material_limit`, or invalid-frame
    /// when the count walk and canonical schema disagree.
    fn material_plan(
        &self,
        request: &ExportLogsServiceRequest,
        material_limit: usize,
        managed: &crate::scribe::otlp_managed::OtlpManagedProjection,
    ) -> Result<crate::scribe::otlp_managed::OtlpManagedMaterialPlan, ScribeError> {
        use crate::scribe::fixed_ipc::FixedIpcColumnPlan;

        let rows = self.accepted;
        let nulls = material_nulls(request)?;
        let bitmap = rows.checked_add(7).ok_or(ScribeError::InvalidFrame)? / 8;
        let offsets = rows
            .checked_add(1)
            .and_then(|value| value.checked_mul(4))
            .ok_or(ScribeError::InvalidFrame)?;
        let fixed = |width: usize, index: usize| -> Result<FixedIpcColumnPlan, ScribeError> {
            Ok(FixedIpcColumnPlan {
                null_count: nulls[index],
                validity_bytes: usize::from(nulls[index] != 0) * bitmap,
                offsets_bytes: 0,
                values_bytes: rows.checked_mul(width).ok_or(ScribeError::InvalidFrame)?,
            })
        };
        let utf8 = |bytes: usize, index: usize| FixedIpcColumnPlan {
            null_count: nulls[index],
            validity_bytes: usize::from(nulls[index] != 0) * bitmap,
            offsets_bytes: offsets,
            values_bytes: bytes,
        };
        let columns = [
            fixed(8, 0)?,
            fixed(8, 1)?,
            fixed(8, 2)?,
            utf8(self.severity_text.bytes, 3),
            utf8(0, 4),
            utf8(self.body.bytes, 5),
            fixed(16, 6)?,
            fixed(8, 7)?,
            fixed(8, 8)?,
            utf8(self.attributes.bytes, 9),
            fixed(8, 10)?,
            utf8(self.service_name.bytes, 11),
            utf8(self.scope_name.bytes, 12),
            utf8(self.scope_version.bytes, 13),
            utf8(0, 14),
            utf8(0, 15),
            utf8(0, 16),
        ];
        let final_plan = managed.material_plan(columns.into_iter())?;
        final_plan.admitted_bytes(material_limit)?;
        Ok(final_plan)
    }
}

/// Counts nulls for every accepted logs schema column.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on checked counter overflow.
fn material_nulls(request: &ExportLogsServiceRequest) -> Result<[usize; 17], ScribeError> {
    let mut nulls = [0_usize; 17];
    visit(request, |record, _resource, scope| {
        if validate(record).is_err() {
            return Ok(());
        }
        let scope = scope.scope.as_ref().filter(|value| !value.name.is_empty());
        let absent = [
            record.time_unix_nano == 0 || i64::try_from(record.time_unix_nano).is_err(),
            false,
            severity(record.severity_number).is_none(),
            record.severity_text.is_empty(),
            true,
            record.body.is_none(),
            record.trace_id.is_empty(),
            record.span_id.is_empty(),
            record.flags.to_le_bytes()[0] == 0,
            unique_keys(&record.attributes) == 0,
            false,
            false,
            scope.is_none(),
            scope.is_none_or(|value| value.version.is_empty()),
            true,
            true,
            true,
        ];
        for (count, absent) in nulls.iter_mut().zip(absent) {
            *count = add(*count, usize::from(absent))?;
        }
        Ok(())
    })?;
    Ok(nulls)
}

/// Counts accepted rows and exact variable-width material without allocation.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on checked arithmetic or JSON sizing failure.
fn plan(request: &ExportLogsServiceRequest) -> Result<LogPlan, ScribeError> {
    let mut plan = LogPlan::default();
    let mut ordinal = 0_usize;
    visit(request, |record, resource, scope| {
        let current = ordinal;
        ordinal = add(ordinal, 1)?;
        if validate(record).is_err() {
            plan.rejected = add(plan.rejected, 1)?;
            plan.first_rejection.get_or_insert(current);
            return Ok(());
        }
        plan.accepted = add(plan.accepted, 1)?;
        count_text(
            &mut plan.severity_text,
            (!record.severity_text.is_empty()).then_some(record.severity_text.as_str()),
        )?;
        match record.body.as_ref() {
            Some(value) => count_writer(&mut plan.body, |writer| write_any(writer, Some(value)))?,
            None => plan.body.has_null = true,
        }
        if unique_keys(&record.attributes) == 0 {
            plan.attributes.has_null = true;
        } else {
            count_writer(&mut plan.attributes, |writer| {
                write_attributes(writer, &record.attributes)
            })?;
        }
        count_text(&mut plan.service_name, Some(service_name(resource)))?;
        let scope = scope.scope.as_ref().filter(|value| !value.name.is_empty());
        count_text(&mut plan.scope_name, scope.map(|value| value.name.as_str()))?;
        count_text(
            &mut plan.scope_version,
            scope
                .filter(|value| !value.version.is_empty())
                .map(|value| value.version.as_str()),
        )?;
        Ok(())
    })?;
    Ok(plan)
}

/// Materializes accepted log records directly into the existing logs schema.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on pass or Arrow divergence.
fn materialize(
    request: &ExportLogsServiceRequest,
    plan: LogPlan,
) -> Result<RecordBatch, ScribeError> {
    let rows = plan.accepted;
    let nulls = material_nulls(request)?;
    let mut time = ExactPrimitive::<TimestampMicrosecondType>::new(rows, nulls[0] != 0);
    let mut observed_time = ExactPrimitive::<TimestampMicrosecondType>::new(rows, false);
    let mut severity_number = ExactPrimitive::<Int64Type>::new(rows, nulls[2] != 0);
    let mut severity_text = NullableText::new_parts(rows, plan.severity_text.bytes, nulls[3] != 0)?;
    let mut body = NullableText::new_parts(rows, plan.body.bytes, plan.body.has_null)?;
    let mut trace_id = ExactFixedBinary::new(rows, 16, nulls[6] != 0)?;
    let mut span_id = ExactFixedBinary::new(rows, 8, nulls[7] != 0)?;
    let mut trace_flags = ExactPrimitive::<Int64Type>::new(rows, nulls[8] != 0);
    let mut attributes =
        NullableText::new_parts(rows, plan.attributes.bytes, plan.attributes.has_null)?;
    let mut dropped_attributes = ExactPrimitive::<Int64Type>::new(rows, false);
    let mut service_name_column = ExactStringColumn::new(rows, plan.service_name.bytes)?;
    let mut scope_name = NullableText::new_parts(rows, plan.scope_name.bytes, nulls[12] != 0)?;
    let mut scope_version =
        NullableText::new_parts(rows, plan.scope_version.bytes, nulls[13] != 0)?;
    let mut produced = 0_usize;

    visit(request, |record, resource, scope| {
        if validate(record).is_err() {
            return Ok(());
        }
        time.append(optional_micros(record.time_unix_nano))?;
        observed_time.append(Some(observed_micros(record)?))?;
        severity_number.append(severity(record.severity_number).map(i64::from))?;
        append_nullable_text(
            &mut severity_text,
            (!record.severity_text.is_empty()).then_some(record.severity_text.as_str()),
        )?;
        match record.body.as_ref() {
            Some(value) => body.append(|writer| write_any(writer, Some(value)))?,
            None => body.append_null()?,
        }
        trace_id.append((!record.trace_id.is_empty()).then_some(record.trace_id.as_slice()))?;
        span_id.append((!record.span_id.is_empty()).then_some(record.span_id.as_slice()))?;
        let flags = record.flags.to_le_bytes()[0];
        if flags == 0 {
            trace_flags.append(None)?;
        } else {
            trace_flags.append(Some(i64::from(flags)))?;
        }
        if unique_keys(&record.attributes) == 0 {
            attributes.append_null()?;
        } else {
            attributes.append(|writer| write_attributes(writer, &record.attributes))?;
        }
        dropped_attributes.append(Some(i64::from(record.dropped_attributes_count)))?;
        let service = service_name(resource);
        service_name_column.append_with(|writer| {
            writer.extend_from_slice(service.as_bytes());
            Ok(())
        })?;
        let scope = scope.scope.as_ref().filter(|value| !value.name.is_empty());
        append_nullable_text(&mut scope_name, scope.map(|value| value.name.as_str()))?;
        append_nullable_text(
            &mut scope_version,
            scope
                .filter(|value| !value.version.is_empty())
                .map(|value| value.version.as_str()),
        )?;
        produced = add(produced, 1)?;
        Ok(())
    })?;
    if produced != rows {
        return Err(ScribeError::InvalidFrame);
    }
    let all_null = || -> Result<ArrayRef, ScribeError> {
        let mut column = NullableText::new_parts(rows, 0, true)?;
        for _ in 0..rows {
            column.append_null()?;
        }
        Ok(Arc::new(column.finish()?) as ArrayRef)
    };
    let columns = vec![
        Arc::new(time.finish()?.with_timezone("UTC")) as ArrayRef,
        Arc::new(observed_time.finish()?.with_timezone("UTC")) as ArrayRef,
        Arc::new(severity_number.finish()?) as ArrayRef,
        Arc::new(severity_text.finish()?) as ArrayRef,
        all_null()?,
        Arc::new(body.finish()?) as ArrayRef,
        Arc::new(trace_id.finish()?) as ArrayRef,
        Arc::new(span_id.finish()?) as ArrayRef,
        Arc::new(trace_flags.finish()?) as ArrayRef,
        Arc::new(attributes.finish()?) as ArrayRef,
        Arc::new(dropped_attributes.finish()?) as ArrayRef,
        Arc::new(service_name_column.finish()?) as ArrayRef,
        Arc::new(scope_name.finish()?) as ArrayRef,
        Arc::new(scope_version.finish()?) as ArrayRef,
        all_null()?,
    ];
    RecordBatch::try_new(retained_write_schema(), columns).map_err(|_| ScribeError::InvalidFrame)
}

/// Visits every record without allocating a flattened collection.
///
/// # Errors
///
/// Propagates checked planning or materialization errors from the visitor.
fn visit(
    request: &ExportLogsServiceRequest,
    mut visit: impl FnMut(&LogRecord, &ResourceLogs, &ScopeLogs) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
    for resource in &request.resource_logs {
        for scope in &resource.scope_logs {
            for record in &scope.log_records {
                visit(record, resource, scope)?;
            }
        }
    }
    Ok(())
}

/// Validates the current log mapper's per-record rejection conditions.
///
/// # Errors
///
/// Returns `()` when timestamps exceed the physical range, trace/span IDs are
/// malformed or inconsistent, or body/attribute material violates the direct
/// log projection contract.
fn validate(record: &LogRecord) -> Result<(), ()> {
    if i64::try_from(record.observed_time_unix_nano).is_err()
        && i64::try_from(record.time_unix_nano).is_err()
    {
        return Err(());
    }
    if !record.trace_id.is_empty() {
        let value: [u8; 16] = record.trace_id.as_slice().try_into().map_err(|_| ())?;
        TraceId::from_bytes(value).map_err(|_| ())?;
    }
    if !record.span_id.is_empty() {
        let value: [u8; 8] = record.span_id.as_slice().try_into().map_err(|_| ())?;
        SpanId::from_bytes(value).map_err(|_| ())?;
        if record.trace_id.is_empty() {
            return Err(());
        }
    }
    let body_bytes = match record.body.as_ref() {
        Some(value) => {
            let mut counter = ByteCounter::default();
            write_any(&mut counter, Some(value)).map_err(|_| ())?;
            counter.0
        }
        None => 0,
    };
    if body_bytes > wyrd_spec::vala::logs::record::MAX_LOG_BODY_BYTES {
        return Err(());
    }
    let mut counter = ByteCounter::default();
    write_attributes(&mut counter, &record.attributes).map_err(|_| ())?;
    if counter.0 > wyrd_spec::vala::logs::record::MAX_LOG_ATTRIBUTES_BYTES {
        return Err(());
    }
    Ok(())
}

/// Renders the legacy mapper reason for the first rejected log record.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the planned ordinal cannot be
/// resolved during the identical borrowed traversal.
fn rejection_reason(
    request: &ExportLogsServiceRequest,
    target: usize,
) -> Result<String, ScribeError> {
    let mut ordinal = 0_usize;
    let mut reason = None;
    visit(request, |record, _resource, _scope| {
        let current = ordinal;
        ordinal = add(ordinal, 1)?;
        if current != target {
            return Ok(());
        }
        reason = Some(
            if i64::try_from(record.observed_time_unix_nano).is_err()
                && i64::try_from(record.time_unix_nano).is_err()
            {
                "log observed_time_unix_nano out of representable range".to_owned()
            } else if let Some(id_reason) = trace_id_rejection(&record.trace_id) {
                id_reason
            } else if let Some(id_reason) = span_id_rejection(&record.span_id) {
                id_reason
            } else if !record.span_id.is_empty() && record.trace_id.is_empty() {
                validation_reason("span_id requires trace_id")
            } else {
                let mut body = ByteCounter::default();
                write_any(&mut body, record.body.as_ref())?;
                if body.0 > wyrd_spec::vala::logs::record::MAX_LOG_BODY_BYTES {
                    validation_reason("log body exceeds MAX_LOG_BODY_BYTES")
                } else {
                    let mut attributes = ByteCounter::default();
                    write_attributes(&mut attributes, &record.attributes)?;
                    if attributes.0 > wyrd_spec::vala::logs::record::MAX_LOG_ATTRIBUTES_BYTES {
                        validation_reason("log attributes exceed MAX_LOG_ATTRIBUTES_BYTES")
                    } else {
                        return Err(ScribeError::InvalidFrame);
                    }
                }
            },
        );
        Ok(())
    })?;
    reason.ok_or(ScribeError::InvalidFrame)
}

/// Renders the stable validation error form used by the legacy record mapper.
fn validation_reason(message: &str) -> String {
    WyrdError::Validation {
        message: message.to_owned(),
        details: serde_json::Value::Null,
    }
    .to_string()
}

/// Returns the legacy mapper reason when a present trace id is malformed.
fn trace_id_rejection(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let value: [u8; 16] = match bytes.try_into() {
        Ok(value) => value,
        Err(_) => return Some(format!("trace_id must be 16 bytes, got {}", bytes.len())),
    };
    TraceId::from_bytes(value)
        .err()
        .map(|error| error.to_string())
}

/// Returns the legacy mapper reason when a present span id is malformed.
fn span_id_rejection(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let value: [u8; 8] = match bytes.try_into() {
        Ok(value) => value,
        Err(_) => return Some(format!("span_id must be 8 bytes, got {}", bytes.len())),
    };
    SpanId::from_bytes(value)
        .err()
        .map(|error| error.to_string())
}

/// Returns the mapper-normalized optional severity number.
fn severity(value: i32) -> Option<u8> {
    match SeverityNumber::try_from(value).unwrap_or(SeverityNumber::Unspecified) {
        SeverityNumber::Unspecified => None,
        other => u8::try_from(other as i32).ok(),
    }
}

/// Returns observed-time microseconds using the mapper's observed/time fallback.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when both timestamps are out of range.
fn observed_micros(record: &LogRecord) -> Result<i64, ScribeError> {
    i64::try_from(record.observed_time_unix_nano)
        .or_else(|_| i64::try_from(record.time_unix_nano))
        .map(|nanos| nanos / 1_000)
        .map_err(|_| ScribeError::InvalidFrame)
}

/// Normalizes an optional nonzero log timestamp into Arrow microseconds.
fn optional_micros(value: u64) -> Option<i64> {
    if value == 0 {
        return None;
    }
    i64::try_from(value).ok().map(|nanos| nanos / 1_000)
}

/// Appends one optional borrowed string directly into exact Arrow storage.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on nullability or capacity divergence.
fn append_nullable_text(column: &mut NullableText, value: Option<&str>) -> Result<(), ScribeError> {
    match value {
        Some(value) => column.append(|writer| {
            writer.extend_from_slice(value.as_bytes());
            Ok(())
        }),
        None => column.append_null(),
    }
}

/// Counts one optional borrowed string.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on checked capacity overflow.
fn count_text(plan: &mut TextCapacity, value: Option<&str>) -> Result<(), ScribeError> {
    match value {
        Some(value) => plan.bytes = add(plan.bytes, value.len())?,
        None => plan.has_null = true,
    }
    Ok(())
}

/// Counts one present direct-writer value.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on serialization or checked overflow.
fn count_writer(
    plan: &mut TextCapacity,
    write: impl FnOnce(&mut ByteCounter) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
    let mut counter = ByteCounter::default();
    write(&mut counter)?;
    plan.bytes = add(plan.bytes, counter.0)?;
    Ok(())
}

/// Counts unique OTLP keys with constant storage.
fn unique_keys(values: &[KeyValue]) -> usize {
    values
        .iter()
        .enumerate()
        .filter(|(index, value)| {
            values[*index + 1..]
                .iter()
                .all(|next| next.key != value.key)
        })
        .count()
}

/// Returns the final string-valued service name.
fn service_name(resource: &ResourceLogs) -> &str {
    resource
        .resource
        .as_ref()
        .and_then(|resource| {
            resource
                .attributes
                .iter()
                .rev()
                .find(|value| value.key == "service.name")
        })
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match value.value.as_ref() {
            Some(any_value::Value::StringValue(value)) => Some(value.as_str()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Performs checked capacity addition.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on `usize` overflow.
fn add(left: usize, right: usize) -> Result<usize, ScribeError> {
    left.checked_add(right).ok_or(ScribeError::InvalidFrame)
}

/// Returns the directly materialized mapper prefix through nullable `run_id`.
///
/// The authoritative managed projection appends all remaining server-owned
/// columns without allocating the discarded mapper placeholders.
fn retained_write_schema() -> Arc<Schema> {
    let mut fields = RecordsTable::arrow_fields();
    fields.push(Field::new(RUN_ID, arrow::datatypes::DataType::Utf8, true));
    Arc::new(Schema::new(fields))
}

/// Returns the existing logs coordinator input schema.
fn write_schema() -> Arc<Schema> {
    let mut fields = RecordsTable::arrow_fields();
    fields.push(Field::new(RUN_ID, arrow::datatypes::DataType::Utf8, true));
    fields.push(Field::new(CARD_UID, arrow::datatypes::DataType::Utf8, true));
    fields.push(Field::new(
        PRINCIPAL_ID,
        arrow::datatypes::DataType::Utf8,
        true,
    ));
    Arc::new(Schema::new(fields))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope};
    use wyrd_tonic::otlp::resource::v1::Resource;

    /// Projects one fixture with deterministic authoritative managed values.
    ///
    /// # Errors
    ///
    /// Returns the direct projector's validation, material-limit, or Arrow
    /// construction error.
    fn project_fixture(
        request: &ExportLogsServiceRequest,
        material_limit: usize,
    ) -> Result<
        (
            Option<crate::scribe::otlp_managed::OtlpManagedBatch>,
            LogsOutcome,
        ),
        ScribeError,
    > {
        let context = super::super::otlp_managed::OtlpTestContext::new();
        let projection = context.projection(write_schema().as_ref());
        let material_limit = if material_limit == usize::MAX {
            super::super::material_plan::ScribeIngressPlanner::default()
                .plan_logs(request, 1024, 0, &projection)?
                .current_material_bytes
        } else {
            material_limit
        };
        projection.project_logs(request, material_limit)
    }

    /// Computes the exact final Arrow-plus-IPC charge from pre-material facts.
    ///
    /// # Errors
    ///
    /// Returns the planner's validation, fingerprint, or checked-size error.
    fn exact_material_bytes(request: &ExportLogsServiceRequest) -> Result<usize, ScribeError> {
        let source = plan(request)?;
        let context = super::super::otlp_managed::OtlpTestContext::new();
        let projection = context.projection(write_schema().as_ref());
        let managed = projection.managed(write_schema(), source.accepted)?;
        source
            .material_plan(request, usize::MAX, &managed)?
            .admitted_bytes(usize::MAX)
    }

    /// Constructs one rich log request with body, IDs, attributes, resource, and scope.
    fn request() -> ExportLogsServiceRequest {
        ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_owned(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("checkout".to_owned())),
                        }),
                    }],
                    ..Resource::default()
                }),
                scope_logs: vec![ScopeLogs {
                    scope: Some(InstrumentationScope {
                        name: "logs-sdk".to_owned(),
                        version: "1.0".to_owned(),
                        ..InstrumentationScope::default()
                    }),
                    log_records: vec![LogRecord {
                        time_unix_nano: 1_700_000_000_000_000_000,
                        observed_time_unix_nano: 1_700_000_000_050_000_000,
                        severity_number: SeverityNumber::Error as i32,
                        severity_text: "ERROR".to_owned(),
                        body: Some(AnyValue {
                            value: Some(any_value::Value::KvlistValue(
                                wyrd_tonic::otlp::common::v1::KeyValueList {
                                    values: vec![KeyValue {
                                        key: "message".to_owned(),
                                        value: Some(AnyValue {
                                            value: Some(any_value::Value::StringValue(
                                                "boom".to_owned(),
                                            )),
                                        }),
                                    }],
                                },
                            )),
                        }),
                        attributes: vec![KeyValue {
                            key: "log.source".to_owned(),
                            value: Some(AnyValue {
                                value: Some(any_value::Value::StringValue("app".to_owned())),
                            }),
                        }],
                        dropped_attributes_count: 2,
                        flags: 1,
                        trace_id: (1_u8..=16).collect(),
                        span_id: (1_u8..=8).collect(),
                        ..LogRecord::default()
                    }],
                    ..ScopeLogs::default()
                }],
                ..ResourceLogs::default()
            }],
        }
    }

    /// Empty typed requests produce no Arrow batch and no rejection.
    #[test]
    fn empty_request_projects_no_batch() {
        let (batch, outcome) =
            project_fixture(&ExportLogsServiceRequest::default(), usize::MAX).expect("project");
        assert!(batch.is_none());
        assert_eq!(outcome.accepted_records, 0);
        assert_eq!(outcome.rejected_records, 0);
    }

    /// Rich body, identifier, attribute, resource, and scope columns match the mapper.
    #[test]
    fn otlp_projection_parity_logs() {
        let request = request();
        let expected = crate::scribe::test_projection_oracle::project_resource_logs(&request)
            .expect("oracle projection")
            .batch
            .expect("oracle batch");
        let (actual, outcome) = project_fixture(&request, usize::MAX).expect("direct projection");
        let actual = actual.expect("direct batch").rows;
        let retained = expected.num_columns() - 2;
        assert_eq!(
            &actual.columns()[..retained],
            &expected.columns()[..retained]
        );
        assert_eq!(outcome.accepted_records, 1);
        assert_eq!(outcome.rejected_records, 0);
    }

    /// Mixed log exports preserve accepted rows and the first rejection summary.
    #[test]
    fn mixed_projection_matches_mapper_rejection() {
        let mut request = request();
        let mut invalid = request.resource_logs[0].scope_logs[0].log_records[0].clone();
        invalid.trace_id.clear();
        invalid.span_id = vec![1_u8; 8];
        request.resource_logs[0].scope_logs[0]
            .log_records
            .push(invalid);
        let mapped =
            crate::scribe::test_projection_oracle::map::map_resource_logs(&request.resource_logs);
        let expected_reason = mapped.rejected.first().map(|value| value.reason.clone());
        let (actual, outcome) = project_fixture(&request, usize::MAX).expect("direct projection");
        assert_eq!(
            actual.expect("accepted row").rows.num_rows(),
            mapped.records.len()
        );
        assert_eq!(
            outcome.accepted_records,
            i64::try_from(mapped.records.len()).expect("accepted record count fits i64")
        );
        assert_eq!(
            outcome.rejected_records,
            i64::try_from(mapped.rejected.len()).expect("rejected record count fits i64")
        );
        assert_eq!(outcome.rejection_message, expected_reason);
    }

    /// Exact log Arrow-plus-IPC material passes at `C` and refuses `C - 1`.
    #[test]
    fn material_limit_has_exact_boundary() {
        let request = request();
        let exact = exact_material_bytes(&request).expect("pre-material exact charge");
        assert!(project_fixture(&request, exact).is_ok());
        assert!(matches!(
            project_fixture(&request, exact - 1),
            Err(ScribeError::DecodedPayloadTooLarge { .. })
        ));
    }
    /// Ingress includes repeated shared values, escaping, Arrow owners, and IPC.
    ///
    /// # Errors
    ///
    /// Returns fixture planning, projection, or IPC encoding failures.
    #[test]
    fn ingress_plan_covers_projection() -> Result<(), ScribeError> {
        let mut request = request();
        let shared = request.resource_logs[0]
            .resource
            .as_mut()
            .expect("fixture resource");
        shared.attributes.push(KeyValue {
            key: "escaped".to_owned(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue("quote\"\n\t".repeat(32))),
            }),
        });
        let repeated = request.resource_logs[0].scope_logs[0].clone();
        request.resource_logs[0].scope_logs.push(repeated);
        let context = super::super::otlp_managed::OtlpTestContext::new();
        let projection = context.projection(write_schema().as_ref());
        let admitted = super::super::material_plan::ScribeIngressPlanner::default().plan_logs(
            &request,
            1024,
            0,
            &projection,
        )?;
        let exact = exact_material_bytes(&request)?;
        assert_eq!(admitted.current_material_bytes, exact);
        let (batch, _) = projection.project_logs(&request, admitted.current_material_bytes)?;
        let batch = batch.expect("valid fixture produces rows");
        let ipc = batch.ipc_plan.encode(&batch.rows)?;
        assert_eq!(
            admitted.current_material_bytes,
            batch.rows.get_array_memory_size() + ipc.len()
        );
        assert_eq!(
            admitted.persistence_candidate_bytes,
            batch.rows.get_array_memory_size()
        );
        assert_eq!(admitted.rows, batch.rows.num_rows());
        Ok(())
    }
}
