//! Exact two-pass projection of typed OTLP traces into one Arrow batch.
//!
//! The planner borrows the generated request and retains only counters and one
//! rejection coordinate. The projector repeats the same validation and writes
//! accepted spans directly into fixed-capacity Arrow builders.

use std::io::Write;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, FixedSizeBinaryArray, Int64Array, StringArray, TimestampMicrosecondArray,
};
use arrow::buffer::{BooleanBuffer, Buffer, NullBuffer, OffsetBuffer, ScalarBuffer};
use arrow::datatypes::{Field, Schema};
use arrow::record_batch::RecordBatch;
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::managed_columns::{CARD_UID, PRINCIPAL_ID, RUN_ID};
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, Span, span, status::StatusCode};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use crate::contracts::ScribeError;
use crate::gate::collector::tables::{DomainTable, SpansTable};
use crate::otlp_contract::IngestOutcome;

/// Stable fallback used by the existing mapper for a missing or unnamed scope.
const UNKNOWN_SCOPE: &str = "unknown_service";

/// Exact UTF-8 value bytes required by the direct span columns.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Utf8Capacities {
    /// `trace_state` value bytes.
    trace_state: usize,
    /// Span-name value bytes.
    name: usize,
    /// Span-kind value bytes.
    kind: usize,
    /// Span-status value bytes.
    status: usize,
    /// Compact attribute JSON value bytes.
    attributes: usize,
    /// Scope-name value bytes.
    scope_name: usize,
    /// Scope-version value bytes.
    scope_version: usize,
    /// Service-name value bytes.
    service_name: usize,
}

/// Coordinate of the first rejected span in request traversal order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RejectionCoordinate {
    /// Resource group index.
    resource: usize,
    /// Scope group index.
    scope: usize,
    /// Span index.
    span: usize,
}

/// Allocation-free first-pass result used to size every second-pass builder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TracePlan {
    /// Accepted row count.
    accepted: usize,
    /// Rejected row count.
    rejected: usize,
    /// First rejected span, rendered only after planning.
    first_rejection: Option<RejectionCoordinate>,
    /// Exact variable-width value capacities.
    utf8: Utf8Capacities,
    /// Accepted rows with no parent span.
    parent_nulls: usize,
    /// Accepted rows with no scope version.
    scope_version_nulls: usize,
}

/// Count-only sink for exact compact JSON sizing.
#[derive(Debug, Default)]
pub(super) struct ByteCounter(pub(super) usize);

impl Write for ByteCounter {
    /// Counts one emitted JSON fragment.
    ///
    /// # Errors
    ///
    /// Returns an IO error when the byte count overflows `usize`.
    fn write(&mut self, value: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(value.len())
            .ok_or_else(|| std::io::Error::other("OTLP JSON length overflow"))?;
        Ok(value.len())
    }

    /// Flushes the stateless counter.
    ///
    /// # Errors
    ///
    /// This operation is infallible.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Exact non-null UTF-8 column storage filled directly by JSON serialization.
#[derive(Debug)]
pub(super) struct ExactStringColumn {
    /// Exact row offsets, beginning with zero.
    offsets: Vec<i32>,
    /// Exact compact value storage.
    values: Vec<u8>,
    /// Exact validity storage for nullable columns.
    validity: Option<Vec<u8>>,
    /// Immutable expected row count.
    rows: usize,
    /// Immutable expected value bytes.
    value_bytes: usize,
}

impl ExactStringColumn {
    /// Allocates the exact public Arrow offset and value capacities.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the row count cannot form a
    /// valid Arrow `i32` offset plan.
    pub(super) fn new(rows: usize, value_bytes: usize) -> Result<Self, ScribeError> {
        Self::new_nullable(rows, value_bytes, false)
    }

    /// Allocates exact UTF-8 storage with an optional validity bitmap.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] for invalid Arrow offset counts.
    fn new_nullable(rows: usize, value_bytes: usize, nullable: bool) -> Result<Self, ScribeError> {
        i32::try_from(value_bytes).map_err(|_| ScribeError::InvalidFrame)?;
        let offset_count = rows.checked_add(1).ok_or(ScribeError::InvalidFrame)?;
        let mut offsets = Vec::with_capacity(offset_count);
        offsets.push(0);
        Ok(Self {
            offsets,
            values: Vec::with_capacity(value_bytes),
            validity: nullable.then(|| vec![0_u8; rows.saturating_add(7) / 8]),
            rows,
            value_bytes,
        })
    }

    /// Writes one compact OTLP attribute object directly into Arrow value storage.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on serialization, offset overflow,
    /// row overflow, or plan divergence.
    fn append_attributes(&mut self, values: &[KeyValue]) -> Result<(), ScribeError> {
        self.append_with(|writer| write_attributes(writer, values))
    }

    /// Writes one non-null value directly into the fixed Arrow value buffer.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on serialization, offset overflow,
    /// row overflow, or plan divergence.
    pub(super) fn append_with(
        &mut self,
        write: impl FnOnce(&mut Vec<u8>) -> Result<(), ScribeError>,
    ) -> Result<(), ScribeError> {
        if self.offsets.len() > self.rows {
            return Err(ScribeError::InvalidFrame);
        }
        write(&mut self.values)?;
        let row = self.offsets.len() - 1;
        if let Some(validity) = &mut self.validity {
            validity[row / 8] |= 1 << (row % 8);
        }
        let offset = i32::try_from(self.values.len()).map_err(|_| ScribeError::InvalidFrame)?;
        self.offsets.push(offset);
        Ok(())
    }

    /// Appends one planned null without writing value bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when the column is non-null or full.
    fn append_null(&mut self) -> Result<(), ScribeError> {
        if self.validity.is_none() || self.offsets.len() > self.rows {
            return Err(ScribeError::InvalidFrame);
        }
        let offset = *self.offsets.last().ok_or(ScribeError::InvalidFrame)?;
        self.offsets.push(offset);
        Ok(())
    }

    /// Freezes exact buffers into a non-null Arrow UTF-8 array.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when materialized lengths diverge
    /// or Arrow rejects the prevalidated offsets.
    pub(super) fn finish(self) -> Result<StringArray, ScribeError> {
        if self.offsets.len() != self.rows + 1
            || self.offsets.capacity() != self.rows + 1
            || self.values.len() != self.value_bytes
            || self.values.capacity() != self.value_bytes
        {
            return Err(ScribeError::InvalidFrame);
        }
        let nulls = self
            .validity
            .map(|bits| NullBuffer::new(BooleanBuffer::new(Buffer::from(bits), 0, self.rows)));
        StringArray::try_new(
            OffsetBuffer::new(ScalarBuffer::from(self.offsets)),
            Buffer::from(self.values),
            nulls,
        )
        .map_err(|_| ScribeError::InvalidFrame)
    }
}

/// Projects one typed trace request through an allocation-free sizing pass and
/// an exact-capacity Arrow construction pass.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when JSON sizing, checked capacity
/// arithmetic, Arrow construction, or pass-to-pass validation diverges.
pub(crate) fn project(
    request: &ExportTraceServiceRequest,
    material_limit: usize,
    principal: &wyrd_runtime::Principal,
    expected_fingerprint: crate::schema::SchemaFingerprint,
    request_id: &wyrd_spec::request_id::RequestId,
    batch_id: uuid::Uuid,
    receipt_micros: i64,
) -> Result<
    (
        Option<crate::scribe::otlp_managed::OtlpManagedBatch>,
        IngestOutcome,
    ),
    ScribeError,
> {
    let plan = plan(request)?;
    let rejection_message = plan
        .first_rejection
        .map(|coordinate| rejection_reason(request, coordinate))
        .transpose()?;
    let outcome = IngestOutcome {
        accepted_spans: i64::try_from(plan.accepted).unwrap_or(i64::MAX),
        rejected_spans: i64::try_from(plan.rejected).unwrap_or(i64::MAX),
        rejection_message,
    };
    if plan.accepted == 0 {
        return Ok((None, outcome));
    }
    let managed = crate::scribe::otlp_managed::OtlpManagedProjection::plan(
        write_schema(),
        expected_fingerprint,
        plan.accepted,
        principal,
        request_id,
        batch_id,
        receipt_micros,
    )?;
    let material = validate_material_plan(plan, material_limit, &managed)?;
    let batch = materialize(request, plan)?;
    Ok((Some(managed.finish(batch, material)?), outcome))
}

/// Refuses an oversized trace slice from immutable public Arrow and IPC facts.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] when the exact combined
/// backing exceeds `material_limit`, and [`ScribeError::InvalidFrame`] when
/// the canonical schema and count facts disagree.
fn validate_material_plan(
    plan: TracePlan,
    material_limit: usize,
    managed: &crate::scribe::otlp_managed::OtlpManagedProjection,
) -> Result<crate::scribe::otlp_managed::OtlpManagedMaterialPlan, ScribeError> {
    use crate::scribe::fixed_ipc::FixedIpcColumnPlan;

    let rows = plan.accepted;
    let bitmap = rows.checked_add(7).ok_or(ScribeError::InvalidFrame)? / 8;
    let offsets = rows
        .checked_add(1)
        .and_then(|value| value.checked_mul(4))
        .ok_or(ScribeError::InvalidFrame)?;
    let fixed = |width: usize, null_count: usize| -> Result<FixedIpcColumnPlan, ScribeError> {
        Ok(FixedIpcColumnPlan {
            null_count,
            validity_bytes: usize::from(null_count != 0) * bitmap,
            offsets_bytes: 0,
            values_bytes: rows.checked_mul(width).ok_or(ScribeError::InvalidFrame)?,
        })
    };
    let utf8 =
        |values_bytes: usize, null_count: usize| -> Result<FixedIpcColumnPlan, ScribeError> {
            Ok(FixedIpcColumnPlan {
                null_count,
                validity_bytes: usize::from(null_count != 0) * bitmap,
                offsets_bytes: offsets,
                values_bytes,
            })
        };
    let columns = [
        fixed(16, 0)?,
        fixed(8, 0)?,
        fixed(8, plan.parent_nulls)?,
        fixed(8, 0)?,
        utf8(plan.utf8.trace_state, 0)?,
        utf8(plan.utf8.name, 0)?,
        utf8(plan.utf8.kind, 0)?,
        fixed(8, 0)?,
        fixed(8, 0)?,
        fixed(8, 0)?,
        utf8(plan.utf8.status, 0)?,
        utf8(plan.utf8.attributes, 0)?,
        fixed(8, 0)?,
        fixed(8, 0)?,
        fixed(8, 0)?,
        utf8(plan.utf8.scope_name, 0)?,
        utf8(plan.utf8.scope_version, plan.scope_version_nulls)?,
        utf8(plan.utf8.service_name, 0)?,
        utf8(0, rows)?,
        utf8(0, rows)?,
        utf8(0, rows)?,
    ];
    let final_plan = managed.material_plan(columns.into_iter())?;
    final_plan.admitted_bytes(material_limit)?;
    Ok(final_plan)
}

/// Plans exact accepted-row and UTF-8 capacities without constructing domain
/// records, Arrow arrays, rejection strings, or nested JSON containers.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when checked capacity or JSON sizing
/// overflows.
fn plan(request: &ExportTraceServiceRequest) -> Result<TracePlan, ScribeError> {
    let mut plan = TracePlan {
        accepted: 0,
        rejected: 0,
        first_rejection: None,
        utf8: Utf8Capacities::default(),
        parent_nulls: 0,
        scope_version_nulls: 0,
    };
    for (resource_index, resource) in request.resource_spans.iter().enumerate() {
        let service_name = service_name(resource);
        for (scope_index, scope) in resource.scope_spans.iter().enumerate() {
            let scope_name = scope
                .scope
                .as_ref()
                .filter(|value| !value.name.is_empty())
                .map_or(UNKNOWN_SCOPE, |value| value.name.as_str());
            let scope_version = scope
                .scope
                .as_ref()
                .filter(|value| !value.version.is_empty())
                .map(|value| value.version.as_str());
            for (span_index, value) in scope.spans.iter().enumerate() {
                let coordinate = RejectionCoordinate {
                    resource: resource_index,
                    scope: scope_index,
                    span: span_index,
                };
                if validate_span(value, resource, scope).is_err() {
                    plan.rejected = checked_add(plan.rejected, 1)?;
                    plan.first_rejection.get_or_insert(coordinate);
                    continue;
                }
                plan.accepted = checked_add(plan.accepted, 1)?;
                if value.parent_span_id.is_empty() {
                    plan.parent_nulls = checked_add(plan.parent_nulls, 1)?;
                }
                if scope_version.is_none() {
                    plan.scope_version_nulls = checked_add(plan.scope_version_nulls, 1)?;
                }
                add(&mut plan.utf8.trace_state, value.trace_state.len())?;
                add(&mut plan.utf8.name, value.name.len())?;
                add(&mut plan.utf8.kind, kind(value.kind).len())?;
                add(&mut plan.utf8.status, status(value).len())?;
                add(
                    &mut plan.utf8.attributes,
                    json_attributes_len(&value.attributes)?,
                )?;
                add(&mut plan.utf8.scope_name, scope_name.len())?;
                if let Some(version) = scope_version {
                    add(&mut plan.utf8.scope_version, version.len())?;
                }
                add(&mut plan.utf8.service_name, service_name.len())?;
            }
        }
    }
    Ok(plan)
}

/// Owns the exact-capacity columns for one trace materialization pass.
struct TraceColumns {
    /// Trace identifiers.
    trace_id: Vec<u8>,
    /// Span identifiers.
    span_id: Vec<u8>,
    /// Parent span identifiers.
    parent_span_id: Vec<u8>,
    /// Optional parent validity bitmap.
    parent_validity: Option<Vec<u8>>,
    /// Span flags.
    flags: Vec<i64>,
    /// Trace-state strings.
    trace_state: ExactStringColumn,
    /// Span names.
    name: ExactStringColumn,
    /// Span-kind strings.
    kind: ExactStringColumn,
    /// Start timestamps.
    start: Vec<i64>,
    /// End timestamps.
    end: Vec<i64>,
    /// Durations.
    duration: Vec<i64>,
    /// Status strings.
    status: ExactStringColumn,
    /// Attribute JSON strings.
    attributes: ExactStringColumn,
    /// Dropped-attribute counts.
    dropped_attributes: Vec<i64>,
    /// Dropped-event counts.
    dropped_events: Vec<i64>,
    /// Dropped-link counts.
    dropped_links: Vec<i64>,
    /// Scope names.
    scope_name: ExactStringColumn,
    /// Optional scope versions.
    scope_version: ExactStringColumn,
    /// Service names.
    service_name: ExactStringColumn,
    /// Accepted rows produced by the second pass.
    produced: usize,
}

impl TraceColumns {
    /// Allocates every column at the immutable first-pass capacity.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on exact string capacity overflow.
    fn new(plan: TracePlan) -> Result<Self, ScribeError> {
        let rows = plan.accepted;
        Ok(Self {
            trace_id: Vec::with_capacity(rows * 16),
            span_id: Vec::with_capacity(rows * 8),
            parent_span_id: Vec::with_capacity(rows * 8),
            parent_validity: (plan.parent_nulls != 0)
                .then(|| vec![0_u8; rows.saturating_add(7) / 8]),
            flags: Vec::with_capacity(rows),
            trace_state: ExactStringColumn::new(rows, plan.utf8.trace_state)?,
            name: ExactStringColumn::new(rows, plan.utf8.name)?,
            kind: ExactStringColumn::new(rows, plan.utf8.kind)?,
            start: Vec::with_capacity(rows),
            end: Vec::with_capacity(rows),
            duration: Vec::with_capacity(rows),
            status: ExactStringColumn::new(rows, plan.utf8.status)?,
            attributes: ExactStringColumn::new(rows, plan.utf8.attributes)?,
            dropped_attributes: Vec::with_capacity(rows),
            dropped_events: Vec::with_capacity(rows),
            dropped_links: Vec::with_capacity(rows),
            scope_name: ExactStringColumn::new(rows, plan.utf8.scope_name)?,
            scope_version: ExactStringColumn::new_nullable(
                rows,
                plan.utf8.scope_version,
                plan.scope_version_nulls != 0,
            )?,
            service_name: ExactStringColumn::new(rows, plan.utf8.service_name)?,
            produced: 0,
        })
    }

    /// Appends one accepted span and its borrowed resource/scope context.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on capacity or timestamp divergence.
    fn append(
        &mut self,
        value: &Span,
        resource: &ResourceSpans,
        scope: &wyrd_tonic::otlp::trace::v1::ScopeSpans,
    ) -> Result<(), ScribeError> {
        self.trace_id.extend_from_slice(&value.trace_id);
        self.span_id.extend_from_slice(&value.span_id);
        if value.parent_span_id.is_empty() {
            self.parent_span_id.extend_from_slice(&[0_u8; 8]);
        } else {
            self.parent_span_id.extend_from_slice(&value.parent_span_id);
            if let Some(validity) = &mut self.parent_validity {
                validity[self.produced / 8] |= 1 << (self.produced % 8);
            }
        }
        self.flags.push(i64::from(value.flags));
        append_text(&mut self.trace_state, &value.trace_state)?;
        append_text(&mut self.name, &value.name)?;
        append_text(&mut self.kind, kind(value.kind))?;
        let start = nanos_to_micros(value.start_time_unix_nano)?;
        let end = nanos_to_micros(value.end_time_unix_nano)?;
        self.start.push(start);
        self.end.push(end);
        self.duration.push(end.saturating_sub(start) / 1_000);
        append_text(&mut self.status, status(value))?;
        self.attributes.append_attributes(&value.attributes)?;
        self.dropped_attributes
            .push(i64::from(value.dropped_attributes_count));
        self.dropped_events
            .push(i64::from(value.dropped_events_count));
        self.dropped_links
            .push(i64::from(value.dropped_links_count));
        let scope_value = scope.scope.as_ref();
        append_text(
            &mut self.scope_name,
            scope_value
                .filter(|v| !v.name.is_empty())
                .map_or(UNKNOWN_SCOPE, |v| v.name.as_str()),
        )?;
        match scope_value.filter(|v| !v.version.is_empty()) {
            Some(v) => append_text(&mut self.scope_version, &v.version)?,
            None => self.scope_version.append_null()?,
        }
        append_text(&mut self.service_name, service_name(resource))?;
        self.produced = checked_add(self.produced, 1)?;
        Ok(())
    }

    /// Verifies exact fixed capacities and freezes the retained source batch.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on pass divergence or Arrow refusal.
    fn finish(self, rows: usize) -> Result<RecordBatch, ScribeError> {
        let exact = |len: usize, capacity: usize| len == rows && capacity == rows;
        if self.produced != rows
            || self.trace_id.len() != rows * 16
            || self.trace_id.capacity() != rows * 16
            || self.span_id.len() != rows * 8
            || self.span_id.capacity() != rows * 8
            || self.parent_span_id.len() != rows * 8
            || self.parent_span_id.capacity() != rows * 8
            || !exact(self.flags.len(), self.flags.capacity())
            || !exact(self.start.len(), self.start.capacity())
            || !exact(self.end.len(), self.end.capacity())
            || !exact(self.duration.len(), self.duration.capacity())
            || !exact(
                self.dropped_attributes.len(),
                self.dropped_attributes.capacity(),
            )
            || !exact(self.dropped_events.len(), self.dropped_events.capacity())
            || !exact(self.dropped_links.len(), self.dropped_links.capacity())
        {
            return Err(ScribeError::InvalidFrame);
        }
        let timestamp = |v| {
            Arc::new(
                TimestampMicrosecondArray::new(ScalarBuffer::from(v), None).with_timezone("UTC"),
            ) as ArrayRef
        };
        let parent_nulls = self
            .parent_validity
            .map(|v| NullBuffer::new(BooleanBuffer::new(Buffer::from(v), 0, rows)));
        let fixed = |width, values| {
            FixedSizeBinaryArray::try_new(width, Buffer::from(values), None)
                .map(|v| Arc::new(v) as ArrayRef)
                .map_err(|_| ScribeError::InvalidFrame)
        };
        let columns = vec![
            fixed(16, self.trace_id)?,
            fixed(8, self.span_id)?,
            Arc::new(
                FixedSizeBinaryArray::try_new(8, Buffer::from(self.parent_span_id), parent_nulls)
                    .map_err(|_| ScribeError::InvalidFrame)?,
            ) as ArrayRef,
            Arc::new(Int64Array::new(ScalarBuffer::from(self.flags), None)) as ArrayRef,
            Arc::new(self.trace_state.finish()?) as ArrayRef,
            Arc::new(self.name.finish()?) as ArrayRef,
            Arc::new(self.kind.finish()?) as ArrayRef,
            timestamp(self.start),
            timestamp(self.end),
            Arc::new(Int64Array::new(ScalarBuffer::from(self.duration), None)) as ArrayRef,
            Arc::new(self.status.finish()?) as ArrayRef,
            Arc::new(self.attributes.finish()?) as ArrayRef,
            Arc::new(Int64Array::new(
                ScalarBuffer::from(self.dropped_attributes),
                None,
            )) as ArrayRef,
            Arc::new(Int64Array::new(
                ScalarBuffer::from(self.dropped_events),
                None,
            )) as ArrayRef,
            Arc::new(Int64Array::new(
                ScalarBuffer::from(self.dropped_links),
                None,
            )) as ArrayRef,
            Arc::new(self.scope_name.finish()?) as ArrayRef,
            Arc::new(self.scope_version.finish()?) as ArrayRef,
            Arc::new(self.service_name.finish()?) as ArrayRef,
            Arc::new(StringArray::new_null(rows)) as ArrayRef,
        ];
        RecordBatch::try_new(retained_write_schema(), columns)
            .map_err(|_| ScribeError::InvalidFrame)
    }
}

/// Appends one borrowed string into an exact-capacity trace column.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on planned-capacity divergence.
fn append_text(column: &mut ExactStringColumn, value: &str) -> Result<(), ScribeError> {
    column.append_with(|writer| {
        writer.extend_from_slice(value.as_bytes());
        Ok(())
    })
}

/// Builds exact-capacity Arrow columns directly from accepted wire spans.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when either pass or Arrow diverges.
fn materialize(
    request: &ExportTraceServiceRequest,
    plan: TracePlan,
) -> Result<RecordBatch, ScribeError> {
    let mut columns = TraceColumns::new(plan)?;
    for resource in &request.resource_spans {
        for scope in &resource.scope_spans {
            for value in &scope.spans {
                if validate_span(value, resource, scope).is_ok() {
                    columns.append(value, resource, scope)?;
                }
            }
        }
    }
    columns.finish(plan.accepted)
}

/// Returns the directly materialized mapper prefix through nullable `run_id`.
///
/// The authoritative managed projection appends all remaining server-owned
/// columns without allocating the discarded mapper placeholders.
fn retained_write_schema() -> Arc<Schema> {
    let mut fields = SpansTable::arrow_fields();
    fields.push(Field::new(RUN_ID, arrow::datatypes::DataType::Utf8, true));
    Arc::new(Schema::new(fields))
}

/// Returns the existing spans coordinator input schema.
fn write_schema() -> Arc<Schema> {
    let mut fields = SpansTable::arrow_fields();
    fields.push(Field::new(RUN_ID, arrow::datatypes::DataType::Utf8, true));
    fields.push(Field::new(CARD_UID, arrow::datatypes::DataType::Utf8, true));
    fields.push(Field::new(
        PRINCIPAL_ID,
        arrow::datatypes::DataType::Utf8,
        true,
    ));
    Arc::new(Schema::new(fields))
}

/// Validates the exact subset of domain invariants used by the current mapper.
///
/// # Errors
///
/// Returns a stable rejection category for invalid IDs, timestamps, names,
/// scope/resource limits, event windows, links, or attribute cardinalities.
fn validate_span(
    value: &Span,
    resource: &ResourceSpans,
    scope: &wyrd_tonic::otlp::trace::v1::ScopeSpans,
) -> Result<(), &'static str> {
    let trace: [u8; 16] = value
        .trace_id
        .as_slice()
        .try_into()
        .map_err(|_| "trace_id")?;
    TraceId::from_bytes(trace).map_err(|_| "trace_id")?;
    let span_id: [u8; 8] = value.span_id.as_slice().try_into().map_err(|_| "span_id")?;
    SpanId::from_bytes(span_id).map_err(|_| "span_id")?;
    if !value.parent_span_id.is_empty() {
        let parent: [u8; 8] = value
            .parent_span_id
            .as_slice()
            .try_into()
            .map_err(|_| "parent")?;
        SpanId::from_bytes(parent).map_err(|_| "parent")?;
    }
    if i64::try_from(value.start_time_unix_nano).is_err()
        || i64::try_from(value.end_time_unix_nano).is_err()
        || value.end_time_unix_nano < value.start_time_unix_nano
        || value.name.is_empty()
        || value.name.len() > 256
        || unique_keys(&value.attributes) > 256
        || value.trace_state.len() > 512
    {
        return Err("span");
    }
    let service = service_name(resource);
    if service.is_empty() || service.len() > 256 {
        return Err("resource");
    }
    let resource_attributes = resource
        .resource
        .as_ref()
        .map_or(&[][..], |value| value.attributes.as_slice());
    if unique_keys(resource_attributes) > 260 {
        return Err("resource");
    }
    if let Some(scope) = scope.scope.as_ref()
        && ((!scope.name.is_empty() && scope.name.len() > 256)
            || scope.version.len() > 64
            || unique_keys(&scope.attributes) > 32)
    {
        return Err("scope");
    }
    for event in &value.events {
        if event.name.is_empty()
            || event.name.len() > 256
            || unique_keys(&event.attributes) > 128
            || event.time_unix_nano < value.start_time_unix_nano
            || event.time_unix_nano > value.end_time_unix_nano
        {
            return Err("event");
        }
    }
    for link in &value.links {
        let trace: [u8; 16] = link.trace_id.as_slice().try_into().map_err(|_| "link")?;
        TraceId::from_bytes(trace).map_err(|_| "link")?;
        let span: [u8; 8] = link.span_id.as_slice().try_into().map_err(|_| "link")?;
        SpanId::from_bytes(span).map_err(|_| "link")?;
        if link.trace_state.len() > 512 || unique_keys(&link.attributes) > 128 {
            return Err("link");
        }
    }
    Ok(())
}

/// Renders the existing mapper's first rejection without retaining a rejection vector.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] if the stored coordinate diverges.
fn rejection_reason(
    request: &ExportTraceServiceRequest,
    coordinate: RejectionCoordinate,
) -> Result<String, ScribeError> {
    let span = request
        .resource_spans
        .get(coordinate.resource)
        .and_then(|resource| resource.scope_spans.get(coordinate.scope))
        .and_then(|scope| scope.spans.get(coordinate.span))
        .ok_or(ScribeError::InvalidFrame)?;
    if span.trace_id.len() != 16 {
        return Ok(format!(
            "trace_id must be 16 bytes, got {}",
            span.trace_id.len()
        ));
    }
    if span.span_id.len() != 8 {
        return Ok(format!(
            "span_id must be 8 bytes, got {}",
            span.span_id.len()
        ));
    }
    if !span.parent_span_id.is_empty() && span.parent_span_id.len() != 8 {
        return Ok(format!(
            "span_id must be 8 bytes, got {}",
            span.parent_span_id.len()
        ));
    }
    Ok("span validation failed".to_owned())
}

/// Returns the final string-valued `service.name`, matching JSON-map overwrite semantics.
fn service_name(resource: &ResourceSpans) -> &str {
    final_key(
        resource
            .resource
            .as_ref()
            .map_or(&[][..], |value| value.attributes.as_slice()),
        "service.name",
    )
    .and_then(|value| match value.value.as_ref() {
        Some(any_value::Value::StringValue(value)) => Some(value.as_str()),
        _ => None,
    })
    .unwrap_or_default()
}

/// Returns the last value for one OTLP key, matching `serde_json::Map::insert`.
fn final_key<'a>(values: &'a [KeyValue], key: &str) -> Option<&'a AnyValue> {
    values
        .iter()
        .rev()
        .find(|value| value.key == key)
        .and_then(|value| value.value.as_ref())
}

/// Counts unique keys with constant storage.
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

/// Returns the canonical short-form span kind.
fn kind(value: i32) -> &'static str {
    match span::SpanKind::try_from(value).unwrap_or(span::SpanKind::Unspecified) {
        span::SpanKind::Server => "SERVER",
        span::SpanKind::Client => "CLIENT",
        span::SpanKind::Producer => "PRODUCER",
        span::SpanKind::Consumer => "CONSUMER",
        span::SpanKind::Unspecified | span::SpanKind::Internal => "INTERNAL",
    }
}

/// Returns the canonical span status.
fn status(value: &Span) -> &'static str {
    match value
        .status
        .as_ref()
        .and_then(|value| StatusCode::try_from(value.code).ok())
        .unwrap_or(StatusCode::Unset)
    {
        StatusCode::Ok => "OK",
        StatusCode::Error => "ERROR",
        StatusCode::Unset => "UNSET",
    }
}

/// Converts an accepted OTLP nanosecond timestamp into Arrow microseconds.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] if validation and materialization diverge.
fn nanos_to_micros(value: u64) -> Result<i64, ScribeError> {
    i64::try_from(value / 1_000).map_err(|_| ScribeError::InvalidFrame)
}

/// Adds one exact capacity with checked arithmetic.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on `usize` overflow.
fn add(total: &mut usize, value: usize) -> Result<(), ScribeError> {
    *total = checked_add(*total, value)?;
    Ok(())
}

/// Performs checked capacity addition.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on `usize` overflow.
fn checked_add(left: usize, right: usize) -> Result<usize, ScribeError> {
    left.checked_add(right).ok_or(ScribeError::InvalidFrame)
}

/// Counts compact JSON bytes for an OTLP attribute object.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on serialization or length overflow.
fn json_attributes_len(values: &[KeyValue]) -> Result<usize, ScribeError> {
    let mut counter = ByteCounter::default();
    write_attributes(&mut counter, values)?;
    Ok(counter.0)
}

/// Writes a compact JSON object in lexicographic key order, retaining the last duplicate.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on sink or scalar serialization failure.
pub(super) fn write_attributes(
    writer: &mut impl Write,
    values: &[KeyValue],
) -> Result<(), ScribeError> {
    writer
        .write_all(b"{")
        .map_err(|_| ScribeError::InvalidFrame)?;
    let mut prior: Option<&str> = None;
    let mut emitted = 0_usize;
    loop {
        let next = values
            .iter()
            .enumerate()
            .filter(|(index, value)| {
                values[*index + 1..]
                    .iter()
                    .all(|later| later.key != value.key)
                    && prior.is_none_or(|prior| value.key.as_str() > prior)
            })
            .min_by(|left, right| left.1.key.cmp(&right.1.key));
        let Some((_, pair)) = next else { break };
        if emitted != 0 {
            writer
                .write_all(b",")
                .map_err(|_| ScribeError::InvalidFrame)?;
        }
        serde_json::to_writer(&mut *writer, &pair.key).map_err(|_| ScribeError::InvalidFrame)?;
        writer
            .write_all(b":")
            .map_err(|_| ScribeError::InvalidFrame)?;
        write_any(writer, pair.value.as_ref())?;
        prior = Some(&pair.key);
        emitted += 1;
    }
    writer
        .write_all(b"}")
        .map_err(|_| ScribeError::InvalidFrame)
}

/// Writes one OTLP `AnyValue` using the mapper's compact JSON representation.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on sink or scalar serialization failure.
pub(super) fn write_any(
    writer: &mut impl Write,
    value: Option<&AnyValue>,
) -> Result<(), ScribeError> {
    let Some(value) = value.and_then(|value| value.value.as_ref()) else {
        return writer
            .write_all(b"null")
            .map_err(|_| ScribeError::InvalidFrame);
    };
    match value {
        any_value::Value::StringValue(value) => {
            serde_json::to_writer(writer, value).map_err(|_| ScribeError::InvalidFrame)
        }
        any_value::Value::BoolValue(value) => {
            serde_json::to_writer(writer, value).map_err(|_| ScribeError::InvalidFrame)
        }
        any_value::Value::IntValue(value) => {
            serde_json::to_writer(writer, value).map_err(|_| ScribeError::InvalidFrame)
        }
        any_value::Value::DoubleValue(value) if value.is_finite() => {
            serde_json::to_writer(writer, value).map_err(|_| ScribeError::InvalidFrame)
        }
        any_value::Value::DoubleValue(_) => writer
            .write_all(b"null")
            .map_err(|_| ScribeError::InvalidFrame),
        any_value::Value::BytesValue(value) => {
            writer
                .write_all(b"\"")
                .map_err(|_| ScribeError::InvalidFrame)?;
            let mut encoder = base64::write::EncoderWriter::new(
                &mut *writer,
                &base64::engine::general_purpose::STANDARD,
            );
            encoder
                .write_all(value)
                .map_err(|_| ScribeError::InvalidFrame)?;
            let writer = encoder.finish().map_err(|_| ScribeError::InvalidFrame)?;
            writer
                .write_all(b"\"")
                .map_err(|_| ScribeError::InvalidFrame)
        }
        any_value::Value::KvlistValue(value) => write_attributes(writer, &value.values),
        any_value::Value::ArrayValue(value) => {
            writer
                .write_all(b"[")
                .map_err(|_| ScribeError::InvalidFrame)?;
            for (index, value) in value.values.iter().enumerate() {
                if index != 0 {
                    writer
                        .write_all(b",")
                        .map_err(|_| ScribeError::InvalidFrame)?;
                }
                write_any(writer, Some(value))?;
            }
            writer
                .write_all(b"]")
                .map_err(|_| ScribeError::InvalidFrame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::principal::{PrincipalId, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_tonic::otlp::common::v1::InstrumentationScope;
    use wyrd_tonic::otlp::resource::v1::Resource;
    use wyrd_tonic::otlp::trace::v1::{ScopeSpans, Status};

    /// Projects one fixture with deterministic authoritative managed values.
    ///
    /// # Errors
    ///
    /// Returns the direct projector's validation, material-limit, or Arrow
    /// construction error.
    fn project_fixture(
        request: &ExportTraceServiceRequest,
        material_limit: usize,
    ) -> Result<
        (
            Option<crate::scribe::otlp_managed::OtlpManagedBatch>,
            IngestOutcome,
        ),
        ScribeError,
    > {
        let principal = wyrd_runtime::Principal::new(
            PrincipalId::new(uuid::Uuid::from_u128(1)),
            PrincipalKind::User,
            DataTenantId::new(
                uuid::Uuid::parse_str("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01")
                    .expect("valid UUIDv7"),
            )
            .expect("tenant id"),
            Vec::new(),
            PermissionSet::new(),
        );
        let fingerprint =
            crate::contracts::projected_source_schema_fingerprint(write_schema().as_ref());
        let request_id =
            wyrd_spec::request_id::RequestId::parse("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00")
                .expect("valid request id");
        project(
            request,
            material_limit,
            &principal,
            fingerprint,
            &request_id,
            uuid::Uuid::from_u128(3),
            1_700_000_000_000_000,
        )
    }

    /// Computes the exact final Arrow-plus-IPC charge from pre-material facts.
    ///
    /// # Errors
    ///
    /// Returns the planner's validation, fingerprint, or checked-size error.
    fn exact_material_bytes(request: &ExportTraceServiceRequest) -> Result<usize, ScribeError> {
        let source = plan(request)?;
        let principal = wyrd_runtime::Principal::new(
            PrincipalId::new(uuid::Uuid::from_u128(1)),
            PrincipalKind::User,
            DataTenantId::new(
                uuid::Uuid::parse_str("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01")
                    .expect("valid UUIDv7"),
            )
            .expect("tenant id"),
            Vec::new(),
            PermissionSet::new(),
        );
        let fingerprint =
            crate::contracts::projected_source_schema_fingerprint(write_schema().as_ref());
        let request_id =
            wyrd_spec::request_id::RequestId::parse("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00")
                .expect("valid request id");
        let managed = crate::scribe::otlp_managed::OtlpManagedProjection::plan(
            write_schema(),
            fingerprint,
            source.accepted,
            &principal,
            &request_id,
            uuid::Uuid::from_u128(3),
            1_700_000_000_000_000,
        )?;
        validate_material_plan(source, usize::MAX, &managed)?.admitted_bytes(usize::MAX)
    }

    /// Constructs one typed OTLP key/value fixture.
    fn kv(key: &str, value: any_value::Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// Constructs one nontrivial valid trace request used for mapper parity.
    fn request() -> ExportTraceServiceRequest {
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![kv(
                        "service.name",
                        any_value::Value::StringValue("checkout".to_owned()),
                    )],
                    ..Resource::default()
                }),
                scope_spans: vec![ScopeSpans {
                    scope: Some(InstrumentationScope {
                        name: "wyrd-tracing".to_owned(),
                        version: "1.0".to_owned(),
                        ..InstrumentationScope::default()
                    }),
                    spans: vec![Span {
                        trace_id: (1_u8..=16).collect(),
                        span_id: (1_u8..=8).collect(),
                        parent_span_id: (9_u8..=16).collect(),
                        trace_state: "vendor=1".to_owned(),
                        name: "GET /items".to_owned(),
                        kind: span::SpanKind::Server as i32,
                        start_time_unix_nano: 1_000_000_000,
                        end_time_unix_nano: 1_002_000_000,
                        attributes: vec![kv(
                            "http.method",
                            any_value::Value::StringValue("GET".to_owned()),
                        )],
                        status: Some(Status {
                            code: StatusCode::Ok as i32,
                            ..Status::default()
                        }),
                        ..Span::default()
                    }],
                    ..ScopeSpans::default()
                }],
                ..ResourceSpans::default()
            }],
        }
    }

    /// Duplicate attribute keys retain their last value and canonical key order.
    #[test]
    fn direct_json_matches_map_semantics() {
        let value = |value: &str| AnyValue {
            value: Some(any_value::Value::StringValue(value.to_owned())),
        };
        let attributes = vec![
            KeyValue {
                key: "z".to_owned(),
                value: Some(value("old")),
            },
            KeyValue {
                key: "a".to_owned(),
                value: Some(value("first")),
            },
            KeyValue {
                key: "z".to_owned(),
                value: Some(value("new")),
            },
        ];
        let length = json_attributes_len(&attributes).expect("direct JSON length");
        let mut column = ExactStringColumn::new(1, length).expect("column plan");
        column
            .append_attributes(&attributes)
            .expect("direct JSON write");
        let array = column.finish().expect("exact string array");
        let encoded = array.value(0);
        assert_eq!(encoded, r#"{"a":"first","z":"new"}"#);
    }

    /// Direct trace projection is column-for-column identical to the existing mapper.
    #[test]
    fn otlp_projection_parity_traces() {
        let request = request();
        let expected = crate::scribe::test_projection_oracle::project_resource_spans(&request)
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
        assert_eq!(outcome.accepted_spans, 1);
        assert_eq!(outcome.rejected_spans, 0);
    }

    /// Mixed trace exports preserve accepted rows and the first rejection summary.
    #[test]
    fn mixed_projection_matches_mapper_rejection() {
        let mut request = request();
        let mut invalid = request.resource_spans[0].scope_spans[0].spans[0].clone();
        invalid.trace_id = vec![1_u8; 15];
        request.resource_spans[0].scope_spans[0].spans.push(invalid);
        let mapped = crate::gate::collector::map::map_resource_spans(&request.resource_spans);
        let expected_reason = mapped.rejected.first().map(|value| value.reason.clone());
        let (actual, outcome) = project_fixture(&request, usize::MAX).expect("direct projection");
        assert_eq!(
            actual.expect("accepted row").rows.num_rows(),
            mapped.records.len()
        );
        assert_eq!(outcome.accepted_spans, mapped.records.len() as i64);
        assert_eq!(outcome.rejected_spans, mapped.rejected.len() as i64);
        assert_eq!(outcome.rejection_message, expected_reason);
    }

    /// Exact Arrow-plus-IPC capacity passes at `C` and refuses `C - 1`.
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
}
