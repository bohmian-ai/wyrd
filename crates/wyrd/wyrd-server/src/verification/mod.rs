//! The supervised verification runtime: scheduler, Verifier runner, Drift
//! baseline fitter, and Operator delivery worker.
//!
//! [`VerificationRuntime`] is the one owner `BoundServer::run` spawns. It
//! composes up to four capability tasks, restarts any that exits or panics
//! while the server is running, publishes their liveness through
//! [`VerificationHealth`], and on shutdown lets each drain before returning.
//! The durable queue in `wyrd.verifier_runs` is the only record of work; the
//! runtime holds no process-local registry, so any process may crash and
//! another reclaims its leases.

mod claims;
pub mod drift;
pub mod engines;
pub mod eval;
pub mod fitter;
pub mod health;
pub mod observations;
pub mod operators;
pub mod permits;
pub mod publisher;
pub mod results;
pub mod runner;
pub mod scheduler;
pub mod telemetry;

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::task::{Id, JoinSet};
use tokio_util::sync::CancellationToken;
use wyrd_sql::queries::verifier_runs::VerifierRunQueue;

use crate::state::AppState;

use self::drift::DriftEngine;
use self::fitter::BaselineFitter;
use self::health::{RuntimeCapability, VerificationHealth};
use self::operators::{OperatorDelivery, OperatorWorker, ProviderEndpoints};
#[cfg(feature = "test-support")]
use self::publisher::PublicationFault;
use self::publisher::ResultPublisher;
#[cfg(feature = "test-support")]
use self::runner::EngineScript;
use self::runner::{VerifierEngines, VerifierRunner};
use self::scheduler::VerificationScheduler;

/// Bounds every runtime loop, lease, and drain obeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLimits {
    /// Process-wide ceiling of concurrent Operator deliveries.
    pub operator_global_permits: usize,
    /// Ceiling of concurrent Operator deliveries for one tenant.
    pub operator_tenant_permits: usize,
    /// How long one claim holds a run before another process may reclaim it.
    ///
    /// Must exceed `execution_timeout + publication_timeout`, so a live
    /// attempt always settles before its lease can be reclaimed.
    pub lease: Duration,
    /// Deadline of one engine execution; exceeding it settles `timed_out`.
    pub execution_timeout: Duration,
    /// Deadline of one result publication attempt; exceeding it retries.
    pub publication_timeout: Duration,
    /// How long shutdown waits for in-flight runs and baseline fits before
    /// releasing them.
    pub drain_grace: Duration,
    /// Idle wait between scheduler passes and between empty claim rounds.
    pub poll_interval: Duration,
    /// Wait before a crashed capability task is restarted.
    pub restart_backoff: Duration,
    /// Wait before an Eval run whose trace has not landed is tried again.
    pub trace_poll: Duration,
    /// How long after its creation an Eval run waits for its trace before it
    /// settles `timed_out`.
    pub trace_deadline: Duration,
    /// Delivery attempts one Operator dispatch may make.
    pub operator_attempts: i32,
    /// Wall-clock budget of one Operator dispatch from its creation.
    pub operator_deadline: Duration,
    /// How long one Operator claim holds a dispatch; exceeds the attempt
    /// timeout so a live attempt settles before reclaim.
    pub operator_lease: Duration,
    /// Ceiling of one Operator delivery attempt.
    pub operator_attempt_timeout: Duration,
    /// Server backoff after the first and every later failed Operator
    /// attempt.
    pub operator_backoff: [Duration; 2],
    /// Wait between Operator key-rewrap passes; rewrap runs beside, never
    /// inside, the delivery claim loop.
    pub rewrap_interval: Duration,
    /// Elapsed-time ceiling of one cross-tenant rewrap pass.
    pub rewrap_pass_budget: Duration,
    /// Elapsed-time ceiling of one tenant's rewrap transaction.
    pub rewrap_tenant_budget: Duration,
}

impl Default for RuntimeLimits {
    /// Production bounds: 16 process-wide and 4 per-tenant Operator deliveries, a ten-minute
    /// lease over a five-minute execution and one-minute publication, a
    /// thirty-second drain, Eval traces polled every five seconds for up to
    /// five minutes, and a five-minute rewrap interval whose passes stop after
    /// two minutes and thirty seconds per tenant.
    fn default() -> Self {
        Self {
            operator_global_permits: 16,
            operator_tenant_permits: 4,
            lease: Duration::from_secs(600),
            execution_timeout: Duration::from_secs(300),
            publication_timeout: Duration::from_secs(60),
            drain_grace: Duration::from_secs(30),
            poll_interval: Duration::from_secs(1),
            restart_backoff: Duration::from_secs(1),
            trace_poll: Duration::from_secs(5),
            trace_deadline: Duration::from_secs(300),
            operator_attempts: 3,
            operator_deadline: Duration::from_secs(300),
            operator_lease: Duration::from_secs(45),
            operator_attempt_timeout: Duration::from_secs(30),
            operator_backoff: [Duration::from_secs(30), Duration::from_secs(120)],
            rewrap_interval: Duration::from_secs(300),
            rewrap_pass_budget: Duration::from_secs(120),
            rewrap_tenant_budget: Duration::from_secs(30),
        }
    }
}

impl RuntimeLimits {
    /// These limits with the drain clipped to fit inside the server's own
    /// shutdown budget, leaving one second for settlement and the rest of the
    /// teardown.
    #[must_use]
    pub fn within_server_drain(mut self, server_drain: Duration) -> Self {
        self.drain_grace = self
            .drain_grace
            .min(server_drain.saturating_sub(Duration::from_secs(1)));
        self
    }

    /// Server backoff after failed Operator attempt number `attempt`
    /// (1-based): the first entry after attempt one, the second afterwards.
    #[must_use]
    pub fn operator_backoff(&self, attempt: i32) -> Duration {
        self.operator_backoff[usize::from(attempt > 1)]
    }
}

/// Test-only switch that panics one capability task at its next loop turn.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Default)]
pub struct CapabilityCrash {
    /// One armed flag per capability slot.
    armed: Arc<[AtomicBool; 4]>,
}

#[cfg(feature = "test-support")]
impl CapabilityCrash {
    /// Panic `capability`'s task at its next loop turn.
    pub fn crash_next(&self, capability: RuntimeCapability) {
        self.armed[capability.index()].store(true, Ordering::SeqCst);
    }

    /// Panic when a crash is armed for `capability`, disarming it.
    ///
    /// # Panics
    /// Panics exactly when a test armed a crash for `capability`.
    pub(crate) fn check(&self, capability: RuntimeCapability) {
        let armed = self.armed[capability.index()].swap(false, Ordering::SeqCst);
        assert!(!armed, "test-injected {} crash", capability.as_str());
    }
}

/// One composed capability and the shared owner its task runs.
#[derive(Clone)]
enum Capability {
    /// The binding scheduler.
    Scheduler(Arc<VerificationScheduler>),
    /// The Verifier runner.
    Runner(Arc<VerifierRunner>),
    /// The Drift baseline fitter.
    Fitter(Arc<BaselineFitter>),
    /// The Operator delivery worker.
    OperatorWorker(Arc<OperatorWorker>),
}

impl Capability {
    /// The health slot this capability occupies.
    const fn slot(&self) -> RuntimeCapability {
        match self {
            Self::Scheduler(_) => RuntimeCapability::Scheduler,
            Self::Runner(_) => RuntimeCapability::Runner,
            Self::Fitter(_) => RuntimeCapability::Fitter,
            Self::OperatorWorker(_) => RuntimeCapability::OperatorWorker,
        }
    }

    /// Spawn this capability's task into `tasks`, returning its task ID.
    fn spawn(&self, tasks: &mut JoinSet<()>, stop: &CancellationToken) -> Id {
        let stop = stop.clone();
        let handle = match self {
            Self::Scheduler(scheduler) => tasks.spawn(Arc::clone(scheduler).run(stop)),
            Self::Runner(runner) => tasks.spawn(Arc::clone(runner).run(stop)),
            Self::Fitter(fitter) => tasks.spawn(Arc::clone(fitter).run(stop)),
            Self::OperatorWorker(worker) => tasks.spawn(Arc::clone(worker).run(stop)),
        };
        handle.id()
    }
}

/// The supervised owner of every verification capability in this process.
pub struct VerificationRuntime {
    /// Composed capabilities; an absent capability is not required.
    capabilities: Vec<Capability>,
    /// Shared liveness the readiness loop reads.
    health: Arc<VerificationHealth>,
    /// Wait before restarting a crashed capability.
    restart_backoff: Duration,
}

impl VerificationRuntime {
    /// Start composing a runtime from `state`.
    #[must_use]
    pub fn builder(state: &AppState) -> VerificationRuntimeBuilder<'_> {
        VerificationRuntimeBuilder {
            state,
            limits: RuntimeLimits::default(),
            providers: None,
            ingest_endpoint: None,
            endpoints: ProviderEndpoints::default(),
            #[cfg(feature = "test-support")]
            publication_fault: None,
            #[cfg(feature = "test-support")]
            engine_script: None,
            #[cfg(feature = "test-support")]
            crash: None,
        }
    }

    /// Whether `capability` was composed into this runtime.
    #[must_use]
    pub fn composes(&self, capability: RuntimeCapability) -> bool {
        self.capabilities
            .iter()
            .any(|composed| composed.slot() == capability)
    }

    /// Supervise every composed capability until `shutdown` is cancelled.
    ///
    /// Each capability runs as its own task. A task that returns or panics
    /// while `shutdown` is live marks its slot down (degrading health), waits
    /// the restart backoff, and is spawned again from the same owner. Once
    /// `shutdown` fires every task drains on its own terms (the runner waits
    /// for in-flight runs up to its drain grace, then releases them) and this
    /// returns after the last one exits.
    ///
    /// # Cancellation
    /// Dropping this future aborts every capability task; claimed runs keep
    /// their leases until they expire and another process reclaims them.
    pub async fn run(self, shutdown: CancellationToken) {
        let mut tasks = JoinSet::new();
        let mut slots: Vec<(Id, Capability)> = Vec::with_capacity(self.capabilities.len());
        for capability in &self.capabilities {
            slots.push((capability.spawn(&mut tasks, &shutdown), capability.clone()));
            self.health.set_up(capability.slot(), true);
        }
        while let Some(exit) = tasks.join_next_with_id().await {
            let id = match &exit {
                Ok((id, ())) => *id,
                Err(error) => error.id(),
            };
            let Some(position) = slots.iter().position(|(slot, _)| *slot == id) else {
                continue;
            };
            let (_, capability) = slots.swap_remove(position);
            self.health.set_up(capability.slot(), false);
            if shutdown.is_cancelled() {
                continue;
            }
            if let Err(error) = &exit {
                tracing::error!(capability = capability.slot().as_str(), %error, "verification capability crashed");
            } else {
                tracing::error!(
                    capability = capability.slot().as_str(),
                    "verification capability exited while the server runs"
                );
            }
            metrics::counter!(
                crate::app::metrics::VERIFICATION_CAPABILITY_RESTARTS_TOTAL,
                "capability" => capability.slot().as_str()
            )
            .increment(1);
            tokio::select! {
                () = shutdown.cancelled() => continue,
                () = tokio::time::sleep(self.restart_backoff) => {}
            }
            slots.push((capability.spawn(&mut tasks, &shutdown), capability.clone()));
            self.health.set_up(capability.slot(), true);
        }
    }
}

/// Composes a [`VerificationRuntime`] from server state and wiring choices.
pub struct VerificationRuntimeBuilder<'a> {
    /// Server state supplying the Wyrd Postgres owner, the operator pool,
    /// the token issuer, and health.
    state: &'a AppState,
    /// Runtime bounds.
    limits: RuntimeLimits,
    /// Model providers Eval judges call; the process default when `None`.
    providers: Option<Arc<skald_runtime::ProviderRegistry>>,
    /// Scribe-bearing gRPC endpoint results are published through.
    ingest_endpoint: Option<String>,
    /// Slack and PagerDuty endpoints Operators deliver to.
    endpoints: ProviderEndpoints,
    /// Test-only publication faults.
    #[cfg(feature = "test-support")]
    publication_fault: Option<PublicationFault>,
    /// Test-only scripted engine outcomes.
    #[cfg(feature = "test-support")]
    engine_script: Option<EngineScript>,
    /// Test-only capability crash switch.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
}

impl VerificationRuntimeBuilder<'_> {
    /// Use `limits` instead of the production defaults.
    #[must_use]
    pub fn limits(mut self, limits: RuntimeLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Judge Eval runs through `providers` instead of the process default
    /// registry built from the environment.
    #[must_use]
    pub fn providers(mut self, providers: Arc<skald_runtime::ProviderRegistry>) -> Self {
        self.providers = Some(providers);
        self
    }

    /// Publish results through `endpoint`, a Scribe-bearing gRPC URL.
    #[must_use]
    pub fn ingest_endpoint(mut self, endpoint: String) -> Self {
        self.ingest_endpoint = Some(endpoint);
        self
    }

    /// Publish results through this process's own gRPC listener at `addr`.
    ///
    /// Applies only when no explicit endpoint was configured and this process
    /// hosts a Scribe; the listener is always plaintext because public TLS
    /// terminates at the edge. Otherwise the runner stays uncomposed unless an
    /// explicit endpoint is set.
    #[must_use]
    pub fn local_ingest(mut self, addr: Option<SocketAddr>) -> Self {
        if self.ingest_endpoint.is_none()
            && self.state.bifrost_ingest().is_some()
            && let Some(mut addr) = addr
        {
            if addr.ip().is_unspecified() {
                addr.set_ip(if addr.is_ipv4() {
                    Ipv4Addr::LOCALHOST.into()
                } else {
                    Ipv6Addr::LOCALHOST.into()
                });
            }
            self.ingest_endpoint = Some(format!("http://{addr}"));
        }
        self
    }

    /// Deliver fixed-provider Operators to `endpoints` (mock providers).
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn provider_endpoints(mut self, endpoints: ProviderEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    /// Inject test-only publication faults.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn publication_fault(mut self, fault: PublicationFault) -> Self {
        self.publication_fault = Some(fault);
        self
    }

    /// Consult `script` before the real engine arms.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn engine_script(mut self, script: EngineScript) -> Self {
        self.engine_script = Some(script);
        self
    }

    /// Let `crash` panic capability tasks.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn crash_switch(mut self, crash: CapabilityCrash) -> Self {
        self.crash = Some(crash);
        self
    }

    /// Compose the runtime, or `None` when this process cannot run any
    /// capability.
    ///
    /// The scheduler and the Drift baseline fitter need the operator pool;
    /// the fitter also reads Data Card artifacts from server storage. The
    /// runner additionally needs the tenant token issuer and an ingest
    /// endpoint, and reads Drift observations through the ordinary query
    /// service, local or peer-forwarded; without the issuer or endpoint it is not
    /// composed and therefore not required, so health is not degraded by an
    /// intentionally absent capability. Every composed capability is marked
    /// required on the shared health.
    #[must_use]
    pub fn build(self) -> Option<VerificationRuntime> {
        let Some(operator) = self.state.postgres.operator_pool() else {
            tracing::warn!("verification runtime not composed: no operator pool");
            return None;
        };
        let postgres = self.state.postgres.wyrd();
        let queue = VerifierRunQueue::default();
        let scheduler = VerificationScheduler::new(
            postgres.clone(),
            operator.clone(),
            queue,
            self.limits.poll_interval,
        );
        #[cfg(feature = "test-support")]
        let scheduler = match &self.crash {
            Some(crash) => scheduler.with_crash(crash.clone()),
            None => scheduler,
        };
        let fitter = BaselineFitter::new(
            postgres.clone(),
            operator.clone(),
            Arc::clone(&self.state.storage),
            &self.limits,
        );
        #[cfg(feature = "test-support")]
        let fitter = match &self.crash {
            Some(crash) => fitter.with_crash(crash.clone()),
            None => fitter,
        };
        let worker = OperatorWorker::new(
            postgres.clone(),
            operator.clone(),
            Arc::clone(&self.state.operator_keys),
            OperatorDelivery::new(
                self.state.deployment_profile.screened_http(),
                self.endpoints,
            ),
            self.limits,
        );
        #[cfg(feature = "test-support")]
        let worker = match &self.crash {
            Some(crash) => worker.with_crash(crash.clone()),
            None => worker,
        };
        let mut capabilities = vec![
            Capability::Scheduler(Arc::new(scheduler)),
            Capability::Fitter(Arc::new(fitter)),
            Capability::OperatorWorker(Arc::new(worker)),
        ];
        match (self.state.auth.tenant_issuer(), self.ingest_endpoint) {
            (Some(issuer), Some(endpoint)) => {
                let drift = DriftEngine::new(
                    self.state.clone(),
                    issuer.clone(),
                    self.limits.execution_timeout,
                );
                let publisher = ResultPublisher::new(postgres.clone(), issuer, endpoint);
                #[cfg(feature = "test-support")]
                let publisher = match self.publication_fault {
                    Some(fault) => publisher.with_fault(fault),
                    None => publisher,
                };
                let runner = VerifierRunner::new(
                    postgres.clone(),
                    operator,
                    queue,
                    publisher,
                    VerifierEngines::new(
                        drift,
                        self::eval::EvalEngine::new(
                            self.state.clone(),
                            self.providers
                                .unwrap_or_else(skald_runtime::default_registry),
                            self.limits.trace_deadline,
                        ),
                    ),
                    self.limits,
                );
                #[cfg(feature = "test-support")]
                let runner = match self.engine_script {
                    Some(script) => runner.with_engine_script(script),
                    None => runner,
                };
                #[cfg(feature = "test-support")]
                let runner = match self.crash {
                    Some(crash) => runner.with_crash(crash),
                    None => runner,
                };
                capabilities.push(Capability::Runner(Arc::new(runner)));
            }
            (None, _) => {
                tracing::warn!("Verifier runner not composed: no tenant token issuer");
            }
            (_, None) => {
                tracing::warn!(
                    "Verifier runner not composed: no Scribe-bearing ingest endpoint; set verification.ingest_endpoint"
                );
            }
        }
        let health = Arc::clone(&self.state.verification);
        for capability in &capabilities {
            health.require(capability.slot());
        }
        Some(VerificationRuntime {
            capabilities,
            health,
            restart_backoff: self.limits.restart_backoff,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::RuntimeLimits;
    use crate::config::WyrdServerConfig;

    /// The default server shutdown budget leaves the runtime its full
    /// 30-second in-flight drain, while a shorter operator budget still clips
    /// it so released leases settle before teardown.
    #[test]
    fn default_server_budget_keeps_the_thirty_second_drain() {
        let server = Duration::from_millis(WyrdServerConfig::default().shutdown.drain_ms);
        assert_eq!(
            RuntimeLimits::default()
                .within_server_drain(server)
                .drain_grace,
            Duration::from_secs(30)
        );
        assert_eq!(
            RuntimeLimits::default()
                .within_server_drain(Duration::from_secs(15))
                .drain_grace,
            Duration::from_secs(14)
        );
    }

    /// Production defaults retain 16 process-wide and 4 per-tenant Operator
    /// deliveries, a 30-second drain, and Operator
    /// dispatches of three attempts, each capped at 30 seconds, retried after
    /// 30 seconds and then two minutes, inside a five-minute deadline.
    #[test]
    fn production_defaults_are_the_specified_ceilings() {
        let limits = RuntimeLimits::default();
        assert_eq!(
            (
                limits.operator_global_permits,
                limits.operator_tenant_permits
            ),
            (16, 4)
        );
        assert_eq!(limits.drain_grace, Duration::from_secs(30));
        assert_eq!(limits.operator_attempts, 3);
        assert_eq!(limits.operator_attempt_timeout, Duration::from_secs(30));
        assert_eq!(limits.operator_deadline, Duration::from_secs(300));
        assert_eq!(
            [1, 2, 3].map(|attempt| limits.operator_backoff(attempt)),
            [
                Duration::from_secs(30),
                Duration::from_secs(120),
                Duration::from_secs(120)
            ]
        );
        assert!(
            limits.operator_lease > limits.operator_attempt_timeout,
            "a live attempt settles before its lease can be reclaimed"
        );
    }
}
