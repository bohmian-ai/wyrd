//! The `ui-sql` evidence: a deterministic model of the requests a
//! development journey sends, and the OTLP spans, logs and metrics each
//! produces inside the catalog's fixed hour.
//!
//! Every value is a plain function of one request index, so each expected
//! answer comes from the model, never from a server. Requests pair up on
//! one start time (timestamp ties); each trace's children after the first
//! export one batch later than their root (late children, repeated trace
//! ids across batches); `child-service` serves children only, so a
//! child-only match selects roots of other services; optional attributes,
//! correlations and event names stay null on a fixed share of rows.

use std::collections::BTreeMap;

use num_traits::ToPrimitive;

use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope, KeyValue, KeyValueList};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics::v1::metric::Data;
use wyrd_tonic::otlp::metrics::v1::number_data_point::Value as Number;
use wyrd_tonic::otlp::metrics::v1::{
    AggregationTemporality, Gauge, Histogram, HistogramDataPoint, Metric, NumberDataPoint,
    ResourceMetrics, ScopeMetrics, Sum,
};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::status::StatusCode;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

use crate::query::telemetry::{attribute, text};
use crate::query::workload::mix;

/// `2026-11-18T12:00:00Z`, the catalog range's start, in Unix nanoseconds.
pub const HOUR_START_NANOS: i64 = 1_795_003_200_000_000_000;

/// One hour in nanoseconds: the catalog range's length.
pub const HOUR_NANOS: i64 = 3_600_000_000_000;

/// Services that serve trace roots, one per route.
pub const ROOT_SERVICES: [&str; 3] = ["checkout", "cart", "search"];

/// The service that serves only child spans: the catalog's child-only
/// match bind.
pub const CHILD_SERVICE: &str = "child-service";

/// Services child spans rotate through.
pub const CHILD_SERVICES: [&str; 4] = ["llm-proxy", CHILD_SERVICE, "inventory", "cart"];

/// The catalog's metric name: every plotted series carries it.
pub const METRIC: &str = "m.requests";

/// A second metric name, so the names query lists more than one.
pub const CPU_METRIC: &str = "m.cpu";

/// Seconds between metric exports.
pub const EXPORT_SECONDS: i64 = 10;

/// Metric exports inside the hour.
pub const EXPORTS: i64 = 3_600 / EXPORT_SECONDS;

/// The export at which route 0's cumulative counter and histogram reset.
pub const RESET_EXPORT: i64 = EXPORTS / 2;

/// The histogram's explicit bounds, milliseconds.
pub const BOUNDS: [f64; 3] = [10.0, 100.0, 1_000.0];

/// The first integer gauge value: one past 2^53, so it survives only as an
/// exact integer, never through a double.
pub const GAUGE_BASE: i64 = (1 << 53) + 1;

/// Requests whose roots and children export together in one OTLP request.
pub const EXPORT_REQUESTS: i64 = 16;

/// One fixture size: the requests a development journey sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Journey {
    /// Requests, an even count; each pair shares a start time.
    pub requests: i64,
    /// Routes the metrics split requests over.
    pub routes: i64,
}

impl Journey {
    /// The default journey: 500 requests.
    pub const DEFAULT: Self = Self {
        requests: 500,
        routes: 3,
    };

    /// Start time of request `r`: pairs of requests share one of
    /// `requests / 2` slots spread evenly over the hour.
    #[must_use]
    pub const fn start(self, r: i64) -> i64 {
        HOUR_START_NANOS + (r / 2) * (HOUR_NANOS / (self.requests / 2))
    }

    /// The metric route of request `r`.
    #[must_use]
    pub const fn route(self, r: i64) -> i64 {
        r % self.routes
    }

    /// The time of metric export `j`; series `k` exports `k`
    /// milliseconds later, so no two points share a time.
    #[must_use]
    pub const fn export_time(j: i64, k: i64) -> i64 {
        HOUR_START_NANOS + j * EXPORT_SECONDS * 1_000_000_000 + k * 1_000_000
    }
}

/// Spans in request `r`'s trace: three to six.
#[must_use]
pub const fn spans(r: i64) -> i64 {
    3 + r % 4
}

/// Request `r`'s 16-byte trace id.
#[must_use]
pub fn trace_id(r: i64) -> [u8; 16] {
    let mut id = [0_u8; 16];
    id[..8].copy_from_slice(&mix(r.cast_unsigned() ^ 0x7a11).to_be_bytes());
    id[8..].copy_from_slice(&mix(r.cast_unsigned() ^ 0x7a12).to_be_bytes());
    id
}

/// The 8-byte id of span `k` of request `r`.
#[must_use]
pub fn span_id(r: i64, k: i64) -> [u8; 8] {
    mix((r.cast_unsigned() << 8 | k.cast_unsigned()) ^ 0x5a17).to_be_bytes()
}

/// The service of span `k` of request `r`: the root's route service, then
/// the child rotation.
#[must_use]
pub fn service(r: i64, k: i64) -> &'static str {
    if k == 0 {
        pick(&ROOT_SERVICES, r)
    } else {
        pick(&CHILD_SERVICES, r + k)
    }
}

/// The name of span `k` of request `r`.
#[must_use]
pub fn span_name(r: i64, k: i64) -> String {
    match k {
        0 => format!("GET /api/{}", pick(&ROOT_SERVICES, r)),
        1 => "chat claude-opus-5".to_owned(),
        2 => "execute_tool search".to_owned(),
        _ => "SELECT orders".to_owned(),
    }
}

/// Start of span `k` of request `r`: children 10 µs apart after the root.
#[must_use]
pub const fn span_start(journey: Journey, r: i64, k: i64) -> i64 {
    journey.start(r) + k * 10_000
}

/// Duration of span `k` of request `r`: a root of 50 ms, 500 ms, or 2 s in
/// a 12:5:3 split, each child half its root.
///
/// The few values form wide plateaus around every p50 and p95 rank the
/// catalog asks, roots alone or with children, so `approx_percentile_cont`
/// answers the plateau's value however the rows are split across memtables,
/// staged runs, and Parquet files.
#[must_use]
pub const fn duration(r: i64, k: i64) -> i64 {
    let root = match mix(r.cast_unsigned() ^ 0xd0) % 20 {
        0..=11 => 50_000_000,
        12..=16 => 500_000_000,
        _ => 2_000_000_000,
    };
    if k == 0 { root } else { root / 2 }
}

/// The OTLP status code of span `k` of request `r`: the root fails every
/// 17th request and is OK every 5th, the last child fails every 11th, and
/// everything else is unset.
#[must_use]
pub const fn status(r: i64, k: i64) -> i32 {
    if k == 0 && r % 17 == 0 || k == spans(r) - 1 && r % 11 == 0 {
        StatusCode::Error as i32
    } else if k == 0 && r % 5 == 0 {
        StatusCode::Ok as i32
    } else {
        StatusCode::Unset as i32
    }
}

/// The root's `http.response.status_code`: 500 when it failed, 404 every
/// ninth request, else 200; children carry none.
#[must_use]
pub const fn http_status(r: i64, k: i64) -> Option<i64> {
    if k != 0 {
        None
    } else if status(r, 0) == StatusCode::Error as i32 {
        Some(500)
    } else if r % 9 == 0 {
        Some(404)
    } else {
        Some(200)
    }
}

/// The `gen_ai.operation.name` of span `k` of request `r`: every fourth
/// request's root invokes an agent, and chat and tool children name their
/// operation.
#[must_use]
pub const fn operation(r: i64, k: i64) -> Option<&'static str> {
    match k {
        0 if r % 4 == 0 => Some("invoke_agent"),
        1 => Some("chat"),
        2 => Some("execute_tool"),
        _ => None,
    }
}

/// Severity number and text of request `r`'s completion log.
#[must_use]
pub const fn severity(r: i64) -> (i32, &'static str) {
    if r % 5 == 0 {
        (13, "WARN")
    } else {
        (9, "INFO")
    }
}

/// Whether request `r` also logs an error.
#[must_use]
pub const fn logs_error(r: i64) -> bool {
    r % 7 == 0
}

/// The exception type of request `r`'s error log.
#[must_use]
pub const fn exception(r: i64) -> &'static str {
    if r % 2 == 0 {
        "TimeoutError"
    } else {
        "ValueError"
    }
}

/// The `event_name` of request `r`'s completion log, on every third.
#[must_use]
pub const fn event_name(r: i64) -> Option<&'static str> {
    if r % 3 == 0 {
        Some("request.completed")
    } else {
        None
    }
}

/// Whether request `r`'s logs carry its trace correlation: all but every
/// tenth.
#[must_use]
pub const fn correlated(r: i64) -> bool {
    r % 10 != 0
}

/// Time of log `l` (0 completion, 1 error) of request `r`; every log time
/// is distinct.
#[must_use]
pub const fn log_time(journey: Journey, r: i64, l: i64) -> i64 {
    journey.start(r) + 500_000 + (r % 2) * 1_000 + l * 100
}

/// Observed time of a log: one millisecond after its time.
#[must_use]
pub const fn observed(time: i64) -> i64 {
    time + 1_000_000
}

impl Journey {
    /// Request indexes whose root exports in batch `b`.
    fn batch(self, b: i64) -> std::ops::Range<i64> {
        (b * EXPORT_REQUESTS).min(self.requests)..((b + 1) * EXPORT_REQUESTS).min(self.requests)
    }

    /// Span export batches: each holds its requests' roots and first
    /// children, plus the later children of the batch before.
    #[must_use]
    pub fn span_batches(self) -> i64 {
        (self.requests + EXPORT_REQUESTS - 1) / EXPORT_REQUESTS + 1
    }

    /// Span export `b`.
    #[must_use]
    pub fn spans_request(self, b: i64) -> ExportTraceServiceRequest {
        let mut by_service: BTreeMap<&'static str, Vec<Span>> = BTreeMap::new();
        for r in self.batch(b) {
            for k in 0..2 {
                by_service
                    .entry(service(r, k))
                    .or_default()
                    .push(self.span(r, k));
            }
        }
        if b > 0 {
            for r in self.batch(b - 1) {
                for k in 2..spans(r) {
                    by_service
                        .entry(service(r, k))
                        .or_default()
                        .push(self.span(r, k));
                }
            }
        }
        ExportTraceServiceRequest {
            resource_spans: by_service
                .into_iter()
                .map(|(service, spans)| ResourceSpans {
                    resource: Some(resource(service)),
                    scope_spans: vec![ScopeSpans {
                        scope: Some(scope()),
                        spans,
                        ..ScopeSpans::default()
                    }],
                    ..ResourceSpans::default()
                })
                .collect(),
        }
    }

    /// Span `k` of request `r`.
    fn span(self, r: i64, k: i64) -> Span {
        let start = span_start(self, r, k);
        let mut attributes = vec![
            text("route", format!("/api/{}", pick(&ROOT_SERVICES, r))),
            attribute(
                "customer",
                Value::KvlistValue(KeyValueList {
                    values: vec![
                        text("id", format!("cust-{:04}", r % 997)),
                        attribute("tier", Value::IntValue(r % 4)),
                    ],
                }),
            ),
        ];
        if let Some(code) = http_status(r, k) {
            attributes.push(attribute(
                "http.response.status_code",
                Value::IntValue(code),
            ));
        }
        if let Some(operation) = operation(r, k) {
            attributes.push(text("gen_ai.operation.name", operation));
        }
        Span {
            trace_id: trace_id(r).to_vec(),
            span_id: span_id(r, k).to_vec(),
            parent_span_id: if k == 0 {
                Vec::new()
            } else {
                span_id(r, i64::from(k > 2)).to_vec()
            },
            name: span_name(r, k),
            kind: if k == 0 { 2 } else { 3 },
            start_time_unix_nano: start.cast_unsigned(),
            end_time_unix_nano: (start + duration(r, k)).cast_unsigned(),
            attributes,
            status: Some(Status {
                message: String::new(),
                code: status(r, k),
            }),
            ..Span::default()
        }
    }

    /// Log export batches, one per span batch's requests.
    #[must_use]
    pub fn log_batches(self) -> i64 {
        (self.requests + EXPORT_REQUESTS - 1) / EXPORT_REQUESTS
    }

    /// Log export `b`: each of its requests' completion log, and its error
    /// log when it has one.
    #[must_use]
    pub fn logs_request(self, b: i64) -> ExportLogsServiceRequest {
        let mut by_service: BTreeMap<&'static str, Vec<LogRecord>> = BTreeMap::new();
        for r in self.batch(b) {
            let (number, text_severity) = severity(r);
            let mut records = vec![self.log(r, 0, number, text_severity, None)];
            if logs_error(r) {
                records.push(self.log(r, 1, 17, "ERROR", Some(exception(r))));
            }
            by_service.entry(service(r, 0)).or_default().extend(records);
        }
        ExportLogsServiceRequest {
            resource_logs: by_service
                .into_iter()
                .map(|(service, log_records)| ResourceLogs {
                    resource: Some(resource(service)),
                    scope_logs: vec![ScopeLogs {
                        scope: Some(scope()),
                        log_records,
                        ..ScopeLogs::default()
                    }],
                    ..ResourceLogs::default()
                })
                .collect(),
        }
    }

    /// Log `l` of request `r`.
    fn log(self, r: i64, l: i64, number: i32, severity: &str, failed: Option<&str>) -> LogRecord {
        let time = log_time(self, r, l);
        let mut attributes = vec![text("route", format!("/api/{}", pick(&ROOT_SERVICES, r)))];
        if let Some(exception) = failed {
            attributes.push(text("exception.type", exception));
        }
        let (trace, span) = if correlated(r) {
            (trace_id(r).to_vec(), span_id(r, 0).to_vec())
        } else {
            (Vec::new(), Vec::new())
        };
        LogRecord {
            time_unix_nano: time.cast_unsigned(),
            observed_time_unix_nano: observed(time).cast_unsigned(),
            severity_number: number,
            severity_text: severity.to_owned(),
            body: Some(AnyValue {
                value: Some(Value::StringValue(if failed.is_some() {
                    format!("request {r} failed")
                } else {
                    format!("request {r} handled")
                })),
            }),
            attributes,
            trace_id: trace,
            span_id: span,
            event_name: if l == 0 {
                event_name(r).unwrap_or_default().to_owned()
            } else {
                String::new()
            },
            ..LogRecord::default()
        }
    }

    /// Requests of route `q` started in `(after, until]`.
    #[must_use]
    pub fn started(self, q: Option<i64>, after: i64, until: i64) -> i64 {
        (0..self.requests)
            .filter(|r| q.is_none_or(|q| self.route(*r) == q))
            .filter(|r| (after + 1..=until).contains(&self.start(*r)))
            .count()
            .to_i64()
            .unwrap_or(i64::MAX)
    }

    /// Root durations, milliseconds, of route `q`'s requests started in
    /// `(after, until]`.
    fn durations(self, q: i64, after: i64, until: i64) -> impl Iterator<Item = f64> {
        (0..self.requests)
            .filter(move |r| self.route(*r) == q && (after + 1..=until).contains(&self.start(*r)))
            .map(|r| duration(r, 0) as f64 / 1e6)
    }

    /// The cumulative start of route `q`'s series at export `j`: one hour
    /// before the range, or the reset export for route 0 after its reset.
    #[must_use]
    pub const fn cumulative_start(q: i64, j: i64) -> i64 {
        if q == 0 && j >= RESET_EXPORT {
            Self::export_time(RESET_EXPORT - 1, 0)
        } else {
            HOUR_START_NANOS - HOUR_NANOS
        }
    }

    /// Metric exports, one per [`EXPORT_SECONDS`] of the hour.
    #[must_use]
    pub const fn metric_batches(self) -> i64 {
        EXPORTS
    }

    /// Metric export `j`: per route a cumulative monotonic counter and a
    /// cumulative histogram of root durations (route 0 resets at
    /// [`RESET_EXPORT`]), one delta counter, one integer gauge above 2^53,
    /// and one double gauge of another metric name.
    #[must_use]
    pub fn metrics_request(self, j: i64) -> ExportMetricsServiceRequest {
        let mut resources: BTreeMap<String, Vec<Metric>> = BTreeMap::new();
        let mut series = 0;
        for q in 0..self.routes {
            let start = Self::cumulative_start(q, j);
            let counted = self.started(Some(q), start, Self::export_time(j, 0));
            let time = Self::export_time(j, series);
            let route = vec![text("route", format!("/r{q:02}"))];
            resources.entry(metric_service(q)).or_default().extend([
                Metric {
                    name: METRIC.to_owned(),
                    unit: "1".to_owned(),
                    data: Some(Data::Sum(Sum {
                        data_points: vec![number(
                            route.clone(),
                            start,
                            time,
                            Number::AsInt(counted),
                        )],
                        aggregation_temporality: AggregationTemporality::Cumulative as i32,
                        is_monotonic: true,
                    })),
                    ..Metric::default()
                },
                Metric {
                    name: METRIC.to_owned(),
                    unit: "ms".to_owned(),
                    data: Some(Data::Histogram(Histogram {
                        data_points: vec![self.histogram(
                            route,
                            q,
                            start,
                            Self::export_time(j, series + 1),
                        )],
                        aggregation_temporality: AggregationTemporality::Cumulative as i32,
                    })),
                    ..Metric::default()
                },
            ]);
            series += 2;
        }
        let previous = Self::export_time(j - 1, 0);
        let delta_time = Self::export_time(j, series);
        let gauge_time = Self::export_time(j, series + 1);
        let cpu_time = Self::export_time(j, series + 2);
        resources.entry(metric_service(0)).or_default().extend([
            Metric {
                name: METRIC.to_owned(),
                unit: "1".to_owned(),
                data: Some(Data::Sum(Sum {
                    data_points: vec![number(
                        vec![text("route", "/batch")],
                        previous,
                        delta_time,
                        Number::AsInt(6),
                    )],
                    aggregation_temporality: AggregationTemporality::Delta as i32,
                    is_monotonic: true,
                })),
                ..Metric::default()
            },
            Metric {
                name: METRIC.to_owned(),
                unit: "By".to_owned(),
                data: Some(Data::Gauge(Gauge {
                    data_points: vec![number(
                        vec![text("route", "/gauge")],
                        0,
                        gauge_time,
                        Number::AsInt(GAUGE_BASE + 3 * j),
                    )],
                })),
                ..Metric::default()
            },
            Metric {
                name: CPU_METRIC.to_owned(),
                unit: "%".to_owned(),
                data: Some(Data::Gauge(Gauge {
                    data_points: vec![number(
                        Vec::new(),
                        0,
                        cpu_time,
                        Number::AsDouble(j as f64 / 4.0),
                    )],
                })),
                ..Metric::default()
            },
        ]);
        ExportMetricsServiceRequest {
            resource_metrics: resources
                .into_iter()
                .map(|(service, metrics)| ResourceMetrics {
                    resource: Some(resource(&service)),
                    scope_metrics: vec![ScopeMetrics {
                        scope: Some(scope()),
                        metrics,
                        ..ScopeMetrics::default()
                    }],
                    ..ResourceMetrics::default()
                })
                .collect(),
        }
    }

    /// OTLP export requests the whole journey sends: every span batch, then
    /// every log batch, then every metric export.
    #[must_use]
    pub fn exports(self) -> i64 {
        self.span_batches() + self.log_batches() + self.metric_batches()
    }

    /// Export `index` of [`Self::exports`].
    #[must_use]
    pub fn export(self, index: i64) -> Export {
        let logs = self.span_batches();
        let metrics = logs + self.log_batches();
        if index < logs {
            Export::Spans(self.spans_request(index))
        } else if index < metrics {
            Export::Logs(self.logs_request(index - logs))
        } else {
            Export::Metrics(self.metrics_request(index - metrics))
        }
    }

    /// Spans the journey exports.
    #[must_use]
    pub fn span_rows(self) -> i64 {
        (0..self.requests).map(spans).sum()
    }

    /// Log records the journey exports: one per request and one per error.
    #[must_use]
    pub fn log_rows(self) -> i64 {
        self.requests
            + (0..self.requests)
                .filter(|r| logs_error(*r))
                .count()
                .to_i64()
                .unwrap_or(i64::MAX)
    }

    /// Metric points the journey exports: per export, a counter and a
    /// histogram per route plus the delta, the integer gauge and the CPU
    /// gauge.
    #[must_use]
    pub const fn point_rows(self) -> i64 {
        EXPORTS * (2 * self.routes + 3)
    }

    /// Route `q`'s cumulative histogram point since `start` at `time`.
    fn histogram(self, route: Vec<KeyValue>, q: i64, start: i64, time: i64) -> HistogramDataPoint {
        let mut counts = vec![0_u64; BOUNDS.len() + 1];
        let mut sum = 0.0;
        let mut count = 0;
        for ms in self.durations(q, start, time) {
            counts[BOUNDS.iter().take_while(|bound| ms > **bound).count()] += 1;
            sum += ms;
            count += 1;
        }
        HistogramDataPoint {
            attributes: route,
            start_time_unix_nano: start.cast_unsigned(),
            time_unix_nano: time.cast_unsigned(),
            count,
            sum: Some(sum),
            bucket_counts: counts,
            explicit_bounds: BOUNDS.to_vec(),
            ..HistogramDataPoint::default()
        }
    }
}

/// One OTLP export request of a journey.
pub enum Export {
    /// A trace service export.
    Spans(ExportTraceServiceRequest),
    /// A logs service export.
    Logs(ExportLogsServiceRequest),
    /// A metrics service export.
    Metrics(ExportMetricsServiceRequest),
}

/// Item `n` of `items`, cycling.
fn pick(items: &[&'static str], n: i64) -> &'static str {
    items[usize::try_from(n.unsigned_abs()).unwrap_or(0) % items.len()]
}

/// The service exporting route `q`'s metrics.
#[must_use]
pub fn metric_service(q: i64) -> String {
    format!("metrics-{}", q % 2)
}

/// One number point.
fn number(attributes: Vec<KeyValue>, start: i64, time: i64, value: Number) -> NumberDataPoint {
    NumberDataPoint {
        attributes,
        start_time_unix_nano: start.cast_unsigned(),
        time_unix_nano: time.cast_unsigned(),
        value: Some(value),
        ..NumberDataPoint::default()
    }
}

/// The resource of `service`.
fn resource(service: &str) -> Resource {
    Resource {
        attributes: vec![
            text("service.name", service),
            text("service.version", "1.0.0"),
            text("deployment.environment.name", "dev"),
        ],
        ..Resource::default()
    }
}

/// The instrumentation scope of every export.
fn scope() -> InstrumentationScope {
    InstrumentationScope {
        name: "wyrd.bench.ui".to_owned(),
        version: "1.0.0".to_owned(),
        ..InstrumentationScope::default()
    }
}

#[cfg(test)]
mod tests {
    use num_traits::ToPrimitive;

    use super::{Journey, spans};

    /// Every span exports exactly once across the batches, roots pair up on
    /// start times, and every request falls inside the hour.
    ///
    /// # Panics
    ///
    /// Panics when a span is lost or duplicated, or a time leaves the hour.
    #[test]
    fn exports_cover_every_span_once_inside_the_hour() {
        for journey in [
            Journey::DEFAULT,
            Journey {
                requests: 5_000,
                routes: 30,
            },
        ] {
            let exported: usize = (0..journey.span_batches())
                .map(|b| {
                    journey
                        .spans_request(b)
                        .resource_spans
                        .iter()
                        .flat_map(|resource| &resource.scope_spans)
                        .map(|scope| scope.spans.len())
                        .sum::<usize>()
                })
                .sum();
            let expected: i64 = (0..journey.requests).map(spans).sum();
            assert_eq!(exported.to_i64(), Some(expected));
            assert_eq!(journey.start(0), journey.start(1));
            assert!(
                journey.start(journey.requests - 1) < super::HOUR_START_NANOS + super::HOUR_NANOS
            );
            assert_eq!(journey.started(None, i64::MIN, i64::MAX), journey.requests);
            let (mut logs, mut points) = (0, 0);
            for index in 0..journey.exports() {
                match journey.export(index) {
                    super::Export::Spans(_) => {}
                    super::Export::Logs(request) => {
                        logs += request
                            .resource_logs
                            .iter()
                            .flat_map(|resource| &resource.scope_logs)
                            .map(|scope| scope.log_records.len().to_i64().unwrap_or(i64::MAX))
                            .sum::<i64>();
                    }
                    super::Export::Metrics(request) => {
                        points += request
                            .resource_metrics
                            .iter()
                            .flat_map(|resource| &resource.scope_metrics)
                            .map(|scope| scope.metrics.len().to_i64().unwrap_or(i64::MAX))
                            .sum::<i64>();
                    }
                }
            }
            assert_eq!((logs, points), (journey.log_rows(), journey.point_rows()));
        }
    }

    /// Every fourth request's root is an agent invocation, so the
    /// invocation statistics count known spans instead of none.
    ///
    /// # Panics
    ///
    /// Panics when the exported invocations differ from every fourth root.
    #[test]
    fn every_fourth_root_invokes_an_agent() {
        let journey = Journey::DEFAULT;
        let invocations = (0..journey.span_batches())
            .flat_map(|b| journey.spans_request(b).resource_spans)
            .flat_map(|resource| resource.scope_spans)
            .flat_map(|scope| scope.spans)
            .filter(|span| {
                span.attributes.iter().any(|attribute| {
                    attribute.key == "gen_ai.operation.name"
                        && attribute
                            .value
                            .as_ref()
                            .and_then(|value| value.value.as_ref())
                            == Some(
                                &wyrd_tonic::otlp::common::v1::any_value::Value::StringValue(
                                    "invoke_agent".to_owned(),
                                ),
                            )
                })
            })
            .inspect(|span| assert!(span.parent_span_id.is_empty(), "an invocation is a root"))
            .count();
        assert_eq!(invocations, 125);
    }
}
