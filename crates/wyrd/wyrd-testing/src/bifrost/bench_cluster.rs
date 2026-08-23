//! Controlled full-cluster benchmark adapter and versioned v2 diagnostics.

use std::collections::{BTreeMap, BTreeSet};
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
    CLUSTER_WORKLOAD_VERSION, ClientTrialMetrics, ClusterBenchmarkScenario, ClusterBenchmarkTrial,
    ClusterTopology, EvidenceStatus, ProductionTelemetryEvidence, TenantStageRows, TrafficMix,
    TrialDistribution, jain_fairness,
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
    ClusterTelemetryEvidence, ClusterTelemetryExpectation, ClusterTelemetryProjection,
    run_sampled_window,
};
use crate::bifrost::{BifrostClusterSpec, BifrostTopology, WyrdTestCluster};

/// Stable logical table used by the controlled workload.
pub(crate) const REFERENCE_TABLE: &str = "cluster_reference_events";
/// Rows preloaded through Gate for every tenant before measured traffic.
pub(crate) const PRELOAD_ROWS: u64 = 8_192;
/// Maximum acknowledged identities assigned to one public reconciliation query.
const IDENTITY_RECONCILIATION_ROWS_PER_QUERY: usize = 1_024;
/// Rows in one public durable write request.
pub(crate) const ROWS_PER_WRITE: u32 = 64;
/// Default profile-driven concurrent public-operation cap.
const DEFAULT_MAX_IN_FLIGHT: usize = 4_096;
/// Disjoint logical row-id space reserved for each authenticated tenant.
const TENANT_ROW_STRIDE: u64 = 1_000_000_000_000;

/// Derive a fresh, per-run ingest table name from a run identifier.
///
/// The qualification ingest sweep boots its cluster over run-shared resources
/// whose Postgres catalog persists across every family cluster in the run. It
/// therefore cannot reuse the smoke-default [`REFERENCE_TABLE`], because the
/// mixed family registers that exact table over the same shared catalog and a
/// second [`crate::bifrost::WyrdTestCluster`] creating it would collide. This
/// seam yields a name that is unique to the run yet deterministic within it, so
/// the ingest sweep owns a private write target while every other family keeps
/// the smoke default. Each character outside `[a-z0-9_]` is folded to `_` so the
/// result is a valid catalog identifier for any run-id token.
#[must_use]
pub(crate) fn fresh_ingest_table(run_id: &str) -> String {
    let sanitized: String = run_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("bench_ingest_{sanitized}")
}

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

/// Convert report topology into the dependency-neutral test-cluster topology.
const fn canonical_topology(topology: ClusterTopology) -> BifrostTopology {
    match topology {
        ClusterTopology::OnePod => BifrostTopology::OnePod,
        ClusterTopology::ThreeServersThreeForgeWorkers => {
            BifrostTopology::ThreeServersThreeForgeWorkers
        }
    }
}

/// Select one deterministic interleaved traffic class from the fixed seed.
#[must_use]
fn is_write_operation(ordinal: u64, write_percent: u8) -> bool {
    (ordinal.saturating_mul(37).saturating_add(0xB1_F057) % 100) < u64::from(write_percent)
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

/// Compare an observed public-Oracle union with the acknowledged identity ledger.
#[cfg(test)]
#[must_use]
fn exact_identity_ledger_matches(expected: &[BTreeSet<u64>], observed: &[BTreeSet<u64>]) -> bool {
    expected == observed
}

/// Execute the canonical three-server, eight-tenant production-telemetry trial.
///
/// This is the journey-owned reproduction of the retired reference-trial path,
/// specialized to the fixed `balanced-three-server-three-worker-eight-tenants`
/// scenario. It boots a fresh three-server/three-forge-worker cluster, drives
/// the retained open-loop [`WindowRun`] warmup and measured windows through the
/// public SDK/Gate path, projects the canonical T17 sampled delta, reconciles it
/// into [`ProductionTelemetryEvidence`], and requires complete terminal cleanup
/// before returning. It exists solely so the
/// `three_server_cluster_reconciles_production_telemetry` journey retains its
/// exact production-telemetry assertions after the balanced reference-scenario
/// matrix and its runner machinery were deleted in T30; the inline
/// [`ReferenceScenarioDefinition`] here is that type's only remaining consumer.
///
/// # Errors
/// Returns a typed environment, cluster, public-client, protocol, telemetry,
/// correctness, audit, or cleanup failure. Partial trials are never returned;
/// any cluster resource retained after shutdown is a hard error.
pub async fn three_server_reference_trial() -> Result<
    (ClusterBenchmarkScenario, ClusterBenchmarkTrial),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let definition = ReferenceScenarioDefinition {
        id: "balanced-three-server-three-worker-eight-tenants",
        topology: ClusterTopology::ThreeServersThreeForgeWorkers,
        tenants: 8,
        traffic: TrafficMix::Balanced,
        write_percent: 50,
        read_percent: 50,
    };
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(
        BifrostClusterSpec::three_servers_three_forge_workers(),
    )
    .await?;
    let result = run_three_server_live_trial(&cluster, definition).await;
    let shutdown = cluster.shutdown_and_inspect().await;
    match (result, shutdown) {
        (Ok((scenario, mut trial)), Ok(cleanup)) => {
            trial.production.cleanup = if cleanup.listeners_stopped
                && cleanup.servers_stopped
                && cleanup.scribe_queued == 0
                && cleanup.scribe_inflight == 0
                && cleanup.scribe_wal_streams == 0
                && cleanup.forge_active_claims == 0
                && cleanup.forge_active_attempts == 0
                && cleanup.supervised_tasks == 0
            {
                EvidenceStatus::Complete
            } else {
                EvidenceStatus::Failed
            };
            if trial.production.cleanup != EvidenceStatus::Complete {
                return Err(format!(
                    "reference trial retained cluster resources after shutdown: {cleanup:?}"
                )
                .into());
            }
            Ok((scenario, trial))
        }
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(Box::new(error)),
        (Err(error), Err(shutdown)) => Err(format!("{error}; shutdown: {shutdown}").into()),
    }
}

/// Drive setup, warmup, one sampled measured window, and reconciliation while the
/// caller retains deterministic shutdown responsibility.
///
/// Specialized to the fixed three-server reference scenario: an offered rate of
/// twenty requests per second across a ten-second
/// warmup and a twenty-second measured window bounded by the default in-flight
/// cap. It preserves the exact post-warmup publication equality, wrong-tenant
/// isolation probe, sampled measured window, cumulative identity ledger, and
/// [`reconcile_production`] evidence assembly that the journey asserts.
///
/// # Errors
/// Returns a provisioning, public-workload, telemetry-projection, correctness,
/// audit, or reconciliation error. Cleanup evidence is finalized by the caller
/// after shutdown.
async fn run_three_server_live_trial(
    cluster: &WyrdTestCluster,
    definition: ReferenceScenarioDefinition,
) -> Result<
    (ClusterBenchmarkScenario, ClusterBenchmarkTrial),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let rate = 20;
    let warmup = Duration::from_secs(10);
    let measured_window = Duration::from_secs(20);
    let max_in_flight = DEFAULT_MAX_IN_FLIGHT;
    let tenants = provision_reference_tenants(cluster, definition.tenants as usize).await?;
    provision_reference_tables(cluster, &tenants).await?;
    let clients = reference_clients(cluster, &tenants).await?;
    preload_reference_rows(cluster, &tenants, &clients).await?;
    let warmup_result = WindowRun {
        cluster,
        tenants: &tenants,
        clients: &clients,
        definition,
        rate,
        duration: warmup,
        measured: false,
        max_in_flight,
        table: REFERENCE_TABLE,
        phase_ordinal_base: 1_000_000,
    }
    .run()
    .await?;
    flush_tenant_writers(cluster, &tenants).await?;
    await_forge_convergence(cluster, &tenants).await?;
    let mut expected_rows_by_tenant = preload_identity_ledger(tenants.len());
    extend_identity_ledger(
        &mut expected_rows_by_tenant,
        &warmup_result.tenant_write_ordinals,
    );
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
                rate,
                duration: measured_window,
                measured: true,
                max_in_flight,
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
        offered_requests_per_second: rate,
        warmup_seconds: warmup.as_secs().try_into()?,
        measured_seconds: measured_window.as_secs().try_into()?,
        trials: 3,
        minimum_samples: 200,
        max_in_flight,
        seed: 0xB1_F057,
    };
    Ok((
        scenario,
        ClusterBenchmarkTrial {
            trial: 1,
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
///
/// Exposed at crate scope so the fixture-driven family runners can offer public
/// writes and strict queries through the same tenant-bound identity the journey
/// uses, without reconstructing SDK clients.
#[derive(Clone)]
pub(crate) struct ReferenceClient {
    /// Public gRPC Gate writer sharing the tenant-bound client identity.
    pub(crate) writer: BifrostGrpcTransport,
    /// Public HTTP Oracle query handle sharing the tenant-bound client identity.
    pub(crate) query: QueryClient,
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
pub(crate) async fn provision_reference_tenants(
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
pub(crate) async fn provision_named_tables(
    cluster: &WyrdTestCluster,
    tenants: &[DataTenantId],
    tables: &[String],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = cluster.server(0).ok_or("reference cluster has no Server")?;
    let catalog = server
        .state()
        .bifrost_catalog()
        .expect("reference cluster server exposes its Bifrost catalog");
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
pub(crate) async fn reference_clients(
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
pub(crate) async fn preload_reference_rows(
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
pub(crate) async fn flush_tenant_writers(
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
pub(crate) async fn await_forge_convergence(
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
pub(crate) fn reference_payload(
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
pub(crate) fn tenant_row_base(tenant_index: usize) -> u64 {
    (tenant_index as u64).saturating_mul(TENANT_ROW_STRIDE)
}

/// Build a stable UUIDv7-shaped batch identity from seed, tenant, and ordinal.
pub(crate) fn deterministic_batch_id(tenant_index: usize, ordinal: u64) -> [u8; 16] {
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
pub(crate) fn is_backpressure(error: &str) -> bool {
    error.contains("backpressure")
        || error.contains("busy")
        || error.contains("admission rejected")
        || error.contains("WYRD_VALA_507_WAL_DISK_FULL")
}

/// Classify only bounded transient publication/admission responses.
pub(crate) fn is_retryable(error: &str) -> bool {
    [
        "temporarily unavailable",
        "not published",
        "freshness",
        "admission rejected",
    ]
    .iter()
    .any(|value| error.contains(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves the fresh ingest table name is namespaced, sanitized to a valid
    /// catalog identifier, deterministic per run, and distinct across runs so the
    /// qualification ingest sweep never collides on a run-shared catalog.
    #[test]
    fn fresh_ingest_table_is_sanitized_and_run_unique() {
        let name = fresh_ingest_table("Run.42_alpha-1");
        assert_eq!(name, "bench_ingest_run_42_alpha_1");
        assert!(name.chars().all(|character| character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '_'));
        assert_eq!(name, fresh_ingest_table("Run.42_alpha-1"));
        assert_ne!(
            fresh_ingest_table("run-a"),
            fresh_ingest_table("run-b"),
            "distinct run ids must yield distinct tables"
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

    /// Proves the reference trial's measured correctness read occurs only after
    /// its sampled telemetry and audit checkpoints have completed.
    #[test]
    fn correctness_reads_follow_sampled_window_and_audit_checkpoint() {
        let source = include_str!("bench_cluster.rs");
        let live_start = source
            .find("async fn run_three_server_live_trial(")
            .expect("reference trial");
        let live_end = source[live_start..]
            .find("struct WindowResult")
            .map(|offset| live_start + offset)
            .expect("reference trial end");
        let live = &source[live_start..live_end];
        assert!(
            live.find("let audit_after = audit_rows").unwrap()
                < live.find(".await?;\n    extend_identity_ledger").unwrap()
        );
        assert!(
            live.find(".await?;\n    extend_identity_ledger").unwrap()
                < live.find("let published_after_identities").unwrap()
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
}
