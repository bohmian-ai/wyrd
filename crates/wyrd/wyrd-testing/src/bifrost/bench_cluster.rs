//! Controlled full-cluster benchmark adapter and versioned v2 diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use hdrhistogram::Histogram;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::{BifrostFrame, BifrostGrpcTransport, QueryClient, ValaSdkError};
use wyrd_bench::{
    AttemptedProbe, BenchmarkEnvironment, BenchmarkOperation, BifrostDiagnosticReport,
    BifrostReferenceProfile, BifrostRuntimeRole, BifrostSloEnvelope, CAPACITY_CONDITIONING_SECONDS,
    CAPACITY_MEASURED_SECONDS, CAPACITY_TABLES_PER_TENANT, CLUSTER_REPORT_VERSION,
    CLUSTER_WORKLOAD_VERSION, CapacityLimit, CapacityStage, CapacityStageIdentity,
    CapacityStageOutcome, CapacityStagePlan, CapacityStateMachine, ClientTrialMetrics,
    ClusterBenchmarkError, ClusterBenchmarkScenario, ClusterBenchmarkTrial, ClusterScenarioReport,
    ClusterTopology, ClusterTrialReport, ClusterWorkloadIdentity, DependencyTelemetryEvidence,
    DiagnosticStatus, EvidenceStatus, PillarTelemetryDelta, ProcessId, ProcessResourceEvidence,
    ProductionTelemetryEvidence, QualificationProfileV2, ReviewedScenarioProfile, SpanDistribution,
    TenantStageRows, TraceManifest, TrafficMix, TrialDistribution, derive_trial_median,
    extract_linux_cpu_identity, extract_macos_cpu_identity, jain_fairness,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, VisibilityMode,
};

use crate::Bootstrap;
use crate::bifrost::telemetry::{
    ClusterRuntimeRole, ClusterTelemetryEvidence, ClusterTelemetryExpectation,
    ClusterTelemetryProjection, ClusterTraceOperation, run_sampled_window,
};
use crate::bifrost::{BifrostClusterSpec, BifrostTopology, WyrdTestCluster};

/// Stable logical table used by the controlled workload.
const REFERENCE_TABLE: &str = "cluster_reference_events";
/// Shared warmup-table name used by one capacity scenario session.
pub const CAPACITY_WARMUP_TABLE: &str = "cluster_capacity_warmup";
/// Number of deterministic capacity measurement-table slots.
pub const CAPACITY_STAGE_TABLE_COUNT: usize = 16;
/// Rows preloaded through Gate for every tenant before measured traffic.
const PRELOAD_ROWS: u64 = 8_192;
/// Maximum acknowledged identities assigned to one public reconciliation query.
const IDENTITY_RECONCILIATION_ROWS_PER_QUERY: usize = 1_024;
/// Rows in one public durable write request.
const ROWS_PER_WRITE: u32 = 64;
/// Default profile-driven concurrent public-operation cap.
const DEFAULT_MAX_IN_FLIGHT: usize = 4_096;
/// Minimum process soft open-file limit required by the reference benchmark.
const MINIMUM_OPEN_FILE_LIMIT: u64 = 8_192;
/// Disjoint logical row-id space reserved for each authenticated tenant.
const TENANT_ROW_STRIDE: u64 = 1_000_000_000_000;

/// Typed first-attempt failure for an ancillary correctness query.
#[derive(Debug, thiserror::Error)]
#[error(
    "ancillary {phase} query failed for tenant index {tenant_index} and table {table}: {source}"
)]
struct AncillaryQueryError {
    /// Closed correctness phase issuing the query.
    phase: &'static str,
    /// Numeric benchmark tenant ordinal without tenant identity material.
    tenant_index: usize,
    /// Repository-owned table name queried by the phase.
    table: String,
    /// Original typed SDK failure from the single public-client attempt.
    #[source]
    source: ValaSdkError,
}

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
    let mut cargo_bench_sentinel = false;
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
            "--bench" if !cargo_bench_sentinel => {
                cargo_bench_sentinel = true;
                index += 1;
            }
            "--bench" => return Err("Cargo benchmark sentinel may occur at most once".to_owned()),
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

/// One fully validated scenario-local qualification route.
#[derive(Debug, Clone, PartialEq, Eq)]
struct QualificationScenarioPlan {
    /// Canonical scenario definition in repository execution order.
    definition: ReferenceScenarioDefinition,
    /// Exact reviewed rates owned by this scenario.
    rates: Vec<u64>,
}

/// Resolve and validate the complete qualification selection before any IO.
///
/// # Errors
/// Returns [`ClusterBenchmarkError::Incompatible`] when command authority is
/// ambiguous or any selected entry differs from its exact canonical workload.
fn qualification_execution_plan(
    profile: &QualificationProfileV2,
    selected_scenario: Option<&str>,
    matrix: bool,
) -> Result<Vec<QualificationScenarioPlan>, ClusterBenchmarkError> {
    profile.validate()?;
    if matrix {
        if selected_scenario.is_some() {
            return Err(ClusterBenchmarkError::Incompatible(
                "qualification selection cannot combine matrix and scenario authority".to_owned(),
            ));
        }
        profile.validate_matrix()?;
    } else if selected_scenario.is_none() {
        return Err(ClusterBenchmarkError::Incompatible(
            "qualification selection requires one scenario or matrix authority".to_owned(),
        ));
    }
    reference_scenario_matrix()
        .into_iter()
        .filter(|definition| matrix || selected_scenario == Some(definition.id))
        .map(|definition| {
            let reviewed = profile.scenario(definition.id)?;
            let workload = &reviewed.qualification_workload;
            if workload.scenario_id != definition.id
                || workload.topology != definition.topology
                || workload.tenants != definition.tenants
                || workload.traffic != definition.traffic
                || workload.rows_per_batch != 64
                || workload.query_row_limit != 64
                || workload.tables_per_tenant != 18
                || workload.trial_count != 3
            {
                return Err(ClusterBenchmarkError::Incompatible(format!(
                    "reviewed workload does not exactly match scenario {}",
                    definition.id
                )));
            }
            Ok(QualificationScenarioPlan {
                definition,
                rates: reviewed
                    .rates
                    .iter()
                    .map(|rate| rate.requests_per_second)
                    .collect(),
            })
        })
        .collect()
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
    let qualification = load_reviewed_qualification_profile(&environment)?;
    let execution_plan = qualification_execution_plan(&qualification, selected_scenario, matrix)?;
    let mut reports = Vec::new();
    let mut attempted_probes = Vec::new();
    let mut current_partial = None;
    for item in execution_plan {
        let definition = item.definition;
        let rates = item.rates;
        wyrd_bench::qualification_preflight_seconds(rates.len(), 20, 180, 1_200)?;
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
                    let projected = ClusterTelemetryProjection::from_delta(
                        &telemetry,
                        ClusterTelemetryExpectation::complete(canonical_topology(
                            definition.topology,
                        )),
                    )?;
                    let evidence = ClusterTrialReport {
                        offered_requests_per_second: rate,
                        trial_index: trial,
                        metrics,
                        telemetry: adapt_pillars(&projected),
                        resources: adapt_process(&projected),
                        dependencies: adapt_dependencies(&projected),
                        traces: adapt_traces(&projected),
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
                let result = session
                    .run_stage(
                        plan,
                        Duration::from_secs(u64::from(CAPACITY_MEASURED_SECONDS)),
                    )
                    .await?;
                let stage = assemble_capacity_stage(
                    definition,
                    plan,
                    result,
                    u64::from(CAPACITY_MEASURED_SECONDS),
                );
                let outcome =
                    retain_completed_capacity_stage(&mut stages, &mut attempted_probes, stage);
                session.record_stage(plan, outcome)?;
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
    write_qualification_profile_candidate(&path, &profile)?;
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
            warmup_seconds: 10,
            measured_seconds: 20,
            trials: 3,
            minimum_samples: 200,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            seed: 0xB1_F057,
        },
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
            warmup_seconds: 10,
            measured_seconds: 20,
            trials: 3,
            minimum_samples: 200,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            seed: 0xB1_F057,
        },
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
    let projection = ClusterTelemetryProjection::from_delta(
        &result.telemetry,
        ClusterTelemetryExpectation::complete(canonical_topology(definition.topology)),
    );
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
    let (telemetry, resources, dependencies, traces) = match projection {
        Ok(projected) => (
            adapt_pillars(&projected),
            adapt_process(&projected),
            adapt_dependencies(&projected),
            adapt_traces(&projected),
        ),
        Err(error) => {
            let invalid = vec![error.to_string()];
            (
                PillarTelemetryDelta {
                    gate_accepted: 0,
                    scribe_wal_bytes: 0,
                    forge_publications: 0,
                    oracle_decoded_rows: 0,
                    oracle_analytical_slots_peak: 0,
                    oracle_slots_total: 0,
                    oracle_interactive_scan_classifications: 0,
                    oracle_predicted_scan_classifications: 0,
                    oracle_global_operator_classifications: 0,
                    oracle_pending_limit_rejections: 0,
                    oracle_lease_timeout_rejections: 0,
                    oracle_cluster_lease_rejections: 0,
                    oracle_class_lease_rejections: 0,
                    oracle_tenant_lease_rejections: 0,
                    oracle_local_slot_rejections: 0,
                    status: EvidenceStatus::Failed,
                    missing_required: Vec::new(),
                    invalid: invalid.clone(),
                },
                Vec::new(),
                DependencyTelemetryEvidence {
                    postgres_pool_wait_us: 0,
                    postgres_transactions: 0,
                    storage_bytes: 0,
                    storage_p99_us: 0,
                    wal_fsync_p99_us: 0,
                    status: EvidenceStatus::Failed,
                    missing_required: Vec::new(),
                    invalid,
                },
                Vec::new(),
            )
        }
    };
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
        kind: plan.kind,
        offered_requests_per_second: plan.offered_requests_per_second,
        completed_requests_per_second: result.client.accepted as f64
            / measured_seconds.max(1) as f64,
        duration: Duration::from_secs(measured_seconds),
        passed,
        in_flight_cap_exhaustions: result.client.in_flight_cap_exhaustions,
        stop_reasons,
        metrics: result.client.metrics(),
        telemetry,
        resources,
        dependencies,
        traces,
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
        stage.stop_reasons.push(CapacityLimit::InvalidEvidence);
    }
    stage
}

/// Classify a retained stage without converting arbitrary gate failures into saturation.
#[must_use]
fn capacity_stage_outcome(stage: &CapacityStage) -> CapacityStageOutcome {
    if stage.passed {
        CapacityStageOutcome::Passing
    } else if stage.stop_reasons == [CapacityLimit::Backpressure] {
        CapacityStageOutcome::ControlledBackpressure
    } else {
        CapacityStageOutcome::Invalid
    }
}

/// Retain one fully assembled stage and its exact probe reasons before transition.
///
/// This ordering ensures a state-machine refusal cannot discard completed
/// diagnostic evidence from the partial capacity artifact.
#[must_use]
fn retain_completed_capacity_stage(
    stages: &mut Vec<CapacityStage>,
    attempted_probes: &mut [AttemptedProbe],
    stage: CapacityStage,
) -> CapacityStageOutcome {
    let outcome = capacity_stage_outcome(&stage);
    complete_current_probe(attempted_probes, &stage.stop_reasons);
    stages.push(stage);
    outcome
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

/// Copy canonical pillar evidence into the stable benchmark report shape.
fn adapt_pillars(evidence: &ClusterTelemetryEvidence) -> PillarTelemetryDelta {
    PillarTelemetryDelta {
        gate_accepted: evidence.pillars.gate_accepted,
        scribe_wal_bytes: evidence.pillars.scribe_wal_bytes,
        forge_publications: evidence.pillars.forge_publications,
        oracle_decoded_rows: evidence.pillars.oracle_decoded_rows,
        oracle_analytical_slots_peak: evidence.pillars.oracle_analytical_slots_peak,
        oracle_slots_total: evidence.pillars.oracle_slots_total,
        oracle_interactive_scan_classifications: evidence
            .pillars
            .oracle_interactive_scan_classifications,
        oracle_predicted_scan_classifications: evidence
            .pillars
            .oracle_predicted_scan_classifications,
        oracle_global_operator_classifications: evidence
            .pillars
            .oracle_global_operator_classifications,
        oracle_pending_limit_rejections: evidence.pillars.oracle_pending_limit_rejections,
        oracle_lease_timeout_rejections: evidence.pillars.oracle_lease_timeout_rejections,
        oracle_cluster_lease_rejections: evidence.pillars.oracle_cluster_lease_rejections,
        oracle_class_lease_rejections: evidence.pillars.oracle_class_lease_rejections,
        oracle_tenant_lease_rejections: evidence.pillars.oracle_tenant_lease_rejections,
        oracle_local_slot_rejections: evidence.pillars.oracle_local_slot_rejections,
        status: EvidenceStatus::Complete,
        missing_required: Vec::new(),
        invalid: Vec::new(),
    }
}

/// Copy required canonical dependencies into the stable benchmark report shape.
fn adapt_dependencies(evidence: &ClusterTelemetryEvidence) -> DependencyTelemetryEvidence {
    DependencyTelemetryEvidence {
        postgres_pool_wait_us: evidence.dependencies.postgres_pool_wait_us.expect(
            "invariant: complete benchmark projection requires PostgreSQL acquire evidence",
        ),
        postgres_transactions: evidence.dependencies.postgres_transactions.expect(
            "invariant: complete benchmark projection requires PostgreSQL transaction evidence",
        ),
        storage_bytes: evidence
            .dependencies
            .storage_bytes
            .expect("invariant: complete benchmark projection requires storage byte evidence"),
        storage_p99_us: evidence
            .dependencies
            .storage_p99_us
            .expect("invariant: complete benchmark projection requires storage duration evidence"),
        wal_fsync_p99_us: evidence
            .dependencies
            .wal_fsync_p99_us
            .expect("invariant: complete benchmark projection requires WAL fsync evidence"),
        status: EvidenceStatus::Complete,
        missing_required: Vec::new(),
        invalid: Vec::new(),
    }
}

/// Copy canonical process evidence into the stable benchmark report shape.
fn adapt_process(evidence: &ClusterTelemetryEvidence) -> Vec<ProcessResourceEvidence> {
    let process = &evidence.process;
    vec![ProcessResourceEvidence {
        process_id: ProcessId(process.identity.clone()),
        epoch: process.epoch,
        hosted_logical_nodes: process.hosted_logical_nodes.clone(),
        roles: process
            .roles
            .iter()
            .map(|role| match role {
                ClusterRuntimeRole::Gate => BifrostRuntimeRole::Gate,
                ClusterRuntimeRole::Scribe => BifrostRuntimeRole::Scribe,
                ClusterRuntimeRole::Forge => BifrostRuntimeRole::Forge,
                ClusterRuntimeRole::Oracle => BifrostRuntimeRole::Oracle,
            })
            .collect(),
        cpu_seconds: process.cpu_seconds,
        peak_rss_bytes: process.peak_rss_bytes,
        current_rss_bytes: process.current_rss_bytes,
        runtime_busy_seconds: process.runtime_busy_seconds,
        runtime_queue_peak: process.runtime_queue_peak,
    }]
}

/// Copy canonical trace evidence into the stable benchmark report shape.
fn adapt_traces(evidence: &ClusterTelemetryEvidence) -> Vec<TraceManifest> {
    evidence
        .traces
        .iter()
        .map(|trace| {
            let operation = match trace.operation {
                ClusterTraceOperation::DurableWrite => BenchmarkOperation::DurableWrite,
                ClusterTraceOperation::FlushToVisible => BenchmarkOperation::FlushToVisible,
                ClusterTraceOperation::QueryTimeToFirstFrame => {
                    BenchmarkOperation::QueryTimeToFirstFrame
                }
                ClusterTraceOperation::QueryTotal => BenchmarkOperation::QueryTotal,
            };
            TraceManifest {
                operation,
                representative_trace_ids: trace.representative_trace_ids.clone(),
                critical_path_spans: vec![SpanDistribution {
                    operation,
                    samples: trace.samples,
                    p95_us: trace.p95_us,
                    p99_us: trace.p99_us,
                }],
            }
        })
        .collect()
}

/// Convert report topology into the dependency-neutral test-cluster topology.
const fn canonical_topology(topology: ClusterTopology) -> BifrostTopology {
    match topology {
        ClusterTopology::OnePod => BifrostTopology::OnePod,
        ClusterTopology::ThreeServersThreeForgeWorkers => {
            BifrostTopology::ThreeServersThreeForgeWorkers
        }
    }
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
        probe.stop_reasons = stop_reasons.to_vec();
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
        "offered_requests_per_second": report.scenario.offered_requests_per_second,
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

/// Select one deterministic interleaved traffic class from the fixed seed.
#[must_use]
fn is_write_operation(ordinal: u64, write_percent: u8) -> bool {
    (ordinal.saturating_mul(37).saturating_add(0xB1_F057) % 100) < u64::from(write_percent)
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
    trial: u8,
) -> Result<
    (ClusterBenchmarkScenario, ClusterBenchmarkTrial),
    Box<dyn std::error::Error + Send + Sync>,
> {
    run_trial_with_windows(ReferenceTrialSpec {
        definition,
        rate: offered_requests_per_second,
        trial,
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
    /// One-based independent trial number.
    trial: u8,
    /// Warmup window excluded from evidence.
    warmup: Duration,
    /// Measured evidence window.
    measured: Duration,
    /// Profile-driven concurrent public operation cap.
    max_in_flight: usize,
}

/// One completed capacity table and its exact acknowledged identities.
struct CompletedTableLedger {
    /// Boot-provisioned table queried during cumulative reconciliation.
    table: String,
    /// Exact acknowledged identities for each tenant in roster order.
    expected_rows_by_tenant: Vec<BTreeSet<u64>>,
}

/// One live, benchmark-only capacity scenario lifecycle.
///
/// The session owns the T15 cluster, tenant-bound public clients, exact
/// seventeen-table allocation, stage allocator, cumulative identity ledger,
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
    /// Completed measurement tables and their exact per-tenant ledgers.
    completed_tables: Vec<CompletedTableLedger>,
}

impl CapacityScenarioSession {
    /// Boot one T15 cluster and provision the complete bounded stage allocation.
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
            return Err(
                "capacity allocation did not contain the complete bounded stage set".into(),
            );
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
    /// Returns an allocation error when the stage slot is outside the sixteen
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
        outcome: CapacityStageOutcome,
    ) -> Result<(), ClusterBenchmarkError> {
        self.stages.record(plan, outcome)
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
            duration: Duration::from_secs(u64::from(CAPACITY_CONDITIONING_SECONDS)),
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
        let expected_rows_by_tenant = result
            .tenant_write_ordinals
            .iter()
            .enumerate()
            .map(|(tenant, ordinals)| {
                stage_row_identity(tenant, ordinals)
                    .row_ids
                    .into_iter()
                    .collect::<BTreeSet<_>>()
            })
            .collect::<Vec<_>>();
        let published = final_published_identities(
            cluster,
            &self.tenants,
            &self.clients,
            self.stage_table(plan)?,
            &expected_rows_by_tenant,
        )
        .await?;
        for (tenant, expected) in expected_rows_by_tenant.iter().enumerate() {
            self.expected_rows[tenant].extend(expected);
        }
        self.completed_tables.push(CompletedTableLedger {
            table: self.stage_table(plan)?.to_owned(),
            expected_rows_by_tenant,
        });
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
        for completed in &self.completed_tables {
            let identities = final_published_identities(
                cluster,
                &self.tenants,
                &self.clients,
                &completed.table,
                &completed.expected_rows_by_tenant,
            )
            .await?;
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
            crate::bifrost::telemetry::BifrostTelemetryDelta,
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
        let ((result, audit_before, audit_after), telemetry) =
            run_sampled_window(cluster.telemetry(), || async {
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
                let audit_after = audit_rows(cluster, &self.tenants).await?;
                Ok((result, audit_before, audit_after))
            })
            .await?;
        let expected_rows_by_tenant = result
            .tenant_write_ordinals
            .iter()
            .enumerate()
            .map(|(tenant, ordinals)| {
                stage_row_identity(tenant, ordinals)
                    .row_ids
                    .into_iter()
                    .collect::<BTreeSet<_>>()
            })
            .collect::<Vec<_>>();
        let published = final_published_identities(
            cluster,
            &self.tenants,
            &self.clients,
            measurement_table,
            &expected_rows_by_tenant,
        )
        .await?
        .iter()
        .map(|identities| identities.len() as u64)
        .sum();
        let projected = ClusterTelemetryProjection::from_delta(
            &telemetry,
            ClusterTelemetryExpectation::complete(canonical_topology(self.definition.topology)),
        )?;
        let production = reconcile_production(
            &projected,
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

/// Build the exact one-warmup plus sixteen-stage capacity allocation.
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
    let mut expected_rows_by_tenant = preload_identity_ledger(tenants.len());
    extend_identity_ledger(&mut expected_rows_by_tenant, &warmup.tenant_write_ordinals);
    let published_before_identities = final_published_identities(
        cluster,
        &tenants,
        &clients,
        REFERENCE_TABLE,
        &expected_rows_by_tenant,
    )
    .await?;
    let published_before = published_before_identities
        .iter()
        .map(|identities| identities.len() as u64)
        .sum::<u64>();
    let expected_before = expected_rows_by_tenant
        .iter()
        .map(|identities| identities.len() as u64)
        .sum::<u64>();
    if published_before != expected_before {
        return Err(format!(
            "post-warmup published rows {published_before} do not equal preload plus acknowledged warmup rows {expected_before}"
        )
        .into());
    }
    prove_wrong_tenant_query(cluster, &tenants, &clients).await?;
    let ((measured, audit_before, audit_after), telemetry) =
        run_sampled_window(cluster.telemetry(), || async {
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
            Ok((measured, audit_before, audit_after))
        })
        .await?;
    extend_identity_ledger(
        &mut expected_rows_by_tenant,
        &measured.tenant_write_ordinals,
    );
    let published_after_identities = final_published_identities(
        cluster,
        &tenants,
        &clients,
        REFERENCE_TABLE,
        &expected_rows_by_tenant,
    )
    .await?;
    let published_rows = published_after_identities
        .iter()
        .map(|identities| identities.len() as u64)
        .sum::<u64>();
    let measured_published_rows = published_rows
        .checked_sub(published_before)
        .ok_or("final publication cardinality regressed below the post-warmup baseline")?;
    let projected = ClusterTelemetryProjection::from_delta(
        &telemetry,
        ClusterTelemetryExpectation::complete(canonical_topology(definition.topology)),
    )?;
    let production = reconcile_production(
        &projected,
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

/// Build the exact deterministic preload ledger for every tenant.
#[must_use]
fn preload_identity_ledger(tenant_count: usize) -> Vec<BTreeSet<u64>> {
    (0..tenant_count)
        .map(|tenant| {
            let start = tenant_row_base(tenant);
            (start..start + PRELOAD_ROWS).collect()
        })
        .collect()
}

/// Extend a per-tenant identity ledger from acknowledged write ordinals.
fn extend_identity_ledger(ledger: &mut [BTreeSet<u64>], acknowledged_ordinals: &[BTreeSet<u64>]) {
    for (tenant, ordinals) in acknowledged_ordinals.iter().enumerate() {
        ledger[tenant].extend(stage_row_identity(tenant, ordinals).row_ids);
    }
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
    /// Stable admission/backpressure rejections from durable writes.
    write_backpressure: u64,
    /// Stable admission/backpressure rejections from queries and visibility probes.
    query_backpressure: u64,
    /// Stable public error-code counts for rejected operations.
    backpressure_by_code: BTreeMap<String, u64>,
    /// Planned operations not spawned because the in-flight cap was exhausted.
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
    telemetry: crate::bifrost::telemetry::BifrostTelemetryDelta,
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
        self.query_backpressure = self.query_backpressure.saturating_add(flush_result.3);
    }

    /// Convert the complete measured ledger into report distributions and rates.
    fn metrics(&self) -> ClientTrialMetrics {
        ClientTrialMetrics {
            planned_operations: self.submitted,
            attempted_operations: self.submitted.saturating_sub(self.missed_operations),
            accepted_operations: self.accepted,
            backpressure_operations: self.backpressure,
            write_backpressure_operations: self.write_backpressure,
            query_backpressure_operations: self.query_backpressure,
            backpressure_by_code: self.backpressure_by_code.clone(),
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
    WriteBackpressure {
        /// Stable public error code returned by Gate.
        code: &'static str,
    },
    /// Stable public query or visibility admission/backpressure rejection.
    QueryBackpressure {
        /// Stable public error code returned by Gate or Oracle.
        code: &'static str,
    },
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
            let Some(offered_planned) =
                await_scheduled_offer(&mut result, planned, tasks.len(), self.max_in_flight).await
            else {
                continue;
            };
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
                    scheduled_write(client, &table, tenant_index, phase_ordinal, offered_planned)
                        .await
                } else {
                    scheduled_query(client, &table, tenant_index, ordinal, offered_planned).await
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

/// Await one immutable instant and hand it unchanged to the spawn boundary.
///
/// A late wakeup remains an offer. `None` means only that the declared
/// in-flight cap refused the operation after its planned instant arrived.
async fn await_scheduled_offer(
    result: &mut WindowResult,
    planned: tokio::time::Instant,
    in_flight: usize,
    max_in_flight: usize,
) -> Option<tokio::time::Instant> {
    tokio::time::sleep_until(planned).await;
    record_scheduled_offer(result, in_flight, max_in_flight).then_some(planned)
}

/// Record one immutable planned offer and decide whether it can be spawned.
///
/// Scheduler lateness is deliberately absent from this decision: a late wakeup
/// still spawns against its original planned instant so end-to-end latency
/// retains the delay. Only the declared in-flight cap can refuse the offer.
#[must_use]
fn record_scheduled_offer(
    result: &mut WindowResult,
    in_flight: usize,
    max_in_flight: usize,
) -> bool {
    result.submitted = result.submitted.saturating_add(1);
    if in_flight < max_in_flight {
        return true;
    }
    result.in_flight_cap_exhaustions = result.in_flight_cap_exhaustions.saturating_add(1);
    result.missed_operations = result.missed_operations.saturating_add(1);
    false
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
                return Ok(OperationResult::WriteBackpressure { code: error.code() });
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
            return Ok(OperationResult::QueryBackpressure { code: error.code() });
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
        OperationResult::WriteBackpressure { code } => {
            result.backpressure = result.backpressure.saturating_add(1);
            result.write_backpressure = result.write_backpressure.saturating_add(1);
            *result
                .backpressure_by_code
                .entry(code.to_owned())
                .or_default() += 1;
        }
        OperationResult::QueryBackpressure { code } => {
            result.backpressure = result.backpressure.saturating_add(1);
            result.query_backpressure = result.query_backpressure.saturating_add(1);
            *result
                .backpressure_by_code
                .entry(code.to_owned())
                .or_default() += 1;
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
    let mut stream =
        clients[0]
            .query
            .query(&request)
            .await
            .map_err(|source| AncillaryQueryError {
                phase: "wrong-tenant isolation",
                tenant_index: 0,
                table: REFERENCE_TABLE.to_owned(),
                source,
            })?;
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

/// One disjoint public-query range spanning part of the signed row-id domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdentityReconciliationRange {
    /// Exclusive lower endpoint, absent for the signed-domain prefix.
    lower_exclusive: Option<u64>,
    /// Inclusive upper endpoint, absent for the signed-domain suffix.
    upper_inclusive: Option<u64>,
    /// Number of acknowledged identities assigned to this query.
    expected_rows: usize,
}

impl IdentityReconciliationRange {
    /// Return whether one observed identity satisfies this range predicate.
    #[must_use]
    fn contains(self, identity: u64) -> bool {
        self.lower_exclusive.is_none_or(|lower| identity > lower)
            && self.upper_inclusive.is_none_or(|upper| identity <= upper)
    }

    /// Append the exact disjoint predicate used by the public SQL query.
    #[must_use]
    fn sql(self, table: &str) -> String {
        let projection = format!("SELECT row_id, wyrd_event_time FROM vala.bifrost.{table}");
        match (self.lower_exclusive, self.upper_inclusive) {
            (None, None) => projection,
            (None, Some(upper)) => format!("{projection} WHERE row_id <= {upper}"),
            (Some(lower), Some(upper)) => {
                format!("{projection} WHERE row_id > {lower} AND row_id <= {upper}")
            }
            (Some(lower), None) => format!("{projection} WHERE row_id > {lower}"),
        }
    }
}

/// Partition the complete signed row-id domain around bounded expected chunks.
#[must_use]
fn identity_reconciliation_ranges(expected: &BTreeSet<u64>) -> Vec<IdentityReconciliationRange> {
    if expected.is_empty() {
        return vec![IdentityReconciliationRange {
            lower_exclusive: None,
            upper_inclusive: None,
            expected_rows: 0,
        }];
    }
    let chunks = expected
        .iter()
        .copied()
        .collect::<Vec<_>>()
        .chunks(IDENTITY_RECONCILIATION_ROWS_PER_QUERY)
        .map(|chunk| {
            (
                chunk.len(),
                *chunk.last().expect("invariant: expected chunk is nonempty"),
            )
        })
        .collect::<Vec<_>>();
    if chunks.len() == 1 {
        return vec![IdentityReconciliationRange {
            lower_exclusive: None,
            upper_inclusive: None,
            expected_rows: chunks[0].0,
        }];
    }
    let mut ranges = Vec::with_capacity(chunks.len());
    let mut previous_endpoint = None;
    for (index, (expected_rows, endpoint)) in chunks.iter().copied().enumerate() {
        let final_range = index + 1 == chunks.len();
        ranges.push(IdentityReconciliationRange {
            lower_exclusive: previous_endpoint,
            upper_inclusive: (!final_range).then_some(endpoint),
            expected_rows,
        });
        previous_endpoint = Some(endpoint);
    }
    ranges
}

/// Merge one range only after its public query reports a successful terminal.
///
/// # Errors
/// Returns an error for a missing/non-success terminal, a predicate violation,
/// or a duplicate already observed by another disjoint range.
fn merge_completed_identity_range(
    range: IdentityReconciliationRange,
    terminal: Option<QueryTerminalOutcome>,
    range_identities: BTreeSet<u64>,
    published: &mut BTreeSet<u64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if terminal != Some(QueryTerminalOutcome::Success) {
        return Err("final reference range query omitted a successful terminal".into());
    }
    if range_identities
        .iter()
        .any(|identity| !range.contains(*identity))
    {
        return Err(
            "final reference range query returned an identity outside its predicate".into(),
        );
    }
    for identity in range_identities {
        if !published.insert(identity) {
            return Err(
                "final reference queries returned a duplicate identity across ranges".into(),
            );
        }
    }
    Ok(())
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
    expected_rows_by_tenant: &[BTreeSet<u64>],
) -> Result<Vec<BTreeSet<u64>>, Box<dyn std::error::Error + Send + Sync>> {
    if tenants.len() != clients.len() || tenants.len() != expected_rows_by_tenant.len() {
        return Err(
            "final identity reconciliation tenant/client/ledger cardinality differs".into(),
        );
    }
    let mut published = Vec::with_capacity(tenants.len());
    for (tenant_index, client) in clients.iter().enumerate().take(tenants.len()) {
        let mut identities = BTreeSet::new();
        let lower = tenant_row_base(tenant_index);
        let upper = lower + TENANT_ROW_STRIDE - 1;
        for range in identity_reconciliation_ranges(&expected_rows_by_tenant[tenant_index]) {
            let request = BifrostQueryRequest {
                sql: range.sql(table),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            };
            let mut stream =
                client
                    .query
                    .query(&request)
                    .await
                    .map_err(|source| AncillaryQueryError {
                        phase: "final published identity",
                        tenant_index,
                        table: table.to_owned(),
                        source,
                    })?;
            let mut range_identities = BTreeSet::new();
            while let Some(batch) = stream.next_batch().await? {
                collect_query_identities(&batch, lower, upper, &mut range_identities)?;
            }
            merge_completed_identity_range(
                range,
                stream.terminal().map(|terminal| terminal.outcome),
                range_identities,
                &mut identities,
            )?;
        }
        if identities != expected_rows_by_tenant[tenant_index] {
            return Err(
                "final public Oracle identities differ from the acknowledged ledger".into(),
            );
        }
        published.push(identities);
    }
    Ok(published)
}

/// Reconcile measured client ledgers with production metric and audit evidence.
fn reconcile_production(
    evidence: &ClusterTelemetryEvidence,
    client: &WindowResult,
    observed_audit_rows: u64,
    published_rows: u64,
) -> Result<ProductionTelemetryEvidence, Box<dyn std::error::Error + Send + Sync>> {
    let rows = client.tenant_write_rows.iter().sum::<u64>();
    let completed_queries = client.telemetry_completed_queries;
    if published_rows != rows {
        return Err(format!(
            "measured published row delta {published_rows} does not equal acknowledged measured rows {rows}"
        )
        .into());
    }
    let evidence = ProductionTelemetryEvidence {
        gate_accepted_rows: evidence.phase.gate_rows,
        scribe_accepted_rows: evidence.phase.scribe_rows,
        client_acknowledged_rows: rows,
        scribe_persisted_rows: evidence.reconciliation.sealed_rows,
        forge_published_rows: published_rows,
        oracle_stream_rows: evidence.phase.oracle_stream_rows,
        client_decoded_rows: client.telemetry_decoded_rows,
        oracle_terminal_outcomes: evidence.reconciliation.successful_queries,
        client_completed_queries: completed_queries,
        expected_audit_rows: completed_queries,
        observed_audit_rows,
        required_telemetry: EvidenceStatus::Complete,
        counter_integrity: EvidenceStatus::Complete,
        spans_clean: EvidenceStatus::Complete,
        cleanup: if evidence.cleanup.is_clean() {
            EvidenceStatus::Complete
        } else {
            EvidenceStatus::Failed
        },
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
            session.record_stage(plan, capacity_stage_outcome(&stage))?;
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
    let repository = repository_root();
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

/// Return the repository root resolved from the owning crate manifest.
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Load the explicitly reviewed qualification profile and verify its source digest when present.
///
/// # Errors
/// Returns `Unsupported` when the checked-in profile is absent, malformed,
/// incompatible with the live environment, or no longer matches the retained
/// canonical capacity artifact.
fn load_reviewed_qualification_profile(
    environment: &BenchmarkEnvironment,
) -> Result<QualificationProfileV2, Box<dyn std::error::Error + Send + Sync>> {
    let profile_path = repository_root().join("benches/bifrost/qualification-profile-v2.json");
    let capacity_path = repository_root().join("target/bifrost-benchmarks/cluster-capacity.json");
    load_reviewed_qualification_profile_from_paths(environment, &profile_path, &capacity_path)
}

/// Load reviewed rates from explicit paths for deterministic source-binding tests.
///
/// # Errors
/// Returns the same profile, environment, digest, report, and membership
/// failures as [`load_reviewed_qualification_profile`].
fn load_reviewed_qualification_profile_from_paths(
    environment: &BenchmarkEnvironment,
    profile_path: &Path,
    capacity_path: &Path,
) -> Result<QualificationProfileV2, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = std::fs::read(profile_path).map_err(|error| {
        ClusterBenchmarkError::Unsupported(format!(
            "reviewed qualification profile {} is unavailable: {error}",
            profile_path.display()
        ))
    })?;
    let profile: QualificationProfileV2 = serde_json::from_slice(&bytes)?;
    profile.validate()?;
    if !profile.environment.compatible_with(environment) {
        return Err(Box::new(ClusterBenchmarkError::Incompatible(
            "reviewed qualification profile environment is incompatible with this capture"
                .to_owned(),
        )));
    }
    if capacity_path.exists() {
        let capacity_bytes = std::fs::read(capacity_path)?;
        if sha256_bytes(&capacity_bytes)? != profile.source_capacity.artifact_sha256 {
            return Err(Box::new(ClusterBenchmarkError::Incompatible(
                "reviewed qualification profile source digest does not match canonical capacity"
                    .to_owned(),
            )));
        }
        let capacity_report: BifrostReferenceProfile = serde_json::from_slice(&capacity_bytes)?;
        profile.validate_capacity_source(&capacity_report)?;
    }
    Ok(profile)
}

/// Generate a non-authoritative candidate beside benchmark captures.
///
/// # Errors
/// Returns the deterministic selection, serialization, digest, or IO error.
fn write_qualification_profile_candidate(
    capacity_path: &Path,
    report: &BifrostReferenceProfile,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let candidate =
        QualificationProfileV2::from_capacity_report(report, sha256_file(capacity_path)?)?;
    let candidate_path =
        repository_root().join("target/bifrost-benchmarks/qualification-profile-v2.candidate.json");
    if let Some(parent) = candidate_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        candidate_path,
        format!("{}\n", serde_json::to_string_pretty(&candidate)?),
    )?;
    Ok(())
}

/// Compute the lowercase SHA-256 emitted by the host's standard digest tool.
///
/// # Errors
/// Returns `Unsupported` when neither supported digest command succeeds or
/// when its output is not a lowercase 64-character SHA-256.
fn sha256_file(path: &Path) -> Result<String, ClusterBenchmarkError> {
    let bytes = std::fs::read(path).map_err(|error| {
        ClusterBenchmarkError::Unsupported(format!(
            "cannot read capacity artifact {}: {error}",
            path.display()
        ))
    })?;
    sha256_bytes(&bytes)
}

/// Compute lowercase SHA-256 over exact already-read artifact bytes.
///
/// # Errors
/// Returns `Unsupported` when the host digest command cannot be started,
/// cannot consume the complete byte slice, fails, or emits an invalid digest.
fn sha256_bytes(bytes: &[u8]) -> Result<String, ClusterBenchmarkError> {
    let mut child = Command::new("shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .or_else(|_| {
            Command::new("sha256sum")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
        })
        .map_err(|error| {
            ClusterBenchmarkError::Unsupported(format!(
                "cannot start a SHA-256 digest command: {error}"
            ))
        })?;
    child
        .stdin
        .take()
        .ok_or_else(|| {
            ClusterBenchmarkError::Unsupported(
                "SHA-256 digest command has no writable stdin".to_owned(),
            )
        })?
        .write_all(bytes)
        .map_err(|error| {
            ClusterBenchmarkError::Unsupported(format!(
                "cannot write capacity bytes to SHA-256 command: {error}"
            ))
        })?;
    let result = child.wait_with_output().map_err(|error| {
        ClusterBenchmarkError::Unsupported(format!(
            "cannot collect SHA-256 digest command: {error}"
        ))
    })?;
    if !result.status.success() {
        return Err(ClusterBenchmarkError::Unsupported(format!(
            "SHA-256 digest command exited with {}",
            result.status
        )));
    }
    let output = String::from_utf8(result.stdout).map_err(|error| {
        ClusterBenchmarkError::Unsupported(format!("SHA-256 digest output is not UTF-8: {error}"))
    })?;
    let digest = output.split_whitespace().next().unwrap_or_default();
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ClusterBenchmarkError::Unsupported(
            "capacity digest command did not emit lowercase SHA-256".to_owned(),
        ));
    }
    Ok(digest.to_owned())
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
        let cargo_capacity = parse_cluster_benchmark_args([
            "--mode",
            "capacity",
            "--scenario",
            "balanced-one-pod-eight-tenants",
            "--bench",
        ])
        .expect("Cargo capacity command parses");
        assert_eq!(cargo_capacity, command);

        let qualification = parse_cluster_benchmark_args(["--mode", "qualification", "--matrix"])
            .expect("canonical qualification command parses");
        let cargo_qualification =
            parse_cluster_benchmark_args(["--mode", "qualification", "--matrix", "--bench"])
                .expect("Cargo qualification command parses");
        assert_eq!(cargo_qualification, qualification);

        assert!(
            parse_cluster_benchmark_args(["--mode", "capacity", "--matrix", "--bench", "--bench"])
                .is_err()
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

    /// Proves capacity boot owns one shared warmup table and sixteen unique
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

    /// Build six independently qualified scenario entries for routing tests.
    fn qualification_profile_fixture() -> QualificationProfileV2 {
        let environment = diagnostic_environment_fixture();
        let reports = reference_scenario_matrix()
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let discovery = if index == 1 {
                    vec![25, 50, 75]
                } else {
                    vec![25, 50, 75, 100, 200]
                };
                let recovery = *discovery.last().expect("fixture has discovery rates");
                let mut stages = discovery
                    .into_iter()
                    .enumerate()
                    .map(|(slot, rate)| {
                        assemble_capacity_stage(
                            definition,
                            CapacityStagePlan {
                                slot: u16::try_from(slot).expect("bounded fixture slot"),
                                offered_requests_per_second: rate,
                                kind: wyrd_bench::CapacityStageKind::Discovery,
                            },
                            completed_stage_fixture(0, true),
                            20,
                        )
                    })
                    .collect::<Vec<_>>();
                stages.push(assemble_capacity_stage(
                    definition,
                    CapacityStagePlan {
                        slot: u16::try_from(stages.len()).expect("bounded fixture slot"),
                        offered_requests_per_second: recovery,
                        kind: wyrd_bench::CapacityStageKind::Recovery,
                    },
                    completed_stage_fixture(0, true),
                    20,
                ));
                capacity_scenario_report(definition, stages)
            })
            .collect();
        let report = BifrostReferenceProfile {
            schema_version: CLUSTER_REPORT_VERSION.to_owned(),
            environment: environment.clone(),
            scenarios: reviewed_scenario_profiles(
                reports,
                &environment_storage_identity(&environment),
            ),
            slos: BifrostSloEnvelope::default(),
        };
        QualificationProfileV2::from_capacity_report(&report, "a".repeat(64))
            .expect("six-scenario fixture qualifies")
    }

    /// Proves all matrix routes validate before a startup loop can begin.
    #[test]
    fn qualification_preflight_rejects_final_workload_before_any_start() {
        let mut profile = qualification_profile_fixture();
        profile
            .scenarios
            .last_mut()
            .expect("sixth scenario exists")
            .qualification_workload
            .query_row_limit = 65;
        let mut starts = 0_u8;
        match qualification_execution_plan(&profile, None, true) {
            Ok(plan) => {
                for _ in plan {
                    starts = starts.saturating_add(1);
                }
                panic!("malformed final workload must fail preflight");
            }
            Err(ClusterBenchmarkError::Incompatible(_)) => {}
            Err(error) => panic!("unexpected preflight error: {error}"),
        }
        assert_eq!(starts, 0);
    }

    /// Proves production preflight preserves distinct scenario-owned rates.
    #[test]
    fn qualification_preflight_routes_distinct_local_rates() {
        let profile = qualification_profile_fixture();
        let plan =
            qualification_execution_plan(&profile, None, true).expect("complete matrix preflights");
        assert_eq!(plan.len(), 6);
        assert_eq!(plan[0].rates, vec![25, 100, 200]);
        assert_eq!(plan[1].rates, vec![25, 50, 75]);
        assert_eq!(plan[0].definition.id, "balanced-one-pod-one-tenant");
        assert_eq!(plan[1].definition.id, "balanced-one-pod-eight-tenants");
    }

    /// Proves an existing digest-matched source is deserialized and every
    /// reviewed rate is checked against retained stage membership.
    #[test]
    fn reviewed_rate_loader_enforces_capacity_source_membership() {
        let environment = diagnostic_environment_fixture();
        let definition = reference_scenario_matrix()[0];
        let mut stages = [25, 50, 75, 100, 200]
            .into_iter()
            .enumerate()
            .map(|(slot, rate)| {
                assemble_capacity_stage(
                    definition,
                    CapacityStagePlan {
                        slot: u16::try_from(slot).expect("bounded fixture slot"),
                        offered_requests_per_second: rate,
                        kind: wyrd_bench::CapacityStageKind::Discovery,
                    },
                    completed_stage_fixture(0, true),
                    20,
                )
            })
            .collect::<Vec<_>>();
        stages.push(assemble_capacity_stage(
            definition,
            CapacityStagePlan {
                slot: 5,
                offered_requests_per_second: 200,
                kind: wyrd_bench::CapacityStageKind::Recovery,
            },
            completed_stage_fixture(0, true),
            20,
        ));
        let capacity_report = BifrostReferenceProfile {
            schema_version: CLUSTER_REPORT_VERSION.to_owned(),
            environment: environment.clone(),
            scenarios: reviewed_scenario_profiles(
                vec![capacity_scenario_report(definition, stages.clone())],
                &environment_storage_identity(&environment),
            ),
            slos: BifrostSloEnvelope::default(),
        };
        let capacity_bytes = serde_json::to_vec(&capacity_report).expect("capacity fixture JSON");
        let profile = QualificationProfileV2::from_capacity_report(
            &capacity_report,
            sha256_bytes(&capacity_bytes).expect("fixture digest"),
        )
        .expect("valid qualification fixture");
        let directory = tempfile::tempdir().expect("temporary profile directory");
        let profile_path = directory.path().join("qualification-profile-v2.json");
        let capacity_path = directory.path().join("cluster-capacity.json");
        std::fs::write(&capacity_path, &capacity_bytes).expect("write capacity fixture");
        std::fs::write(
            &profile_path,
            serde_json::to_vec(&profile).expect("profile fixture JSON"),
        )
        .expect("write profile fixture");
        assert_eq!(
            load_reviewed_qualification_profile_from_paths(
                &environment,
                &profile_path,
                &capacity_path,
            )
            .expect("exact source membership loads")
            .scenarios[0]
                .rates
                .iter()
                .map(|rate| rate.requests_per_second)
                .collect::<Vec<_>>(),
            vec![25, 100, 200],
        );

        let mut invented = profile;
        invented.scenarios[0].selected_stages[0].stage_id = "invented-stage".to_owned();
        std::fs::write(
            &profile_path,
            serde_json::to_vec(&invented).expect("tampered profile JSON"),
        )
        .expect("write tampered profile fixture");
        assert!(
            load_reviewed_qualification_profile_from_paths(
                &environment,
                &profile_path,
                &capacity_path,
            )
            .is_err()
        );
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

    /// Proves an ordinary late scheduler wakeup still offers the operation and
    /// retains the original planned instant in observed latency.
    #[tokio::test(start_paused = true)]
    async fn late_wakeup_still_offers_from_original_planned_instant() {
        let start = tokio::time::Instant::now();
        let planned = start + Duration::from_millis(10);
        tokio::time::advance(Duration::from_millis(30)).await;
        let mut result = WindowResult::default();
        let offered = await_scheduled_offer(&mut result, planned, 0, 1)
            .await
            .expect("late wakeup still offers");
        let metrics = result.metrics();
        assert_eq!(offered, planned);
        assert_eq!(metrics.planned_operations, 1);
        assert_eq!(metrics.attempted_operations, 1);
        assert_eq!(metrics.missed_operations, 0);
        assert!(elapsed_us(offered) >= 20_000);
    }

    /// Proves cap refusal is planned but unattempted and preserves the exact
    /// `planned == attempted + missed` accounting identity.
    #[test]
    fn in_flight_refusal_is_cap_exhaustion_and_missed() {
        let mut result = WindowResult {
            max_in_flight: 1,
            ..WindowResult::default()
        };
        assert!(!record_scheduled_offer(&mut result, 1, 1));
        let metrics = result.metrics();
        assert_eq!(metrics.planned_operations, 1);
        assert_eq!(metrics.attempted_operations, 0);
        assert_eq!(metrics.missed_operations, 1);
        assert_eq!(metrics.in_flight_cap_exhaustions, 1);
        assert_eq!(
            metrics.planned_operations,
            metrics.attempted_operations + metrics.missed_operations
        );
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

    /// Proves per-table capacity ledgers catch corruption that a cumulative
    /// union alone would conceal.
    #[test]
    fn completed_table_ledgers_prevent_cross_table_substitution() {
        let first = CompletedTableLedger {
            table: "first".to_owned(),
            expected_rows_by_tenant: vec![BTreeSet::from([10, 11])],
        };
        let second = CompletedTableLedger {
            table: "second".to_owned(),
            expected_rows_by_tenant: vec![BTreeSet::from([20, 21])],
        };
        let corrupted_first = vec![BTreeSet::from([10, 20])];
        let corrupted_second = vec![BTreeSet::from([11, 21])];
        let expected_union = first.expected_rows_by_tenant[0]
            .union(&second.expected_rows_by_tenant[0])
            .copied()
            .collect::<BTreeSet<_>>();
        let corrupted_union = corrupted_first[0]
            .union(&corrupted_second[0])
            .copied()
            .collect::<BTreeSet<_>>();

        assert_eq!(expected_union, corrupted_union);
        assert!(!exact_identity_ledger_matches(
            &first.expected_rows_by_tenant,
            &corrupted_first
        ));
        assert!(!exact_identity_ledger_matches(
            &second.expected_rows_by_tenant,
            &corrupted_second
        ));
    }

    /// Proves trace report adapters copy the canonical operation and distribution.
    #[test]
    fn benchmark_trace_adapter_copies_canonical_evidence() {
        let evidence = canonical_adapter_evidence();
        let traces = adapt_traces(&evidence);
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].operation, BenchmarkOperation::QueryTotal);
        assert_eq!(traces[0].representative_trace_ids, vec!["trace-1"]);
        assert_eq!(traces[0].critical_path_spans[0].samples, 7);
        assert_eq!(traces[0].critical_path_spans[0].p95_us, 95);
        assert_eq!(traces[0].critical_path_spans[0].p99_us, 99);
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
        apply_operation(
            &mut result,
            OperationResult::WriteBackpressure {
                code: "WYRD_VALA_507_WAL_DISK_FULL",
            },
        );
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

    /// Proves a completed invalid stage reaches the partial artifact before
    /// the state machine returns its non-saturation `NotReady` error.
    #[test]
    fn invalid_completed_stage_is_retained_before_transition_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("partial-invalid-stage.json");
        let environment = diagnostic_environment_fixture();
        let definition = reference_scenario_matrix()[0];
        let plan = CapacityStagePlan {
            slot: 2,
            offered_requests_per_second: 75,
            kind: wyrd_bench::CapacityStageKind::Discovery,
        };
        let invalid =
            assemble_capacity_stage(definition, plan, completed_stage_fixture(0, false), 20);
        assert_eq!(invalid.stop_reasons, vec![CapacityLimit::InvalidEvidence]);
        let mut probes = Vec::new();
        begin_probe(&mut probes, definition.id.to_owned(), 75);
        let mut stages = Vec::new();
        let outcome = retain_completed_capacity_stage(&mut stages, &mut probes, invalid);
        let mut machine = CapacityStateMachine::for_rates(vec![75]);
        let transition = machine.record(plan, outcome);
        assert!(matches!(
            &transition,
            Err(ClusterBenchmarkError::NotReady(_))
        ));
        write_partial_capture(
            &path,
            &environment,
            &transition.expect_err("invalid transition").to_string(),
            probes,
            vec![capacity_scenario_report(definition, stages)],
        )
        .unwrap();
        let report: BifrostDiagnosticReport =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(report.attempted_probes[0].completed);
        assert_eq!(
            report.attempted_probes[0].stop_reasons,
            vec![CapacityLimit::InvalidEvidence]
        );
        assert_eq!(
            report.scenarios[0].reports[0].capacity_stages[0].stop_reasons,
            vec![CapacityLimit::InvalidEvidence]
        );
    }

    /// Build one completed production-shaped stage input for classification tests.
    fn completed_stage_fixture(backpressure: u64, complete_evidence: bool) -> CapacityStageRun {
        let metric = |family: &str| {
            let histogram = family.ends_with("_seconds");
            let labels = match family {
                "bifrost_gate_requests_total" => BTreeMap::from([
                    ("operation".to_owned(), "write".to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                ]),
                "bifrost_gate_rows_total" => {
                    BTreeMap::from([("status".to_owned(), "accepted".to_owned())])
                }
                "bifrost_gate_query_streams_total" => {
                    BTreeMap::from([("outcome".to_owned(), "success".to_owned())])
                }
                "bifrost_scribe_seal_rows_total" => {
                    BTreeMap::from([("stage".to_owned(), "file_list_transaction".to_owned())])
                }
                "bifrost_scribe_rows_total" => {
                    BTreeMap::from([("status".to_owned(), "accepted".to_owned())])
                }
                "bifrost_oracle_stream_rows_total" => {
                    BTreeMap::from([("outcome".to_owned(), "success".to_owned())])
                }
                "vala_postgres_pool_acquire_seconds" => BTreeMap::from([
                    ("le".to_owned(), "0.001".to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                    ("pool".to_owned(), "runtime".to_owned()),
                ]),
                "wyrd_storage_operation_duration_seconds" => BTreeMap::from([
                    ("backend".to_owned(), "local".to_owned()),
                    ("le".to_owned(), "0.001".to_owned()),
                    ("operation".to_owned(), "get".to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                ]),
                "bifrost_scribe_wal_fsync_seconds" => BTreeMap::from([
                    ("le".to_owned(), "0.001".to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                ]),
                "bifrost_gate_request_duration_seconds" => BTreeMap::from([
                    ("le".to_owned(), "0.001".to_owned()),
                    ("operation".to_owned(), "write".to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                ]),
                "bifrost_gate_query_stream_duration_seconds" => BTreeMap::from([
                    ("le".to_owned(), "0.001".to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                ]),
                "vala_postgres_pool_acquire_total" => {
                    BTreeMap::from([("pool".to_owned(), "runtime".to_owned())])
                }
                "wyrd_storage_bytes_total" => BTreeMap::from([
                    ("backend".to_owned(), "local".to_owned()),
                    ("direction".to_owned(), "read".to_owned()),
                    ("operation".to_owned(), "get".to_owned()),
                ]),
                _ => BTreeMap::new(),
            };
            crate::bifrost::telemetry::BifrostMetricSample {
                family: family.to_owned(),
                labels,
                value: 1.0,
                kind: if histogram {
                    crate::bifrost::telemetry::BifrostMetricKind::HistogramBucket
                } else {
                    crate::bifrost::telemetry::BifrostMetricKind::Counter
                },
            }
        };
        let metrics = [
            "bifrost_gate_requests_total",
            "bifrost_gate_rows_total",
            "bifrost_gate_query_streams_total",
            "bifrost_gate_request_duration_seconds",
            "bifrost_gate_query_stream_duration_seconds",
            "bifrost_scribe_rows_total",
            "bifrost_scribe_seal_rows_total",
            "bifrost_scribe_wal_append_bytes_total",
            "bifrost_forge_complete_gauge_publications_total",
            "bifrost_oracle_stream_rows_total",
            "vala_postgres_pool_acquire_seconds",
            "vala_postgres_pool_acquire_total",
            "wyrd_storage_operation_duration_seconds",
            "wyrd_storage_bytes_total",
            "bifrost_scribe_wal_fsync_seconds",
        ]
        .into_iter()
        .map(metric)
        .collect::<Vec<_>>();
        let mut metrics = metrics;
        metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
            family: "bifrost_gate_requests_total".to_owned(),
            labels: BTreeMap::from([
                ("operation".to_owned(), "query".to_owned()),
                ("outcome".to_owned(), "success".to_owned()),
            ]),
            value: 1.0,
            kind: crate::bifrost::telemetry::BifrostMetricKind::Counter,
        });
        for (operation, outcome) in [
            ("query", "rejected"),
            ("query", "failed"),
            ("query", "cancelled"),
            ("write", "cancelled"),
        ] {
            metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
                family: "bifrost_gate_requests_total".to_owned(),
                labels: BTreeMap::from([
                    ("operation".to_owned(), operation.to_owned()),
                    ("outcome".to_owned(), outcome.to_owned()),
                ]),
                value: 0.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::Counter,
            });
        }
        metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
            family: "bifrost_gate_query_streams_total".to_owned(),
            labels: BTreeMap::from([("outcome".to_owned(), "cancelled".to_owned())]),
            value: 0.0,
            kind: crate::bifrost::telemetry::BifrostMetricKind::Counter,
        });
        for (scope, reason) in [
            ("cluster", "pending_limit"),
            ("cluster", "lease_timeout"),
            ("cluster", "lease_capacity"),
            ("class", "lease_capacity"),
            ("tenant", "lease_capacity"),
            ("cluster", "local_slots"),
        ] {
            metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
                family: "bifrost_oracle_admission_rejections_total".to_owned(),
                labels: BTreeMap::from([
                    ("scope".to_owned(), scope.to_owned()),
                    ("reason".to_owned(), reason.to_owned()),
                    ("query_class".to_owned(), "interactive".to_owned()),
                ]),
                value: 0.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::Counter,
            });
        }
        for (query_class, reason, value) in [
            ("interactive", "estimated_scan", 1.0),
            ("analytical", "predicted_scan", 0.0),
            ("analytical", "global_operator", 0.0),
        ] {
            metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
                family: "bifrost_oracle_classification_total".to_owned(),
                labels: BTreeMap::from([
                    ("query_class".to_owned(), query_class.to_owned()),
                    ("reason".to_owned(), reason.to_owned()),
                ]),
                value,
                kind: crate::bifrost::telemetry::BifrostMetricKind::Counter,
            });
        }
        for (family, labels) in [
            ("bifrost_gate_frame_bytes_total", BTreeMap::new()),
            (
                "bifrost_forge_rewrite_input_files_total",
                BTreeMap::from([("source".to_owned(), "staging".to_owned())]),
            ),
            (
                "bifrost_forge_rewrite_input_bytes_total",
                BTreeMap::from([("source".to_owned(), "staging".to_owned())]),
            ),
            (
                "bifrost_forge_rewrite_output_files_total",
                BTreeMap::from([("source".to_owned(), "staging".to_owned())]),
            ),
            (
                "bifrost_forge_rewrite_output_bytes_total",
                BTreeMap::from([("source".to_owned(), "staging".to_owned())]),
            ),
            (
                "bifrost_oracle_source_rows_total",
                BTreeMap::from([("source".to_owned(), "iceberg".to_owned())]),
            ),
            (
                "bifrost_oracle_stream_bytes_total",
                BTreeMap::from([("outcome".to_owned(), "success".to_owned())]),
            ),
        ] {
            metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
                family: family.to_owned(),
                labels,
                value: 1.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::Counter,
            });
        }
        for (family, labels) in [
            (
                "vala_postgres_pool_acquire_seconds",
                BTreeMap::from([
                    ("outcome".to_owned(), "success".to_owned()),
                    ("pool".to_owned(), "runtime".to_owned()),
                ]),
            ),
            (
                "wyrd_storage_operation_duration_seconds",
                BTreeMap::from([
                    ("backend".to_owned(), "local".to_owned()),
                    ("operation".to_owned(), "get".to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                ]),
            ),
            (
                "bifrost_scribe_wal_fsync_seconds",
                BTreeMap::from([("outcome".to_owned(), "success".to_owned())]),
            ),
            (
                "bifrost_gate_request_duration_seconds",
                BTreeMap::from([
                    ("operation".to_owned(), "write".to_owned()),
                    ("outcome".to_owned(), "success".to_owned()),
                ]),
            ),
            (
                "bifrost_gate_query_stream_duration_seconds",
                BTreeMap::from([("outcome".to_owned(), "success".to_owned())]),
            ),
        ] {
            let mut infinity = labels.clone();
            infinity.insert("le".to_owned(), "+Inf".to_owned());
            metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
                family: family.to_owned(),
                labels: infinity,
                value: 1.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::HistogramBucket,
            });
            metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
                family: family.to_owned(),
                labels: labels.clone(),
                value: 1.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::HistogramCount,
            });
            metrics.push(crate::bifrost::telemetry::BifrostMetricSample {
                family: family.to_owned(),
                labels,
                value: 0.001,
                kind: crate::bifrost::telemetry::BifrostMetricKind::HistogramSum,
            });
        }
        let gauge_maxima = vec![
            crate::bifrost::telemetry::BifrostMetricSample {
                family: "bifrost_forge_oldest_backlog_seconds".to_owned(),
                labels: BTreeMap::new(),
                value: 0.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::Gauge,
            },
            crate::bifrost::telemetry::BifrostMetricSample {
                family: "bifrost_oracle_slots_in_use".to_owned(),
                labels: BTreeMap::from([
                    ("query_class".to_owned(), "analytical".to_owned()),
                    ("role".to_owned(), "leader".to_owned()),
                ]),
                value: 4.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::Gauge,
            },
        ];
        let mut gauge_final = vec![
            crate::bifrost::telemetry::BifrostMetricSample {
                family: "bifrost_oracle_slots_total".to_owned(),
                labels: BTreeMap::from([("role".to_owned(), "leader".to_owned())]),
                value: 8.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::Gauge,
            },
            crate::bifrost::telemetry::BifrostMetricSample {
                family: "bifrost_gate_active_streams".to_owned(),
                labels: BTreeMap::from([("operation".to_owned(), "query".to_owned())]),
                value: 0.0,
                kind: crate::bifrost::telemetry::BifrostMetricKind::Gauge,
            },
        ];
        gauge_final.extend(
            [
                ("bifrost_scribe_ingress_active", BTreeMap::new()),
                (
                    "bifrost_scribe_lane_active",
                    BTreeMap::from([("lane".to_owned(), "ingress".to_owned())]),
                ),
                (
                    "bifrost_scribe_lane_queued",
                    BTreeMap::from([("lane".to_owned(), "ingress".to_owned())]),
                ),
                ("bifrost_scribe_persistence_queue_depth", BTreeMap::new()),
                (
                    "bifrost_oracle_in_flight",
                    BTreeMap::from([
                        ("query_class".to_owned(), "analytical".to_owned()),
                        ("visibility".to_owned(), "published_only".to_owned()),
                    ]),
                ),
                (
                    "bifrost_oracle_slots_in_use",
                    BTreeMap::from([
                        ("query_class".to_owned(), "analytical".to_owned()),
                        ("role".to_owned(), "leader".to_owned()),
                    ]),
                ),
                (
                    "wyrd_storage_operations_active",
                    BTreeMap::from([
                        ("backend".to_owned(), "local".to_owned()),
                        ("operation".to_owned(), "get".to_owned()),
                    ]),
                ),
            ]
            .into_iter()
            .map(
                |(family, labels)| crate::bifrost::telemetry::BifrostMetricSample {
                    family: family.to_owned(),
                    labels,
                    value: 0.0,
                    kind: crate::bifrost::telemetry::BifrostMetricKind::Gauge,
                },
            ),
        );
        let spans = if complete_evidence {
            [
                "bifrost.scribe.wal.append",
                "bifrost.forge.catalog.commit",
                "bifrost.oracle.source",
                "bifrost.gate.query.stream",
            ]
            .into_iter()
            .map(|name| wyrd_telemetry::CapturedSpan {
                trace_id: format!("trace-{name}"),
                name: name.to_owned(),
                attributes: BTreeMap::new(),
                duration_nanos: 1_000,
                status: wyrd_telemetry::CapturedSpanStatus::Unset,
            })
            .collect()
        } else {
            Vec::new()
        };
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
            telemetry: crate::bifrost::telemetry::BifrostTelemetryDelta {
                metrics,
                gauge_maxima,
                gauge_final,
                spans,
                interval_seconds: 10.0,
                process: test_process_window(),
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
        assert!(
            healthy_stage.passed,
            "healthy fixture failed: {:?} {:?}",
            healthy_stage.stop_reasons, healthy_stage.telemetry.invalid
        );
        machine
            .record(healthy, capacity_stage_outcome(&healthy_stage))
            .unwrap();

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
        machine
            .record(failed, capacity_stage_outcome(&backpressured))
            .unwrap();

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
                .contains(&CapacityLimit::InvalidEvidence)
        );
        assert!(matches!(
            machine.record(confirmation, capacity_stage_outcome(&incomplete_evidence)),
            Err(ClusterBenchmarkError::NotReady(_))
        ));

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

    /// Proves all benchmark report adapters are pure copies of canonical evidence.
    #[test]
    fn benchmark_adapters_copy_canonical_fields() {
        let evidence = canonical_adapter_evidence();
        let pillars = adapt_pillars(&evidence);
        let dependencies = adapt_dependencies(&evidence);
        let resources = adapt_process(&evidence);
        assert_eq!(pillars.gate_accepted, 11);
        assert_eq!(pillars.scribe_wal_bytes, 12);
        assert_eq!(pillars.forge_publications, 13);
        assert_eq!(pillars.oracle_decoded_rows, 14);
        assert_eq!(dependencies.postgres_pool_wait_us, 21);
        assert_eq!(dependencies.postgres_transactions, 22);
        assert_eq!(dependencies.storage_bytes, 23);
        assert_eq!(dependencies.storage_p99_us, 24);
        assert_eq!(dependencies.wal_fsync_p99_us, 25);
        assert_eq!(resources[0].process_id.0, "pid-test");
        assert_eq!(resources[0].epoch, 3);
        assert_eq!(resources[0].hosted_logical_nodes, vec!["server-0"]);
        assert_eq!(resources[0].roles.len(), 4);
        assert_eq!(resources[0].runtime_queue_peak, 31);
        assert!(evidence.cleanup.is_clean());
    }

    /// Build complete dependency-neutral evidence for pure adapter tests.
    fn canonical_adapter_evidence() -> ClusterTelemetryEvidence {
        ClusterTelemetryEvidence {
            phase: crate::bifrost::telemetry::ClusterPhaseTelemetryEvidence {
                gate_rows: 0,
                gate_bytes: 0,
                gate_success: 0,
                gate_rejected: 0,
                gate_failed: 0,
                gate_cancelled: 0,
                gate_query_request_success: 0,
                gate_write_request_cancelled: 0,
                gate_query_stream_cancelled: 0,
                gate_query_stream_terminals: 0,
                gate_active_streams: 0,
                scribe_rows: 0,
                oracle_source_rows: 0,
                oracle_stream_rows: 0,
                oracle_stream_bytes: 0,
                forge_input_files: 0,
                forge_input_bytes: 0,
                forge_output_files: 0,
                forge_output_bytes: 0,
            },
            pillars: crate::bifrost::telemetry::ClusterPillarTelemetryEvidence {
                gate_accepted: 11,
                scribe_wal_bytes: 12,
                forge_publications: 13,
                oracle_decoded_rows: 14,
                oracle_analytical_slots_peak: 4,
                oracle_slots_total: 8,
                oracle_interactive_scan_classifications: 11,
                oracle_predicted_scan_classifications: 0,
                oracle_global_operator_classifications: 0,
                oracle_pending_limit_rejections: 0,
                oracle_lease_timeout_rejections: 0,
                oracle_cluster_lease_rejections: 0,
                oracle_class_lease_rejections: 0,
                oracle_tenant_lease_rejections: 0,
                oracle_local_slot_rejections: 0,
            },
            dependencies: crate::bifrost::telemetry::ClusterDependencyTelemetryEvidence {
                postgres_pool_wait_us: Some(21),
                postgres_transactions: Some(22),
                storage_bytes: Some(23),
                storage_p99_us: Some(24),
                wal_fsync_p99_us: Some(25),
            },
            process: crate::bifrost::telemetry::ClusterProcessTelemetryEvidence {
                identity: "pid-test".to_owned(),
                epoch: 3,
                hosted_logical_nodes: vec!["server-0".to_owned()],
                roles: vec![
                    ClusterRuntimeRole::Gate,
                    ClusterRuntimeRole::Scribe,
                    ClusterRuntimeRole::Forge,
                    ClusterRuntimeRole::Oracle,
                ],
                cpu_seconds: 1.0,
                peak_rss_bytes: 2,
                current_rss_bytes: 1,
                runtime_busy_seconds: 1.0,
                runtime_queue_peak: 31,
            },
            traces: vec![crate::bifrost::telemetry::ClusterTraceTelemetryEvidence {
                operation: ClusterTraceOperation::QueryTotal,
                representative_trace_ids: vec!["trace-1".to_owned()],
                samples: 7,
                p95_us: 95,
                p99_us: 99,
            }],
            reconciliation: crate::bifrost::telemetry::ClusterReconciliationTelemetryEvidence {
                sealed_rows: 0,
                successful_queries: 0,
            },
            cleanup: crate::bifrost::telemetry::ClusterCleanupTelemetryEvidence {
                gate_active: Some(0),
                scribe_ingress: Some(0),
                scribe_lane_active: Some(0),
                scribe_lane_queued: Some(0),
                scribe_persistence_queue: Some(0),
                oracle_in_flight: Some(0),
                oracle_slots: Some(0),
                storage_active: Some(0),
            },
        }
    }

    /// Build deterministic checked process evidence for projection-only tests.
    fn test_process_window() -> crate::bifrost::telemetry::ProcessWindow {
        crate::bifrost::telemetry::ProcessWindow {
            identity: "pid-test".to_owned(),
            epoch: 0,
            cpu_seconds: 1.0,
            current_rss_bytes: 1,
            peak_rss_bytes: 2,
            tokio_busy_seconds: 1.0,
            queue_peak: 2,
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
        let reasons = [CapacityLimit::Backpressure, CapacityLimit::DependencySlo];
        complete_current_probe(&mut probes, &reasons);
        begin_probe(&mut probes, "scenario-a".to_owned(), 16);
        assert!(probes[0].completed);
        assert_eq!(probes[0].stop_reasons, reasons);
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
                warmup_seconds: 5,
                measured_seconds: 20,
                trials: 3,
                minimum_samples: 200,
                max_in_flight: DEFAULT_MAX_IN_FLIGHT,
                seed: 0xB1_F057,
            },
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

    /// Proves ancillary correctness failures preserve their first typed SDK source.
    #[test]
    fn ancillary_query_is_single_attempt_with_typed_context() {
        let mut attempts = 0_usize;
        let result: Result<(), ValaSdkError> = {
            attempts += 1;
            Err(ValaSdkError::Transport(
                wyrd_spec::vala::error::BifrostError::QueryAdmissionRejected.into(),
            ))
        };
        let error = result
            .map_err(|source| AncillaryQueryError {
                phase: "final published identity",
                tenant_index: 2,
                table: REFERENCE_TABLE.to_owned(),
                source,
            })
            .expect_err("injected admission rejection fails immediately");
        assert_eq!(attempts, 1);
        let source = std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<ValaSdkError>())
            .expect("context retains the typed SDK source");
        assert_eq!(source.code(), "WYRD_VALA_429_QUERY_ADMISSION_REJECTED");
        assert_eq!(source.detail(), "query admission rejected");
        assert_eq!(source.status(), 429);
        let display = error.to_string();
        assert!(display.contains("final published identity"));
        assert!(display.contains("tenant index 2"));
        assert!(display.contains(REFERENCE_TABLE));

        let source_text = include_str!("bench_cluster.rs");
        assert!(!source_text.contains(&["async fn ancillary", "_query"].concat()));
        assert!(!source_text.contains(&["ancillary strict query", " exhausted"].concat()));
    }

    /// Proves bounded ranges partition the full domain with one unbounded tail.
    #[test]
    fn identity_ranges_are_disjoint_complete_and_bounded() {
        let expected = (0_u64..2_049).map(|value| value * 2 + 10).collect();
        let ranges = identity_reconciliation_ranges(&expected);

        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].lower_exclusive, None);
        assert!(ranges[0].upper_inclusive.is_some());
        assert_eq!(ranges[1].lower_exclusive, ranges[0].upper_inclusive);
        assert!(ranges[1].upper_inclusive > ranges[0].upper_inclusive);
        assert_eq!(ranges[2].lower_exclusive, ranges[1].upper_inclusive);
        assert_eq!(ranges[2].upper_inclusive, None);
        assert_eq!(
            ranges
                .iter()
                .filter(|range| range.upper_inclusive.is_none())
                .count(),
            1
        );
        assert!(
            ranges
                .iter()
                .all(|range| { range.expected_rows <= IDENTITY_RECONCILIATION_ROWS_PER_QUERY })
        );
        for identity in [0, 9, 10, 2_057, 4_107, u64::MAX] {
            assert_eq!(
                ranges
                    .iter()
                    .filter(|range| range.contains(identity))
                    .count(),
                1,
                "identity {identity} belongs to exactly one range"
            );
        }
    }

    /// Proves empty and single-chunk ledgers each retain one full-domain query.
    #[test]
    fn identity_ranges_cover_empty_and_single_chunk_ledgers() {
        for expected in [BTreeSet::new(), BTreeSet::from([10, 20, 30])] {
            let ranges = identity_reconciliation_ranges(&expected);
            assert_eq!(ranges.len(), 1);
            assert_eq!(ranges[0].lower_exclusive, None);
            assert_eq!(ranges[0].upper_inclusive, None);
            assert_eq!(ranges[0].expected_rows, expected.len());
            assert!(ranges[0].contains(0));
            assert!(ranges[0].contains(u64::MAX));
        }
    }

    /// Proves completed ranges reject predicate drift, cross-range duplicates,
    /// and incomplete terminals without merging partial observations.
    #[test]
    fn completed_identity_range_merge_is_fail_closed() {
        let prefix = IdentityReconciliationRange {
            lower_exclusive: None,
            upper_inclusive: Some(20),
            expected_rows: 2,
        };
        let suffix = IdentityReconciliationRange {
            lower_exclusive: Some(20),
            upper_inclusive: None,
            expected_rows: 1,
        };
        let mut published = BTreeSet::new();
        merge_completed_identity_range(
            prefix,
            Some(QueryTerminalOutcome::Success),
            BTreeSet::from([10, 20]),
            &mut published,
        )
        .expect("valid prefix merges");
        assert!(
            merge_completed_identity_range(
                suffix,
                Some(QueryTerminalOutcome::Success),
                BTreeSet::from([20]),
                &mut published,
            )
            .is_err()
        );
        assert!(
            merge_completed_identity_range(
                prefix,
                Some(QueryTerminalOutcome::Success),
                BTreeSet::from([21]),
                &mut published,
            )
            .is_err()
        );
        for terminal in [None, Some(QueryTerminalOutcome::Failed)] {
            let mut partial = BTreeSet::new();
            assert!(
                merge_completed_identity_range(
                    prefix,
                    terminal,
                    BTreeSet::from([10]),
                    &mut partial,
                )
                .is_err()
            );
            assert!(partial.is_empty(), "partial failed range is discarded");
        }
    }

    /// Proves exact comparison rejects missing rows and unexpected identities
    /// in the prefix, acknowledged gaps, and suffix.
    #[test]
    fn exact_range_reconciliation_rejects_missing_and_all_extra_regions() {
        let expected = vec![BTreeSet::from([10, 20, 30])];
        assert!(exact_identity_ledger_matches(&expected, &expected));
        for observed in [
            BTreeSet::from([10, 20]),
            BTreeSet::from([9, 10, 20, 30]),
            BTreeSet::from([10, 15, 20, 30]),
            BTreeSet::from([10, 20, 30, 31]),
        ] {
            assert!(!exact_identity_ledger_matches(&expected, &[observed]));
        }
    }

    /// Proves row collection rejects foreign tenant identities and duplicates.
    #[test]
    fn range_row_collection_rejects_foreign_and_duplicate_identities() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("row_id", DataType::Int64, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
                false,
            ),
        ]));
        let timestamps = arrow::array::TimestampMicrosecondArray::from(vec![1, 2]);
        for row_ids in [vec![10_i64, 10], vec![10_i64, 1_001]] {
            let batch = RecordBatch::try_new(
                Arc::clone(&schema),
                vec![
                    Arc::new(Int64Array::from(row_ids)),
                    Arc::new(timestamps.clone()),
                ],
            )
            .expect("fixture batch");
            let mut identities = BTreeSet::new();
            assert!(collect_query_identities(&batch, 0, 1_000, &mut identities).is_err());
        }
    }

    /// Proves final identity reads retain their projection and one attempt per range.
    #[test]
    fn final_identity_query_is_one_unsorted_attempt_per_range() {
        let source = include_str!("bench_cluster.rs");
        let start = source
            .find("struct IdentityReconciliationRange")
            .expect("identity range owner exists");
        let end = source[start..]
            .find("fn reconcile_production(")
            .map(|offset| start + offset)
            .expect("reconciliation follows final identity collection");
        let function = &source[start..end];
        let projection = [
            "SELECT row_id, wyrd_event_time FROM vala.bifrost.",
            "{table}",
        ]
        .concat();
        assert!(function.contains(&projection));
        assert!(!function.contains(&["ORDER", " BY"].concat()));
        assert!(!function.contains(&["LIM", "IT"].concat()));
        assert_eq!(function.matches(".query(&request)").count(), 1);
        assert!(function.contains("for (tenant_index, client)"));
        assert!(function.contains("while let Some(batch)"));
        assert!(function.contains("collect_query_identities"));
        assert!(function.contains("QueryTerminalOutcome::Success"));
        assert!(function.contains("identity_reconciliation_ranges"));
        assert!(function.contains("deadline_ms: Some(5_000)"));
        assert!(!function.contains("sleep"));
        assert!(!function.contains("retry"));
    }

    /// Proves capacity stages rely on their owned lifecycle bounds.
    #[test]
    fn capacity_stage_lifecycle_has_no_outer_cancellation() {
        let source = include_str!("bench_cluster.rs");
        let capacity_start = source
            .find("async fn run_capacity_selected(")
            .expect("capacity runner exists");
        let capacity_end = source[capacity_start..]
            .find("fn capacity_scenario_report(")
            .map(|offset| capacity_start + offset)
            .expect("capacity runner has a stable end");
        let capacity = &source[capacity_start..capacity_end];
        let result_start = capacity
            .find("let result = session")
            .expect("capacity stage result begins");
        let result_end = capacity[result_start..]
            .find("let outcome =")
            .map(|offset| result_start + offset)
            .expect("capacity stage assembly ends");
        let stage_result = &capacity[result_start..result_end];
        assert_eq!(stage_result.matches(".run_stage(").count(), 1);
        assert!(stage_result.contains(".await?"));
        assert!(!stage_result.contains("timeout"));
        // The sole runner timeout remains the separately owned initial warmup bound.
        assert_eq!(capacity.matches("tokio::time::timeout").count(), 1);
    }

    /// Proves live and qualification correctness reads occur only after their
    /// sampled telemetry and audit checkpoints have completed.
    #[test]
    fn correctness_reads_follow_sampled_windows_and_audit_checkpoints() {
        let source = include_str!("bench_cluster.rs");
        let live_start = source.find("async fn run_live_trial(").expect("live trial");
        let live_end = source[live_start..]
            .find("fn preload_identity_ledger(")
            .map(|offset| live_start + offset)
            .expect("live trial end");
        let live = &source[live_start..live_end];
        assert!(
            live.find("let audit_after = audit_rows").unwrap()
                < live.find(".await?;\n    extend_identity_ledger").unwrap()
        );
        assert!(
            live.find(".await?;\n    extend_identity_ledger").unwrap()
                < live.find("let published_after_identities").unwrap()
        );

        let qualification_start = source
            .find("async fn run_qualification_pair(")
            .expect("qualification pair");
        let qualification_end = source[qualification_start..]
            .find("pub async fn shutdown")
            .map(|offset| qualification_start + offset)
            .expect("qualification end");
        let qualification = &source[qualification_start..qualification_end];
        assert!(
            qualification.find("let audit_after = audit_rows").unwrap()
                < qualification.find("let expected_rows_by_tenant").unwrap()
        );
        assert!(
            qualification.find("let expected_rows_by_tenant").unwrap()
                < qualification
                    .find("let published = final_published_identities")
                    .unwrap()
        );
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
