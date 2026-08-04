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
    BenchmarkEnvironment, BifrostReferenceProfile, BifrostSloEnvelope, CALIBRATION_RATE_CAP,
    CLUSTER_REPORT_VERSION, CLUSTER_WORKLOAD_VERSION, ClientTrialMetrics, ClusterBenchmarkError,
    ClusterBenchmarkScenario, ClusterBenchmarkTrial, ClusterScenarioReport, ClusterTopology,
    ClusterWorkloadIdentity, EvidenceStatus, FIRST_PROBE_RATE, KneeProvenance,
    ProductionTelemetryEvidence, ReviewedScenarioProfile, TrafficMix, TrialDistribution,
    derive_trial_median, extract_linux_cpu_identity, extract_macos_cpu_identity, jain_fairness,
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
use crate::load::{BifrostClusterLoad, ClusterLoadProfile};

/// Stable logical table used by the controlled workload.
const REFERENCE_TABLE: &str = "cluster_reference_events";
/// Rows preloaded through Gate for every tenant before measured traffic.
const PRELOAD_ROWS: u64 = 8_192;
/// Rows in one public durable write request.
const ROWS_PER_WRITE: u32 = 64;
/// Default profile-driven concurrent public-operation cap.
const DEFAULT_MAX_IN_FLIGHT: usize = 4_096;
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
    run_reference_selected(command.scenario.as_deref(), command.matrix, false).await
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
    run_reference_selected(command.scenario.as_deref(), command.matrix, true).await
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
                median,
                capacity_stages: Vec::new(),
            });
        }
    }
    let profile = BifrostReferenceProfile {
        schema_version: CLUSTER_REPORT_VERSION.to_owned(),
        environment,
        scenarios: reviewed_scenario_profiles(reports),
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
fn reviewed_scenario_profiles(reports: Vec<ClusterScenarioReport>) -> Vec<ReviewedScenarioProfile> {
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
            Some(ReviewedScenarioProfile {
                scenario_id: scenario_id.clone(),
                workload: ClusterWorkloadIdentity {
                    scenario_id,
                    topology: first.scenario.topology,
                    tenants: first.scenario.tenants,
                    traffic: first.scenario.traffic,
                    rows_per_batch: first.scenario.rows_per_batch,
                    query_row_limit: first.scenario.query_row_limit,
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
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let report = serde_json::json!({
        "schema_version": CLUSTER_REPORT_VERSION,
        "promotable": false,
        "status": "not_ready",
        "error": error,
        "environment": environment,
        "attempted_probes": attempted_rates,
        "stop_reasons": ["failed_first_probe"],
        "scenarios": [],
    });
    std::fs::write(
        report_path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    Ok(())
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
    let report = serde_json::json!({
        "schema_version": CLUSTER_REPORT_VERSION,
        "promotable": false,
        "status": "unsupported",
        "error": error,
        "environment": serde_json::Value::Null,
        "attempted_probes": [],
        "stop_reasons": ["unsupported_environment"],
        "scenarios": [],
    });
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
    }
    .run()
    .await?;
    flush_tenant_writers(cluster, &tenants).await?;
    await_forge_convergence(cluster, &tenants).await?;
    let published_before = final_published_rows(cluster, &tenants, &clients).await?;
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
    }
    .run()
    .await?;
    flush_tenant_writers(cluster, &tenants).await?;
    await_forge_convergence(cluster, &tenants).await?;
    let audit_after = audit_rows(cluster, &tenants).await?;
    let telemetry = cluster.telemetry().delta_since(&checkpoint)?;
    let published_rows = final_published_rows(cluster, &tenants, &clients).await?;
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
struct WindowResult {
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

impl WindowResult {
    /// Convert the complete measured ledger into report distributions and rates.
    fn metrics(&self) -> ClientTrialMetrics {
        ClientTrialMetrics {
            planned_operations: self.submitted,
            attempted_operations: self.submitted.saturating_sub(self.missed_operations),
            accepted_operations: self.accepted,
            backpressure_operations: self.backpressure,
            retry_operations: self.retries,
            in_flight_cap_exhaustions: self.in_flight_cap_exhaustions,
            max_in_flight: u64::try_from(DEFAULT_MAX_IN_FLIGHT).unwrap_or(u64::MAX),
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
        let operations = self.operations(start);
        let flushes = self.flushes(start);
        let (mut result, flush_result) = tokio::try_join!(operations, flushes)?;
        result.measured_seconds = self.duration.as_secs();
        result.flush_us = flush_result.0;
        result.missed_flushes = flush_result.1;
        result.telemetry_decoded_rows =
            result.telemetry_decoded_rows.saturating_add(flush_result.2);
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
    ) -> Result<WindowResult, Box<dyn std::error::Error + Send + Sync>> {
        let interval = Duration::from_secs_f64(1.0 / self.rate as f64);
        let operation_count = self.rate.saturating_mul(self.duration.as_secs());
        let mut tasks = tokio::task::JoinSet::new();
        let mut result = WindowResult {
            tenant_write_rows: vec![0; self.tenants.len()],
            tenant_queries: vec![0; self.tenants.len()],
            ..WindowResult::default()
        };
        for ordinal in 0..operation_count {
            while let Some(joined) = tasks.try_join_next() {
                apply_operation(&mut result, joined??);
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
            let write = is_write_operation(ordinal, self.definition.write_percent);
            let phase_ordinal = ordinal + if self.measured { 2_000_000 } else { 1_000_000 };
            tasks.spawn(async move {
                if write {
                    scheduled_write(client, tenant_index, phase_ordinal, planned).await
                } else {
                    scheduled_query(client, tenant_index, ordinal, planned).await
                }
            });
        }
        while let Some(joined) = tasks.join_next().await {
            apply_operation(&mut result, joined??);
        }
        Ok(result)
    }
}

/// Execute one planned durable write and preserve stable pressure classification.
async fn scheduled_write(
    client: ReferenceClient,
    tenant_index: usize,
    ordinal: u64,
    planned: tokio::time::Instant,
) -> Result<OperationResult, Box<dyn std::error::Error + Send + Sync>> {
    let payload = reference_payload(
        tenant_row_base(tenant_index) + PRELOAD_ROWS + ordinal * 64,
        ROWS_PER_WRITE,
    )?;
    let frame = BifrostFrame {
        table: format!("vala.bifrost.{REFERENCE_TABLE}"),
        batch_id: deterministic_batch_id(tenant_index, 1_000_000 + ordinal),
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
    tenant_index: usize,
    ordinal: u64,
    planned: tokio::time::Instant,
) -> Result<OperationResult, Box<dyn std::error::Error + Send + Sync>> {
    let lower = tenant_row_base(tenant_index)
        .saturating_add(ordinal.saturating_mul(61) % (PRELOAD_ROWS - 64));
    let upper = lower + 63;
    let request = BifrostQueryRequest {
        sql: format!(
            "SELECT row_id, wyrd_event_time FROM vala.bifrost.{REFERENCE_TABLE} WHERE row_id BETWEEN {lower} AND {upper} ORDER BY row_id LIMIT 64"
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
        } => {
            result.accepted = result.accepted.saturating_add(1);
            result.retries = result.retries.saturating_add(retries);
            result.write_us.push(latency_us);
            result.tenant_write_rows[tenant] =
                result.tenant_write_rows[tenant].saturating_add(rows);
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
    /// Returns the first flush, strict-query, schema, or terminal-outcome error.
    async fn flushes(
        &self,
        start: tokio::time::Instant,
    ) -> Result<(Vec<u64>, u64, u64), Box<dyn std::error::Error + Send + Sync>> {
        if !self.measured {
            return Ok((Vec::new(), 0, 0));
        }
        let mut samples = Vec::with_capacity(10);
        let mut missed = 0_u64;
        let mut decoded_rows = 0_u64;
        for flush_index in 0..(self.duration.as_secs() / 2) {
            let planned = start + Duration::from_secs((flush_index + 1) * 2);
            tokio::time::sleep_until(planned).await;
            if tokio::time::Instant::now() > planned + Duration::from_secs(2) {
                missed = missed.saturating_add(1);
                continue;
            }
            let tenant_index = flush_index as usize % self.tenants.len();
            flush_one_tenant(self.cluster, self.tenants, tenant_index).await?;
            let request = BifrostQueryRequest {
                sql: format!(
                    "SELECT row_id, wyrd_event_time FROM vala.bifrost.{REFERENCE_TABLE} WHERE row_id BETWEEN {} AND {} ORDER BY row_id LIMIT 64",
                    tenant_row_base(tenant_index),
                    tenant_row_base(tenant_index) + 63,
                ),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            };
            let mut stream = ancillary_query(&self.clients[tenant_index].query, &request).await?;
            let mut identities = BTreeSet::new();
            while let Some(batch) = stream.next_batch().await? {
                collect_query_identities(
                    &batch,
                    tenant_row_base(tenant_index),
                    tenant_row_base(tenant_index) + 63,
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
        Ok((samples, missed, decoded_rows))
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
    let server = cluster.server(0).ok_or("reference cluster has no Server")?;
    let catalog = server
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("reference cluster has no Redux catalog")?;
    let fields = vec![Field::new("row_id", DataType::Int64, false)];
    for tenant in tenants {
        catalog
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, REFERENCE_TABLE),
                user_fields: fields.clone(),
                tenant: *tenant,
                audit: None,
            })
            .await?;
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
    _cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
    clients: &[ReferenceClient],
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let mut total = 0_u64;
    for (tenant_index, client) in clients.iter().enumerate().take(tenants.len()) {
        let request = BifrostQueryRequest {
            sql: format!(
                "SELECT row_id, wyrd_event_time FROM vala.bifrost.{REFERENCE_TABLE} ORDER BY row_id"
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
            total = total.saturating_add(batch.num_rows() as u64);
        }
        if stream
            .terminal()
            .is_none_or(|terminal| terminal.outcome != QueryTerminalOutcome::Success)
        {
            return Err("final reference query omitted a successful terminal".into());
        }
    }
    Ok(total)
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
/// Returns a cluster error when Task 15's real public Gate adapter fails its
/// correctness, telemetry, tenant-isolation, audit, or cleanup assertions.
pub async fn run_smoke() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let task_15_summary = BifrostClusterLoad::start(ClusterLoadProfile::single_tenant_single_pod())
        .await?
        .run()
        .await?;
    let definition = reference_scenario_matrix()[0];
    let (scenario, trial) =
        run_reference_trial(definition, 20, 100, 1, 20, KneeProvenance::Discovered).await?;
    if trial.client.flush_to_visible.samples != 10
        || trial.client.missed_operations != 0
        || trial.client.missed_flushes != 0
        || !trial.production.reconciles()
    {
        return Err(format!(
            "shortened reference smoke produced incomplete benchmark evidence: client={:?}, production={:?}",
            trial.client, trial.production
        )
        .into());
    }
    let path = report_path("cluster-smoke.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({
                "evidence_class": "shortened-non-slo-smoke",
                "task_15_matrix_adapter": task_15_summary,
                "reference_scenario": scenario,
                "reference_trial": trial,
            }))?
        ),
    )?;
    Ok(path)
}

/// Detect the complete controlled environment from host and wrapper evidence.
///
/// # Errors
/// Returns `Unsupported` when CPU, memory, toolchain, lock, container, database,
/// storage, Git, OS, or timestamp identity is absent or unclassifiable.
pub fn detect_reference_environment() -> Result<BenchmarkEnvironment, ClusterBenchmarkError> {
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
        storage_config: "target/bifrost-benchmarks/storage:dedicated-root".to_owned(),
        storage_backend_kind: "local-filesystem".to_owned(),
        storage_root: "target/bifrost-benchmarks/storage".to_owned(),
        storage_device_class: "local".to_owned(),
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
