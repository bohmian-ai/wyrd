//! Maintainable OTLP span-ingestion load test.
//!
//! This executable measures the real client-to-server OTLP path. It uses the
//! shared random trace generator, sends requests at an open-loop rate, waits
//! for every acknowledgement, flushes Bifrost, and verifies durable rows.
//! Query verification is optional because the ValaQuery route is not yet
//! backed by the Redux table namespace.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::Row;
use thiserror::Error as ThisError;
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

type BenchResult<T> = Result<T, BenchError>;

const DEFAULT_PODS: usize = 3;
const DEFAULT_TENANTS: usize = 10;
const DEFAULT_TARGET_SPANS_PER_SECOND: u64 = 10_000;
const DEFAULT_SPANS_PER_REQUEST: u32 = 100;
const DEFAULT_WARMUP_SECONDS: u64 = 5;
const DEFAULT_MEASUREMENT_SECONDS: u64 = 60;
const DEFAULT_MAX_IN_FLIGHT: usize = 256;
const DEFAULT_VERIFY_SAMPLES: usize = 0;
const MAX_RETAINED_SAMPLE_TRACES: usize = 16;

// Benchmark configuration.

#[derive(Debug, ThisError)]
enum BenchError {
    #[error("invalid benchmark configuration: {0}")]
    Configuration(String),
    #[error("benchmark setup failed: {0}")]
    Setup(String),
    #[error("benchmark task failed: {0}")]
    Task(String),
    #[error("durability verification failed: {0}")]
    Verification(String),
    #[error("database query failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("could not serialize benchmark report: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("could not write benchmark report: {0}")]
    Output(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
struct LoadConfig {
    pods: usize,
    tenants: usize,
    target_spans_per_second: u64,
    spans_per_request: u32,
    warmup: Duration,
    measurement: Duration,
    max_in_flight: usize,
    verify_samples_per_tenant: usize,
    output_path: String,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            pods: DEFAULT_PODS,
            tenants: DEFAULT_TENANTS,
            target_spans_per_second: DEFAULT_TARGET_SPANS_PER_SECOND,
            spans_per_request: DEFAULT_SPANS_PER_REQUEST,
            warmup: Duration::from_secs(DEFAULT_WARMUP_SECONDS),
            measurement: Duration::from_secs(DEFAULT_MEASUREMENT_SECONDS),
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            verify_samples_per_tenant: DEFAULT_VERIFY_SAMPLES,
            output_path:
                "target/bifrost-benchmarks/post-throughput/reports/otlp-span-ingest-rewrite.json"
                    .to_owned(),
        }
    }
}

impl LoadConfig {
    fn from_environment_and_args() -> BenchResult<Self> {
        let mut config = Self::default();

        config.pods = read_value("WYRD_OTLP_PODS", config.pods)?;
        config.tenants = read_value("WYRD_OTLP_TENANTS", config.tenants)?;
        config.target_spans_per_second = read_value(
            "WYRD_OTLP_TARGET_SPANS_PER_SECOND",
            config.target_spans_per_second,
        )?;
        config.spans_per_request =
            read_value("WYRD_OTLP_SPANS_PER_REQUEST", config.spans_per_request)?;
        config.warmup = Duration::from_secs(read_value(
            "WYRD_OTLP_WARMUP_SECONDS",
            config.warmup.as_secs(),
        )?);
        config.measurement = Duration::from_secs(read_value(
            "WYRD_OTLP_MEASUREMENT_SECONDS",
            config.measurement.as_secs(),
        )?);
        config.max_in_flight = read_value("WYRD_OTLP_MAX_IN_FLIGHT", config.max_in_flight)?;
        config.verify_samples_per_tenant =
            read_value("WYRD_OTLP_VERIFY_SAMPLES", config.verify_samples_per_tenant)?;
        if let Ok(path) = std::env::var("WYRD_OTLP_REWRITE_OUTPUT") {
            config.output_path = path;
        }

        for argument in std::env::args().skip(1) {
            let (name, value) = argument.split_once('=').ok_or_else(|| {
                BenchError::Configuration(format!(
                    "arguments must use --name=value syntax; received {argument}"
                ))
            })?;
            match name {
                "--pods" => config.pods = parse_value(name, value)?,
                "--tenants" => config.tenants = parse_value(name, value)?,
                "--target-spans-per-second" => {
                    config.target_spans_per_second = parse_value(name, value)?
                }
                "--spans-per-request" => config.spans_per_request = parse_value(name, value)?,
                "--warmup-seconds" => {
                    config.warmup = Duration::from_secs(parse_value(name, value)?)
                }
                "--measurement-seconds" => {
                    config.measurement = Duration::from_secs(parse_value(name, value)?)
                }
                "--max-in-flight" => config.max_in_flight = parse_value(name, value)?,
                "--verify-samples" => config.verify_samples_per_tenant = parse_value(name, value)?,
                "--output" => config.output_path = value.to_owned(),
                "--help" => print_help_and_exit(),
                _ => {
                    return Err(BenchError::Configuration(format!(
                        "unknown argument {name}"
                    )));
                }
            }
        }

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> BenchResult<()> {
        if self.pods == 0 || self.tenants == 0 {
            return Err(BenchError::Configuration(
                "pods and tenants must be greater than zero".to_owned(),
            ));
        }
        if self.pods != 1 && self.pods != 3 {
            return Err(BenchError::Configuration(
                "pods must be 1 or 3 so the topology matches the benchmark contract".to_owned(),
            ));
        }
        if self.target_spans_per_second == 0 || self.spans_per_request == 0 {
            return Err(BenchError::Configuration(
                "target-spans-per-second and spans-per-request must be greater than zero"
                    .to_owned(),
            ));
        }
        if self.measurement.is_zero() {
            return Err(BenchError::Configuration(
                "measurement duration must be greater than zero".to_owned(),
            ));
        }
        if self.max_in_flight == 0 {
            return Err(BenchError::Configuration(
                "max-in-flight must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

fn read_value<T>(name: &str, default: T) -> BenchResult<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(value) => value.parse().map_err(|error| {
            BenchError::Configuration(format!("{name}={value} is invalid: {error}"))
        }),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(BenchError::Configuration(format!(
            "could not read {name}: {error}"
        ))),
    }
}

fn parse_value<T>(name: &str, value: &str) -> BenchResult<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| BenchError::Configuration(format!("{name}={value} is invalid: {error}")))
}

fn print_help_and_exit() -> ! {
    println!(
        r#"Usage: cargo bench --bench bench_otlp_span_ingest_rewrite -- [options]

--pods=1|3
--tenants=N
--target-spans-per-second=N
--spans-per-request=N
--warmup-seconds=N
--measurement-seconds=N
--max-in-flight=N
--verify-samples=N
--output=PATH"#
    );
    std::process::exit(0);
}

// Cluster endpoints.

#[derive(Debug, Clone)]
struct Endpoint {
    tenant_index: usize,
    tenant: DataTenantId,
    pod_index: usize,
    channel: Arc<Channel>,
    jwt: Arc<str>,
}

#[derive(Debug)]
struct EndpointPool {
    endpoints: Vec<Endpoint>,
}

impl EndpointPool {
    async fn provision(cluster: &WyrdTestCluster, config: &LoadConfig) -> BenchResult<Self> {
        let bootstrap_server = cluster
            .server(0)
            .ok_or_else(|| BenchError::Setup("missing bootstrap pod".to_owned()))?;
        let mut channels = Vec::with_capacity(config.pods);
        for server in cluster.servers() {
            let endpoint = server
                .grpc_url()
                .ok_or_else(|| BenchError::Setup("missing gRPC endpoint".to_owned()))?;
            let channel = Channel::from_shared(endpoint)
                .map_err(|error| BenchError::Setup(error.to_string()))?
                .connect()
                .await
                .map_err(|error| BenchError::Setup(error.to_string()))?;
            channels.push(Arc::new(channel));
        }
        let mut endpoints = Vec::with_capacity(config.tenants * config.pods);

        for tenant_index in 0..config.tenants {
            let tenant = if tenant_index == 0 {
                cluster.data_tenant_id()
            } else {
                cluster
                    .add_tenant(&format!("bench-otlp-rewrite-tenant-{tenant_index}"))
                    .await
                    .map_err(|error| BenchError::Setup(error.to_string()))?
            };
            let service_name = format!("bench-otlp-rewrite-writer-{tenant_index}");
            let machine = bootstrap_server
                .bootstrap_service_in_tenant(tenant, &service_name, &["admin"])
                .await
                .map_err(|error| BenchError::Setup(error.to_string()))?;
            let Bootstrap::Machine { api_key, .. } = machine else {
                return Err(BenchError::Setup(format!(
                    "service bootstrap returned a non-machine credential for tenant {tenant_index}"
                )));
            };

            let client = WyrdClient::with_config(ClientConfig {
                grpc: GrpcConfig {
                    endpoint: bootstrap_server.grpc_url().ok_or_else(|| {
                        BenchError::Setup("missing bootstrap gRPC endpoint".to_owned())
                    })?,
                    connect_retries: 0,
                    ..GrpcConfig::default()
                },
                http: HttpConfig {
                    base_url: bootstrap_server
                        .base_url()
                        .ok_or_else(|| {
                            BenchError::Setup("missing bootstrap HTTP endpoint".to_owned())
                        })?
                        .to_owned(),
                    ..HttpConfig::default()
                },
                api_key: Some(api_key),
                ..ClientConfig::default()
            })
            .map_err(|error| BenchError::Setup(error.to_string()))?;
            let jwt: Arc<str> = client
                .auth()
                .bearer()
                .await
                .map_err(|error| BenchError::Setup(error.to_string()))?
                .expose()
                .to_owned()
                .into();

            for pod_index in 0..config.pods {
                endpoints.push(Endpoint {
                    tenant_index,
                    tenant,
                    pod_index,
                    channel: Arc::clone(channels.get(pod_index).ok_or_else(|| {
                        BenchError::Setup(format!("missing channel for pod {pod_index}"))
                    })?),
                    jwt: Arc::clone(&jwt),
                });
            }
        }

        Ok(Self { endpoints })
    }

    fn next(&self, request_index: u64) -> BenchResult<Endpoint> {
        if self.endpoints.is_empty() {
            return Err(BenchError::Setup("endpoint pool is empty".to_owned()));
        }
        let index = (request_index as usize) % self.endpoints.len();
        self.endpoints
            .get(index)
            .cloned()
            .ok_or_else(|| BenchError::Setup("endpoint pool is empty".to_owned()))
    }

    fn tenants(&self) -> BTreeMap<usize, DataTenantId> {
        self.endpoints
            .iter()
            .map(|endpoint| (endpoint.tenant_index, endpoint.tenant))
            .collect()
    }

    fn query_endpoint(&self, tenant_index: usize) -> Option<&Endpoint> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.tenant_index == tenant_index && endpoint.pod_index == 0)
    }
}

// Open-loop load generation.

#[derive(Debug, Clone, Copy)]
struct PhaseSpec {
    name: &'static str,
    duration: Duration,
    sequence_base: u64,
    retain_samples: bool,
}

#[derive(Debug)]
struct RequestResult {
    endpoint: Endpoint,
    trace_id: [u8; 16],
    offered_bytes: u64,
    accepted_spans: u64,
    rejected_spans: u64,
    latency_us: u64,
    error: Option<String>,
}

#[derive(Debug, Default)]
struct PhaseResult {
    scheduled_requests: u64,
    completed_requests: u64,
    accepted_spans: u64,
    rejected_spans: u64,
    failed_requests: u64,
    schedule_misses: u64,
    max_in_flight: usize,
    offered_bytes: u64,
    elapsed: Duration,
    latencies_us: Vec<u64>,
    errors: BTreeMap<String, u64>,
    accepted_by_tenant: BTreeMap<usize, u64>,
    accepted_by_pod: BTreeMap<usize, u64>,
    sample_traces: BTreeMap<usize, Vec<[u8; 16]>>,
}

impl PhaseResult {
    fn record(&mut self, result: RequestResult, retain_samples: bool) {
        self.completed_requests += 1;
        self.accepted_spans += result.accepted_spans;
        self.rejected_spans += result.rejected_spans;
        self.offered_bytes = self.offered_bytes.saturating_add(result.offered_bytes);
        *self
            .accepted_by_tenant
            .entry(result.endpoint.tenant_index)
            .or_default() += result.accepted_spans;
        *self
            .accepted_by_pod
            .entry(result.endpoint.pod_index)
            .or_default() += result.accepted_spans;
        self.latencies_us.push(result.latency_us);

        if let Some(error) = result.error {
            self.failed_requests += 1;
            *self.errors.entry(error).or_default() += 1;
        } else if retain_samples {
            let traces = self
                .sample_traces
                .entry(result.endpoint.tenant_index)
                .or_default();
            if traces.len() < MAX_RETAINED_SAMPLE_TRACES {
                traces.push(result.trace_id);
            }
        }
    }
}

async fn run_phase(
    endpoints: &EndpointPool,
    config: &LoadConfig,
    spec: PhaseSpec,
) -> BenchResult<PhaseResult> {
    let request_period = request_period(config.target_spans_per_second, config.spans_per_request);
    let mut interval = tokio::time::interval(request_period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let started = Instant::now();
    let deadline = started + spec.duration;
    let mut generator = RandomTraceGenerator::from_seed(spec.sequence_base);
    let mut tasks = JoinSet::new();
    let mut result = PhaseResult::default();
    let mut request_index = 0_u64;

    while Instant::now() < deadline {
        interval.tick().await;
        if Instant::now() >= deadline {
            break;
        }

        while let Some(task) = tasks.try_join_next() {
            result.record(
                task.map_err(|error| BenchError::Task(error.to_string()))?,
                spec.retain_samples,
            );
        }

        if tasks.len() >= config.max_in_flight {
            result.schedule_misses += 1;
            let completed = tasks
                .join_next()
                .await
                .ok_or_else(|| {
                    BenchError::Task("in-flight task set ended unexpectedly".to_owned())
                })?
                .map_err(|error| BenchError::Task(error.to_string()))?;
            result.record(completed, spec.retain_samples);
        }

        let endpoint = endpoints.next(request_index)?;
        let request = generator.export_request(
            endpoint.tenant_index,
            spec.sequence_base.wrapping_add(request_index),
            config.spans_per_request as usize,
        );
        let trace_id = first_trace_id(&request).ok_or_else(|| {
            BenchError::Task(format!(
                "phase {} generated a request without a trace",
                spec.name
            ))
        })?;
        let offered_bytes = u64::try_from(request.encoded_len()).map_err(|error| {
            BenchError::Task(format!("encoded OTLP request length overflowed: {error}"))
        })?;
        result.scheduled_requests += 1;
        result.max_in_flight = result.max_in_flight.max(tasks.len() + 1);

        let offered_spans = config.spans_per_request;
        tasks.spawn(async move {
            send_export(endpoint, request, trace_id, offered_spans, offered_bytes).await
        });
        request_index += 1;
    }

    while let Some(task) = tasks.join_next().await {
        result.record(
            task.map_err(|error| BenchError::Task(error.to_string()))?,
            spec.retain_samples,
        );
    }
    result.elapsed = started.elapsed();
    Ok(result)
}

async fn send_export(
    endpoint: Endpoint,
    request: ExportTraceServiceRequest,
    trace_id: [u8; 16],
    offered_spans: u32,
    offered_bytes: u64,
) -> RequestResult {
    let started = Instant::now();
    let mut client = TraceServiceClient::new(endpoint.channel.as_ref().clone());
    let mut request = Request::new(request);
    let token = match format!("Bearer {}", endpoint.jwt).parse() {
        Ok(token) => token,
        Err(error) => {
            return RequestResult {
                endpoint,
                trace_id,
                offered_bytes,
                accepted_spans: 0,
                rejected_spans: u64::from(offered_spans),
                latency_us: elapsed_micros(started),
                error: Some(format!("invalid access token metadata: {error}")),
            };
        }
    };
    request.metadata_mut().insert("x-wyrd-access-token", token);

    match client.export(request).await {
        Ok(response) => {
            let rejected_spans = response
                .into_inner()
                .partial_success
                .map(|partial| partial.rejected_spans.max(0) as u64)
                .unwrap_or(0);
            RequestResult {
                endpoint,
                trace_id,
                offered_bytes,
                accepted_spans: u64::from(offered_spans).saturating_sub(rejected_spans),
                rejected_spans,
                latency_us: elapsed_micros(started),
                error: None,
            }
        }
        Err(error) => RequestResult {
            endpoint,
            trace_id,
            offered_bytes,
            accepted_spans: 0,
            rejected_spans: u64::from(offered_spans),
            latency_us: elapsed_micros(started),
            error: Some(format!("{}: {}", error.code(), error.message())),
        },
    }
}

async fn prewarm(endpoints: &EndpointPool) -> BenchResult<u64> {
    let generator = RandomTraceGenerator::from_seed(0x51a7_7e11_0b5e_edd1);
    let mut tasks = JoinSet::new();

    for (index, endpoint) in endpoints.endpoints.iter().cloned().enumerate() {
        let request = generator
            .clone()
            .export_request(endpoint.tenant_index, index as u64, 1);
        let trace_id = first_trace_id(&request)
            .ok_or_else(|| BenchError::Setup("prewarm request had no trace".to_owned()))?;
        let offered_bytes = u64::try_from(request.encoded_len()).map_err(|error| {
            BenchError::Setup(format!(
                "encoded prewarm request length overflowed: {error}"
            ))
        })?;
        tasks
            .spawn(async move { send_export(endpoint, request, trace_id, 1, offered_bytes).await });
    }

    let mut accepted = 0_u64;
    while let Some(task) = tasks.join_next().await {
        let result = task.map_err(|error| BenchError::Setup(error.to_string()))?;
        if let Some(error) = result.error {
            return Err(BenchError::Setup(format!(
                "prewarm request failed: {error}"
            )));
        }
        if result.rejected_spans > 0 {
            return Err(BenchError::Setup(format!(
                "prewarm request rejected {} span(s)",
                result.rejected_spans
            )));
        }
        accepted += result.accepted_spans;
    }
    Ok(accepted)
}

async fn flush_all_tenants(
    cluster: &WyrdTestCluster,
    endpoints: &EndpointPool,
) -> BenchResult<Duration> {
    let started = Instant::now();
    for tenant in endpoints.tenants().values() {
        for server in cluster.servers() {
            server
                .flush_bifrost_for_tenant(*tenant)
                .await
                .map_err(|error| BenchError::Verification(error.to_string()))?;
        }
    }
    Ok(started.elapsed())
}

// Post-run verification.

#[derive(Debug, Serialize)]
struct DurabilityVerification {
    expected_accepted_spans: u64,
    file_list_rows: u64,
    file_list_rows_match: bool,
}

#[derive(Debug, Serialize)]
struct QueryVerification {
    enabled: bool,
    requested_samples: usize,
    found_samples: usize,
    found_spans: u64,
    latency: LatencyPercentiles,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct VerificationReport {
    flush_elapsed_ms: u128,
    durability: DurabilityVerification,
    query: QueryVerification,
    passed: bool,
}

async fn verify(
    cluster: &WyrdTestCluster,
    endpoints: &EndpointPool,
    config: &LoadConfig,
    expected_accepted_spans: u64,
    samples: &BTreeMap<usize, Vec<[u8; 16]>>,
) -> BenchResult<VerificationReport> {
    let flush_elapsed = flush_all_tenants(cluster, endpoints).await?;
    let file_list_rows = query_file_list_rows(cluster, endpoints).await?;
    let durability = DurabilityVerification {
        expected_accepted_spans,
        file_list_rows,
        file_list_rows_match: file_list_rows == expected_accepted_spans,
    };
    let query = if config.verify_samples_per_tenant == 0 {
        QueryVerification {
            enabled: false,
            requested_samples: 0,
            found_samples: 0,
            found_spans: 0,
            latency: LatencyPercentiles::from_samples(&[]),
            errors: vec![
                "disabled: ValaQuery is not yet wired to Redux tenant-qualified tables".to_owned(),
            ],
        }
    } else {
        verify_trace_queries(endpoints, samples, config.verify_samples_per_tenant).await
    };

    Ok(VerificationReport {
        flush_elapsed_ms: flush_elapsed.as_millis(),
        passed: durability.file_list_rows_match && (!query.enabled || query.errors.is_empty()),
        durability,
        query,
    })
}

async fn query_file_list_rows(
    cluster: &WyrdTestCluster,
    endpoints: &EndpointPool,
) -> BenchResult<u64> {
    let mut total = 0_u64;
    for tenant in endpoints.tenants().values() {
        let row = sqlx::query(
            r#"SELECT COALESCE(SUM(row_count), 0)::bigint AS rows
               FROM vala.file_list
               WHERE data_tenant_id = $1
                 AND namespace = 'vala.traces'
                 AND table_name = 'spans'"#,
        )
        .bind(tenant.as_uuid())
        .fetch_one(cluster.pg_fixture().platform_admin_pool())
        .await?;
        let rows: i64 = row.try_get("rows")?;
        total = total.saturating_add(u64::try_from(rows).map_err(|error| {
            BenchError::Verification(format!("file_list row count was negative: {error}"))
        })?);
    }
    Ok(total)
}

async fn verify_trace_queries(
    endpoints: &EndpointPool,
    samples: &BTreeMap<usize, Vec<[u8; 16]>>,
    requested_per_tenant: usize,
) -> QueryVerification {
    let mut latencies = Vec::new();
    let mut requested_samples = 0;
    let mut found_samples = 0;
    let mut found_spans = 0_u64;
    let mut errors = Vec::new();

    for (tenant_index, trace_ids) in samples {
        let Some(endpoint) = endpoints.query_endpoint(*tenant_index) else {
            errors.push(format!("missing query endpoint for tenant {tenant_index}"));
            continue;
        };
        let mut client = ValaQueryServiceClient::new(endpoint.channel.as_ref().clone());
        for trace_id in trace_ids.iter().take(requested_per_tenant) {
            requested_samples += 1;
            let started = Instant::now();
            let mut request = Request::new(GetTraceRequest {
                trace_id: hex16(trace_id),
                window: None,
            });
            let token = match format!("Bearer {}", endpoint.jwt).parse() {
                Ok(token) => token,
                Err(error) => {
                    errors.push(format!("invalid query-token metadata: {error}"));
                    continue;
                }
            };
            request.metadata_mut().insert("x-wyrd-access-token", token);
            match client.get_trace(request).await {
                Ok(response) => {
                    found_samples += 1;
                    if let Some(trace) = response.into_inner().trace {
                        found_spans = found_spans.saturating_add(trace.spans.len() as u64);
                    }
                    latencies.push(elapsed_micros(started));
                }
                Err(error) => errors.push(format!("{}: {}", error.code(), error.message())),
            }
        }
    }

    QueryVerification {
        enabled: true,
        requested_samples,
        found_samples,
        found_spans,
        latency: LatencyPercentiles::from_samples(&latencies),
        errors,
    }
}

// Report serialization and execution.

#[derive(Debug, Serialize)]
struct PhaseReport {
    scheduled_requests: u64,
    completed_requests: u64,
    accepted_spans: u64,
    rejected_spans: u64,
    failed_requests: u64,
    schedule_misses: u64,
    max_in_flight: usize,
    elapsed_ms: u128,
    offered_bytes: u64,
    accepted_spans_per_second: f64,
    offered_bytes_per_second: f64,
    ack_latency_us: LatencyPercentiles,
    errors: BTreeMap<String, u64>,
    accepted_by_tenant: BTreeMap<usize, u64>,
    accepted_by_pod: BTreeMap<usize, u64>,
}

impl PhaseReport {
    fn from_result(result: PhaseResult) -> Self {
        let elapsed_seconds = result.elapsed.as_secs_f64();
        Self {
            scheduled_requests: result.scheduled_requests,
            completed_requests: result.completed_requests,
            accepted_spans: result.accepted_spans,
            rejected_spans: result.rejected_spans,
            failed_requests: result.failed_requests,
            schedule_misses: result.schedule_misses,
            max_in_flight: result.max_in_flight,
            elapsed_ms: result.elapsed.as_millis(),
            offered_bytes: result.offered_bytes,
            accepted_spans_per_second: rate(result.accepted_spans, elapsed_seconds),
            offered_bytes_per_second: rate(result.offered_bytes, elapsed_seconds),
            ack_latency_us: LatencyPercentiles::from_samples(&result.latencies_us),
            errors: result.errors,
            accepted_by_tenant: result.accepted_by_tenant,
            accepted_by_pod: result.accepted_by_pod,
        }
    }
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    benchmark: &'static str,
    ack_boundary: &'static str,
    durability_boundary: &'static str,
    queryability_boundary: &'static str,
    config: ReportConfig,
    prewarm_accepted_spans: u64,
    warmup: PhaseReport,
    measurement: PhaseReport,
    verification: VerificationReport,
    readiness: &'static str,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReportConfig {
    pods: usize,
    tenants: usize,
    target_spans_per_second: u64,
    spans_per_request: u32,
    warmup_seconds: u64,
    measurement_seconds: u64,
    max_in_flight: usize,
    verify_samples_per_tenant: usize,
    span_shape: &'static str,
}

async fn run(config: LoadConfig) -> BenchResult<BenchmarkReport> {
    let topology = match config.pods {
        1 => BifrostTopology::OnePod,
        3 => BifrostTopology::ThreePod,
        _ => return Err(BenchError::Configuration("pods must be 1 or 3".to_owned())),
    };
    let cluster = WyrdTestCluster::start(config.pods, topology)
        .await
        .map_err(|error| BenchError::Setup(error.to_string()))?;
    let result = run_on_cluster(&cluster, &config).await;
    let shutdown_result = cluster.shutdown().await;
    match (result, shutdown_result) {
        (Ok(report), Ok(())) => Ok(report),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(BenchError::Setup(format!(
            "cluster shutdown failed: {error}"
        ))),
        (Err(error), Err(shutdown_error)) => Err(BenchError::Setup(format!(
            "benchmark failed: {error}; cluster shutdown failed: {shutdown_error}"
        ))),
    }
}

async fn run_on_cluster(
    cluster: &WyrdTestCluster,
    config: &LoadConfig,
) -> BenchResult<BenchmarkReport> {
    let endpoints = EndpointPool::provision(cluster, config).await?;
    let prewarm_accepted_spans = prewarm(&endpoints).await?;
    flush_all_tenants(cluster, &endpoints).await?;
    let warmup = run_phase(
        &endpoints,
        config,
        PhaseSpec {
            name: "warmup",
            duration: config.warmup,
            sequence_base: 0x51a7_7e11_0b5e_edd1,
            retain_samples: false,
        },
    )
    .await?;
    let measurement = run_phase(
        &endpoints,
        config,
        PhaseSpec {
            name: "measurement",
            duration: config.measurement,
            sequence_base: 0xa11c_e5ed_5eed_0001,
            retain_samples: config.verify_samples_per_tenant > 0,
        },
    )
    .await?;
    let expected_accepted_spans = prewarm_accepted_spans
        .saturating_add(warmup.accepted_spans)
        .saturating_add(measurement.accepted_spans);
    let verification = verify(
        cluster,
        &endpoints,
        config,
        expected_accepted_spans,
        &measurement.sample_traces,
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

    Ok(BenchmarkReport {
        schema_version: 1,
        benchmark: "otlp-span-ingest-rewrite",
        ack_boundary: "Gate accepted the OTLP export and Scribe admitted the projected frame",
        durability_boundary: "post-run Scribe flush committed file-list rows; client ACK is measured separately",
        queryability_boundary: if config.verify_samples_per_tenant == 0 {
            "not run (verify-samples=0)"
        } else {
            "sampled ValaQueryService.GetTrace after flushing every pod"
        },
        config: ReportConfig {
            pods: config.pods,
            tenants: config.tenants,
            target_spans_per_second: config.target_spans_per_second,
            spans_per_request: config.spans_per_request,
            warmup_seconds: config.warmup.as_secs(),
            measurement_seconds: config.measurement.as_secs(),
            max_in_flight: config.max_in_flight,
            verify_samples_per_tenant: config.verify_samples_per_tenant,
            span_shape: "OTLP ResourceSpans with scope, parent/child spans, attributes, events, links, and status",
        },
        prewarm_accepted_spans,
        warmup: PhaseReport::from_result(warmup),
        measurement: PhaseReport::from_result(measurement),
        verification,
        readiness,
        notes: vec![
            "Each request contains realistic randomly generated traces and spans.".to_owned(),
            "Throughput is measured from acknowledged OTLP spans, not synthetic rows.".to_owned(),
            "Durability verification sums vala.file_list row counts after an explicit flush."
                .to_owned(),
            "Forge and Oracle are not required for this Gate-to-Scribe ingest lane.".to_owned(),
        ],
    })
}

fn request_period(target_spans_per_second: u64, spans_per_request: u32) -> Duration {
    let seconds = f64::from(spans_per_request) / target_spans_per_second as f64;
    Duration::from_secs_f64(seconds.max(f64::EPSILON))
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

fn rate(value: u64, seconds: f64) -> f64 {
    if seconds > 0.0 {
        value as f64 / seconds
    } else {
        0.0
    }
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

fn hex16(value: &[u8; 16]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_report(config: &LoadConfig, report: &BenchmarkReport) -> BenchResult<()> {
    let path = std::path::Path::new(&config.output_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(report)?;
    std::fs::write(path, format!("{body}\n"))?;
    println!("benchmark report: {}", path.display());
    Ok(())
}

#[tokio::main]
async fn main() -> BenchResult<()> {
    let config = LoadConfig::from_environment_and_args()?;
    let report = run(config.clone()).await?;
    write_report(&config, &report)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn request_period_matches_requested_span_rate() {
        assert_eq!(
            super::request_period(10_000, 100),
            std::time::Duration::from_millis(10)
        );
    }

    #[test]
    fn trace_id_is_hex_encoded_as_lowercase() {
        assert_eq!(
            super::hex16(&[0xab; 16]),
            "abababababababababababababababab"
        );
    }

    #[test]
    fn empty_latency_sample_is_default() {
        let percentiles = super::LatencyPercentiles::from_samples(&[]);
        assert_eq!(percentiles, super::LatencyPercentiles::default());
    }
}
