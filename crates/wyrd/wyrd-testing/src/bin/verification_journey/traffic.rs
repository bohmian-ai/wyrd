//! The open-loop application traffic: public clients emitting one Run
//! iteration at a time at the phase's offered rate.
//!
//! Each client owns one Service-authenticated `WyrdState` lifetime. An
//! iteration opens one Run, emits one Drift observation from the Model view
//! and one Eval observation from the Agent view through the state-owned
//! Bifrost queue, writes one custom dataset row, and exports one span, one
//! log record, and one gauge point over OTLP/HTTP. Every [`FAIL_EVERY`]th
//! iteration's Eval context answers `no`, so its deterministic task fails
//! and the binding's Operator is dispatched. Iterations are scheduled at
//! fixed instants from the phase start; a client that falls behind runs late
//! (counted) and whatever it has not started by the phase end is counted as
//! unsubmitted. Emits are enqueue-only, so evidence counts as acknowledged
//! only once the state's graceful shutdown drained every producer.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hdrhistogram::Histogram;
use secrecy::{ExposeSecret as _, SecretString};
use tokio::time::Instant;
use wyrd_client::observe::EvalObservationOptions;
use wyrd_client::state::WyrdState;
use wyrd_client::{QueueConfig, WyrdClient};
use wyrd_spec::error::WyrdError;
use wyrd_testing::release_server::SERVER_URL;
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics::v1::{
    Gauge, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, metric, number_data_point,
};
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::prost::Message as _;

use crate::Result;
use crate::tenant::{TABLE, Tenant};

/// Every this-many iterations of a client, the Eval context fails its task.
pub const FAIL_EVERY: u64 = 20;

/// Lateness past an iteration's scheduled instant that counts it late.
const LATE: Duration = Duration::from_millis(250);

/// Marker every Eval context carries, so a trace attribute holding evidence
/// contents is detectable.
pub const EVIDENCE_MARKER: &str = "bench-evidence-context";

/// One phase of the offered-load profile.
#[derive(Debug, Clone, Copy)]
pub struct Phase {
    /// Report name.
    pub name: &'static str,
    /// Wall-clock length.
    pub seconds: u64,
    /// Offered Run iterations per second across every client; zero drains.
    pub rate: u64,
}

/// The task's profile: warmup, steady, burst, then a drain with no arrivals.
pub const PROFILE: [Phase; 4] = [
    Phase {
        name: "warmup",
        seconds: 20,
        rate: 50,
    },
    Phase {
        name: "steady",
        seconds: 120,
        rate: 100,
    },
    Phase {
        name: "burst",
        seconds: 20,
        rate: 150,
    },
    Phase {
        name: "drain",
        seconds: 40,
        rate: 0,
    },
];

/// Evidence one client handed to the server, per signal.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Evidence {
    /// Drift observations, one per Run.
    pub drift: u64,
    /// Eval observations, one per Run.
    pub eval: u64,
    /// Eval observations whose context fails the deterministic task.
    pub failing_eval: u64,
    /// Custom dataset rows.
    pub custom: u64,
    /// OTLP spans.
    pub spans: u64,
    /// OTLP log records.
    pub logs: u64,
    /// OTLP gauge points.
    pub points: u64,
}

impl Evidence {
    /// Adds `other` signal by signal.
    pub fn add(&mut self, other: &Self) {
        self.drift += other.drift;
        self.eval += other.eval;
        self.failing_eval += other.failing_eval;
        self.custom += other.custom;
        self.spans += other.spans;
        self.logs += other.logs;
        self.points += other.points;
    }
}

/// What one phase of traffic did, merged across clients.
pub struct PhaseLoad {
    /// Iterations the profile scheduled.
    pub planned: u64,
    /// Iterations started before the phase ended.
    pub submitted: u64,
    /// Iterations started more than [`LATE`] after their instant.
    pub late: u64,
    /// Refused or failed steps by `step: code`.
    pub errors: BTreeMap<String, u64>,
    /// Client-measured iteration latency in microseconds: enqueue of both
    /// observations plus the awaited dataset write and three OTLP exports.
    pub latency_us: Histogram<u64>,
}

impl PhaseLoad {
    /// An empty tally.
    ///
    /// # Errors
    ///
    /// Returns the histogram construction failure.
    pub fn new() -> Result<Self> {
        Ok(Self {
            planned: 0,
            submitted: 0,
            late: 0,
            errors: BTreeMap::new(),
            latency_us: Histogram::new(3)?,
        })
    }

    /// Merges `other` into this tally.
    ///
    /// # Errors
    ///
    /// Returns the histogram merge failure.
    pub fn merge(&mut self, other: &Self) -> Result<()> {
        self.planned += other.planned;
        self.submitted += other.submitted;
        self.late += other.late;
        for (code, count) in &other.errors {
            *self.errors.entry(code.clone()).or_default() += count;
        }
        self.latency_us.add(&other.latency_us)?;
        Ok(())
    }

    /// Counts one failed step.
    fn fault(&mut self, step: &str, code: &str) {
        *self.errors.entry(format!("{step}: {code}")).or_default() += 1;
    }
}

/// How one client's graceful shutdown went.
pub struct Flush {
    /// Seconds the drain took.
    pub seconds: f64,
    /// The drain failure, when it failed.
    pub error: Option<String>,
}

/// One application client of one tenant.
pub struct Client {
    /// Index of the tenant in the benchmark's tenant list.
    pub tenant: usize,
    /// Position among every client, staggering the schedule.
    slot: usize,
    /// The Service-authenticated state lifetime.
    state: WyrdState,
    /// OTLP/HTTP transport.
    http: reqwest::Client,
    /// The Service bearer an OTLP exporter sends.
    bearer: SecretString,
    /// This client's OTLP `service.instance.id`.
    ///
    /// Both clients of a tenant share one principal, and Bifrost identifies an
    /// OTLP export by its content so an exporter's retry converges. Distinct
    /// emitters must therefore say so, as OpenTelemetry's single-writer rule
    /// requires, or two same-instant records collapse into one.
    instance: String,
    /// Iterations this client has started.
    sequence: u64,
    /// Evidence handed to the server so far.
    pub evidence: Evidence,
}

impl Client {
    /// Loads `tenant`'s bundle, starts its Bifrost lifetime on a fresh
    /// Service client (exchanging the Card-bound key), and reads the bearer
    /// OTLP exports present.
    ///
    /// # Errors
    ///
    /// Returns a bundle, client, startup, or exchange failure.
    pub async fn start(tenant: &Tenant, index: usize, slot: usize) -> Result<Self> {
        let client: WyrdClient = tenant.service_client()?;
        let state = WyrdState::from_path(&tenant.bundle)?;
        state
            .start_bifrost_with_config(&client, None, QueueConfig::default())
            .await?;
        let bearer = SecretString::from(client.auth().bearer().await?.expose().to_owned());
        Ok(Self {
            tenant: index,
            slot,
            state,
            http: reqwest::Client::new(),
            bearer,
            instance: uuid::Uuid::new_v4().to_string(),
            sequence: 0,
            evidence: Evidence::default(),
        })
    }

    /// Runs this client's share of `phase` among `clients`, from `start`.
    ///
    /// # Errors
    ///
    /// Returns the tally construction or latency-recording failure; traffic
    /// failures are counted.
    pub async fn drive(
        &mut self,
        phase: Phase,
        clients: usize,
        start: Instant,
    ) -> Result<PhaseLoad> {
        let mut load = PhaseLoad::new()?;
        let total = phase.rate * phase.seconds;
        let clients_u64 = clients as u64;
        let slot = self.slot as u64;
        load.planned = total / clients_u64 + u64::from(slot < total % clients_u64);
        if load.planned == 0 {
            return Ok(load);
        }
        let per_client = phase.rate as f64 / clients as f64;
        let end = start + Duration::from_secs(phase.seconds);
        for k in 0..load.planned {
            let due = start
                + Duration::from_secs_f64(
                    (k as f64 + self.slot as f64 / clients as f64) / per_client,
                );
            tokio::time::sleep_until(due).await;
            let now = Instant::now();
            if now >= end {
                break;
            }
            if now > due + LATE {
                load.late += 1;
            }
            load.submitted += 1;
            self.iterate(&mut load).await;
            let micros = u64::try_from(now.elapsed().as_micros()).unwrap_or(u64::MAX);
            load.latency_us.record(micros)?;
        }
        Ok(load)
    }

    /// Drains every producer through the state's graceful shutdown.
    pub async fn finish(&self) -> Flush {
        let started = std::time::Instant::now();
        let error = self
            .state
            .shutdown()
            .await
            .err()
            .map(|error| error.to_string());
        Flush {
            seconds: started.elapsed().as_secs_f64(),
            error,
        }
    }

    /// Emits one Run iteration, counting each step's evidence or failure.
    async fn iterate(&mut self, load: &mut PhaseLoad) {
        let sequence = self.sequence;
        self.sequence += 1;
        let failing = sequence.is_multiple_of(FAIL_EVERY);
        let run = self.state.run();
        let drift = run.for_card("model").and_then(|model| {
            model.observe().drift(
                &serde_json::json!({ "latency": 100.0 + (sequence % 50) as f64, "score": 1.2 }),
                None,
            )
        });
        if counted(load, "drift", drift) {
            self.evidence.drift += 1;
        }
        let context = serde_json::json!({
            "question": EVIDENCE_MARKER,
            "answer": if failing { "no" } else { "yes" },
        });
        let eval = run.for_card("agent").and_then(|agent| {
            agent
                .observe()
                .eval(&context, EvalObservationOptions::default())
        });
        if counted(load, "eval", eval) {
            self.evidence.eval += 1;
            self.evidence.failing_eval += u64::from(failing);
        }
        let custom = run
            .observe()
            .record(TABLE, &serde_json::json!({ "sequence": sequence }))
            .await;
        if counted(load, "custom", custom) {
            self.evidence.custom += 1;
        }
        let now = unix_nanos();
        let bodies = [
            ("/v1/traces", trace_export(now, &self.instance)),
            ("/v1/logs", log_export(now, &self.instance)),
            ("/v1/metrics", metric_export(now, &self.instance)),
        ];
        for (path, body) in bodies {
            if self.export(load, path, body).await {
                match path {
                    "/v1/traces" => self.evidence.spans += 1,
                    "/v1/logs" => self.evidence.logs += 1,
                    _ => self.evidence.points += 1,
                }
            }
        }
    }

    /// Posts one OTLP/HTTP protobuf export, returning whether it was accepted.
    async fn export(&self, load: &mut PhaseLoad, path: &str, body: Vec<u8>) -> bool {
        let response = self
            .http
            .post(format!("{SERVER_URL}{path}"))
            .header("content-type", "application/x-protobuf")
            .header(
                "x-wyrd-access-token",
                format!("Bearer {}", self.bearer.expose_secret()),
            )
            .body(body)
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => true,
            Ok(response) => {
                load.fault(path, response.status().as_str());
                false
            }
            Err(_) => {
                load.fault(path, "transport");
                false
            }
        }
    }
}

/// Records `result`'s failure under `step`, returning whether it succeeded.
fn counted(load: &mut PhaseLoad, step: &str, result: std::result::Result<(), WyrdError>) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            load.fault(step, error.code());
            false
        }
    }
}

/// Nanoseconds since the Unix epoch.
fn unix_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
        })
}

/// The resource every export of the client `instance` names.
fn resource(instance: &str) -> Option<Resource> {
    let attribute = |key: &str, value: &str| KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_owned())),
        }),
    };
    Some(Resource {
        attributes: vec![
            attribute("service.name", "verification-bench"),
            attribute("service.instance.id", instance),
        ],
        ..Resource::default()
    })
}

/// One span with fresh random identity from `instance`, encoded as OTLP protobuf.
fn trace_export(now: u64, instance: &str) -> Vec<u8> {
    let identity = uuid::Uuid::new_v4();
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: resource(instance),
            scope_spans: vec![ScopeSpans {
                spans: vec![Span {
                    trace_id: identity.as_bytes().to_vec(),
                    span_id: identity.as_bytes()[..8].to_vec(),
                    name: "bench.iteration".to_owned(),
                    start_time_unix_nano: now.saturating_sub(1_000_000),
                    end_time_unix_nano: now,
                    ..Span::default()
                }],
                ..ScopeSpans::default()
            }],
            ..ResourceSpans::default()
        }],
    }
    .encode_to_vec()
}

/// One log record from `instance`, encoded as OTLP protobuf.
fn log_export(now: u64, instance: &str) -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: resource(instance),
            scope_logs: vec![ScopeLogs {
                log_records: vec![LogRecord {
                    time_unix_nano: now,
                    observed_time_unix_nano: now,
                    severity_text: "INFO".to_owned(),
                    body: Some(AnyValue {
                        value: Some(any_value::Value::StringValue("iteration".to_owned())),
                    }),
                    ..LogRecord::default()
                }],
                ..ScopeLogs::default()
            }],
            ..ResourceLogs::default()
        }],
    }
    .encode_to_vec()
}

/// One gauge point from `instance`, encoded as OTLP protobuf.
fn metric_export(now: u64, instance: &str) -> Vec<u8> {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: resource(instance),
            scope_metrics: vec![ScopeMetrics {
                metrics: vec![Metric {
                    name: "bench.iteration.score".to_owned(),
                    data: Some(metric::Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint {
                            time_unix_nano: now,
                            value: Some(number_data_point::Value::AsDouble(1.2)),
                            ..NumberDataPoint::default()
                        }],
                    })),
                    ..Metric::default()
                }],
                ..ScopeMetrics::default()
            }],
            ..ResourceMetrics::default()
        }],
    }
    .encode_to_vec()
}
