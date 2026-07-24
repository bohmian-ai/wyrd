//! Real OTLP span-ingest capacity benchmark.
//!
//! This lane drives the public OTLP/gRPC `TraceService::Export` surface through
//! Gate and Scribe. It intentionally reports the admission ACK separately from
//! the post-run flush and query checks: an export response is not, by itself, a
//! proof that the span is durable or queryable.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::Row;
use tokio::task::JoinSet;
use wyrd_bench::LatencyPercentiles;
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster};
use wyrd_testing::otlp::RandomTraceGenerator;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::prost::Message;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::GetTraceRequest;
use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;

type BenchError = Box<dyn Error + Send + Sync>;

const DEFAULT_PODS: usize = 3;
const DEFAULT_TENANTS: usize = 10;
const DEFAULT_TARGET_SPANS_PER_SECOND: u64 = 10_000;
const DEFAULT_SPANS_PER_REQUEST: usize = 100;
const DEFAULT_WARMUP_SECONDS: u64 = 5;
const DEFAULT_MEASURED_SECONDS: u64 = 60;
const DEFAULT_MAX_IN_FLIGHT: usize = 256;
const DEFAULT_VERIFY_SAMPLES: usize = 3;
const MAX_RETAINED_SAMPLE_TRACES: usize = 16;

#[derive(Debug, Clone)]
struct Config {
    pods: usize,
    tenants: usize,
    target_spans_per_second: u64,
    spans_per_request: usize,
    warmup_seconds: u64,
    measured_seconds: u64,
    max_in_flight: usize,
    verify_samples: usize,
}

#[derive(Clone)]
struct Endpoint {
    tenant_index: usize,
    tenant: DataTenantId,
    pod_index: usize,
    channel: Arc<Channel>,
    jwt: Arc<str>,
}

#[derive(Debug)]
struct ExportResult {
    tenant_index: usize,
    pod_index: usize,
    trace_id: [u8; 16],
    latency_us: u64,
    accepted_spans: u64,
    rejected_spans: u64,
    error: Option<String>,
}

#[derive(Debug, Default)]
struct PhaseStats {
    scheduled_requests: u64,
    offered_spans: u64,
    offered_bytes: u64,
    completed_requests: u64,
    accepted_spans: u64,
    rejected_spans: u64,
    failed_requests: u64,
    schedule_misses: u64,
    max_in_flight: u64,
    elapsed_ms: u64,
    ack_latencies_us: Vec<u64>,
    errors: BTreeMap<String, u64>,
    tenant_accepted: BTreeMap<usize, u64>,
    pod_accepted: BTreeMap<usize, u64>,
    sample_traces: BTreeMap<usize, Vec<[u8; 16]>>,
}

impl PhaseStats {
    fn record(&mut self, result: ExportResult, retain_samples: bool) {
        self.completed_requests = self.completed_requests.saturating_add(1);
        self.ack_latencies_us.push(result.latency_us);
        self.tenant_accepted
            .entry(result.tenant_index)
            .and_modify(|value| *value = value.saturating_add(result.accepted_spans))
            .or_insert(result.accepted_spans);
        self.pod_accepted
            .entry(result.pod_index)
            .and_modify(|value| *value = value.saturating_add(result.accepted_spans))
            .or_insert(result.accepted_spans);
        self.accepted_spans = self.accepted_spans.saturating_add(result.accepted_spans);
        self.rejected_spans = self.rejected_spans.saturating_add(result.rejected_spans);

        if retain_samples && result.error.is_none() {
            let samples = self.sample_traces.entry(result.tenant_index).or_default();
            if samples.len() < MAX_RETAINED_SAMPLE_TRACES {
                samples.push(result.trace_id);
            }
        }

        if let Some(error) = result.error {
            self.failed_requests = self.failed_requests.saturating_add(1);
            self.errors
                .entry(error)
                .and_modify(|value| *value = value.saturating_add(1))
                .or_insert(1);
        }
    }
}

#[derive(Debug, Serialize)]
struct Report {
    schema_version: u32,
    lane: &'static str,
    ack_boundary: &'static str,
    durability_boundary: &'static str,
    queryability_boundary: &'static str,
    config: ReportConfig,
    prewarm_accepted_spans: u64,
    warmup: PhaseReport,
    measurement: PhaseReport,
    verification: VerificationReport,
    readiness: &'static str,
    notes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct ReportConfig {
    pods: usize,
    tenants: usize,
    target_spans_per_second: u64,
    spans_per_request: usize,
    warmup_seconds: u64,
    measured_seconds: u64,
    max_in_flight: usize,
    verify_samples_per_tenant: usize,
    span_shape: &'static str,
}

#[derive(Debug, Serialize)]
struct PhaseReport {
    elapsed_ms: u64,
    target_spans_per_second: u64,
    scheduled_requests: u64,
    completed_requests: u64,
    offered_spans: u64,
    accepted_spans: u64,
    rejected_spans: u64,
    failed_requests: u64,
    offered_bytes: u64,
    offered_spans_per_second: f64,
    accepted_spans_per_second: f64,
    offered_mib_per_second: f64,
    ack_latency_us: LatencyPercentiles,
    schedule_misses: u64,
    max_in_flight: u64,
    errors: BTreeMap<String, u64>,
    tenant_accepted_spans: BTreeMap<String, u64>,
    pod_accepted_spans: BTreeMap<String, u64>,
}

#[derive(Debug, Default, Serialize)]
struct VerificationReport {
    flush_elapsed_ms: u64,
    expected_accepted_spans: u64,
    file_list_rows: u64,
    file_list_rows_match: bool,
    query_samples_requested: u64,
    query_samples_found: u64,
    query_spans_found: u64,
    query_latency_us: LatencyPercentiles,
    query_errors: Vec<String>,
    passed: bool,
}

#[tokio::main]
async fn main() -> Result<(), BenchError> {
    let config = Config::from_args()?;
    let report = run(config).await?;
    emit_report(&report)?;
    Ok(())
}

impl Config {
    fn from_args() -> Result<Self, BenchError> {
        let config = Self {
            pods: value("pods", "WYRD_OTLP_PODS", DEFAULT_PODS)?,
            tenants: value("tenants", "WYRD_OTLP_TENANTS", DEFAULT_TENANTS)?,
            target_spans_per_second: value(
                "target-spans-per-second",
                "WYRD_OTLP_TARGET_SPANS_PER_SECOND",
                DEFAULT_TARGET_SPANS_PER_SECOND,
            )?,
            spans_per_request: value(
                "spans-per-request",
                "WYRD_OTLP_SPANS_PER_REQUEST",
                DEFAULT_SPANS_PER_REQUEST,
            )?,
            warmup_seconds: value(
                "warmup-seconds",
                "WYRD_OTLP_WARMUP_SECONDS",
                DEFAULT_WARMUP_SECONDS,
            )?,
            measured_seconds: value(
                "measured-seconds",
                "WYRD_OTLP_MEASURED_SECONDS",
                DEFAULT_MEASURED_SECONDS,
            )?,
            max_in_flight: value(
                "max-in-flight",
                "WYRD_OTLP_MAX_IN_FLIGHT",
                DEFAULT_MAX_IN_FLIGHT,
            )?,
            verify_samples: value(
                "verify-samples",
                "WYRD_OTLP_VERIFY_SAMPLES",
                DEFAULT_VERIFY_SAMPLES,
            )?,
        };
        if !matches!(config.pods, 1 | 3) {
            return Err("OTLP benchmark supports one or three pods".into());
        }
        if config.tenants == 0
            || config.target_spans_per_second == 0
            || config.spans_per_request == 0
            || config.measured_seconds == 0
            || config.max_in_flight == 0
        {
            return Err("tenants, target rate, batch size, measured duration, and max in-flight must be positive".into());
        }
        Ok(config)
    }
}

fn value<T>(argument_name: &str, env_name: &str, default: T) -> Result<T, BenchError>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
{
    let argument_prefix = format!("--{argument_name}=");
    let raw = std::env::args()
        .find_map(|argument| argument.strip_prefix(&argument_prefix).map(str::to_owned))
        .or_else(|| std::env::var(env_name).ok());
    raw.map_or(Ok(default), |raw| {
        raw.parse::<T>()
            .map_err(|error| format!("invalid {argument_name}={raw}: {error}").into())
    })
}

async fn run(config: Config) -> Result<Report, BenchError> {
    let topology = if config.pods == 1 {
        BifrostTopology::OnePod
    } else {
        BifrostTopology::ThreePod
    };
    let cluster = WyrdTestCluster::start(config.pods, topology).await?;
    let result = run_on_cluster(&cluster, &config).await;
    let shutdown_result = cluster.shutdown().await;
    match (result, shutdown_result) {
        (Ok(report), Ok(())) => Ok(report),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(error), Err(shutdown_error)) => Err(format!(
            "benchmark failed: {error}; cluster shutdown failed: {shutdown_error}"
        )
        .into()),
    }
}

async fn run_on_cluster(cluster: &WyrdTestCluster, config: &Config) -> Result<Report, BenchError> {
    let endpoints = provision_endpoints(cluster, config).await?;
    let prewarm_accepted_spans = prewarm_endpoints(&endpoints).await?;
    flush_tenants(cluster, &endpoints).await?;
    let mut warmup = PhaseStats::default();
    if config.warmup_seconds > 0 {
        warmup = run_phase(
            &endpoints,
            config,
            Duration::from_secs(config.warmup_seconds),
            0,
            false,
        )
        .await?;
    }
    let measurement = run_phase(
        &endpoints,
        config,
        Duration::from_secs(config.measured_seconds),
        1_000_000_000,
        true,
    )
    .await?;
    let verification = verify_run(
        cluster,
        &endpoints,
        &warmup,
        &measurement,
        prewarm_accepted_spans,
        config,
    )
    .await?;

    let readiness = if measurement.failed_requests == 0
        && measurement.rejected_spans == 0
        && verification.passed
    {
        "ready"
    } else {
        "not_ready"
    };
    Ok(Report {
        schema_version: 1,
        lane: "otlp-span-ingest",
        ack_boundary: "Gate accepted the OTLP export and Scribe admitted the projected frame",
        durability_boundary: "post-run Scribe flush committed file-list rows; WAL fsync timing is not the client ACK",
        queryability_boundary: "sampled ValaQueryService.GetTrace after flushing every pod",
        config: ReportConfig {
            pods: config.pods,
            tenants: config.tenants,
            target_spans_per_second: config.target_spans_per_second,
            spans_per_request: config.spans_per_request,
            warmup_seconds: config.warmup_seconds,
            measured_seconds: config.measured_seconds,
            max_in_flight: config.max_in_flight,
            verify_samples_per_tenant: config.verify_samples,
            span_shape: "OTLP ResourceSpans with scope, parent/child spans, attributes, events, links, and status",
        },
        prewarm_accepted_spans,
        warmup: phase_report(&warmup, config),
        measurement: phase_report(&measurement, config),
        verification,
        readiness,
        notes: vec![
            "This is an open-loop bounded load test; the target is offered spans per second, not a server limit.",
            "Accepted spans per second is measured from successful OTLP responses and excludes rejected or failed requests.",
            "Forge and Oracle are not required for this Gate-to-Scribe ingest lane; their end-to-end stages need separate read/compaction benchmarks.",
        ],
    })
}

async fn provision_endpoints(
    cluster: &WyrdTestCluster,
    config: &Config,
) -> Result<Vec<Endpoint>, BenchError> {
    let bootstrap_server = cluster.server(0).ok_or("missing bootstrap pod")?;
    let mut channels = Vec::with_capacity(config.pods);
    for server in cluster.servers() {
        let url = server.grpc_url().ok_or("missing gRPC endpoint")?;
        let channel = Channel::from_shared(url)
            .map_err(|error| format!("invalid gRPC endpoint: {error}"))?
            .connect()
            .await?;
        channels.push(Arc::new(channel));
    }

    let mut endpoints = Vec::with_capacity(config.tenants.saturating_mul(config.pods));
    for tenant_index in 0..config.tenants {
        let tenant = if tenant_index == 0 {
            cluster.data_tenant_id()
        } else {
            cluster
                .add_tenant(&format!("bench-otlp-tenant-{tenant_index}"))
                .await?
        };
        let bootstrap = bootstrap_server
            .bootstrap_service_in_tenant(
                tenant,
                &format!("bench-otlp-writer-{tenant_index}"),
                &["admin"],
            )
            .await?;
        let api_key = match bootstrap {
            Bootstrap::Machine { api_key, .. } => api_key,
            Bootstrap::User { .. } => return Err("OTLP benchmark bootstrap returned a user".into()),
        };
        let client = WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: bootstrap_server
                    .grpc_url()
                    .ok_or("missing bootstrap gRPC endpoint")?,
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: bootstrap_server
                    .base_url()
                    .ok_or("missing bootstrap HTTP endpoint")?
                    .to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(api_key),
            ..ClientConfig::default()
        })?;
        let jwt: Arc<str> = client.auth().bearer().await?.expose().to_owned().into();
        for (pod_index, channel) in channels.iter().enumerate() {
            endpoints.push(Endpoint {
                tenant_index,
                tenant,
                pod_index,
                channel: Arc::clone(channel),
                jwt: Arc::clone(&jwt),
            });
        }
    }
    Ok(endpoints)
}

async fn run_phase(
    endpoints: &[Endpoint],
    config: &Config,
    duration: Duration,
    sequence_base: u64,
    retain_samples: bool,
) -> Result<PhaseStats, BenchError> {
    let request_period = request_period(config.target_spans_per_second, config.spans_per_request);
    let mut interval = tokio::time::interval(request_period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let started = Instant::now();
    let deadline = started + duration;
    let mut sequence = sequence_base;
    let mut next_endpoint = 0usize;
    let mut generator = RandomTraceGenerator::from_seed(sequence_base ^ 0x9e37_79b9_7f4a_7c15);
    let mut tasks = JoinSet::new();
    let mut stats = PhaseStats::default();

    while Instant::now() < deadline {
        interval.tick().await;
        if Instant::now() >= deadline {
            break;
        }
        if tasks.len() >= config.max_in_flight {
            stats.schedule_misses = stats.schedule_misses.saturating_add(1);
            if let Some(result) = tasks.join_next().await {
                stats.record(result??, retain_samples);
            }
        }
        let endpoint = endpoints
            .get(next_endpoint % endpoints.len())
            .ok_or("endpoint scheduler selected an invalid endpoint")?
            .clone();
        next_endpoint = next_endpoint.wrapping_add(1);
        let request =
            generator.export_request(endpoint.tenant_index, sequence, config.spans_per_request);
        let trace_id = first_trace_id(&request).ok_or("generated OTLP request had no trace")?;
        let offered_spans = u64::try_from(config.spans_per_request)?;
        let offered_bytes = u64::try_from(request.encoded_len())?;
        stats.scheduled_requests = stats.scheduled_requests.saturating_add(1);
        stats.offered_spans = stats.offered_spans.saturating_add(offered_spans);
        stats.offered_bytes = stats.offered_bytes.saturating_add(offered_bytes);
        stats.max_in_flight = stats.max_in_flight.max(u64::try_from(tasks.len() + 1)?);
        tasks.spawn(export(endpoint, request, trace_id, offered_spans));
        sequence = sequence.wrapping_add(1);
    }
    stats.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    while let Some(result) = tasks.join_next().await {
        stats.record(result??, retain_samples);
    }
    Ok(stats)
}

async fn prewarm_endpoints(endpoints: &[Endpoint]) -> Result<u64, BenchError> {
    let mut generator = RandomTraceGenerator::from_seed(0x51a7_7e11_0b5e_edd1);
    let mut accepted_spans = 0_u64;
    for endpoint in endpoints {
        let request = generator.export_request(endpoint.tenant_index, 0, 1);
        let trace_id = first_trace_id(&request).ok_or("prewarm request had no trace")?;
        let result = export(endpoint.clone(), request, trace_id, 1).await?;
        if let Some(error) = result.error {
            return Err(format!(
                "prewarm failed for tenant={} pod={}: {error}",
                endpoint.tenant_index, endpoint.pod_index
            )
            .into());
        }
        accepted_spans = accepted_spans.saturating_add(result.accepted_spans);
    }
    if accepted_spans != u64::try_from(endpoints.len())? {
        return Err(format!(
            "prewarm accepted {accepted_spans} spans for {} endpoints",
            endpoints.len()
        )
        .into());
    }
    Ok(accepted_spans)
}

async fn flush_tenants(
    cluster: &WyrdTestCluster,
    endpoints: &[Endpoint],
) -> Result<(), BenchError> {
    let mut tenant_ids = BTreeMap::new();
    for endpoint in endpoints {
        tenant_ids.insert(endpoint.tenant_index, endpoint.tenant);
    }
    for tenant in tenant_ids.values() {
        for server in cluster.servers() {
            server.flush_bifrost_for_tenant(*tenant).await?;
        }
    }
    Ok(())
}

async fn export(
    endpoint: Endpoint,
    request: ExportTraceServiceRequest,
    trace_id: [u8; 16],
    offered_spans: u64,
) -> Result<ExportResult, BenchError> {
    let started = Instant::now();
    let mut client = TraceServiceClient::new(endpoint.channel.as_ref().clone());
    let mut request = Request::new(request);
    let token = format!("Bearer {}", endpoint.jwt);
    let token = token
        .parse()
        .map_err(|error| format!("invalid access-token metadata: {error}"))?;
    request.metadata_mut().insert("x-wyrd-access-token", token);
    let response = client.export(request).await;
    let latency_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    match response {
        Ok(response) => {
            let rejected_spans = response
                .into_inner()
                .partial_success
                .map(|partial| u64::try_from(partial.rejected_spans).unwrap_or(0))
                .unwrap_or(0);
            Ok(ExportResult {
                tenant_index: endpoint.tenant_index,
                pod_index: endpoint.pod_index,
                trace_id,
                latency_us,
                accepted_spans: offered_spans.saturating_sub(rejected_spans),
                rejected_spans,
                error: None,
            })
        }
        Err(error) => Ok(ExportResult {
            tenant_index: endpoint.tenant_index,
            pod_index: endpoint.pod_index,
            trace_id,
            latency_us,
            accepted_spans: 0,
            rejected_spans: 0,
            error: Some(format!("{}: {}", error.code(), error.message())),
        }),
    }
}

async fn verify_run(
    cluster: &WyrdTestCluster,
    endpoints: &[Endpoint],
    warmup: &PhaseStats,
    measurement: &PhaseStats,
    prewarm_accepted_spans: u64,
    config: &Config,
) -> Result<VerificationReport, BenchError> {
    let started = Instant::now();
    let mut tenant_ids = BTreeMap::new();
    for endpoint in endpoints {
        tenant_ids.insert(endpoint.tenant_index, endpoint.tenant);
    }
    flush_tenants(cluster, endpoints).await?;

    let mut query_latencies = Vec::new();
    let mut verification = VerificationReport {
        flush_elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        expected_accepted_spans: prewarm_accepted_spans
            .saturating_add(warmup.accepted_spans)
            .saturating_add(measurement.accepted_spans),
        ..VerificationReport::default()
    };
    for (tenant_index, trace_ids) in &measurement.sample_traces {
        let endpoint = endpoints
            .iter()
            .find(|endpoint| endpoint.tenant_index == *tenant_index && endpoint.pod_index == 0)
            .ok_or("missing query endpoint")?;
        let mut query = ValaQueryServiceClient::new(endpoint.channel.as_ref().clone());
        for trace_id in trace_ids.iter().take(config.verify_samples) {
            verification.query_samples_requested =
                verification.query_samples_requested.saturating_add(1);
            let query_started = Instant::now();
            let mut request = Request::new(GetTraceRequest {
                window: None,
                trace_id: hex16(trace_id),
            });
            let token = format!("Bearer {}", endpoint.jwt)
                .parse()
                .map_err(|error| format!("invalid query-token metadata: {error}"))?;
            request.metadata_mut().insert("x-wyrd-access-token", token);
            match query.get_trace(request).await {
                Ok(response) => {
                    query_latencies.push(
                        u64::try_from(query_started.elapsed().as_micros()).unwrap_or(u64::MAX),
                    );
                    if let Some(trace) = response.into_inner().trace {
                        verification.query_samples_found =
                            verification.query_samples_found.saturating_add(1);
                        verification.query_spans_found = verification
                            .query_spans_found
                            .saturating_add(u64::try_from(trace.spans.len())?);
                    }
                }
                Err(error) => verification.query_errors.push(format!(
                    "tenant={tenant_index} trace={}: {}",
                    hex16(trace_id),
                    error.message()
                )),
            }
        }
    }

    let mut file_list_rows = 0_u64;
    for tenant in tenant_ids.values() {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(row_count), 0)::bigint AS rows
               FROM vala.file_list
              WHERE data_tenant_id = $1
                AND namespace = 'vala.traces'
                AND table_name = 'spans'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(cluster.pg_fixture().platform_admin_pool())
        .await?;
        let rows: i64 = row.try_get("rows")?;
        file_list_rows = file_list_rows.saturating_add(u64::try_from(rows).unwrap_or(0));
    }
    verification.file_list_rows = file_list_rows;
    verification.query_latency_us = LatencyPercentiles::from_samples(&query_latencies);
    verification.file_list_rows_match = file_list_rows == measurement.accepted_spans;
    verification.passed = verification.query_errors.is_empty()
        && verification.file_list_rows_match
        && (verification.query_samples_requested == 0
            || verification.query_samples_found == verification.query_samples_requested);
    Ok(verification)
}

fn phase_report(stats: &PhaseStats, config: &Config) -> PhaseReport {
    let elapsed_seconds = (stats.elapsed_ms as f64 / 1_000.0).max(f64::EPSILON);
    let tenant_accepted_spans = stats
        .tenant_accepted
        .iter()
        .map(|(index, value)| (index.to_string(), *value))
        .collect();
    let pod_accepted_spans = stats
        .pod_accepted
        .iter()
        .map(|(index, value)| (index.to_string(), *value))
        .collect();
    PhaseReport {
        elapsed_ms: stats.elapsed_ms,
        target_spans_per_second: config.target_spans_per_second,
        scheduled_requests: stats.scheduled_requests,
        completed_requests: stats.completed_requests,
        offered_spans: stats.offered_spans,
        accepted_spans: stats.accepted_spans,
        rejected_spans: stats.rejected_spans,
        failed_requests: stats.failed_requests,
        offered_bytes: stats.offered_bytes,
        offered_spans_per_second: stats.offered_spans as f64 / elapsed_seconds,
        accepted_spans_per_second: stats.accepted_spans as f64 / elapsed_seconds,
        offered_mib_per_second: stats.offered_bytes as f64 / elapsed_seconds / (1024.0 * 1024.0),
        ack_latency_us: LatencyPercentiles::from_samples(&stats.ack_latencies_us),
        schedule_misses: stats.schedule_misses,
        max_in_flight: stats.max_in_flight,
        errors: stats.errors.clone(),
        tenant_accepted_spans,
        pod_accepted_spans,
    }
}

fn request_period(target_spans_per_second: u64, spans_per_request: usize) -> Duration {
    let nanos = (1_000_000_000_u128
        * u128::from(u64::try_from(spans_per_request).unwrap_or(u64::MAX)))
    .div_ceil(u128::from(target_spans_per_second))
    .max(1);
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

fn first_trace_id(request: &ExportTraceServiceRequest) -> Option<[u8; 16]> {
    request
        .resource_spans
        .first()?
        .scope_spans
        .first()?
        .spans
        .first()?
        .trace_id
        .as_slice()
        .try_into()
        .ok()
}

fn hex16(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn emit_report(report: &Report) -> Result<(), BenchError> {
    let path = std::env::var_os("WYRD_OTLP_OUTPUT").map_or_else(
        || {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("target/bifrost-benchmarks/post-throughput/reports/otlp-span-ingest.json")
        },
        std::path::PathBuf::from,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(report)?),
    )?;
    println!("OTLP span benchmark report: {}", path.display());
    Ok(())
}
