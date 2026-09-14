//! The one canonical OTLP dataset every case in this binary sends and reads.
//!
//! Every signal value in the suite originates here as a plain fixture
//! constant. The OTLP messages are built from those constants, and each test
//! asserts the stored row against the same constants directly. Nothing in this
//! module projects OTLP into canonical columns, restates canonical column
//! order, or normalizes a query result: doing so would let a projection defect
//! agree with a matching test defect and pass.

use arrow::record_batch::RecordBatch;
use wyrd_testing::WyrdTestServer;
use wyrd_tonic::otlp::common::v1::{
    AnyValue, ArrayValue, InstrumentationScope, KeyValue, KeyValueList, any_value,
};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::metrics::v1::{
    Exemplar, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge, Histogram,
    HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary,
    SummaryDataPoint, exemplar, exponential_histogram_data_point, metric, number_data_point,
    summary_data_point,
};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::span::{Event, Link};
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use wyrd_tonic::prost::Message as _;

/// The canonical span ledger every trace case reads.
pub(super) const SPANS_TABLE: &str = "vala.traces.spans";
/// The canonical log ledger every log case reads.
pub(super) const LOGS_TABLE: &str = "vala.logs.records";

/// Identity one copy of the maximal span is exported under.
///
/// The dataset is shared by every transport, so each transport sends it under
/// its own trace and span identity. That keeps the rows distinguishable in one
/// table without changing a single signal value between them, which is what
/// makes "protobuf, JSON and gRPC agree" a comparison of transports rather
/// than of three different payloads.
#[derive(Clone, Copy, Debug)]
pub(super) struct SpanIdentity {
    /// The exported trace identity.
    pub(super) trace_id: [u8; 16],
    /// The exported span identity.
    pub(super) span_id: [u8; 8],
}

/// Identity the OTLP/gRPC copy of the maximal span is exported under.
pub(super) const GRPC_SPAN: SpanIdentity = SpanIdentity {
    trace_id: [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ],
    span_id: [0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28],
};

/// Identity the OTLP/HTTP protobuf copy of the maximal span is exported under.
pub(super) const HTTP_PROTOBUF_SPAN: SpanIdentity = SpanIdentity {
    trace_id: [
        0x02, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ],
    span_id: [0x22, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28],
};

/// Identity the OTLP/HTTP protobuf-JSON copy of the maximal span uses.
pub(super) const HTTP_JSON_SPAN: SpanIdentity = SpanIdentity {
    trace_id: [
        0x03, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ],
    span_id: [0x23, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28],
};
/// Parent of the maximal span, proving a non-null `parent_span_id`.
pub(super) const PARENT_SPAN_ID: [u8; 8] = [0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38];
/// Trace identity the maximal span links to.
pub(super) const LINK_TRACE_ID: [u8; 16] = [
    0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50,
];
/// Span identity the maximal span links to.
pub(super) const LINK_SPAN_ID: [u8; 8] = [0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58];

/// W3C trace state carried by the maximal span.
pub(super) const TRACE_STATE: &str = "wyrd=canonical";
/// W3C trace flags carried by the maximal span (sampled).
pub(super) const SPAN_FLAGS: i64 = 1;
/// Name of the maximal span.
pub(super) const SPAN_NAME: &str = "canonical-otlp-span";
/// OTLP `SpanKind::Client`.
pub(super) const SPAN_KIND: i32 = 3;
/// OTLP `StatusCode::Error`.
pub(super) const STATUS_CODE: i32 = 2;
/// Status message of the maximal span.
pub(super) const STATUS_MESSAGE: &str = "canonical status message";
/// Attributes the sender dropped before the maximal span was exported.
pub(super) const DROPPED_ATTRIBUTES: i64 = 11;
/// Events the sender dropped before the maximal span was exported.
pub(super) const DROPPED_EVENTS: i64 = 12;
/// Links the sender dropped before the maximal span was exported.
pub(super) const DROPPED_LINKS: i64 = 13;

/// Name of the maximal span's single event.
pub(super) const EVENT_NAME: &str = "canonical-checkpoint";
/// Attributes the sender dropped from that event.
pub(super) const EVENT_DROPPED_ATTRIBUTES: i64 = 14;
/// Trace state carried by the maximal span's single link.
pub(super) const LINK_TRACE_STATE: &str = "wyrd=linked";
/// Trace flags carried by that link.
pub(super) const LINK_FLAGS: i64 = 1;
/// Attributes the sender dropped from that link.
pub(super) const LINK_DROPPED_ATTRIBUTES: i64 = 15;

/// `service.name` the resource declares, promoted to its own column.
pub(super) const SERVICE_NAME: &str = "wyrd-otlp-journey";
/// Attributes the sender dropped from the resource.
pub(super) const RESOURCE_DROPPED_ATTRIBUTES: i64 = 16;
/// Schema URL the resource declares.
pub(super) const RESOURCE_SCHEMA_URL: &str = "https://wyrd.test/schemas/resource/1.0.0";
/// Instrumentation scope name the maximal span was recorded under.
pub(super) const SCOPE_NAME: &str = "wyrd.tests.otlp.trace";
/// Instrumentation scope name the maximal log record was recorded under.
pub(super) const LOG_SCOPE_NAME: &str = "wyrd.tests.otlp.log";
/// Instrumentation scope version.
pub(super) const SCOPE_VERSION: &str = "1.2.3";
/// Attributes the sender dropped from the scope.
pub(super) const SCOPE_DROPPED_ATTRIBUTES: i64 = 17;
/// Schema URL the scope declares.
pub(super) const SCOPE_SCHEMA_URL: &str = "https://wyrd.test/schemas/scope/1.0.0";

/// `gen_ai.operation.name`, promoted to its own canonical column.
pub(super) const GEN_AI_OPERATION_NAME: &str = "chat";
/// `gen_ai.provider.name`, promoted to its own canonical column.
pub(super) const GEN_AI_PROVIDER_NAME: &str = "anthropic";
/// `gen_ai.request.model`, promoted to its own canonical column.
pub(super) const GEN_AI_REQUEST_MODEL: &str = "claude-opus-5";
/// `gen_ai.conversation.id`, promoted to its own canonical column.
pub(super) const GEN_AI_CONVERSATION_ID: &str = "conv-canonical-0001";
/// `gen_ai.usage.input_tokens`, promoted to its own canonical column.
pub(super) const GEN_AI_INPUT_TOKENS: i64 = 4_096;
/// `gen_ai.usage.output_tokens`, promoted to its own canonical column.
pub(super) const GEN_AI_OUTPUT_TOKENS: i64 = 512;
/// Structured `gen_ai.input.messages` the caller sent, as its JSON encoding.
///
/// The semantic convention carries the message list as one structured value
/// and the canonical ledger declares no promoted column for it, so it stays
/// inside the sensitive `attributes` blob and is gated with it.
pub(super) const GEN_AI_INPUT_MESSAGES: &str =
    r#"[{"role":"user","parts":[{"type":"text","content":"summarize the canonical ledger"}]}]"#;
/// Structured `gen_ai.output.messages` the caller sent, as its JSON encoding.
pub(super) const GEN_AI_OUTPUT_MESSAGES: &str = r#"[{"role":"assistant","parts":[{"type":"text","content":"the ledger is canonical"}],"finish_reason":"stop"}]"#;

/// How long after its start the maximal span ends, in nanoseconds.
pub(super) const SPAN_DURATION_NANOS: i64 = 5_000_000;
/// How long after its span's start the single event was recorded.
pub(super) const EVENT_OFFSET_NANOS: i64 = 1_000_000;

/// `service.name` every stock upstream exporter in this suite declares.
///
/// Distinct from [`SERVICE_NAME`] so a stock-exporter journey and the
/// hand-built protocol dataset can never read each other's rows.
pub(super) const STOCK_SERVICE_NAME: &str = "wyrd-rust-journey";
/// Instrumentation scope the stock upstream tracer records under.
pub(super) const STOCK_TRACE_SCOPE: &str = "wyrd.tests.stock.trace";
/// Instrumentation scope the stock upstream logger records under.
pub(super) const STOCK_LOG_SCOPE: &str = "wyrd.tests.stock.log";
/// Instrumentation scope the stock upstream meter records under.
pub(super) const STOCK_METRIC_SCOPE: &str = "wyrd.tests.stock.metric";

/// The process resource every stock upstream exporter is configured with.
///
/// `builder_empty` rather than `builder` so the resource carries exactly the
/// one attribute the assertions name, instead of whatever the SDK's default
/// detectors happen to find on the machine running the lane.
pub(super) fn stock_resource() -> opentelemetry_sdk::Resource {
    opentelemetry_sdk::Resource::builder_empty()
        .with_attributes([opentelemetry::KeyValue::new(
            "service.name",
            STOCK_SERVICE_NAME,
        )])
        .build()
}

/// The instant the fixture anchors every signal timestamp to.
///
/// Scribe admits rows inside a window around now, so the dataset is anchored
/// to the current wall clock rather than a frozen literal that would age out
/// of the accepted range. Every derived timestamp in one case is computed from
/// a single call so the whole dataset names one instant.
///
/// # Panics
///
/// Panics when the current time does not fit in nanoseconds, which cannot
/// happen before the year 2262.
pub(super) fn anchor_nanos() -> i64 {
    chrono::Utc::now()
        .timestamp_nanos_opt()
        .expect("the current instant fits in nanoseconds")
}

/// Builds one OTLP string attribute.
pub(super) fn string_attribute(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_owned())),
        }),
    }
}

/// Builds one OTLP signed-integer attribute.
pub(super) fn int_attribute(key: &str, value: i64) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::IntValue(value)),
        }),
    }
}

/// Builds one OTLP boolean attribute.
pub(super) fn bool_attribute(key: &str, value: bool) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::BoolValue(value)),
        }),
    }
}

/// Builds one OTLP double attribute.
pub(super) fn double_attribute(key: &str, value: f64) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::DoubleValue(value)),
        }),
    }
}

/// Builds one OTLP array-of-int attribute.
pub(super) fn int_array_attribute(key: &str, values: &[i64]) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::ArrayValue(ArrayValue {
                values: values
                    .iter()
                    .map(|value| AnyValue {
                        value: Some(any_value::Value::IntValue(*value)),
                    })
                    .collect(),
            })),
        }),
    }
}

/// Builds one OTLP key-value-list attribute.
pub(super) fn map_attribute(key: &str, entries: Vec<KeyValue>) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::KvlistValue(KeyValueList {
                values: entries,
            })),
        }),
    }
}

/// The maximal span's own attributes, in the order the exporter sends them.
///
/// The collection deliberately mixes every `AnyValue` shape the canonical
/// encoding must retain with the six pinned `GenAI` promotions and the two
/// structured `GenAI` message payloads, so one span exercises the opaque
/// attribute blob, every promoted column, and the payload gate together.
pub(super) fn span_attributes() -> Vec<KeyValue> {
    vec![
        string_attribute("wyrd.test.marker", "canonical-trace"),
        int_attribute("retry.count", 3),
        bool_attribute("cache.hit", true),
        double_attribute("sample.ratio", 0.25),
        int_array_attribute("test.values", &[1, 2, 3]),
        map_attribute(
            "test.nested",
            vec![
                string_attribute("inner", "value"),
                int_attribute("depth", 2),
            ],
        ),
        string_attribute("gen_ai.operation.name", GEN_AI_OPERATION_NAME),
        string_attribute("gen_ai.provider.name", GEN_AI_PROVIDER_NAME),
        string_attribute("gen_ai.request.model", GEN_AI_REQUEST_MODEL),
        string_attribute("gen_ai.conversation.id", GEN_AI_CONVERSATION_ID),
        int_attribute("gen_ai.usage.input_tokens", GEN_AI_INPUT_TOKENS),
        int_attribute("gen_ai.usage.output_tokens", GEN_AI_OUTPUT_TOKENS),
        string_attribute("gen_ai.input.messages", GEN_AI_INPUT_MESSAGES),
        string_attribute("gen_ai.output.messages", GEN_AI_OUTPUT_MESSAGES),
    ]
}

/// Attributes of the maximal span's single event.
pub(super) fn event_attributes() -> Vec<KeyValue> {
    vec![
        int_attribute("step", 1),
        string_attribute("phase", "commit"),
    ]
}

/// Attributes of the maximal span's single link.
pub(super) fn link_attributes() -> Vec<KeyValue> {
    vec![string_attribute("link.reason", "follows-from")]
}

/// Attributes the shared resource declares.
pub(super) fn resource_attributes() -> Vec<KeyValue> {
    vec![
        string_attribute("service.name", SERVICE_NAME),
        string_attribute("deployment.environment", "journey"),
    ]
}

/// Attributes the shared instrumentation scope declares.
pub(super) fn scope_attributes() -> Vec<KeyValue> {
    vec![string_attribute("scope.owner", "wyrd-otlp-journey")]
}

/// The shared OTLP resource every signal in the dataset is exported under.
pub(super) fn resource() -> Resource {
    Resource {
        attributes: resource_attributes(),
        dropped_attributes_count: u32::try_from(RESOURCE_DROPPED_ATTRIBUTES)
            .expect("the fixture dropped count fits u32"),
        entity_refs: Vec::new(),
    }
}

/// The instrumentation scope one signal is exported under.
///
/// Every signal shares the version, attributes and dropped count so a scope
/// column can only differ between signals by the name the emitter declared.
pub(super) fn signal_scope(name: &str) -> InstrumentationScope {
    InstrumentationScope {
        name: name.to_owned(),
        version: SCOPE_VERSION.to_owned(),
        attributes: scope_attributes(),
        dropped_attributes_count: u32::try_from(SCOPE_DROPPED_ATTRIBUTES)
            .expect("the fixture dropped count fits u32"),
    }
}

/// Builds the one maximal span anchored at `start` under `identity`.
///
/// Every optional OTLP field the canonical ledger declares is populated, so a
/// column that silently stops being written fails a comparison rather than
/// matching an absent fixture value.
pub(super) fn maximal_span(start: i64, identity: SpanIdentity) -> Span {
    Span {
        trace_id: identity.trace_id.to_vec(),
        span_id: identity.span_id.to_vec(),
        trace_state: TRACE_STATE.to_owned(),
        parent_span_id: PARENT_SPAN_ID.to_vec(),
        flags: u32::try_from(SPAN_FLAGS).expect("the fixture flags fit u32"),
        name: SPAN_NAME.to_owned(),
        kind: SPAN_KIND,
        start_time_unix_nano: u64::try_from(start).expect("the anchor instant is positive"),
        end_time_unix_nano: u64::try_from(start + SPAN_DURATION_NANOS)
            .expect("the anchor instant is positive"),
        attributes: span_attributes(),
        dropped_attributes_count: u32::try_from(DROPPED_ATTRIBUTES)
            .expect("the fixture dropped count fits u32"),
        events: vec![Event {
            time_unix_nano: u64::try_from(start + EVENT_OFFSET_NANOS)
                .expect("the anchor instant is positive"),
            name: EVENT_NAME.to_owned(),
            attributes: event_attributes(),
            dropped_attributes_count: u32::try_from(EVENT_DROPPED_ATTRIBUTES)
                .expect("the fixture dropped count fits u32"),
        }],
        dropped_events_count: u32::try_from(DROPPED_EVENTS)
            .expect("the fixture dropped count fits u32"),
        links: vec![Link {
            trace_id: LINK_TRACE_ID.to_vec(),
            span_id: LINK_SPAN_ID.to_vec(),
            trace_state: LINK_TRACE_STATE.to_owned(),
            attributes: link_attributes(),
            dropped_attributes_count: u32::try_from(LINK_DROPPED_ATTRIBUTES)
                .expect("the fixture dropped count fits u32"),
            flags: u32::try_from(LINK_FLAGS).expect("the fixture flags fit u32"),
        }],
        dropped_links_count: u32::try_from(DROPPED_LINKS)
            .expect("the fixture dropped count fits u32"),
        status: Some(Status {
            message: STATUS_MESSAGE.to_owned(),
            code: STATUS_CODE,
        }),
    }
}

/// Wraps one identity's maximal span in the shared resource and scope envelope.
pub(super) fn maximal_resource_spans(start: i64, identity: SpanIdentity) -> Vec<ResourceSpans> {
    vec![ResourceSpans {
        resource: Some(resource()),
        scope_spans: vec![ScopeSpans {
            scope: Some(signal_scope(SCOPE_NAME)),
            spans: vec![maximal_span(start, identity)],
            schema_url: SCOPE_SCHEMA_URL.to_owned(),
        }],
        schema_url: RESOURCE_SCHEMA_URL.to_owned(),
    }]
}

/// Nanoseconds after the anchor at which the maximal log record was observed.
pub(super) const LOG_OBSERVED_OFFSET_NANOS: i64 = 2_000_000;
/// OTLP severity number of the maximal log record (`WARN`).
pub(super) const LOG_SEVERITY_NUMBER: i32 = 13;
/// OTLP severity text of the maximal log record.
pub(super) const LOG_SEVERITY_TEXT: &str = "WARN";
/// `OTel` event name the maximal log record declares.
pub(super) const LOG_EVENT_NAME: &str = "canonical.order.delayed";
/// Body text of the maximal log record.
pub(super) const LOG_BODY_TEXT: &str = "order delayed by canonical journey";
/// W3C trace flags carried by the maximal log record.
pub(super) const LOG_FLAGS: i64 = 1;
/// Attributes the sender dropped from the maximal log record.
pub(super) const LOG_DROPPED_ATTRIBUTES: i64 = 18;

/// Attributes the maximal log record carries.
pub(super) fn log_attributes() -> Vec<KeyValue> {
    vec![
        string_attribute("wyrd.test.marker", "canonical-log"),
        int_attribute("order.id", 4_242),
        bool_attribute("order.expedited", false),
    ]
}

/// The body of the maximal log record, as the exporter sends it.
pub(super) fn log_body() -> AnyValue {
    AnyValue {
        value: Some(any_value::Value::StringValue(LOG_BODY_TEXT.to_owned())),
    }
}

/// Builds the one maximal log record anchored at `time`.
///
/// The record correlates with the trace dataset's gRPC span so the canonical
/// log columns carry a real trace/span context rather than the protocol's
/// permitted absent one.
pub(super) fn maximal_log_record(time: i64) -> LogRecord {
    LogRecord {
        time_unix_nano: u64::try_from(time).expect("the anchor instant is positive"),
        observed_time_unix_nano: u64::try_from(time + LOG_OBSERVED_OFFSET_NANOS)
            .expect("the anchor instant is positive"),
        severity_number: LOG_SEVERITY_NUMBER,
        severity_text: LOG_SEVERITY_TEXT.to_owned(),
        event_name: LOG_EVENT_NAME.to_owned(),
        body: Some(log_body()),
        attributes: log_attributes(),
        dropped_attributes_count: u32::try_from(LOG_DROPPED_ATTRIBUTES)
            .expect("the fixture dropped count fits u32"),
        flags: u32::try_from(LOG_FLAGS).expect("the fixture flags fit u32"),
        trace_id: GRPC_SPAN.trace_id.to_vec(),
        span_id: GRPC_SPAN.span_id.to_vec(),
    }
}

/// Wraps the maximal log record in the shared resource and scope envelope.
pub(super) fn maximal_resource_logs(time: i64) -> Vec<ResourceLogs> {
    vec![ResourceLogs {
        resource: Some(resource()),
        scope_logs: vec![ScopeLogs {
            scope: Some(signal_scope(LOG_SCOPE_NAME)),
            log_records: vec![maximal_log_record(time)],
            schema_url: SCOPE_SCHEMA_URL.to_owned(),
        }],
        schema_url: RESOURCE_SCHEMA_URL.to_owned(),
    }]
}

/// The canonical metric ledger every metric case reads.
pub(super) const METRICS_TABLE: &str = "vala.metrics.points";
/// Instrumentation scope name carried by the metric envelope.
pub(super) const METRIC_SCOPE_NAME: &str = "wyrd.tests.otlp.metric";
/// Distance the metric points' start instant precedes their observation.
pub(super) const METRIC_START_OFFSET_NANOS: i64 = 1_000_000;
/// Offset of the exemplar observation from its owning point.
pub(super) const METRIC_EXEMPLAR_OFFSET_NANOS: i64 = 500_000;
/// Data-point flags carried by every fixture point.
pub(super) const METRIC_FLAGS: i64 = 1;
/// Description shared by every fixture metric.
pub(super) const METRIC_DESCRIPTION: &str = "canonical journey metric";
/// Unit shared by every fixture metric.
pub(super) const METRIC_UNIT: &str = "ms";
/// Aggregation temporality every kind that owns one declares (cumulative).
pub(super) const METRIC_TEMPORALITY: i32 = 2;

/// Name of the integer gauge point.
pub(super) const GAUGE_INT_METRIC: &str = "canonical.gauge.int";
/// Name of the double gauge point.
pub(super) const GAUGE_DOUBLE_METRIC: &str = "canonical.gauge.double";
/// Name of the integer sum point.
pub(super) const SUM_INT_METRIC: &str = "canonical.sum.int";
/// Name of the double sum point.
pub(super) const SUM_DOUBLE_METRIC: &str = "canonical.sum.double";
/// Name of the explicit-bucket histogram point.
pub(super) const HISTOGRAM_METRIC: &str = "canonical.histogram";
/// Name of the exponential histogram point.
pub(super) const EXPONENTIAL_HISTOGRAM_METRIC: &str = "canonical.exponential_histogram";
/// Name of the summary point.
pub(super) const SUMMARY_METRIC: &str = "canonical.summary";

/// Value of the integer gauge point, distinct from every double alternative.
pub(super) const GAUGE_INT_VALUE: i64 = 42;
/// Value of the double gauge point, carrying a fraction an integer cannot hold.
pub(super) const GAUGE_DOUBLE_VALUE: f64 = 1.5;
/// Value of the integer sum point.
pub(super) const SUM_INT_VALUE: i64 = 7;
/// Value of the double sum point.
pub(super) const SUM_DOUBLE_VALUE: f64 = 2.25;
/// Monotonicity both sum points declare.
pub(super) const SUM_IS_MONOTONIC: bool = true;

/// Total observations in the explicit-bucket histogram point.
pub(super) const HISTOGRAM_COUNT: i64 = 6;
/// Sum of the explicit-bucket histogram point.
pub(super) const HISTOGRAM_SUM: f64 = 12.5;
/// Smallest observation in the explicit-bucket histogram point.
pub(super) const HISTOGRAM_MIN: f64 = 0.5;
/// Largest observation in the explicit-bucket histogram point.
pub(super) const HISTOGRAM_MAX: f64 = 9.0;
/// Bucket populations of the explicit-bucket histogram, summing to its count.
pub(super) const HISTOGRAM_BUCKET_COUNTS: [i64; 3] = [1, 2, 3];
/// Strictly increasing bounds of the explicit-bucket histogram.
pub(super) const HISTOGRAM_EXPLICIT_BOUNDS: [f64; 2] = [1.0, 5.0];

/// Total observations in the exponential histogram point.
pub(super) const EXPONENTIAL_COUNT: i64 = 4;
/// Sum of the exponential histogram point.
pub(super) const EXPONENTIAL_SUM: f64 = 8.0;
/// Smallest observation in the exponential histogram point.
pub(super) const EXPONENTIAL_MIN: f64 = 0.25;
/// Largest observation in the exponential histogram point.
pub(super) const EXPONENTIAL_MAX: f64 = 4.0;
/// Resolution scale of the exponential histogram point.
pub(super) const EXPONENTIAL_SCALE: i32 = 2;
/// Zero-bucket population of the exponential histogram point.
pub(super) const EXPONENTIAL_ZERO_COUNT: i64 = 1;
/// Zero-bucket width of the exponential histogram point.
pub(super) const EXPONENTIAL_ZERO_THRESHOLD: f64 = 0.5;
/// Signed index of the first populated positive exponential bucket.
pub(super) const EXPONENTIAL_POSITIVE_OFFSET: i32 = -1;
/// Populations of the positive exponential buckets.
pub(super) const EXPONENTIAL_POSITIVE_COUNTS: [i64; 2] = [1, 2];
/// Signed index of the first populated negative exponential bucket.
pub(super) const EXPONENTIAL_NEGATIVE_OFFSET: i32 = 3;
/// Populations of the negative exponential buckets.
pub(super) const EXPONENTIAL_NEGATIVE_COUNTS: [i64; 1] = [1];

/// Total observations in the summary point.
pub(super) const SUMMARY_COUNT: i64 = 3;
/// Sum of the summary point.
pub(super) const SUMMARY_SUM: f64 = 6.0;
/// Ordered quantiles of the summary point, spanning the unit interval.
pub(super) const SUMMARY_QUANTILES: [(f64, f64); 2] = [(0.5, 1.0), (0.99, 5.0)];

/// Integer value carried by the fixture exemplar.
pub(super) const EXEMPLAR_INT_VALUE: i64 = 3;

/// Metric-level metadata, kept distinct from the point attributes.
pub(super) fn metric_metadata() -> Vec<KeyValue> {
    vec![string_attribute("wyrd.metric.metadata", "canonical")]
}

/// Attributes every fixture data point carries.
pub(super) fn point_attributes() -> Vec<KeyValue> {
    vec![
        string_attribute("wyrd.point.kind", "canonical"),
        int_attribute("wyrd.point.cardinality", 3),
        bool_attribute("wyrd.point.sampled", true),
        double_attribute("wyrd.point.ratio", 0.25),
    ]
}

/// Attributes the fixture exemplar retains after filtering.
pub(super) fn exemplar_attributes() -> Vec<KeyValue> {
    vec![string_attribute("wyrd.exemplar.origin", "canonical")]
}

/// The single exemplar the integer gauge point carries.
///
/// Exemplars correlate a point back to a span, so the fixture points at the
/// same span the trace and log cases store: a stored exemplar that names no
/// real span proves the column survived without proving the correlation did.
pub(super) fn maximal_exemplar(time: i64) -> Exemplar {
    Exemplar {
        filtered_attributes: exemplar_attributes(),
        time_unix_nano: u64::try_from(time + METRIC_EXEMPLAR_OFFSET_NANOS)
            .expect("the anchor instant is positive"),
        span_id: GRPC_SPAN.span_id.to_vec(),
        trace_id: GRPC_SPAN.trace_id.to_vec(),
        value: Some(exemplar::Value::AsInt(EXEMPLAR_INT_VALUE)),
    }
}

/// Builds one numeric data point, optionally carrying the fixture exemplar.
fn number_point(
    time: i64,
    value: number_data_point::Value,
    exemplars: Vec<Exemplar>,
) -> NumberDataPoint {
    NumberDataPoint {
        attributes: point_attributes(),
        start_time_unix_nano: u64::try_from(time - METRIC_START_OFFSET_NANOS)
            .expect("the anchor instant is positive"),
        time_unix_nano: u64::try_from(time).expect("the anchor instant is positive"),
        exemplars,
        flags: u32::try_from(METRIC_FLAGS).expect("the fixture flags fit u32"),
        value: Some(value),
    }
}

/// Builds one fixture metric from its descriptor fields and data collection.
fn fixture_metric(name: &str, data: metric::Data) -> Metric {
    Metric {
        name: name.to_owned(),
        description: METRIC_DESCRIPTION.to_owned(),
        unit: METRIC_UNIT.to_owned(),
        metadata: metric_metadata(),
        data: Some(data),
    }
}

/// Every supported point kind, each as one metric carrying one point.
///
/// Integer and double alternatives are separate metrics rather than one metric
/// with two points, so a projector that collapsed the two numeric columns onto
/// one would be visible as a single row's wrong column rather than hidden in a
/// pair of rows that happen to differ.
pub(super) fn maximal_metrics(time: i64) -> Vec<Metric> {
    let start =
        u64::try_from(time - METRIC_START_OFFSET_NANOS).expect("the anchor instant is positive");
    let observed = u64::try_from(time).expect("the anchor instant is positive");
    let flags = u32::try_from(METRIC_FLAGS).expect("the fixture flags fit u32");
    vec![
        fixture_metric(
            GAUGE_INT_METRIC,
            metric::Data::Gauge(Gauge {
                data_points: vec![number_point(
                    time,
                    number_data_point::Value::AsInt(GAUGE_INT_VALUE),
                    vec![maximal_exemplar(time)],
                )],
            }),
        ),
        fixture_metric(
            GAUGE_DOUBLE_METRIC,
            metric::Data::Gauge(Gauge {
                data_points: vec![number_point(
                    time,
                    number_data_point::Value::AsDouble(GAUGE_DOUBLE_VALUE),
                    Vec::new(),
                )],
            }),
        ),
        fixture_metric(
            SUM_INT_METRIC,
            metric::Data::Sum(Sum {
                data_points: vec![number_point(
                    time,
                    number_data_point::Value::AsInt(SUM_INT_VALUE),
                    Vec::new(),
                )],
                aggregation_temporality: METRIC_TEMPORALITY,
                is_monotonic: SUM_IS_MONOTONIC,
            }),
        ),
        fixture_metric(
            SUM_DOUBLE_METRIC,
            metric::Data::Sum(Sum {
                data_points: vec![number_point(
                    time,
                    number_data_point::Value::AsDouble(SUM_DOUBLE_VALUE),
                    Vec::new(),
                )],
                aggregation_temporality: METRIC_TEMPORALITY,
                is_monotonic: SUM_IS_MONOTONIC,
            }),
        ),
        fixture_metric(
            HISTOGRAM_METRIC,
            metric::Data::Histogram(Histogram {
                data_points: vec![HistogramDataPoint {
                    attributes: point_attributes(),
                    start_time_unix_nano: start,
                    time_unix_nano: observed,
                    count: u64::try_from(HISTOGRAM_COUNT).expect("the fixture count is positive"),
                    sum: Some(HISTOGRAM_SUM),
                    bucket_counts: HISTOGRAM_BUCKET_COUNTS
                        .iter()
                        .map(|count| {
                            u64::try_from(*count).expect("the fixture counts are positive")
                        })
                        .collect(),
                    explicit_bounds: HISTOGRAM_EXPLICIT_BOUNDS.to_vec(),
                    exemplars: Vec::new(),
                    flags,
                    min: Some(HISTOGRAM_MIN),
                    max: Some(HISTOGRAM_MAX),
                }],
                aggregation_temporality: METRIC_TEMPORALITY,
            }),
        ),
        fixture_metric(
            EXPONENTIAL_HISTOGRAM_METRIC,
            metric::Data::ExponentialHistogram(ExponentialHistogram {
                data_points: vec![ExponentialHistogramDataPoint {
                    attributes: point_attributes(),
                    start_time_unix_nano: start,
                    time_unix_nano: observed,
                    count: u64::try_from(EXPONENTIAL_COUNT).expect("the fixture count is positive"),
                    sum: Some(EXPONENTIAL_SUM),
                    scale: EXPONENTIAL_SCALE,
                    zero_count: u64::try_from(EXPONENTIAL_ZERO_COUNT)
                        .expect("the fixture count is positive"),
                    positive: Some(exponential_histogram_data_point::Buckets {
                        offset: EXPONENTIAL_POSITIVE_OFFSET,
                        bucket_counts: EXPONENTIAL_POSITIVE_COUNTS
                            .iter()
                            .map(|count| {
                                u64::try_from(*count).expect("the fixture counts are positive")
                            })
                            .collect(),
                    }),
                    negative: Some(exponential_histogram_data_point::Buckets {
                        offset: EXPONENTIAL_NEGATIVE_OFFSET,
                        bucket_counts: EXPONENTIAL_NEGATIVE_COUNTS
                            .iter()
                            .map(|count| {
                                u64::try_from(*count).expect("the fixture counts are positive")
                            })
                            .collect(),
                    }),
                    flags,
                    exemplars: Vec::new(),
                    min: Some(EXPONENTIAL_MIN),
                    max: Some(EXPONENTIAL_MAX),
                    zero_threshold: EXPONENTIAL_ZERO_THRESHOLD,
                }],
                aggregation_temporality: METRIC_TEMPORALITY,
            }),
        ),
        fixture_metric(
            SUMMARY_METRIC,
            metric::Data::Summary(Summary {
                data_points: vec![SummaryDataPoint {
                    attributes: point_attributes(),
                    start_time_unix_nano: start,
                    time_unix_nano: observed,
                    count: u64::try_from(SUMMARY_COUNT).expect("the fixture count is positive"),
                    sum: SUMMARY_SUM,
                    quantile_values: SUMMARY_QUANTILES
                        .iter()
                        .map(|(quantile, value)| summary_data_point::ValueAtQuantile {
                            quantile: *quantile,
                            value: *value,
                        })
                        .collect(),
                    flags,
                }],
            }),
        ),
    ]
}

/// Wraps every supported point kind in the shared resource and scope envelope.
pub(super) fn maximal_resource_metrics(time: i64) -> Vec<ResourceMetrics> {
    vec![ResourceMetrics {
        resource: Some(resource()),
        scope_metrics: vec![ScopeMetrics {
            scope: Some(signal_scope(METRIC_SCOPE_NAME)),
            metrics: maximal_metrics(time),
            schema_url: SCOPE_SCHEMA_URL.to_owned(),
        }],
        schema_url: RESOURCE_SCHEMA_URL.to_owned(),
    }]
}

/// One bound public journey: a real server, an admin bearer, and a real client.
///
/// Bound rather than in-process because every case drives a real OTLP
/// transport and the public query route, both of which need real endpoints.
pub(super) struct OtlpJourney {
    /// The running server under test.
    server: WyrdTestServer,
    /// A bearer minted from the fixture's own admin API key.
    token: String,
    /// The public SDK client the readback runs through.
    client: wyrd_client::WyrdClient,
}

impl OtlpJourney {
    /// Starts one bound server and mints the credentials the journey uses.
    ///
    /// # Panics
    ///
    /// Panics when the server does not start, does not bind, or cannot
    /// bootstrap the journey's own service principal.
    pub(super) async fn start() -> Self {
        let server = WyrdTestServer::start_bound()
            .await
            .expect("the OTLP journey harness starts");
        let bootstrap = server
            .bootstrap_service("otlp-journey", &["admin"])
            .await
            .expect("the journey bootstraps its own service principal");
        let api_key = bootstrap
            .api_key()
            .expect("the bootstrapped service carries an API key")
            .clone();
        let token = server
            .exchange_api_key(&api_key)
            .await
            .expect("the journey exchanges its API key for a bearer");
        let client = wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: server.grpc_url().expect("the harness binds gRPC"),
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            http: wyrd_client::transport::HttpConfig {
                base_url: server
                    .base_url()
                    .expect("the harness binds HTTP")
                    .to_owned(),
                ..wyrd_client::transport::HttpConfig::default()
            },
            credential: Some(api_key),
            ..wyrd_client::config::ClientConfig::default()
        })
        .expect("the journey builds its public SDK client");
        Self {
            server,
            token,
            client,
        }
    }

    /// The bearer an OTLP exporter sends in `x-wyrd-access-token`.
    pub(super) fn token(&self) -> &str {
        &self.token
    }

    /// The bound gRPC endpoint an OTLP/gRPC exporter dials.
    ///
    /// # Panics
    ///
    /// Panics when the harness did not bind a gRPC listener.
    pub(super) fn grpc_url(&self) -> String {
        self.server.grpc_url().expect("the harness binds gRPC")
    }

    /// The gRPC metadata a stock upstream OTLP exporter authenticates with.
    ///
    /// Upstream exporters take arbitrary metadata, which is the whole reason
    /// an unmodified SDK can talk to Wyrd: the bearer rides in the same
    /// `x-wyrd-access-token` header every other Wyrd caller uses.
    ///
    /// # Panics
    ///
    /// Panics when the minted bearer is not valid ASCII metadata.
    pub(super) fn stock_metadata(&self) -> wyrd_tonic::tonic::metadata::MetadataMap {
        let mut metadata = wyrd_tonic::tonic::metadata::MetadataMap::new();
        metadata.insert(
            "x-wyrd-access-token",
            format!("Bearer {}", self.token)
                .parse()
                .expect("the minted bearer is valid ASCII metadata"),
        );
        metadata
    }

    /// Mints a bearer whose principal holds exactly `permissions`.
    ///
    /// The journey's own principal is an admin, so proving that ingest is
    /// permission-gated needs a caller that is authenticated and permitted to
    /// do something but deliberately not permitted to write records. No
    /// builtin role has that shape, so the role is seeded for the fixture.
    ///
    /// # Panics
    ///
    /// Panics when the role cannot be seeded, the service cannot be
    /// bootstrapped, or its API key cannot be exchanged for a bearer.
    pub(super) async fn token_with_permissions(
        &self,
        name: &str,
        permissions: &[wyrd_runtime::Permission],
    ) -> String {
        self.server
            .seed_role(name, permissions)
            .await
            .expect("the fixture role is seeded");
        let bootstrap = self
            .server
            .bootstrap_service(name, &[name])
            .await
            .expect("the fixture service is bootstrapped onto its role");
        let api_key = bootstrap
            .api_key()
            .expect("the bootstrapped service carries an API key")
            .clone();
        self.server
            .exchange_api_key(&api_key)
            .await
            .expect("the fixture exchanges its API key for a bearer")
    }

    /// The running harness, for the cases that inject a durable fault.
    pub(super) fn server(&self) -> &WyrdTestServer {
        &self.server
    }

    /// The bound HTTP base URL an OTLP/HTTP exporter posts to.
    ///
    /// # Panics
    ///
    /// Panics when the harness did not bind an HTTP listener.
    pub(super) fn base_url(&self) -> &str {
        self.server.base_url().expect("the harness binds HTTP")
    }

    /// Builds a second public client whose principal holds exactly `permissions`.
    ///
    /// The journey's own principal is an admin, so it can read everything; a
    /// payload-authorization assertion needs a caller that is authenticated
    /// and permitted to query but deliberately not permitted to read a
    /// sensitive column. No builtin role has that shape, so the role is seeded
    /// for this fixture and the service is bootstrapped onto it.
    ///
    /// # Panics
    ///
    /// Panics when the role cannot be seeded, the service cannot be
    /// bootstrapped, or the client cannot be built.
    pub(super) async fn client_with_permissions(
        &self,
        name: &str,
        permissions: &[wyrd_runtime::Permission],
    ) -> wyrd_client::WyrdClient {
        self.server
            .seed_role(name, permissions)
            .await
            .expect("the fixture role is seeded");
        let bootstrap = self
            .server
            .bootstrap_service(name, &[name])
            .await
            .expect("the fixture service is bootstrapped onto its role");
        let api_key = bootstrap
            .api_key()
            .expect("the bootstrapped service carries an API key")
            .clone();
        wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: self.grpc_url(),
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            http: wyrd_client::transport::HttpConfig {
                base_url: self.base_url().to_owned(),
                ..wyrd_client::transport::HttpConfig::default()
            },
            credential: Some(api_key),
            ..wyrd_client::config::ClientConfig::default()
        })
        .expect("the fixture builds its restricted SDK client")
    }

    /// Crosses the publication boundary so published readers see the rows.
    ///
    /// # Panics
    ///
    /// Panics when publication does not complete.
    pub(super) async fn publish(&self) {
        self.server
            .flush_bifrost()
            .await
            .expect("the acknowledged rows publish");
    }

    /// Runs one strict fused public query and returns its batches.
    ///
    /// Strict freshness and fused visibility make the read an authority check:
    /// the answer must come from whichever source owns the rows now, not from
    /// whichever source is cheapest.
    ///
    /// # Panics
    ///
    /// Panics when the query does not start or does not stream to completion.
    pub(super) async fn query(&self, sql: &str) -> Vec<RecordBatch> {
        self.query_as(&self.client, sql).await
    }

    /// Runs one strict fused public query through a caller-supplied client.
    ///
    /// # Panics
    ///
    /// Panics when the query does not start or does not stream to completion.
    pub(super) async fn query_as(
        &self,
        client: &wyrd_client::WyrdClient,
        sql: &str,
    ) -> Vec<RecordBatch> {
        let mut stream = wyrd_client::Bifrost::query_only(client)
            .query(&wyrd_spec::vala::api::BifrostQueryRequest {
                sql: sql.to_owned(),
                visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
                freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                deadline_ms: Some(120_000),
            })
            .await
            .unwrap_or_else(|error| panic!("public query `{sql}` starts: {error}"));
        let mut batches = Vec::new();
        while let Some(batch) = stream
            .next_batch()
            .await
            .unwrap_or_else(|error| panic!("public query `{sql}` streams: {error}"))
        {
            batches.push(batch);
        }
        batches
    }

    /// Runs one strict fused public query, returning its stable refusal code.
    ///
    /// A negative journey has to distinguish "the table holds no matching row"
    /// from "no accepted record ever materialized this table"; both are proof
    /// that nothing was committed, so the caller needs the refusal rather than
    /// a panic.
    ///
    /// # Errors
    ///
    /// Returns the stable error code when the query is refused before its
    /// stream opens.
    ///
    /// # Panics
    ///
    /// Panics when an accepted query does not stream to completion.
    pub(super) async fn try_query(&self, sql: &str) -> Result<Vec<RecordBatch>, String> {
        let mut stream = match wyrd_client::Bifrost::query_only(&self.client)
            .query(&wyrd_spec::vala::api::BifrostQueryRequest {
                sql: sql.to_owned(),
                visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
                freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                deadline_ms: Some(120_000),
            })
            .await
        {
            Ok(stream) => stream,
            Err(wyrd_client::bifrost::BifrostClientError::Transport(error)) => {
                return Err(error.code().to_owned());
            }
            Err(other) => panic!("public query `{sql}` fails outside the stable contract: {other}"),
        };
        let mut batches = Vec::new();
        while let Some(batch) = stream
            .next_batch()
            .await
            .unwrap_or_else(|error| panic!("public query `{sql}` streams: {error}"))
        {
            batches.push(batch);
        }
        Ok(batches)
    }

    /// Runs one strict fused public query and returns its single row.
    ///
    /// # Panics
    ///
    /// Panics when the query returns anything other than exactly one row.
    pub(super) async fn query_one_row(&self, sql: &str) -> RecordBatch {
        let batches = self.query(sql).await;
        let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(rows, 1, "`{sql}` must return exactly one row, got {rows}");
        batches
            .into_iter()
            .find(|batch| batch.num_rows() == 1)
            .expect("the single row is carried by one batch")
    }

    /// Shuts the harness down and fails the case when it does not drain.
    ///
    /// # Panics
    ///
    /// Panics when the server does not shut down cleanly.
    pub(super) async fn shutdown(self) {
        self.server
            .shutdown()
            .await
            .expect("the OTLP journey harness drains cleanly");
    }
}

/// Finds the single stored row carrying `span_id` among the queried batches.
///
/// The shared dataset is exported once per transport under its own identity
/// and every other value — the anchor instant included — is deliberately the
/// same, so identity is the only thing a reader can select on.
///
/// # Panics
///
/// Panics when no row or more than one row carries the requested identity.
pub(super) fn row_by_span_id(batches: &[RecordBatch], span_id: [u8; 8]) -> RecordBatch {
    let mut found: Option<RecordBatch> = None;
    for batch in batches {
        let ids = column::<arrow::array::FixedSizeBinaryArray>(batch, "span_id");
        for index in 0..batch.num_rows() {
            if ids.value(index) == span_id {
                assert!(
                    found.is_none(),
                    "span {span_id:02x?} is stored exactly once"
                );
                found = Some(batch.slice(index, 1));
            }
        }
    }
    found.unwrap_or_else(|| panic!("span {span_id:02x?} is stored"))
}

/// Finds the single stored row whose `column` holds `value`.
///
/// # Panics
///
/// Panics when no row or more than one row carries the requested value.
pub(super) fn row_by_string(batches: &[RecordBatch], name: &str, value: &str) -> RecordBatch {
    let mut found: Option<RecordBatch> = None;
    for batch in batches {
        let values = column::<arrow::array::StringArray>(batch, name);
        for index in 0..batch.num_rows() {
            if values.value(index) == value {
                assert!(found.is_none(), "`{name} = {value}` is stored exactly once");
                found = Some(batch.slice(index, 1));
            }
        }
    }
    found.unwrap_or_else(|| panic!("`{name} = {value}` is stored"))
}

/// Reads one non-null column value out of a single-row batch.
///
/// # Panics
///
/// Panics when the column is missing, is not of the requested Arrow type, or
/// carries a null where the canonical ledger declares a value.
pub(super) fn column<'batch, A: arrow::array::Array + 'static>(
    batch: &'batch RecordBatch,
    name: &str,
) -> &'batch A {
    let array = batch
        .column_by_name(name)
        .unwrap_or_else(|| panic!("the result carries a `{name}` column"));
    array.as_any().downcast_ref::<A>().unwrap_or_else(|| {
        panic!(
            "`{name}` has the canonical Arrow type, got {}",
            array.data_type()
        )
    })
}

/// Encodes one attribute collection exactly as the canonical ledger stores it.
///
/// This calls the production encoder rather than restating prost framing: the
/// bytes under test are a pinned protocol encoding, and a second hand-rolled
/// encoder in the fixture would prove only that two encoders agree.
pub(super) fn canonical_attribute_bytes(attributes: &[KeyValue]) -> Vec<u8> {
    vala_bifrost_redux::tables::signal::encode_attributes(attributes)
}

/// Decodes a stored attribute blob's string-valued entries by key.
///
/// A stock-exporter journey cannot compare the blob byte-for-byte the way the
/// pinned dataset does, because the SDK — not the test — decides which
/// attributes it emits and in what order. Reading the blob back as a map lets
/// those cases name the attributes they set without asserting anything about
/// the ones the SDK added on its own.
///
/// # Panics
///
/// Panics when the column does not hold a canonical `KeyValueList` encoding.
pub(super) fn decode_attributes(bytes: &[u8]) -> std::collections::HashMap<String, String> {
    use wyrd_tonic::otlp::common::v1::{KeyValueList, any_value};
    KeyValueList::decode(bytes)
        .expect("a stored attribute column is a canonical KeyValueList encoding")
        .values
        .into_iter()
        .filter_map(|entry| match entry.value.and_then(|value| value.value) {
            Some(any_value::Value::StringValue(text)) => Some((entry.key, text)),
            Some(any_value::Value::IntValue(number)) => Some((entry.key, number.to_string())),
            _ => None,
        })
        .collect()
}
