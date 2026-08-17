//! Bounded JSON cursor used by the OTLP/HTTP generated-message adapters.
//!
//! The schema walk establishes generated layouts, exact retained backing, and
//! bounded lexical scratch without allocating. Signal-specific constructors
//! then rescan each local array and allocate every generated `Vec`, `String`,
//! identifier, and byte value exactly once. A fixed 128-entry syntax stack
//! validates unknown values without recursive descent.

use std::fmt;
use std::mem::size_of;

#[cfg(test)]
use serde::Deserialize;
#[cfg(test)]
use serde::de::{self, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor};
#[cfg(test)]
use std::borrow::Cow;
use vala_bifrost_redux::gate::{IngestError, OtlpWireLimits};
use wyrd_tonic::otlp::common::v1::{
    AnyValue, ArrayValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList, any_value,
};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics::v1::{
    Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint, Metric, NumberDataPoint,
    ResourceMetrics, ScopeMetrics, SummaryDataPoint, summary_data_point,
};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, span};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

/// Maximum JSON object/array nesting accepted at the transport boundary.
const MAX_JSON_SYNTAX_DEPTH: usize = 128;

/// Maximum recursive OTLP `AnyValue` nesting accepted by signal decoders.
pub(crate) const MAX_ANY_VALUE_DEPTH: usize = 8;

/// Exact final and temporary allocation facts for one JSON decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JsonDecodePlan {
    /// Encoded request bytes inspected by the schema-aware preflight.
    pub(crate) wire_bytes: usize,
    /// Exact recursively retained capacity of the generated request.
    pub(crate) decode_bytes: usize,
    /// Maximum scalable lexical/base64 scratch simultaneously live during decode.
    pub(crate) scratch_bytes: usize,
}

/// Error returned by the bounded JSON cursor.
#[derive(Debug)]
pub(crate) struct JsonDecodeError {
    /// Byte offset at which decoding stopped.
    offset: usize,
    /// Stable description of the malformed JSON condition.
    message: String,
}

impl JsonDecodeError {
    /// Creates a cursor error at `offset`.
    pub(crate) fn at(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for JsonDecodeError {
    /// Formats the malformed-input location without echoing request contents.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for JsonDecodeError {}

#[cfg(test)]
impl de::Error for JsonDecodeError {
    /// Converts a generated-message visitor failure into the cursor error.
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::at(0, message.to_string())
    }
}

/// Validates one complete JSON document without heap allocation.
///
/// The scanner uses a fixed 128-entry syntax stack. It validates escapes,
/// numbers, separators, and trailing input while leaving all bytes borrowed.
///
/// # Errors
///
/// Returns [`IngestError::Decode`] for malformed JSON or syntax nesting above
/// 128 containers.
#[cfg(test)]
fn preflight_json_syntax(input: &[u8]) -> Result<(), IngestError> {
    let mut cursor = JsonCursor::new(input);
    cursor
        .skip_value(0)
        .and_then(|()| {
            cursor.skip_ws();
            if cursor.offset == input.len() {
                Ok(())
            } else {
                Err(cursor.error("trailing JSON input"))
            }
        })
        .map_err(|error| IngestError::Decode(format!("OTLP JSON decode failed: {error}")))
}

/// Preflights one trace JSON export and computes exact generated capacity.
///
/// # Errors
///
/// Returns a stable decode refusal for malformed schema, duplicate generated
/// fields, limit excess, arithmetic overflow, or recursive value depth above 8.
pub(crate) fn preflight_trace_json(
    input: &[u8],
    limits: OtlpWireLimits,
) -> Result<JsonDecodePlan, IngestError> {
    preflight_signal_json(
        input,
        limits,
        MessageKind::TraceRequest,
        size_of::<ExportTraceServiceRequest>(),
    )
}

/// Preflights one metrics JSON export and computes exact generated capacity.
///
/// # Errors
///
/// Returns a stable decode refusal for malformed schema, duplicate generated
/// fields, limit excess, arithmetic overflow, or recursive value depth above 8.
pub(crate) fn preflight_metrics_json(
    input: &[u8],
    limits: OtlpWireLimits,
) -> Result<JsonDecodePlan, IngestError> {
    preflight_signal_json(
        input,
        limits,
        MessageKind::MetricsRequest,
        size_of::<ExportMetricsServiceRequest>(),
    )
}

/// Preflights one logs JSON export and computes exact generated capacity.
///
/// # Errors
///
/// Returns a stable decode refusal for malformed schema, duplicate generated
/// fields, limit excess, arithmetic overflow, or recursive value depth above 8.
pub(crate) fn preflight_logs_json(
    input: &[u8],
    limits: OtlpWireLimits,
) -> Result<JsonDecodePlan, IngestError> {
    preflight_signal_json(
        input,
        limits,
        MessageKind::LogsRequest,
        size_of::<ExportLogsServiceRequest>(),
    )
}

/// Executes the common allocation-free schema walk for one signal root.
fn preflight_signal_json(
    input: &[u8],
    limits: OtlpWireLimits,
    root: MessageKind,
    root_bytes: usize,
) -> Result<JsonDecodePlan, IngestError> {
    if input.len() > limits.request_bytes {
        return Err(malformed("OTLP JSON request byte limit exceeded"));
    }
    let mut facts = JsonFacts::new(limits, root_bytes);
    let mut cursor = JsonCursor::new(input);
    scan_message(&mut cursor, root, 0, &mut facts)
        .and_then(|()| {
            cursor.skip_ws();
            if cursor.offset == input.len() {
                Ok(())
            } else {
                Err(cursor.error("trailing JSON input"))
            }
        })
        .map_err(json_ingest_error)?;
    Ok(JsonDecodePlan {
        wire_bytes: input.len(),
        decode_bytes: facts.decode_bytes,
        scratch_bytes: facts.scratch_bytes,
    })
}

/// Stable malformed-input constructor shared by the JSON preflight.
fn malformed(message: impl Into<String>) -> IngestError {
    IngestError::Decode(message.into())
}

/// Maps a lexical failure to the stable OTLP decode error.
pub(crate) fn json_ingest_error(error: JsonDecodeError) -> IngestError {
    malformed(format!("OTLP JSON decode failed: {error}"))
}

/// Fixed schema kinds traversed by the allocation preflight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MessageKind {
    /// Trace export root.
    TraceRequest,
    /// Metrics export root.
    MetricsRequest,
    /// Logs export root.
    LogsRequest,
    /// Trace resource group.
    ResourceSpans,
    /// Metrics resource group.
    ResourceMetrics,
    /// Logs resource group.
    ResourceLogs,
    /// Trace scope group.
    ScopeSpans,
    /// Metrics scope group.
    ScopeMetrics,
    /// Logs scope group.
    ScopeLogs,
    /// Shared resource.
    Resource,
    /// Shared entity reference.
    EntityRef,
    /// Shared instrumentation scope.
    Scope,
    /// Shared key/value entry.
    KeyValue,
    /// Shared recursive value.
    AnyValue,
    /// Shared array-value wrapper.
    ArrayValue,
    /// Shared key/value-list wrapper.
    KeyValueList,
    /// Trace span.
    Span,
    /// Trace event.
    Event,
    /// Trace link.
    Link,
    /// Trace status.
    Status,
    /// Metric descriptor.
    Metric,
    /// Gauge aggregation.
    Gauge,
    /// Sum aggregation.
    Sum,
    /// Histogram aggregation.
    Histogram,
    /// Exponential-histogram aggregation.
    ExponentialHistogram,
    /// Summary aggregation.
    Summary,
    /// Scalar metric point.
    NumberPoint,
    /// Histogram point.
    HistogramPoint,
    /// Exponential-histogram point.
    ExponentialHistogramPoint,
    /// Exponential bucket set.
    Buckets,
    /// Summary point.
    SummaryPoint,
    /// Summary quantile entry.
    Quantile,
    /// Metric exemplar.
    Exemplar,
    /// Log record.
    LogRecord,
}

/// Exact counters and public-layout facts retained during preflight.
struct JsonFacts {
    /// Shared immutable transport/Scribe limits.
    limits: OtlpWireLimits,
    /// Resource-group count.
    resources: usize,
    /// Scope-group count.
    scopes: usize,
    /// Signal-record count.
    records: usize,
    /// Attribute-entry count.
    attributes: usize,
    /// Retained variable-width byte count.
    value_bytes: usize,
    /// Exact recursively retained generated capacity.
    decode_bytes: usize,
    /// Maximum temporary string/base64 capacity used for one scalar.
    scratch_bytes: usize,
}

impl JsonFacts {
    /// Creates a fact set with the root generated layout charged once.
    const fn new(limits: OtlpWireLimits, root_bytes: usize) -> Self {
        Self {
            limits,
            resources: 0,
            scopes: 0,
            records: 0,
            attributes: 0,
            value_bytes: 0,
            decode_bytes: root_bytes,
            scratch_bytes: 0,
        }
    }

    /// Adds a checked amount and enforces its configured ceiling.
    fn add_bounded(
        value: &mut usize,
        amount: usize,
        limit: usize,
        label: &'static str,
    ) -> Result<(), JsonDecodeError> {
        *value = value
            .checked_add(amount)
            .ok_or_else(|| JsonDecodeError::at(0, format!("OTLP JSON {label} overflow")))?;
        if *value > limit {
            return Err(JsonDecodeError::at(
                0,
                format!("OTLP JSON {label} limit exceeded"),
            ));
        }
        Ok(())
    }

    /// Charges exact retained public-layout/backing bytes.
    fn add_decode(&mut self, amount: usize) -> Result<(), JsonDecodeError> {
        Self::add_bounded(
            &mut self.decode_bytes,
            amount,
            self.limits.material_bytes,
            "material",
        )
    }

    /// Charges retained variable-width bytes and their backing allocation.
    fn add_value(&mut self, amount: usize) -> Result<(), JsonDecodeError> {
        Self::add_bounded(
            &mut self.value_bytes,
            amount,
            self.limits.value_bytes,
            "value bytes",
        )?;
        self.add_decode(amount)
    }

    /// Retains the largest one-at-a-time temporary scalar allocation.
    fn observe_scratch(&mut self, amount: usize) {
        self.scratch_bytes = self.scratch_bytes.max(amount);
    }

    /// Charges a resource, scope, record, or attribute cardinality.
    fn count_kind(&mut self, kind: MessageKind) -> Result<(), JsonDecodeError> {
        match kind {
            MessageKind::ResourceSpans
            | MessageKind::ResourceMetrics
            | MessageKind::ResourceLogs => Self::add_bounded(
                &mut self.resources,
                1,
                self.limits.resources,
                "resource count",
            ),
            MessageKind::ScopeSpans | MessageKind::ScopeMetrics | MessageKind::ScopeLogs => {
                Self::add_bounded(&mut self.scopes, 1, self.limits.scopes, "scope count")
            }
            MessageKind::Span
            | MessageKind::NumberPoint
            | MessageKind::HistogramPoint
            | MessageKind::ExponentialHistogramPoint
            | MessageKind::SummaryPoint
            | MessageKind::LogRecord => {
                Self::add_bounded(&mut self.records, 1, self.limits.records, "record count")
            }
            MessageKind::KeyValue => Self::add_bounded(
                &mut self.attributes,
                1,
                self.limits.attributes,
                "attribute count",
            ),
            _ => Ok(()),
        }
    }
}

/// Canonical lower-camel OTLP JSON field names used by the schema walker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    ResourceSpans,
    ResourceMetrics,
    ResourceLogs,
    Resource,
    ScopeSpans,
    ScopeMetrics,
    ScopeLogs,
    Scope,
    Spans,
    Metrics,
    LogRecords,
    SchemaUrl,
    Attributes,
    DroppedAttributesCount,
    EntityRefs,
    Type,
    IdKeys,
    DescriptionKeys,
    Name,
    Version,
    Key,
    Value,
    Values,
    StringValue,
    BoolValue,
    IntValue,
    DoubleValue,
    ArrayValue,
    KvlistValue,
    BytesValue,
    TraceId,
    SpanId,
    ParentSpanId,
    TraceState,
    Flags,
    Kind,
    StartTimeUnixNano,
    EndTimeUnixNano,
    TimeUnixNano,
    ObservedTimeUnixNano,
    Events,
    DroppedEventsCount,
    Links,
    DroppedLinksCount,
    Status,
    Message,
    Code,
    Description,
    Unit,
    Metadata,
    Gauge,
    Sum,
    Histogram,
    ExponentialHistogram,
    Summary,
    DataPoints,
    AggregationTemporality,
    IsMonotonic,
    Exemplars,
    Count,
    BucketCounts,
    ExplicitBounds,
    Min,
    Max,
    Scale,
    ZeroCount,
    Positive,
    Negative,
    Offset,
    ZeroThreshold,
    QuantileValues,
    Quantile,
    FilteredAttributes,
    SeverityNumber,
    SeverityText,
    Body,
    EventName,
    AsInt,
    AsDouble,
    /// Unknown protobuf field, skipped without allocation.
    Unknown,
}

/// Resolves one validated field token without allocating, including escaped names.
fn field_from_token(token: JsonStringToken<'_>) -> Result<Field, JsonDecodeError> {
    if !token.escaped {
        let text = std::str::from_utf8(token.raw)
            .map_err(|_| JsonDecodeError::at(token.offset, "JSON key is not UTF-8"))?;
        return Ok(field_from_str(text));
    }
    if token.decoded_len > 64 {
        return Ok(Field::Unknown);
    }
    let mut storage = [0u8; 64];
    let written = decode_escaped_into(token.raw, token.offset, &mut storage)?;
    let text = std::str::from_utf8(&storage[..written])
        .map_err(|_| JsonDecodeError::at(token.offset, "JSON key is not UTF-8"))?;
    Ok(field_from_str(text))
}

/// Maps one decoded field name to its closed schema discriminator.
fn field_from_str(value: &str) -> Field {
    match value {
        "resourceSpans" => Field::ResourceSpans,
        "resourceMetrics" => Field::ResourceMetrics,
        "resourceLogs" => Field::ResourceLogs,
        "resource" => Field::Resource,
        "scopeSpans" => Field::ScopeSpans,
        "scopeMetrics" => Field::ScopeMetrics,
        "scopeLogs" => Field::ScopeLogs,
        "scope" => Field::Scope,
        "spans" => Field::Spans,
        "metrics" => Field::Metrics,
        "logRecords" => Field::LogRecords,
        "schemaUrl" => Field::SchemaUrl,
        "attributes" => Field::Attributes,
        "droppedAttributesCount" => Field::DroppedAttributesCount,
        "entityRefs" => Field::EntityRefs,
        "type" => Field::Type,
        "idKeys" => Field::IdKeys,
        "descriptionKeys" => Field::DescriptionKeys,
        "name" => Field::Name,
        "version" => Field::Version,
        "key" => Field::Key,
        "value" => Field::Value,
        "values" => Field::Values,
        "stringValue" => Field::StringValue,
        "boolValue" => Field::BoolValue,
        "intValue" => Field::IntValue,
        "doubleValue" => Field::DoubleValue,
        "arrayValue" => Field::ArrayValue,
        "kvlistValue" => Field::KvlistValue,
        "bytesValue" => Field::BytesValue,
        "traceId" => Field::TraceId,
        "spanId" => Field::SpanId,
        "parentSpanId" => Field::ParentSpanId,
        "traceState" => Field::TraceState,
        "flags" => Field::Flags,
        "kind" => Field::Kind,
        "startTimeUnixNano" => Field::StartTimeUnixNano,
        "endTimeUnixNano" => Field::EndTimeUnixNano,
        "timeUnixNano" => Field::TimeUnixNano,
        "observedTimeUnixNano" => Field::ObservedTimeUnixNano,
        "events" => Field::Events,
        "droppedEventsCount" => Field::DroppedEventsCount,
        "links" => Field::Links,
        "droppedLinksCount" => Field::DroppedLinksCount,
        "status" => Field::Status,
        "message" => Field::Message,
        "code" => Field::Code,
        "description" => Field::Description,
        "unit" => Field::Unit,
        "metadata" => Field::Metadata,
        "gauge" => Field::Gauge,
        "sum" => Field::Sum,
        "histogram" => Field::Histogram,
        "exponentialHistogram" => Field::ExponentialHistogram,
        "summary" => Field::Summary,
        "dataPoints" => Field::DataPoints,
        "aggregationTemporality" => Field::AggregationTemporality,
        "isMonotonic" => Field::IsMonotonic,
        "exemplars" => Field::Exemplars,
        "count" => Field::Count,
        "bucketCounts" => Field::BucketCounts,
        "explicitBounds" => Field::ExplicitBounds,
        "min" => Field::Min,
        "max" => Field::Max,
        "scale" => Field::Scale,
        "zeroCount" => Field::ZeroCount,
        "positive" => Field::Positive,
        "negative" => Field::Negative,
        "offset" => Field::Offset,
        "zeroThreshold" => Field::ZeroThreshold,
        "quantileValues" => Field::QuantileValues,
        "quantile" => Field::Quantile,
        "filteredAttributes" => Field::FilteredAttributes,
        "severityNumber" => Field::SeverityNumber,
        "severityText" => Field::SeverityText,
        "body" => Field::Body,
        "eventName" => Field::EventName,
        "asInt" => Field::AsInt,
        "asDouble" => Field::AsDouble,
        _ => Field::Unknown,
    }
}

/// Decodes an escaped string into caller-owned fixed storage.
fn decode_escaped_into(
    raw: &[u8],
    offset: usize,
    output: &mut [u8],
) -> Result<usize, JsonDecodeError> {
    let mut source = 0usize;
    let mut target = 0usize;
    while source < raw.len() {
        if raw[source] != b'\\' {
            let start = source;
            while source < raw.len() && raw[source] != b'\\' {
                source += 1;
            }
            let part = raw
                .get(start..source)
                .ok_or_else(|| JsonDecodeError::at(offset + start, "invalid JSON key range"))?;
            let end = target
                .checked_add(part.len())
                .ok_or_else(|| JsonDecodeError::at(offset + start, "JSON key overflow"))?;
            output
                .get_mut(target..end)
                .ok_or_else(|| JsonDecodeError::at(offset + start, "JSON key too large"))?
                .copy_from_slice(part);
            target = end;
            continue;
        }
        source += 1;
        let escaped = *raw
            .get(source)
            .ok_or_else(|| JsonDecodeError::at(offset + source, "truncated JSON escape"))?;
        source += 1;
        let mut encoded = [0u8; 4];
        let bytes: &[u8] = match escaped {
            b'"' => b"\"",
            b'\\' => b"\\",
            b'/' => b"/",
            b'b' => b"\x08",
            b'f' => b"\x0c",
            b'n' => b"\n",
            b'r' => b"\r",
            b't' => b"\t",
            b'u' => {
                let (character, consumed) = decode_unicode_escape(&raw[source..], offset + source)?;
                source += consumed;
                character.encode_utf8(&mut encoded).as_bytes()
            }
            _ => {
                return Err(JsonDecodeError::at(
                    offset + source - 1,
                    "invalid JSON escape",
                ));
            }
        };
        let end = target
            .checked_add(bytes.len())
            .ok_or_else(|| JsonDecodeError::at(offset + source, "JSON key overflow"))?;
        output
            .get_mut(target..end)
            .ok_or_else(|| JsonDecodeError::at(offset + source, "JSON key too large"))?
            .copy_from_slice(bytes);
        target = end;
    }
    Ok(target)
}

/// Expected JSON value shape and retained allocation behavior for one field.
#[derive(Clone, Copy)]
enum FieldSpec {
    /// Unknown field skipped under the encoded-byte ceiling.
    Unknown,
    /// Scalar with no retained backing allocation.
    Scalar,
    /// Retained generated string.
    String,
    /// Quoted or unquoted integer whose quoted form is temporary scratch.
    Integer,
    /// Hex-encoded identifier retained as bytes.
    Hex,
    /// Ordinary base64 bytes retained in an `AnyValue`.
    Base64,
    /// Optional nested message whose layout is inline in its parent.
    Message(MessageKind),
    /// Repeated nested messages with one public layout per element.
    Messages(MessageKind, usize),
    /// Repeated strings with one `String` layout per element.
    Strings,
    /// Repeated unsigned 64-bit primitives.
    U64s,
    /// Repeated 64-bit floats.
    F64s,
}

/// Resolves one field against its owning generated message.
fn field_spec(kind: MessageKind, field: Field) -> FieldSpec {
    use Field as F;
    use FieldSpec as S;
    use MessageKind as K;
    match (kind, field) {
        (K::TraceRequest, F::ResourceSpans) => {
            S::Messages(K::ResourceSpans, size_of::<ResourceSpans>())
        }
        (K::MetricsRequest, F::ResourceMetrics) => {
            S::Messages(K::ResourceMetrics, size_of::<ResourceMetrics>())
        }
        (K::LogsRequest, F::ResourceLogs) => {
            S::Messages(K::ResourceLogs, size_of::<ResourceLogs>())
        }

        (K::ResourceSpans, F::Resource)
        | (K::ResourceMetrics, F::Resource)
        | (K::ResourceLogs, F::Resource) => S::Message(K::Resource),
        (K::ResourceSpans, F::ScopeSpans) => S::Messages(K::ScopeSpans, size_of::<ScopeSpans>()),
        (K::ResourceMetrics, F::ScopeMetrics) => {
            S::Messages(K::ScopeMetrics, size_of::<ScopeMetrics>())
        }
        (K::ResourceLogs, F::ScopeLogs) => S::Messages(K::ScopeLogs, size_of::<ScopeLogs>()),
        (K::ResourceSpans | K::ResourceMetrics | K::ResourceLogs, F::SchemaUrl) => S::String,

        (K::ScopeSpans | K::ScopeMetrics | K::ScopeLogs, F::Scope) => S::Message(K::Scope),
        (K::ScopeSpans, F::Spans) => S::Messages(K::Span, size_of::<Span>()),
        (K::ScopeMetrics, F::Metrics) => S::Messages(K::Metric, size_of::<Metric>()),
        (K::ScopeLogs, F::LogRecords) => S::Messages(K::LogRecord, size_of::<LogRecord>()),
        (K::ScopeSpans | K::ScopeMetrics | K::ScopeLogs, F::SchemaUrl) => S::String,

        (K::Resource, F::Attributes) | (K::Scope, F::Attributes) => {
            S::Messages(K::KeyValue, size_of::<KeyValue>())
        }
        (K::Resource, F::EntityRefs) => S::Messages(K::EntityRef, size_of::<EntityRef>()),
        (K::Resource | K::Scope, F::DroppedAttributesCount) => S::Scalar,
        (K::Scope, F::Name | F::Version) => S::String,
        (K::EntityRef, F::SchemaUrl | F::Type) => S::String,
        (K::EntityRef, F::IdKeys | F::DescriptionKeys) => S::Strings,

        (K::KeyValue, F::Key) => S::String,
        (K::KeyValue, F::Value) => S::Message(K::AnyValue),
        (K::ArrayValue, F::Values) => S::Messages(K::AnyValue, size_of::<AnyValue>()),
        (K::KeyValueList, F::Values) => S::Messages(K::KeyValue, size_of::<KeyValue>()),

        (K::Span, F::TraceId | F::SpanId | F::ParentSpanId) => S::Hex,
        (K::Span, F::TraceState | F::Name) => S::String,
        (K::Span, F::StartTimeUnixNano | F::EndTimeUnixNano) => S::Integer,
        (K::Span, F::Attributes) => S::Messages(K::KeyValue, size_of::<KeyValue>()),
        (K::Span, F::Events) => S::Messages(K::Event, size_of::<span::Event>()),
        (K::Span, F::Links) => S::Messages(K::Link, size_of::<span::Link>()),
        (K::Span, F::Status) => S::Message(K::Status),
        (
            K::Span,
            F::Flags
            | F::Kind
            | F::DroppedAttributesCount
            | F::DroppedEventsCount
            | F::DroppedLinksCount,
        ) => S::Scalar,
        (K::Event, F::TimeUnixNano) => S::Integer,
        (K::Event, F::Name) => S::String,
        (K::Event, F::Attributes) => S::Messages(K::KeyValue, size_of::<KeyValue>()),
        (K::Event, F::DroppedAttributesCount) => S::Scalar,
        (K::Link, F::TraceId | F::SpanId) => S::Hex,
        (K::Link, F::TraceState) => S::String,
        (K::Link, F::Attributes) => S::Messages(K::KeyValue, size_of::<KeyValue>()),
        (K::Link, F::DroppedAttributesCount | F::Flags) => S::Scalar,
        (K::Status, F::Message) => S::String,
        (K::Status, F::Code) => S::Scalar,

        (K::Metric, F::Name | F::Description | F::Unit) => S::String,
        (K::Metric, F::Metadata) => S::Messages(K::KeyValue, size_of::<KeyValue>()),
        (K::Metric, F::Gauge) => S::Message(K::Gauge),
        (K::Metric, F::Sum) => S::Message(K::Sum),
        (K::Metric, F::Histogram) => S::Message(K::Histogram),
        (K::Metric, F::ExponentialHistogram) => S::Message(K::ExponentialHistogram),
        (K::Metric, F::Summary) => S::Message(K::Summary),
        (K::Gauge | K::Sum, F::DataPoints) => {
            S::Messages(K::NumberPoint, size_of::<NumberDataPoint>())
        }
        (K::Histogram, F::DataPoints) => {
            S::Messages(K::HistogramPoint, size_of::<HistogramDataPoint>())
        }
        (K::ExponentialHistogram, F::DataPoints) => S::Messages(
            K::ExponentialHistogramPoint,
            size_of::<ExponentialHistogramDataPoint>(),
        ),
        (K::Summary, F::DataPoints) => S::Messages(K::SummaryPoint, size_of::<SummaryDataPoint>()),
        (K::Sum, F::AggregationTemporality | F::IsMonotonic)
        | (K::Histogram | K::ExponentialHistogram, F::AggregationTemporality) => S::Scalar,

        (
            K::NumberPoint | K::HistogramPoint | K::ExponentialHistogramPoint | K::SummaryPoint,
            F::Attributes,
        ) => S::Messages(K::KeyValue, size_of::<KeyValue>()),
        (
            K::NumberPoint | K::HistogramPoint | K::ExponentialHistogramPoint | K::SummaryPoint,
            F::StartTimeUnixNano | F::TimeUnixNano,
        ) => S::Integer,
        (K::NumberPoint | K::HistogramPoint | K::ExponentialHistogramPoint, F::Exemplars) => {
            S::Messages(K::Exemplar, size_of::<Exemplar>())
        }
        (K::NumberPoint, F::AsInt) => S::Integer,
        (K::NumberPoint, F::AsDouble | F::Flags) => S::Scalar,
        (K::HistogramPoint, F::Count) => S::Integer,
        (K::HistogramPoint, F::BucketCounts) => S::U64s,
        (K::HistogramPoint, F::ExplicitBounds) => S::F64s,
        (K::HistogramPoint, F::Sum | F::Min | F::Max | F::Flags) => S::Scalar,
        (K::ExponentialHistogramPoint, F::Count | F::ZeroCount) => S::Integer,
        (K::ExponentialHistogramPoint, F::Positive | F::Negative) => S::Message(K::Buckets),
        (
            K::ExponentialHistogramPoint,
            F::Sum | F::Scale | F::Flags | F::Min | F::Max | F::ZeroThreshold,
        ) => S::Scalar,
        (K::Buckets, F::BucketCounts) => S::U64s,
        (K::Buckets, F::Offset) => S::Scalar,
        (K::SummaryPoint, F::Count) => S::Integer,
        (K::SummaryPoint, F::QuantileValues) => S::Messages(
            K::Quantile,
            size_of::<summary_data_point::ValueAtQuantile>(),
        ),
        (K::SummaryPoint, F::Sum | F::Flags) | (K::Quantile, F::Quantile | F::Value) => S::Scalar,
        (K::Exemplar, F::FilteredAttributes) => S::Messages(K::KeyValue, size_of::<KeyValue>()),
        (K::Exemplar, F::TimeUnixNano | F::AsInt) => S::Integer,
        (K::Exemplar, F::SpanId | F::TraceId) => S::Hex,
        (K::Exemplar, F::AsDouble) => S::Scalar,

        (K::LogRecord, F::TimeUnixNano | F::ObservedTimeUnixNano) => S::Integer,
        (K::LogRecord, F::SeverityText | F::EventName) => S::String,
        (K::LogRecord, F::Body) => S::Message(K::AnyValue),
        (K::LogRecord, F::Attributes) => S::Messages(K::KeyValue, size_of::<KeyValue>()),
        (K::LogRecord, F::TraceId | F::SpanId) => S::Hex,
        (K::LogRecord, F::SeverityNumber | F::DroppedAttributesCount | F::Flags) => S::Scalar,
        _ => S::Unknown,
    }
}

/// Walks one generated-message object and charges its retained backing facts.
fn scan_message(
    cursor: &mut JsonCursor<'_>,
    kind: MessageKind,
    value_depth: usize,
    facts: &mut JsonFacts,
) -> Result<(), JsonDecodeError> {
    if kind == MessageKind::AnyValue {
        return scan_any_value(cursor, value_depth, facts);
    }
    cursor.expect(b'{')?;
    let mut seen = 0u128;
    let mut oneof_seen = false;
    if cursor.peek() == Some(b'}') {
        return cursor.expect(b'}');
    }
    loop {
        let field = field_from_token(cursor.string_token()?)?;
        cursor.expect(b':')?;
        let spec = field_spec(kind, field);
        if !matches!(spec, FieldSpec::Unknown) {
            if is_oneof_field(kind, field) {
                if oneof_seen {
                    return Err(cursor.error("multiple OTLP JSON oneof fields"));
                }
                oneof_seen = true;
            }
            let bit = 1u128
                .checked_shl(field as u32)
                .ok_or_else(|| cursor.error("OTLP JSON field-set overflow"))?;
            if seen & bit != 0 {
                return Err(cursor.error("duplicate OTLP JSON field"));
            }
            seen |= bit;
        }
        scan_field(cursor, spec, value_depth, facts)?;
        match cursor.peek() {
            Some(b',') => cursor.expect(b',')?,
            Some(b'}') => {
                cursor.expect(b'}')?;
                return Ok(());
            }
            _ => return Err(cursor.error("expected OTLP JSON object separator")),
        }
    }
}

/// Reports whether one field belongs to a non-`AnyValue` generated oneof.
fn is_oneof_field(kind: MessageKind, field: Field) -> bool {
    match kind {
        MessageKind::Metric => matches!(
            field,
            Field::Gauge
                | Field::Sum
                | Field::Histogram
                | Field::ExponentialHistogram
                | Field::Summary
        ),
        MessageKind::NumberPoint | MessageKind::Exemplar => {
            matches!(field, Field::AsDouble | Field::AsInt)
        }
        _ => false,
    }
}

/// Scans one field according to its generated-message value shape.
fn scan_field(
    cursor: &mut JsonCursor<'_>,
    spec: FieldSpec,
    value_depth: usize,
    facts: &mut JsonFacts,
) -> Result<(), JsonDecodeError> {
    if cursor.peek() == Some(b'n') {
        cursor.literal(b"null")?;
        return Ok(());
    }
    match spec {
        FieldSpec::Unknown => cursor.skip_value(0),
        FieldSpec::Scalar => scan_scalar(cursor),
        FieldSpec::String => {
            let token = cursor.string_token()?;
            facts.add_value(token.decoded_len)
        }
        FieldSpec::Integer => scan_integer(cursor, facts),
        FieldSpec::Hex => scan_hex(cursor, facts),
        FieldSpec::Base64 => scan_base64(cursor, facts),
        FieldSpec::Message(kind) => scan_message(cursor, kind, value_depth, facts),
        FieldSpec::Messages(kind, layout) => {
            scan_message_array(cursor, kind, layout, value_depth, facts)
        }
        FieldSpec::Strings => scan_string_array(cursor, facts),
        FieldSpec::U64s => scan_primitive_array(cursor, size_of::<u64>(), true, facts),
        FieldSpec::F64s => scan_primitive_array(cursor, size_of::<f64>(), false, facts),
    }
}

/// Scans one scalar accepted by generated protobuf fields.
fn scan_scalar(cursor: &mut JsonCursor<'_>) -> Result<(), JsonDecodeError> {
    match cursor.peek() {
        Some(b't') => cursor.literal(b"true"),
        Some(b'f') => cursor.literal(b"false"),
        Some(b'"') => cursor.string_token().map(|_| ()),
        Some(b'-' | b'0'..=b'9') => cursor.number().map(|_| ()),
        _ => Err(cursor.error("invalid OTLP JSON scalar")),
    }
}

/// Scans a quoted or unquoted integer and records quoted-string scratch.
fn scan_integer(cursor: &mut JsonCursor<'_>, facts: &mut JsonFacts) -> Result<(), JsonDecodeError> {
    if cursor.peek() == Some(b'"') {
        let token = cursor.string_token()?;
        facts.observe_scratch(token.decoded_len);
        return Ok(());
    }
    cursor.number().map(|_| ())
}

/// Validates one hex ID and charges its exact byte-vector capacity.
fn scan_hex(cursor: &mut JsonCursor<'_>, facts: &mut JsonFacts) -> Result<(), JsonDecodeError> {
    let token = cursor.string_token()?;
    if token.decoded_len % 2 != 0 {
        return Err(cursor.error("OTLP JSON hex ID must be even-length"));
    }
    for_each_decoded_ascii(token, |byte| {
        hex_nibble(byte)
            .map(|_| ())
            .ok_or_else(|| JsonDecodeError::at(token.offset, "invalid OTLP JSON hex ID"))
    })?;
    if token.escaped {
        facts.observe_scratch(token.decoded_len);
    }
    facts.add_value(token.decoded_len / 2)
}

/// Validates ordinary padded/unpadded base64 and charges generated capacities.
fn scan_base64(cursor: &mut JsonCursor<'_>, facts: &mut JsonFacts) -> Result<(), JsonDecodeError> {
    let token = cursor.string_token()?;
    let capacity = base64_token_decoded_len(token)
        .ok_or_else(|| cursor.error("invalid OTLP JSON base64 length or padding"))?;
    if token.escaped {
        facts.observe_scratch(token.decoded_len);
    }
    facts.add_value(capacity)
}

/// Scans one repeated generated-message field with exact element layout.
fn scan_message_array(
    cursor: &mut JsonCursor<'_>,
    kind: MessageKind,
    layout: usize,
    value_depth: usize,
    facts: &mut JsonFacts,
) -> Result<(), JsonDecodeError> {
    cursor.expect(b'[')?;
    if cursor.peek() == Some(b']') {
        return cursor.expect(b']');
    }
    loop {
        facts.count_kind(kind)?;
        facts.add_decode(layout)?;
        scan_message(cursor, kind, value_depth, facts)?;
        match cursor.peek() {
            Some(b',') => cursor.expect(b',')?,
            Some(b']') => {
                cursor.expect(b']')?;
                return Ok(());
            }
            _ => return Err(cursor.error("expected OTLP JSON array separator")),
        }
    }
}

/// Scans repeated strings and charges both element layouts and exact backing.
fn scan_string_array(
    cursor: &mut JsonCursor<'_>,
    facts: &mut JsonFacts,
) -> Result<(), JsonDecodeError> {
    cursor.expect(b'[')?;
    if cursor.peek() == Some(b']') {
        return cursor.expect(b']');
    }
    loop {
        facts.add_decode(size_of::<String>())?;
        let token = cursor.string_token()?;
        facts.add_value(token.decoded_len)?;
        match cursor.peek() {
            Some(b',') => cursor.expect(b',')?,
            Some(b']') => {
                cursor.expect(b']')?;
                return Ok(());
            }
            _ => return Err(cursor.error("expected OTLP JSON string-array separator")),
        }
    }
}

/// Scans a repeated primitive field and charges exact vector backing.
fn scan_primitive_array(
    cursor: &mut JsonCursor<'_>,
    layout: usize,
    quoted: bool,
    facts: &mut JsonFacts,
) -> Result<(), JsonDecodeError> {
    cursor.expect(b'[')?;
    if cursor.peek() == Some(b']') {
        return cursor.expect(b']');
    }
    loop {
        facts.add_decode(layout)?;
        if quoted {
            scan_integer(cursor, facts)?;
        } else {
            scan_scalar(cursor)?;
        }
        match cursor.peek() {
            Some(b',') => cursor.expect(b',')?,
            Some(b']') => {
                cursor.expect(b']')?;
                return Ok(());
            }
            _ => return Err(cursor.error("expected OTLP JSON primitive-array separator")),
        }
    }
}

/// Scans an `AnyValue` with last-known-field-wins semantics and depth eight.
fn scan_any_value(
    cursor: &mut JsonCursor<'_>,
    value_depth: usize,
    facts: &mut JsonFacts,
) -> Result<(), JsonDecodeError> {
    if value_depth >= facts.limits.value_depth.min(MAX_ANY_VALUE_DEPTH) {
        return Err(cursor.error("OTLP JSON AnyValue nesting exceeds configured depth"));
    }
    cursor.expect(b'{')?;
    let mut selected: Option<(Field, usize, usize)> = None;
    if cursor.peek() == Some(b'}') {
        return Err(cursor.error("OTLP JSON AnyValue has no known value"));
    }
    loop {
        let field = field_from_token(cursor.string_token()?)?;
        cursor.expect(b':')?;
        let start = cursor.offset;
        cursor.skip_value(0)?;
        let end = cursor.offset;
        if matches!(
            field,
            Field::StringValue
                | Field::BoolValue
                | Field::IntValue
                | Field::DoubleValue
                | Field::ArrayValue
                | Field::KvlistValue
                | Field::BytesValue
        ) {
            selected = Some((field, start, end));
        }
        match cursor.peek() {
            Some(b',') => cursor.expect(b',')?,
            Some(b'}') => {
                cursor.expect(b'}')?;
                break;
            }
            _ => return Err(cursor.error("expected AnyValue object separator")),
        }
    }
    let (field, start, end) =
        selected.ok_or_else(|| cursor.error("OTLP JSON AnyValue has no known value"))?;
    let input = cursor
        .input
        .get(start..end)
        .ok_or_else(|| cursor.error("invalid AnyValue range"))?;
    let mut selected_cursor = JsonCursor::new(input);
    let spec = match field {
        Field::StringValue => FieldSpec::String,
        Field::BoolValue | Field::DoubleValue => FieldSpec::Scalar,
        Field::IntValue => FieldSpec::Integer,
        Field::ArrayValue => FieldSpec::Message(MessageKind::ArrayValue),
        Field::KvlistValue => FieldSpec::Message(MessageKind::KeyValueList),
        Field::BytesValue => FieldSpec::Base64,
        _ => FieldSpec::Unknown,
    };
    scan_field(&mut selected_cursor, spec, value_depth + 1, facts)
}

/// Decodes a repeated message field with one exact allocation.
pub(crate) fn decode_message_array<T>(
    cursor: &mut JsonCursor<'_>,
    mut decode: impl FnMut(&mut JsonCursor<'_>) -> Result<T, JsonDecodeError>,
) -> Result<Vec<T>, JsonDecodeError> {
    let count = cursor.array_len()?;
    let mut values = Vec::with_capacity(count);
    cursor.expect(b'[')?;
    for index in 0..count {
        values.push(decode(cursor)?);
        if index + 1 < count {
            cursor.expect(b',')?;
        }
    }
    cursor.expect(b']')?;
    debug_assert_eq!(values.len(), values.capacity());
    Ok(values)
}

/// Marks one ordinary generated field and rejects duplicates like serde derive.
pub(crate) fn mark_seen(seen: &mut u128, field: Field) -> Result<(), JsonDecodeError> {
    if field == Field::Unknown {
        return Ok(());
    }
    let bit = 1u128
        .checked_shl(field as u32)
        .ok_or_else(|| JsonDecodeError::at(0, "OTLP JSON field-set overflow"))?;
    if *seen & bit != 0 {
        return Err(JsonDecodeError::at(0, "duplicate OTLP JSON field"));
    }
    *seen |= bit;
    Ok(())
}

/// Advances between object fields, returning false after the closing brace.
pub(crate) fn next_object_field(
    cursor: &mut JsonCursor<'_>,
    first: &mut bool,
) -> Result<Option<Field>, JsonDecodeError> {
    if cursor.peek() == Some(b'}') {
        cursor.expect(b'}')?;
        return Ok(None);
    }
    if !*first {
        cursor.expect(b',')?;
    }
    *first = false;
    cursor.field().map(Some)
}

/// Decodes a shared OTLP Resource directly into exact-capacity generated storage.
pub(crate) fn decode_resource(cursor: &mut JsonCursor<'_>) -> Result<Resource, JsonDecodeError> {
    let mut output = Resource::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(
            field,
            Field::Attributes | Field::DroppedAttributesCount | Field::EntityRefs
        ) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::Attributes => {
                output.attributes = decode_message_array(cursor, decode_key_value)?
            }
            Field::DroppedAttributesCount => output.dropped_attributes_count = decode_u32(cursor)?,
            Field::EntityRefs => {
                output.entity_refs = decode_message_array(cursor, decode_entity_ref)?
            }
            Field::Unknown => cursor.skip_value(0)?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes a shared instrumentation scope directly.
pub(crate) fn decode_scope(
    cursor: &mut JsonCursor<'_>,
) -> Result<InstrumentationScope, JsonDecodeError> {
    let mut output = InstrumentationScope::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(
            field,
            Field::Name | Field::Version | Field::Attributes | Field::DroppedAttributesCount
        ) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::Name => output.name = cursor.owned_string()?,
            Field::Version => output.version = cursor.owned_string()?,
            Field::Attributes => {
                output.attributes = decode_message_array(cursor, decode_key_value)?
            }
            Field::DroppedAttributesCount => output.dropped_attributes_count = decode_u32(cursor)?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one shared entity reference directly.
fn decode_entity_ref(cursor: &mut JsonCursor<'_>) -> Result<EntityRef, JsonDecodeError> {
    let mut output = EntityRef::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(
            field,
            Field::SchemaUrl | Field::Type | Field::IdKeys | Field::DescriptionKeys
        ) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::SchemaUrl => output.schema_url = cursor.owned_string()?,
            Field::Type => output.r#type = cursor.owned_string()?,
            Field::IdKeys => output.id_keys = decode_string_array(cursor)?,
            Field::DescriptionKeys => output.description_keys = decode_string_array(cursor)?,
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one shared key/value pair directly.
pub(crate) fn decode_key_value(cursor: &mut JsonCursor<'_>) -> Result<KeyValue, JsonDecodeError> {
    decode_key_value_at(cursor, 0)
}

/// Decodes one shared key/value pair at its enclosing recursive value depth.
fn decode_key_value_at(
    cursor: &mut JsonCursor<'_>,
    depth: usize,
) -> Result<KeyValue, JsonDecodeError> {
    let mut output = KeyValue::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if !matches!(field, Field::Key | Field::Value) {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::Key => output.key = cursor.owned_string()?,
            Field::Value => {
                output.value = decode_optional(cursor, |cursor| decode_any_value(cursor, depth))?
            }
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes one recursive `AnyValue` with depth-eight and last-known-field-wins semantics.
pub(crate) fn decode_any_value(
    cursor: &mut JsonCursor<'_>,
    depth: usize,
) -> Result<AnyValue, JsonDecodeError> {
    if depth >= MAX_ANY_VALUE_DEPTH {
        return Err(JsonDecodeError::at(0, "OTLP JSON AnyValue depth exceeds 8"));
    }
    let mut selected: Option<(Field, usize, usize)> = None;
    let mut first = true;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        let start = cursor.offset;
        cursor.skip_value(0)?;
        let end = cursor.offset;
        if matches!(
            field,
            Field::StringValue
                | Field::BoolValue
                | Field::IntValue
                | Field::DoubleValue
                | Field::BytesValue
                | Field::ArrayValue
                | Field::KvlistValue
        ) {
            selected = Some((field, start, end));
        }
    }
    let (field, start, end) =
        selected.ok_or_else(|| JsonDecodeError::at(0, "OTLP JSON AnyValue has no known value"))?;
    let input = cursor
        .input
        .get(start..end)
        .ok_or_else(|| JsonDecodeError::at(start, "invalid AnyValue range"))?;
    let mut selected_cursor = JsonCursor::new(input);
    let value = match field {
        Field::StringValue => any_value::Value::StringValue(selected_cursor.owned_string()?),
        Field::BoolValue => any_value::Value::BoolValue(selected_cursor.bool()?),
        Field::IntValue => any_value::Value::IntValue(selected_cursor.i64()?),
        Field::DoubleValue => any_value::Value::DoubleValue(selected_cursor.f64()?),
        Field::BytesValue => any_value::Value::BytesValue(selected_cursor.base64_bytes()?),
        Field::ArrayValue => {
            any_value::Value::ArrayValue(decode_array_value(&mut selected_cursor, depth + 1)?)
        }
        Field::KvlistValue => {
            any_value::Value::KvlistValue(decode_key_value_list(&mut selected_cursor, depth + 1)?)
        }
        _ => return Err(JsonDecodeError::at(start, "invalid AnyValue selection")),
    };
    selected_cursor.finish()?;
    Ok(AnyValue { value: Some(value) })
}

/// Decodes an `ArrayValue` wrapper directly.
fn decode_array_value(
    cursor: &mut JsonCursor<'_>,
    depth: usize,
) -> Result<ArrayValue, JsonDecodeError> {
    let mut output = ArrayValue::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if field != Field::Values {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::Values => {
                output.values =
                    decode_message_array(cursor, |cursor| decode_any_value(cursor, depth))?;
            }
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes a `KeyValueList` wrapper directly.
fn decode_key_value_list(
    cursor: &mut JsonCursor<'_>,
    depth: usize,
) -> Result<KeyValueList, JsonDecodeError> {
    if depth > MAX_ANY_VALUE_DEPTH {
        return Err(JsonDecodeError::at(0, "OTLP JSON AnyValue depth exceeds 8"));
    }
    let mut output = KeyValueList::default();
    let mut first = true;
    let mut seen = 0u128;
    cursor.expect(b'{')?;
    while let Some(field) = next_object_field(cursor, &mut first)? {
        if field != Field::Values {
            cursor.skip_value(0)?;
            continue;
        }
        mark_seen(&mut seen, field)?;
        match field {
            Field::Values => {
                output.values =
                    decode_message_array(cursor, |cursor| decode_key_value_at(cursor, depth))?
            }
            _ => cursor.skip_value(0)?,
        }
    }
    Ok(output)
}

/// Decodes an optional generated message, accepting JSON null as absent.
pub(crate) fn decode_optional<T>(
    cursor: &mut JsonCursor<'_>,
    decode: impl FnOnce(&mut JsonCursor<'_>) -> Result<T, JsonDecodeError>,
) -> Result<Option<T>, JsonDecodeError> {
    if cursor.peek() == Some(b'n') {
        cursor.literal(b"null")?;
        Ok(None)
    } else {
        decode(cursor).map(Some)
    }
}

/// Decodes a repeated string field with exact element and backing capacities.
fn decode_string_array(cursor: &mut JsonCursor<'_>) -> Result<Vec<String>, JsonDecodeError> {
    decode_message_array(cursor, |cursor| cursor.owned_string())
}

/// Decodes repeated quoted-or-unquoted uint64 values with exact capacity.
pub(crate) fn decode_u64_array(cursor: &mut JsonCursor<'_>) -> Result<Vec<u64>, JsonDecodeError> {
    decode_message_array(cursor, |cursor| cursor.u64())
}

/// Decodes repeated double values with exact capacity.
pub(crate) fn decode_f64_array(cursor: &mut JsonCursor<'_>) -> Result<Vec<f64>, JsonDecodeError> {
    decode_message_array(cursor, |cursor| cursor.f64())
}

/// Decodes a protobuf uint32 with checked range conversion.
pub(crate) fn decode_u32(cursor: &mut JsonCursor<'_>) -> Result<u32, JsonDecodeError> {
    u32::try_from(cursor.u64()?).map_err(|_| JsonDecodeError::at(0, "OTLP uint32 out of range"))
}

/// Decodes a protobuf int32 with checked range conversion.
pub(crate) fn decode_i32(cursor: &mut JsonCursor<'_>) -> Result<i32, JsonDecodeError> {
    i32::try_from(cursor.i64()?).map_err(|_| JsonDecodeError::at(0, "OTLP int32 out of range"))
}

/// Converts one ASCII hex digit to its nibble.
fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

/// Visits the decoded ASCII bytes of one JSON string without allocating.
///
/// JSON escapes are resolved before `visit` runs. Non-ASCII literal bytes and
/// Unicode escapes that do not resolve to ASCII are rejected because OTLP hex
/// and base64 fields use ASCII alphabets.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] when an escape is malformed, resolves outside
/// ASCII, or `visit` rejects a decoded byte.
fn for_each_decoded_ascii(
    token: JsonStringToken<'_>,
    mut visit: impl FnMut(u8) -> Result<(), JsonDecodeError>,
) -> Result<(), JsonDecodeError> {
    let mut index = 0;
    while index < token.raw.len() {
        let byte = token.raw[index];
        if byte != b'\\' {
            if !byte.is_ascii() {
                return Err(JsonDecodeError::at(
                    token.offset + index,
                    "OTLP encoded bytes must use ASCII",
                ));
            }
            visit(byte)?;
            index += 1;
            continue;
        }

        let escape_offset = token.offset + index;
        index += 1;
        let escaped = *token
            .raw
            .get(index)
            .ok_or_else(|| JsonDecodeError::at(escape_offset, "truncated JSON escape"))?;
        index += 1;
        let decoded = match escaped {
            b'"' | b'\\' | b'/' => escaped,
            b'b' => 0x08,
            b'f' => 0x0c,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'u' => {
                let (character, consumed) =
                    decode_unicode_escape(&token.raw[index..], token.offset + index)?;
                index = index.checked_add(consumed).ok_or_else(|| {
                    JsonDecodeError::at(token.offset + index, "JSON escape offset overflow")
                })?;
                u8::try_from(u32::from(character)).map_err(|_| {
                    JsonDecodeError::at(escape_offset, "OTLP encoded bytes must use ASCII")
                })?
            }
            _ => {
                return Err(JsonDecodeError::at(escape_offset, "invalid JSON escape"));
            }
        };
        visit(decoded)?;
    }
    Ok(())
}

/// Computes the exact decoded length of padded or unpadded base64 in a JSON token.
///
/// The scan resolves JSON escapes without allocation, validates the standard
/// base64 alphabet and canonical trailing bits, and returns the exact retained
/// byte-vector capacity required by the construction pass.
fn base64_token_decoded_len(token: JsonStringToken<'_>) -> Option<usize> {
    let mut length = 0usize;
    let mut padding = 0usize;
    let mut saw_padding = false;
    let mut final_value = None;
    for_each_decoded_ascii(token, |byte| {
        length = length
            .checked_add(1)
            .ok_or_else(|| JsonDecodeError::at(token.offset, "OTLP JSON base64 length overflow"))?;
        if byte == b'=' {
            saw_padding = true;
            padding = padding.checked_add(1).ok_or_else(|| {
                JsonDecodeError::at(token.offset, "OTLP JSON base64 padding overflow")
            })?;
            return Ok(());
        }
        if saw_padding {
            return Err(JsonDecodeError::at(
                token.offset,
                "OTLP JSON base64 data follows padding",
            ));
        }
        final_value =
            Some(base64_value(byte).ok_or_else(|| {
                JsonDecodeError::at(token.offset, "invalid OTLP JSON base64 symbol")
            })?);
        Ok(())
    })
    .ok()?;

    if padding > 2 {
        return None;
    }
    let unpadded = length.checked_sub(padding)?;
    if unpadded == 0 {
        return (padding == 0).then_some(0);
    }
    let remainder = unpadded % 4;
    if remainder == 1 || (padding > 0 && !length.is_multiple_of(4)) {
        return None;
    }
    if (padding == 2 && remainder != 2) || (padding == 1 && remainder != 3) {
        return None;
    }
    let final_value = final_value?;
    if (remainder == 2 && final_value & 0x0f != 0) || (remainder == 3 && final_value & 0x03 != 0) {
        return None;
    }
    let tail = match remainder {
        0 => 0,
        2 => 1,
        3 => 2,
        _ => return None,
    };
    (unpadded / 4).checked_mul(3)?.checked_add(tail)
}

/// Converts one ordinary base64 symbol to its six-bit value.
fn base64_value(value: u8) -> Option<u8> {
    match value {
        b'A'..=b'Z' => Some(value - b'A'),
        b'a'..=b'z' => Some(value - b'a' + 26),
        b'0'..=b'9' => Some(value - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decodes already validated standard base64 into one exact-capacity vector.
///
/// The decoder writes complete quartets and then the two- or three-symbol tail
/// directly, avoiding the conservative output-buffer estimate used by generic
/// base64 decoders for unpadded input.
///
/// # Errors
///
/// Returns [`JsonDecodeError`] if `input` no longer matches the alphabet and
/// shape established by preflight, or if the produced length differs from
/// `decoded_len`.
fn decode_base64_text(
    input: &[u8],
    decoded_len: usize,
    offset: usize,
) -> Result<Vec<u8>, JsonDecodeError> {
    let unpadded_len = input
        .iter()
        .rposition(|byte| *byte != b'=')
        .map_or(0, |last| last + 1);
    let unpadded = &input[..unpadded_len];
    let complete_len = unpadded.len() / 4 * 4;
    let mut output = Vec::with_capacity(decoded_len);

    for (quartet_index, quartet) in unpadded[..complete_len].chunks_exact(4).enumerate() {
        let source_offset = offset + quartet_index * 4;
        let a = base64_value(quartet[0])
            .ok_or_else(|| JsonDecodeError::at(source_offset, "invalid OTLP JSON base64"))?;
        let b = base64_value(quartet[1])
            .ok_or_else(|| JsonDecodeError::at(source_offset + 1, "invalid OTLP JSON base64"))?;
        let c = base64_value(quartet[2])
            .ok_or_else(|| JsonDecodeError::at(source_offset + 2, "invalid OTLP JSON base64"))?;
        let d = base64_value(quartet[3])
            .ok_or_else(|| JsonDecodeError::at(source_offset + 3, "invalid OTLP JSON base64"))?;
        output.push((a << 2) | (b >> 4));
        output.push(((b & 0x0f) << 4) | (c >> 2));
        output.push(((c & 0x03) << 6) | d);
    }

    let tail = &unpadded[complete_len..];
    if tail.len() >= 2 {
        let a = base64_value(tail[0]).ok_or_else(|| {
            JsonDecodeError::at(offset + complete_len, "invalid OTLP JSON base64")
        })?;
        let b = base64_value(tail[1]).ok_or_else(|| {
            JsonDecodeError::at(offset + complete_len + 1, "invalid OTLP JSON base64")
        })?;
        output.push((a << 2) | (b >> 4));
        if tail.len() == 3 {
            let c = base64_value(tail[2]).ok_or_else(|| {
                JsonDecodeError::at(offset + complete_len + 2, "invalid OTLP JSON base64")
            })?;
            output.push(((b & 0x0f) << 4) | (c >> 2));
        }
    }

    if output.len() != decoded_len {
        return Err(JsonDecodeError::at(
            offset,
            "OTLP JSON base64 preflight/decode length mismatch",
        ));
    }
    Ok(output)
}

/// Computes retained backing beneath a shared Resource.
pub(crate) fn resource_capacity(resource: &Resource) -> usize {
    attributes_capacity(&resource.attributes)
        + resource.entity_refs.capacity() * size_of::<EntityRef>()
        + resource
            .entity_refs
            .iter()
            .map(|entity| {
                entity.schema_url.capacity()
                    + entity.r#type.capacity()
                    + entity.id_keys.capacity() * size_of::<String>()
                    + entity.id_keys.iter().map(String::capacity).sum::<usize>()
                    + entity.description_keys.capacity() * size_of::<String>()
                    + entity
                        .description_keys
                        .iter()
                        .map(String::capacity)
                        .sum::<usize>()
            })
            .sum::<usize>()
}

/// Computes retained backing beneath a shared instrumentation scope.
pub(crate) fn scope_capacity(scope: &InstrumentationScope) -> usize {
    scope.name.capacity() + scope.version.capacity() + attributes_capacity(&scope.attributes)
}

/// Computes retained layout and backing for a shared attribute vector.
pub(crate) fn attributes_capacity(attributes: &Vec<KeyValue>) -> usize {
    attributes.capacity() * size_of::<KeyValue>()
        + attributes
            .iter()
            .map(|attribute| {
                attribute.key.capacity() + attribute.value.as_ref().map_or(0, any_value_capacity)
            })
            .sum::<usize>()
}

/// Computes recursive retained backing beneath one shared `AnyValue`.
pub(crate) fn any_value_capacity(value: &AnyValue) -> usize {
    match value.value.as_ref() {
        Some(any_value::Value::StringValue(value)) => value.capacity(),
        Some(any_value::Value::BytesValue(value)) => value.capacity(),
        Some(any_value::Value::ArrayValue(array)) => {
            array.values.capacity() * size_of::<AnyValue>()
                + array.values.iter().map(any_value_capacity).sum::<usize>()
        }
        Some(any_value::Value::KvlistValue(list)) => attributes_capacity(&list.values),
        _ => 0,
    }
}

/// Directly constructs a generated DTO through the exact-capacity cursor.
///
/// Every sequence visitor receives its exact element count from a borrowed
/// look-ahead cursor, so repeated generated fields cannot grow while decoding.
///
/// # Errors
///
/// Returns [`IngestError::Decode`] when the document is malformed or does not
/// satisfy the generated DTO's protobuf-JSON schema.
#[cfg(test)]
pub(crate) fn decode_json_exact<T>(input: &[u8]) -> Result<T, IngestError>
where
    T: for<'de> Deserialize<'de>,
{
    let mut decoder = JsonDeserializer::new(input);
    let value = T::deserialize(&mut decoder)
        .map_err(|error| IngestError::Decode(format!("OTLP JSON decode failed: {error}")))?;
    decoder.cursor.skip_ws();
    if decoder.cursor.offset != input.len() {
        return Err(IngestError::Decode(
            "OTLP JSON decode failed: trailing JSON input".to_owned(),
        ));
    }
    Ok(value)
}

/// Borrowing cursor over one JSON document.
#[derive(Clone)]
pub(crate) struct JsonCursor<'de> {
    /// Original request bytes retained by the HTTP body.
    input: &'de [u8],
    /// Current byte offset.
    offset: usize,
}

/// Borrowed lexical facts for one validated JSON string token.
#[derive(Clone, Copy)]
struct JsonStringToken<'de> {
    /// Bytes between quotes, retaining JSON escapes.
    raw: &'de [u8],
    /// Absolute byte offset of `raw`.
    offset: usize,
    /// Exact decoded UTF-8 byte length.
    decoded_len: usize,
    /// Whether `raw` contains an escape.
    escaped: bool,
}

impl<'de> JsonCursor<'de> {
    /// Creates a cursor at the start of `input`.
    pub(crate) const fn new(input: &'de [u8]) -> Self {
        Self { input, offset: 0 }
    }

    /// Creates an error at the current byte offset.
    fn error(&self, message: impl Into<String>) -> JsonDecodeError {
        JsonDecodeError::at(self.offset, message)
    }

    /// Skips JSON whitespace.
    fn skip_ws(&mut self) {
        while matches!(
            self.input.get(self.offset),
            Some(b' ' | b'\n' | b'\r' | b'\t')
        ) {
            self.offset += 1;
        }
    }

    /// Returns the next non-whitespace byte without consuming it.
    pub(crate) fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.input.get(self.offset).copied()
    }

    /// Consumes one expected punctuation byte.
    pub(crate) fn expect(&mut self, expected: u8) -> Result<(), JsonDecodeError> {
        self.skip_ws();
        if self.input.get(self.offset) == Some(&expected) {
            self.offset += 1;
            Ok(())
        } else {
            Err(self.error(format!("expected `{}`", char::from(expected))))
        }
    }

    /// Consumes an exact ASCII literal.
    pub(crate) fn literal(&mut self, literal: &[u8]) -> Result<(), JsonDecodeError> {
        self.skip_ws();
        let end = self
            .offset
            .checked_add(literal.len())
            .ok_or_else(|| self.error("JSON offset overflow"))?;
        if self.input.get(self.offset..end) == Some(literal) {
            self.offset = end;
            Ok(())
        } else {
            Err(self.error("invalid JSON literal"))
        }
    }

    /// Validates and borrows one string token without allocating.
    fn string_token(&mut self) -> Result<JsonStringToken<'de>, JsonDecodeError> {
        self.expect(b'"')?;
        let start = self.offset;
        let mut scan = self.offset;
        let mut escaped = false;
        while let Some(&byte) = self.input.get(scan) {
            match byte {
                b'"' => {
                    let raw = self
                        .input
                        .get(start..scan)
                        .ok_or_else(|| self.error("invalid JSON string range"))?;
                    self.offset = scan + 1;
                    let decoded_len = if escaped {
                        escaped_string_len(raw, start)?
                    } else {
                        std::str::from_utf8(raw)
                            .map_err(|_| self.error("JSON string is not UTF-8"))?
                            .len()
                    };
                    return Ok(JsonStringToken {
                        raw,
                        offset: start,
                        decoded_len,
                        escaped,
                    });
                }
                b'\\' => {
                    escaped = true;
                    scan = scan
                        .checked_add(2)
                        .ok_or_else(|| self.error("JSON string offset overflow"))?;
                }
                0x00..=0x1f => return Err(self.error("unescaped JSON control character")),
                _ => scan += 1,
            }
        }
        Err(self.error("unterminated JSON string"))
    }

    /// Borrows an unescaped string or constructs one exact-capacity decoded string.
    #[cfg(test)]
    fn string(&mut self) -> Result<Cow<'de, str>, JsonDecodeError> {
        let token = self.string_token()?;
        if token.escaped {
            decode_escaped_string(token.raw, token.offset).map(Cow::Owned)
        } else {
            std::str::from_utf8(token.raw)
                .map(Cow::Borrowed)
                .map_err(|_| JsonDecodeError::at(token.offset, "JSON string is not UTF-8"))
        }
    }

    /// Decodes one generated string into exactly sized owned storage.
    pub(crate) fn owned_string(&mut self) -> Result<String, JsonDecodeError> {
        let token = self.string_token()?;
        if token.escaped {
            return decode_escaped_string(token.raw, token.offset);
        }
        let mut value = String::with_capacity(token.decoded_len);
        value.push_str(
            std::str::from_utf8(token.raw)
                .map_err(|_| JsonDecodeError::at(token.offset, "JSON string is not UTF-8"))?,
        );
        debug_assert_eq!(value.len(), value.capacity());
        Ok(value)
    }

    /// Decodes the next lower-camel OTLP object key and its colon without allocation.
    pub(crate) fn field(&mut self) -> Result<Field, JsonDecodeError> {
        let field = field_from_token(self.string_token()?)?;
        self.expect(b':')?;
        Ok(field)
    }

    /// Verifies that the cursor consumed the complete document.
    pub(crate) fn finish(mut self) -> Result<(), JsonDecodeError> {
        self.skip_ws();
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(self.error("trailing OTLP JSON input"))
        }
    }

    /// Decodes a quoted or unquoted signed 64-bit integer.
    pub(crate) fn i64(&mut self) -> Result<i64, JsonDecodeError> {
        if self.peek() == Some(b'"') {
            return self
                .owned_string()?
                .parse()
                .map_err(|_| self.error("invalid quoted OTLP int64"));
        }
        self.number()?
            .parse()
            .map_err(|_| self.error("invalid OTLP int64"))
    }

    /// Decodes a quoted or unquoted unsigned 64-bit integer.
    pub(crate) fn u64(&mut self) -> Result<u64, JsonDecodeError> {
        if self.peek() == Some(b'"') {
            return self
                .owned_string()?
                .parse()
                .map_err(|_| self.error("invalid quoted OTLP uint64"));
        }
        self.number()?
            .parse()
            .map_err(|_| self.error("invalid OTLP uint64"))
    }

    /// Decodes one JSON number as `f64`.
    pub(crate) fn f64(&mut self) -> Result<f64, JsonDecodeError> {
        if self.peek() == Some(b'"') {
            let token = self.string_token()?;
            if token.escaped {
                return Err(self.error("escaped OTLP special double is unsupported"));
            }
            return std::str::from_utf8(token.raw)
                .map_err(|_| self.error("invalid OTLP special double encoding"))?
                .parse()
                .map_err(|_| self.error("invalid OTLP special double"));
        }
        self.number()?
            .parse()
            .map_err(|_| self.error("invalid OTLP double"))
    }

    /// Decodes one JSON boolean.
    pub(crate) fn bool(&mut self) -> Result<bool, JsonDecodeError> {
        if self.peek() == Some(b't') {
            self.literal(b"true")?;
            Ok(true)
        } else {
            self.literal(b"false")?;
            Ok(false)
        }
    }

    /// Decodes one hex identifier into an exactly sized vector.
    pub(crate) fn hex_bytes(&mut self) -> Result<Vec<u8>, JsonDecodeError> {
        let token = self.string_token()?;
        if token.decoded_len % 2 != 0 {
            return Err(self.error("invalid OTLP hex identifier"));
        }
        let decoded;
        let text = if token.escaped {
            decoded = decode_escaped_string(token.raw, token.offset)?;
            decoded.as_bytes()
        } else {
            token.raw
        };
        let mut bytes = Vec::with_capacity(token.decoded_len / 2);
        for pair in text.chunks_exact(2) {
            let high = hex_nibble(pair[0]).ok_or_else(|| self.error("invalid OTLP hex digit"))?;
            let low = hex_nibble(pair[1]).ok_or_else(|| self.error("invalid OTLP hex digit"))?;
            bytes.push((high << 4) | low);
        }
        Ok(bytes)
    }

    /// Decodes ordinary base64 into an exactly sized vector.
    pub(crate) fn base64_bytes(&mut self) -> Result<Vec<u8>, JsonDecodeError> {
        let token = self.string_token()?;
        let decoded_len = base64_token_decoded_len(token)
            .ok_or_else(|| self.error("invalid OTLP base64 length or padding"))?;
        let decoded;
        let text = if token.escaped {
            decoded = decode_escaped_string(token.raw, token.offset)?;
            decoded.as_bytes()
        } else {
            token.raw
        };
        decode_base64_text(text, decoded_len, token.offset)
    }

    /// Borrows the next complete JSON number lexeme.
    pub(crate) fn number(&mut self) -> Result<&'de str, JsonDecodeError> {
        self.skip_ws();
        let start = self.offset;
        if self.input.get(self.offset) == Some(&b'-') {
            self.offset += 1;
        }
        match self.input.get(self.offset) {
            Some(b'0') => self.offset += 1,
            Some(b'1'..=b'9') => {
                self.offset += 1;
                while matches!(self.input.get(self.offset), Some(b'0'..=b'9')) {
                    self.offset += 1;
                }
            }
            _ => return Err(self.error("invalid JSON number")),
        }
        if self.input.get(self.offset) == Some(&b'.') {
            self.offset += 1;
            let fraction = self.offset;
            while matches!(self.input.get(self.offset), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
            if fraction == self.offset {
                return Err(self.error("missing JSON fractional digits"));
            }
        }
        if matches!(self.input.get(self.offset), Some(b'e' | b'E')) {
            self.offset += 1;
            if matches!(self.input.get(self.offset), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            let exponent = self.offset;
            while matches!(self.input.get(self.offset), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
            if exponent == self.offset {
                return Err(self.error("missing JSON exponent digits"));
            }
        }
        let bytes = self
            .input
            .get(start..self.offset)
            .ok_or_else(|| self.error("invalid JSON number range"))?;
        std::str::from_utf8(bytes).map_err(|_| self.error("invalid JSON number encoding"))
    }

    /// Skips one complete value with an actual fixed 128-entry syntax stack.
    pub(crate) fn skip_value(&mut self, _depth: usize) -> Result<(), JsonDecodeError> {
        let mut stack = [SyntaxFrame::EMPTY; MAX_JSON_SYNTAX_DEPTH];
        let mut stack_len = 0usize;
        let mut need_value = true;
        loop {
            if need_value {
                match self.peek() {
                    Some(b'{') => {
                        if stack_len == stack.len() {
                            return Err(self.error("JSON syntax nesting exceeds 128"));
                        }
                        self.expect(b'{')?;
                        if self.peek() == Some(b'}') {
                            self.expect(b'}')?;
                            need_value = false;
                        } else {
                            stack[stack_len] = SyntaxFrame::OBJECT_KEY;
                            stack_len += 1;
                            self.string_token()?;
                            self.expect(b':')?;
                        }
                    }
                    Some(b'[') => {
                        if stack_len == stack.len() {
                            return Err(self.error("JSON syntax nesting exceeds 128"));
                        }
                        self.expect(b'[')?;
                        if self.peek() == Some(b']') {
                            self.expect(b']')?;
                            need_value = false;
                        } else {
                            stack[stack_len] = SyntaxFrame::ARRAY_VALUE;
                            stack_len += 1;
                        }
                    }
                    Some(b'"') => {
                        self.string_token()?;
                        need_value = false;
                    }
                    Some(b't') => {
                        self.literal(b"true")?;
                        need_value = false;
                    }
                    Some(b'f') => {
                        self.literal(b"false")?;
                        need_value = false;
                    }
                    Some(b'n') => {
                        self.literal(b"null")?;
                        need_value = false;
                    }
                    Some(b'-' | b'0'..=b'9') => {
                        self.number()?;
                        need_value = false;
                    }
                    Some(_) => return Err(self.error("invalid JSON value")),
                    None => return Err(self.error("unexpected end of JSON input")),
                }
                if need_value {
                    continue;
                }
            }

            if stack_len == 0 {
                return Ok(());
            }
            let frame = &stack[stack_len - 1];
            match (frame.kind, self.peek()) {
                (SyntaxKind::Array, Some(b',')) => {
                    self.expect(b',')?;
                    need_value = true;
                }
                (SyntaxKind::Array, Some(b']')) => {
                    self.expect(b']')?;
                    stack_len -= 1;
                    need_value = false;
                }
                (SyntaxKind::Object, Some(b',')) => {
                    self.expect(b',')?;
                    self.string_token()?;
                    self.expect(b':')?;
                    need_value = true;
                }
                (SyntaxKind::Object, Some(b'}')) => {
                    self.expect(b'}')?;
                    stack_len -= 1;
                    need_value = false;
                }
                _ => return Err(self.error("expected JSON container separator")),
            }
        }
    }

    /// Counts elements in the next array without modifying this cursor.
    pub(crate) fn array_len(&self) -> Result<usize, JsonDecodeError> {
        let mut lookahead = self.clone();
        lookahead.expect(b'[')?;
        if lookahead.peek() == Some(b']') {
            return Ok(0);
        }
        let mut count = 0usize;
        loop {
            lookahead.skip_value(1)?;
            count = count
                .checked_add(1)
                .ok_or_else(|| lookahead.error("JSON array length overflow"))?;
            match lookahead.peek() {
                Some(b',') => lookahead.expect(b',')?,
                Some(b']') => return Ok(count),
                _ => return Err(lookahead.error("expected array separator")),
            }
        }
    }
}

/// Fixed-stack JSON container kind.
#[derive(Clone, Copy)]
enum SyntaxKind {
    /// Object delimited by braces.
    Object,
    /// Array delimited by brackets.
    Array,
}

/// One entry in the iterative JSON syntax stack.
#[derive(Clone, Copy)]
struct SyntaxFrame {
    /// Container kind.
    kind: SyntaxKind,
}

impl SyntaxFrame {
    /// Placeholder used to initialize the fixed stack.
    const EMPTY: Self = Self {
        kind: SyntaxKind::Array,
    };
    /// Object frame positioned before its first value.
    const OBJECT_KEY: Self = Self {
        kind: SyntaxKind::Object,
    };
    /// Array frame positioned before its first value.
    const ARRAY_VALUE: Self = Self {
        kind: SyntaxKind::Array,
    };
}

/// Decodes one escaped JSON string with an exact output allocation.
fn decode_escaped_string(raw: &[u8], offset: usize) -> Result<String, JsonDecodeError> {
    let decoded_len = escaped_string_len(raw, offset)?;
    let mut output = String::with_capacity(decoded_len);
    let mut index = 0usize;
    while index < raw.len() {
        if raw[index] != b'\\' {
            let start = index;
            while index < raw.len() && raw[index] != b'\\' {
                index += 1;
            }
            let part = std::str::from_utf8(&raw[start..index])
                .map_err(|_| JsonDecodeError::at(offset + start, "JSON string is not UTF-8"))?;
            output.push_str(part);
            continue;
        }
        index += 1;
        let escaped = *raw
            .get(index)
            .ok_or_else(|| JsonDecodeError::at(offset + index, "truncated JSON escape"))?;
        index += 1;
        match escaped {
            b'"' => output.push('"'),
            b'\\' => output.push('\\'),
            b'/' => output.push('/'),
            b'b' => output.push('\u{0008}'),
            b'f' => output.push('\u{000c}'),
            b'n' => output.push('\n'),
            b'r' => output.push('\r'),
            b't' => output.push('\t'),
            b'u' => {
                let (character, consumed) = decode_unicode_escape(&raw[index..], offset + index)?;
                output.push(character);
                index += consumed;
            }
            _ => {
                return Err(JsonDecodeError::at(
                    offset + index - 1,
                    "invalid JSON escape",
                ));
            }
        }
    }
    debug_assert_eq!(output.len(), decoded_len);
    Ok(output)
}

/// Counts decoded UTF-8 bytes for one escaped string without allocating.
fn escaped_string_len(raw: &[u8], offset: usize) -> Result<usize, JsonDecodeError> {
    let mut length = 0usize;
    let mut index = 0usize;
    while index < raw.len() {
        if raw[index] != b'\\' {
            let start = index;
            while index < raw.len() && raw[index] != b'\\' {
                index += 1;
            }
            let part = std::str::from_utf8(&raw[start..index])
                .map_err(|_| JsonDecodeError::at(offset + start, "JSON string is not UTF-8"))?;
            length = length.checked_add(part.len()).ok_or_else(|| {
                JsonDecodeError::at(offset + start, "JSON string length overflow")
            })?;
            continue;
        }
        index += 1;
        let escaped = *raw
            .get(index)
            .ok_or_else(|| JsonDecodeError::at(offset + index, "truncated JSON escape"))?;
        index += 1;
        let bytes = match escaped {
            b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => 1,
            b'u' => {
                let (character, consumed) = decode_unicode_escape(&raw[index..], offset + index)?;
                index += consumed;
                character.len_utf8()
            }
            _ => {
                return Err(JsonDecodeError::at(
                    offset + index - 1,
                    "invalid JSON escape",
                ));
            }
        };
        length = length
            .checked_add(bytes)
            .ok_or_else(|| JsonDecodeError::at(offset + index, "JSON string length overflow"))?;
    }
    Ok(length)
}

/// Decodes one `\uXXXX` escape, including a required low surrogate when needed.
fn decode_unicode_escape(input: &[u8], offset: usize) -> Result<(char, usize), JsonDecodeError> {
    let high = decode_hex_quad(input, offset)?;
    if (0xd800..=0xdbff).contains(&high) {
        if input.get(4..6) != Some(b"\\u") {
            return Err(JsonDecodeError::at(offset, "missing low JSON surrogate"));
        }
        let low = decode_hex_quad(&input[6..], offset + 6)?;
        if !(0xdc00..=0xdfff).contains(&low) {
            return Err(JsonDecodeError::at(
                offset + 6,
                "invalid low JSON surrogate",
            ));
        }
        let scalar = 0x1_0000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00);
        char::from_u32(scalar)
            .map(|value| (value, 10))
            .ok_or_else(|| JsonDecodeError::at(offset, "invalid JSON Unicode scalar"))
    } else if (0xdc00..=0xdfff).contains(&high) {
        Err(JsonDecodeError::at(offset, "unpaired low JSON surrogate"))
    } else {
        char::from_u32(u32::from(high))
            .map(|value| (value, 4))
            .ok_or_else(|| JsonDecodeError::at(offset, "invalid JSON Unicode scalar"))
    }
}

/// Decodes four hexadecimal digits without allocating.
fn decode_hex_quad(input: &[u8], offset: usize) -> Result<u16, JsonDecodeError> {
    let digits = input
        .get(..4)
        .ok_or_else(|| JsonDecodeError::at(offset, "truncated JSON Unicode escape"))?;
    let mut value = 0u16;
    for (index, digit) in digits.iter().copied().enumerate() {
        let nibble = match digit {
            b'0'..=b'9' => u16::from(digit - b'0'),
            b'a'..=b'f' => u16::from(digit - b'a' + 10),
            b'A'..=b'F' => u16::from(digit - b'A' + 10),
            _ => {
                return Err(JsonDecodeError::at(
                    offset + index,
                    "invalid JSON hex digit",
                ));
            }
        };
        value = (value << 4) | nibble;
    }
    Ok(value)
}

/// Serde-compatible direct decoder backed by [`JsonCursor`].
#[cfg(test)]
struct JsonDeserializer<'de> {
    /// Borrowing lexical cursor.
    cursor: JsonCursor<'de>,
}

#[cfg(test)]
impl<'de> JsonDeserializer<'de> {
    /// Creates a direct generated-message decoder.
    const fn new(input: &'de [u8]) -> Self {
        Self {
            cursor: JsonCursor::new(input),
        }
    }
}

/// Exact-length array access for generated repeated fields.
#[cfg(test)]
struct JsonSeqAccess<'a, 'de> {
    /// Decoder positioned after the opening bracket.
    decoder: &'a mut JsonDeserializer<'de>,
    /// Elements remaining in this array.
    remaining: usize,
}

#[cfg(test)]
impl<'de> SeqAccess<'de> for JsonSeqAccess<'_, 'de> {
    type Error = JsonDecodeError;

    /// Decodes the next array element and consumes its separator.
    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if self.remaining == 0 {
            self.decoder.cursor.expect(b']')?;
            return Ok(None);
        }
        let value = seed.deserialize(&mut *self.decoder)?;
        self.remaining -= 1;
        if self.remaining > 0 {
            self.decoder.cursor.expect(b',')?;
        }
        Ok(Some(value))
    }

    /// Supplies the exact element count so generated `Vec`s allocate once.
    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}

/// Object access that leaves unknown-field skipping to generated visitors.
#[cfg(test)]
struct JsonMapAccess<'a, 'de> {
    /// Decoder positioned after the opening brace.
    decoder: &'a mut JsonDeserializer<'de>,
    /// Whether the next entry is the first entry.
    first: bool,
    /// Whether the closing brace has been consumed.
    finished: bool,
}

#[cfg(test)]
impl<'de> MapAccess<'de> for JsonMapAccess<'_, 'de> {
    type Error = JsonDecodeError;

    /// Decodes the next object key.
    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        if self.finished {
            return Ok(None);
        }
        if self.decoder.cursor.peek() == Some(b'}') {
            self.decoder.cursor.expect(b'}')?;
            self.finished = true;
            return Ok(None);
        }
        if !self.first {
            self.decoder.cursor.expect(b',')?;
        }
        self.first = false;
        let key = self.decoder.cursor.string()?;
        self.decoder.cursor.expect(b':')?;
        match key {
            Cow::Borrowed(value) => seed.deserialize(value.into_deserializer()).map(Some),
            Cow::Owned(value) => seed.deserialize(value.into_deserializer()).map(Some),
        }
    }

    /// Decodes the value corresponding to the preceding key.
    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self.decoder)
    }
}

#[cfg(test)]
impl<'de> de::Deserializer<'de> for &mut JsonDeserializer<'de> {
    type Error = JsonDecodeError;

    /// Dispatches based on the next JSON token.
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.cursor.peek() {
            Some(b'{') => self.deserialize_map(visitor),
            Some(b'[') => self.deserialize_seq(visitor),
            Some(b'"') => self.deserialize_string(visitor),
            Some(b't' | b'f') => self.deserialize_bool(visitor),
            Some(b'n') => self.deserialize_option(visitor),
            Some(b'-' | b'0'..=b'9') => {
                let number = self.cursor.number()?;
                if number.contains(['.', 'e', 'E']) {
                    visitor.visit_f64(number.parse().map_err(de::Error::custom)?)
                } else if number.starts_with('-') {
                    visitor.visit_i64(number.parse().map_err(de::Error::custom)?)
                } else {
                    visitor.visit_u64(number.parse().map_err(de::Error::custom)?)
                }
            }
            _ => Err(self.cursor.error("invalid JSON value")),
        }
    }

    /// Decodes a boolean literal.
    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.cursor.peek() == Some(b't') {
            self.cursor.literal(b"true")?;
            visitor.visit_bool(true)
        } else {
            self.cursor.literal(b"false")?;
            visitor.visit_bool(false)
        }
    }

    /// Decodes a signed integer, accepting a quoted protobuf integer too.
    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = if self.cursor.peek() == Some(b'"') {
            self.cursor.string()?.parse().map_err(de::Error::custom)?
        } else {
            self.cursor.number()?.parse().map_err(de::Error::custom)?
        };
        visitor.visit_i64(value)
    }

    /// Decodes an unsigned integer, accepting a quoted protobuf integer too.
    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = if self.cursor.peek() == Some(b'"') {
            self.cursor.string()?.parse().map_err(de::Error::custom)?
        } else {
            self.cursor.number()?.parse().map_err(de::Error::custom)?
        };
        visitor.visit_u64(value)
    }

    /// Decodes a floating-point value.
    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = if self.cursor.peek() == Some(b'"') {
            self.cursor.string()?.parse().map_err(de::Error::custom)?
        } else {
            self.cursor.number()?.parse().map_err(de::Error::custom)?
        };
        visitor.visit_f64(value)
    }

    /// Decodes a character from a one-scalar JSON string.
    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let text = self.cursor.string()?;
        let mut chars = text.chars();
        let value = chars
            .next()
            .ok_or_else(|| self.cursor.error("empty JSON character"))?;
        if chars.next().is_some() {
            return Err(self.cursor.error("multi-scalar JSON character"));
        }
        visitor.visit_char(value)
    }

    /// Decodes a string, also exposing number lexemes as strings for protobuf int64 helpers.
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.cursor.peek() == Some(b'"') {
            match self.cursor.string()? {
                Cow::Borrowed(value) => visitor.visit_borrowed_str(value),
                Cow::Owned(value) => visitor.visit_string(value),
            }
        } else {
            visitor.visit_borrowed_str(self.cursor.number()?)
        }
    }

    /// Decodes an owned string through the same exact lexical path.
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    /// Decodes byte visitors from an ordinary JSON string.
    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.cursor.string()? {
            Cow::Borrowed(value) => visitor.visit_borrowed_bytes(value.as_bytes()),
            Cow::Owned(value) => visitor.visit_byte_buf(value.into_bytes()),
        }
    }

    /// Decodes an owned byte buffer from an ordinary JSON string.
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    /// Decodes nullable optional fields.
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.cursor.peek() == Some(b'n') {
            self.cursor.literal(b"null")?;
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    /// Decodes a unit from JSON null.
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.cursor.literal(b"null")?;
        visitor.visit_unit()
    }

    /// Decodes a unit struct from JSON null.
    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    /// Decodes a newtype through its wrapped visitor.
    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    /// Decodes an array after computing its exact element count.
    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let length = self.cursor.array_len()?;
        self.cursor.expect(b'[')?;
        visitor.visit_seq(JsonSeqAccess {
            decoder: self,
            remaining: length,
        })
    }

    /// Decodes a fixed tuple from a JSON array.
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    /// Decodes a tuple struct from a JSON array.
    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    /// Decodes an object and delegates field behavior to the generated visitor.
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.cursor.expect(b'{')?;
        visitor.visit_map(JsonMapAccess {
            decoder: self,
            first: true,
            finished: false,
        })
    }

    /// Decodes a generated message struct from a JSON object.
    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    /// Decodes an enum using either its string name or integer representation.
    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.cursor.peek() == Some(b'"') {
            let value = self.cursor.string()?;
            visitor.visit_enum(value.into_owned().into_deserializer())
        } else {
            visitor.visit_enum(self.cursor.number()?.into_deserializer())
        }
    }

    /// Decodes an identifier as a JSON string.
    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    /// Skips one unknown value without allocating.
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.cursor.skip_value(0)?;
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        i8 i16 i32 u8 u16 u32 f32
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonCursor, decode_json_exact, preflight_json_syntax};
    use serde::Deserialize;

    /// Minimal generated-message analogue used to prove exact sequence capacity.
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        /// Repeated field whose capacity must equal its length.
        values: Vec<String>,
    }

    /// Proves look-ahead supplies exact repeated-field capacity.
    #[test]
    fn direct_decoder_never_leaves_spare_sequence_capacity() {
        let fixture: Fixture =
            decode_json_exact(br#"{"values":["a","bb","ccc"]}"#).expect("fixture decodes");
        assert_eq!(fixture.values.len(), fixture.values.capacity());
        assert!(
            fixture
                .values
                .iter()
                .all(|value| value.len() == value.capacity())
        );
    }

    /// Proves malformed escapes and trailing bytes fail in preflight.
    #[test]
    fn syntax_preflight_rejects_malformed_and_trailing_input() {
        assert!(preflight_json_syntax(br#"{"a":"\uD800"}"#).is_err());
        assert!(preflight_json_syntax(br#"{} true"#).is_err());
    }

    /// Proves the fixed syntax stack rejects a 129th nested container.
    #[test]
    fn syntax_preflight_enforces_fixed_depth() {
        let mut input = vec![b'['; 129];
        input.extend(std::iter::repeat_n(b']', 129));
        assert!(preflight_json_syntax(&input).is_err());
    }

    /// Proves escaped ASCII and padded or unpadded encoded bytes decode exactly.
    #[test]
    fn encoded_bytes_accept_json_escapes_and_optional_padding() {
        let mut unpadded = JsonCursor::new(br#""AQI""#);
        let unpadded_bytes = unpadded.base64_bytes().expect("unpadded base64 decodes");
        unpadded.finish().expect("unpadded input is complete");
        assert_eq!(unpadded_bytes, [1, 2]);
        assert_eq!(unpadded_bytes.len(), unpadded_bytes.capacity());

        let mut escaped = JsonCursor::new(br#""AQI\u0044""#);
        let escaped_bytes = escaped.base64_bytes().expect("escaped base64 decodes");
        escaped.finish().expect("escaped input is complete");
        assert_eq!(escaped_bytes, [1, 2, 3]);
        assert_eq!(escaped_bytes.len(), escaped_bytes.capacity());

        let mut hex = JsonCursor::new(br#""0a\u0062B""#);
        let hex_bytes = hex.hex_bytes().expect("escaped hexadecimal ID decodes");
        hex.finish().expect("hex input is complete");
        assert_eq!(hex_bytes, [0x0a, 0xbb]);
        assert_eq!(hex_bytes.len(), hex_bytes.capacity());
    }

    /// Proves encoded-byte validation rejects noncanonical and non-ASCII input.
    #[test]
    fn encoded_bytes_reject_invalid_alphabets_and_padding() {
        for input in [
            br#""A""#.as_slice(),
            br#""AR""#,
            br#""AQ=I""#,
            br#""\u00e9""#,
        ] {
            assert!(JsonCursor::new(input).base64_bytes().is_err());
        }
        assert!(JsonCursor::new(br#""0\u00e9""#).hex_bytes().is_err());
    }
}
