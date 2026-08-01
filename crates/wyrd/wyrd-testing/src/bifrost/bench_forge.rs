//! Standalone Forge maintenance benchmark backed only by production telemetry.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{
    Forge, ForgeError, ForgeSchedulerTrigger, ForgeWorker, ForgeWorkerCompletionObserver,
    ForgeWorkerConfig,
};
use wyrd_bench::{BifrostLane, BifrostScenario, SloGate};
use wyrd_server::{ForgeProcessRole, install_capture_runtime, start_capture_forge_role};
use wyrd_telemetry::TelemetryConfig;

use crate::bifrost::{
    ForgeMaintenanceTelemetryReport, ForgeTelemetryCapture, StandaloneForgeFixture,
};

type BenchError = Box<dyn std::error::Error + Send + Sync>;

/// The complete standalone maintenance artifact for one immutable scenario identity.
#[derive(Debug, Serialize)]
struct ForgeBenchmarkReport {
    /// Shared scenario identity and readiness classification.
    envelope: wyrd_bench::BifrostReportEnvelope,
    /// Every Forge R13 value mapped exclusively from the production exporter delta.
    maintenance: ForgeMaintenanceTelemetryReport,
    /// Passing repository-owned SLO evaluation over the production task p99.
    slo: ForgeSloEvidence,
}

/// Serializable evidence returned by the canonical repository SLO gate.
#[derive(Debug, Serialize)]
struct ForgeSloEvidence {
    /// Normalized checked-in threshold group.
    group: String,
    /// Metric name required by that threshold.
    metric: String,
    /// Production telemetry value evaluated by the gate.
    value: f64,
}

/// Real standalone scheduler/worker composition retained for one benchmark run.
struct StandaloneForgeRoles {
    /// Forge dependency graph shared by the production scheduler and workers.
    forge: Arc<Forge>,
    /// Passive wakeup for the already-running production scheduler loop.
    scheduler_trigger: ForgeSchedulerTrigger,
    /// Passive observer of successful durable worker completions.
    completion_observer: ForgeWorkerCompletionObserver,
    /// Cancellation boundary owned by the production scheduler supervisor.
    scheduler_stop: CancellationToken,
    /// Running production scheduler supervisor task.
    scheduler_task: JoinHandle<Result<(), ForgeError>>,
    /// Independently supervised production worker roles.
    workers: Vec<StandaloneForgeWorkerRole>,
    /// Immutable role counts used only to validate production topology series.
    expected: BTreeMap<String, u64>,
    /// Embedded-process lifecycle guard shared by its scheduler and worker.
    all_guard: Option<wyrd_server::app::metrics::TestForgeRoleTelemetryGuard>,
    /// Dedicated scheduler/server lifecycle guard.
    server_guard: Option<wyrd_server::app::metrics::TestForgeRoleTelemetryGuard>,
}

/// One dedicated production worker supervisor and its process-lifecycle evidence.
struct StandaloneForgeWorkerRole {
    /// Cancellation boundary for this exact worker process replacement unit.
    stop: CancellationToken,
    /// Running production worker loop.
    task: JoinHandle<Result<(), ForgeError>>,
    /// Dedicated-worker role guard; embedded execution uses the shared `all` guard.
    guard: Option<wyrd_server::app::metrics::TestForgeRoleTelemetryGuard>,
}

impl StandaloneForgeWorkerRole {
    /// Spawn one real production worker loop over the shared Forge graph.
    ///
    /// # Errors
    ///
    /// Returns invalid worker configuration before the supervisor starts.
    fn start(
        forge: Arc<Forge>,
        guard: Option<wyrd_server::app::metrics::TestForgeRoleTelemetryGuard>,
    ) -> Result<Self, BenchError> {
        let worker = ForgeWorker::new(forge, ForgeWorkerConfig::default())?;
        let stop = CancellationToken::new();
        let worker_stop = stop.clone();
        let task = tokio::spawn(async move { worker.run(worker_stop).await });
        Ok(Self { stop, task, guard })
    }

    /// Cancel and join this production worker before releasing its role gauge.
    ///
    /// # Errors
    ///
    /// Returns when the worker misses the bounded drain deadline, panics, or
    /// exits with a production worker error.
    async fn stop(self) -> Result<(), BenchError> {
        self.stop.cancel();
        let joined = tokio::time::timeout(Duration::from_secs(10), self.task)
            .await
            .map_err(|_| "standalone Forge worker missed its drain deadline")?
            .map_err(|error| format!("standalone Forge worker task panicked: {error}"))?;
        joined?;
        drop(self.guard);
        Ok(())
    }
}

impl StandaloneForgeRoles {
    /// Compose one scheduler and the configured number of real Forge workers.
    ///
    /// # Errors
    ///
    /// Returns scheduler, worker, or numeric conversion failures, including a
    /// zero-worker topology.
    fn start(
        forge: Arc<Forge>,
        pods: u32,
        scheduler_trigger: ForgeSchedulerTrigger,
        completion_observer: ForgeWorkerCompletionObserver,
    ) -> Result<Self, BenchError> {
        let worker_count = usize::try_from(pods)?;
        if worker_count == 0 {
            return Err("Forge benchmark needs at least one executed pod".into());
        }
        let scheduler_stop = CancellationToken::new();
        let scheduler_shutdown = scheduler_stop.clone();
        let scheduler_forge = Arc::clone(&forge);
        let scheduler_task =
            tokio::spawn(async move { scheduler_forge.run(scheduler_shutdown).await });
        let (expected, all_guard, server_guard) = if worker_count == 1 {
            (
                BTreeMap::from([("all".to_owned(), 1)]),
                Some(start_capture_forge_role(ForgeProcessRole::All)),
                None,
            )
        } else {
            (
                BTreeMap::from([
                    ("server".to_owned(), 1),
                    ("forge_worker".to_owned(), u64::from(pods)),
                ]),
                None,
                Some(start_capture_forge_role(ForgeProcessRole::Server)),
            )
        };
        let workers = (0..worker_count)
            .map(|_| {
                let guard = (worker_count > 1)
                    .then(|| start_capture_forge_role(ForgeProcessRole::ForgeWorker));
                StandaloneForgeWorkerRole::start(Arc::clone(&forge), guard)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            forge,
            scheduler_trigger,
            completion_observer,
            scheduler_stop,
            scheduler_task,
            workers,
            expected,
            all_guard,
            server_guard,
        })
    }

    /// Request and await one pass from the already-supervised production scheduler.
    ///
    /// # Errors
    ///
    /// Returns when the supervised scheduler misses its bounded completion deadline.
    async fn request_scheduler_pass(&self) -> Result<(), BenchError> {
        let expected = self.scheduler_trigger.completed_passes().saturating_add(1);
        self.scheduler_trigger.request_pass();
        tokio::time::timeout(
            Duration::from_secs(10),
            self.scheduler_trigger.wait_for_passes_at_least(expected),
        )
        .await?;
        Ok(())
    }

    /// Wait for an exact minimum of durable production worker completions.
    ///
    /// # Errors
    ///
    /// Returns when real workers do not finish the expected workload before
    /// the benchmark's bounded observation deadline.
    async fn wait_for_completions(&self, expected: usize) -> Result<(), BenchError> {
        tokio::time::timeout(
            Duration::from_secs(30),
            self.completion_observer.wait_for_at_least(expected),
        )
        .await
        .map_err(|_| {
            format!(
                "standalone Forge completed {} task(s), expected at least {expected}",
                self.completion_observer.completed()
            )
        })?;
        Ok(())
    }

    /// Replace one dedicated production worker after its supervisor drains.
    ///
    /// # Errors
    ///
    /// Returns a worker construction error or rejects an embedded topology,
    /// which represents its scheduler and worker through one `all` role.
    async fn replace_worker(&mut self, index: usize) -> Result<(), BenchError> {
        if self.workers.len() < 2 {
            return Err("embedded Forge topology has no dedicated worker role".into());
        }
        if index >= self.workers.len() {
            return Err("Forge replacement worker index is outside the topology".into());
        }
        self.workers.swap_remove(index).stop().await?;
        self.workers.push(StandaloneForgeWorkerRole::start(
            Arc::clone(&self.forge),
            Some(start_capture_forge_role(ForgeProcessRole::ForgeWorker)),
        )?);
        Ok(())
    }

    /// Run the benchmark workload only through supervised production loops.
    ///
    /// # Errors
    ///
    /// Returns scheduler, worker completion, or replacement lifecycle failures.
    async fn run_workload(&mut self, tenant_count: u32) -> Result<u64, BenchError> {
        let first_stage = usize::try_from(tenant_count)?;
        self.request_scheduler_pass().await?;
        self.wait_for_completions(first_stage).await?;
        if self.workers.len() > 1 {
            self.replace_worker(0).await?;
        }
        let required = first_stage
            .checked_mul(2)
            .ok_or("Forge benchmark completion target overflowed")?;
        self.request_scheduler_pass().await?;
        self.wait_for_completions(required).await?;
        Ok(u64::try_from(self.completion_observer.completed())?)
    }

    /// Borrow immutable executed-role evidence for telemetry validation.
    #[must_use]
    fn expected(&self) -> &BTreeMap<String, u64> {
        &self.expected
    }

    /// Cancel and join every scheduler/worker loop before releasing role gauges.
    ///
    /// # Errors
    ///
    /// Returns a production-loop error, panic, or bounded drain timeout.
    async fn shutdown(mut self) -> Result<(), BenchError> {
        self.scheduler_stop.cancel();
        for worker in self.workers.drain(..) {
            worker.stop().await?;
        }
        let scheduler = tokio::time::timeout(Duration::from_secs(10), self.scheduler_task)
            .await
            .map_err(|_| "standalone Forge scheduler missed its drain deadline")?
            .map_err(|error| format!("standalone Forge scheduler task panicked: {error}"))?;
        scheduler?;
        drop(self.all_guard);
        drop(self.server_guard);
        Ok(())
    }
}

/// Run one standalone Forge maintenance topology without composing a Wyrd server.
///
/// # Errors
///
/// Returns runtime, fixture, scheduler, worker, capture, mapping, or report-write
/// errors. This path never starts `WyrdTestServer`, HTTP, gRPC, or `AppState`.
pub async fn run(scenario: BifrostScenario) -> Result<(), BenchError> {
    if scenario.lane != BifrostLane::Forge {
        return Err("Forge adapter received a non-Forge scenario".into());
    }
    let (runtime, traces) = install_capture_runtime(TelemetryConfig {
        service_name: Some("wyrd-forge-benchmark".to_owned()),
        ..TelemetryConfig::default()
    })?;
    let capture = ForgeTelemetryCapture::new(runtime.prometheus(), traces);
    let checkpoint = capture.checkpoint()?;
    let fixture =
        StandaloneForgeFixture::start_topology("bifrost_bench_forge", scenario.tenants).await?;
    let sampler = capture.begin_gauge_sampling(&checkpoint).await?;
    let forge_fixture = fixture.fixture();
    for tenant_fixture in fixture.fixtures() {
        for sequence in 0_i64..8 {
            tenant_fixture
                .append_forge_file_with_rows(sequence, 10_000)
                .await;
        }
    }
    let completion_observer = ForgeWorkerCompletionObserver::new();
    let scheduler_trigger = ForgeSchedulerTrigger::new();
    let forge = forge_fixture.context_with_supervision(
        forge_fixture.config.clone(),
        completion_observer.clone(),
        scheduler_trigger.clone(),
    );
    let mut roles =
        StandaloneForgeRoles::start(forge, scenario.pods, scheduler_trigger, completion_observer)?;
    let workload = roles.run_workload(scenario.tenants).await;
    if let Err(error) = workload {
        let _ = roles.shutdown().await;
        return Err(error);
    }
    let expected_roles = roles.expected().clone();
    let delta = capture.delta_since(checkpoint, sampler).await;
    let shutdown = roles.shutdown().await;
    shutdown?;
    let delta = delta?;
    let maintenance =
        ForgeMaintenanceTelemetryReport::from_production_delta(&delta, &expected_roles)?;
    let slo = SloGate::from_default_path()?
        .check_value("bench.bifrost.forge_slo", maintenance.task_latency_p99_us)?;
    let verified = maintenance.throughput_mib_per_sec > 0.0
        && maintenance.task_latency_p99_us.is_finite()
        && maintenance.backlog_age_us.is_finite();
    let report = ForgeBenchmarkReport {
        envelope: wyrd_bench::BifrostReportEnvelope::new(
            scenario,
            super::bench_report::readiness(verified),
        ),
        maintenance,
        slo: ForgeSloEvidence {
            group: slo.group,
            metric: slo.metric,
            value: slo.value,
        },
    };
    let path = report_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    if verified {
        Ok(())
    } else {
        Err("Forge production telemetry did not satisfy finite report invariants".into())
    }
}

/// Resolve the report path selected by the registered benchmark task.
fn report_path() -> std::path::PathBuf {
    std::env::var_os("WYRD_BIFROST_REPORT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("target/bifrost-benchmarks/forge.json"))
}
