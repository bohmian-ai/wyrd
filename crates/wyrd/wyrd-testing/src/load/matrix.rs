//! Deterministic public-SDK Bifrost cluster load matrix.

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use arrow::array::{Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::admission::{AdmissionConfig, EventTimeWindow};
use vala_sdk::{
    BifrostFrame, BifrostGrpcTransport, CollectedQueryLimits, CollectedQueryResult, QueryClient,
    ValaSdkError,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};

use crate::Bootstrap;
use crate::bifrost::telemetry::{
    ClusterPhaseTelemetryEvidence, ClusterTelemetryEvidence, ClusterTelemetryExpectation,
    ClusterTelemetryProjection, ClusterTraceOperation, run_sampled_window,
};
use crate::bifrost::{BifrostTelemetryCapture, BifrostTopology, WyrdTestCluster};

/// Stable table name provisioned independently inside every tenant.
const TABLE_NAME: &str = "bifrost_cluster_load";
/// Whole-scenario progress ceiling, not a latency SLO.
const DEFAULT_SCENARIO_DEADLINE: Duration = Duration::from_secs(60);

/// Budget granted to cluster shutdown, independent of the scenario deadline.
///
/// Shutdown must not draw from the workload's remaining time. When the scenario
/// deadline is what expired, no scenario time is left, so a shared budget gives
/// shutdown zero and guarantees it also fails -- dropping the cluster with its
/// listeners live and its temporary volume roots still in use by in-flight
/// background work. The resulting errno cascade hides the one failure that
/// actually mattered. An independent budget keeps the workload timeout and a
/// genuine shutdown hang distinguishable.
const CLUSTER_SHUTDOWN_BUDGET: Duration = Duration::from_secs(30);
/// Exact bindings required to reconcile warmup writes.
const WARMUP_BINDINGS: &[&str] = &[
    "gate.requests.success",
    "gate.bytes",
    "gate.rows",
    "scribe.rows",
];
/// Exact bindings required to reconcile mixed measured traffic.
const MEASURED_BINDINGS: &[&str] = &[
    "gate.requests.success",
    "gate.bytes",
    "gate.rows",
    "scribe.rows",
];
/// Exact bindings required to reconcile final public reads.
const FINAL_BINDINGS: &[&str] = &[
    "gate.requests.query_success",
    "oracle.rows",
    "oracle.stream_bytes",
];
/// Exact bindings required to prove request-versus-stream cancellation.
const CANCELLATION_BINDINGS: &[&str] = &[
    "gate.requests.success",
    "gate.requests.rejected",
    "gate.requests.failed",
    "gate.requests.cancelled",
    "gate.requests.query_success",
    "gate.requests.write_cancelled",
    "gate.query_streams",
    "gate.query_streams.cancelled",
    "gate.active",
];
/// Exact dependencies guaranteed by warmup writes and publication.
const WARMUP_DEPENDENCIES: &[&str] = &[
    "postgres.acquire",
    "postgres.transactions",
    "storage.bytes",
    "wal.fsync",
];
/// Exact dependencies guaranteed by measured mixed traffic.
const MEASURED_DEPENDENCIES: &[&str] = &["postgres.acquire", "postgres.transactions", "wal.fsync"];
/// Exact dependencies guaranteed by publication reconciliation.
const PUBLICATION_DEPENDENCIES: &[&str] =
    &["postgres.acquire", "postgres.transactions", "storage.bytes"];
/// Exact dependencies guaranteed by public reads and cancellation.
const QUERY_DEPENDENCIES: &[&str] = &["postgres.acquire", "postgres.transactions"];
/// Exact cleanup finals required after publication drains.
const PUBLICATION_CLEANUP: &[&str] = &[
    "cleanup.scribe_ingress",
    "cleanup.scribe_lane_active",
    "cleanup.scribe_lane_queued",
    "cleanup.scribe_persistence_queue",
    "cleanup.storage_active",
];
/// Exact cleanup finals required after query and cancellation phases.
const QUERY_CLEANUP: &[&str] = &[
    "gate.active",
    "cleanup.oracle_active",
    "cleanup.storage_active",
];
/// Trace operations required while acknowledging warmup writes and publishing them.
const WARMUP_TRACES: &[ClusterTraceOperation] = &[
    ClusterTraceOperation::DurableWrite,
    ClusterTraceOperation::FlushToVisible,
];
/// Trace operations required by the mixed measured phase.
const MEASURED_TRACES: &[ClusterTraceOperation] = &[
    ClusterTraceOperation::DurableWrite,
    ClusterTraceOperation::QueryTimeToFirstFrame,
    ClusterTraceOperation::QueryTotal,
];
/// Trace operation required by a publication-only phase.
const PUBLICATION_TRACES: &[ClusterTraceOperation] = &[ClusterTraceOperation::FlushToVisible];
/// Trace operations required by a strict public read phase.
const QUERY_TRACES: &[ClusterTraceOperation] = &[
    ClusterTraceOperation::QueryTimeToFirstFrame,
    ClusterTraceOperation::QueryTotal,
];
/// Trace operation required by the cancellation phase.
const CANCELLATION_TRACES: &[ClusterTraceOperation] = &[ClusterTraceOperation::QueryTotal];
/// Immutable operation counts collected for one tenant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TenantLoadResult {
    /// Stable zero-based tenant index used for pressure-cohort assertions.
    pub tenant_index: usize,
    /// Warmup batches acknowledged before the measured barrier.
    pub warmup_acknowledged_batches: u32,
    /// Warmup wire bytes acknowledged before the measured barrier.
    pub warmup_acknowledged_bytes: u64,
    /// Measured batches acknowledged after the measured barrier.
    pub measured_acknowledged_batches: u32,
    /// Number of batches submitted through the public SDK.
    pub submitted_batches: u32,
    /// Number of batches acknowledged by Gate.
    pub acknowledged_batches: u32,
    /// Rows accepted by the client-facing ingest operation.
    pub acknowledged_rows: u64,
    /// Rows observed by strict reads.
    pub queried_rows: u64,
    /// Encoded bytes accepted by Gate.
    pub acknowledged_bytes: u64,
    /// Encoded bytes received by strict reads.
    pub queried_bytes: u64,
    /// Rows from the one final unique read after publication convergence.
    pub final_rows: u64,
    /// Encoded bytes from the one final unique read.
    pub final_bytes: u64,
    /// Number of decoded columns in the final schema.
    pub final_schema_columns: usize,
    /// Number of terminal frames validated by the final read.
    pub final_terminal_count: u8,
    /// Tenant-bound durable read audits observed after final publication.
    pub tenant_audit_rows: u64,
    /// Number of completed strict reads.
    pub completed_reads: u32,
    /// Retryable write attempts observed by this tenant.
    pub retries: u32,
    /// Explicit Scribe admission/backpressure responses.
    pub backpressure: u32,
    /// Measured batches rejected with an explicit stable backpressure response.
    pub rejected_batches: u32,
    /// Number of measured phases in which this tenant made progress.
    pub progressed_phases: u32,
}

/// Aggregated resource state after cancellation and shutdown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ClusterCleanupSnapshot {
    /// Scribe queued append commands.
    pub scribe_queued: u64,
    /// Scribe in-flight append commands.
    pub scribe_inflight: u64,
    /// Persistent Scribe WAL streams retained by the server runtime.
    pub scribe_wal_streams: u64,
    /// Exact Oracle tail fences observed after the cancellation owner released.
    pub oracle_tail_fences: u64,
    /// Forge claims/attempts still active at the cleanup checkpoint.
    pub forge_active_claims: u64,
    /// Forge attempts still owned by a non-terminal task.
    pub forge_active_attempts: u64,
    /// Historical Forge attempt identifiers retained for diagnostics.
    pub forge_historical_attempts: u64,
    /// Forge terminal tasks carrying committed evidence.
    pub forge_terminal_tasks: u64,
    /// Active Gate streams observed by production telemetry.
    pub gate_active_streams: u64,
    /// Supervised server tasks retained after shutdown.
    pub supervised_tasks: u64,
    /// Whether every configured listener has stopped.
    pub listeners_stopped: bool,
    /// Whether every cluster server has been shut down.
    pub servers_stopped: bool,
}

/// Deterministic workload parameters shared by every matrix scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterLoadProfile {
    /// Production role topology to boot.
    pub topology: BifrostTopology,
    /// Number of isolated data tenants.
    pub tenants: usize,
    /// Warmup batches excluded from measured counters.
    pub warmup_batches_per_tenant: u32,
    /// Batches submitted during the measured phase.
    pub measured_batches_per_tenant: u32,
    /// Rows in every deterministic batch.
    pub rows_per_batch: u32,
    /// Bounded public writer tasks per tenant.
    pub writers_per_tenant: usize,
    /// Bounded strict reader tasks per tenant.
    pub readers_per_tenant: usize,
    /// Minimum completed reads required per tenant.
    pub minimum_reads_per_tenant: u32,
    /// Deadlock/progress ceiling for one scenario.
    pub scenario_deadline: Duration,
    /// Stable row and batch seed.
    pub seed: u64,
    /// Tenant index receiving deterministic admission pressure, when enabled.
    pub pressured_tenant: Option<usize>,
}

impl Serialize for ClusterLoadProfile {
    /// Serializes the profile using milliseconds for the duration field.
    ///
    /// # Errors
    /// Returns the serializer's error when any field cannot be emitted.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ClusterLoadProfile", 11)?;
        state.serialize_field("topology", topology_name(self.topology))?;
        state.serialize_field("tenants", &self.tenants)?;
        state.serialize_field("warmup_batches_per_tenant", &self.warmup_batches_per_tenant)?;
        state.serialize_field(
            "measured_batches_per_tenant",
            &self.measured_batches_per_tenant,
        )?;
        state.serialize_field("rows_per_batch", &self.rows_per_batch)?;
        state.serialize_field("writers_per_tenant", &self.writers_per_tenant)?;
        state.serialize_field("readers_per_tenant", &self.readers_per_tenant)?;
        state.serialize_field("minimum_reads_per_tenant", &self.minimum_reads_per_tenant)?;
        state.serialize_field("scenario_deadline_ms", &self.scenario_deadline.as_millis())?;
        state.serialize_field("seed", &self.seed)?;
        state.serialize_field("pressured_tenant", &self.pressured_tenant)?;
        state.end()
    }
}

impl ClusterLoadProfile {
    /// Validate deterministic bounds before any process is booted.
    ///
    /// # Errors
    /// Returns [`ClusterLoadError::Profile`] for zero tenants, unbounded task
    /// counts, an empty phase, or a deadline below one millisecond.
    pub fn validate(&self) -> Result<(), ClusterLoadError> {
        if self.tenants == 0
            || self.rows_per_batch == 0
            || self.writers_per_tenant != 2
            || self.readers_per_tenant != 2
            || ![
                self.readers_per_tenant as u32,
                self.measured_batches_per_tenant.saturating_mul(2),
            ]
            .contains(&self.minimum_reads_per_tenant)
            || self.warmup_batches_per_tenant < 2
            || self.measured_batches_per_tenant < 8
            || self.scenario_deadline < Duration::from_millis(1)
            || self
                .pressured_tenant
                .is_some_and(|tenant| tenant >= self.tenants)
        {
            return Err(ClusterLoadError::Profile(
                "cluster load profile violates deterministic matrix bounds".to_owned(),
            ));
        }
        Ok(())
    }

    /// Build the one-tenant smoke profile.
    #[must_use]
    pub const fn single_tenant_single_pod() -> Self {
        Self::for_topology(BifrostTopology::OnePod, 1)
    }

    /// Build the eight-tenant single-pod smoke profile.
    #[must_use]
    pub const fn multi_tenant_single_pod() -> Self {
        Self::for_topology(BifrostTopology::OnePod, 8)
    }

    /// Build the eight-tenant profile with tenant zero pressure enabled.
    #[must_use]
    pub const fn pressured_multi_tenant_single_pod() -> Self {
        let mut profile = Self::multi_tenant_single_pod();
        profile.pressured_tenant = Some(0);
        profile
    }

    /// Build the exact three-Server/three-ForgeWorker smoke profile.
    #[must_use]
    pub const fn multi_tenant_three_server_three_worker() -> Self {
        Self::for_topology(BifrostTopology::ThreeServersThreeForgeWorkers, 8)
    }

    /// Build the three-Server pressure profile with tenant zero on Server zero.
    #[must_use]
    pub const fn pressured_multi_tenant_three_server_three_worker() -> Self {
        let mut profile = Self::multi_tenant_three_server_three_worker();
        profile.pressured_tenant = Some(0);
        profile
    }

    /// Construct the locked workload shared by each topology profile.
    const fn for_topology(topology: BifrostTopology, tenants: usize) -> Self {
        Self {
            topology,
            tenants,
            warmup_batches_per_tenant: 2,
            measured_batches_per_tenant: 8,
            rows_per_batch: 64,
            writers_per_tenant: 2,
            readers_per_tenant: 2,
            minimum_reads_per_tenant: 2,
            scenario_deadline: DEFAULT_SCENARIO_DEADLINE,
            seed: 0xB1F0_57A5,
            pressured_tenant: None,
        }
    }
}

/// Aggregate result emitted by the test-tier matrix owner.
#[derive(Debug, Clone, Serialize)]
pub struct BifrostClusterLoadSummary {
    /// Immutable profile used for this run.
    pub profile: ClusterLoadProfile,
    /// Per-tenant operation results in deterministic tenant order.
    pub tenants: BTreeMap<String, TenantLoadResult>,
    /// Jain fairness over acknowledged batches.
    pub jain_fairness: f64,
    /// Final owner/resource state.
    pub cleanup: ClusterCleanupSnapshot,
}

/// Errors raised by the deterministic public-Gate matrix.
#[derive(Debug, Error)]
pub enum ClusterLoadError {
    /// Profile values are outside the locked deterministic matrix.
    #[error("invalid cluster load profile: {0}")]
    Profile(String),
    /// Cluster startup, setup, or shutdown failed.
    #[error("cluster lifecycle failed: {0}")]
    Cluster(String),
    /// Public SDK setup or operation failed.
    #[error("public SDK operation failed: {0}")]
    Client(String),
    /// A required matrix assertion failed.
    #[error("cluster load assertion failed: {0}")]
    Assertion(String),
    /// Production telemetry could not be mapped to D24's closed families.
    #[error("cluster telemetry failed: {0}")]
    Telemetry(String),
}

/// Execute and project one matrix phase through the canonical sampled window.
///
/// # Errors
/// Returns the workload error or canonical capture/projection failure without
/// suppressing sampler cleanup context.
async fn sampled_phase<T, F, Fut>(
    capture: &BifrostTelemetryCapture,
    expectation: ClusterTelemetryExpectation,
    workload: F,
) -> Result<(T, ClusterTelemetryEvidence), ClusterLoadError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, ClusterLoadError>>,
{
    if !matches!(
        expectation.topology,
        BifrostTopology::OnePod | BifrostTopology::ThreeServersThreeForgeWorkers
    ) {
        return Err(ClusterLoadError::Telemetry(
            "topology is outside the canonical cluster matrix".to_owned(),
        ));
    }
    let (value, delta) = run_sampled_window(capture, || async {
        workload()
            .await
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
    })
    .await
    .map_err(|error| ClusterLoadError::Telemetry(error.to_string()))?;
    let evidence =
        ClusterTelemetryProjection::from_delta(&delta, expectation).map_err(|error| {
            let families = delta
                .metrics
                .iter()
                .map(|sample| format!("{}:{:?}", sample.family, sample.kind))
                .collect::<std::collections::BTreeSet<_>>();
            ClusterLoadError::Telemetry(format!("{error}; captured families={families:?}"))
        })?;
    Ok((value, evidence))
}

/// Owns one complete deterministic cluster workload and its cleanup boundary.
pub struct BifrostClusterLoad {
    cluster: Option<WyrdTestCluster>,
    profile: ClusterLoadProfile,
    telemetry: BifrostTelemetryCapture,
}

impl BifrostClusterLoad {
    /// Boot a cluster for one profile and retain its process telemetry owner.
    ///
    /// # Errors
    /// Returns [`ClusterLoadError::Profile`] or [`ClusterLoadError::Cluster`]
    /// when validation or role-complete startup fails.
    pub async fn start(profile: ClusterLoadProfile) -> Result<Self, ClusterLoadError> {
        profile.validate()?;
        let cluster = if profile.pressured_tenant.is_some() {
            WyrdTestCluster::start_spec_with_admission_and_forge_completion_observer_on_node(
                profile.topology.spec_for_test(),
                0,
                AdmissionConfig {
                    memory_limit_bytes: 1024 * 1024 * 1024,
                    scribe_memory_limit_bytes: Some(256 * 1024),
                    event_time_window: EventTimeWindow::default(),
                },
            )
            .await
        } else {
            WyrdTestCluster::start_spec_with_forge_completion_observer(
                profile.topology.spec_for_test(),
            )
            .await
        }
        .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
        let telemetry = cluster.telemetry().clone();
        Ok(Self {
            cluster: Some(cluster),
            profile,
            telemetry,
        })
    }

    /// Run all warmup, measured, reconciliation, and cleanup phases.
    ///
    /// Measured writes and reads use only the public Rust SDK. Administrative
    /// table creation and tenant/bootstrap setup happen before the phase
    /// barrier and are excluded from operation counts.
    pub async fn run(mut self) -> Result<BifrostClusterLoadSummary, ClusterLoadError> {
        let result = tokio::time::timeout(self.profile.scenario_deadline, self.run_inner()).await;
        let result = match result {
            Ok(result) => result,
            Err(_) => Err(ClusterLoadError::Assertion(
                "scenario deadline exceeded during matrix IO".to_owned(),
            )),
        };
        let shutdown = tokio::time::timeout(
            CLUSTER_SHUTDOWN_BUDGET,
            self.cluster
                .take()
                .expect("invariant: cluster remains owned until run completion")
                .shutdown_and_inspect(),
        )
        .await
        .map_err(|_| ClusterLoadError::Cluster("shutdown deadline exceeded".to_owned()))
        .and_then(|result| result.map_err(|error| ClusterLoadError::Cluster(error.to_string())));
        match (result, shutdown) {
            (Ok(mut summary), Ok(shutdown_inspection)) => {
                summary.cleanup.listeners_stopped = shutdown_inspection.listeners_stopped;
                summary.cleanup.servers_stopped = shutdown_inspection.servers_stopped;
                summary.cleanup.scribe_queued = shutdown_inspection.scribe_queued;
                summary.cleanup.scribe_inflight = shutdown_inspection.scribe_inflight;
                summary.cleanup.scribe_wal_streams = shutdown_inspection.scribe_wal_streams;
                summary.cleanup.forge_active_claims = shutdown_inspection.forge_active_claims;
                summary.cleanup.forge_active_attempts = shutdown_inspection.forge_active_attempts;
                summary.cleanup.supervised_tasks = shutdown_inspection.supervised_tasks;
                Ok(summary)
            }
            (Err(error), Ok(_)) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(shutdown_error)) => Err(ClusterLoadError::Cluster(format!(
                "{error}; {shutdown_error}"
            ))),
        }
    }

    /// Execute the workload while the outer owner retains shutdown responsibility.
    ///
    /// # Errors
    /// Returns a typed cluster, client, telemetry, or assertion failure from any phase.
    async fn run_inner(&self) -> Result<BifrostClusterLoadSummary, ClusterLoadError> {
        let cluster = self
            .cluster
            .as_ref()
            .ok_or_else(|| ClusterLoadError::Cluster("cluster already shut down".to_owned()))?;
        let tenants = provision_tenants(cluster, self.profile.tenants).await?;
        let table = format!("vala.bifrost.{TABLE_NAME}");
        provision_tables(cluster, &tenants).await?;
        let setup_server = cluster
            .server(0)
            .ok_or_else(|| ClusterLoadError::Cluster("setup Server is absent".to_owned()))?;
        let setup_client = public_client(setup_server, tenants[0], "load-setup-telemetry").await?;
        QueryClient::new(&setup_client)
            .collect_bounded(
                &BifrostQueryRequest {
                    sql: format!("SELECT id, tenant, batch FROM {table} LIMIT 0"),
                    visibility: VisibilityMode::PublishedOnly,
                    freshness: FreshnessPolicy::Strict,
                    deadline_ms: Some(5_000),
                },
                CollectedQueryLimits {
                    max_rows: 1,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await
            .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
        let results =
            run_public_matrix(&self.telemetry, cluster, &self.profile, &tenants, &table).await?;
        let fairness = jain_fairness(results.values().filter_map(|result| {
            (self.profile.pressured_tenant != Some(result.tenant_index))
                .then_some(result.acknowledged_rows)
        }));
        if fairness < 0.95 {
            return Err(ClusterLoadError::Assertion(format!(
                "Jain fairness {fairness:.3} is below 0.95"
            )));
        }
        let cancellation_owners = phase_owner_checkpoint(cluster).await?;
        let (_, cancellation) = sampled_phase(
            &self.telemetry,
            ClusterTelemetryExpectation {
                topology: self.profile.topology,
                required_binding_ids: CANCELLATION_BINDINGS,
                required_dependency_ids: QUERY_DEPENDENCIES,
                required_trace_operations: CANCELLATION_TRACES,
                required_clean_binding_ids: QUERY_CLEANUP,
            },
            || async { exercise_query_cancellation(setup_server, &setup_client, &table).await },
        )
        .await?;
        let cancellation_finished_owners = phase_owner_checkpoint(cluster).await?;
        let cancellation_owner_delta =
            owner_delta(cancellation_owners, cancellation_finished_owners)?;
        assert_cancellation_outcomes(&cancellation.phase)?;
        assert_phase_counter(
            "cancellation Oracle read audits",
            cancellation_owner_delta.read_audit_rows,
            1,
        )?;
        for server in cluster.servers() {
            if server.bifrost_scribe().is_some() {
                server
                    .flush_bifrost()
                    .await
                    .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
            }
        }
        let cleanup = cleanup_snapshot(cluster, cancellation.phase.gate_active_streams).await?;
        if cleanup.forge_active_claims != 0 || cleanup.forge_active_attempts != 0 {
            return Err(ClusterLoadError::Assertion(format!(
                "Forge cleanup retained {} claims and {} attempts",
                cleanup.forge_active_claims, cleanup.forge_active_attempts
            )));
        }
        let summary = BifrostClusterLoadSummary {
            profile: self.profile,
            tenants: results,
            jain_fairness: fairness,
            cleanup,
        };
        Ok(summary)
    }
}

/// Observe whether a configured query stall or the query task completes first.
///
/// # Errors
/// Returns the original stall, client, join, or unexpected-success evidence
/// with cancellation-probe context when the query does not reach the stall.
async fn await_query_schema_stall_or_completion<S, T, E>(
    stall: S,
    query_task: &mut tokio::task::JoinHandle<Result<T, E>>,
) -> Result<String, ClusterLoadError>
where
    S: Future<Output = Result<String, crate::WyrdTestServerError>>,
    T: fmt::Debug,
    E: fmt::Display,
{
    tokio::pin!(stall);
    tokio::select! {
        stalled = &mut stall => stalled.map_err(|error| {
            ClusterLoadError::Client(format!(
                "cancellation query schema stall failed: {error}"
            ))
        }),
        completed = &mut *query_task => match completed {
            Err(error) => Err(ClusterLoadError::Client(format!(
                "cancellation query task join failed before schema stall: {error}"
            ))),
            Ok(Err(error)) => Err(ClusterLoadError::Client(format!(
                "cancellation query completed before schema stall: {error}"
            ))),
            Ok(Ok(result)) => Err(ClusterLoadError::Assertion(format!(
                "cancellation query unexpectedly succeeded before schema stall: {result:?}"
            ))),
        },
    }
}

/// Cancel one schema-stalled public query and prove every query owner returns
/// to its captured baseline before the matrix continues.
///
/// # Errors
/// Returns a client, cluster, or assertion error when either public operation
/// cannot reach its deterministic stall or release every observed owner.
async fn exercise_query_cancellation(
    server: &crate::WyrdTestServer,
    client: &WyrdClient,
    table: &str,
) -> Result<crate::server::BifrostQueryResourceSnapshot, ClusterLoadError> {
    let lifecycle_observer = vala_bifrost_redux::oracle::query_lifecycle_observer_for_test();
    let lifecycle_target = lifecycle_observer.cancelled().saturating_add(1);
    let writer = BifrostGrpcTransport::connect(client)
        .await
        .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    let write_payload = ipc_payload(server.data_tenant_id(), 0, u32::MAX, 64)?;
    let write_inflight_baseline = server
        .bifrost_scribe()
        .ok_or_else(|| ClusterLoadError::Cluster("cancellation server has no Scribe".to_owned()))?
        .inflight_items_for_test();
    let write_table = table.to_owned();
    let write_stall = server
        .stall_next_bifrost_write()
        .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    let write_task = tokio::spawn(async move {
        writer
            .send_frame(BifrostFrame {
                table: write_table,
                batch_id: deterministic_batch_id(0xCA11_CE11, 0, u32::MAX),
                arrow_ipc: write_payload.into(),
            })
            .await
    });
    write_stall.wait_entered().await;
    let write_inflight_stalled = server
        .bifrost_scribe()
        .ok_or_else(|| ClusterLoadError::Cluster("cancellation server has no Scribe".to_owned()))?
        .inflight_items_for_test();
    if write_inflight_stalled != write_inflight_baseline.saturating_add(1) {
        return Err(ClusterLoadError::Assertion(format!(
            "stalled write did not own exactly one admission: baseline={write_inflight_baseline} stalled={write_inflight_stalled}"
        )));
    }
    write_task.abort();
    let _ = write_task.await;
    tokio::time::timeout(Duration::from_secs(5), write_stall.wait_completed())
        .await
        .map_err(|_| {
            ClusterLoadError::Client("write cancellation completion timed out".to_owned())
        })?;
    let write_inflight_released = server
        .bifrost_scribe()
        .ok_or_else(|| ClusterLoadError::Cluster("cancellation server has no Scribe".to_owned()))?
        .inflight_items_for_test();
    if write_inflight_released != write_inflight_baseline {
        return Err(ClusterLoadError::Assertion(format!(
            "cancelled write retained admission: baseline={write_inflight_baseline} final={write_inflight_released}"
        )));
    }

    server.stall_next_query_after_schema();
    let query = QueryClient::new(client);
    let sql = format!("SELECT id, tenant, batch FROM {table} LIMIT 0");
    let mut task = tokio::spawn(async move {
        query
            .collect_bounded(
                &BifrostQueryRequest {
                    sql,
                    visibility: VisibilityMode::PublishedOnly,
                    freshness: FreshnessPolicy::Strict,
                    deadline_ms: Some(5_000),
                },
                CollectedQueryLimits {
                    max_rows: 1,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await
    });
    let query_id =
        await_query_schema_stall_or_completion(server.wait_query_schema_stall(), &mut task).await?;
    let baseline = crate::server::BifrostQueryResourceSnapshot {
        admission_slots: 0,
        memory_bytes: 0,
        peer_slots: 0,
        tail_fences: 0,
    };
    task.abort();
    let _ = task.await;
    tokio::time::timeout(
        Duration::from_secs(5),
        lifecycle_observer.wait_for_cancelled_at_least(lifecycle_target),
    )
    .await
    .map_err(|_| {
        ClusterLoadError::Client("query lifecycle cancellation observer timed out".to_owned())
    })?;
    let released = server
        .wait_bifrost_query_resources_released(&query_id, baseline)
        .await
        .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    Ok(released)
}

/// Provision the exact isolated tenant roster before measured traffic begins.
///
/// # Errors
/// Returns [`ClusterLoadError::Cluster`] when any tenant cannot be created.
async fn provision_tenants(
    cluster: &WyrdTestCluster,
    count: usize,
) -> Result<Vec<DataTenantId>, ClusterLoadError> {
    let mut tenants = vec![cluster.data_tenant_id()];
    for index in 1..count {
        tenants.push(
            cluster
                .add_tenant(&format!("cluster-load-{index}"))
                .await
                .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?,
        );
    }
    Ok(tenants)
}

/// Provision the shared typed table contract independently for every tenant.
///
/// # Errors
/// Returns a cluster error when the catalog is absent or table creation fails.
async fn provision_tables(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
) -> Result<(), ClusterLoadError> {
    let server = cluster
        .server(0)
        .ok_or_else(|| ClusterLoadError::Cluster("cluster has no Server".to_owned()))?;
    let catalog = server.state().bifrost_catalog();
    let fields = vec![
        Field::new("id", DataType::Int64, false),
        Field::new("tenant", DataType::Utf8, false),
        Field::new("batch", DataType::Utf8, false),
    ];
    for tenant in tenants {
        catalog
            .expect("production server has Bifrost catalog")
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
                user_fields: fields.clone(),
                tenant: *tenant,
                audit: None,
            })
            .await
            .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
    }
    Ok(())
}

/// Run synchronized public writes and reads and return phase telemetry evidence.
///
/// # Errors
/// Returns a typed lifecycle, public-client, telemetry, or reconciliation failure.
async fn run_public_matrix(
    telemetry: &BifrostTelemetryCapture,
    cluster: &WyrdTestCluster,
    profile: &ClusterLoadProfile,
    tenants: &[DataTenantId],
    table: &str,
) -> Result<BTreeMap<String, TenantLoadResult>, ClusterLoadError> {
    let publish_tenants = || async {
        for (tenant_index, tenant) in tenants.iter().copied().enumerate() {
            let node_count = cluster.ready_ingest_nodes().len().max(1);
            let pressured = profile.pressured_tenant == Some(tenant_index);
            let mut writer_index = tenant_index % node_count;
            if !pressured && profile.pressured_tenant.is_some() && writer_index == 0 {
                writer_index = 1 % node_count;
            }
            let writer = cluster.server(writer_index).ok_or_else(|| {
                ClusterLoadError::Cluster("flush writer Server is absent".to_owned())
            })?;
            let completion = cluster.forge_completion_observer();
            let expected_completion = completion
                .as_ref()
                .map(|observer| observer.completed().saturating_add(1));
            writer
                .flush_bifrost_for_tenant(tenant)
                .await
                .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
            let pending_forge_tasks = writer
                .bifrost_pending_forge_tasks_for_tenant(tenant)
                .await
                .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
            if pending_forge_tasks > 0
                && let (Some(observer), Some(expected)) = (completion, expected_completion)
            {
                tokio::time::timeout(Duration::from_secs(5), observer.wait_for_at_least(expected))
                    .await
                    .map_err(|_| {
                        ClusterLoadError::Cluster(format!(
                            "Forge publication observer did not complete a scheduled task for tenant {tenant}"
                        ))
                    })?;
            }
        }
        Ok::<_, ClusterLoadError>(())
    };
    let matrix_owners = phase_owner_checkpoint(cluster).await?;
    let ((phase_progress, mut tasks), warmup) = sampled_phase(
        telemetry,
        ClusterTelemetryExpectation {
            topology: profile.topology,
            required_binding_ids: WARMUP_BINDINGS,
            required_dependency_ids: WARMUP_DEPENDENCIES,
            required_trace_operations: WARMUP_TRACES,
            required_clean_binding_ids: &[],
        },
        || async {
            let failed = TenantFailure::default();
            let phase_progress = Arc::new(PhaseProgress::new(tenants.len(), failed.clone()));
            let mut tasks = tokio::task::JoinSet::new();
            let barriers = Arc::new(TenantPhaseBarriers::new(tenants.len(), failed));
            for (tenant_index, tenant) in tenants.iter().copied().enumerate() {
                let node_count = cluster.ready_ingest_nodes().len().max(1);
                let pressured = profile.pressured_tenant == Some(tenant_index);
                let mut writer_index = tenant_index % node_count;
                if !pressured && profile.pressured_tenant.is_some() && writer_index == 0 {
                    writer_index = 1 % node_count;
                }
                let mut reader_index = if cluster.ready_query_nodes().len() > 1 {
                    (writer_index + 1) % cluster.ready_query_nodes().len()
                } else {
                    writer_index
                };
                if !pressured && profile.pressured_tenant.is_some() && reader_index == 0 {
                    reader_index = 1 % cluster.ready_query_nodes().len().max(1);
                }
                let writer = cluster.server(writer_index).ok_or_else(|| {
                    ClusterLoadError::Cluster("writer Server is absent".to_owned())
                })?;
                let reader = cluster.server(reader_index).ok_or_else(|| {
                    ClusterLoadError::Cluster("reader Server is absent".to_owned())
                })?;
                let writer_client =
                    public_client(writer, tenant, &format!("load-writer-{tenant_index}")).await?;
                let reader_client =
                    public_client(reader, tenant, &format!("load-reader-{tenant_index}")).await?;
                let writer_transport = BifrostGrpcTransport::connect(&writer_client)
                    .await
                    .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
                let query = QueryClient::new(&reader_client);
                let profile = *profile;
                let table = table.to_owned();
                let barriers = Arc::clone(&barriers);
                let phase_progress = Arc::clone(&phase_progress);
                tasks.spawn(async move {
                    // Trip the shared token before returning so no sibling is
                    // left parked on a barrier that can no longer release.
                    let outcome = run_tenant(TenantRunContext {
                        tenant,
                        tenant_index,
                        profile,
                        table,
                        writer: writer_transport,
                        query,
                        barriers: Arc::clone(&barriers),
                        phase_progress,
                    })
                    .await;
                    if let Err(error) = &outcome {
                        barriers.fail(error);
                    }
                    outcome.map(|result| (tenant.to_string(), result))
                });
            }
            phase_progress.wait_for(LoadPhase::Warmup).await?;
            publish_tenants().await?;
            Ok((phase_progress, tasks))
        },
    )
    .await
    .map_err(|error| ClusterLoadError::Telemetry(format!("warmup: {error}")))?;
    let warmup_owners = phase_owner_checkpoint(cluster).await?;
    let warmup_owner_delta = owner_delta(matrix_owners, warmup_owners)?;
    let (_, measured) = sampled_phase(
        telemetry,
        ClusterTelemetryExpectation {
            topology: profile.topology,
            required_binding_ids: MEASURED_BINDINGS,
            required_dependency_ids: MEASURED_DEPENDENCIES,
            required_trace_operations: MEASURED_TRACES,
            required_clean_binding_ids: &[],
        },
        || async {
            phase_progress.release_warmup();
            phase_progress.wait_for(LoadPhase::Measured).await?;
            Ok(())
        },
    )
    .await
    .map_err(|error| ClusterLoadError::Telemetry(format!("measured: {error}")))?;
    let measured_owners = phase_owner_checkpoint(cluster).await?;
    let measured_owner_delta = owner_delta(warmup_owners, measured_owners)?;

    let mut results = BTreeMap::new();
    while let Some(result) = tasks.join_next().await {
        let (tenant, report) =
            result.map_err(|error| ClusterLoadError::Client(error.to_string()))??;
        results.insert(tenant, report);
    }
    let (_, publication) = sampled_phase(
        telemetry,
        ClusterTelemetryExpectation {
            topology: profile.topology,
            required_binding_ids: &[],
            required_dependency_ids: PUBLICATION_DEPENDENCIES,
            required_trace_operations: PUBLICATION_TRACES,
            required_clean_binding_ids: PUBLICATION_CLEANUP,
        },
        publish_tenants,
    )
    .await?;
    let publication_owners = phase_owner_checkpoint(cluster).await?;
    let publication_owner_delta = owner_delta(measured_owners, publication_owners)?;
    let (_, final_verification) = sampled_phase(telemetry, ClusterTelemetryExpectation {
        topology: profile.topology,
        required_binding_ids: FINAL_BINDINGS,
        required_dependency_ids: QUERY_DEPENDENCIES,
        required_trace_operations: QUERY_TRACES,
        required_clean_binding_ids: QUERY_CLEANUP,
    }, || async {
        for (tenant_index, tenant) in tenants.iter().copied().enumerate() {
        let node_count = cluster.ready_ingest_nodes().len().max(1);
        let pressured = profile.pressured_tenant == Some(tenant_index);
        let mut writer_index = tenant_index % node_count;
        if !pressured && profile.pressured_tenant.is_some() && writer_index == 0 {
            writer_index = 1 % node_count;
        }
        let mut reader_index = if cluster.ready_query_nodes().len() > 1 {
            (writer_index + 1) % cluster.ready_query_nodes().len()
        } else {
            writer_index
        };
        if !pressured && profile.pressured_tenant.is_some() && reader_index == 0 {
            reader_index = 1 % cluster.ready_query_nodes().len().max(1);
        }
        let reader = cluster
            .server(reader_index)
            .ok_or_else(|| ClusterLoadError::Cluster("final reader Server is absent".to_owned()))?;
        let final_client =
            public_client(reader, tenant, &format!("load-final-{tenant_index}")).await?;
        let query = QueryClient::new(&final_client);
        let audit_count_before = reader
            .bifrost_read_decision_count_for_tenant(tenant)
            .await
            .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
        let final_sql = if profile.pressured_tenant == Some(tenant_index) {
            format!("SELECT id, tenant, batch FROM {table}")
        } else {
            format!(
                "SELECT id, tenant, batch FROM {table} WHERE CAST(batch AS BIGINT) >= 2"
            )
        };
        let final_result = query
            .collect_bounded(
                &BifrostQueryRequest {
                    sql: final_sql,
                    visibility: VisibilityMode::PublishedOnly,
                    freshness: FreshnessPolicy::Strict,
                    deadline_ms: Some(5_000),
                },
                CollectedQueryLimits {
                    max_rows: usize::MAX,
                    max_encoded_bytes: 256 * 1024 * 1024,
                },
            )
            .await
            .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
        let report = results
            .get_mut(&tenant.to_string())
            .ok_or_else(|| ClusterLoadError::Assertion("missing tenant result".to_owned()))?;
        report.final_rows = final_result.rows as u64;
        report.final_bytes = query_payload_bytes(&final_result)?;
        report.final_schema_columns = final_result.schema.fields().len();
        let expected_schema = [
            ("id", DataType::Int64),
            ("tenant", DataType::Utf8),
            ("batch", DataType::Utf8),
        ];
        if final_result
            .schema
            .fields()
            .iter()
            .zip(expected_schema.iter())
            .any(|(field, (name, data_type))| {
                field.name() != *name || field.data_type() != data_type
            })
            || final_result.schema.fields().len() != expected_schema.len()
        {
            return Err(ClusterLoadError::Assertion(format!(
                "tenant {tenant} final schema does not match id/tenant/batch contract: {:?}",
                final_result.schema
            )));
        }
        let expected_tenant = tenant.to_string();
        for batch in &final_result.batches {
            let values = batch
                .column(1)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| {
                    ClusterLoadError::Assertion("tenant column is not Utf8".to_owned())
                })?;
            if (0..values.len()).any(|index| values.value(index) != expected_tenant) {
                return Err(ClusterLoadError::Assertion(format!(
                    "tenant {tenant} final query exposed a cross-tenant row"
                )));
            }
        }
        let terminal_valid = final_result
            .terminal
            .validate(VisibilityMode::PublishedOnly)
            .is_ok()
            && final_result.terminal.row_count == final_result.rows as u64;
        if !terminal_valid {
            return Err(ClusterLoadError::Assertion(format!(
                "tenant {tenant} final terminal is not a valid single terminal"
            )));
        }
        report.final_terminal_count = u8::from(terminal_valid);
        // Wait for the read-audit relay to drain the just-executed public read
        // before asserting on its durable row count; the pending WAL residual is
        // an exact counter, so this converges without masking a real shortfall.
        await_read_audit_convergence(cluster).await?;
        let audit_count_after = reader
            .bifrost_read_decision_count_for_tenant(tenant)
            .await
            .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
        if audit_count_after != audit_count_before + 1 {
            return Err(ClusterLoadError::Assertion(format!(
                "tenant {tenant} final public read committed {} audit rows, expected one",
                audit_count_after.saturating_sub(audit_count_before)
            )));
        }
        let final_request_id = reader
            .latest_bifrost_read_decision_request_id(tenant)
            .await
            .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?
            .ok_or_else(|| {
                ClusterLoadError::Assertion(format!(
                    "tenant {tenant} final public read omitted its request ID"
                ))
            })?;
        uuid::Uuid::parse_str(&final_request_id).map_err(|error| {
            ClusterLoadError::Assertion(format!(
                "tenant {tenant} final audit request ID is invalid: {error}"
            ))
        })?;
        report.tenant_audit_rows = reader
            .bifrost_read_decision_for_request(tenant, &final_request_id)
            .await
            .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?
            as u64;
        if report.tenant_audit_rows != 1 {
            return Err(ClusterLoadError::Assertion(format!(
                "tenant {tenant} final read request {final_request_id} produced {} audit rows, expected one",
                report.tenant_audit_rows
            )));
        }
        let admitted_rows = u64::from(profile.measured_batches_per_tenant)
            .saturating_mul(u64::from(profile.rows_per_batch));
        let expected_rows = if profile.pressured_tenant == Some(tenant_index) {
            if report.backpressure == 0 {
                return Err(ClusterLoadError::Assertion(
                    "pressured tenant observed no explicit backpressure".to_owned(),
                ));
            }
            if report.measured_acknowledged_batches == 0 || report.completed_reads == 0 {
                return Err(ClusterLoadError::Assertion(format!(
                    "pressured tenant {tenant} made no measured write/read progress"
                )));
            }
            let expected_rejections = profile
                .measured_batches_per_tenant
                .saturating_sub(report.acknowledged_batches);
            if report.rejected_batches != expected_rejections {
                return Err(ClusterLoadError::Assertion(format!(
                    "pressured tenant {tenant} rejected {} batches; expected {expected_rejections}",
                    report.rejected_batches
                )));
            }
            report.acknowledged_rows.saturating_add(
                u64::from(profile.warmup_batches_per_tenant)
                    .saturating_mul(u64::from(profile.rows_per_batch)),
            )
        } else {
            if report.acknowledged_rows != admitted_rows || report.progressed_phases != 2 {
                return Err(ClusterLoadError::Assertion(format!(
                    "tenant {tenant} lost admitted work: rows={} expected={admitted_rows} phases={}",
                    report.acknowledged_rows, report.progressed_phases
                )));
            }
            admitted_rows
        };
        if report.final_rows != expected_rows
            || report.final_schema_columns != 3
            || report.final_terminal_count != 1
            || report.tenant_audit_rows == 0
        {
            return Err(ClusterLoadError::Assertion(format!(
                "tenant {tenant} final publication mismatch: rows={} schema={} terminals={} audits={}",
                report.final_rows,
                report.final_schema_columns,
                report.final_terminal_count,
                report.tenant_audit_rows
            )));
        }
        }
        Ok(())
    })
    .await?;
    let final_owners = phase_owner_checkpoint(cluster).await?;
    let final_owner_delta = owner_delta(publication_owners, final_owners)?;
    reconcile_matrix_telemetry(
        profile,
        &results,
        &[
            ("warmup", &warmup, warmup_owner_delta),
            ("measured", &measured, measured_owner_delta),
            ("flush_publication", &publication, publication_owner_delta),
            ("final_verification", &final_verification, final_owner_delta),
        ],
    )?;
    Ok(results)
}

/// Durable owner counts captured at one telemetry phase boundary.
#[derive(Clone, Copy)]
struct PhaseOwnerCheckpoint {
    /// Forge tasks with durable terminal evidence.
    forge_terminal_tasks: u64,
    /// Oracle read-decision audit rows durably committed.
    read_audit_rows: u64,
}

/// Bounded wall-clock budget for the read-audit relay to drain before a phase
/// boundary reads the durable audit-row counts it is about to assert on.
const AUDIT_RELAY_CONVERGENCE_BUDGET: Duration = Duration::from_secs(30);

/// Capture durable Forge and Oracle owner counts for one phase boundary.
///
/// Before reading the counts, this waits on a production durability signal: it
/// polls every pod's pending read-audit WAL residual (`audit_wal_records`, an
/// exact counter) until it reaches zero, so the subsequent `vala.audit_outbox`
/// row counts reflect a fully relayed cluster. This is a convergence wait, not a
/// masking retry — the asserted counts are unchanged and the wait errors loudly
/// with the residual and oldest-record age if the relay fails to drain.
///
/// # Errors
/// Returns [`ClusterLoadError::Cluster`] when the shared database inspection
/// fails, or [`ClusterLoadError::Assertion`] when the read-audit relay does not
/// converge within [`AUDIT_RELAY_CONVERGENCE_BUDGET`].
async fn phase_owner_checkpoint(
    cluster: &WyrdTestCluster,
) -> Result<PhaseOwnerCheckpoint, ClusterLoadError> {
    let inspection = await_read_audit_convergence(cluster).await?;
    Ok(PhaseOwnerCheckpoint {
        forge_terminal_tasks: inspection.forge_terminal_tasks,
        read_audit_rows: inspection.read_audit_rows,
    })
}

/// Polls cluster Oracle inspection until every pod's read-audit WAL is drained.
///
/// Returns the converged inspection so the caller reuses its durable counts
/// without a second query.
///
/// # Errors
/// Returns [`ClusterLoadError::Cluster`] on inspection failure or
/// [`ClusterLoadError::Assertion`] when the pending residual is still non-zero
/// at [`AUDIT_RELAY_CONVERGENCE_BUDGET`].
async fn await_read_audit_convergence(
    cluster: &WyrdTestCluster,
) -> Result<crate::bifrost::cluster::OracleInspection, ClusterLoadError> {
    let deadline = Instant::now() + AUDIT_RELAY_CONVERGENCE_BUDGET;
    loop {
        let inspection = cluster
            .oracle_inspection()
            .await
            .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
        if inspection.audit_wal_records == 0 {
            return Ok(inspection);
        }
        if Instant::now() >= deadline {
            return Err(ClusterLoadError::Assertion(format!(
                "read-audit relay did not converge: {} WAL records pending (oldest {:?}) after {:?}",
                inspection.audit_wal_records,
                inspection.audit_oldest_age,
                AUDIT_RELAY_CONVERGENCE_BUDGET,
            )));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Attach checked durable-owner deltas to one production telemetry window.
///
/// # Errors
/// Returns [`ClusterLoadError::Assertion`] when a durable owner counter regresses.
fn owner_delta(
    before: PhaseOwnerCheckpoint,
    after: PhaseOwnerCheckpoint,
) -> Result<PhaseOwnerDelta, ClusterLoadError> {
    let forge_terminal_tasks = after
        .forge_terminal_tasks
        .checked_sub(before.forge_terminal_tasks)
        .ok_or_else(|| {
            ClusterLoadError::Assertion("Forge terminal task count regressed".to_owned())
        })?;
    let read_audit_rows = after
        .read_audit_rows
        .checked_sub(before.read_audit_rows)
        .ok_or_else(|| {
            ClusterLoadError::Assertion("Oracle read audit count regressed".to_owned())
        })?;
    Ok(PhaseOwnerDelta {
        forge_terminal_tasks,
        read_audit_rows,
    })
}

/// Direct durable-owner deltas paired with one canonical metric projection.
#[derive(Clone, Copy)]
struct PhaseOwnerDelta {
    /// Forge tasks reaching a durable terminal state in the phase.
    forge_terminal_tasks: u64,
    /// Tenant-bound Oracle read audit rows committed in the phase.
    read_audit_rows: u64,
}

/// Reconcile each public workload phase against its owner-side operation ledger.
///
/// The checkpoint windows are intentionally checked independently so a counter
/// emitted by a later publication/read phase cannot hide a missing write or an
/// unexpected rejection in the measured phase.
///
/// # Errors
/// Returns [`ClusterLoadError::Assertion`] when a phase counter, byte count, or
/// closed Gate outcome differs from the public SDK ledger.
fn reconcile_matrix_telemetry(
    profile: &ClusterLoadProfile,
    results: &BTreeMap<String, TenantLoadResult>,
    phases: &[(&str, &ClusterTelemetryEvidence, PhaseOwnerDelta)],
) -> Result<(), ClusterLoadError> {
    let phase = |name: &str| {
        phases
            .iter()
            .find(|(phase, _, _)| *phase == name)
            .map(|(_, evidence, owners)| (&evidence.phase, *owners))
            .ok_or_else(|| ClusterLoadError::Assertion(format!("missing telemetry phase {name}")))
    };
    let (warmup, warmup_owners) = phase("warmup")?;
    let (measured, measured_owners) = phase("measured")?;
    let (final_verification, final_owners) = phase("final_verification")?;
    let warmup_rows = results
        .values()
        .map(|tenant| {
            u64::from(tenant.warmup_acknowledged_batches)
                .saturating_mul(u64::from(profile.rows_per_batch))
        })
        .sum::<u64>();
    let warmup_bytes = results
        .values()
        .map(|tenant| tenant.warmup_acknowledged_bytes)
        .sum::<u64>();
    let measured_rows = results
        .values()
        .map(|tenant| tenant.acknowledged_rows)
        .sum::<u64>();
    let measured_bytes = results
        .values()
        .map(|tenant| tenant.acknowledged_bytes)
        .sum::<u64>();
    let measured_submitted_bytes = results
        .keys()
        .map(|tenant_name| {
            let tenant = uuid::Uuid::parse_str(tenant_name)
                .ok()
                .and_then(|uuid| DataTenantId::new(uuid).ok())
                .ok_or_else(|| {
                    ClusterLoadError::Assertion(format!("invalid tenant identity {tenant_name}"))
                })?;
            (2..profile.measured_batches_per_tenant + 2)
                .map(|batch| {
                    ipc_payload(tenant, 0, batch, profile.rows_per_batch)
                        .map(|bytes| bytes.len() as u64)
                })
                .try_fold(0_u64, |total, bytes| {
                    bytes.map(|value| total.saturating_add(value))
                })
        })
        .collect::<Result<Vec<_>, ClusterLoadError>>()?
        .into_iter()
        .sum::<u64>();
    let measured_successes = results
        .values()
        .map(|tenant| u64::from(tenant.acknowledged_batches) + u64::from(tenant.completed_reads))
        .sum::<u64>();
    let rejected = results
        .values()
        .map(|tenant| u64::from(tenant.rejected_batches))
        .sum::<u64>();
    let final_rows = results
        .values()
        .map(|tenant| tenant.final_rows)
        .sum::<u64>();
    let queried_rows = results
        .values()
        .map(|tenant| tenant.queried_rows)
        .sum::<u64>();
    let queried_bytes = results
        .values()
        .map(|tenant| tenant.queried_bytes)
        .sum::<u64>();
    let retries = results
        .values()
        .map(|tenant| u64::from(tenant.retries))
        .sum::<u64>();
    assert_phase_counter("warmup Gate rows", warmup.gate_rows, warmup_rows)?;
    assert_phase_counter("warmup Gate bytes", warmup.gate_bytes, warmup_bytes)?;
    assert_phase_counter("warmup Scribe rows", warmup.scribe_rows, warmup_rows)?;
    assert_phase_counter("warmup Oracle stream rows", warmup.oracle_stream_rows, 0)?;
    assert_phase_counter(
        "warmup Oracle read audits",
        warmup_owners.read_audit_rows,
        0,
    )?;
    assert_phase_counter(
        "warmup Gate successes",
        warmup.gate_success,
        warmup_rows / u64::from(profile.rows_per_batch),
    )?;
    assert_phase_counter("measured Gate rows", measured.gate_rows, measured_rows)?;
    let expected_measured_bytes = if profile.pressured_tenant.is_some() {
        measured_submitted_bytes
    } else {
        measured_bytes
    };
    assert_phase_counter(
        "measured Gate bytes",
        measured.gate_bytes,
        expected_measured_bytes,
    )?;
    assert_phase_counter("measured Scribe rows", measured.scribe_rows, measured_rows)?;
    assert_phase_counter(
        "measured Oracle stream rows",
        measured.oracle_stream_rows,
        queried_rows,
    )?;
    assert_phase_counter(
        "measured Oracle stream bytes",
        measured.oracle_stream_bytes,
        queried_bytes,
    )?;
    assert_phase_counter(
        "measured Oracle read audits",
        measured_owners.read_audit_rows,
        results
            .values()
            .map(|tenant| u64::from(tenant.completed_reads))
            .sum(),
    )?;
    assert_phase_counter(
        "measured Gate successes",
        measured.gate_success,
        measured_successes,
    )?;
    assert_phase_counter(
        "measured Gate non-success retry/rejection outcomes",
        measured.gate_rejected + measured.gate_failed,
        rejected.saturating_add(retries),
    )?;
    assert_phase_counter(
        "final Oracle stream rows",
        final_verification.oracle_stream_rows,
        final_rows,
    )?;
    assert_phase_counter(
        "final Oracle stream bytes",
        final_verification.oracle_stream_bytes,
        results.values().map(|tenant| tenant.final_bytes).sum(),
    )?;
    assert_phase_counter(
        "final Oracle read audits",
        final_owners.read_audit_rows,
        profile.tenants as u64,
    )?;
    assert_phase_counter(
        "final Gate successes",
        final_verification.gate_success,
        profile.tenants as u64,
    )?;
    for (name, evidence, owners) in phases {
        assert_forge_phase(name, &evidence.phase, *owners)?;
    }
    for (name, evidence, _) in phases {
        assert_phase_counter(
            &format!("{name} cancelled Gate requests"),
            evidence.phase.gate_cancelled,
            0,
        )?;
    }
    Ok(())
}

/// Reconcile a phase's durable Forge terminals with committed rewrite volume.
///
/// # Errors
/// Returns [`ClusterLoadError::Assertion`] when volume exists without a durable
/// terminal task, a committed task lacks volume, or file/byte counts are invalid.
fn assert_forge_phase(
    phase: &str,
    evidence: &ClusterPhaseTelemetryEvidence,
    owners: PhaseOwnerDelta,
) -> Result<(), ClusterLoadError> {
    let has_volume = evidence.forge_input_files > 0
        || evidence.forge_input_bytes > 0
        || evidence.forge_output_files > 0
        || evidence.forge_output_bytes > 0;
    if owners.forge_terminal_tasks == 0 && has_volume {
        return Err(ClusterLoadError::Assertion(format!(
            "{phase} Forge volume has no durable terminal task"
        )));
    }
    if owners.forge_terminal_tasks > 0 {
        assert_phase_counter(
            &format!("{phase} Forge output files"),
            evidence.forge_output_files,
            owners.forge_terminal_tasks,
        )?;
        if evidence.forge_input_files < owners.forge_terminal_tasks
            || evidence.forge_input_bytes == 0
            || evidence.forge_output_bytes == 0
        {
            return Err(ClusterLoadError::Assertion(format!(
                "{phase} Forge committed tasks lack exact input/output volume: tasks={} input_files={} input_bytes={} output_files={} output_bytes={}",
                owners.forge_terminal_tasks,
                evidence.forge_input_files,
                evidence.forge_input_bytes,
                evidence.forge_output_files,
                evidence.forge_output_bytes
            )));
        }
    }
    Ok(())
}

/// Compare an integer production counter represented by a floating metric.
fn assert_phase_counter(name: &str, actual: u64, expected: u64) -> Result<(), ClusterLoadError> {
    if actual != expected {
        return Err(ClusterLoadError::Assertion(format!(
            "{name} mismatch: actual={actual} expected={expected}"
        )));
    }
    Ok(())
}

/// Assert exact D24 terminal outcomes for the active write and query cancellations.
///
/// # Errors
/// Returns [`ClusterLoadError::Assertion`] when either cancelled operation has
/// zero or multiple terminal outcomes, or any other outcome is emitted.
fn assert_cancellation_outcomes(
    evidence: &ClusterPhaseTelemetryEvidence,
) -> Result<(), ClusterLoadError> {
    assert_phase_counter(
        "cancellation Gate query request success",
        evidence.gate_query_request_success,
        1,
    )?;
    assert_phase_counter(
        "cancellation Gate write request cancelled",
        evidence.gate_write_request_cancelled,
        1,
    )?;
    assert_phase_counter(
        "cancellation Gate aggregate successes",
        evidence.gate_success,
        evidence.gate_query_request_success,
    )?;
    assert_phase_counter(
        "cancellation Gate aggregate cancellations",
        evidence.gate_cancelled,
        evidence.gate_write_request_cancelled,
    )?;
    assert_phase_counter(
        "cancellation Gate aggregate rejections",
        evidence.gate_rejected,
        0,
    )?;
    assert_phase_counter(
        "cancellation Gate aggregate failures",
        evidence.gate_failed,
        0,
    )?;
    assert_phase_counter(
        "cancellation Gate query stream cancellations",
        evidence.gate_query_stream_cancelled,
        1,
    )?;
    assert_phase_counter(
        "cancellation Gate query stream terminals",
        evidence.gate_query_stream_terminals,
        evidence.gate_query_stream_cancelled,
    )?;
    assert_phase_counter(
        "cancellation Gate active query streams",
        evidence.gate_active_streams,
        0,
    )?;
    Ok(())
}

/// Build one tenant-bound public SDK client from the normal bootstrap surface.
///
/// # Errors
/// Returns a client error when bootstrap, endpoint discovery, or configuration fails.
async fn public_client(
    server: &crate::WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> Result<WyrdClient, ClusterLoadError> {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
        .await
        .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    let key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => {
            return Err(ClusterLoadError::Client(
                "load bootstrap unexpectedly returned a user".to_owned(),
            ));
        }
    };
    WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server
                .grpc_url()
                .ok_or_else(|| ClusterLoadError::Client("missing gRPC endpoint".to_owned()))?,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server
                .base_url()
                .ok_or_else(|| ClusterLoadError::Client("missing HTTP endpoint".to_owned()))?
                .to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(key),
        ..ClientConfig::default()
    })
    .map_err(|error| ClusterLoadError::Client(error.to_string()))
}

/// Owned dependencies and phase barriers for one tenant's public load task.
struct TenantRunContext {
    /// Tenant identity used in all frames and queries.
    tenant: DataTenantId,
    /// Stable matrix index used for deterministic IDs and routing.
    tenant_index: usize,
    /// Immutable workload and phase configuration.
    profile: ClusterLoadProfile,
    /// Fully qualified public table name.
    table: String,
    /// Public ingest transport bound to the tenant's writer server.
    writer: BifrostGrpcTransport,
    /// Public query client bound to the tenant's reader server.
    query: QueryClient,
    /// Phase barriers shared by every tenant task in this run.
    barriers: Arc<TenantPhaseBarriers>,
    /// Shared progress evidence used to capture immutable telemetry windows.
    phase_progress: Arc<PhaseProgress>,
}

/// Abort signal shared by every rendezvous in one matrix run.
///
/// Both the tenant barriers and the owner's phase counters need the same
/// answer to "is anyone still coming?". Sharing one signal keeps them from
/// disagreeing: a tenant that fails releases its siblings' barriers and the
/// owner's phase wait in the same instant, so the run reports the original
/// error rather than a scenario timeout.
#[derive(Clone, Default)]
struct TenantFailure {
    /// Wakes every parked rendezvous once any tenant has failed.
    token: tokio_util::sync::CancellationToken,
    /// First tenant error, retained so waiters report the cause rather than
    /// the fact that they were abandoned. The owner's phase waits release
    /// before the tenant tasks are joined, so without this the run reports
    /// "a tenant failed" and the actual error is never surfaced.
    cause: Arc<std::sync::OnceLock<String>>,
}

impl TenantFailure {
    /// Records the first tenant failure and releases every parked rendezvous.
    ///
    /// Idempotent: concurrent failures keep the first cause, matching the
    /// first error the joining owner would otherwise have reported.
    fn trip(&self, error: &ClusterLoadError) {
        let _ = self.cause.set(error.to_string());
        self.token.cancel();
    }

    /// Resolves once any tenant has failed.
    async fn cancelled(&self) {
        self.token.cancelled().await;
    }

    /// Returns the first tenant failure, or a placeholder if it raced the trip.
    fn cause(&self) -> &str {
        self.cause
            .get()
            .map_or("cause not yet recorded", String::as_str)
    }
}

/// Three-phase barrier set shared by every tenant task in one matrix run.
///
/// A bare [`tokio::sync::Barrier`] has no failure path. When one tenant returns
/// early, every sibling stays parked until the scenario deadline elapses, so the
/// matrix reports a timeout instead of the error that actually happened. This
/// owner carries a token that the first failing tenant trips, releasing every
/// parked sibling immediately so the original error is the one that propagates.
struct TenantPhaseBarriers {
    /// Released after every tenant finishes its warmup writes.
    warmup: tokio::sync::Barrier,
    /// Released before measured writers and readers start.
    measured: tokio::sync::Barrier,
    /// Released after every tenant finishes its measured tasks.
    completed: tokio::sync::Barrier,
    /// Shared abort signal tripped by the first failing tenant.
    failed: TenantFailure,
}

impl TenantPhaseBarriers {
    /// Builds one barrier set sized for a fixed tenant count.
    fn new(tenants: usize, failed: TenantFailure) -> Self {
        Self {
            warmup: tokio::sync::Barrier::new(tenants),
            measured: tokio::sync::Barrier::new(tenants),
            completed: tokio::sync::Barrier::new(tenants),
            failed,
        }
    }

    /// Records that one tenant failed, releasing every parked sibling.
    fn fail(&self, error: &ClusterLoadError) {
        self.failed.trip(error);
    }

    /// Parks until every tenant reaches the warmup checkpoint.
    ///
    /// # Errors
    /// Returns an assertion error when another tenant failed before release.
    async fn warmup(&self) -> Result<(), ClusterLoadError> {
        self.wait_on(&self.warmup, "warmup").await
    }

    /// Parks until every tenant is ready to begin measured IO.
    ///
    /// # Errors
    /// Returns an assertion error when another tenant failed before release.
    async fn measured(&self) -> Result<(), ClusterLoadError> {
        self.wait_on(&self.measured, "measured").await
    }

    /// Parks until every tenant finishes its measured IO.
    ///
    /// # Errors
    /// Returns an assertion error when another tenant failed before release.
    async fn completed(&self) -> Result<(), ClusterLoadError> {
        self.wait_on(&self.completed, "completed").await
    }

    /// Races one barrier against the shared failure token.
    ///
    /// # Errors
    /// Returns an assertion error naming `phase` when the token is tripped
    /// first, which means a sibling tenant already failed and this barrier will
    /// never release on its own.
    async fn wait_on(
        &self,
        barrier: &tokio::sync::Barrier,
        phase: &'static str,
    ) -> Result<(), ClusterLoadError> {
        tokio::select! {
            _ = barrier.wait() => Ok(()),
            () = self.failed.cancelled() => Err(ClusterLoadError::Assertion(format!(
                "{phase} barrier abandoned after tenant failure: {}",
                self.failed.cause()
            ))),
        }
    }
}

/// Public-load barrier reached by every tenant before the next checkpoint.
#[derive(Clone, Copy, Debug)]
enum LoadPhase {
    /// Warmup writes have completed for every tenant.
    Warmup,
    /// Measured writes and reads have completed for every tenant.
    Measured,
}

/// Counts tenant barrier arrivals without polling production state.
///
/// Every wait here races the run's shared [`TenantFailure`] signal. Without
/// that, a tenant that returns early never marks its arrival and the owner
/// parks until the scenario deadline, which reports a timeout instead of the
/// error that actually happened.
struct PhaseProgress {
    /// Number of tenant tasks required for one phase completion.
    tenants: usize,
    /// Warmup barrier arrivals.
    warmup: AtomicUsize,
    /// Measured barrier arrivals.
    measured: AtomicUsize,
    /// Wakes the owner when any phase counter advances.
    notify: tokio::sync::Notify,
    /// Releases tenant tasks only after the warmup checkpoint is captured.
    warmup_release: tokio::sync::Notify,
    /// Persistent release state preventing a lost notification race.
    warmup_released: AtomicBool,
    /// Shared abort signal tripped by the first tenant that fails.
    failed: TenantFailure,
}

impl PhaseProgress {
    /// Construct an empty progress coordinator for a fixed tenant count.
    fn new(tenants: usize, failed: TenantFailure) -> Self {
        Self {
            tenants,
            warmup: AtomicUsize::new(0),
            measured: AtomicUsize::new(0),
            notify: tokio::sync::Notify::new(),
            warmup_release: tokio::sync::Notify::new(),
            warmup_released: AtomicBool::new(false),
            failed,
        }
    }

    /// Record one tenant's arrival at a phase barrier.
    fn mark(&self, phase: LoadPhase) {
        let counter = match phase {
            LoadPhase::Warmup => &self.warmup,
            LoadPhase::Measured => &self.measured,
        };
        counter.fetch_add(1, Ordering::AcqRel);
        self.notify.notify_waiters();
    }

    /// Wait until every tenant has arrived at the selected barrier.
    ///
    /// # Errors
    /// Returns an assertion error when a tenant failed before every arrival
    /// landed, because the remaining arrivals are then never coming.
    async fn wait_for(&self, phase: LoadPhase) -> Result<(), ClusterLoadError> {
        let counter = match phase {
            LoadPhase::Warmup => &self.warmup,
            LoadPhase::Measured => &self.measured,
        };
        loop {
            // Register the waiter BEFORE reading the counter. `notify_waiters`
            // stores no permit, so a `mark` landing between a read and a later
            // registration would be lost and this wait would never wake.
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if counter.load(Ordering::Acquire) >= self.tenants {
                return Ok(());
            }
            tokio::select! {
                () = notified => {}
                () = self.failed.cancelled() => {
                    return Err(ClusterLoadError::Assertion(format!(
                        "{phase:?} phase abandoned after tenant failure: {}",
                        self.failed.cause()
                    )));
                }
            }
        }
    }

    /// Release all tenant tasks into the measured barrier.
    fn release_warmup(&self) {
        self.warmup_released.store(true, Ordering::Release);
        self.warmup_release.notify_waiters();
    }

    /// Wait for the owner to finish the warmup telemetry checkpoint.
    ///
    /// # Errors
    /// Returns an assertion error when a tenant failed before the owner
    /// released the warmup checkpoint.
    async fn wait_warmup_release(&self) -> Result<(), ClusterLoadError> {
        loop {
            // Same pre-registration rule as `wait_for`: the persistent flag
            // only covers waiters that arrive after the release, not one that
            // reads the flag and registers around it.
            let notified = self.warmup_release.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.warmup_released.load(Ordering::Acquire) {
                return Ok(());
            }
            tokio::select! {
                () = notified => {}
                () = self.failed.cancelled() => {
                    return Err(ClusterLoadError::Assertion(format!(
                        "warmup release abandoned after tenant failure: {}",
                        self.failed.cause()
                    )));
                }
            }
        }
    }
}

/// Execute one tenant's synchronized warmup, measured, and read phases.
async fn run_tenant(context: TenantRunContext) -> Result<TenantLoadResult, ClusterLoadError> {
    let TenantRunContext {
        tenant,
        tenant_index,
        profile,
        table,
        writer,
        query,
        barriers,
        phase_progress,
    } = context;
    let mut report = TenantLoadResult {
        tenant_index,
        ..TenantLoadResult::default()
    };
    for batch in 0..profile.warmup_batches_per_tenant {
        let payload = ipc_payload(tenant, tenant_index, batch, profile.rows_per_batch)?;
        writer
            .send_frame(BifrostFrame {
                table: table.clone(),
                batch_id: deterministic_batch_id(profile.seed, tenant_index, batch),
                arrow_ipc: payload.clone().into(),
            })
            .await
            .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
        report.warmup_acknowledged_bytes = report
            .warmup_acknowledged_bytes
            .saturating_add(payload.len() as u64);
    }
    report.warmup_acknowledged_batches = profile.warmup_batches_per_tenant;
    barriers.warmup().await?;
    phase_progress.mark(LoadPhase::Warmup);
    phase_progress.wait_warmup_release().await?;
    barriers.measured().await?;
    let mut tasks = tokio::task::JoinSet::new();
    for writer_index in 0..profile.writers_per_tenant {
        let writer = writer.clone();
        let table = table.clone();
        tasks.spawn(async move {
            let mut result = TenantLoadResult::default();
            for batch in (writer_index as u32..profile.measured_batches_per_tenant)
                .step_by(profile.writers_per_tenant)
            {
                let payload = ipc_payload(tenant, tenant_index, batch + 2, profile.rows_per_batch)?;
                result.submitted_batches += 1;
                match writer
                    .send_frame(BifrostFrame {
                        table: table.clone(),
                        batch_id: deterministic_batch_id(profile.seed, tenant_index, batch + 2),
                        arrow_ipc: payload.clone().into(),
                    })
                    .await
                {
                    Ok(()) => {
                        result.acknowledged_batches += 1;
                        result.acknowledged_rows += u64::from(profile.rows_per_batch);
                        result.acknowledged_bytes += payload.len() as u64;
                    }
                    Err(error)
                        if error.to_string().contains("backpressure")
                            || error.to_string().contains("busy") =>
                    {
                        result.backpressure += 1;
                        result.rejected_batches += 1;
                    }
                    Err(error) => return Err(ClusterLoadError::Client(error.to_string())),
                }
            }
            Ok::<_, ClusterLoadError>(result)
        });
    }
    let min_reads = profile.minimum_reads_per_tenant;
    for reader_index in 0..profile.readers_per_tenant {
        let query = query.clone();
        let table = table.clone();
        tasks.spawn(async move {
            let mut result = TenantLoadResult::default();
            let target = min_reads / profile.readers_per_tenant as u32;
            let mut attempts = 0_u32;
            while result.completed_reads < target
                && attempts < target.saturating_mul(32).max(target)
            {
                attempts = attempts.saturating_add(1);
                let (sql, deadline_ms) = if reader_index % 2 == 0 {
                    (
                        format!("SELECT id, tenant, batch FROM {table} WHERE batch = 0 ORDER BY id LIMIT 1"),
                        5_000,
                    )
                } else {
                    (format!("SELECT COUNT(*) AS total FROM {table}"), 15_000)
                };
                let response = query
                    .collect_bounded(
                        &BifrostQueryRequest {
                            sql,
                            visibility: VisibilityMode::PublishedOnly,
                            freshness: FreshnessPolicy::Strict,
                            deadline_ms: Some(deadline_ms),
                        },
                        CollectedQueryLimits {
                            max_rows: usize::MAX,
                            max_encoded_bytes: 256 * 1024 * 1024,
                        },
                    )
                    .await;
                match response {
                    Ok(response) => {
                        result.completed_reads += 1;
                        result.queried_rows += response.rows as u64;
                        result.queried_bytes = result
                            .queried_bytes
                            .saturating_add(query_payload_bytes(&response)?);
                    }
                    Err(error) if is_retryable_read_error(&error) => {
                        // A silently retried read still costs a full server-side
                        // query, so it still writes a read audit. Without the code
                        // here, a high retry rate looks like an audit-count
                        // mismatch rather than the refusal it actually is.
                        tracing::warn!(
                            code = error.code(),
                            tenant = %tenant,
                            "load matrix retried a refused read"
                        );
                        result.retries += 1;
                    }
                    Err(error) => return Err(ClusterLoadError::Client(error.to_string())),
                }
            }
            if result.completed_reads < target && profile.pressured_tenant != Some(tenant_index) {
                return Err(ClusterLoadError::Assertion(format!(
                    "tenant {tenant} (index {tenant_index}) completed {} of {target} strict reads with {} retries",
                    result.completed_reads,
                    result.backpressure
                )));
            }
            Ok::<_, ClusterLoadError>(result)
        });
    }
    while let Some(result) = tasks.join_next().await {
        let result = result.map_err(|error| ClusterLoadError::Client(error.to_string()))??;
        report.submitted_batches += result.submitted_batches;
        report.acknowledged_batches += result.acknowledged_batches;
        report.acknowledged_rows += result.acknowledged_rows;
        report.queried_rows += result.queried_rows;
        report.acknowledged_bytes += result.acknowledged_bytes;
        report.rejected_batches += result.rejected_batches;
        report.queried_bytes += result.queried_bytes;
        report.completed_reads += result.completed_reads;
        report.retries += result.retries;
        report.backpressure += result.backpressure;
    }
    barriers.completed().await?;
    phase_progress.mark(LoadPhase::Measured);
    report.measured_acknowledged_batches = report.acknowledged_batches;
    report.progressed_phases = u32::from(report.warmup_acknowledged_batches > 0)
        + u32::from(report.measured_acknowledged_batches > 0);
    Ok(report)
}

/// Reports whether one failed measured read should be retried.
///
/// Matching is on stable Wyrd error codes, not on `Display` text. The prose is
/// not a contract and drifts; a query timeout under load previously fell
/// through every prose needle and failed its tenant outright, which ended the
/// whole run. Codes are the contract, so a new terminal that is not listed here
/// is treated as fatal on purpose rather than by accident.
fn is_retryable_read_error(error: &ValaSdkError) -> bool {
    [
        // Transient saturation: the server refused or could not finish in time.
        "WYRD_VALA_429_QUERY_ADMISSION_REJECTED",
        "WYRD_VALA_429_INGEST_BUSY",
        "WYRD_VALA_504_QUERY_TIMEOUT",
        "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
        // A role or source that has not converged yet on this node.
        "WYRD_VALA_503_ORACLE_ROLE_UNAVAILABLE",
        "WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE",
        "WYRD_VALA_503_QUERY_AUDIT_UNAVAILABLE",
        "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
    ]
    .contains(&error.code())
}

/// Encode one deterministic Arrow batch for public Gate ingest.
///
/// # Errors
/// Returns a client error when Arrow batch construction or IPC encoding fails.
fn ipc_payload(
    tenant: DataTenantId,
    tenant_index: usize,
    batch: u32,
    rows: u32,
) -> Result<Vec<u8>, ClusterLoadError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("tenant", DataType::Utf8, false),
        Field::new("batch", DataType::Utf8, false),
    ]));
    let ids = (0..rows)
        .map(|row| (tenant_index as i64) * 1_000_000 + (batch as i64) * 1_000 + row as i64)
        .collect::<Vec<_>>();
    let tenant_values = vec![tenant.to_string(); rows as usize];
    let batch_values = vec![batch.to_string(); rows as usize];
    let record = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids)),
            Arc::new(StringArray::from(tenant_values)),
            Arc::new(StringArray::from(batch_values)),
        ],
    )
    .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    let mut payload = Vec::new();
    let mut writer = StreamWriter::try_new(&mut payload, schema.as_ref())
        .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    writer
        .write(&record)
        .and_then(|()| writer.finish())
        .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    Ok(payload)
}

/// Re-encode a collected public result at Oracle's schema/batch IPC boundary.
///
/// Oracle records exactly the schema stream payload plus one independent IPC
/// stream per emitted batch. Reusing Arrow's deterministic writer here keeps
/// the client-side byte ledger independent of production telemetry.
///
/// # Errors
/// Returns [`ClusterLoadError::Client`] when Arrow cannot encode the decoded
/// public schema or a returned batch, or when the byte total does not fit `u64`.
fn query_payload_bytes(result: &CollectedQueryResult) -> Result<u64, ClusterLoadError> {
    let mut total = 0_usize;
    let mut schema_payload = Vec::new();
    let mut schema_writer = StreamWriter::try_new(&mut schema_payload, result.schema.as_ref())
        .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    schema_writer
        .finish()
        .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
    drop(schema_writer);
    total = total.saturating_add(schema_payload.len());
    for batch in &result.batches {
        let mut batch_payload = Vec::new();
        let mut batch_writer = StreamWriter::try_new(&mut batch_payload, batch.schema().as_ref())
            .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
        batch_writer
            .write(batch)
            .and_then(|()| batch_writer.finish())
            .map_err(|error| ClusterLoadError::Client(error.to_string()))?;
        drop(batch_writer);
        total = total.saturating_add(batch_payload.len());
    }
    u64::try_from(total).map_err(|error| ClusterLoadError::Client(error.to_string()))
}

/// Derive a stable batch identity from the matrix seed and logical coordinates.
fn deterministic_batch_id(seed: u64, tenant_index: usize, batch: u32) -> [u8; 16] {
    let millis = 1_700_000_000_000_u64
        .saturating_add((tenant_index as u64).saturating_mul(32))
        .saturating_add(u64::from(batch));
    let entropy =
        (u128::from(seed) << 64) | (u128::from(tenant_index as u64) << 32) | u128::from(batch);
    let mut bytes = [0_u8; 16];
    bytes[..6].copy_from_slice(&millis.to_be_bytes()[2..]);
    bytes[6] = 0x70 | ((entropy >> 56) as u8 & 0x0f);
    bytes[7] = (entropy >> 48) as u8;
    bytes[8] = 0x80 | ((entropy >> 56) as u8 & 0x3f);
    bytes[9..].copy_from_slice(&entropy.to_be_bytes()[9..]);
    bytes
}

/// Aggregate exact live owner state immediately before cluster shutdown.
///
/// # Errors
/// Returns a cluster or telemetry error when an owner cannot be inspected.
async fn cleanup_snapshot(
    cluster: &WyrdTestCluster,
    gate_active_streams: u64,
) -> Result<ClusterCleanupSnapshot, ClusterLoadError> {
    let inspection = cluster
        .oracle_inspection()
        .await
        .map_err(|error| ClusterLoadError::Cluster(error.to_string()))?;
    let mut queued = 0_u64;
    let mut inflight = 0_u64;
    let mut wal_streams = 0_u64;
    for server in cluster.servers() {
        if let Ok(snapshot) = server.scribe_inspection_snapshot() {
            queued += snapshot.queued_items as u64;
            wal_streams += snapshot.open_wal_stream_count as u64;
        }
        if let Some(scribe) = server.bifrost_scribe() {
            inflight = inflight.saturating_add(scribe.inflight_items_for_test() as u64);
        }
    }
    Ok(ClusterCleanupSnapshot {
        scribe_queued: queued,
        scribe_inflight: inflight,
        scribe_wal_streams: wal_streams,
        oracle_tail_fences: inspection.active_tail_fences,
        forge_active_claims: inspection.forge_active_claims,
        forge_active_attempts: inspection.forge_active_attempts,
        forge_historical_attempts: inspection.forge_historical_attempts,
        forge_terminal_tasks: inspection.forge_terminal_tasks,
        gate_active_streams,
        supervised_tasks: cluster
            .servers()
            .map(|server| server.supervised_task_count_for_test() as u64)
            .sum(),
        listeners_stopped: false,
        servers_stopped: false,
    })
}

/// Compute Jain's fairness index over acknowledged tenant work.
fn jain_fairness<I>(values: I) -> f64
where
    I: Iterator<Item = u64>,
{
    let values = values.map(|value| value as f64).collect::<Vec<_>>();
    if values.is_empty() {
        return 1.0;
    }
    let sum = values.iter().sum::<f64>();
    let square_sum = values.iter().map(|value| value * value).sum::<f64>();
    if square_sum == 0.0 {
        0.0
    } else {
        sum * sum / (values.len() as f64 * square_sum)
    }
}

/// Return the stable report label for one closed test topology.
fn topology_name(topology: BifrostTopology) -> &'static str {
    match topology {
        BifrostTopology::OnePod => "one_pod",
        BifrostTopology::TwoPod => "two_pod",
        BifrostTopology::ThreePod => "three_pod",
        BifrostTopology::RoleSeparated => "role_separated",
        BifrostTopology::SixPod => "six_pod",
        BifrostTopology::DedicatedForgeWorkers => "dedicated_forge_workers",
        BifrostTopology::ThreeServersThreeForgeWorkers => "three_servers_three_forge_workers",
    }
}

impl fmt::Debug for BifrostClusterLoad {
    /// Render only immutable profile and owner-presence state.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BifrostClusterLoad")
            .field("profile", &self.profile)
            .field("cluster_running", &self.cluster.is_some())
            .finish_non_exhaustive()
    }
}

/// Maps the closed public topology enum to its exact test process descriptor.
trait TopologySpec {
    /// Return the production-role descriptor used to boot this topology.
    fn spec_for_test(self) -> crate::bifrost::BifrostClusterSpec;
}

impl TopologySpec for BifrostTopology {
    /// Select the exact descriptor without inventing a production role.
    fn spec_for_test(self) -> crate::bifrost::BifrostClusterSpec {
        match self {
            BifrostTopology::OnePod => crate::bifrost::BifrostClusterSpec::one_mixed(),
            BifrostTopology::TwoPod => crate::bifrost::BifrostClusterSpec::two_mixed(),
            BifrostTopology::ThreePod => crate::bifrost::BifrostClusterSpec::three_mixed(),
            BifrostTopology::RoleSeparated => crate::bifrost::BifrostClusterSpec::role_separated(),
            BifrostTopology::SixPod => crate::bifrost::BifrostClusterSpec::six_capacity(),
            BifrostTopology::DedicatedForgeWorkers => {
                crate::bifrost::BifrostClusterSpec::dedicated_forge_workers()
            }
            BifrostTopology::ThreeServersThreeForgeWorkers => {
                crate::bifrost::BifrostClusterSpec::three_servers_three_forge_workers()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stall-first observation leaves the query task owned for explicit cancellation.
    #[tokio::test]
    async fn cancellation_probe_preserves_stall_first_ordering() {
        let mut query_task = tokio::spawn(std::future::pending::<Result<(), &'static str>>());
        let query_id = await_query_schema_stall_or_completion(
            async { Ok::<_, crate::WyrdTestServerError>("query-id".to_owned()) },
            &mut query_task,
        )
        .await
        .expect("schema stall wins");
        assert_eq!(query_id, "query-id");
        assert!(!query_task.is_finished());
        query_task.abort();
        let _ = query_task.await;
    }

    /// Completion-first observation retains client and join failures instead of a timeout.
    #[tokio::test]
    async fn cancellation_probe_preserves_completion_first_errors() {
        let mut client_failure = tokio::spawn(async { Err::<(), _>("original client failure") });
        let error = await_query_schema_stall_or_completion(
            std::future::pending::<Result<String, crate::WyrdTestServerError>>(),
            &mut client_failure,
        )
        .await
        .expect_err("client completion wins");
        assert!(error.to_string().contains("original client failure"));
        assert!(!error.to_string().contains("schema stall deadline elapsed"));

        let mut join_failure: tokio::task::JoinHandle<Result<(), &'static str>> =
            tokio::spawn(async {
                panic!("injected cancellation query panic");
            });
        let error = await_query_schema_stall_or_completion(
            std::future::pending::<Result<String, crate::WyrdTestServerError>>(),
            &mut join_failure,
        )
        .await
        .expect_err("join failure wins");
        assert!(error.to_string().contains("query task join failed"));
        assert!(
            error
                .to_string()
                .contains("injected cancellation query panic")
        );
    }

    /// The profile encodes D21's exact bounded writer/reader matrix.
    #[test]
    fn profile_is_exact_and_serializable() {
        let profile = ClusterLoadProfile::multi_tenant_three_server_three_worker();
        profile.validate().expect("profile validates");
        let json = serde_json::to_value(profile).expect("profile serializes");
        assert_eq!(json["tenants"], 8);
        assert_eq!(json["rows_per_batch"], 64);
        assert_eq!(json["writers_per_tenant"], 2);
        assert_eq!(json["readers_per_tenant"], 2);
        assert_eq!(json["minimum_reads_per_tenant"], 2);
        assert_eq!(
            profile.minimum_reads_per_tenant / profile.readers_per_tenant as u32,
            1
        );
        assert_eq!(
            profile.minimum_reads_per_tenant % profile.readers_per_tenant as u32,
            0
        );
    }

    /// The closed matrix profile rejects missing or mismatched reader participation.
    #[test]
    fn profile_rejects_inexact_reader_participation() {
        for minimum_reads in [0, 1, 3] {
            let mut profile = ClusterLoadProfile::multi_tenant_three_server_three_worker();
            profile.minimum_reads_per_tenant = minimum_reads;
            assert!(
                matches!(profile.validate(), Err(ClusterLoadError::Profile(_))),
                "minimum read count {minimum_reads} unexpectedly validated"
            );
        }
    }

    /// Jain fairness remains deterministic and rewards equal admitted work.
    #[test]
    fn fairness_for_equal_work_is_one() {
        assert!((jain_fairness([64, 64, 64, 64].into_iter()) - 1.0).abs() < f64::EPSILON);
    }

    /// Deterministic batch identities retain the Gate-required UUIDv7 contract.
    #[test]
    fn deterministic_batch_id_is_uuid_v7_and_stable() {
        let first = deterministic_batch_id(0xB1F0_57A5, 2, 7);
        assert_eq!(first, deterministic_batch_id(0xB1F0_57A5, 2, 7));
        let uuid = uuid::Uuid::from_bytes(first);
        assert_eq!(uuid.get_version_num(), 7);
        assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122);
    }

    /// Matrix phases require only dependency evidence guaranteed by their operations.
    #[test]
    fn phase_dependency_expectations_are_exact() {
        assert_eq!(
            WARMUP_DEPENDENCIES,
            &[
                "postgres.acquire",
                "postgres.transactions",
                "storage.bytes",
                "wal.fsync",
            ]
        );
        assert_eq!(
            MEASURED_DEPENDENCIES,
            &["postgres.acquire", "postgres.transactions", "wal.fsync"]
        );
        assert_eq!(
            PUBLICATION_DEPENDENCIES,
            &["postgres.acquire", "postgres.transactions", "storage.bytes"]
        );
        assert_eq!(
            QUERY_DEPENDENCIES,
            &["postgres.acquire", "postgres.transactions"]
        );
    }
}
