//! Exact two-pass projection of typed OTLP metrics into one Arrow batch.
//!
//! Planning walks borrowed prost nodes with constant state. Materialization
//! repeats that traversal and writes accepted points directly into fixed Arrow
//! builders, including opaque JSON columns, without constructing metric DTOs.

use std::io::Write;
use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, PrimitiveArray};
use arrow::buffer::{BooleanBuffer, Buffer, NullBuffer, OffsetBuffer, ScalarBuffer};
use arrow::datatypes::{
    ArrowPrimitiveType, Field, Float64Type, Int32Type, Int64Type, Schema, TimestampMicrosecondType,
};
use arrow::record_batch::RecordBatch;
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::managed_columns::{CARD_UID, PRINCIPAL_ID, RUN_ID};
use wyrd_tonic::otlp::metrics::v1::{
    Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint, Metric, NumberDataPoint,
    SummaryDataPoint, exemplar, metric, number_data_point,
};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;

use super::direct_traces::{ByteCounter, ExactStringColumn, write_attributes};
use crate::contracts::ScribeError;
use crate::gate::collector::MetricsOutcome;
use crate::gate::collector::tables::{DomainTable, PointsTable};

/// One borrowed metric point closed over the five OTLP point variants.
#[derive(Clone, Copy)]
enum Point<'a> {
    /// Gauge point.
    Gauge(&'a NumberDataPoint),
    /// Sum point plus its enclosing monotonicity and temporality.
    Sum(&'a NumberDataPoint, i32, bool),
    /// Explicit histogram point plus temporality.
    Histogram(&'a HistogramDataPoint, i32),
    /// Exponential histogram point plus temporality.
    Exponential(&'a ExponentialHistogramDataPoint, i32),
    /// Summary point.
    Summary(&'a SummaryDataPoint),
}

/// Exact nullable UTF-8 capacity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TextCapacity {
    /// Total non-null value bytes.
    bytes: usize,
    /// Whether at least one row is null.
    has_null: bool,
}

/// Exact variable-width material for the 13 metric UTF-8 columns.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TextPlan {
    /// Metric names.
    metric_name: TextCapacity,
    /// Descriptions.
    description: TextCapacity,
    /// Units.
    unit: TextCapacity,
    /// Metric kinds.
    metric_type: TextCapacity,
    /// Temporalities.
    temporality: TextCapacity,
    /// Explicit bucket counts.
    bucket_counts: TextCapacity,
    /// Explicit bounds.
    explicit_bounds: TextCapacity,
    /// Positive exponential buckets.
    positive_buckets: TextCapacity,
    /// Negative exponential buckets.
    negative_buckets: TextCapacity,
    /// Summary quantiles.
    quantiles: TextCapacity,
    /// Exemplars.
    exemplars: TextCapacity,
    /// Point attributes.
    attributes: TextCapacity,
    /// Resource service names.
    service_name: TextCapacity,
    /// Scope names.
    scope_name: TextCapacity,
    /// Scope versions.
    scope_version: TextCapacity,
}

/// Allocation-free metric projection plan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MetricPlan {
    /// Accepted points.
    accepted: usize,
    /// Rejected points.
    rejected: usize,
    /// Traversal ordinal of the first rejection.
    first_rejection: Option<usize>,
    /// Exact UTF-8 capacities.
    text: TextPlan,
}

/// Exact nullable Arrow UTF-8 storage with direct writer access.
#[derive(Debug)]
pub(super) struct NullableText {
    /// Row offsets.
    offsets: Vec<i32>,
    /// Compact values.
    values: Vec<u8>,
    /// Packed validity bits when any null is planned.
    validity: Option<Vec<u8>>,
    /// Planned rows.
    rows: usize,
    /// Planned bytes.
    bytes: usize,
}

/// Exact fixed-width primitive storage with optional packed validity.
#[derive(Debug)]
pub(super) struct ExactPrimitive<T: ArrowPrimitiveType>
where
    T::Native: Default,
{
    /// Exact values, including default bytes in null slots.
    values: Vec<T::Native>,
    /// Packed validity bits when any row is null.
    validity: Option<Vec<u8>>,
    /// Planned rows.
    rows: usize,
}

impl<T: ArrowPrimitiveType> ExactPrimitive<T>
where
    T::Native: Default,
{
    /// Allocates exact value and optional validity capacity.
    pub(super) fn new(rows: usize, has_null: bool) -> Self {
        Self {
            values: Vec::with_capacity(rows),
            validity: has_null.then(|| vec![0_u8; rows.saturating_add(7) / 8]),
            rows,
        }
    }

    /// Appends one optional primitive value.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on nullability or row divergence.
    pub(super) fn append(&mut self, value: Option<T::Native>) -> Result<(), ScribeError> {
        if self.values.len() == self.rows || (value.is_none() && self.validity.is_none()) {
            return Err(ScribeError::InvalidFrame);
        }
        let row = self.values.len();
        if value.is_some()
            && let Some(validity) = &mut self.validity
        {
            validity[row / 8] |= 1 << (row % 8);
        }
        self.values.push(value.unwrap_or_default());
        Ok(())
    }

    /// Freezes exact buffers into one primitive array.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when capacity diverges.
    pub(super) fn finish(self) -> Result<PrimitiveArray<T>, ScribeError> {
        if self.values.len() != self.rows || self.values.capacity() != self.rows {
            return Err(ScribeError::InvalidFrame);
        }
        let nulls = self
            .validity
            .map(|bits| NullBuffer::new(BooleanBuffer::new(Buffer::from(bits), 0, self.rows)));
        Ok(PrimitiveArray::new(ScalarBuffer::from(self.values), nulls))
    }
}

/// Exact Boolean value and validity bitmaps.
#[derive(Debug)]
struct ExactBoolean {
    /// Packed value bits.
    values: Vec<u8>,
    /// Packed validity bits when any row is null.
    validity: Option<Vec<u8>>,
    /// Planned rows.
    rows: usize,
    /// Appended rows.
    len: usize,
}

impl ExactBoolean {
    /// Allocates exact public bitmap capacities.
    fn new(rows: usize, has_null: bool) -> Self {
        let bytes = rows.saturating_add(7) / 8;
        Self {
            values: vec![0_u8; bytes],
            validity: has_null.then(|| vec![0_u8; bytes]),
            rows,
            len: 0,
        }
    }

    /// Appends one optional Boolean bit.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on nullability or row divergence.
    fn append(&mut self, value: Option<bool>) -> Result<(), ScribeError> {
        if self.len == self.rows || (value.is_none() && self.validity.is_none()) {
            return Err(ScribeError::InvalidFrame);
        }
        if value == Some(true) {
            self.values[self.len / 8] |= 1 << (self.len % 8);
        }
        if value.is_some()
            && let Some(validity) = &mut self.validity
        {
            validity[self.len / 8] |= 1 << (self.len % 8);
        }
        self.len += 1;
        Ok(())
    }

    /// Freezes exact bitmaps into one Boolean array.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on row divergence.
    fn finish(self) -> Result<BooleanArray, ScribeError> {
        if self.len != self.rows {
            return Err(ScribeError::InvalidFrame);
        }
        let values = BooleanBuffer::new(Buffer::from(self.values), 0, self.rows);
        let nulls = self
            .validity
            .map(|bits| NullBuffer::new(BooleanBuffer::new(Buffer::from(bits), 0, self.rows)));
        Ok(BooleanArray::new(values, nulls))
    }
}

impl NullableText {
    /// Allocates exact offset, value, and optional validity storage.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when counts exceed Arrow offset bounds.
    pub(super) fn new_parts(
        rows: usize,
        bytes: usize,
        has_null: bool,
    ) -> Result<Self, ScribeError> {
        Self::new(rows, TextCapacity { bytes, has_null })
    }

    /// Allocates exact storage from a metrics text-capacity fact.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when counts exceed Arrow offset bounds.
    fn new(rows: usize, plan: TextCapacity) -> Result<Self, ScribeError> {
        i32::try_from(plan.bytes).map_err(|_| ScribeError::InvalidFrame)?;
        let offset_count = rows.checked_add(1).ok_or(ScribeError::InvalidFrame)?;
        let mut offsets = Vec::with_capacity(offset_count);
        offsets.push(0);
        let validity = plan
            .has_null
            .then(|| vec![0_u8; rows.saturating_add(7) / 8]);
        Ok(Self {
            offsets,
            values: Vec::with_capacity(plan.bytes),
            validity,
            rows,
            bytes: plan.bytes,
        })
    }

    /// Appends one null without touching the value buffer.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] if nullability diverges from plan.
    pub(super) fn append_null(&mut self) -> Result<(), ScribeError> {
        if self.validity.is_none() || self.offsets.len() > self.rows {
            return Err(ScribeError::InvalidFrame);
        }
        let offset = *self.offsets.last().ok_or(ScribeError::InvalidFrame)?;
        self.offsets.push(offset);
        Ok(())
    }

    /// Writes one value directly into fixed Arrow storage.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on writer, offset, or row divergence.
    pub(super) fn append(
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
        self.offsets
            .push(i32::try_from(self.values.len()).map_err(|_| ScribeError::InvalidFrame)?);
        Ok(())
    }

    /// Freezes exact storage into an Arrow UTF-8 array.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on capacity or Arrow divergence.
    pub(super) fn finish(self) -> Result<arrow::array::StringArray, ScribeError> {
        if self.offsets.len() != self.rows + 1
            || self.offsets.capacity() != self.rows + 1
            || self.values.len() != self.bytes
            || self.values.capacity() != self.bytes
        {
            return Err(ScribeError::InvalidFrame);
        }
        let nulls = self
            .validity
            .map(|bits| NullBuffer::new(BooleanBuffer::new(Buffer::from(bits), 0, self.rows)));
        arrow::array::StringArray::try_new(
            OffsetBuffer::new(ScalarBuffer::from(self.offsets)),
            Buffer::from(self.values),
            nulls,
        )
        .map_err(|_| ScribeError::InvalidFrame)
    }
}

/// Projects one typed metrics request with an exact count pass followed by
/// fixed-capacity direct Arrow construction and authoritative managed stamping.
/// The immutable caller context is planned before source materialization so
/// the limit covers the final persisted schema and IPC frame.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on checked overflow, serialization,
/// Arrow construction, fingerprint or managed-context validation, material
/// refusal, or pass divergence.
pub(crate) fn project(
    request: &ExportMetricsServiceRequest,
    material_limit: usize,
    principal: &wyrd_runtime::Principal,
    expected_fingerprint: crate::schema::SchemaFingerprint,
    request_id: &wyrd_spec::request_id::RequestId,
    batch_id: uuid::Uuid,
    receipt_micros: i64,
) -> Result<
    (
        Option<crate::scribe::otlp_managed::OtlpManagedBatch>,
        MetricsOutcome,
    ),
    ScribeError,
> {
    let plan = plan(request)?;
    let outcome = MetricsOutcome {
        accepted_points: i64::try_from(plan.accepted).unwrap_or(i64::MAX),
        rejected_points: i64::try_from(plan.rejected).unwrap_or(i64::MAX),
        rejection_message: plan
            .first_rejection
            .map(|ordinal| rejection_reason(request, ordinal))
            .transpose()?,
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
    let material = validate_material_plan(request, &plan, material_limit, &managed)?;
    let batch = materialize(request, &plan)?;
    Ok((Some(managed.finish(batch, material)?), outcome))
}

/// Refuses an oversized metric slice from exact borrowed-source buffer facts.
///
/// # Errors
///
/// Returns a stable material refusal when combined Arrow and IPC backing
/// exceeds `material_limit`, or invalid-frame when the fact walk diverges.
fn validate_material_plan(
    request: &ExportMetricsServiceRequest,
    plan: &MetricPlan,
    material_limit: usize,
    managed: &crate::scribe::otlp_managed::OtlpManagedProjection,
) -> Result<crate::scribe::otlp_managed::OtlpManagedMaterialPlan, ScribeError> {
    use crate::scribe::fixed_ipc::FixedIpcColumnPlan;

    let rows = plan.accepted;
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
    let boolean = FixedIpcColumnPlan {
        null_count: nulls[7],
        validity_bytes: usize::from(nulls[7] != 0) * bitmap,
        offsets_bytes: 0,
        values_bytes: bitmap,
    };
    let text = &plan.text;
    let columns = [
        utf8(text.metric_name.bytes, 0),
        fixed(8, 1)?,
        fixed(8, 2)?,
        utf8(text.description.bytes, 3),
        utf8(text.unit.bytes, 4),
        utf8(text.metric_type.bytes, 5),
        utf8(text.temporality.bytes, 6),
        boolean,
        fixed(8, 8)?,
        fixed(8, 9)?,
        fixed(8, 10)?,
        fixed(8, 11)?,
        fixed(8, 12)?,
        fixed(8, 13)?,
        utf8(text.bucket_counts.bytes, 14),
        utf8(text.explicit_bounds.bytes, 15),
        fixed(4, 16)?,
        fixed(8, 17)?,
        fixed(8, 18)?,
        utf8(text.positive_buckets.bytes, 19),
        utf8(text.negative_buckets.bytes, 20),
        utf8(text.quantiles.bytes, 21),
        utf8(text.exemplars.bytes, 22),
        utf8(text.attributes.bytes, 23),
        utf8(text.service_name.bytes, 24),
        utf8(text.scope_name.bytes, 25),
        utf8(text.scope_version.bytes, 26),
        utf8(0, 27),
        utf8(0, 28),
        utf8(0, 29),
    ];
    let final_plan = managed.material_plan(columns.into_iter())?;
    final_plan.admitted_bytes(material_limit)?;
    Ok(final_plan)
}

/// Counts exact nulls for every metrics schema column.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on checked counter overflow.
fn material_nulls(request: &ExportMetricsServiceRequest) -> Result<[usize; 30], ScribeError> {
    let mut nulls = [0_usize; 30];
    visit(request, |metric, _resource, scope, point| {
        if validate(point).is_err() {
            return Ok(());
        }
        let scope = scope
            .and_then(|value| value.scope.as_ref())
            .filter(|value| !value.name.is_empty());
        let absent = [
            false,
            false,
            point.start_time() == 0,
            metric.description.is_empty(),
            metric.unit.is_empty(),
            false,
            point.temporality().is_none(),
            point.monotonic().is_none(),
            point.flags().is_none(),
            point.value().is_none(),
            point.count().is_none(),
            point.sum().is_none(),
            point.min().is_none(),
            point.max().is_none(),
            point.bucket_counts().is_none(),
            point.explicit_bounds().is_none(),
            point.scale().is_none(),
            point.zero_count().is_none(),
            point.zero_threshold().is_none(),
            point.positive().is_none(),
            point.negative().is_none(),
            point.quantiles().is_none(),
            point.exemplars().is_empty(),
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

/// Counts accepted points and exact Arrow text capacities without allocation.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on checked arithmetic or JSON sizing failure.
fn plan(request: &ExportMetricsServiceRequest) -> Result<MetricPlan, ScribeError> {
    let mut plan = MetricPlan::default();
    let mut ordinal = 0_usize;
    visit(request, |metric, resource, scope, point| {
        let current = ordinal;
        ordinal = add(ordinal, 1)?;
        if validate(point).is_err() {
            plan.rejected = add(plan.rejected, 1)?;
            plan.first_rejection.get_or_insert(current);
            return Ok(());
        }
        plan.accepted = add(plan.accepted, 1)?;
        count_text(&mut plan.text.metric_name, Some(&metric.name))?;
        count_text(
            &mut plan.text.description,
            (!metric.description.is_empty()).then_some(metric.description.as_str()),
        )?;
        count_text(
            &mut plan.text.unit,
            (!metric.unit.is_empty()).then_some(metric.unit.as_str()),
        )?;
        count_text(&mut plan.text.metric_type, Some(point.kind()))?;
        count_text(&mut plan.text.temporality, point.temporality())?;
        count_json_columns(&mut plan.text, point)?;
        let attributes = point.attributes();
        count_writer(&mut plan.text.attributes, true, |writer| {
            write_attributes(writer, attributes)
        })?;
        count_text(&mut plan.text.service_name, Some(service_name(resource)))?;
        let scope = scope.and_then(|value| value.scope.as_ref());
        count_text(
            &mut plan.text.scope_name,
            scope
                .filter(|value| !value.name.is_empty())
                .map(|value| value.name.as_str()),
        )?;
        count_text(
            &mut plan.text.scope_version,
            scope
                .filter(|value| !value.name.is_empty() && !value.version.is_empty())
                .map(|value| value.version.as_str()),
        )?;
        Ok(())
    })?;
    Ok(plan)
}

/// Owns the exact-capacity mutable Arrow columns for one metrics material pass.
struct MetricColumns {
    /// Metric names.
    metric_name: ExactStringColumn,
    /// Point timestamps.
    time: ExactPrimitive<TimestampMicrosecondType>,
    /// Optional point start timestamps.
    start_time: ExactPrimitive<TimestampMicrosecondType>,
    /// Optional metric descriptions.
    description: NullableText,
    /// Optional metric units.
    unit: NullableText,
    /// Metric shape names.
    metric_type: ExactStringColumn,
    /// Optional aggregation temporalities.
    temporality: NullableText,
    /// Optional monotonicity flags.
    monotonic: ExactBoolean,
    /// Optional point flags.
    flags: ExactPrimitive<Int64Type>,
    /// Optional scalar values.
    value: ExactPrimitive<Float64Type>,
    /// Optional aggregate counts.
    count: ExactPrimitive<Int64Type>,
    /// Optional aggregate sums.
    sum: ExactPrimitive<Float64Type>,
    /// Optional aggregate minima.
    min: ExactPrimitive<Float64Type>,
    /// Optional aggregate maxima.
    max: ExactPrimitive<Float64Type>,
    /// Optional histogram bucket counts.
    bucket_counts: NullableText,
    /// Optional histogram bounds.
    explicit_bounds: NullableText,
    /// Optional exponential scales.
    scale: ExactPrimitive<Int32Type>,
    /// Optional exponential zero counts.
    zero_count: ExactPrimitive<Int64Type>,
    /// Optional exponential zero thresholds.
    zero_threshold: ExactPrimitive<Float64Type>,
    /// Optional positive exponential buckets.
    positive: NullableText,
    /// Optional negative exponential buckets.
    negative: NullableText,
    /// Optional summary quantiles.
    quantiles: NullableText,
    /// Optional exemplars.
    exemplars: NullableText,
    /// Point attributes.
    attributes: ExactStringColumn,
    /// Resource service names.
    service_name: ExactStringColumn,
    /// Optional scope names.
    scope_name: NullableText,
    /// Optional scope versions.
    scope_version: NullableText,
}

impl MetricColumns {
    /// Allocates every mutable column at the exact immutable planned capacity.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on checked capacity overflow.
    fn new(plan: &MetricPlan, nulls: &[usize; 30]) -> Result<Self, ScribeError> {
        let n = plan.accepted;
        Ok(Self {
            metric_name: ExactStringColumn::new(n, plan.text.metric_name.bytes)?,
            time: ExactPrimitive::new(n, false),
            start_time: ExactPrimitive::new(n, nulls[2] != 0),
            description: NullableText::new_parts(n, plan.text.description.bytes, nulls[3] != 0)?,
            unit: NullableText::new_parts(n, plan.text.unit.bytes, nulls[4] != 0)?,
            metric_type: ExactStringColumn::new(n, plan.text.metric_type.bytes)?,
            temporality: NullableText::new_parts(n, plan.text.temporality.bytes, nulls[6] != 0)?,
            monotonic: ExactBoolean::new(n, nulls[7] != 0),
            flags: ExactPrimitive::new(n, nulls[8] != 0),
            value: ExactPrimitive::new(n, nulls[9] != 0),
            count: ExactPrimitive::new(n, nulls[10] != 0),
            sum: ExactPrimitive::new(n, nulls[11] != 0),
            min: ExactPrimitive::new(n, nulls[12] != 0),
            max: ExactPrimitive::new(n, nulls[13] != 0),
            bucket_counts: NullableText::new(n, plan.text.bucket_counts)?,
            explicit_bounds: NullableText::new(n, plan.text.explicit_bounds)?,
            scale: ExactPrimitive::new(n, nulls[16] != 0),
            zero_count: ExactPrimitive::new(n, nulls[17] != 0),
            zero_threshold: ExactPrimitive::new(n, nulls[18] != 0),
            positive: NullableText::new(n, plan.text.positive_buckets)?,
            negative: NullableText::new(n, plan.text.negative_buckets)?,
            quantiles: NullableText::new(n, plan.text.quantiles)?,
            exemplars: NullableText::new(n, plan.text.exemplars)?,
            attributes: ExactStringColumn::new(n, plan.text.attributes.bytes)?,
            service_name: ExactStringColumn::new(n, plan.text.service_name.bytes)?,
            scope_name: NullableText::new_parts(n, plan.text.scope_name.bytes, nulls[25] != 0)?,
            scope_version: NullableText::new_parts(
                n,
                plan.text.scope_version.bytes,
                nulls[26] != 0,
            )?,
        })
    }

    /// Freezes the exact columns into the retained metrics source schema.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on exact fill or Arrow divergence.
    fn finish(self, rows: usize) -> Result<RecordBatch, ScribeError> {
        let columns = vec![
            Arc::new(self.metric_name.finish()?) as ArrayRef,
            Arc::new(self.time.finish()?.with_timezone("UTC")) as ArrayRef,
            Arc::new(self.start_time.finish()?.with_timezone("UTC")) as ArrayRef,
            Arc::new(self.description.finish()?) as ArrayRef,
            Arc::new(self.unit.finish()?) as ArrayRef,
            Arc::new(self.metric_type.finish()?) as ArrayRef,
            Arc::new(self.temporality.finish()?) as ArrayRef,
            Arc::new(self.monotonic.finish()?) as ArrayRef,
            Arc::new(self.flags.finish()?) as ArrayRef,
            Arc::new(self.value.finish()?) as ArrayRef,
            Arc::new(self.count.finish()?) as ArrayRef,
            Arc::new(self.sum.finish()?) as ArrayRef,
            Arc::new(self.min.finish()?) as ArrayRef,
            Arc::new(self.max.finish()?) as ArrayRef,
            Arc::new(self.bucket_counts.finish()?) as ArrayRef,
            Arc::new(self.explicit_bounds.finish()?) as ArrayRef,
            Arc::new(self.scale.finish()?) as ArrayRef,
            Arc::new(self.zero_count.finish()?) as ArrayRef,
            Arc::new(self.zero_threshold.finish()?) as ArrayRef,
            Arc::new(self.positive.finish()?) as ArrayRef,
            Arc::new(self.negative.finish()?) as ArrayRef,
            Arc::new(self.quantiles.finish()?) as ArrayRef,
            Arc::new(self.exemplars.finish()?) as ArrayRef,
            Arc::new(self.attributes.finish()?) as ArrayRef,
            Arc::new(self.service_name.finish()?) as ArrayRef,
            Arc::new(self.scope_name.finish()?) as ArrayRef,
            Arc::new(self.scope_version.finish()?) as ArrayRef,
            all_null_text(rows)?,
        ];
        RecordBatch::try_new(retained_write_schema(), columns)
            .map_err(|_| ScribeError::InvalidFrame)
    }
}

/// Counts every opaque JSON column for one accepted point.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on serialization or checked overflow.
fn count_json_columns(text: &mut TextPlan, point: Point<'_>) -> Result<(), ScribeError> {
    count_optional_writer(&mut text.bucket_counts, point.bucket_counts(), write_json)?;
    count_optional_writer(
        &mut text.explicit_bounds,
        point.explicit_bounds(),
        write_json,
    )?;
    count_optional_writer(&mut text.positive_buckets, point.positive(), write_buckets)?;
    count_optional_writer(&mut text.negative_buckets, point.negative(), write_buckets)?;
    count_optional_writer(&mut text.quantiles, point.quantiles(), write_quantiles)?;
    let exemplars = point.exemplars();
    count_writer(&mut text.exemplars, !exemplars.is_empty(), |writer| {
        write_exemplars(writer, exemplars)
    })
}

/// Materializes accepted points into the existing metrics schema.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on pass or Arrow divergence.
fn materialize(
    request: &ExportMetricsServiceRequest,
    plan: &MetricPlan,
) -> Result<RecordBatch, ScribeError> {
    let n = plan.accepted;
    let nulls = material_nulls(request)?;
    let mut columns = MetricColumns::new(plan, &nulls)?;
    let mut produced = 0_usize;
    visit(request, |metric, resource, scope, point| {
        if validate(point).is_err() {
            return Ok(());
        }
        append_exact_text(&mut columns.metric_name, &metric.name)?;
        columns.time.append(Some(nanos(point.time())?))?;
        columns
            .start_time
            .append(optional_nanos(point.start_time()))?;
        append_nullable_text(
            &mut columns.description,
            (!metric.description.is_empty()).then_some(metric.description.as_str()),
        )?;
        append_nullable_text(
            &mut columns.unit,
            (!metric.unit.is_empty()).then_some(metric.unit.as_str()),
        )?;
        append_exact_text(&mut columns.metric_type, point.kind())?;
        append_nullable_text(&mut columns.temporality, point.temporality())?;
        columns.monotonic.append(point.monotonic())?;
        columns.flags.append(point.flags().map(i64::from))?;
        columns.value.append(point.value())?;
        columns.count.append(point.count().map(signed))?;
        columns.sum.append(point.sum())?;
        columns.min.append(point.min())?;
        columns.max.append(point.max())?;
        append_json(
            &mut columns.bucket_counts,
            point.bucket_counts(),
            write_json,
        )?;
        append_json(
            &mut columns.explicit_bounds,
            point.explicit_bounds(),
            write_json,
        )?;
        columns.scale.append(point.scale())?;
        columns.zero_count.append(point.zero_count().map(signed))?;
        columns.zero_threshold.append(point.zero_threshold())?;
        append_json(&mut columns.positive, point.positive(), write_buckets)?;
        append_json(&mut columns.negative, point.negative(), write_buckets)?;
        append_json(&mut columns.quantiles, point.quantiles(), write_quantiles)?;
        if point.exemplars().is_empty() {
            columns.exemplars.append_null()?;
        } else {
            columns
                .exemplars
                .append(|writer| write_exemplars(writer, point.exemplars()))?;
        }
        columns
            .attributes
            .append_with(|writer| write_attributes(writer, point.attributes()))?;
        append_exact_text(&mut columns.service_name, service_name(resource))?;
        let scope = scope
            .and_then(|value| value.scope.as_ref())
            .filter(|value| !value.name.is_empty());
        append_nullable_text(
            &mut columns.scope_name,
            scope.map(|value| value.name.as_str()),
        )?;
        append_nullable_text(
            &mut columns.scope_version,
            scope
                .filter(|value| !value.version.is_empty())
                .map(|value| value.version.as_str()),
        )?;
        produced = add(produced, 1)?;
        Ok(())
    })?;
    if produced != n {
        return Err(ScribeError::InvalidFrame);
    }
    columns.finish(n)
}

/// Builds an exact all-null UTF-8 column for a managed nullable placeholder.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] if exact allocation or fill diverges.
fn all_null_text(rows: usize) -> Result<ArrayRef, ScribeError> {
    let mut column = NullableText::new_parts(rows, 0, true)?;
    for _ in 0..rows {
        column.append_null()?;
    }
    Ok(Arc::new(column.finish()?) as ArrayRef)
}

/// Visits every point without constructing an outer or per-signal collection.
///
/// # Errors
///
/// Propagates the visitor's checked planning or materialization error.
fn visit(
    request: &ExportMetricsServiceRequest,
    mut visit: impl FnMut(
        &Metric,
        &wyrd_tonic::otlp::metrics::v1::ResourceMetrics,
        Option<&wyrd_tonic::otlp::metrics::v1::ScopeMetrics>,
        Point<'_>,
    ) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
    for resource in &request.resource_metrics {
        for scope in &resource.scope_metrics {
            for metric in &scope.metrics {
                match metric.data.as_ref() {
                    Some(metric::Data::Gauge(value)) => {
                        for point in &value.data_points {
                            visit(metric, resource, Some(scope), Point::Gauge(point))?;
                        }
                    }
                    Some(metric::Data::Sum(value)) => {
                        for point in &value.data_points {
                            visit(
                                metric,
                                resource,
                                Some(scope),
                                Point::Sum(
                                    point,
                                    value.aggregation_temporality,
                                    value.is_monotonic,
                                ),
                            )?;
                        }
                    }
                    Some(metric::Data::Histogram(value)) => {
                        for point in &value.data_points {
                            visit(
                                metric,
                                resource,
                                Some(scope),
                                Point::Histogram(point, value.aggregation_temporality),
                            )?;
                        }
                    }
                    Some(metric::Data::ExponentialHistogram(value)) => {
                        for point in &value.data_points {
                            visit(
                                metric,
                                resource,
                                Some(scope),
                                Point::Exponential(point, value.aggregation_temporality),
                            )?;
                        }
                    }
                    Some(metric::Data::Summary(value)) => {
                        for point in &value.data_points {
                            visit(metric, resource, Some(scope), Point::Summary(point))?;
                        }
                    }
                    None => {}
                }
            }
        }
    }
    Ok(())
}

impl<'a> Point<'a> {
    /// Returns the canonical metric kind string.
    fn kind(self) -> &'static str {
        match self {
            Self::Gauge(_) => "gauge",
            Self::Sum(..) => "sum",
            Self::Histogram(..) => "histogram",
            Self::Exponential(..) => "exponential_histogram",
            Self::Summary(_) => "summary",
        }
    }
    /// Returns canonical temporality when specified.
    fn temporality(self) -> Option<&'static str> {
        let raw = match self {
            Self::Sum(_, value, _) | Self::Histogram(_, value) | Self::Exponential(_, value) => {
                Some(value)
            }
            _ => None,
        }?;
        match raw {
            1 => Some("delta"),
            2 => Some("cumulative"),
            _ => None,
        }
    }
    /// Returns point event time.
    fn time(self) -> u64 {
        match self {
            Self::Gauge(v) | Self::Sum(v, ..) => v.time_unix_nano,
            Self::Histogram(v, _) => v.time_unix_nano,
            Self::Exponential(v, _) => v.time_unix_nano,
            Self::Summary(v) => v.time_unix_nano,
        }
    }
    /// Returns optional start time sentinel.
    fn start_time(self) -> u64 {
        match self {
            Self::Gauge(v) | Self::Sum(v, ..) => v.start_time_unix_nano,
            Self::Histogram(v, _) => v.start_time_unix_nano,
            Self::Exponential(v, _) => v.start_time_unix_nano,
            Self::Summary(v) => v.start_time_unix_nano,
        }
    }
    /// Returns point flags.
    fn flags(self) -> Option<u32> {
        let v = match self {
            Self::Gauge(v) | Self::Sum(v, ..) => v.flags,
            Self::Histogram(v, _) => v.flags,
            Self::Exponential(v, _) => v.flags,
            Self::Summary(v) => v.flags,
        };
        (v != 0).then_some(v)
    }
    /// Returns monotonicity for sums.
    fn monotonic(self) -> Option<bool> {
        match self {
            Self::Sum(_, _, value) => Some(value),
            _ => None,
        }
    }
    /// Returns scalar value.
    fn value(self) -> Option<f64> {
        match self {
            Self::Gauge(v) | Self::Sum(v, ..) => v.value.as_ref().map(|v| match v {
                number_data_point::Value::AsDouble(v) => *v,
                number_data_point::Value::AsInt(v) => metric_integer_as_f64(*v),
            }),
            _ => None,
        }
    }
    /// Returns count.
    fn count(self) -> Option<u64> {
        match self {
            Self::Histogram(v, _) => Some(v.count),
            Self::Exponential(v, _) => Some(v.count),
            Self::Summary(v) => Some(v.count),
            _ => None,
        }
    }
    /// Returns sum.
    fn sum(self) -> Option<f64> {
        match self {
            Self::Histogram(v, _) => v.sum,
            Self::Exponential(v, _) => v.sum,
            Self::Summary(v) => Some(v.sum),
            _ => None,
        }
    }
    /// Returns minimum.
    fn min(self) -> Option<f64> {
        match self {
            Self::Histogram(v, _) => v.min,
            Self::Exponential(v, _) => v.min,
            _ => None,
        }
    }
    /// Returns maximum.
    fn max(self) -> Option<f64> {
        match self {
            Self::Histogram(v, _) => v.max,
            Self::Exponential(v, _) => v.max,
            _ => None,
        }
    }
    /// Returns explicit bucket counts when mapper represents them.
    fn bucket_counts(self) -> Option<&'a [u64]> {
        match self {
            Self::Histogram(v, _)
                if !v.bucket_counts.is_empty() || !v.explicit_bounds.is_empty() =>
            {
                Some(&v.bucket_counts)
            }
            _ => None,
        }
    }
    /// Returns explicit bounds when mapper represents them.
    fn explicit_bounds(self) -> Option<&'a [f64]> {
        match self {
            Self::Histogram(v, _)
                if !v.bucket_counts.is_empty() || !v.explicit_bounds.is_empty() =>
            {
                Some(&v.explicit_bounds)
            }
            _ => None,
        }
    }
    /// Returns exponential scale.
    fn scale(self) -> Option<i32> {
        match self {
            Self::Exponential(v, _) => Some(v.scale),
            _ => None,
        }
    }
    /// Returns zero count.
    fn zero_count(self) -> Option<u64> {
        match self {
            Self::Exponential(v, _) => Some(v.zero_count),
            _ => None,
        }
    }
    /// Returns zero threshold.
    fn zero_threshold(self) -> Option<f64> {
        match self {
            Self::Exponential(v, _) => Some(v.zero_threshold),
            _ => None,
        }
    }
    /// Returns positive buckets.
    fn positive(
        self,
    ) -> Option<&'a wyrd_tonic::otlp::metrics::v1::exponential_histogram_data_point::Buckets> {
        match self {
            Self::Exponential(v, _) => v.positive.as_ref(),
            _ => None,
        }
    }
    /// Returns negative buckets.
    fn negative(
        self,
    ) -> Option<&'a wyrd_tonic::otlp::metrics::v1::exponential_histogram_data_point::Buckets> {
        match self {
            Self::Exponential(v, _) => v.negative.as_ref(),
            _ => None,
        }
    }
    /// Returns summary quantiles.
    fn quantiles(
        self,
    ) -> Option<&'a [wyrd_tonic::otlp::metrics::v1::summary_data_point::ValueAtQuantile]> {
        match self {
            Self::Summary(v) => Some(&v.quantile_values),
            _ => None,
        }
    }
    /// Returns point exemplars.
    fn exemplars(self) -> &'a [Exemplar] {
        match self {
            Self::Gauge(v) | Self::Sum(v, ..) => &v.exemplars,
            Self::Histogram(v, _) => &v.exemplars,
            Self::Exponential(v, _) => &v.exemplars,
            Self::Summary(_) => &[],
        }
    }
    /// Returns point attributes.
    fn attributes(self) -> &'a [wyrd_tonic::otlp::common::v1::KeyValue] {
        match self {
            Self::Gauge(v) | Self::Sum(v, ..) => &v.attributes,
            Self::Histogram(v, _) => &v.attributes,
            Self::Exponential(v, _) => &v.attributes,
            Self::Summary(v) => &v.attributes,
        }
    }
}

/// Validates point and exemplar invariants without allocation.
///
/// # Errors
///
/// Returns `()` for out-of-range timestamps, absent scalar values, malformed
/// histogram buckets/quantiles, or invalid exemplar identifiers and filtered
/// attributes.
fn validate(point: Point<'_>) -> Result<(), ()> {
    if i64::try_from(point.time()).is_err()
        || matches!(point, Point::Gauge(v) | Point::Sum(v, ..) if v.value.is_none())
    {
        return Err(());
    }
    if let Point::Histogram(value, _) = point
        && (!value.bucket_counts.is_empty() || !value.explicit_bounds.is_empty())
        && value.bucket_counts.len() != value.explicit_bounds.len() + 1
    {
        return Err(());
    }
    if let Point::Summary(value) = point
        && (value.quantile_values.is_empty()
            || value
                .quantile_values
                .iter()
                .any(|v| !(0.0..=1.0).contains(&v.quantile)))
    {
        return Err(());
    }
    for exemplar in point.exemplars() {
        if i64::try_from(exemplar.time_unix_nano).is_err()
            || (!exemplar.span_id.is_empty() && exemplar.trace_id.is_empty())
        {
            return Err(());
        }
        if !exemplar.trace_id.is_empty() {
            let id: [u8; 16] = exemplar.trace_id.as_slice().try_into().map_err(|_| ())?;
            TraceId::from_bytes(id).map_err(|_| ())?;
        }
        if !exemplar.span_id.is_empty() {
            let id: [u8; 8] = exemplar.span_id.as_slice().try_into().map_err(|_| ())?;
            SpanId::from_bytes(id).map_err(|_| ())?;
        }
    }
    Ok(())
}

/// Renders the legacy mapper reason for the first rejected metric point.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the planned ordinal cannot be
/// resolved during the identical borrowed traversal.
fn rejection_reason(
    request: &ExportMetricsServiceRequest,
    target: usize,
) -> Result<String, ScribeError> {
    let mut ordinal = 0_usize;
    let mut reason = None;
    visit(request, |metric, _resource, _scope, point| {
        let current = ordinal;
        ordinal = add(ordinal, 1)?;
        if current != target {
            return Ok(());
        }
        reason = Some(if i64::try_from(point.time()).is_err() {
            "metric time_unix_nano out of representable range".to_owned()
        } else if matches!(point, Point::Gauge(value) | Point::Sum(value, ..) if value.value.is_none())
        {
            format!("metric {} number point carries no value", metric.name)
        } else {
            "metric point validation failed".to_owned()
        });
        Ok(())
    })?;
    reason.ok_or(ScribeError::InvalidFrame)
}

/// Returns the final string `service.name` resource attribute.
fn service_name(resource: &wyrd_tonic::otlp::metrics::v1::ResourceMetrics) -> &str {
    resource
        .resource
        .as_ref()
        .and_then(|r| r.attributes.iter().rev().find(|v| v.key == "service.name"))
        .and_then(|v| v.value.as_ref())
        .and_then(|v| match v.value.as_ref() {
            Some(wyrd_tonic::otlp::common::v1::any_value::Value::StringValue(v)) => {
                Some(v.as_str())
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// Counts one optional direct string.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the planned UTF-8 byte count
/// overflows.
fn count_text(plan: &mut TextCapacity, value: Option<&str>) -> Result<(), ScribeError> {
    match value {
        Some(v) => plan.bytes = add(plan.bytes, v.len())?,
        None => plan.has_null = true,
    }
    Ok(())
}
/// Counts one optional direct writer.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when `write` rejects the value or the
/// resulting exact byte count overflows.
fn count_writer(
    plan: &mut TextCapacity,
    present: bool,
    write: impl FnOnce(&mut ByteCounter) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
    if !present {
        plan.has_null = true;
        return Ok(());
    }
    let mut counter = ByteCounter::default();
    write(&mut counter)?;
    plan.bytes = add(plan.bytes, counter.0)?;
    Ok(())
}
/// Counts one optional borrowed JSON value.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when JSON counting fails or the
/// resulting exact byte count overflows.
fn count_optional_writer<T: ?Sized>(
    plan: &mut TextCapacity,
    value: Option<&T>,
    write: fn(&mut ByteCounter, &T) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
    if let Some(v) = value {
        count_writer(plan, true, |w| write(w, v))
    } else {
        plan.has_null = true;
        Ok(())
    }
}
/// Appends one optional JSON value.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when JSON writing fails or the
/// materialized nullable-text storage diverges from its planned capacity.
fn append_json<T: ?Sized>(
    column: &mut NullableText,
    value: Option<&T>,
    write: fn(&mut Vec<u8>, &T) -> Result<(), ScribeError>,
) -> Result<(), ScribeError> {
    match value {
        Some(v) => column.append(|w| write(w, v)),
        None => column.append_null(),
    }
}
/// Writes any serde-compatible borrowed slice as compact JSON.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when compact JSON serialization or
/// the destination writer fails.
fn write_json<T: serde::Serialize + ?Sized>(
    writer: &mut impl Write,
    value: &T,
) -> Result<(), ScribeError> {
    serde_json::to_writer(writer, value).map_err(|_| ScribeError::InvalidFrame)
}
/// Writes exponential buckets in domain DTO field order.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the destination rejects bytes
/// or a bucket field cannot be serialized.
fn write_buckets(
    writer: &mut impl Write,
    value: &wyrd_tonic::otlp::metrics::v1::exponential_histogram_data_point::Buckets,
) -> Result<(), ScribeError> {
    writer
        .write_all(b"{\"offset\":")
        .map_err(|_| ScribeError::InvalidFrame)?;
    write_json(writer, &value.offset)?;
    writer
        .write_all(b",\"bucket_counts\":")
        .map_err(|_| ScribeError::InvalidFrame)?;
    write_json(writer, &value.bucket_counts)?;
    writer
        .write_all(b"}")
        .map_err(|_| ScribeError::InvalidFrame)
}
/// Writes summary quantiles in domain DTO field order.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the destination rejects bytes
/// or a quantile/value field cannot be serialized.
fn write_quantiles(
    writer: &mut impl Write,
    values: &[wyrd_tonic::otlp::metrics::v1::summary_data_point::ValueAtQuantile],
) -> Result<(), ScribeError> {
    writer
        .write_all(b"[")
        .map_err(|_| ScribeError::InvalidFrame)?;
    for (i, v) in values.iter().enumerate() {
        if i != 0 {
            writer
                .write_all(b",")
                .map_err(|_| ScribeError::InvalidFrame)?;
        }
        writer
            .write_all(b"{\"quantile\":")
            .map_err(|_| ScribeError::InvalidFrame)?;
        write_json(writer, &v.quantile)?;
        writer
            .write_all(b",\"value\":")
            .map_err(|_| ScribeError::InvalidFrame)?;
        write_json(writer, &v.value)?;
        writer
            .write_all(b"}")
            .map_err(|_| ScribeError::InvalidFrame)?;
    }
    writer
        .write_all(b"]")
        .map_err(|_| ScribeError::InvalidFrame)
}
/// Writes exemplars without an intermediate DTO vector.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for out-of-range exemplar timestamps,
/// missing values, invalid identifiers/attributes, serialization failure, or
/// destination-write failure.
fn write_exemplars(writer: &mut impl Write, values: &[Exemplar]) -> Result<(), ScribeError> {
    writer
        .write_all(b"[")
        .map_err(|_| ScribeError::InvalidFrame)?;
    for (i, v) in values.iter().enumerate() {
        if i != 0 {
            writer
                .write_all(b",")
                .map_err(|_| ScribeError::InvalidFrame)?;
        }
        let time = chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(
            i64::try_from(v.time_unix_nano).map_err(|_| ScribeError::InvalidFrame)?,
        );
        writer
            .write_all(b"{\"time\":")
            .map_err(|_| ScribeError::InvalidFrame)?;
        write_json(writer, &time)?;
        let scalar = match v.value.as_ref() {
            Some(exemplar::Value::AsDouble(v)) => *v,
            Some(exemplar::Value::AsInt(v)) => metric_integer_as_f64(*v),
            None => 0.0,
        };
        writer
            .write_all(b",\"value\":")
            .map_err(|_| ScribeError::InvalidFrame)?;
        write_json(writer, &scalar)?;
        if !v.trace_id.is_empty() {
            let id: [u8; 16] = v
                .trace_id
                .as_slice()
                .try_into()
                .map_err(|_| ScribeError::InvalidFrame)?;
            writer
                .write_all(b",\"trace_id\":")
                .map_err(|_| ScribeError::InvalidFrame)?;
            write_json(
                writer,
                &TraceId::from_bytes(id).map_err(|_| ScribeError::InvalidFrame)?,
            )?;
        }
        if !v.span_id.is_empty() {
            let id: [u8; 8] = v
                .span_id
                .as_slice()
                .try_into()
                .map_err(|_| ScribeError::InvalidFrame)?;
            writer
                .write_all(b",\"span_id\":")
                .map_err(|_| ScribeError::InvalidFrame)?;
            write_json(
                writer,
                &SpanId::from_bytes(id).map_err(|_| ScribeError::InvalidFrame)?,
            )?;
        }
        writer
            .write_all(b",\"filtered_attributes\":")
            .map_err(|_| ScribeError::InvalidFrame)?;
        write_attributes(writer, &v.filtered_attributes)?;
        writer
            .write_all(b"}")
            .map_err(|_| ScribeError::InvalidFrame)?;
    }
    writer
        .write_all(b"]")
        .map_err(|_| ScribeError::InvalidFrame)
}
/// Converts one accepted timestamp to Arrow microseconds.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the microsecond value exceeds
/// the physical signed 64-bit range.
fn nanos(value: u64) -> Result<i64, ScribeError> {
    i64::try_from(value / 1_000).map_err(|_| ScribeError::InvalidFrame)
}
/// Normalizes an optional nonzero timestamp into Arrow microseconds.
fn optional_nanos(value: u64) -> Option<i64> {
    if value == 0 {
        return None;
    }
    i64::try_from(value).ok().map(|nanos| nanos / 1_000)
}

/// Preserves the legacy mapper's explicit integer-to-double metric semantics.
///
/// Large integers round to the nearest representable IEEE-754 value, matching
/// the existing physical metrics schema conversion.
fn metric_integer_as_f64(value: i64) -> f64 {
    num_traits::ToPrimitive::to_f64(&value)
        .expect("every i64 value has a finite representable f64 conversion")
}

/// Appends one borrowed non-null string into exact Arrow storage.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] on planned-capacity divergence.
fn append_exact_text(column: &mut ExactStringColumn, value: &str) -> Result<(), ScribeError> {
    column.append_with(|writer| {
        writer.extend_from_slice(value.as_bytes());
        Ok(())
    })
}

/// Appends one optional borrowed string into exact Arrow storage.
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
/// Saturates unsigned physical counts into Iceberg `Int64`.
fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
/// Checked capacity addition.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the capacity sum overflows.
fn add(left: usize, right: usize) -> Result<usize, ScribeError> {
    left.checked_add(right).ok_or(ScribeError::InvalidFrame)
}
/// Returns the directly materialized mapper prefix through nullable `run_id`.
///
/// The authoritative managed projection appends all remaining server-owned
/// columns without allocating the discarded mapper placeholders.
fn retained_write_schema() -> Arc<Schema> {
    let mut fields = PointsTable::arrow_fields();
    fields.push(Field::new(RUN_ID, arrow::datatypes::DataType::Utf8, true));
    Arc::new(Schema::new(fields))
}

/// Returns the existing metrics coordinator input schema.
fn write_schema() -> Arc<Schema> {
    let mut fields = PointsTable::arrow_fields();
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
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::principal::{PrincipalId, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value};
    use wyrd_tonic::otlp::metrics::v1::{
        AggregationTemporality, ExponentialHistogram, Gauge, Histogram, ResourceMetrics,
        ScopeMetrics, Sum, Summary, exponential_histogram_data_point, summary_data_point,
    };
    use wyrd_tonic::otlp::resource::v1::Resource;

    /// Projects one fixture with deterministic authoritative managed values.
    ///
    /// # Errors
    ///
    /// Returns the direct projector's validation, material-limit, or Arrow
    /// construction error.
    fn project_fixture(
        request: &ExportMetricsServiceRequest,
        material_limit: usize,
    ) -> Result<
        (
            Option<crate::scribe::otlp_managed::OtlpManagedBatch>,
            MetricsOutcome,
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
    fn exact_material_bytes(request: &ExportMetricsServiceRequest) -> Result<usize, ScribeError> {
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
        validate_material_plan(request, &source, usize::MAX, &managed)?.admitted_bytes(usize::MAX)
    }

    /// Constructs one string OTLP attribute fixture.
    fn kv(key: &str, value: &str) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue(value.to_owned())),
            }),
        }
    }

    /// Constructs one number point with an exemplar and attributes.
    fn number(value: f64) -> NumberDataPoint {
        NumberDataPoint {
            attributes: vec![kv("pod", "a")],
            start_time_unix_nano: 1_000_000_000,
            time_unix_nano: 1_002_000_000,
            exemplars: vec![Exemplar {
                filtered_attributes: vec![kv("sampler", "head")],
                time_unix_nano: 1_001_000_000,
                span_id: (1_u8..=8).collect(),
                trace_id: (1_u8..=16).collect(),
                value: Some(exemplar::Value::AsDouble(value - 1.0)),
            }],
            flags: 1,
            value: Some(number_data_point::Value::AsDouble(value)),
        }
    }

    /// Constructs a request spanning all five metric point variants.
    fn request() -> ExportMetricsServiceRequest {
        let gauge = Metric {
            name: "gauge".to_owned(),
            data: Some(metric::Data::Gauge(Gauge {
                data_points: vec![number(1.0)],
            })),
            ..Metric::default()
        };
        let sum = Metric {
            name: "sum".to_owned(),
            description: "requests".to_owned(),
            unit: "1".to_owned(),
            data: Some(metric::Data::Sum(Sum {
                data_points: vec![number(2.0)],
                aggregation_temporality: AggregationTemporality::Cumulative as i32,
                is_monotonic: true,
            })),
            ..Metric::default()
        };
        let histogram = Metric {
            name: "histogram".to_owned(),
            data: Some(metric::Data::Histogram(Histogram {
                data_points: vec![HistogramDataPoint {
                    time_unix_nano: 1_002_000_000,
                    count: 3,
                    sum: Some(30.0),
                    bucket_counts: vec![1, 2],
                    explicit_bounds: vec![10.0],
                    min: Some(1.0),
                    max: Some(20.0),
                    ..HistogramDataPoint::default()
                }],
                aggregation_temporality: AggregationTemporality::Delta as i32,
            })),
            ..Metric::default()
        };
        let exponential = Metric {
            name: "exponential".to_owned(),
            data: Some(metric::Data::ExponentialHistogram(ExponentialHistogram {
                data_points: vec![ExponentialHistogramDataPoint {
                    time_unix_nano: 1_002_000_000,
                    count: 5,
                    sum: Some(50.0),
                    scale: 2,
                    zero_count: 1,
                    zero_threshold: 0.1,
                    positive: Some(exponential_histogram_data_point::Buckets {
                        offset: -1,
                        bucket_counts: vec![1, 3],
                    }),
                    ..ExponentialHistogramDataPoint::default()
                }],
                aggregation_temporality: AggregationTemporality::Cumulative as i32,
            })),
            ..Metric::default()
        };
        let summary = Metric {
            name: "summary".to_owned(),
            data: Some(metric::Data::Summary(Summary {
                data_points: vec![SummaryDataPoint {
                    time_unix_nano: 1_002_000_000,
                    count: 4,
                    sum: 40.0,
                    quantile_values: vec![summary_data_point::ValueAtQuantile {
                        quantile: 0.99,
                        value: 12.0,
                    }],
                    ..SummaryDataPoint::default()
                }],
            })),
            ..Metric::default()
        };
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![kv("service.name", "checkout")],
                    ..Resource::default()
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "metrics-sdk".to_owned(),
                        version: "1.0".to_owned(),
                        ..InstrumentationScope::default()
                    }),
                    metrics: vec![gauge, sum, histogram, exponential, summary],
                    ..ScopeMetrics::default()
                }],
                ..ResourceMetrics::default()
            }],
        }
    }
    /// Empty typed requests project no Arrow batch and a full-success outcome.
    #[test]
    fn empty_request_projects_no_batch() {
        let (batch, outcome) =
            project_fixture(&ExportMetricsServiceRequest::default(), usize::MAX).expect("project");
        assert!(batch.is_none());
        assert_eq!(outcome.accepted_points, 0);
        assert_eq!(outcome.rejected_points, 0);
    }

    /// All metric variants match the established mapper column for column.
    #[test]
    fn all_metric_variants_match_mapper() {
        let request = request();
        let mapped = crate::gate::collector::map::map_resource_metrics(&request.resource_metrics);
        assert!(mapped.rejected.is_empty());
        let expected = crate::gate::collector::map::metrics_to_record_batch(&mapped.records)
            .expect("mapper batch");
        let (actual, outcome) = project_fixture(&request, usize::MAX).expect("direct projection");
        let actual = actual.expect("direct batch").rows;
        let retained = expected.num_columns() - 2;
        assert_eq!(
            &actual.columns()[..retained],
            &expected.columns()[..retained]
        );
        assert_eq!(outcome.accepted_points, 5);
        assert_eq!(outcome.rejected_points, 0);
    }

    /// Mixed metric exports preserve accepted rows and the first rejection summary.
    #[test]
    fn mixed_projection_matches_mapper_rejection() {
        let mut request = request();
        request.resource_metrics[0].scope_metrics[0]
            .metrics
            .push(Metric {
                name: "invalid".to_owned(),
                data: Some(metric::Data::Gauge(Gauge {
                    data_points: vec![NumberDataPoint::default()],
                })),
                ..Metric::default()
            });
        let mapped = crate::gate::collector::map::map_resource_metrics(&request.resource_metrics);
        let expected_reason = mapped.rejected.first().map(|value| value.reason.clone());
        let (actual, outcome) = project_fixture(&request, usize::MAX).expect("direct projection");
        assert_eq!(
            actual.expect("accepted rows").rows.num_rows(),
            mapped.records.len()
        );
        assert_eq!(outcome.accepted_points, mapped.records.len() as i64);
        assert_eq!(outcome.rejected_points, mapped.rejected.len() as i64);
        assert_eq!(outcome.rejection_message, expected_reason);
    }

    /// Exact metric Arrow-plus-IPC material passes at `C` and refuses `C - 1`.
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
