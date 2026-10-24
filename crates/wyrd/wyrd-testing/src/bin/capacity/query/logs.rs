//! The `bifrost.logs.records` bulky-log fixture: deterministic Kubernetes
//! application logs sent through OTLP log ingest.
//!
//! Each record is what a cluster's log collector ships for one structured
//! application line: Kubernetes resource attributes, a handful of record
//! attributes, and a JSON body of at least [`BODY_BYTES`]. Every value is a
//! plain function of one record index, as in the other fixtures, so a run is
//! reproducible and distinct indexes never derive the same export batch.
//! Records spread evenly over one day of `time_unix_nano`, the table's time
//! column.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use num_traits::ToPrimitive;
use wyrd_tonic::otlp::common::v1::any_value::Value;
use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::resource::v1::Resource;

use crate::query::telemetry::{attribute, text};
use crate::query::workload::{DAY_HOURS, DAY_START_MS, HOUR_MS, mix};

/// Smallest body of every record, in bytes: a bulky structured line.
pub const BODY_BYTES: usize = 1_024;

/// Records per OTLP export request: the OpenTelemetry SDK batch log
/// processor's default `max_export_batch_size`.
pub const REQUEST_RECORDS: i64 = 512;

/// Distinct pods emitting logs; each is one OTLP resource.
const PODS: u64 = 96;

/// Words the body's free-text detail is drawn from.
const WORDS: [&str; 16] = [
    "upstream",
    "connection",
    "reused",
    "cache",
    "miss",
    "retrying",
    "after",
    "timeout",
    "payload",
    "validated",
    "against",
    "schema",
    "tenant",
    "quota",
    "exceeded",
    "resolved",
];

/// `time_unix_nano` of record zero, matching the other fixtures' day.
const DAY_START_NANOS: i64 = DAY_START_MS * 1_000_000;

/// One bulky-log fixture size and every record derived from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Logs {
    /// Records the fixture spreads over its day.
    pub records: i64,
}

impl Logs {
    /// `time_unix_nano` of record `index`: records spread evenly over one
    /// day; indexes past [`Self::records`] continue into the following days.
    #[must_use]
    pub const fn time_nanos(self, index: i64) -> i64 {
        DAY_START_NANOS + index * (DAY_HOURS * HOUR_MS * 1_000_000 / self.records)
    }

    /// The OTLP export request carrying records `first..last`, one
    /// `ResourceLogs` per pod as a node's collector batches them.
    #[must_use]
    pub fn request(self, first: i64, last: i64) -> ExportLogsServiceRequest {
        let mut by_pod: BTreeMap<u64, Vec<LogRecord>> = BTreeMap::new();
        for index in first..last {
            by_pod
                .entry(pod(index))
                .or_default()
                .push(self.record(index));
        }
        ExportLogsServiceRequest {
            resource_logs: by_pod
                .into_iter()
                .map(|(pod, log_records)| ResourceLogs {
                    resource: Some(pod_resource(pod)),
                    scope_logs: vec![ScopeLogs {
                        scope: Some(InstrumentationScope {
                            name: "wyrd.bench.logs".to_owned(),
                            version: "1.0.0".to_owned(),
                            ..InstrumentationScope::default()
                        }),
                        log_records,
                        ..ScopeLogs::default()
                    }],
                    ..ResourceLogs::default()
                })
                .collect(),
        }
    }

    /// Record `index`: severity, a few attributes, and its bulky JSON body.
    fn record(self, index: i64) -> LogRecord {
        let draw = mix(index.cast_unsigned());
        let status = [200, 200, 200, 201, 204, 304, 404, 500][(draw % 8) as usize];
        let (severity_number, severity_text) = match status {
            500 => (17, "ERROR"),
            404 => (13, "WARN"),
            _ => (9, "INFO"),
        };
        let time = self.time_nanos(index).cast_unsigned();
        LogRecord {
            time_unix_nano: time,
            observed_time_unix_nano: time + 1_000_000,
            severity_number,
            severity_text: severity_text.to_owned(),
            body: Some(AnyValue {
                value: Some(Value::StringValue(body(index, draw, status))),
            }),
            attributes: vec![
                text("log.iostream", "stdout"),
                text(
                    "log.file.path",
                    format!("/var/log/pods/app_{}/api/0.log", pod(index)),
                ),
                text(
                    "http.request.method",
                    ["GET", "POST", "PUT"][(draw % 3) as usize],
                ),
                attribute("http.response.status_code", Value::IntValue(status)),
            ],
            ..LogRecord::default()
        }
    }
}

/// The pod that emits record `index`.
const fn pod(index: i64) -> u64 {
    mix(index.cast_unsigned() ^ 0x5eed) % PODS
}

/// The Kubernetes resource attributes of `pod`.
fn pod_resource(pod: u64) -> Resource {
    Resource {
        attributes: vec![
            text("service.name", format!("svc-{:02}", pod % 12)),
            text("k8s.namespace.name", format!("team-{}", pod % 4)),
            text("k8s.deployment.name", format!("svc-{:02}", pod % 12)),
            text(
                "k8s.pod.name",
                format!("svc-{:02}-7d9f8c-{pod:05x}", pod % 12),
            ),
            text("k8s.container.name", "api"),
            text("k8s.node.name", format!("node-{}", pod % 8)),
            text("cloud.region", "us-east-1"),
        ],
        ..Resource::default()
    }
}

/// The JSON body of record `index`: an access line padded with free-text
/// detail to at least [`BODY_BYTES`].
fn body(index: i64, draw: u64, status: i64) -> String {
    let mut line = String::with_capacity(BODY_BYTES + 64);
    let _ = write!(
        line,
        r#"{{"level":"info","logger":"api.http","msg":"request completed","method":"GET","path":"/api/v1/orders/{order}","status":{status},"latency_ms":{latency}.{fraction:03},"bytes":{bytes},"request_id":"{request:016x}{index:016x}","user_id":"u-{user}","client_ip":"10.{a}.{b}.{c}","user_agent":"Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36","detail":""#,
        order = draw % 1_000_000,
        latency = draw % 900,
        fraction = (draw >> 10) % 1_000,
        bytes = (draw >> 20) % 65_536,
        request = mix(draw),
        user = (draw >> 30) % 200_000,
        a = (draw >> 40) % 256,
        b = (draw >> 48) % 256,
        c = (draw >> 56) % 256,
    );
    let mut word = draw;
    while line.len() < BODY_BYTES {
        word = mix(word);
        line.push_str(WORDS[(word % WORDS.len() as u64).to_usize().unwrap_or(0)]);
        line.push(' ');
    }
    line.push_str(r#""}"#);
    line
}

#[cfg(test)]
mod tests {
    use super::{BODY_BYTES, Logs, REQUEST_RECORDS};
    use num_traits::ToPrimitive;
    use wyrd_tonic::otlp::common::v1::any_value::Value;

    /// Every record's body is valid JSON of at least [`BODY_BYTES`], and a
    /// request carries exactly the records it names.
    ///
    /// # Panics
    ///
    /// Panics when a body is short or not JSON, or a record is missing.
    #[test]
    fn requests_carry_bulky_json_bodies() {
        let request = Logs { records: 1_000_000 }.request(0, REQUEST_RECORDS);
        let records: Vec<_> = request
            .resource_logs
            .iter()
            .flat_map(|resource| &resource.scope_logs)
            .flat_map(|scope| &scope.log_records)
            .collect();

        assert_eq!(records.len().to_i64(), Some(REQUEST_RECORDS));
        for record in records {
            let Some(Value::StringValue(body)) =
                record.body.as_ref().and_then(|body| body.value.as_ref())
            else {
                panic!("every body is a string");
            };
            assert!(body.len() >= BODY_BYTES, "{} bytes", body.len());
            serde_json::from_str::<serde_json::Value>(body).expect("the body is JSON");
        }
    }
}
