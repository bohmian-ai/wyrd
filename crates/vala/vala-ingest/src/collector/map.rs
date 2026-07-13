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
//! JSON string in the `attributes` `Utf8View` column. Redaction of that column
//! happens at coordinator flush (the table is `PayloadClass::Sensitive`), never
//! in this crate.

use std::sync::Arc;

use arrow::array::{
    ArrayRef, FixedSizeBinaryBuilder, Int64Array, RecordBatch, StringArray, StringViewArray,
    TimestampMicrosecondArray, UInt32Array,
};
use arrow::datatypes::{Field, Schema, SchemaRef};
use chrono::{DateTime, Utc};
use vala_bifrost::tables::traces::SpansTable;
use vala_bifrost::tables::{CorrelationPolicy, DomainTable};
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::system_columns::{CARD_UID, PRINCIPAL_ID, RUN_ID};
use wyrd_spec::vala::trace::{
    InstrumentationScope, Resource, SpanEvent, SpanKind, SpanLink, SpanRecord, SpanStatus,
};

use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, Span as OtlpSpan, span, status::StatusCode};

/// OTel `service.*` resource semantic-convention attribute keys promoted to
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
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
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

    let flags = Arc::new(UInt32Array::from_iter_values(
        records.iter().map(|r| r.flags),
    )) as ArrayRef;
    let trace_state = Arc::new(StringArray::from_iter(
        records.iter().map(|r| Some(r.trace_state.clone())),
    )) as ArrayRef;
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

    let attributes = Arc::new(StringViewArray::from_iter(
        records.iter().map(|r| Some(attrs_json(&r.attributes))),
    )) as ArrayRef;

    let dropped_attributes_count = Arc::new(UInt32Array::from_iter_values(
        records.iter().map(|r| r.dropped_attributes_count),
    )) as ArrayRef;
    let dropped_events_count = Arc::new(UInt32Array::from_iter_values(
        records.iter().map(|r| r.dropped_events_count),
    )) as ArrayRef;
    let dropped_links_count = Arc::new(UInt32Array::from_iter_values(
        records.iter().map(|r| r.dropped_links_count),
    )) as ArrayRef;

    let scope_name = Arc::new(StringArray::from_iter(
        records.iter().map(|r| Some(r.scope.name.clone())),
    )) as ArrayRef;
    let scope_version = Arc::new(StringArray::from_iter(
        records.iter().map(|r| r.scope.version.clone()),
    )) as ArrayRef;
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

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Array, FixedSizeBinaryArray, StringArray, StringViewArray};
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

        // attributes is an opaque JSON string in a Utf8View column.
        let attrs = batch
            .column_by_name("attributes")
            .unwrap()
            .as_any()
            .downcast_ref::<StringViewArray>()
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
}
