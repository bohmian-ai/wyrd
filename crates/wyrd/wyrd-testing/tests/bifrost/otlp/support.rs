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
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::span::{Event, Link};
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};

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

/// How long after its start the maximal span ends, in nanoseconds.
pub(super) const SPAN_DURATION_NANOS: i64 = 5_000_000;
/// How long after its span's start the single event was recorded.
pub(super) const EVENT_OFFSET_NANOS: i64 = 1_000_000;

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
/// encoding must retain with the six pinned `GenAI` promotions, so one span
/// exercises both the opaque attribute blob and every promoted column.
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

    /// Runs one public query expected to be refused and returns its error code.
    ///
    /// The refusal must arrive before the stream opens, as a transport-level
    /// stable error: a caller that is handed an open stream and then a failed
    /// terminal has already been told the query was accepted, and the closed
    /// terminal code vocabulary cannot name a payload refusal.
    ///
    /// # Panics
    ///
    /// Panics when the query is accepted, or fails in any shape other than a
    /// pre-stream stable refusal.
    pub(super) async fn query_error(&self, client: &wyrd_client::WyrdClient, sql: &str) -> String {
        let outcome = vala_sdk::query::QueryClient::new(client)
            .query(&wyrd_spec::vala::api::BifrostQueryRequest {
                sql: sql.to_owned(),
                visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
                freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                deadline_ms: Some(120_000),
            })
            .await;
        match outcome {
            Ok(_) => panic!("`{sql}` must be refused before its stream opens"),
            Err(vala_sdk::query::ValaSdkError::Transport(error)) => error.code().to_owned(),
            Err(other) => panic!("`{sql}` is refused pre-stream, got {other}"),
        }
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
        let mut stream = vala_sdk::query::QueryClient::new(client)
            .query(&wyrd_spec::vala::api::BifrostQueryRequest {
                sql: sql.to_owned(),
                visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
                freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                deadline_ms: Some(120_000),
            })
            .await
            .unwrap_or_else(|error| panic!("public query `{sql}` starts: {}", error.detail()));
        let mut batches = Vec::new();
        while let Some(batch) = stream
            .next_batch()
            .await
            .unwrap_or_else(|error| panic!("public query `{sql}` streams: {}", error.detail()))
        {
            batches.push(batch);
        }
        batches
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
