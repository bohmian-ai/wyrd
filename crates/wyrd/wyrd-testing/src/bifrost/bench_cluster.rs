//! Controlled full-cluster benchmark adapter and versioned v2 diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use hdrhistogram::Histogram;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::{BifrostFrame, BifrostGrpcTransport, QueryClient, QueryResultStream};
use wyrd_bench::{
    AttemptedProbe, BenchmarkEnvironment, BenchmarkOperation, BifrostDiagnosticReport,
    BifrostReferenceProfile, BifrostRuntimeRole, BifrostSloEnvelope, CALIBRATION_RATE_CAP,
    CAPACITY_TABLES_PER_TENANT, CLUSTER_REPORT_VERSION, CLUSTER_WORKLOAD_VERSION, CapacityLimit,
    CapacityStage, CapacityStageIdentity, CapacityStagePlan, CapacityStateMachine,
    ClientTrialMetrics, ClusterBenchmarkError, ClusterBenchmarkScenario, ClusterBenchmarkTrial,
    ClusterScenarioReport, ClusterTopology, ClusterTrialReport, ClusterWorkloadIdentity,
    DependencyTelemetryEvidence, DiagnosticStatus, EvidenceStatus, FIRST_PROBE_RATE,
    KneeProvenance, NodeId, NodeResourceEvidence, PillarTelemetryDelta,
    ProductionTelemetryEvidence, ReviewedScenarioProfile, SpanDistribution, TenantStageRows,
    TraceManifest, TrafficMix, TrialDistribution, derive_trial_median, extract_linux_cpu_identity,
    extract_macos_cpu_identity, jain_fairness,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, VisibilityMode,
};

use crate::Bootstrap;
use crate::bifrost::{BifrostClusterSpec, WyrdTestCluster};

/// Stable logical table used by the controlled workload.
const REFERENCE_TABLE: &str = "cluster_reference_events";
/// Shared warmup-table name used by one capacity scenario session.
pub const CAPACITY_WARMUP_TABLE: &str = "cluster_capacity_warmup";
/// Number of deterministic capacity measurement-table slots.
pub const CAPACITY_STAGE_TABLE_COUNT: usize = 13;
/// Rows preloaded through Gate for every tenant before measured traffic.
const PRELOAD_ROWS: u64 = 8_192;
/// Rows in one public durable write request.
const ROWS_PER_WRITE: u32 = 64;
/// Default profile-driven concurrent public-operation cap.
const DEFAULT_MAX_IN_FLIGHT: usize = 4_096;
/// Minimum process soft open-file limit required by the reference benchmark.
const MINIMUM_OPEN_FILE_LIMIT: u64 = 8_192;
/// Disjoint logical row-id space reserved for each authenticated tenant.
const TENANT_ROW_STRIDE: u64 = 1_000_000_000_000;

/// Public benchmark execution mode selected by the canonical command grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterBenchmarkMode {
    /// Run the bounded absolute-rate capacity sweep.
    Capacity,
    /// Replay reviewed absolute rates for controlled qualification.
    Qualification,
}

/// Parsed selector for one canonical cluster invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterBenchmarkCommand {
    /// Selected benchmark mode.
    pub mode: ClusterBenchmarkMode,
    /// One fixed scenario selector, when not running the matrix.
    pub scenario: Option<String>,
    /// Whether all six fixed scenarios run sequentially.
    pub matrix: bool,
}

/// Parse the exact public `bench_bifrost_cluster` grammar before startup.
///
/// The parser intentionally ignores lifecycle/profile/output environment
/// variables. They are consumed only after this grammar succeeds.
///
/// # Errors
/// Returns a message when mode, selector cardinality, selector identity, or
/// an unknown flag is invalid.
pub fn parse_cluster_benchmark_args<I, S>(args: I) -> Result<ClusterBenchmarkCommand, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let mut mode = None;
    let mut scenario = None;
    let mut matrix = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--mode" => {
                if mode.is_some() || index + 1 >= args.len() {
                    return Err(
                        "--mode must occur exactly once with capacity or qualification".to_owned(),
                    );
                }
                mode = Some(match args[index + 1].as_str() {
                    "capacity" => ClusterBenchmarkMode::Capacity,
                    "qualification" => ClusterBenchmarkMode::Qualification,
                    value => return Err(format!("unknown --mode value `{value}`")),
                });
                index += 2;
            }
            "--scenario" => {
                if scenario.is_some() || matrix || index + 1 >= args.len() {
                    return Err(
                        "exactly one of --scenario <scenario-id> or --matrix is required"
                            .to_owned(),
                    );
                }
                let value = args[index + 1].clone();
                if !reference_scenario_matrix()
                    .iter()
                    .any(|entry| entry.id == value)
                {
                    return Err(format!("unknown scenario `{value}`"));
                }
                scenario = Some(value);
                index += 2;
            }
            "--matrix" => {
                if matrix || scenario.is_some() {
                    return Err(
                        "exactly one of --scenario <scenario-id> or --matrix is required"
                            .to_owned(),
                    );
                }
                matrix = true;
                index += 1;
            }
            value => return Err(format!("unknown benchmark argument `{value}`")),
        }
    }
    let mode = mode.ok_or_else(|| "--mode capacity|qualification is required".to_owned())?;
    if scenario.is_none() && !matrix {
        return Err("exactly one of --scenario <scenario-id> or --matrix is required".to_owned());
    }
    Ok(ClusterBenchmarkCommand {
        mode,
        scenario,
        matrix,
    })
}

/// One fixed D22 scenario before calibration assigns absolute rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceScenarioDefinition {
    /// Stable scenario identifier.
    pub id: &'static str,
    /// Process topology exercised by the scenario.
    pub topology: ClusterTopology,
    /// Isolated tenant count.
    pub tenants: u32,
    /// Exact public traffic mix.
    pub traffic: TrafficMix,
    /// Durable-write percentage of public operations.
    pub write_percent: u8,
    /// Strict-query percentage of public operations.
    pub read_percent: u8,
}

/// Return D22's exact six scenarios in deterministic execution order.
#[must_use]
pub const fn reference_scenario_matrix() -> [ReferenceScenarioDefinition; 6] {
    [
        ReferenceScenarioDefinition {
            id: "balanced-one-pod-one-tenant",
            topology: ClusterTopology::OnePod,
            tenants: 1,
            traffic: TrafficMix::Balanced,
            write_percent: 50,
            read_percent: 50,
        },
        ReferenceScenarioDefinition {
            id: "balanced-one-pod-eight-tenants",
            topology: ClusterTopology::OnePod,
            tenants: 8,
            traffic: TrafficMix::Balanced,
            write_percent: 50,
            read_percent: 50,
        },
        ReferenceScenarioDefinition {
            id: "balanced-three-server-three-worker-eight-tenants",
            topology: ClusterTopology::ThreeServersThreeForgeWorkers,
            tenants: 8,
            traffic: TrafficMix::Balanced,
            write_percent: 50,
            read_percent: 50,
        },
        ReferenceScenarioDefinition {
            id: "balanced-three-server-three-worker-thirty-two-tenants",
            topology: ClusterTopology::ThreeServersThreeForgeWorkers,
            tenants: 32,
            traffic: TrafficMix::Balanced,
            write_percent: 50,
            read_percent: 50,
        },
        ReferenceScenarioDefinition {
            id: "write-heavy-three-server-three-worker-eight-tenants",
            topology: ClusterTopology::ThreeServersThreeForgeWorkers,
            tenants: 8,
            traffic: TrafficMix::WriteHeavy,
            write_percent: 90,
            read_percent: 10,
        },
        ReferenceScenarioDefinition {
            id: "read-heavy-three-server-three-worker-eight-tenants",
            topology: ClusterTopology::ThreeServersThreeForgeWorkers,
            tenants: 8,
            traffic: TrafficMix::ReadHeavy,
            write_percent: 10,
            read_percent: 90,
        },
    ]
}

/// Run calibration and all 54 controlled trials, then emit one strict report.
///
/// The checked-in baseline is never modified. Dirty captures are written for
/// diagnostics and then return `Unsupported` from profile validation.
///
/// # Errors
/// Returns an environment, calibration, cluster, workload, reconciliation,
/// report, or IO error. Initial-probe failure is `NotReady`; censored knees
/// remain explicit in the emitted profile.
pub async fn run_reference() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    run_reference_selected(None, true, false).await
}

/// Run qualification with the parsed public selector.
///
/// # Errors
/// Propagates the non-promotable diagnostic or lifecycle error emitted by the
/// selected controlled run.
pub async fn run_qualification(
    command: ClusterBenchmarkCommand,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    run_qualification_selected(command.scenario.as_deref(), command.matrix).await
}

/// Execute qualification in one live cluster session per selected scenario.
async fn run_qualification_selected(
    selected_scenario: Option<&str>,
    matrix: bool,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let path = report_path("cluster-candidate.json");
    let environment = detect_reference_environment_or_write(&path)?;
    let rates = wyrd_bench::DEFAULT_REVIEWED_QUALIFICATION_RATES;
    wyrd_bench::qualification_preflight_seconds(rates.len(), 20, 180, 1_200)?;
    let mut reports = Vec::new();
    let mut attempted_probes = Vec::new();
    let mut current_partial = None;
    for definition in reference_scenario_matrix() {
        if !matrix && selected_scenario != Some(definition.id) {
            continue;
        }
        let session = CapacityScenarioSession::start_with_tables(
            definition,
            qualification_table_names(rates.len()),
            PathBuf::from(&environment.storage_root),
        )
        .await?;
        let result = async {
            for (rate_index, rate) in rates.iter().copied().enumerate() {
                let mut trials = Vec::with_capacity(3);
                let mut trial_evidence = Vec::with_capacity(3);
                for trial in 1..=3 {
                    begin_probe(
                        &mut attempted_probes,
                        format!("{}:trial-{trial}", definition.id),
                        rate,
                    );
                    let pair = rate_index * 3 + usize::from(trial - 1);
                    let (window, production, telemetry) = tokio::time::timeout(
                        Duration::from_secs(45),
                        session.run_qualification_pair(pair, rate, Duration::from_secs(20)),
                    )
                    .await
                    .map_err(|_| "qualification pair exceeded its 45-second bound")??;
                    complete_current_probe(&mut attempted_probes, &[]);
                    let metrics = window.metrics();
                    trials.push(ClusterBenchmarkTrial {
                        trial,
                        client: metrics.clone(),
                        production,
                    });
                    let evidence = ClusterTrialReport {
                        offered_requests_per_second: rate,
                        trial_index: trial,
                        metrics,
                        telemetry: capacity_pillar_evidence(&telemetry),
                        resources: capacity_resource_evidence(definition, &telemetry),
                        dependencies: capacity_dependency_evidence(&telemetry),
                        traces: capacity_trace_evidence(&telemetry),
                    };
                    evidence.validate_evidence()?;
                    trial_evidence.push(evidence);
                    current_partial = Some(qualification_scenario_report(
                        definition,
                        rate,
                        trials.clone(),
                        trial_evidence.clone(),
                        false,
                    )?);
                }
                reports.push(qualification_scenario_report(
                    definition,
                    rate,
                    trials,
                    trial_evidence,
                    true,
                )?);
                current_partial = None;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }
        .await;
        let shutdown = session.shutdown().await;
        if let Err(error) = result {
            let cleanup_error = shutdown.err().map(|cleanup| cleanup.to_string());
            let combined = cleanup_error.as_ref().map_or_else(
                || error.to_string(),
                |cleanup| format!("{error}; cleanup failed: {cleanup}"),
            );
            if let Some(partial) = current_partial {
                reports.push(partial);
            }
            write_partial_capture(&path, &environment, &combined, attempted_probes, reports)?;
            return Err(error);
        }
        shutdown?;
        for report in reports
            .iter_mut()
            .filter(|report| report.scenario.scenario_id == definition.id)
        {
            for trial in &mut report.trials {
                trial.production.cleanup = EvidenceStatus::Complete;
            }
        }
    }
    let profile = BifrostReferenceProfile {
        schema_version: CLUSTER_REPORT_VERSION.to_owned(),
        environment: environment.clone(),
        scenarios: reviewed_scenario_profiles(reports, &environment_storage_identity(&environment)),
        slos: BifrostSloEnvelope::default(),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&profile)?),
    )?;
    let validation = selected_scenario.map_or_else(
        || profile.validate_reference(),
        |id| profile.validate_selected(id),
    );
    if let Err(error) = validation {
        write_qualification_diagnostic(&path, &profile, &error)?;
        return Err(Box::new(error));
    }
    Ok(path)
}

/// Run the bounded absolute-rate capacity characterization.
///
/// Capacity reports are diagnostics only. They retain failed stages and never
/// become a qualification baseline.
///
/// # Errors
/// Propagates the selected cluster lifecycle or report-write error.
pub async fn run_capacity(
    command: ClusterBenchmarkCommand,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    run_capacity_selected(command.scenario.as_deref(), command.matrix).await
}

/// Execute the bounded curve in one live session per selected scenario.
async fn run_capacity_selected(
    selected_scenario: Option<&str>,
    matrix: bool,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let path = report_path("cluster-capacity.json");
    let environment = detect_reference_environment_or_write(&path)?;
    let mut reports = Vec::new();
    for definition in reference_scenario_matrix() {
        if !matrix && selected_scenario != Some(definition.id) {
            continue;
        }
        let mut session =
            CapacityScenarioSession::start(definition, PathBuf::from(&environment.storage_root))
                .await?;
        let mut stages = Vec::new();
        let mut attempted_probes = Vec::new();
        let execution = async {
            tokio::time::timeout(Duration::from_secs(15), session.run_initial_warmup())
                .await
                .map_err(|_| "capacity initial warmup exceeded its 15-second bound")??;
            while let Some(plan) = session.next_stage() {
                begin_probe(
                    &mut attempted_probes,
                    definition.id.to_owned(),
                    plan.offered_requests_per_second,
                );
                let result = tokio::time::timeout(
                    Duration::from_secs(30),
                    session.run_stage(plan, Duration::from_secs(20)),
                )
                .await
                .map_err(|_| "capacity stage exceeded its 30-second bound")??;
                let stage = assemble_capacity_stage(definition, plan, result, 20);
                session.record_stage(plan, stage.passed)?;
                complete_current_probe(&mut attempted_probes, &stage.stop_reasons);
                stages.push(stage);
            }
            session.reconcile_cumulative().await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }
        .await;
        let shutdown = session.shutdown().await;
        if let Err(error) = execution {
            let partial =
                (!stages.is_empty()).then(|| capacity_scenario_report(definition, stages));
            let cleanup_error = shutdown.err().map(|cleanup| cleanup.to_string());
            let combined = cleanup_error.as_ref().map_or_else(
                || error.to_string(),
                |cleanup| format!("{error}; cleanup failed: {cleanup}"),
            );
            write_partial_capture(
                &path,
                &environment,
                &combined,
                attempted_probes,
                partial.into_iter().collect(),
            )?;
            return Err(error);
        }
        shutdown?;
        reports.push(capacity_scenario_report(definition, stages));
    }
    let profile = BifrostReferenceProfile {
        schema_version: CLUSTER_REPORT_VERSION.to_owned(),
        environment: environment.clone(),
        scenarios: reviewed_scenario_profiles(reports, &environment_storage_identity(&environment)),
        slos: BifrostSloEnvelope::default(),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&profile)?),
    )?;
    Ok(path)
}

/// Assemble one capacity report while retaining every completed stage.
#[must_use]
fn capacity_scenario_report(
    definition: ReferenceScenarioDefinition,
    stages: Vec<CapacityStage>,
) -> ClusterScenarioReport {
    let rate = stages
        .last()
        .map_or(1, |stage| stage.offered_requests_per_second);
    let knee = stages
        .iter()
        .filter(|stage| stage.passed)
        .map(|stage| stage.offered_requests_per_second)
        .max()
        .unwrap_or(0);
    ClusterScenarioReport {
        scenario: ClusterBenchmarkScenario {
            scenario_id: definition.id.to_owned(),
            workload_version: CLUSTER_WORKLOAD_VERSION.to_owned(),
            topology: definition.topology,
            tenants: definition.tenants,
            traffic: definition.traffic,
            rows_per_batch: ROWS_PER_WRITE,
            query_row_limit: 64,
            offered_requests_per_second: rate,
            offered_load_percent: 0,
            knee_provenance: KneeProvenance::Discovered,
            warmup_seconds: 10,
            measured_seconds: 20,
            trials: 3,
            minimum_samples: 200,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            seed: 0xB1_F057,
        },
        discovered_knee_requests_per_second: knee,
        discovered_knee_provenance: KneeProvenance::Discovered,
        trials: Vec::new(),
        trial_evidence: Vec::new(),
        median: Default::default(),
        capacity_stages: stages,
    }
}

/// Assemble a complete or diagnostic partial qualification-rate report.
fn qualification_scenario_report(
    definition: ReferenceScenarioDefinition,
    rate: u64,
    trials: Vec<ClusterBenchmarkTrial>,
    trial_evidence: Vec<ClusterTrialReport>,
    complete: bool,
) -> Result<ClusterScenarioReport, ClusterBenchmarkError> {
    let median = if complete {
        derive_trial_median(&trials)?
    } else {
        Default::default()
    };
    Ok(ClusterScenarioReport {
        scenario: ClusterBenchmarkScenario {
            scenario_id: definition.id.to_owned(),
            workload_version: CLUSTER_WORKLOAD_VERSION.to_owned(),
            topology: definition.topology,
            tenants: definition.tenants,
            traffic: definition.traffic,
            rows_per_batch: ROWS_PER_WRITE,
            query_row_limit: 64,
            offered_requests_per_second: rate,
            offered_load_percent: 0,
            knee_provenance: KneeProvenance::Discovered,
            warmup_seconds: 10,
            measured_seconds: 20,
            trials: 3,
            minimum_samples: 200,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            seed: 0xB1_F057,
        },
        discovered_knee_requests_per_second: *wyrd_bench::DEFAULT_REVIEWED_QUALIFICATION_RATES
            .last()
            .unwrap_or(&rate),
        discovered_knee_provenance: KneeProvenance::Discovered,
        trials,
        trial_evidence,
        median,
        capacity_stages: Vec::new(),
    })
}

/// Convert a completed public workload window into one retained capacity stage.
#[must_use]
fn capacity_stage_report(
    definition: ReferenceScenarioDefinition,
    plan: CapacityStagePlan,
    result: CapacityStageRun,
    passed: bool,
    measured_seconds: u64,
) -> CapacityStage {
    let mut stop_reasons = Vec::new();
    if result.client.backpressure > 0 {
        stop_reasons.push(CapacityLimit::Backpressure);
    }
    if result.client.in_flight_cap_exhaustions > 0 {
        stop_reasons.push(CapacityLimit::InFlightCap);
    }
    if result.client.missed_operations > 0 {
        stop_reasons.push(CapacityLimit::MissedDeadline);
    }
    CapacityStage {
        identity: CapacityStageIdentity {
            stage_id: format!("{}-{:02}", definition.id, plan.slot),
            ordinal: plan.slot,
            tenant_rows: result
                .client
                .tenant_write_ordinals
                .iter()
                .enumerate()
                .map(|(tenant, ordinals)| stage_row_identity(tenant, ordinals))
                .collect(),
        },
        offered_requests_per_second: plan.offered_requests_per_second,
        completed_requests_per_second: result.client.accepted as f64
            / measured_seconds.max(1) as f64,
        duration: Duration::from_secs(measured_seconds),
        passed,
        in_flight_cap_exhaustions: result.client.in_flight_cap_exhaustions,
        stop_reasons,
        metrics: result.client.metrics(),
        telemetry: capacity_pillar_evidence(&result.telemetry),
        resources: capacity_resource_evidence(definition, &result.telemetry),
        dependencies: capacity_dependency_evidence(&result.telemetry),
        traces: capacity_trace_evidence(&result.telemetry),
    }
}

/// Assemble and validate one completed public workload stage for capacity or smoke.
///
/// The helper applies the single production outcome policy before validating
/// the existing telemetry, dependency, resource, and trace evidence shape.
#[must_use]
fn assemble_capacity_stage(
    definition: ReferenceScenarioDefinition,
    plan: CapacityStagePlan,
    result: CapacityStageRun,
    measured_seconds: u64,
) -> CapacityStage {
    let passed = result.client.missed_operations == 0
        && result.client.missed_flushes == 0
        && result.client.in_flight_cap_exhaustions == 0
        && result.client.backpressure == 0
        && result.published_rows == result.client.tenant_write_rows.iter().sum::<u64>()
        && result.audit_rows > 0;
    let mut stage = capacity_stage_report(definition, plan, result, passed, measured_seconds);
    if stage.validate_evidence().is_err() {
        stage.passed = false;
        stage.stop_reasons.push(CapacityLimit::DependencySlo);
    }
    stage
}

/// Derive the report identity from the same ordinal used to write public frames.
#[must_use]
fn stage_row_identity(tenant: usize, ordinals: &BTreeSet<u64>) -> TenantStageRows {
    let batch_ordinals = ordinals.iter().copied().collect::<Vec<_>>();
    let row_ids = ordinals
        .iter()
        .flat_map(|ordinal| {
            let start = tenant_row_base(tenant) + PRELOAD_ROWS + ordinal * 64;
            start..start + u64::from(ROWS_PER_WRITE)
        })
        .collect::<Vec<_>>();
    let row_id_start = row_ids
        .first()
        .copied()
        .unwrap_or(tenant_row_base(tenant) + PRELOAD_ROWS);
    TenantStageRows {
        tenant_ordinal: tenant as u32,
        batch_id_seed: batch_ordinals.first().copied().unwrap_or(0),
        row_id_start,
        row_id_end_exclusive: row_ids.last().map_or(row_id_start, |row| row + 1),
        batch_ordinals,
        row_ids,
    }
}

/// Project bounded production counters into capacity pillar evidence.
#[must_use]
fn capacity_pillar_evidence(
    telemetry: &crate::bifrost::telemetry::ForgeTelemetryDelta,
) -> PillarTelemetryDelta {
    let total = |family: &str| {
        telemetry
            .metrics
            .iter()
            .filter(|sample| sample.family == family)
            .map(|sample| sample.value.max(0.0))
            .sum::<f64>() as u64
    };
    let required = [
        "bifrost_gate_requests_total",
        "bifrost_scribe_rows_total",
        "bifrost_oracle_stream_rows_total",
    ];
    PillarTelemetryDelta {
        gate_accepted: total("bifrost_gate_requests_total"),
        scribe_wal_bytes: total("bifrost_scribe_wal_bytes_total"),
        forge_materialized_rows: total("bifrost_forge_materialized_rows_total"),
        oracle_decoded_rows: total("bifrost_oracle_stream_rows_total"),
        complete: required.iter().all(|family| {
            telemetry
                .metrics
                .iter()
                .any(|sample| sample.family == *family)
        }),
    }
}

/// Project dependency families without inventing missing observations.
#[must_use]
fn capacity_dependency_evidence(
    telemetry: &crate::bifrost::telemetry::ForgeTelemetryDelta,
) -> DependencyTelemetryEvidence {
    let total = |needle: &str| {
        telemetry
            .metrics
            .iter()
            .filter(|sample| sample.family.contains(needle))
            .map(|sample| sample.value.max(0.0))
            .sum::<f64>() as u64
    };
    DependencyTelemetryEvidence {
        postgres_pool_wait_us: total("pool_wait"),
        postgres_transactions: total("transaction"),
        storage_bytes: total("storage_bytes"),
        storage_p99_us: total("storage_duration"),
        wal_fsync_p99_us: total("fsync"),
        complete: ["pool", "storage", "fsync"].iter().all(|needle| {
            telemetry
                .metrics
                .iter()
                .any(|sample| sample.family.contains(needle))
        }),
    }
}

/// Capture bounded process resource evidence for the active role topology.
#[must_use]
fn capacity_resource_evidence(
    _definition: ReferenceScenarioDefinition,
    telemetry: &crate::bifrost::telemetry::ForgeTelemetryDelta,
) -> Vec<NodeResourceEvidence> {
    let pid = std::process::id().to_string();
    let rss = command_output("ps", &["-o", "rss=", "-p", &pid])
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok());
    let cpu = command_output("ps", &["-o", "time=", "-p", &pid])
        .ok()
        .and_then(|value| parse_process_cpu_seconds(value.trim()));
    match (cpu, rss) {
        (Some(cpu_seconds), Some(rss_kib)) => vec![NodeResourceEvidence {
            node_id: NodeId(format!("in-process-cluster-{pid}")),
            roles: vec![
                BifrostRuntimeRole::Gate,
                BifrostRuntimeRole::Scribe,
                BifrostRuntimeRole::Forge,
                BifrostRuntimeRole::Oracle,
            ],
            cpu_seconds,
            peak_rss_bytes: rss_kib.saturating_mul(1024),
            runtime_busy_seconds: telemetry
                .spans
                .iter()
                .map(|span| span.duration_nanos as f64 / 1_000_000_000.0)
                .sum(),
            runtime_queue_peak: tokio::runtime::Handle::current()
                .metrics()
                .global_queue_depth() as u64,
        }],
        _ => Vec::new(),
    }
}

/// Parse the host `ps` CPU-time representation without substituting wall time.
#[must_use]
fn parse_process_cpu_seconds(rendered: &str) -> Option<f64> {
    let fields = rendered.split(':').collect::<Vec<_>>();
    match fields.as_slice() {
        [minutes, seconds] => {
            Some(minutes.parse::<f64>().ok()? * 60.0 + seconds.parse::<f64>().ok()?)
        }
        [hours, minutes, seconds] => Some(
            hours.parse::<f64>().ok()? * 3_600.0
                + minutes.parse::<f64>().ok()? * 60.0
                + seconds.parse::<f64>().ok()?,
        ),
        _ => None,
    }
}

/// Classify a production span only when its name proves one benchmark operation.
#[must_use]
fn benchmark_span_operation(name: &str) -> Option<BenchmarkOperation> {
    let name = name.to_ascii_lowercase();
    if name.contains("first_frame") || name.contains("time_to_first") {
        Some(BenchmarkOperation::QueryTimeToFirstFrame)
    } else if name.contains("oracle") || name.contains("query") {
        Some(BenchmarkOperation::QueryTotal)
    } else if name.contains("forge") || name.contains("flush") || name.contains("publish") {
        Some(BenchmarkOperation::FlushToVisible)
    } else if name.contains("scribe") || name.contains("ingest") || name.contains("durable_write") {
        Some(BenchmarkOperation::DurableWrite)
    } else {
        None
    }
}

/// Build four bounded trace manifests from the production capture window.
#[must_use]
fn capacity_trace_evidence(
    telemetry: &crate::bifrost::telemetry::ForgeTelemetryDelta,
) -> Vec<TraceManifest> {
    [
        BenchmarkOperation::DurableWrite,
        BenchmarkOperation::FlushToVisible,
        BenchmarkOperation::QueryTimeToFirstFrame,
        BenchmarkOperation::QueryTotal,
    ]
    .into_iter()
    .map(|operation| {
        let mut durations = telemetry
            .spans
            .iter()
            .filter(|span| benchmark_span_operation(&span.name) == Some(operation))
            .map(|span| span.duration_nanos / 1_000)
            .collect::<Vec<_>>();
        durations.sort_unstable();
        let index = durations.len().saturating_sub(1);
        TraceManifest {
            operation,
            representative_trace_ids: telemetry
                .spans
                .iter()
                .filter(|span| benchmark_span_operation(&span.name) == Some(operation))
                .rev()
                .take(3)
                .map(|span| span.trace_id.clone())
                .collect(),
            critical_path_spans: vec![SpanDistribution {
                operation,
                samples: durations.len() as u64,
                p95_us: durations.get(index * 95 / 100).copied().unwrap_or(0),
                p99_us: durations.get(index * 99 / 100).copied().unwrap_or(0),
            }],
        }
    })
    .collect()
}

/// Execute the selected scenarios for either qualification or capacity mode.
async fn run_reference_selected(
    selected_scenario: Option<&str>,
    matrix: bool,
    capacity: bool,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let environment = match detect_reference_environment() {
        Ok(environment) => environment,
        Err(error) => {
            let path = report_path(if capacity {
                "cluster-capacity.json"
            } else {
                "cluster-candidate.json"
            });
            write_unavailable_capture(&path, &error.to_string())?;
            return Err(Box::new(error));
        }
    };
    let mut reports = Vec::with_capacity(18);
    for definition in reference_scenario_matrix() {
        if !matrix && selected_scenario != Some(definition.id) {
            continue;
        }
        let (knee, provenance) = match calibrate(definition).await {
            Ok(value) => value,
            Err(error) => {
                let path = report_path(if capacity {
                    "cluster-capacity.json"
                } else {
                    "cluster-candidate.json"
                });
                write_failed_capture(&path, &environment, &error.to_string(), &[FIRST_PROBE_RATE])?;
                return Err(error);
            }
        };
        let rates = if capacity {
            wyrd_bench::CANONICAL_CAPACITY_RATES.to_vec()
        } else {
            // Qualification replays reviewed absolute rates. The discovered
            // knee remains diagnostic provenance only.
            wyrd_bench::DEFAULT_REVIEWED_QUALIFICATION_RATES.to_vec()
        };
        for rate in rates {
            // Percent-of-knee is diagnostic-only and is never persisted in a
            // v2 report or used for compatibility.
            let percent = 0;
            let mut trials = Vec::with_capacity(3);
            let mut scenario = None;
            for trial in 1..=3 {
                let (identity, result) = match run_reference_trial(
                    definition,
                    rate.max(1),
                    percent,
                    trial,
                    knee,
                    provenance,
                )
                .await
                {
                    Ok(value) => value,
                    Err(error) => {
                        let path = report_path(if capacity {
                            "cluster-capacity.json"
                        } else {
                            "cluster-candidate.json"
                        });
                        write_failed_capture(
                            &path,
                            &environment,
                            &error.to_string(),
                            &reports
                                .iter()
                                .map(|report: &ClusterScenarioReport| {
                                    report.scenario.offered_requests_per_second
                                })
                                .chain(std::iter::once(rate))
                                .collect::<Vec<_>>(),
                        )?;
                        return Err(error);
                    }
                };
                scenario = Some(identity);
                trials.push(result);
            }
            let median = derive_trial_median(&trials)?;
            reports.push(ClusterScenarioReport {
                scenario: scenario.ok_or("reference trials emitted no scenario")?,
                discovered_knee_requests_per_second: knee,
                discovered_knee_provenance: provenance,
                trials,
                trial_evidence: Vec::new(),
                median,
                capacity_stages: Vec::new(),
            });
        }
    }
    let profile = BifrostReferenceProfile {
        schema_version: CLUSTER_REPORT_VERSION.to_owned(),
        environment: environment.clone(),
        scenarios: reviewed_scenario_profiles(reports, &environment_storage_identity(&environment)),
        slos: BifrostSloEnvelope::default(),
    };
    let path = report_path(if capacity {
        "cluster-capacity.json"
    } else {
        "cluster-candidate.json"
    });
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&profile)?),
    )?;
    let validation = selected_scenario.map_or_else(
        || profile.validate_reference(),
        |scenario_id| profile.validate_selected(scenario_id),
    );
    if let Err(error) = validation {
        write_qualification_diagnostic(&path, &profile, &error)?;
        if !capacity {
            return Err(Box::new(error));
        }
    }
    Ok(path)
}

/// Group flat trial reports into the persisted reviewed-scenario contract.
///
/// Reports are sorted by absolute offered rate inside each fixed scenario so
/// qualification replay and comparison consume one deterministic shape.
#[must_use]
fn reviewed_scenario_profiles(
    reports: Vec<ClusterScenarioReport>,
    storage_runtime_identity: &str,
) -> Vec<ReviewedScenarioProfile> {
    let mut grouped = BTreeMap::<String, Vec<ClusterScenarioReport>>::new();
    for report in reports {
        grouped
            .entry(report.scenario.scenario_id.clone())
            .or_default()
            .push(report);
    }
    grouped
        .into_iter()
        .filter_map(|(scenario_id, mut reports)| {
            reports.sort_by_key(|report| report.scenario.offered_requests_per_second);
            let first = reports.first()?;
            let capacity = !first.capacity_stages.is_empty();
            let ordered_rates = if capacity {
                first
                    .capacity_stages
                    .iter()
                    .map(|stage| stage.offered_requests_per_second)
                    .collect()
            } else {
                reports
                    .iter()
                    .map(|report| report.scenario.offered_requests_per_second)
                    .collect()
            };
            Some(ReviewedScenarioProfile {
                scenario_id: scenario_id.clone(),
                workload: ClusterWorkloadIdentity {
                    scenario_id,
                    topology: first.scenario.topology,
                    tenants: first.scenario.tenants,
                    traffic: first.scenario.traffic,
                    rows_per_batch: first.scenario.rows_per_batch,
                    query_row_limit: first.scenario.query_row_limit,
                    auth_cache_ttl_seconds: wyrd_bench::AUTH_CACHE_TTL_SECONDS,
                    auth_invalidation_mode: wyrd_bench::AUTH_INVALIDATION_MODE.to_owned(),
                    flush_policy: wyrd_bench::FLUSH_POLICY.to_owned(),
                    routing_policy: wyrd_bench::ROUTING_POLICY.to_owned(),
                    batching_policy: wyrd_bench::BATCHING_POLICY.to_owned(),
                    storage_runtime_identity: storage_runtime_identity.to_owned(),
                    allocation_policy: wyrd_bench::ALLOCATION_POLICY.to_owned(),
                    tables_per_tenant: if capacity {
                        CAPACITY_TABLES_PER_TENANT
                    } else {
                        u32::try_from(reports.len() * 3 * 2).unwrap_or(u32::MAX)
                    },
                    ordered_rates,
                    trial_count: if capacity { 1 } else { 3 },
                    warmup_seconds: 10,
                    conditioning_seconds: 3,
                    measured_seconds: 20,
                },
                offered_rates: reports
                    .iter()
                    .map(|report| report.scenario.offered_requests_per_second)
                    .collect(),
                reports,
            })
        })
        .collect()
}

/// Derive the workload storage identity from inspected environment evidence.
#[must_use]
fn environment_storage_identity(environment: &BenchmarkEnvironment) -> String {
    format!(
        "{}:{}:{}",
        environment.storage_backend_kind,
        environment.storage_root,
        environment.storage_device_class
    )
}

/// Persist a machine-readable non-promotable qualification result beside a capture.
///
/// # Errors
/// Returns an IO or JSON error when the diagnostic cannot be serialized or written.
fn write_qualification_diagnostic(
    report_path: &Path,
    profile: &BifrostReferenceProfile,
    error: &ClusterBenchmarkError,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let diagnostic_path = report_path.with_extension("qualification.json");
    std::fs::write(
        diagnostic_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&qualification_diagnostic(profile, error))?
        ),
    )?;
    Ok(())
}

/// Persist a v2 non-promotable report when setup or the first probe fails.
///
/// Keeping this envelope on disk makes a failed or partial run auditable and
/// prevents the old behavior of silently dropping the first failed probe.
///
/// # Errors
/// Returns an IO or JSON error when the diagnostic cannot be serialized or
/// written.
fn write_failed_capture(
    report_path: &Path,
    environment: &BenchmarkEnvironment,
    error: &str,
    attempted_rates: &[u64],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    write_partial_capture(
        report_path,
        environment,
        error,
        attempted_rates
            .iter()
            .map(|rate| AttemptedProbe {
                scenario_id: None,
                offered_requests_per_second: *rate,
                completed: false,
                stop_reasons: vec![CapacityLimit::DependencySlo],
            })
            .collect(),
        Vec::new(),
    )
}

/// Persist completed reports and the current incomplete probe after a failure.
///
/// # Errors
/// Returns an IO or JSON error when the diagnostic cannot be written.
fn write_partial_capture(
    report_path: &Path,
    environment: &BenchmarkEnvironment,
    error: &str,
    attempted_probes: Vec<AttemptedProbe>,
    reports: Vec<ClusterScenarioReport>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let report = BifrostDiagnosticReport::failure(
        DiagnosticStatus::NotReady,
        error.to_owned(),
        Some(environment.clone()),
        attempted_probes,
        reviewed_scenario_profiles(reports, &environment_storage_identity(environment)),
    );
    std::fs::write(
        report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    Ok(())
}

/// Append the current probe before any fallible measurement work begins.
fn begin_probe(probes: &mut Vec<AttemptedProbe>, scenario_id: String, rate: u64) {
    probes.push(AttemptedProbe {
        scenario_id: Some(scenario_id),
        offered_requests_per_second: rate,
        completed: false,
        stop_reasons: Vec::new(),
    });
}

/// Mark only the most recently attempted probe complete.
fn complete_current_probe(probes: &mut [AttemptedProbe], stop_reasons: &[CapacityLimit]) {
    if let Some(probe) = probes.last_mut() {
        probe.completed = true;
        probe.stop_reasons.clone_from_slice(stop_reasons);
    }
}

/// Persist a v2 diagnostic when environment discovery itself is unsupported.
///
/// # Errors
/// Returns an IO or JSON error when the diagnostic cannot be written.
fn write_unavailable_capture(
    report_path: &Path,
    error: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let report = BifrostDiagnosticReport::failure(
        DiagnosticStatus::Unsupported,
        error.to_owned(),
        None,
        Vec::new(),
        Vec::new(),
    );
    std::fs::write(
        report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    Ok(())
}

/// Build exact deficient-trial evidence without changing the strict qualification gate.
#[must_use]
fn qualification_diagnostic(
    profile: &BifrostReferenceProfile,
    error: &ClusterBenchmarkError,
) -> serde_json::Value {
    let status = match error {
        ClusterBenchmarkError::Invalid(_) => "invalid",
        ClusterBenchmarkError::Unsupported(_) => "unsupported",
        ClusterBenchmarkError::NotReady(_) => "not_ready",
        ClusterBenchmarkError::Incompatible(_) => "incompatible",
    };
    let deficiencies = profile
        .scenarios
        .iter()
        .flat_map(|reviewed| {
            reviewed.reports.iter().flat_map(|report| {
                report
                    .trials
                    .iter()
                    .filter_map(|trial| trial_deficiency(report, trial))
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "schema_version": "wyrd.bifrost.qualification-diagnostic/v1",
        "promotable": false,
        "status": status,
        "error": error.to_string(),
        "report_schema_version": profile.schema_version,
        "git_sha": profile.environment.git_sha,
        "deficiencies": deficiencies,
    })
}

/// Return exact machine-readable evidence when one trial misses its sample floor.
#[must_use]
fn trial_deficiency(
    report: &ClusterScenarioReport,
    trial: &ClusterBenchmarkTrial,
) -> Option<serde_json::Value> {
    let client = &trial.client;
    let floor = report.scenario.minimum_samples;
    if client.durable_write.samples >= floor
        && client.query_time_to_first_frame.samples >= floor
        && client.total_query.samples >= floor
    {
        return None;
    }
    Some(serde_json::json!({
        "scenario_id": report.scenario.scenario_id,
        "load_percent": report.scenario.offered_load_percent,
        "offered_requests_per_second": report.scenario.offered_requests_per_second,
        "knee_requests_per_second": report.discovered_knee_requests_per_second,
        "knee_provenance": report.discovered_knee_provenance,
        "trial": trial.trial,
        "minimum_samples": floor,
        "planned_operations": client.planned_operations,
        "attempted_operations": client.attempted_operations,
        "accepted_operations": client.accepted_operations,
        "successful_writes": client.durable_write.samples,
        "successful_query_ttfb": client.query_time_to_first_frame.samples,
        "successful_query_total": client.total_query.samples,
        "retry_operations": client.retry_operations,
        "backpressure_operations": client.backpressure_operations,
        "in_flight_cap_exhaustions": client.in_flight_cap_exhaustions,
        "max_in_flight": client.max_in_flight,
        "missed_operations": client.missed_operations,
    }))
}

/// Discover one scenario knee by doubling from eight requests per second.
async fn calibrate(
    definition: ReferenceScenarioDefinition,
) -> Result<(u64, KneeProvenance), Box<dyn std::error::Error + Send + Sync>> {
    let mut rate = FIRST_PROBE_RATE;
    let mut last_passing = None;
    loop {
        let measured_seconds = calibration_seconds(rate, definition)?;
        let (_, trial) = run_trial_with_windows(ReferenceTrialSpec {
            definition,
            rate,
            load_percent: 100,
            trial: 1,
            knee_provenance: KneeProvenance::Discovered,
            warmup: Duration::from_secs(3),
            measured: Duration::from_secs(u64::from(measured_seconds)),
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
        })
        .await?;
        let client = &trial.client;
        let relevant_samples =
            client.durable_write.samples >= 30 && client.query_time_to_first_frame.samples >= 30;
        let passes = relevant_samples
            && client.durable_write.p99_us <= 100_000
            && client.flush_to_visible.p99_us <= 5_000_000
            && client.query_time_to_first_frame.p99_us <= 500_000
            && client.backpressure_ratio <= 0.01
            && client.missed_operations == 0;
        if !passes {
            return last_passing.map_or_else(
                || {
                    Err(Box::<dyn std::error::Error + Send + Sync>::from(
                        ClusterBenchmarkError::NotReady(
                            format!(
                                "initial eight-request/second calibration probe failed: durable_write_samples={}, query_samples={}, durable_write_p99_us={}, flush_to_visible_p99_us={}, query_time_to_first_frame_p99_us={}, backpressure_ratio={}, missed_operations={}",
                                client.durable_write.samples,
                                client.query_time_to_first_frame.samples,
                                client.durable_write.p99_us,
                                client.flush_to_visible.p99_us,
                                client.query_time_to_first_frame.p99_us,
                                client.backpressure_ratio,
                                client.missed_operations,
                            ),
                        ),
                    ))
                },
                |knee| Ok((knee, KneeProvenance::Discovered)),
            );
        }
        last_passing = Some(rate);
        if rate == CALIBRATION_RATE_CAP {
            return Ok((rate, KneeProvenance::CensoredAtCap));
        }
        rate = rate.saturating_mul(2).min(CALIBRATION_RATE_CAP);
    }
}

/// Select the first five-second increment with thirty attempts per operation.
fn calibration_seconds(
    rate: u64,
    definition: ReferenceScenarioDefinition,
) -> Result<u32, ClusterBenchmarkError> {
    (5..=45)
        .step_by(5)
        .find(|seconds| {
            let attempts = rate.saturating_mul(u64::from(*seconds));
            let (writes, reads) = operation_attempt_counts(attempts, definition.write_percent);
            writes >= 30 && reads >= 30
        })
        .ok_or_else(|| {
            ClusterBenchmarkError::Unsupported(
                "calibration cannot reach thirty attempts per operation by 45 seconds".to_owned(),
            )
        })
}

/// Select one deterministic interleaved traffic class from the fixed seed.
#[must_use]
fn is_write_operation(ordinal: u64, write_percent: u8) -> bool {
    (ordinal.saturating_mul(37).saturating_add(0xB1_F057) % 100) < u64::from(write_percent)
}

/// Count the actual deterministic operation classes scheduled in one window.
#[must_use]
fn operation_attempt_counts(attempts: u64, write_percent: u8) -> (usize, usize) {
    let writes = (0..attempts)
        .filter(|ordinal| is_write_operation(*ordinal, write_percent))
        .count();
    let reads = usize::try_from(attempts)
        .unwrap_or(usize::MAX)
        .saturating_sub(writes);
    (writes, reads)
}

/// Execute one real open-loop controlled trial through the public SDK/Gate path.
///
/// Every operation is scheduled from an immutable planned instant. Falling
/// behind by more than one interval records a missed operation instead of
/// shifting later work and hiding coordinated omission. A separate coordinator
/// owns the ten fixed measured flush instants.
///
/// # Errors
/// Returns a typed environment, cluster, public-client, protocol, telemetry,
/// correctness, audit, or cleanup failure. Partial trials are never returned.
pub async fn run_reference_trial(
    definition: ReferenceScenarioDefinition,
    offered_requests_per_second: u64,
    offered_load_percent: u8,
    trial: u8,
    _knee: u64,
    knee_provenance: KneeProvenance,
) -> Result<
    (ClusterBenchmarkScenario, ClusterBenchmarkTrial),
    Box<dyn std::error::Error + Send + Sync>,
> {
    run_trial_with_windows(ReferenceTrialSpec {
        definition,
        rate: offered_requests_per_second,
        load_percent: offered_load_percent,
        trial,
        knee_provenance,
        warmup: Duration::from_secs(10),
        measured: Duration::from_secs(20),
        max_in_flight: DEFAULT_MAX_IN_FLIGHT,
    })
    .await
}

/// Immutable execution identity for one fresh calibration or reference trial.
#[derive(Debug, Clone, Copy)]
struct ReferenceTrialSpec {
    /// Fixed D22 scenario.
    definition: ReferenceScenarioDefinition,
    /// Absolute offered public request rate.
    rate: u64,
    /// Reference-knee percentage represented by the rate.
    load_percent: u8,
    /// One-based independent trial number.
    trial: u8,
    /// Discovered or censored knee provenance.
    knee_provenance: KneeProvenance,
    /// Warmup window excluded from evidence.
    warmup: Duration,
    /// Measured evidence window.
    measured: Duration,
    /// Profile-driven concurrent public operation cap.
    max_in_flight: usize,
}

/// One live, benchmark-only capacity scenario lifecycle.
///
/// The session owns the T15 cluster, tenant-bound public clients, exact
/// fourteen-table allocation, stage allocator, cumulative identity ledger,
/// and terminal shutdown. It deliberately cannot clone the cluster owner.
pub struct CapacityScenarioSession {
    /// Live T15 cluster, retained until the single terminal shutdown.
    cluster: Option<WyrdTestCluster>,
    /// Fixed scenario definition for every stage.
    definition: ReferenceScenarioDefinition,
    /// Tenant roster provisioned once during boot.
    tenants: Vec<DataTenantId>,
    /// Authenticated public Gate/Oracle clients provisioned once during boot.
    clients: Vec<ReferenceClient>,
    /// Closed discovery, confirmation, and recovery transition owner.
    stages: CapacityStateMachine,
    /// Exact boot-provisioned table names in allocation order.
    tables: Vec<String>,
    /// Cumulative expected row identities by tenant.
    expected_rows: Vec<BTreeSet<u64>>,
    /// Measurement tables completed and eligible for final cumulative union.
    completed_tables: Vec<String>,
}

impl CapacityScenarioSession {
    /// Boot one T15 cluster and provision exactly fourteen tables per tenant.
    ///
    /// # Errors
    /// Returns a cluster, tenant, authentication, provisioning, or allocation
    /// inspection error. The caller must still invoke [`Self::shutdown`]
    /// whenever this constructor returns a session.
    pub async fn start(
        definition: ReferenceScenarioDefinition,
        storage_root: PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let tables = capacity_table_names();
        if tables.len() != usize::try_from(CAPACITY_TABLES_PER_TENANT)? {
            return Err("capacity allocation did not contain exactly fourteen tables".into());
        }
        Self::start_with_tables(definition, tables, storage_root).await
    }

    /// Boot one live scenario with a formula-preflighted table allocation.
    ///
    /// # Errors
    /// Returns the same lifecycle, tenant, authentication, and catalog errors
    /// as [`Self::start`].
    async fn start_with_tables(
        definition: ReferenceScenarioDefinition,
        tables: Vec<String>,
        storage_root: PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let cluster_spec = match definition.topology {
            ClusterTopology::OnePod => BifrostClusterSpec::one_mixed(),
            ClusterTopology::ThreeServersThreeForgeWorkers => {
                BifrostClusterSpec::three_servers_three_forge_workers()
            }
        };
        let storage_root = storage_root.canonicalize()?;
        let cluster = WyrdTestCluster::start_spec_with_forge_observer_and_storage_root(
            cluster_spec,
            storage_root.clone(),
        )
        .await?;
        if cluster.storage_root() != storage_root {
            return Err("declared and live cluster storage roots differ".into());
        }
        let tenants = provision_reference_tenants(&cluster, definition.tenants as usize).await?;
        provision_named_tables(&cluster, &tenants, &tables).await?;
        provision_named_tables(&cluster, &tenants, &[REFERENCE_TABLE.to_owned()]).await?;
        let clients = reference_clients(&cluster, &tenants).await?;
        preload_reference_rows(&cluster, &tenants, &clients).await?;
        await_forge_convergence(&cluster, &tenants).await?;
        let expected_rows = vec![BTreeSet::new(); tenants.len()];
        Ok(Self {
            cluster: Some(cluster),
            definition,
            tenants,
            clients,
            stages: CapacityStateMachine::canonical(),
            tables,
            expected_rows,
            completed_tables: Vec::new(),
        })
    }

    /// Return the next stage plan without creating runtime state or tables.
    #[must_use]
    pub fn next_stage(&mut self) -> Option<CapacityStagePlan> {
        self.stages.next_plan()
    }

    /// Return stable session cardinalities used by runtime allocation checks.
    #[must_use]
    pub fn allocation_identity(&self) -> (&str, usize, usize, usize) {
        (
            self.definition.id,
            self.tenants.len(),
            self.clients.len(),
            self.expected_rows.len(),
        )
    }

    /// Run the exact ten-second scenario warmup against the shared warmup table.
    ///
    /// # Errors
    /// Returns a public Gate workload error or a repeated-shutdown error.
    async fn run_initial_warmup(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cluster = self
            .cluster
            .as_ref()
            .ok_or("capacity session is shut down")?;
        WindowRun {
            cluster,
            tenants: &self.tenants,
            clients: &self.clients,
            definition: self.definition,
            rate: wyrd_bench::CANONICAL_CAPACITY_RATES[0],
            duration: Duration::from_secs(10),
            measured: false,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            table: CAPACITY_WARMUP_TABLE,
            phase_ordinal_base: 0,
        }
        .run()
        .await?;
        Ok(())
    }

    /// Return the deterministic measurement table assigned to a stage.
    ///
    /// # Errors
    /// Returns an allocation error when the stage slot is outside the thirteen
    /// measured slots provisioned at boot.
    pub fn stage_table(
        &self,
        plan: CapacityStagePlan,
    ) -> Result<&str, Box<dyn std::error::Error + Send + Sync>> {
        self.tables
            .get(usize::from(plan.slot) + 1)
            .map(String::as_str)
            .ok_or_else(|| "capacity stage exceeded its boot-provisioned table allocation".into())
    }

    /// Record one completed stage transition after the public workload owner
    /// has retained its full evidence.
    ///
    /// # Errors
    /// Returns the state-machine error for invalid terminal/recovery ordering.
    pub fn record_stage(
        &mut self,
        plan: CapacityStagePlan,
        passed: bool,
    ) -> Result<(), ClusterBenchmarkError> {
        self.stages.record(plan, passed)
    }

    /// Drive one conditioned measured stage through the public Gate workload.
    ///
    /// # Errors
    /// Returns a public workload, allocation, telemetry, or lifecycle error.
    async fn run_stage(
        &mut self,
        plan: CapacityStagePlan,
        measured: Duration,
    ) -> Result<CapacityStageRun, Box<dyn std::error::Error + Send + Sync>> {
        let cluster = self
            .cluster
            .as_ref()
            .ok_or("capacity session is shut down")?;
        WindowRun {
            cluster,
            tenants: &self.tenants,
            clients: &self.clients,
            definition: self.definition,
            rate: plan.offered_requests_per_second,
            duration: Duration::from_secs(3),
            measured: false,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            table: CAPACITY_WARMUP_TABLE,
            phase_ordinal_base: 1_000_000 + u64::from(plan.slot) * 100_000,
        }
        .run()
        .await?;
        let checkpoint = cluster.telemetry().checkpoint()?;
        let audit_before = audit_rows(cluster, &self.tenants).await?;
        let result = WindowRun {
            cluster,
            tenants: &self.tenants,
            clients: &self.clients,
            definition: self.definition,
            rate: plan.offered_requests_per_second,
            duration: measured,
            measured: true,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            table: self.stage_table(plan)?,
            phase_ordinal_base: 2_000_000 + u64::from(plan.slot) * 100_000,
        }
        .run()
        .await?;
        flush_tenant_writers(cluster, &self.tenants).await?;
        await_forge_convergence(cluster, &self.tenants).await?;
        let telemetry = cluster.telemetry().delta_since(&checkpoint)?;
        let audit_after = audit_rows(cluster, &self.tenants).await?;
        let published = final_published_identities(
            cluster,
            &self.tenants,
            &self.clients,
            self.stage_table(plan)?,
        )
        .await?;
        for (tenant, ordinals) in result.tenant_write_ordinals.iter().enumerate() {
            let expected = stage_row_identity(tenant, ordinals)
                .row_ids
                .into_iter()
                .collect::<BTreeSet<_>>();
            if published[tenant] != expected {
                return Err(
                    "stage public Oracle identities differ from acknowledged writes".into(),
                );
            }
            self.expected_rows[tenant].extend(expected);
        }
        self.completed_tables
            .push(self.stage_table(plan)?.to_owned());
        Ok(CapacityStageRun {
            client: result,
            telemetry,
            published_rows: published.iter().map(BTreeSet::len).sum::<usize>() as u64,
            audit_rows: audit_after.saturating_sub(audit_before),
        })
    }

    /// Re-read every assigned stage table and compare the exact cumulative union.
    ///
    /// # Errors
    /// Returns an exact-identity error when any earlier stage is missing,
    /// duplicated, corrupted, or visible under the wrong tenant.
    async fn reconcile_cumulative(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cluster = self
            .cluster
            .as_ref()
            .ok_or("capacity session is shut down")?;
        let mut observed = vec![BTreeSet::new(); self.tenants.len()];
        for table in &self.completed_tables {
            let identities =
                final_published_identities(cluster, &self.tenants, &self.clients, table).await?;
            for (tenant, rows) in identities.into_iter().enumerate() {
                observed[tenant].extend(rows);
            }
        }
        if !exact_identity_ledger_matches(&self.expected_rows, &observed) {
            return Err(
                "final cumulative Oracle union differs from the exact expected identity ledger"
                    .into(),
            );
        }
        Ok(())
    }

    /// Run one qualification rate/trial pair against its dedicated warmup and
    /// untouched measurement tables.
    ///
    /// # Errors
    /// Returns a table-allocation, public workload, telemetry, convergence,
    /// correctness, or audit error.
    async fn run_qualification_pair(
        &self,
        pair: usize,
        rate: u64,
        measured: Duration,
    ) -> Result<
        (
            WindowResult,
            ProductionTelemetryEvidence,
            crate::bifrost::telemetry::ForgeTelemetryDelta,
        ),
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let cluster = self
            .cluster
            .as_ref()
            .ok_or("qualification session is shut down")?;
        let warmup_table = self
            .tables
            .get(pair * 2)
            .ok_or("missing qualification warmup table")?;
        let measurement_table = self
            .tables
            .get(pair * 2 + 1)
            .ok_or("missing qualification measurement table")?;
        for duration in [Duration::from_secs(10), Duration::from_secs(3)] {
            WindowRun {
                cluster,
                tenants: &self.tenants,
                clients: &self.clients,
                definition: self.definition,
                rate,
                duration,
                measured: false,
                max_in_flight: DEFAULT_MAX_IN_FLIGHT,
                table: warmup_table,
                phase_ordinal_base: 10_000_000 + pair as u64 * 100_000,
            }
            .run()
            .await?;
        }
        let checkpoint = cluster.telemetry().checkpoint()?;
        let audit_before = audit_rows(cluster, &self.tenants).await?;
        let result = WindowRun {
            cluster,
            tenants: &self.tenants,
            clients: &self.clients,
            definition: self.definition,
            rate,
            duration: measured,
            measured: true,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            table: measurement_table,
            phase_ordinal_base: 20_000_000 + pair as u64 * 100_000,
        }
        .run()
        .await?;
        flush_tenant_writers(cluster, &self.tenants).await?;
        await_forge_convergence(cluster, &self.tenants).await?;
        let telemetry = cluster.telemetry().delta_since(&checkpoint)?;
        let audit_after = audit_rows(cluster, &self.tenants).await?;
        let published =
            final_published_rows(cluster, &self.tenants, &self.clients, measurement_table).await?;
        let production = reconcile_production(
            &telemetry,
            &result,
            audit_after.saturating_sub(audit_before),
            published,
        )?;
        Ok((result, production, telemetry))
    }

    /// Shut the live cluster down exactly once and require complete cleanup.
    ///
    /// # Errors
    /// Returns a lifecycle error for repeated shutdown or retained listeners,
    /// queues, leases, slots, claims, attempts, or supervised tasks.
    pub async fn shutdown(mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cluster = self
            .cluster
            .take()
            .ok_or("capacity session already shut down")?;
        let cleanup = cluster.shutdown_and_inspect().await?;
        if !cleanup.listeners_stopped
            || !cleanup.servers_stopped
            || cleanup.scribe_queued != 0
            || cleanup.scribe_inflight != 0
            || cleanup.scribe_wal_streams != 0
            || cleanup.oracle_leases != 0
            || cleanup.oracle_slots != 0
            || cleanup.forge_active_claims != 0
            || cleanup.forge_active_attempts != 0
            || cleanup.supervised_tasks != 0
        {
            return Err(format!("capacity session retained resources: {cleanup:?}").into());
        }
        Ok(())
    }
}

/// Compare an observed public-Oracle union with the acknowledged identity ledger.
#[must_use]
fn exact_identity_ledger_matches(expected: &[BTreeSet<u64>], observed: &[BTreeSet<u64>]) -> bool {
    expected == observed
}

/// Build the exact one-warmup plus thirteen-stage capacity allocation.
#[must_use]
pub fn capacity_table_names() -> Vec<String> {
    std::iter::once(CAPACITY_WARMUP_TABLE.to_owned())
        .chain(
            (0..CAPACITY_STAGE_TABLE_COUNT).map(|slot| format!("cluster_capacity_stage_{slot:02}")),
        )
        .collect()
}

/// Build dedicated qualification warmup/measurement table pairs in closed
/// rate-major, trial-minor order.
#[must_use]
fn qualification_table_names(rate_count: usize) -> Vec<String> {
    (0..rate_count * 3)
        .flat_map(|pair| {
            [
                format!("cluster_qualification_{pair:02}_warmup"),
                format!("cluster_qualification_{pair:02}_measurement"),
            ]
        })
        .collect()
}

/// Execute one fresh-cluster trial with caller-selected calibration/reference windows.
async fn run_trial_with_windows(
    spec: ReferenceTrialSpec,
) -> Result<
    (ClusterBenchmarkScenario, ClusterBenchmarkTrial),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let cluster_spec = match spec.definition.topology {
        ClusterTopology::OnePod => BifrostClusterSpec::one_mixed(),
        ClusterTopology::ThreeServersThreeForgeWorkers => {
            BifrostClusterSpec::three_servers_three_forge_workers()
        }
    };
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(cluster_spec).await?;
    let result = run_live_trial(&cluster, &spec).await;
    let shutdown = cluster.shutdown_and_inspect().await;
    match (result, shutdown) {
        (Ok((scenario, mut result)), Ok(cleanup)) => {
            result.production.cleanup = if cleanup.listeners_stopped
                && cleanup.servers_stopped
                && cleanup.scribe_queued == 0
                && cleanup.scribe_inflight == 0
                && cleanup.scribe_wal_streams == 0
                && cleanup.oracle_leases == 0
                && cleanup.oracle_slots == 0
                && cleanup.forge_active_claims == 0
                && cleanup.forge_active_attempts == 0
                && cleanup.supervised_tasks == 0
            {
                EvidenceStatus::Complete
            } else {
                EvidenceStatus::Failed
            };
            if result.production.cleanup != EvidenceStatus::Complete {
                return Err(format!(
                    "reference trial retained cluster resources after shutdown: {cleanup:?}"
                )
                .into());
            }
            Ok((scenario, result))
        }
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(Box::new(error)),
        (Err(error), Err(shutdown)) => Err(format!("{error}; shutdown: {shutdown}").into()),
    }
}

/// Execute setup, warmup, measurement, fixed flushes, and reconciliation while
/// the outer owner retains deterministic shutdown responsibility.
async fn run_live_trial(
    cluster: &WyrdTestCluster,
    spec: &ReferenceTrialSpec,
) -> Result<
    (ClusterBenchmarkScenario, ClusterBenchmarkTrial),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let definition = spec.definition;
    let tenants = provision_reference_tenants(cluster, definition.tenants as usize).await?;
    provision_reference_tables(cluster, &tenants).await?;
    let clients = reference_clients(cluster, &tenants).await?;
    preload_reference_rows(cluster, &tenants, &clients).await?;
    let warmup = WindowRun {
        cluster,
        tenants: &tenants,
        clients: &clients,
        definition,
        rate: spec.rate,
        duration: spec.warmup,
        measured: false,
        max_in_flight: spec.max_in_flight,
        table: REFERENCE_TABLE,
        phase_ordinal_base: 1_000_000,
    }
    .run()
    .await?;
    flush_tenant_writers(cluster, &tenants).await?;
    await_forge_convergence(cluster, &tenants).await?;
    let published_before =
        final_published_rows(cluster, &tenants, &clients, REFERENCE_TABLE).await?;
    let expected_before = PRELOAD_ROWS
        .saturating_mul(tenants.len() as u64)
        .saturating_add(warmup.tenant_write_rows.iter().sum::<u64>());
    if published_before != expected_before {
        return Err(format!(
            "post-warmup published rows {published_before} do not equal preload plus acknowledged warmup rows {expected_before}"
        )
        .into());
    }
    prove_wrong_tenant_query(cluster, &tenants, &clients).await?;
    let checkpoint = cluster.telemetry().checkpoint()?;
    let audit_before = audit_rows(cluster, &tenants).await?;
    let measured = WindowRun {
        cluster,
        tenants: &tenants,
        clients: &clients,
        definition,
        rate: spec.rate,
        duration: spec.measured,
        measured: true,
        max_in_flight: spec.max_in_flight,
        table: REFERENCE_TABLE,
        phase_ordinal_base: 2_000_000,
    }
    .run()
    .await?;
    flush_tenant_writers(cluster, &tenants).await?;
    await_forge_convergence(cluster, &tenants).await?;
    let audit_after = audit_rows(cluster, &tenants).await?;
    let telemetry = cluster.telemetry().delta_since(&checkpoint)?;
    let published_rows = final_published_rows(cluster, &tenants, &clients, REFERENCE_TABLE).await?;
    let measured_published_rows = published_rows
        .checked_sub(published_before)
        .ok_or("final publication cardinality regressed below the post-warmup baseline")?;
    let production = reconcile_production(
        &telemetry,
        &measured,
        audit_after.saturating_sub(audit_before),
        measured_published_rows,
    )?;
    let scenario = ClusterBenchmarkScenario {
        scenario_id: definition.id.to_owned(),
        workload_version: CLUSTER_WORKLOAD_VERSION.to_owned(),
        topology: definition.topology,
        tenants: definition.tenants,
        traffic: definition.traffic,
        rows_per_batch: ROWS_PER_WRITE,
        query_row_limit: 64,
        offered_requests_per_second: spec.rate,
        offered_load_percent: spec.load_percent,
        knee_provenance: spec.knee_provenance,
        warmup_seconds: spec.warmup.as_secs().try_into()?,
        measured_seconds: spec.measured.as_secs().try_into()?,
        trials: 3,
        minimum_samples: 200,
        max_in_flight: spec.max_in_flight,
        seed: 0xB1_F057,
    };
    Ok((
        scenario,
        ClusterBenchmarkTrial {
            trial: spec.trial,
            client: measured.metrics(),
            production,
        },
    ))
}

/// Client-side ledger accumulated by one open-loop window.
#[derive(Default)]
pub struct WindowResult {
    /// Exact concurrency cap used for this window.
    max_in_flight: usize,
    /// Planned-instant durable-write acknowledgement latencies.
    write_us: Vec<u64>,
    /// Planned-instant flush-to-strict-visibility latencies.
    flush_us: Vec<u64>,
    /// Planned-instant strict-query first-frame latencies.
    query_ttfb_us: Vec<u64>,
    /// Planned-instant strict-query terminal latencies.
    query_total_us: Vec<u64>,
    /// Durable acknowledged rows in tenant order.
    tenant_write_rows: Vec<u64>,
    /// Exact accepted write ordinals in tenant order.
    tenant_write_ordinals: Vec<BTreeSet<u64>>,
    /// Successful strict query counts in tenant order.
    tenant_queries: Vec<u64>,
    /// Rows decoded by measured throughput queries only.
    decoded_rows: u64,
    /// Rows decoded by every measured query including flush confirmations.
    telemetry_decoded_rows: u64,
    /// Successful measured queries including flush confirmations.
    telemetry_completed_queries: u64,
    /// Public operations submitted at planned instants.
    submitted: u64,
    /// Public operations accepted by Gate.
    accepted: u64,
    /// Bounded transient responses observed by the client.
    retries: u64,
    /// Stable admission/backpressure rejections observed by the client.
    backpressure: u64,
    /// Operations skipped after falling behind by more than one interval.
    missed_operations: u64,
    /// Operations refused solely because the declared cap was exhausted.
    in_flight_cap_exhaustions: u64,
    /// Fixed flush cadence instants missed by more than one cadence interval.
    missed_flushes: u64,
    /// Measured window denominator for absolute rates.
    measured_seconds: u64,
}

/// Live stage result retained until report evidence is reconciled.
struct CapacityStageRun {
    /// Complete client scheduler ledger.
    client: WindowResult,
    /// Production telemetry delta for only this stage.
    telemetry: crate::bifrost::telemetry::ForgeTelemetryDelta,
    /// Exact rows visible through public Oracle for this stage table.
    published_rows: u64,
    /// Durable audit rows committed during this stage.
    audit_rows: u64,
}

impl WindowResult {
    /// Merge one measured flush cadence result into the existing client ledger.
    fn merge_flush_result(&mut self, flush_result: (Vec<u64>, u64, u64, u64)) {
        self.flush_us = flush_result.0;
        self.missed_flushes = flush_result.1;
        self.telemetry_decoded_rows = self.telemetry_decoded_rows.saturating_add(flush_result.2);
        self.backpressure = self.backpressure.saturating_add(flush_result.3);
    }

    /// Convert the complete measured ledger into report distributions and rates.
    fn metrics(&self) -> ClientTrialMetrics {
        ClientTrialMetrics {
            planned_operations: self.submitted,
            attempted_operations: self.submitted.saturating_sub(self.missed_operations),
            accepted_operations: self.accepted,
            backpressure_operations: self.backpressure,
            retry_operations: self.retries,
            in_flight_cap_exhaustions: self.in_flight_cap_exhaustions,
            max_in_flight: u64::try_from(self.max_in_flight).unwrap_or(u64::MAX),
            durable_write: distribution(&self.write_us),
            flush_to_visible: distribution(&self.flush_us),
            query_time_to_first_frame: distribution(&self.query_ttfb_us),
            total_query: distribution(&self.query_total_us),
            durable_rows_per_second: self.tenant_write_rows.iter().sum::<u64>() as f64
                / self.measured_seconds.max(1) as f64,
            queries_per_second: self.tenant_queries.iter().sum::<u64>() as f64
                / self.measured_seconds.max(1) as f64,
            query_rows_per_second: self.decoded_rows as f64 / self.measured_seconds.max(1) as f64,
            backpressure_ratio: ratio(self.backpressure, self.submitted),
            retry_ratio: ratio(self.retries, self.accepted),
            write_fairness: jain_fairness(&self.tenant_write_rows),
            read_fairness: jain_fairness(&self.tenant_queries),
            missed_operations: self.missed_operations,
            missed_flushes: self.missed_flushes,
        }
    }
}

/// Reused tenant-bound public SDK handles for one fresh trial.
#[derive(Clone)]
struct ReferenceClient {
    /// Public gRPC Gate writer sharing the tenant-bound client identity.
    writer: BifrostGrpcTransport,
    /// Public HTTP Oracle query handle sharing the tenant-bound client identity.
    query: QueryClient,
}

/// One finished scheduled public operation.
enum OperationResult {
    /// One accepted durable write.
    Write {
        /// Tenant index used by fairness and reconciliation ledgers.
        tenant: usize,
        /// Acknowledgement latency from the planned dispatch instant.
        latency_us: u64,
        /// Durable rows acknowledged by Gate.
        rows: u64,
        /// Bounded retry attempts retaining the original batch identity.
        retries: u64,
        /// Exact scheduled write ordinal retained after acknowledgement.
        ordinal: u64,
    },
    /// One successful strict query.
    Query {
        /// Tenant index used by fairness and reconciliation ledgers.
        tenant: usize,
        /// First-frame latency from the planned dispatch instant.
        ttfb_us: u64,
        /// Terminal latency from the planned dispatch instant.
        total_us: u64,
        /// Rows decoded from the bounded query.
        rows: u64,
    },
    /// Stable public admission/backpressure rejection.
    Backpressure,
    /// Bounded transient public response eligible for retry accounting.
    Retry,
}

/// Own one warmup or measured open-loop window and its fixed flush cadence.
struct WindowRun<'a> {
    /// Live cluster used for flush and visibility operations.
    cluster: &'a WyrdTestCluster,
    /// Tenant identities distributed round-robin across operations.
    tenants: &'a [DataTenantId],
    /// Tenant-bound public SDK handles.
    clients: &'a [ReferenceClient],
    /// Fixed scenario traffic mix.
    definition: ReferenceScenarioDefinition,
    /// Absolute offered operation rate.
    rate: u64,
    /// Window duration.
    duration: Duration,
    /// Whether measured-only counters and flushes are enabled.
    measured: bool,
    /// Bounded public operation concurrency.
    max_in_flight: usize,
    /// Boot-provisioned phase table used by every operation in this window.
    table: &'a str,
    /// Disjoint deterministic identity base for this phase.
    phase_ordinal_base: u64,
}

impl WindowRun<'_> {
    /// Drive scheduled operations and optional fixed flushes from one start instant.
    ///
    /// # Errors
    /// Returns the first public SDK, strict-query, flush, or task-join error.
    async fn run(&self) -> Result<WindowResult, Box<dyn std::error::Error + Send + Sync>> {
        if self.rate == 0 {
            return Err("open-loop offered rate must be positive".into());
        }
        let start = tokio::time::Instant::now();
        let accepted_ordinals = Arc::new(std::sync::Mutex::new(vec![None; self.tenants.len()]));
        let operations = self.operations(start, Arc::clone(&accepted_ordinals));
        let flushes = self.flushes(start, accepted_ordinals);
        let (mut result, flush_result) = tokio::try_join!(operations, flushes)?;
        result.measured_seconds = self.duration.as_secs();
        result.merge_flush_result(flush_result);
        result.telemetry_completed_queries = result
            .telemetry_completed_queries
            .saturating_add(result.flush_us.len() as u64);
        Ok(result)
    }

    /// Schedule public writes and strict queries at immutable planned instants.
    ///
    /// # Errors
    /// Returns a public SDK or task-join error from one scheduled operation.
    async fn operations(
        &self,
        start: tokio::time::Instant,
        accepted_ordinals: Arc<std::sync::Mutex<Vec<Option<u64>>>>,
    ) -> Result<WindowResult, Box<dyn std::error::Error + Send + Sync>> {
        let interval = Duration::from_secs_f64(1.0 / self.rate as f64);
        let operation_count = self.rate.saturating_mul(self.duration.as_secs());
        let mut tasks = tokio::task::JoinSet::new();
        let mut tenant_write_ordinals = vec![0_u64; self.tenants.len()];
        let mut result = WindowResult {
            max_in_flight: self.max_in_flight,
            tenant_write_rows: vec![0; self.tenants.len()],
            tenant_write_ordinals: vec![BTreeSet::new(); self.tenants.len()],
            tenant_queries: vec![0; self.tenants.len()],
            ..WindowResult::default()
        };
        for ordinal in 0..operation_count {
            while let Some(joined) = tasks.try_join_next() {
                let operation = joined??;
                record_latest_accepted_ordinal(&accepted_ordinals, &operation)?;
                apply_operation(&mut result, operation);
            }
            let planned = start + interval.mul_f64(ordinal as f64);
            tokio::time::sleep_until(planned).await;
            result.submitted = result.submitted.saturating_add(1);
            if tokio::time::Instant::now() > planned + interval {
                result.missed_operations = result.missed_operations.saturating_add(1);
                continue;
            }
            if tasks.len() >= self.max_in_flight {
                result.in_flight_cap_exhaustions =
                    result.in_flight_cap_exhaustions.saturating_add(1);
                continue;
            }
            let tenant_index = ordinal as usize % self.tenants.len();
            let client = self.clients[tenant_index].clone();
            let write = ordinal == 0 || is_write_operation(ordinal, self.definition.write_percent);
            let table = operation_table(self.table, write).to_owned();
            // Each untouched measurement table begins with one public write so
            // later reads have a caller-visible working set without preload.
            let phase_ordinal = self.phase_ordinal_base + tenant_write_ordinals[tenant_index];
            if write {
                tenant_write_ordinals[tenant_index] =
                    tenant_write_ordinals[tenant_index].saturating_add(1);
            }
            tasks.spawn(async move {
                if write {
                    scheduled_write(client, &table, tenant_index, phase_ordinal, planned).await
                } else {
                    scheduled_query(client, &table, tenant_index, ordinal, planned).await
                }
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let operation = joined??;
            record_latest_accepted_ordinal(&accepted_ordinals, &operation)?;
            apply_operation(&mut result, operation);
        }
        Ok(result)
    }
}

/// Publish the latest accepted write ordinal for concurrent visibility probes.
///
/// # Errors
/// Returns an error when a prior task poisoned the shared visibility ledger.
fn record_latest_accepted_ordinal(
    ledger: &std::sync::Mutex<Vec<Option<u64>>>,
    operation: &OperationResult,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let OperationResult::Write {
        tenant, ordinal, ..
    } = operation
    {
        let mut accepted = ledger
            .lock()
            .map_err(|_| "accepted write ledger poisoned")?;
        accepted[*tenant] = Some(accepted[*tenant].map_or(*ordinal, |latest| latest.max(*ordinal)));
    }
    Ok(())
}

/// Route reads to the converged preload table and writes to the untouched phase table.
#[must_use]
fn operation_table(write_table: &str, write: bool) -> &str {
    if write { write_table } else { REFERENCE_TABLE }
}

/// Execute one planned durable write and preserve stable pressure classification.
async fn scheduled_write(
    client: ReferenceClient,
    table: &str,
    tenant_index: usize,
    ordinal: u64,
    planned: tokio::time::Instant,
) -> Result<OperationResult, Box<dyn std::error::Error + Send + Sync>> {
    let payload = reference_payload(
        tenant_row_base(tenant_index) + PRELOAD_ROWS + ordinal * 64,
        ROWS_PER_WRITE,
    )?;
    let frame = BifrostFrame {
        table: format!("vala.bifrost.{table}"),
        batch_id: deterministic_batch_id(tenant_index, ordinal),
        arrow_ipc: payload.into(),
    };
    let mut retries = 0_u64;
    loop {
        match client.writer.send_frame(frame.clone()).await {
            Ok(()) => {
                return Ok(OperationResult::Write {
                    tenant: tenant_index,
                    latency_us: elapsed_us(planned),
                    rows: u64::from(ROWS_PER_WRITE),
                    retries,
                    ordinal,
                });
            }
            Err(error) if is_backpressure(&error.to_string()) => {
                return Ok(OperationResult::Backpressure);
            }
            Err(error) if is_retryable(&error.to_string()) && retries < 2 => {
                retries = retries.saturating_add(1);
            }
            Err(error) if is_retryable(&error.to_string()) => return Ok(OperationResult::Retry),
            Err(error) => return Err(error.into()),
        }
    }
}

/// Execute one bounded strict query and separately measure first batch and terminal.
async fn scheduled_query(
    client: ReferenceClient,
    table: &str,
    tenant_index: usize,
    _ordinal: u64,
    planned: tokio::time::Instant,
) -> Result<OperationResult, Box<dyn std::error::Error + Send + Sync>> {
    let lower = tenant_row_base(tenant_index);
    let upper = lower.saturating_add(u64::MAX / 2);
    let request = BifrostQueryRequest {
        sql: format!(
            "SELECT row_id, wyrd_event_time FROM vala.bifrost.{table} ORDER BY row_id LIMIT 64"
        ),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(5_000),
    };
    let mut stream = match client.query.query(&request).await {
        Ok(stream) => stream,
        Err(error) if is_backpressure(&error.to_string()) => {
            return Ok(OperationResult::Backpressure);
        }
        Err(error) if is_retryable(&error.to_string()) => return Ok(OperationResult::Retry),
        Err(error) => return Err(error.into()),
    };
    let mut first = None;
    let mut rows = 0_u64;
    let mut identities = BTreeSet::new();
    while let Some(batch) = stream.next_batch().await? {
        first.get_or_insert_with(|| elapsed_us(planned));
        collect_query_identities(&batch, lower, upper, &mut identities)?;
        rows = rows.saturating_add(batch.num_rows() as u64);
    }
    let terminal = stream
        .terminal()
        .ok_or("strict query completed without a terminal")?;
    if terminal.outcome != QueryTerminalOutcome::Success || rows != 64 || identities.len() != 64 {
        return Err("strict query did not return the exact 64 tenant-bound row identities".into());
    }
    Ok(OperationResult::Query {
        tenant: tenant_index,
        ttfb_us: first.unwrap_or_else(|| elapsed_us(planned)),
        total_us: elapsed_us(planned),
        rows,
    })
}

/// Start an ancillary strict query across bounded production admission races.
///
/// Measured workload queries account a transient response directly; setup,
/// isolation, flush, and final-correctness queries instead need one completed
/// observation and retry at most 32 immediate admission handoffs.
///
/// # Errors
/// Returns the first non-retryable public-client error or an error after 32
/// retryable responses.
async fn ancillary_query(
    client: &QueryClient,
    request: &BifrostQueryRequest,
) -> Result<QueryResultStream, Box<dyn std::error::Error + Send + Sync>> {
    for _ in 0..32 {
        match client.query(request).await {
            Ok(stream) => return Ok(stream),
            Err(error) if is_retryable(&error.to_string()) => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err("ancillary strict query exhausted 32 bounded admission retries".into())
}

/// Collect caller-visible row identities and reject schema, range, or duplicate drift.
///
/// # Errors
/// Returns an error when Oracle exposes a non-contract column, omits the event
/// timestamp, returns an out-of-range row, or repeats an identity.
fn collect_query_identities(
    batch: &RecordBatch,
    lower: u64,
    upper: u64,
    identities: &mut BTreeSet<u64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let schema = batch.schema();
    if schema.fields().len() != 2
        || schema.field(0).name() != "row_id"
        || schema.field(1).name() != "wyrd_event_time"
    {
        return Err("Oracle returned a non-contract caller-visible schema".into());
    }
    let row_ids = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or("Oracle row_id column is not Int64")?;
    for value in row_ids.values() {
        let value = u64::try_from(*value)?;
        if value < lower || value > upper || !identities.insert(value) {
            return Err("Oracle returned an out-of-range or duplicate row identity".into());
        }
    }
    Ok(())
}

/// Apply one operation outcome to exact client ledgers.
fn apply_operation(result: &mut WindowResult, operation: OperationResult) {
    match operation {
        OperationResult::Write {
            tenant,
            latency_us,
            rows,
            retries,
            ordinal,
        } => {
            result.accepted = result.accepted.saturating_add(1);
            result.retries = result.retries.saturating_add(retries);
            result.write_us.push(latency_us);
            result.tenant_write_rows[tenant] =
                result.tenant_write_rows[tenant].saturating_add(rows);
            result.tenant_write_ordinals[tenant].insert(ordinal);
        }
        OperationResult::Query {
            tenant,
            ttfb_us,
            total_us,
            rows,
        } => {
            result.accepted = result.accepted.saturating_add(1);
            result.query_ttfb_us.push(ttfb_us);
            result.query_total_us.push(total_us);
            result.tenant_queries[tenant] = result.tenant_queries[tenant].saturating_add(1);
            result.decoded_rows = result.decoded_rows.saturating_add(rows);
            result.telemetry_decoded_rows = result.telemetry_decoded_rows.saturating_add(rows);
            result.telemetry_completed_queries =
                result.telemetry_completed_queries.saturating_add(1);
        }
        OperationResult::Backpressure => {
            result.backpressure = result.backpressure.saturating_add(1);
        }
        OperationResult::Retry => {
            result.retries = result.retries.saturating_add(1);
        }
    }
}

/// Execute ten immutable flush instants and strict visibility confirmations.
impl WindowRun<'_> {
    /// Run the measured two-second flush cadence or return an empty warmup result.
    ///
    /// # Errors
    /// Returns the first flush, non-admission query, schema, or terminal-outcome error.
    async fn flushes(
        &self,
        start: tokio::time::Instant,
        accepted_ordinals: Arc<std::sync::Mutex<Vec<Option<u64>>>>,
    ) -> Result<(Vec<u64>, u64, u64, u64), Box<dyn std::error::Error + Send + Sync>> {
        if !self.measured {
            return Ok((Vec::new(), 0, 0, 0));
        }
        let mut samples = Vec::with_capacity(10);
        let mut missed = 0_u64;
        let mut decoded_rows = 0_u64;
        let mut query_admissions = 0_u64;
        for flush_index in 0..(self.duration.as_secs() / 2) {
            let planned = start + Duration::from_secs((flush_index + 1) * 2);
            tokio::time::sleep_until(planned).await;
            if tokio::time::Instant::now() > planned + Duration::from_secs(2) {
                missed = missed.saturating_add(1);
                continue;
            }
            let tenant_index = flush_index as usize % self.tenants.len();
            let ordinal = accepted_ordinals
                .lock()
                .map_err(|_| "accepted write ledger poisoned")?[tenant_index];
            let Some(ordinal) = ordinal else {
                missed = missed.saturating_add(1);
                continue;
            };
            flush_one_tenant(self.cluster, self.tenants, tenant_index).await?;
            await_forge_convergence(self.cluster, &self.tenants[tenant_index..=tenant_index])
                .await?;
            let request = BifrostQueryRequest {
                sql: format!(
                    "SELECT row_id, wyrd_event_time FROM vala.bifrost.{} WHERE row_id BETWEEN {} AND {} ORDER BY row_id LIMIT 64",
                    self.table,
                    tenant_row_base(tenant_index) + PRELOAD_ROWS + ordinal * 64,
                    tenant_row_base(tenant_index) + PRELOAD_ROWS + ordinal * 64 + 63,
                ),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            };
            let mut stream = match self.clients[tenant_index].query.query(&request).await {
                Ok(stream) => stream,
                Err(error)
                    if is_backpressure(&error.to_string()) || is_retryable(&error.to_string()) =>
                {
                    query_admissions = query_admissions.saturating_add(1);
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let mut identities = BTreeSet::new();
            while let Some(batch) = stream.next_batch().await? {
                collect_query_identities(
                    &batch,
                    tenant_row_base(tenant_index) + PRELOAD_ROWS + ordinal * 64,
                    tenant_row_base(tenant_index) + PRELOAD_ROWS + ordinal * 64 + 63,
                    &mut identities,
                )?;
                decoded_rows = decoded_rows.saturating_add(batch.num_rows() as u64);
            }
            let terminal = stream
                .terminal()
                .ok_or("flush visibility query omitted terminal")?;
            if terminal.outcome != QueryTerminalOutcome::Success || identities.len() != 64 {
                return Err("flush visibility query did not complete".into());
            }
            samples.push(elapsed_us(planned));
        }
        Ok((samples, missed, decoded_rows, query_admissions))
    }
}

/// Create the isolated tenant roster for one fresh trial.
async fn provision_reference_tenants(
    cluster: &WyrdTestCluster,
    count: usize,
) -> Result<Vec<DataTenantId>, Box<dyn std::error::Error + Send + Sync>> {
    let mut tenants = vec![cluster.data_tenant_id()];
    for index in 1..count {
        tenants.push(cluster.add_tenant(&format!("reference-{index}")).await?);
    }
    Ok(tenants)
}

/// Create the exact user schema independently for every tenant.
///
/// Gate appends the server-managed `data_tenant_id` and `wyrd_event_time`
/// physical columns; declaring them here would violate the public catalog
/// contract.
async fn provision_reference_tables(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    provision_named_tables(cluster, tenants, &[REFERENCE_TABLE.to_owned()]).await
}

/// Create the exact user schema for every tenant/table binding at boot.
///
/// # Errors
/// Returns a missing-catalog or catalog create error. No caller invokes this
/// after warmup begins.
async fn provision_named_tables(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
    tables: &[String],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = cluster.server(0).ok_or("reference cluster has no Server")?;
    let catalog = server
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("reference cluster has no Redux catalog")?;
    let fields = vec![Field::new("row_id", DataType::Int64, false)];
    for tenant in tenants {
        for table in tables {
            catalog
                .create_table(CreateTableRequest {
                    table: TableRef::new(BifrostNamespace::Bifrost, table),
                    user_fields: fields.clone(),
                    tenant: *tenant,
                    audit: None,
                })
                .await?;
        }
    }
    Ok(())
}

/// Build normal tenant-bound public writer and query clients.
async fn reference_clients(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
) -> Result<Vec<ReferenceClient>, Box<dyn std::error::Error + Send + Sync>> {
    let mut clients = Vec::with_capacity(tenants.len());
    let public_nodes = cluster.ready_ingest_nodes();
    if public_nodes.is_empty() {
        return Err("reference cluster has no public Server nodes".into());
    }
    for (index, tenant) in tenants.iter().copied().enumerate() {
        let server = cluster
            .server_by_node(public_nodes[index % public_nodes.len()])
            .ok_or("reference Server is absent")?;
        let bootstrap = server
            .bootstrap_service_in_tenant(tenant, &format!("reference-{index}"), &["admin"])
            .await?;
        let key = match bootstrap {
            Bootstrap::Machine { api_key, .. } => api_key,
            Bootstrap::User { .. } => return Err("reference bootstrap returned a user".into()),
        };
        let client = WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: server.grpc_url().ok_or("Server has no gRPC endpoint")?,
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: server
                    .base_url()
                    .ok_or("Server has no HTTP endpoint")?
                    .to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(key),
            ..ClientConfig::default()
        })?;
        clients.push(ReferenceClient {
            writer: BifrostGrpcTransport::connect(&client).await?,
            query: QueryClient::new(&client),
        });
    }
    Ok(clients)
}

/// Preload exactly 8,192 rows per tenant through Gate and publish them.
async fn preload_reference_rows(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
    clients: &[ReferenceClient],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let writer_nodes = cluster.ready_ingest_nodes();
    if writer_nodes.is_empty() {
        return Err("reference cluster has no tenant writer Server".into());
    }
    for (tenant_index, tenant) in tenants.iter().copied().enumerate() {
        for batch in 0..(PRELOAD_ROWS / u64::from(ROWS_PER_WRITE)) {
            clients[tenant_index]
                .writer
                .send_frame(BifrostFrame {
                    table: format!("vala.bifrost.{REFERENCE_TABLE}"),
                    batch_id: deterministic_batch_id(tenant_index, batch),
                    arrow_ipc: reference_payload(
                        tenant_row_base(tenant_index) + batch * u64::from(ROWS_PER_WRITE),
                        ROWS_PER_WRITE,
                    )?
                    .into(),
                })
                .await?;
        }
        cluster
            .server_by_node(writer_nodes[tenant_index % writer_nodes.len()])
            .ok_or("preload Server is absent")?
            .flush_bifrost_for_tenant(tenant)
            .await?;
    }
    Ok(())
}

/// Flush each tenant exactly once through the Server that owns its writer.
///
/// # Errors
/// Returns an error when an owning Server is absent or its Scribe flush fails.
async fn flush_tenant_writers(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let writer_nodes = cluster.ready_ingest_nodes();
    if writer_nodes.is_empty() {
        return Err("reference cluster has no tenant writer Server".into());
    }
    for (tenant_index, tenant) in tenants.iter().copied().enumerate() {
        cluster
            .server_by_node(writer_nodes[tenant_index % writer_nodes.len()])
            .ok_or("reference cluster has no tenant writer Server")?
            .flush_bifrost_for_tenant(tenant)
            .await?;
    }
    Ok(())
}

/// Flush only the tenant selected for this staggered cadence slot.
async fn flush_one_tenant(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
    tenant_index: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let writer_nodes = cluster.ready_ingest_nodes();
    if writer_nodes.is_empty() {
        return Err("reference cluster has no tenant writer Server".into());
    }
    let tenant = tenants
        .get(tenant_index)
        .ok_or("staggered flush selected an unknown tenant")?;
    let node = writer_nodes
        .get(tenant_index % writer_nodes.len())
        .ok_or("reference cluster has no tenant writer Server")?;
    cluster
        .server_by_node(*node)
        .ok_or("reference cluster has no tenant writer Server")?
        .flush_bifrost_for_tenant(*tenant)
        .await?;
    Ok(())
}

/// Prove a tenant-bound public client cannot observe another tenant's row range.
///
/// The one-tenant scenario uses the next reserved tenant range; multi-tenant
/// scenarios use tenant one's populated range. The successful empty query must
/// still append exactly one tenant-bound Oracle audit decision.
///
/// # Errors
/// Returns an error for foreign rows, a non-success terminal, or an audit delta
/// other than one.
async fn prove_wrong_tenant_query(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
    clients: &[ReferenceClient],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let audit_before = audit_rows(cluster, &tenants[..1]).await?;
    let lower = tenant_row_base(1);
    let upper = lower + 63;
    let request = BifrostQueryRequest {
        sql: format!(
            "SELECT row_id, wyrd_event_time FROM vala.bifrost.{REFERENCE_TABLE} WHERE row_id BETWEEN {lower} AND {upper} ORDER BY row_id LIMIT 64"
        ),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(5_000),
    };
    let mut stream = ancillary_query(&clients[0].query, &request).await?;
    if stream.next_batch().await?.is_some()
        || stream
            .terminal()
            .is_none_or(|terminal| terminal.outcome != QueryTerminalOutcome::Success)
    {
        return Err("wrong-tenant query returned foreign rows or failed its terminal".into());
    }
    let audit_after = audit_rows(cluster, &tenants[..1]).await?;
    if audit_after.checked_sub(audit_before) != Some(1) {
        return Err("wrong-tenant query did not append exactly one Oracle audit decision".into());
    }
    Ok(())
}

/// Return aggregate tenant-bound Oracle read-audit cardinality.
async fn audit_rows(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let server = cluster.server(0).ok_or("reference cluster has no Server")?;
    let mut total = 0_u64;
    for tenant in tenants {
        let count = server
            .bifrost_read_decision_count_for_tenant(*tenant)
            .await?;
        total = total.saturating_add(u64::try_from(count)?);
    }
    Ok(total)
}

/// Wait for every measured Forge task to reach durable completion.
async fn await_forge_convergence(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let observer = cluster
        .forge_completion_observer()
        .ok_or("reference cluster has no Forge completion observer")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let writer_nodes = cluster.ready_ingest_nodes();
    if writer_nodes.is_empty() {
        return Err("reference cluster has no tenant writer Server".into());
    }
    loop {
        let mut pending = 0_i64;
        for (tenant_index, tenant) in tenants.iter().enumerate() {
            let writer = cluster
                .server_by_node(writer_nodes[tenant_index % writer_nodes.len()])
                .ok_or("reference cluster has no tenant writer Server")?;
            pending = pending.saturating_add(
                writer
                    .bifrost_pending_forge_tasks_for_tenant(*tenant)
                    .await?,
            );
        }
        if pending == 0 {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!(
                "Forge publication did not converge before the 30-second deadline; {pending} tasks remain"
            )
            .into());
        }
        let expected = observer.completed().saturating_add(1);
        let _ = tokio::time::timeout(
            remaining.min(Duration::from_secs(1)),
            observer.wait_for_at_least(expected),
        )
        .await;
    }
}

/// Query every tenant after convergence and prove exact published unique rows.
async fn final_published_rows(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
    clients: &[ReferenceClient],
    table: &str,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    Ok(final_published_identities(cluster, tenants, clients, table)
        .await?
        .iter()
        .map(|identities| identities.len() as u64)
        .sum())
}

/// Query every tenant through the public Oracle and retain its exact row identities.
///
/// # Errors
/// Returns an error when a query fails, exposes a foreign tenant identity, or
/// omits its successful terminal.
async fn final_published_identities(
    _cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
    clients: &[ReferenceClient],
    table: &str,
) -> Result<Vec<BTreeSet<u64>>, Box<dyn std::error::Error + Send + Sync>> {
    let mut published = Vec::with_capacity(tenants.len());
    for (tenant_index, client) in clients.iter().enumerate().take(tenants.len()) {
        let request = BifrostQueryRequest {
            sql: format!(
                "SELECT row_id, wyrd_event_time FROM vala.bifrost.{table} ORDER BY row_id"
            ),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(5_000),
        };
        let mut stream = ancillary_query(&client.query, &request).await?;
        let mut identities = BTreeSet::new();
        let lower = tenant_row_base(tenant_index);
        let upper = lower + TENANT_ROW_STRIDE - 1;
        while let Some(batch) = stream.next_batch().await? {
            collect_query_identities(&batch, lower, upper, &mut identities)?;
        }
        if stream
            .terminal()
            .is_none_or(|terminal| terminal.outcome != QueryTerminalOutcome::Success)
        {
            return Err("final reference query omitted a successful terminal".into());
        }
        published.push(identities);
    }
    Ok(published)
}

/// Reconcile measured client ledgers with production metric and audit evidence.
fn reconcile_production(
    telemetry: &crate::bifrost::telemetry::ForgeTelemetryDelta,
    client: &WindowResult,
    observed_audit_rows: u64,
    published_rows: u64,
) -> Result<ProductionTelemetryEvidence, Box<dyn std::error::Error + Send + Sync>> {
    let total = |family: &str| {
        telemetry
            .metrics
            .iter()
            .filter(|sample| sample.family == family)
            .map(|sample| sample.value.max(0.0))
            .sum::<f64>() as u64
    };
    let labeled_total = |family: &str, labels: &[(&str, &str)]| {
        telemetry
            .metrics
            .iter()
            .filter(|sample| {
                sample.family == family
                    && labels.iter().all(|(key, value)| {
                        sample.labels.get(*key).is_some_and(|label| label == value)
                    })
            })
            .map(|sample| sample.value.max(0.0))
            .sum::<f64>() as u64
    };
    let spans_error = telemetry.spans.iter().any(|span| {
        span.attributes
            .get("outcome")
            .is_some_and(|value| matches!(value.as_str(), "failed" | "error"))
            || span.attributes.contains_key("error.type")
            || span.attributes.contains_key("error.message")
    });
    let rows = client.tenant_write_rows.iter().sum::<u64>();
    let completed_queries = client.telemetry_completed_queries;
    if published_rows != rows {
        return Err(format!(
            "measured published row delta {published_rows} does not equal acknowledged measured rows {rows}"
        )
        .into());
    }
    let evidence = ProductionTelemetryEvidence {
        gate_accepted_rows: total("bifrost_gate_rows_total"),
        scribe_accepted_rows: total("bifrost_scribe_rows_total"),
        client_acknowledged_rows: rows,
        scribe_persisted_rows: labeled_total(
            "bifrost_scribe_seal_rows_total",
            &[("stage", "file_list_transaction")],
        ),
        forge_published_rows: published_rows,
        oracle_stream_rows: total("bifrost_oracle_stream_rows_total"),
        client_decoded_rows: client.telemetry_decoded_rows,
        oracle_terminal_outcomes: labeled_total(
            "bifrost_gate_requests_total",
            &[("operation", "query"), ("outcome", "success")],
        ),
        client_completed_queries: completed_queries,
        expected_audit_rows: completed_queries,
        observed_audit_rows,
        required_telemetry: if [
            "bifrost_gate_requests_total",
            "bifrost_gate_request_duration_seconds",
            "bifrost_gate_active_streams",
            "bifrost_scribe_rows_total",
            "bifrost_scribe_seal_rows_total",
            "bifrost_oracle_stream_rows_total",
        ]
        .iter()
        .all(|family| {
            telemetry
                .metrics
                .iter()
                .chain(telemetry.gauge_final.iter())
                .any(|sample| sample.family == *family)
        }) {
            EvidenceStatus::Complete
        } else {
            EvidenceStatus::Failed
        },
        counter_integrity: EvidenceStatus::Complete,
        spans_clean: if spans_error {
            EvidenceStatus::Failed
        } else {
            EvidenceStatus::Complete
        },
        cleanup: EvidenceStatus::Failed,
        correctness: EvidenceStatus::Complete,
    };
    Ok(evidence)
}

/// Encode one exact controlled 64-row Arrow request.
fn reference_payload(
    first_row: u64,
    rows: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "row_id",
        DataType::Int64,
        false,
    )]));
    let row_ids = (0..rows)
        .map(|offset| (first_row + u64::from(offset)) as i64)
        .collect::<Vec<_>>();
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Int64Array::from(row_ids))],
    )?;
    let mut payload = Vec::new();
    let mut writer = StreamWriter::try_new(&mut payload, schema.as_ref())?;
    writer.write(&batch)?;
    writer.finish()?;
    drop(writer);
    if payload.len() > 64 * 1024 {
        return Err("controlled write payload exceeds 64 KiB".into());
    }
    Ok(payload)
}

/// Return the start of one tenant's deterministic disjoint row-id range.
#[must_use]
fn tenant_row_base(tenant_index: usize) -> u64 {
    (tenant_index as u64).saturating_mul(TENANT_ROW_STRIDE)
}

/// Build a stable UUIDv7-shaped batch identity from seed, tenant, and ordinal.
fn deterministic_batch_id(tenant_index: usize, ordinal: u64) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&(0xB1_F057_u64 ^ ordinal).to_be_bytes());
    bytes[8..].copy_from_slice(&(tenant_index as u64 ^ ordinal.rotate_left(17)).to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

/// Convert a bounded latency vector to one HDR-derived report distribution.
fn distribution(values: &[u64]) -> TrialDistribution {
    let mut histogram = Histogram::<u64>::new(3).expect("invariant: HDR precision is valid");
    let mut overflowed = false;
    for value in values {
        if histogram.record((*value).max(1)).is_err() {
            overflowed = true;
        }
    }
    TrialDistribution {
        samples: values.len() as u64,
        p50_us: histogram.value_at_quantile(0.50),
        p95_us: histogram.value_at_quantile(0.95),
        p99_us: histogram.value_at_quantile(0.99),
        max_us: histogram.max(),
        overflowed,
    }
}

/// Compute one bounded ratio without NaN.
fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

/// Return elapsed microseconds from the immutable planned dispatch instant.
fn elapsed_us(planned: tokio::time::Instant) -> u64 {
    planned.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

/// Classify only explicit stable admission/backpressure responses.
fn is_backpressure(error: &str) -> bool {
    error.contains("backpressure")
        || error.contains("busy")
        || error.contains("admission rejected")
        || error.contains("WYRD_VALA_507_WAL_DISK_FULL")
}

/// Classify only bounded transient publication/admission responses.
fn is_retryable(error: &str) -> bool {
    [
        "temporarily unavailable",
        "not published",
        "freshness",
        "admission rejected",
    ]
    .iter()
    .any(|value| error.contains(value))
}

/// Run the shortened real public-cluster smoke without claiming wall-clock SLOs.
///
/// # Errors
/// Returns a cluster error when the real public Gate adapter fails its
/// correctness, telemetry, tenant-isolation, audit, or cleanup assertions.
pub async fn run_smoke() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let definition = reference_scenario_matrix()[2];
    let path = report_path("cluster-smoke.json");
    let environment = detect_reference_environment_or_write(&path)?;
    let mut session =
        CapacityScenarioSession::start(definition, PathBuf::from(&environment.storage_root))
            .await?;
    session.stages = CapacityStateMachine::for_rates(wyrd_bench::SMOKE_CAPACITY_RATES.to_vec());
    let mut attempted_probes = Vec::new();
    let result = async {
        let mut stages = Vec::new();
        while let Some(plan) = session.next_stage() {
            begin_probe(
                &mut attempted_probes,
                definition.id.to_owned(),
                plan.offered_requests_per_second,
            );
            let run = tokio::time::timeout(
                Duration::from_secs(20),
                session.run_stage(plan, Duration::from_secs(10)),
            )
            .await
            .map_err(|_| "smoke stage exceeded 20 seconds")??;
            let stage = assemble_capacity_stage(definition, plan, run, 10);
            session.record_stage(plan, stage.passed)?;
            complete_current_probe(&mut attempted_probes, &stage.stop_reasons);
            stages.push(stage);
        }
        session.reconcile_cumulative().await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(stages)
    }
    .await;
    let shutdown = session.shutdown().await;
    let stages = match result {
        Ok(stages) => stages,
        Err(error) => {
            let cleanup_error = shutdown.err().map(|cleanup| cleanup.to_string());
            let combined = cleanup_error.as_ref().map_or_else(
                || error.to_string(),
                |cleanup| format!("{error}; cleanup failed: {cleanup}"),
            );
            write_partial_capture(&path, &environment, &combined, attempted_probes, Vec::new())?;
            return Err(error);
        }
    };
    shutdown?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({
                "evidence_class": "shortened-non-slo-smoke",
                "schema_version": CLUSTER_REPORT_VERSION,
                "scenario_id": definition.id,
                "stages": stages,
            }))?
        ),
    )?;
    Ok(path)
}

/// Detect the reference environment and persist the existing unsupported diagnostic on failure.
///
/// # Errors
/// Returns the original environment error after the diagnostic is written, or the diagnostic
/// write error when the artifact cannot be persisted.
fn detect_reference_environment_or_write(
    report_path: &Path,
) -> Result<BenchmarkEnvironment, Box<dyn std::error::Error + Send + Sync>> {
    match detect_reference_environment() {
        Ok(environment) => Ok(environment),
        Err(error) => {
            write_unavailable_capture(report_path, &error.to_string())?;
            Err(Box::new(error))
        }
    }
}

/// Detect the complete controlled environment from host and wrapper evidence.
///
/// # Errors
/// Returns `Unsupported` when CPU, memory, toolchain, lock, container, database,
/// storage, Git, OS, or timestamp identity is absent or unclassifiable.
pub fn detect_reference_environment() -> Result<BenchmarkEnvironment, ClusterBenchmarkError> {
    validate_open_file_limit(&command_output("sh", &["-c", "ulimit -n"])?)?;
    let os = std::env::consts::OS.to_owned();
    let (cpu_vendor, cpu_model) = match os.as_str() {
        "linux" => {
            let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").map_err(|error| {
                ClusterBenchmarkError::Unsupported(format!("cannot read /proc/cpuinfo: {error}"))
            })?;
            extract_linux_cpu_identity(&cpuinfo)?
        }
        "macos" => extract_macos_cpu_identity(&command_output(
            "sysctl",
            &["-n", "machdep.cpu.brand_string"],
        )?)?,
        other => {
            return Err(ClusterBenchmarkError::Unsupported(format!(
                "unsupported reference OS `{other}`"
            )));
        }
    };
    let logical_cores = std::thread::available_parallelism()
        .map_err(|error| ClusterBenchmarkError::Unsupported(error.to_string()))?
        .get()
        .try_into()
        .map_err(|_| ClusterBenchmarkError::Unsupported("logical core overflow".to_owned()))?;
    let host_memory_bytes = match os.as_str() {
        "macos" => command_output("sysctl", &["-n", "hw.memsize"])?,
        "linux" => {
            let meminfo = std::fs::read_to_string("/proc/meminfo").map_err(|error| {
                ClusterBenchmarkError::Unsupported(format!("cannot read /proc/meminfo: {error}"))
            })?;
            let kib = meminfo
                .lines()
                .find_map(|line| line.strip_prefix("MemTotal:"))
                .and_then(|value| value.split_ascii_whitespace().next())
                .ok_or_else(|| {
                    ClusterBenchmarkError::Unsupported("Linux MemTotal is absent".to_owned())
                })?;
            (kib.parse::<u64>().map_err(|error| {
                ClusterBenchmarkError::Unsupported(format!("invalid Linux MemTotal: {error}"))
            })? * 1024)
                .to_string()
        }
        _ => unreachable!("OS was validated above"),
    }
    .parse::<u64>()
    .map_err(|error| ClusterBenchmarkError::Unsupported(format!("invalid host memory: {error}")))?;
    let container_cpu_nanos = required_environment_u64("WYRD_BENCH_CONTAINER_CPU_NANOS")?;
    let container_cpu_millis = (container_cpu_nanos / 1_000_000)
        .try_into()
        .map_err(|_| ClusterBenchmarkError::Unsupported("container CPU overflow".to_owned()))?;
    let container_memory_bytes = required_environment_u64("WYRD_BENCH_CONTAINER_MEMORY_BYTES")?;
    let rust = command_output("rustc", &["--version"])?;
    let rust_major_minor = rust
        .split_ascii_whitespace()
        .nth(1)
        .and_then(|version| version.rsplit_once('.').map(|(major_minor, _)| major_minor))
        .ok_or_else(|| {
            ClusterBenchmarkError::Unsupported("rustc version is unclassifiable".to_owned())
        })?
        .to_owned();
    let lock = include_str!("../../../../../Cargo.lock");
    let storage_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("target/bifrost-benchmarks/storage");
    std::fs::create_dir_all(&storage_root).map_err(|error| {
        ClusterBenchmarkError::Unsupported(format!(
            "cannot create dedicated benchmark storage root: {error}"
        ))
    })?;
    let storage_root = storage_root.canonicalize().map_err(|error| {
        ClusterBenchmarkError::Unsupported(format!(
            "cannot inspect dedicated benchmark storage root: {error}"
        ))
    })?;
    let storage_device_class = if os == "macos" {
        command_output(
            "stat",
            &["-f", "%T", storage_root.to_string_lossy().as_ref()],
        )?
    } else {
        command_output(
            "stat",
            &["-f", "-c", "%T", storage_root.to_string_lossy().as_ref()],
        )?
    };
    let storage_root = storage_root.to_string_lossy().into_owned();
    let environment = BenchmarkEnvironment {
        architecture: std::env::consts::ARCH.to_owned(),
        cpu_vendor,
        cpu_model,
        logical_cores,
        host_memory_bytes,
        container_cpu_millis,
        container_memory_bytes,
        postgres_image: "postgres:16".to_owned(),
        postgres_version: postgres_version()?,
        postgres_config: "wyrd-reference-v1:tmpfs:2cpu:4gib".to_owned(),
        storage_image: "local-filesystem".to_owned(),
        storage_version: env!("CARGO_PKG_VERSION").to_owned(),
        storage_config: format!("local-filesystem:{storage_root}:dedicated-root"),
        storage_backend_kind: "local-filesystem".to_owned(),
        storage_root,
        storage_device_class,
        rust_major_minor,
        arrow_version: lock_version(lock, "arrow")?,
        datafusion_version: lock_version(lock, "datafusion")?,
        iceberg_version: lock_source_revision(lock, "iceberg")?,
        os,
        kernel: command_output("uname", &["-srv"])?,
        git_sha: command_output("git", &["rev-parse", "HEAD"])?,
        dirty_worktree: !command_output("git", &["status", "--porcelain"])?.is_empty(),
        captured_at: chrono::Utc::now().to_rfc3339(),
    };
    environment.validate()?;
    Ok(environment)
}

/// Parse and validate the process soft open-file limit before cluster construction.
///
/// # Errors
/// Returns `Unsupported` when the value is unreadable or below the benchmark minimum.
fn validate_open_file_limit(rendered: &str) -> Result<u64, ClusterBenchmarkError> {
    let limit = rendered.parse::<u64>().map_err(|error| {
        ClusterBenchmarkError::Unsupported(format!("cannot parse process open-file limit: {error}"))
    })?;
    if limit < MINIMUM_OPEN_FILE_LIMIT {
        return Err(ClusterBenchmarkError::Unsupported(format!(
            "process open-file limit {limit} is below benchmark minimum {MINIMUM_OPEN_FILE_LIMIT}"
        )));
    }
    Ok(limit)
}

/// Resolve a default or caller-selected target report path.
fn report_path(file_name: &str) -> PathBuf {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    std::env::var_os("WYRD_BIFROST_REPORT").map_or_else(
        || repository.join("target/bifrost-benchmarks").join(file_name),
        |configured| {
            let configured = PathBuf::from(configured);
            if configured.is_absolute() {
                configured
            } else {
                repository.join(configured)
            }
        },
    )
}

/// Execute one local command and return trimmed UTF-8 output.
fn command_output(program: &str, arguments: &[&str]) -> Result<String, ClusterBenchmarkError> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| {
            ClusterBenchmarkError::Unsupported(format!("cannot execute {program}: {error}"))
        })?;
    if !output.status.success() {
        return Err(ClusterBenchmarkError::Unsupported(format!(
            "{program} exited with {}",
            output.status
        )));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| ClusterBenchmarkError::Unsupported(format!("invalid UTF-8: {error}")))
}

/// Read one mandatory positive integer from the reference wrapper.
fn required_environment_u64(name: &str) -> Result<u64, ClusterBenchmarkError> {
    let value = std::env::var(name).map_err(|_| {
        ClusterBenchmarkError::Unsupported(format!("required environment {name} is absent"))
    })?;
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            ClusterBenchmarkError::Unsupported(format!("required environment {name} is invalid"))
        })
}

/// Query the exact server version through the wrapper-owned benchmark DSN.
fn postgres_version() -> Result<String, ClusterBenchmarkError> {
    let dsn = std::env::var("WYRD_BENCH_PG_URL").map_err(|_| {
        ClusterBenchmarkError::Unsupported("WYRD_BENCH_PG_URL is absent".to_owned())
    })?;
    command_output("psql", &[&dsn, "-Atqc", "SHOW server_version"])
}

/// Extract one exact package version from the locked dependency graph.
fn lock_version(lock: &str, package: &str) -> Result<String, ClusterBenchmarkError> {
    lock.split("[[package]]")
        .find(|entry| {
            entry
                .lines()
                .any(|line| line == format!("name = \"{package}\""))
        })
        .and_then(|entry| {
            entry.lines().find_map(|line| {
                line.strip_prefix("version = \"")
                    .and_then(|value| value.strip_suffix('"'))
            })
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            ClusterBenchmarkError::Unsupported(format!("Cargo.lock has no {package} version"))
        })
}

/// Extract one exact Git source revision from the locked dependency graph.
fn lock_source_revision(lock: &str, package: &str) -> Result<String, ClusterBenchmarkError> {
    lock.split("[[package]]")
        .find(|entry| {
            entry
                .lines()
                .any(|line| line == format!("name = \"{package}\""))
        })
        .and_then(|entry| {
            entry.lines().find_map(|line| {
                line.strip_prefix("source = \"")
                    .and_then(|value| value.strip_suffix('"'))
                    .and_then(|value| value.rsplit_once('#').map(|(_, revision)| revision))
            })
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            ClusterBenchmarkError::Unsupported(format!(
                "Cargo.lock has no Git revision for {package}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a complete environment identity for diagnostic-assembly unit tests.
    fn diagnostic_environment_fixture() -> BenchmarkEnvironment {
        BenchmarkEnvironment {
            architecture: "aarch64".to_owned(),
            cpu_vendor: "apple".to_owned(),
            cpu_model: "Apple M4".to_owned(),
            logical_cores: 10,
            host_memory_bytes: 16_000_000_000,
            container_cpu_millis: 4_000,
            container_memory_bytes: 4_000_000_000,
            postgres_image: "postgres:16".to_owned(),
            postgres_version: "16".to_owned(),
            postgres_config: "reference".to_owned(),
            storage_image: "local-filesystem".to_owned(),
            storage_version: "1".to_owned(),
            storage_config: "reference".to_owned(),
            storage_backend_kind: "local-filesystem".to_owned(),
            storage_root: "target/bifrost-benchmarks/storage".to_owned(),
            storage_device_class: "apfs".to_owned(),
            rust_major_minor: "1.90".to_owned(),
            arrow_version: "1".to_owned(),
            datafusion_version: "1".to_owned(),
            iceberg_version: "1".to_owned(),
            os: "macos".to_owned(),
            kernel: "test".to_owned(),
            git_sha: "deadbeef".to_owned(),
            dirty_worktree: true,
            captured_at: "2026-08-04T00:00:00Z".to_owned(),
        }
    }

    /// Proves the public grammar requires one mode and one selector before
    /// any cluster or database lifecycle starts.
    #[test]
    fn public_command_grammar_is_exact() {
        let command = parse_cluster_benchmark_args([
            "--mode",
            "capacity",
            "--scenario",
            "balanced-one-pod-eight-tenants",
        ])
        .expect("canonical command parses");
        assert_eq!(command.mode, ClusterBenchmarkMode::Capacity);
        assert_eq!(
            command.scenario.as_deref(),
            Some("balanced-one-pod-eight-tenants")
        );
        assert!(parse_cluster_benchmark_args(["--mode", "capacity"]).is_err());
        assert!(
            parse_cluster_benchmark_args([
                "--mode",
                "capacity",
                "--matrix",
                "--scenario",
                "balanced-one-pod-one-tenant"
            ])
            .is_err()
        );
        assert!(
            parse_cluster_benchmark_args([
                "--mode",
                "qualification",
                "--scenario",
                "not-a-scenario"
            ])
            .is_err()
        );
        assert!(
            parse_cluster_benchmark_args(["--mode", "qualification", "--matrix", "--unknown"])
                .is_err()
        );
    }

    /// Proves capacity boot owns one shared warmup table and thirteen unique
    /// measurement slots without runtime provisioning.
    #[test]
    fn capacity_allocation_is_exact_and_deterministic() {
        let tables = capacity_table_names();
        assert_eq!(
            tables.len(),
            usize::try_from(CAPACITY_TABLES_PER_TENANT).unwrap()
        );
        assert_eq!(tables[0], CAPACITY_WARMUP_TABLE);
        assert_eq!(tables[1], "cluster_capacity_stage_00");
        assert_eq!(tables[13], "cluster_capacity_stage_12");
        assert_eq!(tables.iter().collect::<BTreeSet<_>>().len(), tables.len());
    }

    /// Proves report metrics retain a non-default profile concurrency cap.
    #[test]
    fn window_metrics_retain_profile_in_flight_cap() {
        let result = WindowResult {
            max_in_flight: 17,
            ..WindowResult::default()
        };
        assert_eq!(result.metrics().max_in_flight, 17);
    }

    /// Proves final reconciliation rejects corruption in an earlier completed stage.
    #[test]
    fn cumulative_identity_ledger_rejects_earlier_stage_corruption() {
        let expected = vec![BTreeSet::from([10, 11, 20, 21])];
        let exact = vec![BTreeSet::from([10, 11, 20, 21])];
        let corrupted = vec![BTreeSet::from([10, 12, 20, 21])];
        assert!(exact_identity_ledger_matches(&expected, &exact));
        assert!(!exact_identity_ledger_matches(&expected, &corrupted));
    }

    /// Proves trace manifests cannot reuse one span across multiple operations.
    #[test]
    fn benchmark_span_classification_is_operation_specific() {
        assert_eq!(
            benchmark_span_operation("scribe_durable_write"),
            Some(BenchmarkOperation::DurableWrite)
        );
        assert_eq!(
            benchmark_span_operation("forge_publish"),
            Some(BenchmarkOperation::FlushToVisible)
        );
        assert_eq!(
            benchmark_span_operation("oracle_first_frame"),
            Some(BenchmarkOperation::QueryTimeToFirstFrame)
        );
        assert_eq!(
            benchmark_span_operation("oracle_query_total"),
            Some(BenchmarkOperation::QueryTotal)
        );
        assert_eq!(benchmark_span_operation("unrelated"), None);
    }

    /// Proves serialized row and batch seeds describe the actual first frame.
    #[test]
    fn stage_report_identity_matches_written_frame_identity() {
        let ordinals = BTreeSet::from([2_300_000, 2_300_002]);
        let identity = stage_row_identity(2, &ordinals);
        assert_eq!(identity.batch_id_seed, 2_300_000);
        assert_eq!(
            deterministic_batch_id(2, identity.batch_id_seed),
            deterministic_batch_id(2, 2_300_000)
        );
        assert_eq!(
            identity.row_id_start,
            tenant_row_base(2) + PRELOAD_ROWS + 2_300_000 * 64
        );
        assert_eq!(identity.batch_ordinals, vec![2_300_000, 2_300_002]);
        assert!(
            !identity
                .row_ids
                .contains(&(tenant_row_base(2) + PRELOAD_ROWS + 2_300_001 * 64))
        );
    }

    /// Proves a rejected early write leaves a serialized gap before a later success.
    #[test]
    fn accepted_write_ledger_preserves_rejected_ordinal_gap() {
        let mut result = WindowResult {
            tenant_write_rows: vec![0],
            tenant_write_ordinals: vec![BTreeSet::new()],
            ..Default::default()
        };
        apply_operation(&mut result, OperationResult::Backpressure);
        apply_operation(
            &mut result,
            OperationResult::Write {
                tenant: 0,
                latency_us: 1,
                rows: 64,
                retries: 0,
                ordinal: 42,
            },
        );
        let identity = stage_row_identity(0, &result.tenant_write_ordinals[0]);
        assert_eq!(identity.batch_ordinals, vec![42]);
        assert_eq!(
            serde_json::to_value(identity).unwrap()["batch_ordinals"][0],
            42
        );
    }

    /// Proves mixed traffic reads only the converged preload and writes its phase table.
    #[test]
    fn mixed_workload_routes_reads_to_visible_preload() {
        assert_eq!(operation_table("phase-7", false), REFERENCE_TABLE);
        assert_eq!(operation_table("phase-7", true), "phase-7");
    }

    /// Proves concurrent visibility follows the greatest accepted write ordinal.
    #[test]
    fn flush_visibility_uses_latest_accepted_ordinal() {
        let ledger = std::sync::Mutex::new(vec![None]);
        for ordinal in [9, 4, 12] {
            record_latest_accepted_ordinal(
                &ledger,
                &OperationResult::Write {
                    tenant: 0,
                    latency_us: 1,
                    rows: 64,
                    retries: 0,
                    ordinal,
                },
            )
            .unwrap();
        }
        assert_eq!(ledger.lock().unwrap()[0], Some(12));
    }

    /// Proves the runtime open-file parser accepts the minimum and fails closed otherwise.
    #[test]
    fn runtime_open_file_limit_fails_closed_before_boot() {
        assert_eq!(validate_open_file_limit("8192").unwrap(), 8_192);
        assert!(matches!(
            validate_open_file_limit("256"),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
        assert!(matches!(
            validate_open_file_limit("unlimited"),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
    }

    /// Proves a failed shell adjustment reaches the existing unsupported artifact shape.
    #[test]
    fn failed_limit_raise_reaches_unsupported_capture() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("unsupported.json");
        let error = validate_open_file_limit("256").unwrap_err();
        write_unavailable_capture(&path, &error.to_string()).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["status"], "unsupported");
        assert_eq!(value["promotable"], false);
        assert!(value["error"].as_str().unwrap().contains("256"));
    }

    /// Proves ordinary IO diagnostics retain an incomplete probe without capacity stages.
    #[test]
    fn infrastructure_error_retains_incomplete_attempt_without_capacity_classification() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("partial.json");
        let environment = diagnostic_environment_fixture();
        let mut probes = Vec::new();
        begin_probe(&mut probes, "scenario-a".to_owned(), 500);
        write_partial_capture(
            &path,
            &environment,
            "ordinary IO failure",
            probes,
            Vec::new(),
        )
        .unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["attempted_probes"][0]["completed"], false);
        assert!(
            value["error"]
                .as_str()
                .unwrap()
                .contains("ordinary IO failure")
        );
        assert_eq!(value["scenarios"].as_array().unwrap().len(), 0);
        assert_eq!(
            value["attempted_probes"][0]["stop_reasons"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    /// Build one completed production-shaped stage input for classification tests.
    fn completed_stage_fixture(backpressure: u64, complete_evidence: bool) -> CapacityStageRun {
        let metric = |family: &str| crate::bifrost::telemetry::ForgeMetricSample {
            family: family.to_owned(),
            labels: BTreeMap::new(),
            value: 1.0,
        };
        let metrics = [
            "bifrost_gate_requests_total",
            "bifrost_scribe_rows_total",
            "bifrost_oracle_stream_rows_total",
            "postgres_pool_wait_total",
            "storage_bytes_total",
            "wal_fsync_total",
        ]
        .into_iter()
        .map(metric)
        .collect();
        let spans = complete_evidence
            .then(|| {
                [
                    "scribe_durable_write",
                    "forge_publish",
                    "oracle_first_frame",
                    "oracle_query_total",
                ]
                .into_iter()
                .map(|name| wyrd_telemetry::CapturedSpan {
                    trace_id: format!("trace-{name}"),
                    name: name.to_owned(),
                    attributes: BTreeMap::new(),
                    duration_nanos: 1_000,
                })
                .collect()
            })
            .unwrap_or_default();
        CapacityStageRun {
            client: WindowResult {
                max_in_flight: DEFAULT_MAX_IN_FLIGHT,
                write_us: vec![1],
                flush_us: vec![1],
                query_ttfb_us: vec![1],
                query_total_us: vec![1],
                tenant_write_rows: vec![64],
                tenant_write_ordinals: vec![BTreeSet::from([0])],
                tenant_queries: vec![1],
                decoded_rows: 64,
                telemetry_decoded_rows: 128,
                telemetry_completed_queries: 2,
                submitted: 2,
                accepted: 2,
                backpressure,
                measured_seconds: 10,
                ..WindowResult::default()
            },
            telemetry: crate::bifrost::telemetry::ForgeTelemetryDelta {
                metrics,
                gauge_maxima: Vec::new(),
                gauge_final: Vec::new(),
                spans,
                interval_seconds: 10.0,
            },
            published_rows: 64,
            audit_rows: 1,
        }
    }

    /// Proves smoke classifies completed production outcomes through the capacity helper.
    #[tokio::test]
    async fn smoke_completed_stage_classification_confirms_and_recovers() {
        let definition = reference_scenario_matrix()[0];
        let mut machine = CapacityStateMachine::for_rates(vec![100, 200]);
        let healthy = machine.next_plan().unwrap();
        let healthy_stage =
            assemble_capacity_stage(definition, healthy, completed_stage_fixture(0, true), 10);
        assert!(healthy_stage.passed);
        machine.record(healthy, healthy_stage.passed).unwrap();

        let failed = machine.next_plan().unwrap();
        let mut admission = completed_stage_fixture(0, true);
        admission.client.merge_flush_result((Vec::new(), 0, 0, 1));
        assert_eq!(admission.client.backpressure, 1);
        let backpressured = assemble_capacity_stage(definition, failed, admission, 10);
        assert!(!backpressured.passed);
        assert!(
            backpressured
                .stop_reasons
                .contains(&CapacityLimit::Backpressure)
        );
        machine.record(failed, backpressured.passed).unwrap();

        let confirmation = machine.next_plan().unwrap();
        assert_eq!(
            confirmation.kind,
            wyrd_bench::CapacityStageKind::Confirmation
        );
        let incomplete_evidence = assemble_capacity_stage(
            definition,
            confirmation,
            completed_stage_fixture(0, false),
            10,
        );
        assert!(!incomplete_evidence.passed);
        assert!(
            incomplete_evidence
                .stop_reasons
                .contains(&CapacityLimit::DependencySlo)
        );
        machine
            .record(confirmation, incomplete_evidence.passed)
            .unwrap();

        let recovery = machine.next_plan().unwrap();
        assert_eq!(recovery.kind, wyrd_bench::CapacityStageKind::Recovery);
        assert_eq!(recovery.offered_requests_per_second, 100);

        let mut missed_flush = completed_stage_fixture(0, true);
        missed_flush.client.missed_flushes = 1;
        let missed_flush_stage = assemble_capacity_stage(
            definition,
            CapacityStagePlan {
                slot: 0,
                offered_requests_per_second: 100,
                kind: wyrd_bench::CapacityStageKind::Discovery,
            },
            missed_flush,
            10,
        );
        assert!(!missed_flush_stage.passed);
    }

    /// Proves both logical topologies are truthfully attributed to the single harness process.
    #[tokio::test]
    async fn in_process_resource_topologies_have_exact_roles() {
        let telemetry = crate::bifrost::telemetry::ForgeTelemetryDelta {
            metrics: Vec::new(),
            gauge_maxima: Vec::new(),
            gauge_final: Vec::new(),
            spans: Vec::new(),
            interval_seconds: 1.0,
        };
        for definition in [
            reference_scenario_matrix()[0],
            reference_scenario_matrix()[2],
        ] {
            let resources = capacity_resource_evidence(definition, &telemetry);
            assert_eq!(resources.len(), 1);
            assert_eq!(
                resources[0].roles,
                vec![
                    BifrostRuntimeRole::Gate,
                    BifrostRuntimeRole::Scribe,
                    BifrostRuntimeRole::Forge,
                    BifrostRuntimeRole::Oracle
                ]
            );
            assert!(resources[0].peak_rss_bytes > 0);
        }
    }

    /// Proves primary and cleanup failures are retained together.
    #[test]
    fn dual_failure_text_preserves_both_causes() {
        let primary = "measurement timed out";
        let cleanup = "cluster shutdown timed out";
        let combined = format!("{primary}; cleanup failed: {cleanup}");
        assert!(combined.contains(primary));
        assert!(combined.contains(cleanup));
    }

    /// Proves a first-stage timeout retains the attempted incomplete probe.
    #[test]
    fn first_stage_timeout_progress_is_retained() {
        let mut probes = Vec::new();
        begin_probe(&mut probes, "scenario-a".to_owned(), 8);
        assert_eq!(probes.len(), 1);
        assert!(!probes[0].completed);
    }

    /// Proves a mid-curve failure retains completed predecessors and current work.
    #[test]
    fn mid_curve_progress_is_retained() {
        let mut probes = Vec::new();
        begin_probe(&mut probes, "scenario-a".to_owned(), 8);
        complete_current_probe(&mut probes, &[]);
        begin_probe(&mut probes, "scenario-a".to_owned(), 16);
        assert!(probes[0].completed);
        assert!(!probes[1].completed);
    }

    /// Proves qualification progress distinguishes trials and retains the failing trial.
    #[test]
    fn mid_qualification_progress_is_retained() {
        let mut probes = Vec::new();
        begin_probe(&mut probes, "scenario-a:trial-1".to_owned(), 32);
        complete_current_probe(&mut probes, &[]);
        begin_probe(&mut probes, "scenario-a:trial-2".to_owned(), 32);
        assert_eq!(probes[0].scenario_id.as_deref(), Some("scenario-a:trial-1"));
        assert_eq!(probes[1].scenario_id.as_deref(), Some("scenario-a:trial-2"));
        assert!(!probes[1].completed);
    }

    /// Proves a partial rate report serializes trial-one client and production evidence.
    #[test]
    fn partial_qualification_report_serializes_completed_trial() {
        let definition = reference_scenario_matrix()[0];
        let trial = ClusterBenchmarkTrial {
            trial: 1,
            client: ClientTrialMetrics {
                accepted_operations: 7,
                ..Default::default()
            },
            production: ProductionTelemetryEvidence {
                gate_accepted_rows: 7,
                ..Default::default()
            },
        };
        let report =
            qualification_scenario_report(definition, 32, vec![trial], Vec::new(), false).unwrap();
        let encoded = serde_json::to_value(&report).unwrap();
        assert_eq!(encoded["trials"][0]["client"]["accepted_operations"], 7);
        assert_eq!(encoded["trials"][0]["production"]["gate_accepted_rows"], 7);
    }

    /// Drives the representative distributed shortened curve through one live
    /// public-Gate capacity session without making an SLO claim.
    #[tokio::test]
    #[ignore = "requires repository-managed Postgres and benchmark environment identity"]
    async fn shortened_capacity_smoke_is_reachable() {
        let path = run_smoke().await.expect("shortened smoke completes");
        assert!(path.ends_with("cluster-smoke.json"));
    }

    /// Proves the public benchmark owns exactly D22's six scenario identities.
    #[test]
    fn reference_matrix_is_exact() {
        let scenarios = reference_scenario_matrix();
        assert_eq!(scenarios.len(), 6);
        assert_eq!(scenarios[0].tenants, 1);
        assert_eq!(scenarios[3].tenants, 32);
        assert_eq!(
            (scenarios[4].write_percent, scenarios[4].read_percent),
            (90, 10)
        );
        assert_eq!(
            (scenarios[5].write_percent, scenarios[5].read_percent),
            (10, 90)
        );
        assert!(scenarios.iter().all(|scenario| {
            scenario.write_percent + scenario.read_percent == 100 && !scenario.id.is_empty()
        }));
    }

    /// Proves calibration extends only in fixed five-second increments until
    /// both operation classes reach the required thirty scheduled attempts.
    #[test]
    fn calibration_window_obeys_attempt_floor() {
        let scenarios = reference_scenario_matrix();
        assert_eq!(calibration_seconds(8, scenarios[0]).unwrap(), 10);
        assert_eq!(calibration_seconds(8, scenarios[4]).unwrap(), 40);
        assert_eq!(calibration_seconds(8, scenarios[5]).unwrap(), 40);
        assert!(matches!(
            calibration_seconds(0, scenarios[0]),
            Err(ClusterBenchmarkError::Unsupported(_))
        ));
        for definition in scenarios {
            let seconds = calibration_seconds(8, definition).unwrap();
            let attempts = 8 * u64::from(seconds);
            let (writes, reads) = operation_attempt_counts(attempts, definition.write_percent);
            assert!(writes >= 30 && reads >= 30);
            if seconds > 5 {
                let prior_attempts = 8 * u64::from(seconds - 5);
                let (prior_writes, prior_reads) =
                    operation_attempt_counts(prior_attempts, definition.write_percent);
                assert!(prior_writes < 30 || prior_reads < 30);
            }
        }
        let balanced = (0..100)
            .map(|ordinal| is_write_operation(ordinal, 50))
            .collect::<Vec<_>>();
        assert!(balanced.windows(10).all(|window| {
            window.iter().any(|write| *write) && window.iter().any(|write| !*write)
        }));
    }

    /// Proves an unsupported capture preserves exact scheduler and outcome counts.
    #[test]
    fn qualification_diagnostic_preserves_deficient_trial_counts() {
        let report = ClusterScenarioReport {
            scenario: ClusterBenchmarkScenario {
                scenario_id: "write-heavy-three-server-three-worker-eight-tenants".to_owned(),
                workload_version: CLUSTER_WORKLOAD_VERSION.to_owned(),
                topology: ClusterTopology::ThreeServersThreeForgeWorkers,
                tenants: 8,
                traffic: TrafficMix::WriteHeavy,
                rows_per_batch: 64,
                query_row_limit: 64,
                offered_requests_per_second: 32,
                offered_load_percent: 50,
                knee_provenance: KneeProvenance::Discovered,
                warmup_seconds: 5,
                measured_seconds: 20,
                trials: 3,
                minimum_samples: 200,
                max_in_flight: DEFAULT_MAX_IN_FLIGHT,
                seed: 0xB1_F057,
            },
            discovered_knee_requests_per_second: 64,
            discovered_knee_provenance: KneeProvenance::Discovered,
            trials: Vec::new(),
            trial_evidence: Vec::new(),
            median: Default::default(),
            capacity_stages: Vec::new(),
        };
        let trial = ClusterBenchmarkTrial {
            trial: 1,
            client: ClientTrialMetrics {
                planned_operations: 640,
                attempted_operations: 640,
                accepted_operations: 639,
                backpressure_operations: 1,
                durable_write: TrialDistribution {
                    samples: 575,
                    ..Default::default()
                },
                query_time_to_first_frame: TrialDistribution {
                    samples: 64,
                    ..Default::default()
                },
                total_query: TrialDistribution {
                    samples: 64,
                    ..Default::default()
                },
                ..Default::default()
            },
            production: Default::default(),
        };

        let diagnostic = trial_deficiency(&report, &trial).expect("trial is deficient");
        assert_eq!(diagnostic["planned_operations"], 640);
        assert_eq!(diagnostic["attempted_operations"], 640);
        assert_eq!(diagnostic["successful_writes"], 575);
        assert_eq!(diagnostic["successful_query_total"], 64);
        assert_eq!(diagnostic["backpressure_operations"], 1);
        assert_eq!(diagnostic["missed_operations"], 0);
        assert_eq!(diagnostic["knee_requests_per_second"], 64);

        let profile = BifrostReferenceProfile {
            schema_version: CLUSTER_REPORT_VERSION.to_owned(),
            environment: BenchmarkEnvironment {
                architecture: "aarch64".to_owned(),
                cpu_vendor: "apple".to_owned(),
                cpu_model: "apple m1".to_owned(),
                logical_cores: 8,
                host_memory_bytes: 16 * 1024 * 1024 * 1024,
                container_cpu_millis: 2_000,
                container_memory_bytes: 4 * 1024 * 1024 * 1024,
                postgres_image: "postgres:16".to_owned(),
                postgres_version: "16".to_owned(),
                postgres_config: "reference".to_owned(),
                storage_image: "local-filesystem".to_owned(),
                storage_version: "0.0.1".to_owned(),
                storage_config: "loopback-memory-v1".to_owned(),
                storage_backend_kind: "local-filesystem".to_owned(),
                storage_root: "target/bifrost-benchmarks/storage".to_owned(),
                storage_device_class: "local".to_owned(),
                rust_major_minor: "1.88".to_owned(),
                arrow_version: "58.4.0".to_owned(),
                datafusion_version: "53.1.0".to_owned(),
                iceberg_version: "revision".to_owned(),
                os: "macos".to_owned(),
                kernel: "test".to_owned(),
                git_sha: "candidate".to_owned(),
                dirty_worktree: false,
                captured_at: "2026-08-03T00:00:00Z".to_owned(),
            },
            scenarios: vec![ReviewedScenarioProfile {
                scenario_id: report.scenario.scenario_id.clone(),
                workload: ClusterWorkloadIdentity {
                    scenario_id: report.scenario.scenario_id.clone(),
                    topology: report.scenario.topology,
                    tenants: report.scenario.tenants,
                    traffic: report.scenario.traffic,
                    rows_per_batch: report.scenario.rows_per_batch,
                    query_row_limit: report.scenario.query_row_limit,
                    auth_cache_ttl_seconds: wyrd_bench::AUTH_CACHE_TTL_SECONDS,
                    auth_invalidation_mode: wyrd_bench::AUTH_INVALIDATION_MODE.to_owned(),
                    flush_policy: wyrd_bench::FLUSH_POLICY.to_owned(),
                    routing_policy: wyrd_bench::ROUTING_POLICY.to_owned(),
                    batching_policy: wyrd_bench::BATCHING_POLICY.to_owned(),
                    storage_runtime_identity:
                        "local-filesystem:target/bifrost-benchmarks/storage:local".to_owned(),
                    allocation_policy: wyrd_bench::ALLOCATION_POLICY.to_owned(),
                    tables_per_tenant: 6,
                    ordered_rates: vec![report.scenario.offered_requests_per_second],
                    trial_count: 3,
                    warmup_seconds: 10,
                    conditioning_seconds: 3,
                    measured_seconds: 20,
                },
                offered_rates: vec![report.scenario.offered_requests_per_second],
                reports: vec![ClusterScenarioReport {
                    trials: vec![trial],
                    ..report
                }],
            }],
            slos: BifrostSloEnvelope::default(),
        };
        let directory = tempfile::tempdir().expect("temporary diagnostic directory");
        let report_path = directory.path().join("cluster-candidate.json");
        write_qualification_diagnostic(
            &report_path,
            &profile,
            &ClusterBenchmarkError::Unsupported("insufficient samples".to_owned()),
        )
        .expect("unsupported diagnostic persists");
        let persisted = std::fs::read_to_string(
            directory
                .path()
                .join("cluster-candidate.qualification.json"),
        )
        .expect("persisted diagnostic is readable");
        let persisted: serde_json::Value =
            serde_json::from_str(&persisted).expect("persisted diagnostic is JSON");
        assert_eq!(persisted["promotable"], false);
        assert_eq!(persisted["status"], "unsupported");
        assert_eq!(persisted["deficiencies"][0]["successful_query_total"], 64);
    }

    /// Proves tenant row identities occupy non-overlapping deterministic ranges.
    #[test]
    fn tenant_row_ranges_are_disjoint() {
        for tenant in 0..32 {
            let lower = tenant_row_base(tenant);
            let upper = lower + TENANT_ROW_STRIDE - 1;
            assert!(upper < tenant_row_base(tenant + 1));
        }
    }

    /// Stable query admission rejection belongs to D22's pressure boundary.
    #[test]
    fn query_admission_rejection_is_backpressure() {
        let error = "query admission rejected";
        assert!(is_backpressure(error));
        assert!(is_retryable(error));
    }

    /// Stable WAL capacity stops calibration while unrelated server errors stay fatal.
    ///
    /// # Panics
    ///
    /// Panics when the stable WAL-capacity code is not classified as
    /// backpressure or an unrelated server failure is misclassified.
    #[test]
    fn wal_disk_full_is_backpressure_but_other_server_errors_are_not() {
        assert!(is_backpressure(
            "server error: WAL disk full; code=WYRD_VALA_507_WAL_DISK_FULL"
        ));
        assert!(!is_backpressure(
            "server error: internal failure; code=WYRD_VALA_500_INTERNAL"
        ));
        assert!(!is_backpressure(
            "server error: upstream failure; code=WYRD_VALA_502_UPSTREAM"
        ));
    }

    /// Proves locked package identities are extracted without invoking Cargo.
    #[test]
    fn locked_versions_are_exact() {
        let lock = include_str!("../../../../../Cargo.lock");
        assert_eq!(lock_version(lock, "arrow").unwrap(), "58.4.0");
        assert_eq!(lock_version(lock, "datafusion").unwrap(), "53.1.0");
        assert_eq!(
            lock_source_revision(lock, "iceberg").unwrap(),
            "1422e94ac570ed99abbf530403dd40de67d37dbf"
        );
    }
}
