//! The `vala.traces.spans` telemetry fixture: deterministic GenAI/HTTP spans
//! sent through OTLP ingest, and the exact answers they must read back as.
//!
//! Like the events fixture, every value is a plain function of one span
//! index, so any report row can be checked against its data by hand. Spans
//! come in traces of [`SPANS_PER_TRACE`] with a fixed parent tree, spread
//! evenly over one day of `start_time_unix_nano`. Their attributes straddle
//! Bifrost's 10% Variant shredding threshold on purpose: core keys sit on
//! 25-90% of spans, mid keys on 6-25%, and thousands of long-tail `custom.*`
//! keys each on about 0.1%, so a published file shows which keys shredded.
//!
//! `start_time_unix_nano` is the table's time column, so a span-time window
//! prunes by its hourly partitions before any file statistics.

use std::collections::BTreeMap;

use num_traits::ToPrimitive;

use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope, KeyValue, KeyValueList};
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::span::Event;
use wyrd_tonic::otlp::trace::v1::status::StatusCode;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use crate::query::run::Usage;
use crate::query::workload::{DAY_HOURS, DAY_START_MS, HOUR_MS, Rows, mix};

/// The canonical span table OTLP trace ingest writes.
pub const TABLE: &str = "vala.traces.spans";

/// Spans in the standard telemetry fixture, as many as the events fixture's
/// rows.
pub const STANDARD_SPANS: i64 = 10_000_000;

/// Spans in every trace; the trace tree is [`Role::ALL`].
pub const SPANS_PER_TRACE: i64 = 8;

/// [`SPANS_PER_TRACE`] unsigned, the bound that indexes [`Role::ALL`].
const ROLES: u64 = SPANS_PER_TRACE.cast_unsigned();

/// Spans per OTLP export request: the OpenTelemetry SDK batch processor's
/// default `max_export_batch_size`.
pub const REQUEST_SPANS: i64 = 512;

/// Distinct `service.name` values.
pub const SERVICES: i64 = 24;

/// Distinct `http.route` values.
pub const ROUTES: u64 = 48;

/// The `gen_ai.request.model` values, chosen per trace.
pub const MODELS: [&str; 6] = [
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-haiku-4-5",
    "gpt-5",
    "gemini-3-pro",
    "llama-4",
];

/// Distinct `customer.id` values.
pub const CUSTOMERS: u64 = 200_000;

/// Distinct long-tail `custom.*` attribute keys.
pub const TAIL_KEYS: u64 = 3_000;

/// Long-tail keys each span carries.
const TAIL_PER_SPAN: u64 = 3;

/// Percent of spans that end in error with an `exception` event.
pub const ERROR_PERCENT: u64 = 2;

/// The `region` values, the 25% mid key.
pub const REGIONS: [&str; 4] = ["us-east", "us-west", "eu-west", "ap-south"];

/// The `user.plan` values.
const PLANS: [&str; 4] = ["free", "pro", "team", "enterprise"];

/// The `exception.type` values of failed spans.
const EXCEPTIONS: [&str; 3] = ["TimeoutError", "ConnectionResetError", "ValueError"];

/// Span attribute keys present on at least 10% of spans, which every
/// shredded published file must shred.
pub const SHREDDED_KEYS: [&str; 13] = [
    "customer.id",
    "user",
    "http.request.method",
    "http.route",
    "http.response.status_code",
    "gen_ai.operation.name",
    "gen_ai.provider.name",
    "gen_ai.request.model",
    "gen_ai.conversation.id",
    "region",
    "feature.flag",
    "cache.hit",
    "url.full",
];

/// The mid key deliberately under the threshold, which no file may shred.
pub const UNSHREDDED_KEY: &str = "retry.count";

/// Prefix of the long-tail keys, none of which may shred.
pub const TAIL_PREFIX: &str = "custom.";

/// `start_time_unix_nano` of the first trace: the events fixture's day start.
const DAY_START_NANOS: i64 = DAY_START_MS * 1_000_000;

/// The one-hour span-time window the pruning question reads: the seventh
/// hour of the fixture day, as an offset from its start.
const WINDOW_START_HOUR: i64 = 6;

/// Nanoseconds between consecutive spans' starts within one trace.
const SPAN_STAGGER_NANOS: i64 = 50_000;

/// The salts that make each per-span draw independent of the others.
mod salt {
    /// Duration's doubling class.
    pub const DURATION: u64 = 0x01;
    /// Duration within its class.
    pub const JITTER: u64 = 0x02;
    /// Whether the span failed.
    pub const ERROR: u64 = 0x03;
    /// Whether the span carries `customer.id`.
    pub const CUSTOMER: u64 = 0x04;
    /// Whether, and which, `user` the span carries.
    pub const USER: u64 = 0x05;
    /// `region` presence and value.
    pub const REGION: u64 = 0x06;
    /// `feature.flag` presence and value.
    pub const FLAG: u64 = 0x07;
    /// `cache.hit` presence and value.
    pub const CACHE: u64 = 0x08;
    /// `retry.count` presence and value.
    pub const RETRY: u64 = 0x09;
    /// Long-tail key choice.
    pub const TAIL: u64 = 0x0a;
    /// Trace identifier.
    pub const TRACE: u64 = 0x0b;
    /// Span identifier.
    pub const SPAN: u64 = 0x0c;
    /// Model choice per trace.
    pub const MODEL: u64 = 0x0d;
    /// Route choice per trace.
    pub const ROUTE: u64 = 0x0e;
    /// Token counts.
    pub const TOKENS: u64 = 0x0f;
}

/// One independent 64-bit draw for span `index` under `salt`.
const fn draw(index: i64, salt: u64) -> u64 {
    mix(index.cast_unsigned() ^ salt.rotate_left(56))
}

/// Whether span `index` carries the attribute drawn under `salt`, on
/// `percent` of spans.
const fn present(index: i64, salt: u64, percent: u64) -> bool {
    draw(index, salt) % 100 < percent
}

/// What a span does in its trace, by its position in the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Position 0: the inbound HTTP request, the trace root.
    Request,
    /// Position 1: the GenAI chat call the request makes.
    Chat,
    /// Position 2: the tool the chat call executes.
    Tool,
    /// Position 3: the database query the tool runs.
    Query,
    /// Position 4: the follow-up chat call with the tool's result.
    FollowUp,
    /// Position 5: an outbound HTTP call the request makes.
    Outbound,
    /// Position 6: a cache lookup the request makes.
    Cache,
    /// Position 7: the database query behind the outbound call.
    OutboundQuery,
}

impl Role {
    /// Every role in trace position order.
    pub const ALL: &[Self] = &[
        Self::Request,
        Self::Chat,
        Self::Tool,
        Self::Query,
        Self::FollowUp,
        Self::Outbound,
        Self::Cache,
        Self::OutboundQuery,
    ];

    /// The role of span `index`.
    #[must_use]
    pub const fn of(index: i64) -> Self {
        Self::ALL[(index.cast_unsigned() % ROLES) as usize]
    }

    /// The trace position of this role's parent; `None` for the root.
    const fn parent(self) -> Option<i64> {
        match self {
            Self::Request => None,
            Self::Chat | Self::Outbound | Self::Cache => Some(0),
            Self::Tool | Self::FollowUp => Some(1),
            Self::Query => Some(2),
            Self::OutboundQuery => Some(5),
        }
    }

    /// The service offset from the trace's root service.
    const fn service_offset(self) -> i64 {
        match self {
            Self::Request | Self::Cache => 0,
            Self::Chat | Self::Tool | Self::FollowUp => 1,
            Self::Query => 2,
            Self::Outbound | Self::OutboundQuery => 3,
        }
    }

    /// The OTLP span kind: server, client, or internal.
    const fn kind(self) -> i32 {
        match self {
            Self::Request => 2,
            Self::Cache | Self::Tool => 1,
            Self::Chat | Self::FollowUp | Self::Query | Self::Outbound | Self::OutboundQuery => 3,
        }
    }

    /// Whether this role is a GenAI operation.
    const fn is_gen_ai(self) -> bool {
        matches!(self, Self::Chat | Self::Tool | Self::FollowUp)
    }

    /// Whether this role is an HTTP call.
    const fn is_http(self) -> bool {
        matches!(self, Self::Request | Self::Outbound)
    }
}

/// The trace span `index` belongs to.
#[must_use]
pub const fn trace(index: i64) -> i64 {
    index / SPANS_PER_TRACE
}

/// The 16-byte identifier of `trace`.
#[must_use]
pub fn trace_id(trace: i64) -> [u8; 16] {
    let high = draw(trace, salt::TRACE).to_be_bytes();
    let low = mix(draw(trace, salt::TRACE)).to_be_bytes();
    let mut id = [0_u8; 16];
    id[..8].copy_from_slice(&high);
    id[8..].copy_from_slice(&low);
    id
}

/// The 8-byte identifier of span `index`.
#[must_use]
pub const fn span_id(index: i64) -> [u8; 8] {
    draw(index, salt::SPAN).to_be_bytes()
}

/// The `service.name` index of span `index`.
#[must_use]
pub const fn service(index: i64) -> i64 {
    (trace(index) * 5 + Role::of(index).service_offset()) % SERVICES
}

/// The `service.name` of service `service`.
#[must_use]
pub fn service_name(service: i64) -> String {
    format!("svc-{service:02}")
}

/// Span `index`'s duration: a doubling class drawn with halving
/// probability, from 250 µs up to 512 ms, then uniform within its class, so
/// the distribution has a long tail.
#[must_use]
pub const fn duration_nanos(index: i64) -> i64 {
    let class = (draw(index, salt::DURATION) | 1 << 11).trailing_zeros();
    let floor = 250_000_i64 << class;
    floor + (draw(index, salt::JITTER) % floor.cast_unsigned()).cast_signed()
}

/// Whether span `index` failed.
#[must_use]
pub const fn is_error(index: i64) -> bool {
    present(index, salt::ERROR, ERROR_PERCENT)
}

/// The `customer.id` span `index` carries, on 90% of spans; one customer per
/// trace.
#[must_use]
pub fn customer(index: i64) -> Option<String> {
    present(index, salt::CUSTOMER, 90)
        .then(|| format!("cust-{:06}", draw(trace(index), salt::CUSTOMER) % CUSTOMERS))
}

/// The `region` span `index` carries, on 25% of spans.
#[must_use]
pub const fn region(index: i64) -> Option<&'static str> {
    if present(index, salt::REGION, 25) {
        Some(REGIONS[(draw(index, salt::REGION) >> 32) as usize % REGIONS.len()])
    } else {
        None
    }
}

/// The `gen_ai.usage.input_tokens` of span `index`, on chat calls only.
#[must_use]
pub const fn input_tokens(index: i64) -> Option<i64> {
    match Role::of(index) {
        Role::Chat | Role::FollowUp => {
            Some(100 + (draw(index, salt::TOKENS) % 4_000).cast_signed())
        }
        _ => None,
    }
}

/// The `gen_ai.usage.output_tokens` of span `index`, on chat calls only.
#[must_use]
pub const fn output_tokens(index: i64) -> Option<i64> {
    match Role::of(index) {
        Role::Chat | Role::FollowUp => {
            Some(20 + ((draw(index, salt::TOKENS) >> 32) % 1_000).cast_signed())
        }
        _ => None,
    }
}

/// The `http.response.status_code` of span `index`, on HTTP calls only: 500
/// when the span failed, else 200.
#[must_use]
pub const fn status_code(index: i64) -> Option<i64> {
    if !Role::of(index).is_http() {
        None
    } else if is_error(index) {
        Some(500)
    } else {
        Some(200)
    }
}

/// One string attribute.
pub(crate) fn text(key: &str, value: impl Into<String>) -> KeyValue {
    attribute(key, Value::StringValue(value.into()))
}

/// One attribute of any value.
pub(crate) fn attribute(key: &str, value: Value) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue { value: Some(value) }),
    }
}

/// One telemetry fixture size and every span derived from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Telemetry {
    /// Spans written to [`TABLE`]; a multiple of [`SPANS_PER_TRACE`].
    pub spans: i64,
}

impl Telemetry {
    /// The standard 10-million-span fixture.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            spans: STANDARD_SPANS,
        }
    }

    /// Traces in the fixture.
    #[must_use]
    pub const fn traces(self) -> i64 {
        self.spans / SPANS_PER_TRACE
    }

    /// `start_time_unix_nano` of span `index`: traces spread evenly over one
    /// day, spans staggered within their trace.
    #[must_use]
    pub const fn start_nanos(self, index: i64) -> i64 {
        let day = DAY_HOURS * HOUR_MS * 1_000_000;
        DAY_START_NANOS
            + trace(index) * (day / self.traces())
            + (index % SPANS_PER_TRACE) * SPAN_STAGGER_NANOS
    }

    /// The OTLP export request carrying spans `first..last`, one
    /// `ResourceSpans` per service as an exporter batches them.
    #[must_use]
    pub fn request(self, first: i64, last: i64) -> ExportTraceServiceRequest {
        let mut by_service: BTreeMap<i64, Vec<Span>> = BTreeMap::new();
        for index in first..last {
            by_service
                .entry(service(index))
                .or_default()
                .push(self.span(index));
        }
        ExportTraceServiceRequest {
            resource_spans: by_service
                .into_iter()
                .map(|(service, spans)| ResourceSpans {
                    resource: Some(Resource {
                        attributes: vec![
                            text("service.name", service_name(service)),
                            text("service.version", format!("1.{}.0", service % 5)),
                            text("deployment.environment.name", "prod"),
                            text("host.name", format!("host-{}", service % 6)),
                        ],
                        ..Resource::default()
                    }),
                    scope_spans: vec![ScopeSpans {
                        scope: Some(InstrumentationScope {
                            name: "wyrd.bench.telemetry".to_owned(),
                            version: "1.0.0".to_owned(),
                            ..InstrumentationScope::default()
                        }),
                        spans,
                        ..ScopeSpans::default()
                    }],
                    ..ResourceSpans::default()
                })
                .collect(),
        }
    }

    /// Builds span `index`: its identity, timing, status, attributes, and an
    /// `exception` event when it failed.
    fn span(self, index: i64) -> Span {
        let role = Role::of(index);
        let start = self.start_nanos(index);
        let end = start + duration_nanos(index);
        let trace_start = index - index % SPANS_PER_TRACE;
        let model = MODELS[(draw(trace(index), salt::MODEL) % MODELS.len() as u64)
            .to_usize()
            .unwrap_or(0)];
        let route = draw(trace(index), salt::ROUTE) % ROUTES;
        let name = match role {
            Role::Request => format!("GET /api/r{route:02}"),
            Role::Chat | Role::FollowUp => format!("chat {model}"),
            Role::Tool => "execute_tool search".to_owned(),
            Role::Query | Role::OutboundQuery => "SELECT orders".to_owned(),
            Role::Outbound => "GET".to_owned(),
            Role::Cache => "cache.get".to_owned(),
        };
        let failed = is_error(index);
        let exception = EXCEPTIONS[index.to_usize().unwrap_or(0) % EXCEPTIONS.len()];
        let message = format!("{exception}: request {} failed", index % 997);
        Span {
            trace_id: trace_id(trace(index)).to_vec(),
            span_id: span_id(index).to_vec(),
            parent_span_id: role
                .parent()
                .map_or_else(Vec::new, |parent| span_id(trace_start + parent).to_vec()),
            name,
            kind: role.kind(),
            start_time_unix_nano: u64::try_from(start).unwrap_or_default(),
            end_time_unix_nano: u64::try_from(end).unwrap_or_default(),
            attributes: self.attributes(index, role, model, route),
            events: if failed {
                vec![Event {
                    time_unix_nano: u64::try_from(start + (end - start) / 2).unwrap_or_default(),
                    name: "exception".to_owned(),
                    attributes: vec![
                        text("exception.type", exception),
                        text("exception.message", message.clone()),
                        text(
                            "exception.stacktrace",
                            format!(
                                "{exception}: {message}\n  at handler (app/r{route:02}.py:{})\n  \
                                 at dispatch (app/server.py:212)\n  at main (app/main.py:40)",
                                index % 400
                            ),
                        ),
                    ],
                    ..Event::default()
                }]
            } else {
                Vec::new()
            },
            status: Some(Status {
                message: if failed { message } else { String::new() },
                code: if failed {
                    StatusCode::Error
                } else {
                    StatusCode::Unset
                } as i32,
            }),
            ..Span::default()
        }
    }

    /// Span `index`'s attributes: its role's semantic conventions, then the
    /// core, mid, and long-tail keys at their presence rates.
    fn attributes(self, index: i64, role: Role, model: &str, route: u64) -> Vec<KeyValue> {
        let mut attributes = Vec::with_capacity(16);
        if role.is_http() {
            attributes.push(text("http.request.method", "GET"));
            attributes.push(text("http.route", format!("/api/r{route:02}")));
            if let Some(status) = status_code(index) {
                attributes.push(attribute(
                    "http.response.status_code",
                    Value::IntValue(status),
                ));
            }
        }
        if role == Role::Outbound {
            attributes.push(text(
                "url.full",
                format!(
                    "https://partner.example/api/r{route:02}?trace={}",
                    trace(index)
                ),
            ));
        }
        if role.is_gen_ai() {
            let operation = if role == Role::Tool {
                "execute_tool"
            } else {
                "chat"
            };
            attributes.push(text("gen_ai.operation.name", operation));
            attributes.push(text("gen_ai.provider.name", "anthropic"));
            attributes.push(text("gen_ai.request.model", model));
            attributes.push(text(
                "gen_ai.conversation.id",
                format!("conv-{:05}", trace(index) % 50_000),
            ));
        }
        if let (Some(input), Some(output)) = (input_tokens(index), output_tokens(index)) {
            attributes.push(attribute(
                "gen_ai.usage.input_tokens",
                Value::IntValue(input),
            ));
            attributes.push(attribute(
                "gen_ai.usage.output_tokens",
                Value::IntValue(output),
            ));
        }
        if let Some(customer) = customer(index) {
            attributes.push(text("customer.id", customer));
        }
        if present(index, salt::USER, 70) {
            let user = draw(index, salt::USER);
            attributes.push(attribute(
                "user",
                Value::KvlistValue(KeyValueList {
                    values: vec![
                        text("id", format!("u-{:07}", (user >> 8) % 1_000_000)),
                        text("plan", PLANS[(user >> 40) as usize % PLANS.len()]),
                    ],
                }),
            ));
        }
        if let Some(region) = region(index) {
            attributes.push(text("region", region));
        }
        if present(index, salt::FLAG, 20) {
            attributes.push(text(
                "feature.flag",
                format!("flag-{}", (draw(index, salt::FLAG) >> 32) % 12),
            ));
        }
        if present(index, salt::CACHE, 15) {
            attributes.push(attribute(
                "cache.hit",
                Value::BoolValue(draw(index, salt::CACHE) >> 63 == 1),
            ));
        }
        if present(index, salt::RETRY, 6) {
            attributes.push(attribute(
                UNSHREDDED_KEY,
                Value::IntValue(1 + ((draw(index, salt::RETRY) >> 32) % 4).cast_signed()),
            ));
        }
        let mut tail = draw(index, salt::TAIL);
        for _ in 0..TAIL_PER_SPAN {
            let key = tail % TAIL_KEYS;
            let value = match key % 4 {
                0 => Value::IntValue((tail >> 20).cast_signed() % 10_000),
                1 => Value::StringValue(format!("v{}", (tail >> 20) % 1_000)),
                2 => Value::BoolValue(tail >> 63 == 1),
                _ => Value::DoubleValue(((tail >> 20) % 10_000) as f64 / 100.0),
            };
            attributes.push(attribute(&format!("{TAIL_PREFIX}k{key:04}"), value));
            tail = mix(tail);
        }
        attributes
    }

    /// Counts every span: the unbounded read the window is compared with.
    #[must_use]
    pub fn all_spans(self) -> (&'static str, String, Rows) {
        (
            "spans",
            format!("SELECT COUNT(*) FROM {TABLE}"),
            vec![vec![self.spans]],
        )
    }

    /// Counts the spans starting in one hour of the fixture day, bounded on
    /// the table's time column `start_time_unix_nano`.
    ///
    /// The bound is a nanosecond instant, so Oracle prunes by the hourly
    /// partitions and file bounds before reading a row.
    #[must_use]
    pub fn span_window(self) -> (&'static str, String, Rows) {
        let lower = DAY_START_NANOS + WINDOW_START_HOUR * HOUR_MS * 1_000_000;
        let upper = lower + HOUR_MS * 1_000_000;
        let inside = (0..self.spans)
            .filter(|index| (lower..upper).contains(&self.start_nanos(*index)))
            .count();
        (
            "span window",
            format!(
                "SELECT COUNT(*) FROM {TABLE} \
                 WHERE start_time_unix_nano >= to_timestamp_nanos({lower}) \
                 AND start_time_unix_nano < to_timestamp_nanos({upper})"
            ),
            vec![vec![inside.to_i64().unwrap_or(i64::MAX)]],
        )
    }

    /// The exact-answer statements of the written spans, each with its name
    /// and the answer computed from the span functions.
    ///
    /// Each answer loops over every span, so callers compute them once.
    #[must_use]
    pub fn questions(self) -> Vec<(&'static str, String, Rows)> {
        let count =
            |keep: &dyn Fn(i64) -> bool| (0..self.spans).filter(|index| keep(*index)).count();
        let count =
            |keep: &dyn Fn(i64) -> bool| vec![vec![count(keep).to_i64().unwrap_or(i64::MAX)]];
        let lookup = self.traces() / 2;
        let lookup_hex = hex::encode(trace_id(lookup));
        let lookup_spans = lookup * SPANS_PER_TRACE..(lookup + 1) * SPANS_PER_TRACE;
        let customer = customer(lookup * SPANS_PER_TRACE + 1).unwrap_or_default();
        vec![
            self.all_spans(),
            self.span_window(),
            (
                "one service",
                format!(
                    "SELECT COUNT(*) FROM {TABLE} WHERE service_name = '{}'",
                    service_name(3)
                ),
                count(&|index| service(index) == 3),
            ),
            (
                "trace lookup",
                format!(
                    "SELECT COUNT(*), SUM(duration_nano) FROM {TABLE} \
                     WHERE trace_id = X'{lookup_hex}'"
                ),
                vec![vec![
                    SPANS_PER_TRACE,
                    lookup_spans.map(duration_nanos).sum(),
                ]],
            ),
            (
                "errors",
                format!("SELECT COUNT(*) FROM {TABLE} WHERE status_code = 2"),
                count(&is_error),
            ),
            (
                "server errors",
                format!("SELECT COUNT(*) FROM {TABLE} WHERE http_response_status_code = 500"),
                count(&|index| status_code(index) == Some(500)),
            ),
            (
                "input tokens",
                format!("SELECT SUM(gen_ai_usage_input_tokens) FROM {TABLE}"),
                vec![vec![(0..self.spans).filter_map(input_tokens).sum()]],
            ),
            (
                "customer",
                format!(
                    "SELECT COUNT(*) FROM {TABLE} WHERE attributes ->> 'customer.id' = '{customer}'"
                ),
                count(&|index| self::customer(index).as_deref() == Some(customer.as_str())),
            ),
            (
                "region",
                format!("SELECT COUNT(*) FROM {TABLE} WHERE attributes ->> 'region' = 'eu-west'"),
                count(&|index| region(index) == Some("eu-west")),
            ),
        ]
    }
}

/// Judges span-time pruning: the one-hour window must scan fewer files or
/// fewer row groups than the unbounded count over the same published spans.
///
/// Returns the judged outcome with both reads' scan counts.
pub fn judge_pruning(all: &Usage, window: &Usage) -> std::result::Result<String, String> {
    let scanned = |usage: &Usage| {
        (
            usage.scanned("oracle_query_files_scanned_total"),
            usage.scanned("oracle_query_row_groups_scanned_total"),
        )
    };
    let (all_files, all_groups) = scanned(all);
    let (window_files, window_groups) = scanned(window);
    let summary = format!(
        "window scanned {window_files:.0} files and {window_groups:.0} row groups, \
         unbounded {all_files:.0} and {all_groups:.0}"
    );
    if window_files < all_files || window_groups < all_groups {
        Ok(summary)
    } else {
        Err(format!("{summary}; the window pruned nothing"))
    }
}

/// Judges the Variant layout of the published span files under `root`:
/// every shredded file shreds each of [`SHREDDED_KEYS`] in `attributes` and
/// neither [`UNSHREDDED_KEY`] nor any long-tail key.
///
/// Returns the judged outcome; no shredded file at all is a failure, since
/// the fixture exists to put keys over the threshold.
pub fn judge_shredding(files: &[std::path::PathBuf]) -> std::result::Result<String, String> {
    let (mut shredded, mut flat, mut faults) = (0_usize, 0_usize, Vec::new());
    for path in files {
        match shredded_keys(path) {
            Ok(None) => {}
            Ok(Some(keys)) if keys.is_empty() => flat += 1,
            Ok(Some(keys)) => {
                shredded += 1;
                let missing: Vec<_> = SHREDDED_KEYS
                    .iter()
                    .filter(|key| !keys.iter().any(|shredded| shredded == *key))
                    .collect();
                let extra: Vec<_> = keys
                    .iter()
                    .filter(|key| *key == UNSHREDDED_KEY || key.starts_with(TAIL_PREFIX))
                    .collect();
                if !missing.is_empty() || !extra.is_empty() {
                    faults.push(format!(
                        "{}: missing {missing:?}, unexpected {extra:?}",
                        path.display()
                    ));
                }
            }
            Err(error) => faults.push(format!("{}: {error}", path.display())),
        }
    }
    let summary = format!("{shredded} shredded and {flat} unshredded span files");
    if shredded == 0 {
        Err(format!("{summary}; none shredded"))
    } else if let Some(first) = faults.first() {
        Err(format!("{summary}; {} wrong, first {first}", faults.len()))
    } else {
        Ok(format!(
            "{summary}; each shreds the {} core and mid keys, not {UNSHREDDED_KEY} or {TAIL_PREFIX}*",
            SHREDDED_KEYS.len()
        ))
    }
}

/// The keys shredded under `attributes.typed_value` in the Parquet file at
/// `path`; `None` when the file is not a span file (no `span_id`), empty when it is
/// unshredded.
///
/// # Errors
///
/// Returns the open, footer, or schema-conversion failure.
fn shredded_keys(path: &std::path::Path) -> std::result::Result<Option<Vec<String>>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|error| error.to_string())?;
    let schema = builder.schema();
    let (Ok(_), Ok(attributes)) = (
        schema.field_with_name("span_id"),
        schema.field_with_name("attributes"),
    ) else {
        return Ok(None);
    };
    let arrow::datatypes::DataType::Struct(storage) = attributes.data_type() else {
        return Ok(Some(Vec::new()));
    };
    Ok(Some(
        match storage
            .find("typed_value")
            .map(|(_, field)| field.data_type())
        {
            Some(arrow::datatypes::DataType::Struct(keys)) => {
                keys.iter().map(|key| key.name().clone()).collect()
            }
            _ => Vec::new(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Usage whose window scanned `files` files and `groups` row groups.
    fn scanning(files: f64, groups: f64) -> Usage {
        let mut usage = Usage::default();
        for (counter, value) in [
            ("oracle_query_files_scanned_total", files),
            ("oracle_query_row_groups_scanned_total", groups),
        ] {
            let index = crate::query::run::SCAN_COUNTERS
                .iter()
                .position(|name| *name == counter)
                .expect("the counter is a scan counter");
            usage.scan_counts[index] = value;
        }
        usage
    }

    /// The window passes when it scans fewer files or fewer row groups than
    /// the unbounded read, and fails when it scans as much.
    ///
    /// # Panics
    ///
    /// Panics when the judge passes an unpruned window or fails a pruned one.
    #[test]
    fn span_window_pruning_needs_fewer_files_or_row_groups() {
        let all = scanning(24.0, 96.0);

        assert!(judge_pruning(&all, &scanning(1.0, 4.0)).is_ok());
        assert!(judge_pruning(&all, &scanning(24.0, 90.0)).is_ok());
        assert!(judge_pruning(&all, &scanning(24.0, 96.0)).is_err());
    }

    /// Share of the first `spans` spans for which `keep` holds.
    fn share(spans: i64, keep: impl Fn(i64) -> bool) -> f64 {
        (0..spans).filter(|index| keep(*index)).count() as f64 / spans as f64
    }

    /// Whether span `index`'s request carries attribute `key`.
    fn carries(index: i64, key: &str) -> bool {
        Telemetry::standard()
            .span(index)
            .attributes
            .iter()
            .any(|attribute| attribute.key == key)
    }

    /// The generator is deterministic, builds each trace as the fixed tree,
    /// and puts every key class on its side of the 10% shredding threshold.
    ///
    /// # Panics
    ///
    /// Panics when a span, its tree, or a presence rate drifts from the
    /// fixture's stated shape.
    #[test]
    fn telemetry_spans_are_deterministic_trees_straddling_the_threshold() {
        let fixture = Telemetry::standard();
        let request = fixture.request(0, 16);
        let spans: Vec<&Span> = request
            .resource_spans
            .iter()
            .flat_map(|resource| &resource.scope_spans)
            .flat_map(|scope| &scope.spans)
            .collect();
        assert_eq!(spans.len(), 16);
        assert_eq!(fixture.request(0, 16), request, "the same spans every time");
        assert_eq!(fixture.span(9).parent_span_id, span_id(8).to_vec());
        assert_eq!(fixture.span(11).parent_span_id, span_id(10).to_vec());
        assert!(
            fixture.span(8).parent_span_id.is_empty(),
            "position 0 is the root"
        );
        assert_eq!(fixture.span(8).trace_id, fixture.span(15).trace_id);
        assert_ne!(fixture.span(7).trace_id, fixture.span(8).trace_id);
        let last = fixture.start_nanos(STANDARD_SPANS - 1) / 1_000_000 - DAY_START_MS;
        assert!(
            (DAY_HOURS * HOUR_MS - 100..DAY_HOURS * HOUR_MS).contains(&last),
            "the last trace starts in the day's last 100 ms, at {last} ms"
        );

        let sample = 200_000;
        for key in SHREDDED_KEYS {
            let rate = share(sample, |index| carries(index, key));
            assert!(rate >= 0.12, "{key} is on {rate:.3} of spans");
        }
        let retry = share(sample, |index| carries(index, UNSHREDDED_KEY));
        assert!((0.05..0.07).contains(&retry), "retry.count on {retry:.3}");
        let tail = share(sample, |index| carries(index, "custom.k0042"));
        assert!(tail < 0.01, "a long-tail key is on {tail:.4} of spans");
        let errors = share(sample, is_error);
        assert!((0.015..0.025).contains(&errors), "errors on {errors:.3}");
        assert!(
            (0..sample).map(duration_nanos).max() > Some(100_000_000),
            "the duration tail reaches past 100 ms"
        );
    }

    /// The exact answers agree with the span functions on a small fixture.
    ///
    /// # Panics
    ///
    /// Panics when an answer disagrees with a direct count over the spans.
    #[test]
    fn telemetry_answers_count_the_generated_spans() {
        let fixture = Telemetry { spans: 8_000 };
        let questions = fixture.questions();
        assert_eq!(questions[0].2, vec![vec![8_000]]);
        let errors = (0..8_000).filter(|index| is_error(*index)).count().to_i64();
        assert_eq!(questions[4].2, vec![vec![errors.unwrap_or(i64::MAX)]]);
        assert!(
            questions[7].2[0][0] >= 1,
            "the looked-up trace's customer is counted"
        );
    }
}
