//! Pure OTLP → `SpanRecord` mapping and the `SpanRecord` → Arrow encoder.
//!
//! Two responsibilities, both IO-free and server-free:
//!
//! 1. Flatten an `ExportTraceServiceRequest`'s `ResourceSpans → ScopeSpans →
//!    Span` tree into flat [`SpanRecord`]s, carrying resource + scope identity
//!    onto every span. A single malformed span (invalid id, `end < start`) is
//!    reported as a rejection, not a whole-request failure.
//! 2. Encode a slice of [`SpanRecord`]s into an Arrow [`RecordBatch`] whose
//!    schema matches `SpansTable::arrow_fields()` plus the Observation-policy
//!    correlation columns (`run_id`, `card_uid`, `principal_id`), exactly what
//!    the group-commit coordinator expects as input before it stamps the
//!    system columns.
//!
//! `attributes` stays OPAQUE here: each span's attribute bag is serialized to a
//! JSON string in the `attributes` `Utf8` column. Redaction of that column
//! happens at coordinator flush (the table is `PayloadClass::Sensitive`), never
//! in this crate.

use std::sync::Arc;

use super::tables::{CorrelationPolicy, DomainTable, PointsTable, RecordsTable, SpansTable};
use arrow::array::{
    ArrayRef, BooleanArray, FixedSizeBinaryBuilder, Float64Array, Int32Array, Int64Array,
    RecordBatch, StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{Field, Schema, SchemaRef};
use chrono::{DateTime, Utc};
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::logs::record::LogRecord;
use wyrd_spec::vala::managed_columns::{CARD_UID, PRINCIPAL_ID, RUN_ID};
use wyrd_spec::vala::metrics::record::{
    AggregationTemporality, ExponentialBuckets, MetricExemplar, MetricRecord, MetricType,
    QuantileValue,
};
use wyrd_spec::vala::trace::{
    InstrumentationScope, Resource, SpanEvent, SpanKind, SpanLink, SpanRecord, SpanStatus,
};

use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::logs::v1::{LogRecord as OtlpLog, ResourceLogs, SeverityNumber};
use wyrd_tonic::otlp::metrics::v1::{
    AggregationTemporality as OtlpTemporality, Exemplar as OtlpExemplar, ExponentialHistogram,
    ExponentialHistogramDataPoint, Gauge, Histogram, HistogramDataPoint, Metric, NumberDataPoint,
    ResourceMetrics, Sum, Summary, SummaryDataPoint, exemplar, metric, number_data_point,
};
use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, Span as OtlpSpan, span, status::StatusCode};

/// `OTel` `service.*` resource semantic-convention attribute keys promoted to
/// typed [`Resource`] columns.
const SERVICE_NAME: &str = "service.name";
const SERVICE_NAMESPACE: &str = "service.namespace";
const SERVICE_VERSION: &str = "service.version";
const SERVICE_INSTANCE_ID: &str = "service.instance.id";

/// Fallback instrumentation-scope name for spans exported without a scope, or
/// with an empty scope name. OTLP makes the scope optional on the wire, but the
/// [`InstrumentationScope`] contract requires a non-empty name, so an unnamed
/// scope is normalized to this stable sentinel rather than dropped.
const UNKNOWN_SCOPE_NAME: &str = "unknown_service";

/// A span the receiver could not accept, with the reason it was dropped.
#[derive(Debug)]
pub struct RejectedSpan {
    /// Human-readable rejection reason (surfaced in OTLP `partial_success`).
    pub reason: String,
}

/// Outcome of flattening one export request: the accepted span records plus the
/// per-span rejections. A whole request only fails on a total-decode error;
/// individual bad spans land here.
#[derive(Debug, Default)]
pub struct MappedSpans {
    /// Spans that mapped and validated into a [`SpanRecord`].
    pub records: Vec<SpanRecord>,
    /// Spans dropped for a per-span reason (invalid id, `end < start`, ...).
    pub rejected: Vec<RejectedSpan>,
}

/// Flatten every `ResourceSpans → ScopeSpans → Span` in an export request into
/// [`SpanRecord`]s, carrying resource + scope identity onto each span.
///
/// Never returns `Err`: a malformed individual span is recorded in
/// [`MappedSpans::rejected`] so the caller can report OTLP `partial_success`.
#[must_use]
pub fn map_resource_spans(resource_spans: &[ResourceSpans]) -> MappedSpans {
    let mut out = MappedSpans::default();
    for rs in resource_spans {
        let resource = resource_from_otlp(rs.resource.as_ref());
        for ss in &rs.scope_spans {
            let scope = scope_from_otlp(ss.scope.as_ref());
            for span in &ss.spans {
                match map_span(span, &resource, &scope) {
                    Ok(record) => out.records.push(record),
                    Err(reason) => out.rejected.push(RejectedSpan { reason }),
                }
            }
        }
    }
    out
}

/// Map one OTLP [`OtlpSpan`] against its owning resource + scope into a
/// validated [`SpanRecord`].
///
/// # Errors
/// Returns the rejection reason string when the span carries an invalid
/// trace/span id, an out-of-range timestamp, or fails [`SpanRecord::validate`].
pub fn map_span(
    span: &OtlpSpan,
    resource: &Resource,
    scope: &InstrumentationScope,
) -> Result<SpanRecord, String> {
    let trace_id = trace_id_from_bytes(&span.trace_id)?;
    let span_id = span_id_from_bytes(&span.span_id)?;
    let parent_span_id = if span.parent_span_id.is_empty() {
        None
    } else {
        Some(span_id_from_bytes(&span.parent_span_id)?)
    };

    let start_time = ts_from_unix_nano(span.start_time_unix_nano)
        .ok_or_else(|| "span.start_time_unix_nano out of representable range".to_owned())?;
    let end_time = ts_from_unix_nano(span.end_time_unix_nano)
        .ok_or_else(|| "span.end_time_unix_nano out of representable range".to_owned())?;
    let duration_ms = duration_ms(start_time, end_time);

    let record = SpanRecord {
        trace_id,
        span_id,
        parent_span_id,
        flags: span.flags,
        trace_state: span.trace_state.clone(),
        name: span.name.clone(),
        kind: span_kind_from_i32(span.kind),
        start_time,
        end_time,
        duration_ms,
        status: span_status_from_otlp(span.status.as_ref()),
        attributes: attributes_to_map(&span.attributes),
        dropped_attributes_count: span.dropped_attributes_count,
        events: span.events.iter().map(map_event).collect(),
        dropped_events_count: span.dropped_events_count,
        links: span.links.iter().map(map_link).collect::<Result<_, _>>()?,
        dropped_links_count: span.dropped_links_count,
        scope: scope.clone(),
        resource: resource.clone(),
    };

    record.validate().map_err(|error| error.to_string())?;
    Ok(record)
}

fn map_event(event: &span::Event) -> SpanEvent {
    SpanEvent {
        timestamp: ts_from_unix_nano(event.time_unix_nano).unwrap_or_else(Utc::now),
        name: event.name.clone(),
        attributes: attributes_to_map(&event.attributes),
        dropped_attributes_count: event.dropped_attributes_count,
    }
}

fn map_link(link: &span::Link) -> Result<SpanLink, String> {
    Ok(SpanLink {
        trace_id: trace_id_from_bytes(&link.trace_id)?,
        span_id: span_id_from_bytes(&link.span_id)?,
        trace_state: link.trace_state.clone(),
        flags: link.flags,
        attributes: attributes_to_map(&link.attributes),
        dropped_attributes_count: link.dropped_attributes_count,
    })
}

fn trace_id_from_bytes(bytes: &[u8]) -> Result<TraceId, String> {
    let array: [u8; 16] = bytes
        .try_into()
        .map_err(|_| format!("trace_id must be 16 bytes, got {}", bytes.len()))?;
    TraceId::from_bytes(array).map_err(|error| error.to_string())
}

fn span_id_from_bytes(bytes: &[u8]) -> Result<SpanId, String> {
    let array: [u8; 8] = bytes
        .try_into()
        .map_err(|_| format!("span_id must be 8 bytes, got {}", bytes.len()))?;
    SpanId::from_bytes(array).map_err(|error| error.to_string())
}

/// Convert a UNIX-epoch nanosecond count to a UTC timestamp. `0` is the OTLP
/// "unset" sentinel and maps to the epoch.
fn ts_from_unix_nano(unix_nano: u64) -> Option<DateTime<Utc>> {
    let nanos = i64::try_from(unix_nano).ok()?;
    DateTime::from_timestamp_nanos(nanos).into()
}

/// Milliseconds between `start` and `end`, floored at 0. Mirrors the trace
/// module's `pub(crate)` `duration_ms_from_timestamps` (which is not reachable
/// from this crate) so `SpanRecord::validate`'s derived-duration check passes.
fn duration_ms(start: DateTime<Utc>, end: DateTime<Utc>) -> u64 {
    let delta = end.signed_duration_since(start).num_milliseconds();
    u64::try_from(delta).unwrap_or(0)
}

fn span_kind_from_i32(kind: i32) -> SpanKind {
    match span::SpanKind::try_from(kind).unwrap_or(span::SpanKind::Unspecified) {
        span::SpanKind::Server => SpanKind::Server,
        span::SpanKind::Client => SpanKind::Client,
        span::SpanKind::Producer => SpanKind::Producer,
        span::SpanKind::Consumer => SpanKind::Consumer,
        // Unspecified defaults to Internal per the OTLP spec.
        span::SpanKind::Unspecified | span::SpanKind::Internal => SpanKind::Internal,
    }
}

fn span_status_from_otlp(status: Option<&wyrd_tonic::otlp::trace::v1::Status>) -> SpanStatus {
    let Some(status) = status else {
        return SpanStatus::Unset;
    };
    match StatusCode::try_from(status.code).unwrap_or(StatusCode::Unset) {
        StatusCode::Ok => SpanStatus::Ok,
        StatusCode::Error => SpanStatus::Error {
            description: (!status.message.is_empty()).then(|| status.message.clone()),
        },
        StatusCode::Unset => SpanStatus::Unset,
    }
}

fn resource_from_otlp(resource: Option<&OtlpResource>) -> Resource {
    let mut attributes = resource
        .map(|r| attributes_to_map(&r.attributes))
        .unwrap_or_default();
    let service_name = take_string(&mut attributes, SERVICE_NAME).unwrap_or_default();
    Resource {
        service_name,
        service_namespace: take_string(&mut attributes, SERVICE_NAMESPACE),
        service_version: take_string(&mut attributes, SERVICE_VERSION),
        service_instance_id: take_string(&mut attributes, SERVICE_INSTANCE_ID),
        attributes,
    }
}

fn scope_from_otlp(
    scope: Option<&wyrd_tonic::otlp::common::v1::InstrumentationScope>,
) -> InstrumentationScope {
    match scope {
        Some(scope) if !scope.name.is_empty() => InstrumentationScope {
            name: scope.name.clone(),
            version: (!scope.version.is_empty()).then(|| scope.version.clone()),
            attributes: attributes_to_map(&scope.attributes),
        },
        // Absent scope, or scope with an empty name: OTLP allows both, but the
        // record contract requires a non-empty name, so normalize to the
        // unknown-scope sentinel and keep any version/attributes the exporter
        // did send.
        Some(scope) => InstrumentationScope {
            name: UNKNOWN_SCOPE_NAME.to_owned(),
            version: (!scope.version.is_empty()).then(|| scope.version.clone()),
            attributes: attributes_to_map(&scope.attributes),
        },
        None => InstrumentationScope {
            name: UNKNOWN_SCOPE_NAME.to_owned(),
            version: None,
            attributes: serde_json::Map::new(),
        },
    }
}

/// Pull the string value of a promoted `service.*` key out of the attribute bag,
/// leaving non-string or absent values untouched.
fn take_string(map: &mut serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(serde_json::Value::String(_)) => match map.remove(key) {
            Some(serde_json::Value::String(value)) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

/// Flatten a repeated OTLP `KeyValue` list into a flat JSON object, keeping the
/// bag OPAQUE (no per-key typing beyond OTLP's `AnyValue` shape).
fn attributes_to_map(pairs: &[KeyValue]) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::with_capacity(pairs.len());
    for pair in pairs {
        map.insert(pair.key.clone(), any_value_to_json(pair.value.as_ref()));
    }
    map
}

fn any_value_to_json(value: Option<&AnyValue>) -> serde_json::Value {
    let Some(value) = value.and_then(|v| v.value.as_ref()) else {
        return serde_json::Value::Null;
    };
    match value {
        any_value::Value::StringValue(s) => serde_json::Value::String(s.clone()),
        any_value::Value::BoolValue(b) => serde_json::Value::Bool(*b),
        any_value::Value::IntValue(i) => serde_json::Value::from(*i),
        any_value::Value::DoubleValue(d) => serde_json::Number::from_f64(*d)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        any_value::Value::ArrayValue(array) => serde_json::Value::Array(
            array
                .values
                .iter()
                .map(|element| any_value_to_json(Some(element)))
                .collect(),
        ),
        any_value::Value::KvlistValue(kv) => {
            serde_json::Value::Object(attributes_to_map(&kv.values))
        }
        any_value::Value::BytesValue(bytes) => {
            use base64::Engine;
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
    }
}

/// String form of a [`SpanKind`] written to the `kind` column. Uppercase OTLP
/// short form (no `SPAN_KIND_` prefix), matching the query read path.
fn kind_str(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::Internal => "INTERNAL",
        SpanKind::Server => "SERVER",
        SpanKind::Client => "CLIENT",
        SpanKind::Producer => "PRODUCER",
        SpanKind::Consumer => "CONSUMER",
    }
}

/// String form of a [`SpanStatus`] written to the `status` column. The read
/// path (`aggregate_spans_to_summaries`) keys on `"ERROR"`.
fn status_str(status: &SpanStatus) -> &'static str {
    match status {
        SpanStatus::Unset => "UNSET",
        SpanStatus::Ok => "OK",
        SpanStatus::Error { .. } => "ERROR",
    }
}

/// Full input schema for the spans coordinator: the declared user fields plus
/// the Observation-policy correlation columns. The coordinator appends the
/// system columns (`wyrd_event_time`, `wyrd_ingested_at`, `wyrd_batch_id`,
/// `data_tenant_id`); this schema must not include them.
fn spans_write_schema() -> SchemaRef {
    let mut fields = SpansTable::arrow_fields();
    debug_assert_eq!(
        SpansTable::CORRELATION_POLICY,
        CorrelationPolicy::Observation
    );
    fields.push(Field::new(RUN_ID, arrow::datatypes::DataType::Utf8, true));
    fields.push(Field::new(CARD_UID, arrow::datatypes::DataType::Utf8, true));
    fields.push(Field::new(
        PRINCIPAL_ID,
        arrow::datatypes::DataType::Utf8,
        true,
    ));
    SchemaRef::new(Schema::new(fields))
}

/// Encode a slice of [`SpanRecord`]s into one Arrow [`RecordBatch`] shaped for
/// the spans group-commit coordinator (user fields + correlation columns).
///
/// # Errors
/// Returns an Arrow error string when column assembly fails (mismatched lengths
/// or an invalid fixed-size-binary value).
pub fn spans_to_record_batch(records: &[SpanRecord]) -> Result<RecordBatch, String> {
    let n = records.len();

    let trace_id = fixed16(records.iter().map(|r| Some(*r.trace_id.as_bytes())))?;
    let span_id = fixed8(records.iter().map(|r| Some(*r.span_id.as_bytes())))?;
    let parent_span_id = fixed8(
        records
            .iter()
            .map(|r| r.parent_span_id.map(|id| *id.as_bytes())),
    )?;

    let flags = Arc::new(Int64Array::from_iter_values(
        records.iter().map(|r| i64::from(r.flags)),
    )) as ArrayRef;
    let trace_state = Arc::new(
        records
            .iter()
            .map(|r| Some(r.trace_state.clone()))
            .collect::<StringArray>(),
    ) as ArrayRef;
    let name = Arc::new(StringArray::from_iter_values(
        records.iter().map(|r| r.name.clone()),
    )) as ArrayRef;
    let kind = Arc::new(StringArray::from_iter_values(
        records.iter().map(|r| kind_str(r.kind)),
    )) as ArrayRef;

    let start_time = Arc::new(
        TimestampMicrosecondArray::from_iter_values(records.iter().map(|r| micros(r.start_time)))
            .with_timezone("UTC".to_string()),
    ) as ArrayRef;
    let end_time = Arc::new(
        TimestampMicrosecondArray::from_iter_values(records.iter().map(|r| micros(r.end_time)))
            .with_timezone("UTC".to_string()),
    ) as ArrayRef;

    // Iceberg has no unsigned integer type; `duration_ms` is a signed Int64
    // physical column (SpanRecord carries it as u64). A span duration never
    // exceeds i64::MAX in practice, so saturate on the impossible overflow.
    let duration_ms = Arc::new(Int64Array::from_iter_values(
        records
            .iter()
            .map(|r| i64::try_from(r.duration_ms).unwrap_or(i64::MAX)),
    )) as ArrayRef;
    let status = Arc::new(StringArray::from_iter_values(
        records.iter().map(|r| status_str(&r.status)),
    )) as ArrayRef;

    let attributes = Arc::new(
        records
            .iter()
            .map(|r| Some(attrs_json(&r.attributes)))
            .collect::<StringArray>(),
    ) as ArrayRef;

    let dropped_attributes_count = Arc::new(Int64Array::from_iter_values(
        records
            .iter()
            .map(|r| i64::from(r.dropped_attributes_count)),
    )) as ArrayRef;
    let dropped_events_count = Arc::new(Int64Array::from_iter_values(
        records.iter().map(|r| i64::from(r.dropped_events_count)),
    )) as ArrayRef;
    let dropped_links_count = Arc::new(Int64Array::from_iter_values(
        records.iter().map(|r| i64::from(r.dropped_links_count)),
    )) as ArrayRef;

    let scope_name = Arc::new(
        records
            .iter()
            .map(|r| Some(r.scope.name.clone()))
            .collect::<StringArray>(),
    ) as ArrayRef;
    let scope_version = Arc::new(
        records
            .iter()
            .map(|r| r.scope.version.clone())
            .collect::<StringArray>(),
    ) as ArrayRef;
    let service_name = Arc::new(StringArray::from_iter_values(
        records.iter().map(|r| r.resource.service_name.clone()),
    )) as ArrayRef;

    // Observation correlation columns: OTLP spans carry no run/card/principal
    // correlation, so all three are null.
    let null_utf8 = || Arc::new(StringArray::new_null(n)) as ArrayRef;

    let columns: Vec<ArrayRef> = vec![
        trace_id,
        span_id,
        parent_span_id,
        flags,
        trace_state,
        name,
        kind,
        start_time,
        end_time,
        duration_ms,
        status,
        attributes,
        dropped_attributes_count,
        dropped_events_count,
        dropped_links_count,
        scope_name,
        scope_version,
        service_name,
        null_utf8(),
        null_utf8(),
        null_utf8(),
    ];

    RecordBatch::try_new(spans_write_schema(), columns).map_err(|error| error.to_string())
}

fn micros(ts: DateTime<Utc>) -> i64 {
    ts.timestamp_micros()
}

fn attrs_json(attributes: &serde_json::Map<String, serde_json::Value>) -> String {
    serde_json::Value::Object(attributes.clone()).to_string()
}

fn fixed16(values: impl Iterator<Item = Option<[u8; 16]>>) -> Result<ArrayRef, String> {
    let mut builder = FixedSizeBinaryBuilder::new(16);
    for value in values {
        match value {
            Some(bytes) => builder.append_value(bytes).map_err(|e| e.to_string())?,
            None => builder.append_null(),
        }
    }
    Ok(Arc::new(builder.finish()) as ArrayRef)
}

fn fixed8(values: impl Iterator<Item = Option<[u8; 8]>>) -> Result<ArrayRef, String> {
    let mut builder = FixedSizeBinaryBuilder::new(8);
    for value in values {
        match value {
            Some(bytes) => builder.append_value(bytes).map_err(|e| e.to_string())?,
            None => builder.append_null(),
        }
    }
    Ok(Arc::new(builder.finish()) as ArrayRef)
}

// ─── metrics ───────────────────────────────────────────────────────────────────

/// A metric data point the receiver could not accept, with the reason it was
/// dropped.
#[derive(Debug)]
pub struct RejectedPoint {
    /// Human-readable rejection reason (surfaced in OTLP `partial_success`).
    pub reason: String,
}

/// Outcome of flattening one metrics export request: the accepted point records
/// plus the per-point rejections. A whole request only fails on a total-decode
/// error; individual bad points land here.
#[derive(Debug, Default)]
pub struct MappedMetrics {
    /// Points that mapped and validated into a [`MetricRecord`].
    pub records: Vec<MetricRecord>,
    /// Points dropped for a per-point reason (invalid value shape, bad exemplar
    /// id, out-of-range timestamp, ...).
    pub rejected: Vec<RejectedPoint>,
}

/// Flatten every `ResourceMetrics → ScopeMetrics → Metric → DataPoint` in an
/// export request into [`MetricRecord`]s, carrying resource + scope identity onto
/// each point.
///
/// Never returns `Err`: a malformed individual point is recorded in
/// [`MappedMetrics::rejected`] so the caller can report OTLP `partial_success`.
#[must_use]
pub fn map_resource_metrics(resource_metrics: &[ResourceMetrics]) -> MappedMetrics {
    let mut out = MappedMetrics::default();
    for rm in resource_metrics {
        let resource = resource_from_otlp(rm.resource.as_ref());
        for sm in &rm.scope_metrics {
            let scope = metric_scope_from_otlp(sm.scope.as_ref());
            for metric in &sm.metrics {
                map_metric(metric, &resource, scope.as_ref(), &mut out);
            }
        }
    }
    out
}

/// Instrumentation scope for a metric/log signal.
///
/// Unlike spans, [`MetricRecord`]/[`LogRecord`] carry an `Option<InstrumentationScope>`
/// (OTLP allows an absent scope and the record contract accepts `None`), so an
/// unset or unnamed scope maps to `None` rather than the unknown-scope sentinel.
fn metric_scope_from_otlp(
    scope: Option<&wyrd_tonic::otlp::common::v1::InstrumentationScope>,
) -> Option<InstrumentationScope> {
    let scope = scope?;
    if scope.name.is_empty() {
        return None;
    }
    Some(InstrumentationScope {
        name: scope.name.clone(),
        version: (!scope.version.is_empty()).then(|| scope.version.clone()),
        attributes: attributes_to_map(&scope.attributes),
    })
}

/// Expand one OTLP [`Metric`] (which fans out to many data points across its
/// `data` oneof) into validated [`MetricRecord`]s, pushing accepted records and
/// per-point rejections onto `out`.
fn map_metric(
    metric: &Metric,
    resource: &Resource,
    scope: Option<&InstrumentationScope>,
    out: &mut MappedMetrics,
) {
    let Some(data) = metric.data.as_ref() else {
        // A metric with no data oneof carries no points; nothing to map.
        return;
    };
    match data {
        metric::Data::Gauge(Gauge { data_points }) => {
            for point in data_points {
                push_number_point(metric, resource, scope, point, MetricType::Gauge, None, out);
            }
        }
        metric::Data::Sum(Sum {
            data_points,
            aggregation_temporality,
            is_monotonic,
        }) => {
            for point in data_points {
                push_number_point(
                    metric,
                    resource,
                    scope,
                    point,
                    MetricType::Sum,
                    Some((*aggregation_temporality, *is_monotonic)),
                    out,
                );
            }
        }
        metric::Data::Histogram(Histogram {
            data_points,
            aggregation_temporality,
        }) => {
            for point in data_points {
                push_histogram_point(
                    metric,
                    resource,
                    scope,
                    point,
                    *aggregation_temporality,
                    out,
                );
            }
        }
        metric::Data::ExponentialHistogram(ExponentialHistogram {
            data_points,
            aggregation_temporality,
        }) => {
            for point in data_points {
                push_exp_histogram_point(
                    metric,
                    resource,
                    scope,
                    point,
                    *aggregation_temporality,
                    out,
                );
            }
        }
        metric::Data::Summary(Summary { data_points }) => {
            for point in data_points {
                push_summary_point(metric, resource, scope, point, out);
            }
        }
    }
}

/// Shared metadata carried from the enclosing [`Metric`] onto every one of its
/// data-point records.
fn metric_base(
    metric: &Metric,
    resource: &Resource,
    scope: Option<&InstrumentationScope>,
    metric_type: MetricType,
) -> MetricRecord {
    MetricRecord {
        time: Utc::now(),
        start_time: None,
        metric_name: metric.name.clone(),
        description: (!metric.description.is_empty()).then(|| metric.description.clone()),
        unit: (!metric.unit.is_empty()).then(|| metric.unit.clone()),
        metric_type,
        temporality: None,
        is_monotonic: None,
        flags: None,
        value: None,
        count: None,
        sum: None,
        min: None,
        max: None,
        bucket_counts: None,
        explicit_bounds: None,
        scale: None,
        zero_count: None,
        zero_threshold: None,
        positive_buckets: None,
        negative_buckets: None,
        quantile_values: None,
        exemplars: Vec::new(),
        resource: resource.clone(),
        scope: scope.cloned(),
        attributes: serde_json::Map::new(),
    }
}

fn push_number_point(
    metric: &Metric,
    resource: &Resource,
    scope: Option<&InstrumentationScope>,
    point: &NumberDataPoint,
    metric_type: MetricType,
    sum_meta: Option<(i32, bool)>,
    out: &mut MappedMetrics,
) {
    let Some(time) = ts_from_unix_nano(point.time_unix_nano) else {
        out.rejected.push(RejectedPoint {
            reason: "metric time_unix_nano out of representable range".to_owned(),
        });
        return;
    };
    let value = match point.value.as_ref() {
        Some(number_data_point::Value::AsDouble(v)) => *v,
        Some(number_data_point::Value::AsInt(v)) => {
            serde_json::Number::from(*v).as_f64().unwrap_or(f64::NAN)
        }
        None => {
            out.rejected.push(RejectedPoint {
                reason: format!("metric {} number point carries no value", metric.name),
            });
            return;
        }
    };

    let mut record = metric_base(metric, resource, scope, metric_type);
    record.time = time;
    record.start_time = opt_ts(point.start_time_unix_nano);
    record.flags = opt_flags(point.flags);
    record.value = Some(value);
    record.attributes = attributes_to_map(&point.attributes);
    if let Some((temporality, is_monotonic)) = sum_meta {
        record.temporality = temporality_from_i32(temporality);
        record.is_monotonic = Some(is_monotonic);
    }
    finish_point(record, &point.exemplars, out);
}

fn push_histogram_point(
    metric: &Metric,
    resource: &Resource,
    scope: Option<&InstrumentationScope>,
    point: &HistogramDataPoint,
    temporality: i32,
    out: &mut MappedMetrics,
) {
    let Some(time) = ts_from_unix_nano(point.time_unix_nano) else {
        out.rejected.push(RejectedPoint {
            reason: "metric time_unix_nano out of representable range".to_owned(),
        });
        return;
    };
    let mut record = metric_base(metric, resource, scope, MetricType::Histogram);
    record.time = time;
    record.start_time = opt_ts(point.start_time_unix_nano);
    record.flags = opt_flags(point.flags);
    record.temporality = temporality_from_i32(temporality);
    record.count = Some(point.count);
    record.sum = point.sum;
    record.min = point.min;
    record.max = point.max;
    // OTLP omits both bucket arrays for a count/sum-only histogram; keep them
    // absent so validate()'s length rule is not applied to an empty pair.
    if !point.bucket_counts.is_empty() || !point.explicit_bounds.is_empty() {
        record.bucket_counts = Some(point.bucket_counts.clone());
        record.explicit_bounds = Some(point.explicit_bounds.clone());
    }
    record.attributes = attributes_to_map(&point.attributes);
    finish_point(record, &point.exemplars, out);
}

fn push_exp_histogram_point(
    metric: &Metric,
    resource: &Resource,
    scope: Option<&InstrumentationScope>,
    point: &ExponentialHistogramDataPoint,
    temporality: i32,
    out: &mut MappedMetrics,
) {
    let Some(time) = ts_from_unix_nano(point.time_unix_nano) else {
        out.rejected.push(RejectedPoint {
            reason: "metric time_unix_nano out of representable range".to_owned(),
        });
        return;
    };
    let mut record = metric_base(metric, resource, scope, MetricType::ExponentialHistogram);
    record.time = time;
    record.start_time = opt_ts(point.start_time_unix_nano);
    record.flags = opt_flags(point.flags);
    record.temporality = temporality_from_i32(temporality);
    record.count = Some(point.count);
    record.sum = point.sum;
    record.min = point.min;
    record.max = point.max;
    record.scale = Some(point.scale);
    record.zero_count = Some(point.zero_count);
    record.zero_threshold = Some(point.zero_threshold);
    record.positive_buckets = point.positive.as_ref().map(exp_buckets_from_otlp);
    record.negative_buckets = point.negative.as_ref().map(exp_buckets_from_otlp);
    record.attributes = attributes_to_map(&point.attributes);
    finish_point(record, &point.exemplars, out);
}

fn push_summary_point(
    metric: &Metric,
    resource: &Resource,
    scope: Option<&InstrumentationScope>,
    point: &SummaryDataPoint,
    out: &mut MappedMetrics,
) {
    let Some(time) = ts_from_unix_nano(point.time_unix_nano) else {
        out.rejected.push(RejectedPoint {
            reason: "metric time_unix_nano out of representable range".to_owned(),
        });
        return;
    };
    let mut record = metric_base(metric, resource, scope, MetricType::Summary);
    record.time = time;
    record.start_time = opt_ts(point.start_time_unix_nano);
    record.flags = opt_flags(point.flags);
    record.count = Some(point.count);
    record.sum = Some(point.sum);
    record.quantile_values = Some(
        point
            .quantile_values
            .iter()
            .map(|q| QuantileValue {
                quantile: q.quantile,
                value: q.value,
            })
            .collect(),
    );
    record.attributes = attributes_to_map(&point.attributes);
    // Summaries carry no exemplars in OTLP.
    finish_point(record, &[], out);
}

/// Attach the point's exemplars (per-item rejection on a bad exemplar id),
/// validate the assembled record, and route it to accepted or rejected.
fn finish_point(mut record: MetricRecord, exemplars: &[OtlpExemplar], out: &mut MappedMetrics) {
    match map_exemplars(exemplars) {
        Ok(exemplars) => record.exemplars = exemplars,
        Err(reason) => {
            out.rejected.push(RejectedPoint { reason });
            return;
        }
    }
    match record.validate() {
        Ok(()) => out.records.push(record),
        Err(error) => out.rejected.push(RejectedPoint {
            reason: error.to_string(),
        }),
    }
}

fn map_exemplars(exemplars: &[OtlpExemplar]) -> Result<Vec<MetricExemplar>, String> {
    exemplars.iter().map(map_exemplar).collect()
}

fn map_exemplar(exemplar: &OtlpExemplar) -> Result<MetricExemplar, String> {
    let time = ts_from_unix_nano(exemplar.time_unix_nano)
        .ok_or_else(|| "exemplar time_unix_nano out of representable range".to_owned())?;
    let value = match exemplar.value.as_ref() {
        Some(exemplar::Value::AsDouble(v)) => *v,
        Some(exemplar::Value::AsInt(v)) => {
            serde_json::Number::from(*v).as_f64().unwrap_or(f64::NAN)
        }
        None => 0.0,
    };
    let trace_id = if exemplar.trace_id.is_empty() {
        None
    } else {
        Some(trace_id_from_bytes(&exemplar.trace_id)?)
    };
    let span_id = if exemplar.span_id.is_empty() {
        None
    } else {
        Some(span_id_from_bytes(&exemplar.span_id)?)
    };
    Ok(MetricExemplar {
        time,
        value,
        trace_id,
        span_id,
        filtered_attributes: attributes_to_map(&exemplar.filtered_attributes),
    })
}

fn exp_buckets_from_otlp(
    buckets: &wyrd_tonic::otlp::metrics::v1::exponential_histogram_data_point::Buckets,
) -> ExponentialBuckets {
    ExponentialBuckets {
        offset: buckets.offset,
        bucket_counts: buckets.bucket_counts.clone(),
    }
}

/// `0` is the OTLP "unset" sentinel for `start_time_unix_nano`; map it to `None`.
fn opt_ts(unix_nano: u64) -> Option<DateTime<Utc>> {
    if unix_nano == 0 {
        None
    } else {
        ts_from_unix_nano(unix_nano)
    }
}

/// `0` is the "no flags" default; keep the column null in that case.
fn opt_flags(flags: u32) -> Option<u32> {
    (flags != 0).then_some(flags)
}

fn temporality_from_i32(temporality: i32) -> Option<AggregationTemporality> {
    match OtlpTemporality::try_from(temporality).unwrap_or(OtlpTemporality::Unspecified) {
        OtlpTemporality::Delta => Some(AggregationTemporality::Delta),
        OtlpTemporality::Cumulative => Some(AggregationTemporality::Cumulative),
        OtlpTemporality::Unspecified => None,
    }
}

/// Snake-case string form of a [`MetricType`] written to the `metric_type`
/// column, matching the `MetricType` serde representation and the query filter's
/// string equality on that column.
fn metric_type_str(metric_type: MetricType) -> &'static str {
    match metric_type {
        MetricType::Gauge => "gauge",
        MetricType::Sum => "sum",
        MetricType::Histogram => "histogram",
        MetricType::ExponentialHistogram => "exponential_histogram",
        MetricType::Summary => "summary",
    }
}

/// Snake-case string form of an [`AggregationTemporality`] for the `temporality`
/// column, matching its serde representation.
fn temporality_str(temporality: AggregationTemporality) -> &'static str {
    match temporality {
        AggregationTemporality::Delta => "delta",
        AggregationTemporality::Cumulative => "cumulative",
    }
}

/// Full input schema for the metrics coordinator: the declared user fields plus
/// the Observation-policy correlation columns. The coordinator appends the system
/// columns; this schema must not include them.
fn metrics_write_schema() -> SchemaRef {
    correlation_write_schema(PointsTable::arrow_fields())
}

/// Append the three Observation correlation columns to a table's user fields and
/// build the coordinator input schema.
fn correlation_write_schema(mut fields: Vec<Field>) -> SchemaRef {
    fields.push(Field::new(RUN_ID, arrow::datatypes::DataType::Utf8, true));
    fields.push(Field::new(CARD_UID, arrow::datatypes::DataType::Utf8, true));
    fields.push(Field::new(
        PRINCIPAL_ID,
        arrow::datatypes::DataType::Utf8,
        true,
    ));
    SchemaRef::new(Schema::new(fields))
}

/// Encode a slice of [`MetricRecord`]s into one Arrow [`RecordBatch`] shaped for
/// the metrics group-commit coordinator (user fields + correlation columns).
///
/// The bucket/quantile/exemplar/attribute columns carry JSON in their `Utf8`
/// columns, kept OPAQUE (redaction, where applicable, happens at coordinator
/// flush). Resource `service.name` and the optional scope name/version land in
/// the typed `service_name` / `scope_name` / `scope_version` columns.
///
/// # Errors
/// Returns an Arrow error string when column assembly fails.
pub fn metrics_to_record_batch(records: &[MetricRecord]) -> Result<RecordBatch, String> {
    debug_assert_eq!(
        PointsTable::CORRELATION_POLICY,
        CorrelationPolicy::Observation
    );
    let n = records.len();

    let mut columns = metric_scalar_columns(records);
    columns.extend(metric_opaque_columns(records));
    columns.extend([
        Arc::new(StringArray::new_null(n)) as ArrayRef,
        Arc::new(StringArray::new_null(n)) as ArrayRef,
        Arc::new(StringArray::new_null(n)) as ArrayRef,
    ]);

    RecordBatch::try_new(metrics_write_schema(), columns).map_err(|error| error.to_string())
}

fn metric_scalar_columns(records: &[MetricRecord]) -> Vec<ArrayRef> {
    let metric_name = Arc::new(StringArray::from_iter_values(
        records.iter().map(|r| r.metric_name.clone()),
    )) as ArrayRef;
    let time = ts_micros_col(records.iter().map(|r| Some(r.time)));
    let start_time = ts_micros_col(records.iter().map(|r| r.start_time));
    let description = Arc::new(
        records
            .iter()
            .map(|r| r.description.clone())
            .collect::<StringArray>(),
    ) as ArrayRef;
    let unit = Arc::new(
        records
            .iter()
            .map(|r| r.unit.clone())
            .collect::<StringArray>(),
    ) as ArrayRef;
    let metric_type = Arc::new(StringArray::from_iter_values(
        records.iter().map(|r| metric_type_str(r.metric_type)),
    )) as ArrayRef;
    let temporality = Arc::new(
        records
            .iter()
            .map(|r| r.temporality.map(temporality_str))
            .collect::<StringArray>(),
    ) as ArrayRef;
    let is_monotonic = Arc::new(
        records
            .iter()
            .map(|r| r.is_monotonic)
            .collect::<BooleanArray>(),
    ) as ArrayRef;
    let flags = Arc::new(
        records
            .iter()
            .map(|r| r.flags.map(i64::from))
            .collect::<Int64Array>(),
    ) as ArrayRef;
    let value = Arc::new(records.iter().map(|r| r.value).collect::<Float64Array>()) as ArrayRef;
    let count = Arc::new(
        records
            .iter()
            .map(|r| r.count.map(u64_to_i64))
            .collect::<Int64Array>(),
    ) as ArrayRef;
    let sum = Arc::new(records.iter().map(|r| r.sum).collect::<Float64Array>()) as ArrayRef;
    let min = Arc::new(records.iter().map(|r| r.min).collect::<Float64Array>()) as ArrayRef;
    let max = Arc::new(records.iter().map(|r| r.max).collect::<Float64Array>()) as ArrayRef;
    vec![
        metric_name,
        time,
        start_time,
        description,
        unit,
        metric_type,
        temporality,
        is_monotonic,
        flags,
        value,
        count,
        sum,
        min,
        max,
    ]
}

fn metric_opaque_columns(records: &[MetricRecord]) -> Vec<ArrayRef> {
    let bucket_counts = json_col(
        records
            .iter()
            .map(|r| r.bucket_counts.as_ref().map(json_of)),
    );
    let explicit_bounds = json_col(
        records
            .iter()
            .map(|r| r.explicit_bounds.as_ref().map(json_of)),
    );
    let scale = Arc::new(records.iter().map(|r| r.scale).collect::<Int32Array>()) as ArrayRef;
    let zero_count = Arc::new(
        records
            .iter()
            .map(|r| r.zero_count.map(u64_to_i64))
            .collect::<Int64Array>(),
    ) as ArrayRef;
    let zero_threshold = Arc::new(
        records
            .iter()
            .map(|r| r.zero_threshold)
            .collect::<Float64Array>(),
    ) as ArrayRef;
    let positive_buckets = json_col(
        records
            .iter()
            .map(|r| r.positive_buckets.as_ref().map(json_of)),
    );
    let negative_buckets = json_col(
        records
            .iter()
            .map(|r| r.negative_buckets.as_ref().map(json_of)),
    );
    let quantile_values = json_col(
        records
            .iter()
            .map(|r| r.quantile_values.as_ref().map(json_of)),
    );
    let exemplars = json_col(records.iter().map(|r| {
        if r.exemplars.is_empty() {
            None
        } else {
            Some(json_of(&r.exemplars))
        }
    }));
    let attributes = Arc::new(
        records
            .iter()
            .map(|r| Some(attrs_json(&r.attributes)))
            .collect::<StringArray>(),
    ) as ArrayRef;
    let service_name = Arc::new(StringArray::from_iter_values(
        records.iter().map(|r| r.resource.service_name.clone()),
    )) as ArrayRef;
    let scope_name = Arc::new(
        records
            .iter()
            .map(|r| r.scope.as_ref().map(|s| s.name.clone()))
            .collect::<StringArray>(),
    ) as ArrayRef;
    let scope_version = Arc::new(
        records
            .iter()
            .map(|r| r.scope.as_ref().and_then(|s| s.version.clone()))
            .collect::<StringArray>(),
    ) as ArrayRef;

    vec![
        bucket_counts,
        explicit_bounds,
        scale,
        zero_count,
        zero_threshold,
        positive_buckets,
        negative_buckets,
        quantile_values,
        exemplars,
        attributes,
        service_name,
        scope_name,
        scope_version,
    ]
}

// ─── logs ────────────────────────────────────────────────────────────────────

/// A log record the receiver could not accept, with the reason it was dropped.
#[derive(Debug)]
pub struct RejectedRecord {
    /// Human-readable rejection reason (surfaced in OTLP `partial_success`).
    pub reason: String,
}

/// Outcome of flattening one logs export request: the accepted log records plus
/// the per-record rejections.
#[derive(Debug, Default)]
pub struct MappedLogs {
    /// Log records that mapped and validated into a [`LogRecord`].
    pub records: Vec<LogRecord>,
    /// Records dropped for a per-record reason.
    pub rejected: Vec<RejectedRecord>,
}

/// Flatten every `ResourceLogs → ScopeLogs → LogRecord` in an export request into
/// [`LogRecord`]s, carrying resource + scope identity onto each record.
///
/// Never returns `Err`: a malformed individual record is recorded in
/// [`MappedLogs::rejected`] so the caller can report OTLP `partial_success`.
#[must_use]
pub fn map_resource_logs(resource_logs: &[ResourceLogs]) -> MappedLogs {
    let mut out = MappedLogs::default();
    for rl in resource_logs {
        let resource = resource_from_otlp(rl.resource.as_ref());
        for sl in &rl.scope_logs {
            let scope = metric_scope_from_otlp(sl.scope.as_ref());
            for record in &sl.log_records {
                match map_log(record, &resource, scope.as_ref()) {
                    Ok(record) => out.records.push(record),
                    Err(reason) => out.rejected.push(RejectedRecord { reason }),
                }
            }
        }
    }
    out
}

/// Map one OTLP [`OtlpLog`] against its owning resource + scope into a validated
/// [`LogRecord`].
///
/// # Errors
/// Returns the rejection reason string when the record carries an invalid
/// trace/span id, an out-of-range timestamp, or fails [`LogRecord::validate`].
fn map_log(
    log: &OtlpLog,
    resource: &Resource,
    scope: Option<&InstrumentationScope>,
) -> Result<LogRecord, String> {
    // observed_time is required by the record; OTLP allows it unset (0), in which
    // case fall back to time_unix_nano, then to the epoch.
    let observed_time = ts_from_unix_nano(log.observed_time_unix_nano)
        .or_else(|| ts_from_unix_nano(log.time_unix_nano))
        .ok_or_else(|| "log observed_time_unix_nano out of representable range".to_owned())?;
    let time = opt_ts(log.time_unix_nano);

    let trace_id = if log.trace_id.is_empty() {
        None
    } else {
        Some(trace_id_from_bytes(&log.trace_id)?)
    };
    let span_id = if log.span_id.is_empty() {
        None
    } else {
        Some(span_id_from_bytes(&log.span_id)?)
    };

    let record = LogRecord {
        time,
        observed_time,
        severity_number: severity_number_from_i32(log.severity_number),
        severity_text: (!log.severity_text.is_empty()).then(|| log.severity_text.clone()),
        // OTLP logs.v1 in this proto version carries no top-level `event_name`
        // field; the record leaves it unset and it stays opaque otherwise.
        event_name: None,
        body: log.body.as_ref().map(|body| any_value_to_json(Some(body))),
        trace_id,
        span_id,
        // OTLP packs the W3C trace flags into the low 8 bits of `flags`.
        trace_flags: log_trace_flags(log.flags),
        resource: Some(resource.clone()),
        scope: scope.cloned(),
        attributes: attributes_to_map(&log.attributes),
        dropped_attributes_count: log.dropped_attributes_count,
    };

    record.validate().map_err(|error| error.to_string())?;
    Ok(record)
}

/// `0`/`UNSPECIFIED` maps to `None`; any other value is the `OTel` `SeverityNumber`
/// (1..=24), narrowed to the record's `u8`.
fn severity_number_from_i32(severity: i32) -> Option<u8> {
    match SeverityNumber::try_from(severity).unwrap_or(SeverityNumber::Unspecified) {
        SeverityNumber::Unspecified => None,
        other => u8::try_from(other as i32).ok(),
    }
}

/// Extract the W3C trace-flags byte from the OTLP `flags` bit-field (low 8 bits).
/// `0` is the "no flags" default and maps to `None`.
fn log_trace_flags(flags: u32) -> Option<u8> {
    let byte = (flags & 0xff) as u8;
    (byte != 0).then_some(byte)
}

/// Full input schema for the logs coordinator: the declared user fields plus the
/// Observation-policy correlation columns.
fn logs_write_schema() -> SchemaRef {
    correlation_write_schema(RecordsTable::arrow_fields())
}

/// Encode a slice of [`LogRecord`]s into one Arrow [`RecordBatch`] shaped for the
/// logs group-commit coordinator (user fields + correlation columns).
///
/// `body` and `attributes` stay OPAQUE (`PayloadClass::Sensitive` — redaction at
/// coordinator flush). Resource `service.name` and the optional scope
/// name/version land in the typed columns.
///
/// # Errors
/// Returns an Arrow error string when column assembly fails.
pub fn logs_to_record_batch(records: &[LogRecord]) -> Result<RecordBatch, String> {
    debug_assert_eq!(
        RecordsTable::CORRELATION_POLICY,
        CorrelationPolicy::Observation
    );
    let n = records.len();

    let time = ts_micros_col(records.iter().map(|r| r.time));
    let observed_time = ts_micros_col(records.iter().map(|r| Some(r.observed_time)));
    let severity_number = Arc::new(
        records
            .iter()
            .map(|r| r.severity_number.map(|value| i64::from(u32::from(value))))
            .collect::<Int64Array>(),
    ) as ArrayRef;
    let severity_text = Arc::new(
        records
            .iter()
            .map(|r| r.severity_text.clone())
            .collect::<StringArray>(),
    ) as ArrayRef;
    let event_name = Arc::new(
        records
            .iter()
            .map(|r| r.event_name.clone())
            .collect::<StringArray>(),
    ) as ArrayRef;
    let body = json_col(records.iter().map(|r| r.body.as_ref().map(json_of)));
    let trace_id = fixed16(records.iter().map(|r| r.trace_id.map(|id| *id.as_bytes())))?;
    let span_id = fixed8(records.iter().map(|r| r.span_id.map(|id| *id.as_bytes())))?;
    let trace_flags = Arc::new(
        records
            .iter()
            .map(|r| r.trace_flags.map(|value| i64::from(u32::from(value))))
            .collect::<Int64Array>(),
    ) as ArrayRef;
    let attributes = json_col(records.iter().map(|r| {
        if r.attributes.is_empty() {
            None
        } else {
            Some(attrs_json(&r.attributes))
        }
    }));
    let dropped_attributes_count = Arc::new(Int64Array::from_iter_values(
        records
            .iter()
            .map(|r| i64::from(r.dropped_attributes_count)),
    )) as ArrayRef;
    let service_name = Arc::new(
        records
            .iter()
            .map(|r| r.resource.as_ref().map(|res| res.service_name.clone()))
            .collect::<StringArray>(),
    ) as ArrayRef;
    let scope_name = Arc::new(
        records
            .iter()
            .map(|r| r.scope.as_ref().map(|s| s.name.clone()))
            .collect::<StringArray>(),
    ) as ArrayRef;
    let scope_version = Arc::new(
        records
            .iter()
            .map(|r| r.scope.as_ref().and_then(|s| s.version.clone()))
            .collect::<StringArray>(),
    ) as ArrayRef;

    let null_utf8 = || Arc::new(StringArray::new_null(n)) as ArrayRef;

    let columns: Vec<ArrayRef> = vec![
        time,
        observed_time,
        severity_number,
        severity_text,
        event_name,
        body,
        trace_id,
        span_id,
        trace_flags,
        attributes,
        dropped_attributes_count,
        service_name,
        scope_name,
        scope_version,
        null_utf8(),
        null_utf8(),
        null_utf8(),
    ];

    RecordBatch::try_new(logs_write_schema(), columns).map_err(|error| error.to_string())
}

// ─── shared metric/log column helpers ──────────────────────────────────────────

/// Build a UTC `TimestampMicrosecond` column from an iterator of optional
/// timestamps (null where `None`).
fn ts_micros_col(values: impl Iterator<Item = Option<DateTime<Utc>>>) -> ArrayRef {
    Arc::new(
        values
            .map(|v| v.map(micros))
            .collect::<TimestampMicrosecondArray>()
            .with_timezone("UTC".to_string()),
    ) as ArrayRef
}

/// Build a `Utf8` column from an iterator of optional JSON strings.
fn json_col(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(values.collect::<StringArray>()) as ArrayRef
}

/// Serialize a value to its compact JSON string form for an opaque `Utf8`
/// column.
fn json_of<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

/// Iceberg has no unsigned integer type; counts are physical `Int64` columns.
/// A count never exceeds `i64::MAX` in practice, so saturate on the impossible
/// overflow.
fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Array, FixedSizeBinaryArray, StringArray};
    use wyrd_tonic::otlp::common::v1::{
        AnyValue, InstrumentationScope as OtlpScope, KeyValue, any_value,
    };
    use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
    use wyrd_tonic::otlp::trace::v1::{
        ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
    };

    fn kv(key: &str, value: any_value::Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    fn sample_span() -> OtlpSpan {
        OtlpSpan {
            trace_id: (1u8..=16).collect(),
            span_id: (1u8..=8).collect(),
            parent_span_id: (9u8..=16).collect(),
            trace_state: "vendor=1".to_owned(),
            flags: 1,
            name: "GET /widgets".to_owned(),
            kind: span::SpanKind::Server as i32,
            // 1_000_000_000 ns = 1s epoch; +2ms end.
            start_time_unix_nano: 1_000_000_000,
            end_time_unix_nano: 1_002_000_000,
            attributes: vec![
                kv(
                    "http.method",
                    any_value::Value::StringValue("GET".to_owned()),
                ),
                kv("http.status_code", any_value::Value::IntValue(200)),
            ],
            dropped_attributes_count: 3,
            events: vec![span::Event {
                time_unix_nano: 1_001_000_000,
                name: "exception".to_owned(),
                attributes: vec![kv(
                    "exception.type",
                    any_value::Value::StringValue("Boom".to_owned()),
                )],
                dropped_attributes_count: 0,
            }],
            dropped_events_count: 1,
            links: vec![span::Link {
                trace_id: (17u8..=32).collect(),
                span_id: (17u8..=24).collect(),
                trace_state: String::new(),
                flags: 0,
                attributes: vec![],
                dropped_attributes_count: 0,
            }],
            dropped_links_count: 2,
            status: Some(OtlpStatus {
                message: "kaboom".to_owned(),
                code: StatusCode::Error as i32,
            }),
        }
    }

    fn sample_resource() -> OtlpResource {
        OtlpResource {
            attributes: vec![
                kv(
                    "service.name",
                    any_value::Value::StringValue("checkout".to_owned()),
                ),
                kv(
                    "service.version",
                    any_value::Value::StringValue("2.1.0".to_owned()),
                ),
                kv(
                    "host.name",
                    any_value::Value::StringValue("node-a".to_owned()),
                ),
            ],
            dropped_attributes_count: 0,
            entity_refs: Vec::new(),
        }
    }

    fn sample_scope() -> OtlpScope {
        OtlpScope {
            name: "wyrd-tracing".to_owned(),
            version: "0.1.0".to_owned(),
            attributes: vec![],
            dropped_attributes_count: 0,
        }
    }

    #[test]
    fn otlp_span_record_parity() {
        let resource = resource_from_otlp(Some(&sample_resource()));
        let scope = scope_from_otlp(Some(&sample_scope()));
        let record = map_span(&sample_span(), &resource, &scope).expect("span maps");

        // Ids carried through byte-for-byte.
        assert_eq!(
            record.trace_id.as_bytes(),
            &(1u8..=16).collect::<Vec<_>>()[..]
        );
        assert_eq!(
            record.span_id.as_bytes(),
            &(1u8..=8).collect::<Vec<_>>()[..]
        );
        assert_eq!(
            record.parent_span_id.expect("parent present").as_bytes(),
            &(9u8..=16).collect::<Vec<_>>()[..]
        );

        // Scalar fields.
        assert_eq!(record.name, "GET /widgets");
        assert_eq!(record.kind, SpanKind::Server);
        assert_eq!(record.flags, 1);
        assert_eq!(record.trace_state, "vendor=1");
        assert_eq!(record.duration_ms, 2);
        assert_eq!(record.dropped_attributes_count, 3);
        assert_eq!(record.dropped_events_count, 1);
        assert_eq!(record.dropped_links_count, 2);

        // Status Error carries the message as description.
        match &record.status {
            SpanStatus::Error { description } => {
                assert_eq!(description.as_deref(), Some("kaboom"));
            }
            other => panic!("expected error status, got {other:?}"),
        }

        // Attributes kept opaque as a flat JSON bag.
        assert_eq!(
            record.attributes.get("http.method"),
            Some(&serde_json::Value::String("GET".to_owned()))
        );
        assert_eq!(
            record.attributes.get("http.status_code"),
            Some(&serde_json::Value::from(200i64))
        );

        // Resource: service.* promoted to typed columns, rest stays in the bag.
        assert_eq!(record.resource.service_name, "checkout");
        assert_eq!(record.resource.service_version.as_deref(), Some("2.1.0"));
        assert!(record.resource.attributes.contains_key("host.name"));
        assert!(!record.resource.attributes.contains_key("service.name"));

        // Scope carried onto the span.
        assert_eq!(record.scope.name, "wyrd-tracing");
        assert_eq!(record.scope.version.as_deref(), Some("0.1.0"));

        // Events / links flattened with parity.
        assert_eq!(record.events.len(), 1);
        assert_eq!(record.events[0].name, "exception");
        assert_eq!(record.links.len(), 1);
        assert_eq!(
            record.links[0].trace_id.as_bytes(),
            &(17u8..=32).collect::<Vec<_>>()[..]
        );
    }

    #[test]
    fn missing_scope_maps_to_unknown_scope_name() {
        // OTLP allows an absent scope; the record contract requires a non-empty
        // name, so an unnamed scope must normalize to the sentinel and still
        // validate (rather than being dropped as a rejection).
        let resource = resource_from_otlp(Some(&sample_resource()));
        let scope = scope_from_otlp(None);
        assert_eq!(scope.name, UNKNOWN_SCOPE_NAME);

        let record = map_span(&sample_span(), &resource, &scope).expect("span maps");
        assert_eq!(record.scope.name, UNKNOWN_SCOPE_NAME);

        let request = vec![ResourceSpans {
            resource: Some(sample_resource()),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![sample_span()],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];
        let mapped = map_resource_spans(&request);
        assert_eq!(
            mapped.records.len(),
            1,
            "unnamed-scope span must be accepted"
        );
        assert!(mapped.rejected.is_empty());
    }

    #[test]
    fn map_resource_spans_reports_bad_span_as_rejection() {
        let mut bad = sample_span();
        bad.trace_id = vec![0u8; 16]; // all-zero sentinel → rejected, not fatal.
        let good = sample_span();

        let request = vec![ResourceSpans {
            resource: Some(sample_resource()),
            scope_spans: vec![ScopeSpans {
                scope: Some(sample_scope()),
                spans: vec![good, bad],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];

        let mapped = map_resource_spans(&request);
        assert_eq!(mapped.records.len(), 1);
        assert_eq!(mapped.rejected.len(), 1);
    }

    #[test]
    fn spans_to_record_batch_matches_write_schema() {
        let resource = resource_from_otlp(Some(&sample_resource()));
        let scope = scope_from_otlp(Some(&sample_scope()));
        let record = map_span(&sample_span(), &resource, &scope).expect("span maps");
        let batch = spans_to_record_batch(std::slice::from_ref(&record)).expect("encodes");

        assert_eq!(batch.num_rows(), 1);
        // Column presence + order matches the coordinator's expected input:
        // user fields then the three Observation correlation columns.
        let schema = batch.schema();
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(names.first(), Some(&"trace_id"));
        assert_eq!(names.last(), Some(&"principal_id"));
        assert!(names.contains(&"run_id"));
        assert!(names.contains(&"card_uid"));

        let trace_id = batch
            .column_by_name("trace_id")
            .unwrap()
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .unwrap();
        assert_eq!(trace_id.value(0), &(1u8..=16).collect::<Vec<_>>()[..]);

        let kind = batch
            .column_by_name("kind")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(kind.value(0), "SERVER");

        let status = batch
            .column_by_name("status")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(status.value(0), "ERROR");

        // attributes is an opaque JSON string in the storage-compatible Utf8 column.
        let attrs = batch
            .column_by_name("attributes")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(attrs.value(0)).expect("json");
        assert_eq!(
            parsed.get("http.method").and_then(|v| v.as_str()),
            Some("GET")
        );

        // correlation columns are null for OTLP spans.
        let run_id = batch
            .column_by_name("run_id")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(run_id.is_null(0));
    }

    // ─── metrics ────────────────────────────────────────────────────────────

    use arrow::array::{BooleanArray, Float64Array, Int32Array, Int64Array};
    use wyrd_tonic::otlp::metrics::v1::{
        Exemplar as OtlpExemplar, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge,
        Histogram, HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum,
        Summary, SummaryDataPoint, exemplar, exponential_histogram_data_point, metric,
        number_data_point, summary_data_point,
    };

    fn number_point(value: f64) -> NumberDataPoint {
        NumberDataPoint {
            attributes: vec![kv(
                "k8s.pod",
                any_value::Value::StringValue("p-1".to_owned()),
            )],
            start_time_unix_nano: 1_000_000_000,
            time_unix_nano: 1_002_000_000,
            exemplars: vec![OtlpExemplar {
                filtered_attributes: vec![kv(
                    "sampler",
                    any_value::Value::StringValue("head".to_owned()),
                )],
                time_unix_nano: 1_001_000_000,
                span_id: (1u8..=8).collect(),
                trace_id: (1u8..=16).collect(),
                value: Some(exemplar::Value::AsDouble(41.0)),
            }],
            flags: 1,
            value: Some(number_data_point::Value::AsDouble(value)),
        }
    }

    fn sum_metric() -> Metric {
        Metric {
            name: "http.server.requests".to_owned(),
            description: "count of requests".to_owned(),
            unit: "{request}".to_owned(),
            metadata: vec![],
            data: Some(metric::Data::Sum(Sum {
                data_points: vec![number_point(42.0)],
                aggregation_temporality: OtlpTemporality::Cumulative as i32,
                is_monotonic: true,
            })),
        }
    }

    #[test]
    fn otlp_metric_record_parity() {
        // Sum → MetricRecord field-for-field, exemplar carried through.
        let request = vec![ResourceMetrics {
            resource: Some(sample_resource()),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(sample_scope()),
                metrics: vec![sum_metric()],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];

        let mapped = map_resource_metrics(&request);
        assert!(
            mapped.rejected.is_empty(),
            "no rejections: {:?}",
            mapped.rejected
        );
        assert_eq!(mapped.records.len(), 1);
        let record = &mapped.records[0];

        assert_eq!(record.metric_name, "http.server.requests");
        assert_eq!(record.metric_type, MetricType::Sum);
        assert_eq!(record.description.as_deref(), Some("count of requests"));
        assert_eq!(record.unit.as_deref(), Some("{request}"));
        assert_eq!(record.value, Some(42.0));
        assert_eq!(record.temporality, Some(AggregationTemporality::Cumulative));
        assert_eq!(record.is_monotonic, Some(true));
        assert_eq!(record.flags, Some(1));
        assert!(record.start_time.is_some());

        // Resource + scope carried onto the point.
        assert_eq!(record.resource.service_name, "checkout");
        assert_eq!(record.scope.as_ref().expect("scope").name, "wyrd-tracing");

        // Data-point attributes opaque.
        assert_eq!(
            record.attributes.get("k8s.pod"),
            Some(&serde_json::Value::String("p-1".to_owned()))
        );

        // Exemplar carried through, span/trace ids preserved.
        assert_eq!(record.exemplars.len(), 1);
        let ex = &record.exemplars[0];
        assert!((ex.value - 41.0).abs() < f64::EPSILON);
        assert_eq!(
            ex.trace_id.expect("trace id").as_bytes(),
            &(1u8..=16).collect::<Vec<_>>()[..]
        );
        assert_eq!(
            ex.span_id.expect("span id").as_bytes(),
            &(1u8..=8).collect::<Vec<_>>()[..]
        );
    }

    #[test]
    fn otlp_metric_record_batch_matches_write_schema() {
        let request = vec![ResourceMetrics {
            resource: Some(sample_resource()),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(sample_scope()),
                metrics: vec![sum_metric()],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];
        let mapped = map_resource_metrics(&request);
        let batch = metrics_to_record_batch(&mapped.records).expect("encodes");

        assert_eq!(batch.num_rows(), 1);
        let schema = batch.schema();
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(names.first(), Some(&"metric_name"));
        assert_eq!(names.last(), Some(&"principal_id"));
        assert!(names.contains(&"run_id"));

        let mtype = batch
            .column_by_name("metric_type")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(mtype.value(0), "sum");

        let temporality = batch
            .column_by_name("temporality")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(temporality.value(0), "cumulative");

        let value = batch
            .column_by_name("value")
            .unwrap()
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        assert!((value.value(0) - 42.0).abs() < f64::EPSILON);

        let is_monotonic = batch
            .column_by_name("is_monotonic")
            .unwrap()
            .as_any()
            .downcast_ref::<BooleanArray>()
            .unwrap();
        assert!(is_monotonic.value(0));

        // Exemplars land as a JSON array in the exemplars Utf8 column.
        let exemplars = batch
            .column_by_name("exemplars")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(exemplars.value(0)).expect("json");
        assert!(parsed.as_array().map(std::vec::Vec::len) == Some(1));

        let service_name = batch
            .column_by_name("service_name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(service_name.value(0), "checkout");
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "covers all OTLP aggregate encodings in one projection test"
    )]
    fn histogram_and_exp_histogram_and_summary_map_and_encode() {
        // One of each aggregating type, to exercise every points column.
        let histogram = Metric {
            name: "latency".to_owned(),
            description: String::new(),
            unit: "ms".to_owned(),
            metadata: vec![],
            data: Some(metric::Data::Histogram(Histogram {
                data_points: vec![HistogramDataPoint {
                    attributes: vec![],
                    start_time_unix_nano: 1_000_000_000,
                    time_unix_nano: 1_002_000_000,
                    count: 3,
                    sum: Some(30.0),
                    bucket_counts: vec![1, 2],
                    explicit_bounds: vec![10.0],
                    exemplars: vec![],
                    flags: 0,
                    min: Some(1.0),
                    max: Some(20.0),
                }],
                aggregation_temporality: OtlpTemporality::Delta as i32,
            })),
        };
        let exp = Metric {
            name: "exp.latency".to_owned(),
            description: String::new(),
            unit: String::new(),
            metadata: vec![],
            data: Some(metric::Data::ExponentialHistogram(ExponentialHistogram {
                data_points: vec![ExponentialHistogramDataPoint {
                    attributes: vec![],
                    start_time_unix_nano: 1_000_000_000,
                    time_unix_nano: 1_002_000_000,
                    count: 5,
                    sum: Some(50.0),
                    scale: 2,
                    zero_count: 1,
                    positive: Some(exponential_histogram_data_point::Buckets {
                        offset: 0,
                        bucket_counts: vec![1, 3],
                    }),
                    negative: None,
                    flags: 0,
                    exemplars: vec![],
                    min: Some(0.5),
                    max: Some(9.0),
                    zero_threshold: 0.1,
                }],
                aggregation_temporality: OtlpTemporality::Cumulative as i32,
            })),
        };
        let summary = Metric {
            name: "summary.latency".to_owned(),
            description: String::new(),
            unit: String::new(),
            metadata: vec![],
            data: Some(metric::Data::Summary(Summary {
                data_points: vec![SummaryDataPoint {
                    attributes: vec![],
                    start_time_unix_nano: 1_000_000_000,
                    time_unix_nano: 1_002_000_000,
                    count: 4,
                    sum: 40.0,
                    quantile_values: vec![summary_data_point::ValueAtQuantile {
                        quantile: 0.99,
                        value: 12.0,
                    }],
                    flags: 0,
                }],
            })),
        };

        let request = vec![ResourceMetrics {
            resource: Some(sample_resource()),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![histogram, exp, summary],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];
        let mapped = map_resource_metrics(&request);
        assert!(
            mapped.rejected.is_empty(),
            "no rejections: {:?}",
            mapped.rejected
        );
        assert_eq!(mapped.records.len(), 3);
        // Scope absent → None (not the unknown-scope sentinel).
        assert!(mapped.records[0].scope.is_none());

        let batch = metrics_to_record_batch(&mapped.records).expect("encodes");
        assert_eq!(batch.num_rows(), 3);

        let count = batch
            .column_by_name("count")
            .unwrap()
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(count.value(0), 3);

        let scale = batch
            .column_by_name("scale")
            .unwrap()
            .as_any()
            .downcast_ref::<Int32Array>()
            .unwrap();
        assert!(scale.is_null(0), "histogram row has no scale");
        assert_eq!(scale.value(1), 2, "exp-histogram scale carried");

        let quantiles = batch
            .column_by_name("quantile_values")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(quantiles.is_null(0));
        assert!(!quantiles.is_null(2), "summary carries quantile_values");
    }

    #[test]
    fn number_point_without_value_is_rejected() {
        let mut point = number_point(1.0);
        point.value = None;
        let metric = Metric {
            name: "g".to_owned(),
            description: String::new(),
            unit: String::new(),
            metadata: vec![],
            data: Some(metric::Data::Gauge(Gauge {
                data_points: vec![point, number_point(7.0)],
            })),
        };
        let request = vec![ResourceMetrics {
            resource: Some(sample_resource()),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![metric],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];
        let mapped = map_resource_metrics(&request);
        assert_eq!(mapped.records.len(), 1, "the valued gauge point survives");
        assert_eq!(mapped.rejected.len(), 1, "the value-less point is rejected");
    }

    // ─── logs ───────────────────────────────────────────────────────────────

    use wyrd_tonic::otlp::logs::v1::{
        LogRecord as OtlpLog, ResourceLogs, ScopeLogs, SeverityNumber,
    };

    fn sample_log() -> OtlpLog {
        OtlpLog {
            time_unix_nano: 1_700_000_000_000_000_000,
            observed_time_unix_nano: 1_700_000_000_050_000_000,
            severity_number: SeverityNumber::Error as i32,
            severity_text: "ERROR".to_owned(),
            body: Some(AnyValue {
                value: Some(any_value::Value::StringValue("boom".to_owned())),
            }),
            attributes: vec![kv(
                "log.source",
                any_value::Value::StringValue("app".to_owned()),
            )],
            dropped_attributes_count: 2,
            flags: 1,
            trace_id: (1u8..=16).collect(),
            span_id: (1u8..=8).collect(),
            event_name: String::new(),
        }
    }

    #[test]
    fn otlp_log_record_parity() {
        let request = vec![ResourceLogs {
            resource: Some(sample_resource()),
            scope_logs: vec![ScopeLogs {
                scope: Some(sample_scope()),
                log_records: vec![sample_log()],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];

        let mapped = map_resource_logs(&request);
        assert!(
            mapped.rejected.is_empty(),
            "no rejections: {:?}",
            mapped.rejected
        );
        assert_eq!(mapped.records.len(), 1);
        let record = &mapped.records[0];

        assert_eq!(record.severity_number, Some(17));
        assert_eq!(record.severity_text.as_deref(), Some("ERROR"));
        // OTLP logs.v1 in this proto version has no top-level event_name field.
        assert_eq!(record.event_name, None);
        assert_eq!(
            record.body,
            Some(serde_json::Value::String("boom".to_owned()))
        );
        assert_eq!(record.dropped_attributes_count, 2);
        assert_eq!(record.trace_flags, Some(1));
        assert!(record.time.is_some());

        // trace/span correlation ids preserved.
        assert_eq!(
            record.trace_id.expect("trace id").as_bytes(),
            &(1u8..=16).collect::<Vec<_>>()[..]
        );
        assert_eq!(
            record.span_id.expect("span id").as_bytes(),
            &(1u8..=8).collect::<Vec<_>>()[..]
        );

        // resource + scope carried.
        assert_eq!(
            record.resource.as_ref().expect("resource").service_name,
            "checkout"
        );
        assert_eq!(record.scope.as_ref().expect("scope").name, "wyrd-tracing");

        // attributes opaque.
        assert_eq!(
            record.attributes.get("log.source"),
            Some(&serde_json::Value::String("app".to_owned()))
        );
    }

    #[test]
    fn otlp_log_record_batch_matches_write_schema() {
        let request = vec![ResourceLogs {
            resource: Some(sample_resource()),
            scope_logs: vec![ScopeLogs {
                scope: Some(sample_scope()),
                log_records: vec![sample_log()],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];
        let mapped = map_resource_logs(&request);
        let batch = logs_to_record_batch(&mapped.records).expect("encodes");

        assert_eq!(batch.num_rows(), 1);
        let schema = batch.schema();
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(names.first(), Some(&"time"));
        assert_eq!(names.last(), Some(&"principal_id"));

        let sev = batch
            .column_by_name("severity_number")
            .unwrap()
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(sev.value(0), 17);

        let body = batch
            .column_by_name("body")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(body.value(0)).expect("json");
        assert_eq!(parsed.as_str(), Some("boom"));

        let trace_id = batch
            .column_by_name("trace_id")
            .unwrap()
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .unwrap();
        assert_eq!(trace_id.value(0), &(1u8..=16).collect::<Vec<_>>()[..]);

        let observed = batch
            .column_by_name("observed_time")
            .unwrap()
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();
        assert!(!observed.is_null(0));
    }

    #[test]
    fn log_span_id_without_trace_id_is_rejected() {
        let mut bad = sample_log();
        bad.trace_id = vec![]; // span_id set, trace_id empty → validate() rejects.
        let request = vec![ResourceLogs {
            resource: Some(sample_resource()),
            scope_logs: vec![ScopeLogs {
                scope: None,
                log_records: vec![sample_log(), bad],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];
        let mapped = map_resource_logs(&request);
        assert_eq!(mapped.records.len(), 1);
        assert_eq!(mapped.rejected.len(), 1);
    }
}
