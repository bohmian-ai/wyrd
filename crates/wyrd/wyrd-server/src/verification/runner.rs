//! The generic Verifier runner: claim, execute, stage, settle.
//!
//! [`VerifierRunner`] claims and starts executions without execution-count
//! permits. The claim transaction also resolves the execution's target
//! through [`VerifierCardCache::resolve`](super::cache::VerifierCardCache::resolve), the one read route direct verify
//! and acceptance share, on the same connection, so an execution holds a
//! connection only to claim, settle, and renew. Every execution runs through the one
//! building-block engine [`HostedBlocks`] builds, as the authority accepted
//! with it and executed by the tenant SYSTEM principal; a scheduled window
//! first scores its drift requirements through [`DriftEngine`]. Each attempt
//! ends in exactly one fenced transition the runner applies itself. A
//! completed outcome becomes one result payload, staged on an outbox slot
//! reserved before the engine ran (a full reserved share defers the
//! execution instead) on the process
//! [`ScribeOutbox`] attributed to the execution's subject and SYSTEM inside
//! the fenced transaction that completes the execution, so a visible
//! completion always has its result staged; delivery belongs to the outbox
//! and never re-executes the execution. A retryable failure is retried within
//! the attempt budget; a terminal failure is terminated without a verdict;
//! work abandoned by shutdown is released with its attempt refunded; and work
//! whose lease is lost stops without settling. While an execution is in
//! flight its lease is renewed by [`LeaseRenewal`]. Claims, renewals, and
//! settlements are engine mechanics, not authorization decisions, so none of
//! them writes audit.

use std::collections::BTreeMap;
#[cfg(feature = "test-support")]
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tokio::sync::Semaphore;
#[cfg(feature = "test-support")]
use tokio::sync::watch::Sender;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use tracing::field::Empty;
use vala_verify::blocks::{DriftChecks, EngineError};
use wyrd_runtime::outbox::Reservation;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, PrincipalId};
use wyrd_spec::card::operator::DriftCounts;
use wyrd_spec::card::verifier::VerifierSpec;
use wyrd_spec::ids::VerificationResultId;
use wyrd_spec::verification::VerifierReadiness;
use wyrd_spec::verification::execution::VerificationOutcome;
use wyrd_spec::verification::{VerificationError, VerificationVerdict};
use wyrd_sql::queries::verifier_executions::{
    ClaimedExecution, Completion, EnqueueRefusal, ExecutionInput, RetryOutcome, TerminalStatus,
    VerifierExecutionQueue,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
use super::cache::{Db, Target};
use super::claims::{ClaimLoop, LeasedWork, settled};
use super::completions::{self, Completions, SharedResult};
use super::drift::DriftEngine;
use super::engines::{self, EngineOutcome};
use super::health::RuntimeCapability;
use super::hosted::blocks::{Execution, HostedBlocks};
use super::leases::LeaseRenewal;
use super::results::{ResultPayloadBuilder, ResultRun};
use super::telemetry::{ExecutionMode, ExecutionTelemetry, Phase};
use crate::components::auth::Caller;
use crate::components::gateway::GatewayCallContext;
use crate::scribe_outbox::{ScribeOutbox, ScribeSink, ScribeWrite};
use crate::state::AppState;

/// A completed execution's result write and the outbox slot reserved for it
/// before the engine ran.
pub(super) type StagedResult = (Reservation<ScribeSink>, ScribeWrite);

/// Stable error code when the exact Verifier Card cannot be loaded or is not
/// a Verifier.
pub const VERIFIER_UNAVAILABLE: &str = "verifier_unavailable";
/// Stable error code when an execution exceeded its deadline.
pub const EXECUTION_TIMED_OUT: &str = "execution_timed_out";
/// Stable error code when the Verifier's step graph cannot be planned.
pub const VERIFIER_INVALID: &str = "verifier_invalid";
/// Stable error code when a block task stopped unexpectedly.
pub const EXECUTION_INTERRUPTED: &str = "execution_interrupted";
/// Stable error code when a completed result could not be encoded.
pub const RESULT_INVALID: &str = "result_invalid";
/// Stable error code when a block was refused process capacity, so the
/// execution is deferred.
pub const RUNTIME_BUSY: &str = "verification_runtime_busy";
/// Stable error code when the submitted evidence does not conform to the
/// Verifier's input slots or exceeds an input bound, so no block ran.
pub const INPUT_INVALID: &str = "input_invalid";
/// Stable error code when the evidence metadata admission needs could not
/// be read, so no block ran.
pub const EVIDENCE_UNAVAILABLE: &str = "evidence_unavailable";
/// Stable error code when the tenant has no active `UUIDv7` SYSTEM principal
/// to execute and attribute the result as.
pub const SYSTEM_PRINCIPAL_MISSING: &str = "system_principal_missing";

/// First wait before a failed settlement is tried again.
const PERSIST_INITIAL: Duration = Duration::from_millis(50);
/// Longest wait between settlement attempts.
const PERSIST_MAX: Duration = Duration::from_secs(5);

/// The single transition one claimed execution ends in.
#[derive(Debug, Clone, PartialEq)]
pub enum Transition {
    /// The result was built; settle `completed` and stage it before commit.
    Complete {
        /// The staged result.
        result_id: VerificationResultId,
        /// Its verdict.
        verdict: VerificationVerdict,
        /// Bounded human-readable summary frozen into failure dispatches.
        summary: String,
        /// The one drift requirement's feature counts frozen into failure
        /// dispatches, when the outcome has exactly one.
        drift: Option<DriftCounts>,
    },
    /// Try the same execution and input again within its attempt budget.
    Retry(VerificationError),
    /// Settle without a verdict.
    Terminate(TerminalStatus, VerificationError),
    /// Return the run to its queue with its attempt refunded.
    Release,
    /// Return the run to its queue with its attempt refunded, due after one
    /// poll interval, because an input read or a hosted block met admission
    /// backpressure.
    Defer(VerificationError),
    /// The lease is no longer held; another claimant owns the run, so nothing
    /// is settled.
    LeaseLost,
}

/// One claimed execution, its resolved Verifier, and the attempt span opened
/// before its claim.
///
/// The `verification.attempt` span is created before the claim statement
/// runs and closes when the attempt settles, so one trace covers claim,
/// Verifier resolution, evidence read, engine execution, result staging, and
/// settlement. It carries only scrubbed identifiers and
/// bounded labels.
pub struct AttemptClaim {
    /// The claimed execution.
    run: ClaimedExecution,
    /// The exact Verifier, or the error a run without one terminates with.
    verifier: Result<Arc<Target>, VerificationError>,
    /// When the claim transaction started and finished resolving the
    /// Verifier, recorded as the attempt's `load` phase.
    loaded: (Instant, Instant),
    /// The attempt's root span.
    span: tracing::Span,
}

/// Owner of claiming, executing, staging, and settling Verifier executions.
pub struct VerifierRunner {
    /// Server state every engine reads Cards, evidence, and traces through.
    state: AppState,
    /// The process's hosted block capacity every engine is built over.
    blocks: Arc<HostedBlocks>,
    /// Wyrd Postgres owner that opens every tenant-scoped claim and
    /// settlement transaction.
    postgres: WyrdPostgres,
    /// Operator pool for the cross-tenant runnable list.
    operator: OperatorPool,
    /// Queue policies and transitions.
    queue: VerifierExecutionQueue,
    /// Shared claim loop owning durable claims, shutdown, and drain.
    claims: ClaimLoop,
    /// The process's Scribe outbox every result is staged on.
    outbox: Arc<ScribeOutbox>,
    /// Scorer of a scheduled window's drift requirements.
    drift: DriftEngine,
    /// Renewal of every in-flight run's lease.
    renewal: Arc<LeaseRenewal>,
    /// Group commit of completed runs.
    completions: Completions,
    /// Runtime bounds.
    limits: RuntimeLimits,
    /// Test-only scripted engine outcomes.
    #[cfg(feature = "test-support")]
    script: Option<EngineScript>,
}

impl VerifierRunner {
    /// Tie the not-yet-started attempt `span` of a manual `run` to the
    /// request that enqueued it.
    ///
    /// The span links to the request's span, carrying the caller's own
    /// sampled flag, and starts under this server's tenant span sink, so a
    /// sampled caller's attempt and its catalog descendants reach the
    /// tenant's trace copy as a trace linked to the request, while the
    /// operator samples the attempt by its own policy. The attempt is marked
    /// continued, so a gateway call inside it captures its attempt spans in
    /// the attempt's trace. Scheduled runs have no cause and stay out of the
    /// tenant copy. This must run before the span is first entered;
    /// afterwards it does nothing.
    fn trace(&self, span: &tracing::Span, run: &ClaimedExecution) {
        let Some(cause) = &run.cause else {
            return;
        };
        wyrd_telemetry::serve_linked_work(
            span,
            &cause.trace_id,
            &cause.span_id,
            cause.sampled,
            self.state.bifrost.tenant_span_sink().as_ref(),
        );
    }

    /// Build a runner over `state`'s Wyrd Postgres owner, the operator pool,
    /// and the hosted `blocks`, each running execution holding one permit of
    /// `capacity`, the execution capacity group snapshot handoff shares.
    ///
    /// The runner carries no coordination clock: queue availability, claims,
    /// leases, renewals, retries, and settlements are decided and stamped by
    /// PostgreSQL. It keeps [`Instant`] for its own elapsed-time telemetry and
    /// deadlines and the producer's wall clock only for the event facts a
    /// result carries.
    #[must_use]
    pub fn new(
        state: AppState,
        blocks: Arc<HostedBlocks>,
        operator: OperatorPool,
        queue: VerifierExecutionQueue,
        outbox: Arc<ScribeOutbox>,
        capacity: Arc<Semaphore>,
        limits: RuntimeLimits,
    ) -> Self {
        let postgres = state.postgres.wyrd().clone();
        Self {
            claims: ClaimLoop::new(
                postgres.clone(),
                None,
                capacity,
                Some(state.verification.submissions().sink().accepted()),
                &limits,
            ),
            renewal: Arc::new(LeaseRenewal::new(postgres.clone(), queue, limits.lease)),
            completions: Completions::new(&postgres, queue),
            drift: DriftEngine::new(state.clone(), limits.execution_timeout),
            state,
            blocks,
            postgres,
            operator,
            queue,
            outbox,
            limits,
            #[cfg(feature = "test-support")]
            script: None,
        }
    }

    /// Consult `script` before the real engine.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_engine_script(mut self, script: EngineScript) -> Self {
        self.script = Some(script);
        self
    }

    /// Let `crash` panic this runner's loop.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_crash(mut self, crash: CapabilityCrash) -> Self {
        self.claims.arm_crash(crash);
        self
    }

    /// Claim and run executions until `stop` is cancelled, then drain.
    ///
    /// Delegates to the shared [`ClaimLoop`]: each turn claims one execution per
    /// due tenant, most overdue first, and spawns its execution; on
    /// `stop` no further claim is admitted (an uncommitted claim rolls back
    /// and a claim committed after `stop` is released unexecuted), executions
    /// spawned before `stop` get the drain grace to settle, and any still
    /// running are then cancelled and release their leases before this
    /// returns. Lease renewal runs beside the loop and stops once it returns.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed a runner crash. The
    /// in-flight executions are aborted with the task and keep their leases
    /// until expiry.
    pub async fn run(self: Arc<Self>, stop: CancellationToken) {
        let renewing = CancellationToken::new();
        let claims = async {
            self.claims.run(&self, stop).await;
            renewing.cancel();
        };
        tokio::join!(claims, self.renewal.run(renewing.clone()));
    }

    /// Resolve the claimed execution's target inside the claim transaction,
    /// through the one read route every Verifier execution shares.
    ///
    /// The target is rechecked here because its Cards can change while the
    /// execution waits in the queue. A refused target resolves to the error
    /// the execution terminates with: [`VERIFIER_UNAVAILABLE`] for a Verifier
    /// that is gone, inactive, or not a Verifier, otherwise the refusal's own
    /// public error, as acceptance would have settled it.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the target read fails, which aborts the
    /// claim transaction so the claim rolls back without consuming an
    /// attempt.
    async fn resolve(
        &self,
        conn: &mut TenantConn<'_>,
        run: &ClaimedExecution,
    ) -> Result<Result<Arc<Target>, VerificationError>, SqlError> {
        let tenant = conn.data_tenant_id();
        let resolved = self
            .state
            .verification
            .verifier_cards()
            .resolve(
                Db::Conn(conn.transaction()),
                tenant,
                &run.verifier_card_uid,
                &run.card_uid,
            )
            .await?;
        Ok(resolved.map_err(|refusal| {
            tracing::warn!(execution_id = %run.lease.execution_id, ?refusal, "the execution's target cannot run");
            match refusal {
                EnqueueRefusal::NotReady(VerifierReadiness::VerifierUnavailable) => {
                    failure(VERIFIER_UNAVAILABLE, "the Verifier Card cannot be loaded")
                }
                refusal => refusal.error(),
            }
        }))
    }

    /// Run the execution and map the attempt to a transition.
    ///
    /// An execution without a loadable Verifier or a SYSTEM principal
    /// terminates. One outbox slot is then reserved for its result; when the
    /// reserved share is full the execution defers without spending an
    /// attempt, so a full outbox backs claims off instead of losing results.
    /// Otherwise the engine runs within the execution deadline and a
    /// completed outcome is built into its result write, which
    /// [`settle`](Self::settle) stages inside the lease-fenced transaction
    /// that completes the execution. Never fails: every failure becomes the
    /// [`Transition`] it maps to.
    async fn execute(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        verifier: Result<Arc<Target>, VerificationError>,
        telemetry: &ExecutionTelemetry,
    ) -> (Transition, Option<StagedResult>) {
        let verifier = match verifier {
            Ok(verifier) => verifier,
            Err(error) => return (Transition::Terminate(TerminalStatus::Errored, error), None),
        };
        let system = match system_principal(&verifier) {
            Ok(system) => system,
            Err(transition) => return (transition, None),
        };
        let Ok(slot) = self.outbox.reserve(1) else {
            return (
                Transition::Defer(failure(
                    RUNTIME_BUSY,
                    "the verification result staging share is full",
                )),
                None,
            );
        };
        let started_at = Utc::now();
        let deadline = tokio::time::Instant::now() + self.limits.execution_timeout;
        let executed = telemetry
            .phase(
                Phase::Engine,
                tokio::time::timeout_at(
                    deadline,
                    self.dispatch(tenant, run, &verifier.spec, system, deadline, telemetry),
                )
                .instrument(tracing::info_span!("verification.engine")),
            )
            .await;
        let outcome = executed.unwrap_or_else(|_| timed_out());
        match outcome {
            EngineOutcome::Completed(outcome) => {
                let (transition, write) = result(run, &outcome, system, started_at);
                (transition, write.map(|write| (slot, write)))
            }
            EngineOutcome::Retry(error) => (Transition::Retry(error), None),
            EngineOutcome::Deferred(error) => (Transition::Defer(error), None),
            EngineOutcome::Terminal(status, error) => (Transition::Terminate(status, error), None),
        }
    }

    /// Run `spec` over the claimed execution's frozen input.
    ///
    /// A scheduled window first scores every drift requirement as the
    /// tenant's SYSTEM read authority, then runs with no pinned evidence; a
    /// submission runs over its pinned inputs and route overrides; a group
    /// snapshot runs over its pinned source lists. The engine
    /// executes as the authority accepted with the execution, executed by
    /// `system`, with model calls attributed to the verified Card and the
    /// submission's application Run. Every attempt before the last defers a
    /// not-yet-landed trace, so the queue retries the same execution under
    /// its backoff; the last attempt judges it inconclusive and completes
    /// under the Verifier's decision rule. Under `test-support` a scripted
    /// outcome, when queued, replaces the engine.
    async fn dispatch(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        spec: &Arc<VerifierSpec>,
        system: PrincipalId,
        deadline: tokio::time::Instant,
        telemetry: &ExecutionTelemetry,
    ) -> EngineOutcome {
        #[cfg(feature = "test-support")]
        if let Some(script) = &self.script
            && let Some(outcome) = script.next().await
        {
            return outcome;
        }
        let (no_inputs, no_routes) = (BTreeMap::new(), BTreeMap::new());
        let (drift, evidence, routes, run_id) = match &run.input {
            ExecutionInput::Scheduled { window } => {
                match self
                    .drift
                    .checks(tenant, run, system, spec, window, telemetry)
                    .await
                {
                    Ok(drift) => (drift, &no_inputs, &no_routes, None),
                    Err(outcome) => return outcome,
                }
            }
            ExecutionInput::Submitted {
                run_id,
                inputs,
                route_overrides,
            } => (
                DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                inputs,
                route_overrides,
                run_id.clone(),
            ),
            ExecutionInput::Grouped { inputs, .. } => (
                DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                inputs,
                &no_routes,
                None,
            ),
        };
        let caller = Caller::accepted(&run.authority, system);
        let engine = match self.blocks.engine(
            &self.state,
            Execution {
                caller: &caller,
                evidence,
                routes,
                subject: GatewayCallContext {
                    workload_card_uid: run
                        .owner_card_uid
                        .as_ref()
                        .or_else(|| run.authority.owning_root())
                        .cloned(),
                    subject_card_uid: Some(run.card_uid.clone()),
                    run_id,
                    deployment_environment: run.deployment_environment.clone(),
                },
                defer_unlanded_traces: run.attempt < run.max_attempts,
                drift,
                clock: telemetry.clock(),
            },
        ) {
            Ok(engine) => engine,
            Err(error) => {
                tracing::warn!(execution_id = %run.lease.execution_id, %error, "the execution engine cannot be built");
                return EngineOutcome::Retry(failure(
                    EXECUTION_INTERRUPTED,
                    "the verification engine cannot be built",
                ));
            }
        };
        match engine
            .run(Arc::clone(spec), evidence.clone(), deadline)
            .await
        {
            Ok(outcome) => EngineOutcome::Completed(outcome),
            Err(error) => engine_failure(run, &error),
        }
    }

    /// Apply `transition` to `run` in one tenant transaction.
    ///
    /// A [`Transition::Complete`] first joins the group commit
    /// through [`Completions`], which settles it in one transaction with the
    /// tenant's other completing runs; only when no batch committed it does
    /// it settle alone here. Either way, when the completion applies under
    /// this lease, the write in `result` is taken and staged on its reserved
    /// slot before the transaction commits, so no reader sees the run
    /// `completed` before its result is pending. A stale lease stages nothing. If the commit then fails, the
    /// already-staged write keeps its result ID: a retried settlement under
    /// the same lease completes the run without staging again, and a lost
    /// lease leaves the result written for a run another claim re-executes.
    ///
    /// Returns the stable outcome label: `completed`, `retrying`,
    /// `exhausted`, `cancelled`, `timed_out`, `errored`,
    /// `released`, `deferred`, or `stale_lease` when another claim already
    /// holds the run. [`Transition::LeaseLost`] opens no transaction and is
    /// `stale_lease`.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction fails; nothing is applied.
    pub async fn settle(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        transition: Transition,
        result: &SharedResult,
    ) -> Result<&'static str, SqlError> {
        if transition == Transition::LeaseLost {
            return Ok("stale_lease");
        }
        let lease = run.lease;
        if let Transition::Complete {
            result_id,
            verdict,
            summary,
            drift,
        } = &transition
        {
            let completion = Completion {
                lease,
                result_id: *result_id,
                verdict: *verdict,
                summary: summary.clone(),
                drift: *drift,
            };
            if let Some(settlement) = self.completions.complete(tenant, completion, result).await {
                return Ok(settled(settlement, "completed"));
            }
        }
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let outcome = match transition {
            Transition::Complete {
                result_id,
                verdict,
                summary,
                drift,
            } => {
                let settlement = self
                    .queue
                    .complete(&mut conn, lease, result_id, verdict, &summary, drift)
                    .await?;
                completions::stage(tenant, settlement, result);
                settled(settlement, "completed")
            }
            Transition::Retry(error) => {
                tracing::warn!(
                    execution_id = %lease.execution_id,
                    code = %error.code,
                    message = %error.message,
                    "verification attempt failed"
                );
                match self.queue.retry(&mut conn, lease, &error).await? {
                    RetryOutcome::Scheduled(_) => "retrying",
                    RetryOutcome::Exhausted => "exhausted",
                    RetryOutcome::StaleLease => "stale_lease",
                }
            }
            Transition::Terminate(status, error) => settled(
                self.queue
                    .terminate(&mut conn, lease, status, &error)
                    .await?,
                terminal_label(status),
            ),
            Transition::Release => settled(
                self.queue
                    .release(&mut conn, lease, chrono::Duration::zero())
                    .await?,
                "released",
            ),
            Transition::Defer(error) => {
                tracing::info!(
                    execution_id = %lease.execution_id,
                    code = %error.code,
                    message = %error.message,
                    "verification execution deferred by admission backpressure"
                );
                let delay = chrono::Duration::from_std(self.limits.poll_interval)
                    .unwrap_or_else(|_| chrono::Duration::seconds(1));
                settled(
                    self.queue.release(&mut conn, lease, delay).await?,
                    "deferred",
                )
            }
            Transition::LeaseLost => "stale_lease",
        };
        conn.commit().await?;
        Ok(outcome)
    }
}

impl LeasedWork for VerifierRunner {
    type Claim = AttemptClaim;

    const CAPABILITY: RuntimeCapability = RuntimeCapability::Runner;
    /// Verifier activity is counted per kind by [`ExecutionTelemetry`].
    const ACTIVE_GAUGE: Option<&'static str> = None;

    /// Every tenant with runnable executions, most overdue first.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant read fails.
    async fn due_tenants(&self) -> Result<Vec<DataTenantId>, SqlError> {
        Ok(self
            .queue
            .tenants_with_runnable_executions(&self.operator)
            .await?)
    }

    /// Claim up to `limit` of the tenant's runnable runs in one statement,
    /// each under its own fresh lease, and resolve their Verifiers in the
    /// same transaction.
    ///
    /// Each claimed run gets its `verification.attempt` span only once its
    /// row names the run's scrubbed identity and cause, so
    /// [`VerifierRunner::trace`] can install its link and sink before the
    /// span starts. The shared claim statement is then recorded on each
    /// attempt as a `claim` child carrying the statement's duration and the
    /// batch size, so every attempt's trace still covers claim to settlement.
    /// Each Verifier resolves inside its attempt, which travels with the
    /// claim to settlement. A failed claim opens no attempt.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the claim or a Verifier read fails; the
    /// caller's transaction then rolls every claim of the batch back.
    async fn claim(
        &self,
        conn: &mut TenantConn<'_>,
        limit: u32,
    ) -> Result<Vec<AttemptClaim>, SqlError> {
        let lease = chrono::Duration::from_std(self.limits.lease)
            .unwrap_or_else(|_| chrono::Duration::minutes(10));
        let resolving = Instant::now();
        let claimed = self.queue.claim(conn, lease, limit).await?;
        let statement_us = u64::try_from(resolving.elapsed().as_micros()).unwrap_or(u64::MAX);
        let batch = claimed.len();
        let mut claims = Vec::with_capacity(batch);
        for run in claimed {
            let span = tracing::info_span!(
                "verification.attempt",
                execution_id = %run.lease.execution_id,
                attempt = run.attempt,
                mode = ExecutionMode::Queued.as_str(),
                origin = run.origin.as_str(),
                task_start_delay_us = Empty,
                outcome = Empty,
                error_code = Empty,
                otel.status_code = Empty,
            );
            self.trace(&span, &run);
            drop(tracing::info_span!(
                parent: &span,
                "claim",
                operation = "verification.executions.claim",
                statement_us,
                batch,
            ));
            let verifier = async {
                let verifier = self.resolve(conn, &run).await?;
                // Bound before `verification.load` ends, so the load span is routed too.
                if let Ok(target) = &verifier
                    && let Ok(system) = system_principal(target)
                {
                    wyrd_telemetry::bind_tenant_route(wyrd_telemetry::TenantSpanRoute {
                        tenant: run.authority.principal.tenant_id,
                        attribution: Caller::accepted(&run.authority, system).attribution(),
                    });
                }
                Ok::<_, SqlError>(verifier)
            }
            .instrument(tracing::info_span!(parent: &span, "verification.load"))
            .instrument(span.clone())
            .await?;
            claims.push(AttemptClaim {
                run,
                verifier,
                loaded: (resolving, Instant::now()),
                span,
            });
        }
        Ok(claims)
    }

    /// Execute one claimed run and apply its single transition.
    ///
    /// Records the spawn-to-first-execution delay since `spawned_at` on the
    /// attempt span. The claim loop tracks the task until settlement. Cancellation through `abandon`
    /// (shutdown past its grace) stops the execution and releases the lease;
    /// a retryable failure observed after `stop` is also released rather than
    /// charged an attempt, since the process, not the run, failed. Resolves
    /// to `true` when admission backpressure deferred the run.
    async fn process(
        self: Arc<Self>,
        tenant: DataTenantId,
        claim: AttemptClaim,
        stop: CancellationToken,
        abandon: CancellationToken,
        spawned_at: Instant,
    ) -> bool {
        let AttemptClaim {
            run,
            verifier,
            loaded,
            span,
        } = claim;
        span.record(
            "task_start_delay_us",
            u64::try_from(spawned_at.elapsed().as_micros()).unwrap_or(u64::MAX),
        );
        self.attempt(tenant, &run, verifier, loaded, &stop, &abandon)
            .instrument(span)
            .await
    }

    /// Release a run claimed after shutdown began, with its attempt refunded
    /// and without executing it.
    ///
    /// Uses the same fenced release transition as the drain. A failed release
    /// is logged; the lease then expires into a reclaim.
    async fn release_late(&self, tenant: DataTenantId, claim: &AttemptClaim) {
        let run = &claim.run;
        match self
            .settle(tenant, run, Transition::Release, &SharedResult::default())
            .await
        {
            Ok(outcome) => {
                tracing::info!(execution_id = %run.lease.execution_id, outcome, "claim committed after shutdown began; released");
            }
            Err(error) => {
                tracing::error!(execution_id = %run.lease.execution_id, %error, "releasing a claim committed after shutdown failed; the lease will expire");
            }
        }
    }
}

impl VerifierRunner {
    /// Execute one claimed run inside its attempt span and settle it.
    ///
    /// The claim's Verifier resolution is recorded as the `load` phase. The
    /// run's lease is registered for renewal for the whole attempt;
    /// losing it stops the work at once without settling. A settlement whose
    /// transaction fails is retried with backoff while the lease holds.
    /// One [`ExecutionTelemetry`] owns the attempt's metrics: it counts the
    /// attempt once classified, holds the per-kind active gauge until it
    /// drops, and emits the phase, overhead, failure, and claim-to-settlement
    /// series. This method adds the queued-only series: the
    /// PostgreSQL-measured queue wait, the `settlement` phase, and — only once
    /// a terminal outcome is durably settled — the run's trigger-to-terminal
    /// latency: its age at claim plus this attempt's elapsed [`Instant`] time.
    /// Returns whether admission backpressure deferred the run.
    async fn attempt(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        verifier: Result<Arc<Target>, VerificationError>,
        loaded: (Instant, Instant),
        stop: &CancellationToken,
        abandon: &CancellationToken,
    ) -> bool {
        let started = Instant::now();
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Queued, Some(loaded.0));
        telemetry.record_phase(Phase::Load, loaded.0, loaded.1);
        let origin = run.origin.as_str();
        let held = self.renewal.hold(tenant, run.lease.token, abandon);
        let lost = held.lost();
        let (transition, result) = tokio::select! {
            biased;
            () = abandon.cancelled() => (Transition::Release, None),
            () = lost.cancelled() => (Transition::LeaseLost, None),
            executed = self.execute(tenant, run, verifier, &telemetry) => executed,
        };
        let transition = match transition {
            Transition::LeaseLost if abandon.is_cancelled() => Transition::Release,
            Transition::Retry(_) if stop.is_cancelled() => Transition::Release,
            transition => transition,
        };
        let refused = matches!(transition, Transition::Defer(_));
        if let Some(error) = transition.error() {
            tracing::Span::current().record("error_code", error.code.as_str());
        }
        let result = Arc::new(Mutex::new(result));
        let settled = telemetry
            .phase(
                Phase::Settlement,
                persist(lost, || {
                    self.settle(tenant, run, transition.clone(), &result)
                })
                .instrument(tracing::info_span!("verification.settle")),
            )
            .await;
        drop(held);
        let outcome = match settled {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::error!(execution_id = %run.lease.execution_id, %error, "verification settlement failed; the lease will expire");
                "settlement_failed"
            }
        };
        let span = tracing::Span::current();
        span.record("outcome", outcome);
        let failed = !matches!(outcome, "completed" | "released" | "deferred");
        if failed {
            span.record("otel.status_code", "ERROR");
        }
        metrics::histogram!(
            crate::app::metrics::VERIFICATION_QUEUE_WAIT_SECONDS,
            "origin" => origin
        )
        .record(run.queue_wait.as_secs_f64());
        if matches!(
            outcome,
            "completed" | "exhausted" | "cancelled" | "timed_out" | "errored"
        ) {
            metrics::histogram!(
                crate::app::metrics::VERIFICATION_TRIGGER_TO_TERMINAL_SECONDS,
                "origin" => origin,
                "outcome" => outcome
            )
            .record((run.age + started.elapsed()).as_secs_f64());
        }
        telemetry.finish(outcome, failed);
        refused
    }
}

/// Build `outcome`'s result payload once into the write that completes `run`.
///
/// The payload mints its own result ID, which the completing settlement
/// records as the execution's selected result, and `ended_at`, the results table's
/// time column, is a result fact this process observes, not a coordination
/// instant. The write is attributed to the execution's subject with `system`
/// as actor, and returned rather than staged: only a settlement that still
/// holds the lease may stage it, so a holder that lost its lease writes no
/// result. An outcome that cannot be encoded terminates `errored` with no
/// write.
fn result(
    run: &ClaimedExecution,
    outcome: &VerificationOutcome,
    system: PrincipalId,
    started_at: DateTime<Utc>,
) -> (Transition, Option<ScribeWrite>) {
    let (run_id, window) = match &run.input {
        ExecutionInput::Scheduled { window, .. } => (None, Some(window)),
        ExecutionInput::Submitted { run_id, .. } => (run_id.as_ref(), None),
        ExecutionInput::Grouped { .. } => (None, None),
    };
    let built = ResultPayloadBuilder::new(
        ResultRun {
            execution_id: run.lease.execution_id,
            run_id,
            verifier_version: &run.verifier_version,
            card_uid: &run.card_uid,
            owner_card_uid: run
                .owner_card_uid
                .as_ref()
                .or_else(|| run.authority.owning_root()),
            group_id: run.group_id.as_ref(),
            deployment_environment: run.deployment_environment.as_deref(),
            binding_id: run.binding_id,
            trigger: run.trigger.as_ref(),
            window,
        },
        &run.verifier_card_uid,
        started_at,
        Utc::now(),
    )
    .build(outcome);
    let payload = match built {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(execution_id = %run.lease.execution_id, %error, "verification result encoding failed");
            return (
                Transition::Terminate(
                    TerminalStatus::Errored,
                    failure(RESULT_INVALID, "the verification result cannot be encoded"),
                ),
                None,
            );
        }
    };
    (
        Transition::Complete {
            result_id: payload.result_id(),
            verdict: payload.verdict(),
            summary: engines::summary(outcome),
            drift: engines::drift_counts(outcome),
        },
        Some(ScribeWrite::Result {
            payload,
            attribution: Attribution::new(run.attribution.sub_principal_id, system),
        }),
    )
}

/// The outcome of an engine run that returned no verdict.
///
/// An expired deadline times out, an unplannable Verifier, an uncapturable
/// actual value, or nonconforming evidence terminates `errored`, a stopped
/// block task, unreadable evidence metadata, or a deferred not-yet-landed
/// trace retries, and a block refused
/// process capacity defers without spending an attempt.
fn engine_failure(run: &ClaimedExecution, error: &EngineError) -> EngineOutcome {
    tracing::warn!(execution_id = %run.lease.execution_id, %error, "the verification engine failed");
    match error {
        EngineError::DeadlineExceeded => timed_out(),
        EngineError::Plan(_) => EngineOutcome::Terminal(
            TerminalStatus::Errored,
            failure(VERIFIER_INVALID, "the Verifier cannot be planned"),
        ),
        EngineError::Capture(_) => EngineOutcome::Terminal(
            TerminalStatus::Errored,
            failure(
                RESULT_INVALID,
                "a requirement actual value cannot be captured",
            ),
        ),
        EngineError::Join(_) => EngineOutcome::Retry(failure(
            EXECUTION_INTERRUPTED,
            "a verification block task stopped unexpectedly",
        )),
        EngineError::Busy(_) => EngineOutcome::Deferred(failure(
            RUNTIME_BUSY,
            "a verification block was refused process capacity",
        )),
        EngineError::InputInvalid(reason) | EngineError::InputTooLarge(reason) => {
            EngineOutcome::Terminal(TerminalStatus::Errored, failure(INPUT_INVALID, reason))
        }
        EngineError::EvidenceUnavailable(_) => EngineOutcome::Retry(failure(
            EVIDENCE_UNAVAILABLE,
            "the verification evidence cannot be admitted now",
        )),
    }
}

/// The terminal outcome of an execution past its deadline.
fn timed_out() -> EngineOutcome {
    EngineOutcome::Terminal(
        TerminalStatus::TimedOut,
        failure(
            EXECUTION_TIMED_OUT,
            "the Verifier exceeded its execution deadline",
        ),
    )
}

/// The tenant SYSTEM principal the target resolved, when it is an active
/// `UUIDv7` principal the execution can run and be attributed as.
///
/// # Errors
/// Terminates `errored` with [`SYSTEM_PRINCIPAL_MISSING`] otherwise.
fn system_principal(target: &Target) -> Result<PrincipalId, Transition> {
    target
        .system_principal
        .filter(|principal| principal.as_uuid().get_version_num() == 7)
        .ok_or_else(|| {
            Transition::Terminate(
                TerminalStatus::Errored,
                failure(
                    SYSTEM_PRINCIPAL_MISSING,
                    "the tenant has no SYSTEM principal to execute the Verifier as",
                ),
            )
        })
}

/// Run `operation` until it succeeds, retrying failures with doubling backoff
/// until `lost` fires.
///
/// The first attempt always runs, so work whose lease is already lost still
/// tries once (a release on abandon).
///
/// # Errors
/// Returns the latest failure once `lost` is cancelled.
async fn persist<T, F, Fut>(lost: &CancellationToken, mut operation: F) -> Result<T, SqlError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, SqlError>>,
{
    let mut pause = PERSIST_INITIAL;
    loop {
        let error = match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        tracing::warn!(%error, "a verification execution transaction failed; retrying while the lease holds");
        tokio::select! {
            () = lost.cancelled() => return Err(error),
            () = tokio::time::sleep(pause) => {}
        }
        pause = (pause * 2).min(PERSIST_MAX);
    }
}

impl Transition {
    /// The stable error this transition settles with, when it carries one.
    fn error(&self) -> Option<&VerificationError> {
        match self {
            Self::Retry(error) | Self::Terminate(_, error) | Self::Defer(error) => Some(error),
            Self::Complete { .. } | Self::Release | Self::LeaseLost => None,
        }
    }
}

/// Stable label of a terminal status.
const fn terminal_label(status: TerminalStatus) -> &'static str {
    match status {
        TerminalStatus::Cancelled => "cancelled",
        TerminalStatus::TimedOut => "timed_out",
        TerminalStatus::Errored => "errored",
    }
}

/// An execution failure with a stable `code` and fixed public `message`.
///
/// The error is persisted and exposed in execution status, so `message` names only
/// the failed operation; dependency causes go to the diagnostic log instead.
fn failure(code: &str, message: &str) -> VerificationError {
    VerificationError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

/// Test-only queue of engine outcomes consulted before the real engine.
///
/// Each execution pops the next scripted outcome; an empty script falls
/// through to the real engine. While held, executions that popped an
/// outcome queued by [`push`](Self::push) wait until
/// [`release`](Self::release) or cancellation, so a test can observe or shut
/// down in-flight work deterministically; outcomes queued by
/// [`push_unheld`](Self::push_unheld) return at once.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone)]
pub struct EngineScript {
    /// Scripted outcomes, oldest first, each with whether it obeys the hold.
    outcomes: Arc<Mutex<VecDeque<(EngineOutcome, bool)>>>,
    /// Whether executions are held before returning.
    held: Arc<Sender<bool>>,
    /// Executions that have taken a scripted outcome.
    entered: Arc<AtomicUsize>,
}

#[cfg(feature = "test-support")]
impl Default for EngineScript {
    /// An empty, unheld script.
    fn default() -> Self {
        Self {
            outcomes: Arc::default(),
            held: Arc::new(Sender::new(false)),
            entered: Arc::default(),
        }
    }
}

#[cfg(feature = "test-support")]
impl EngineScript {
    /// Queue `outcome` for the next execution.
    ///
    /// # Panics
    /// Panics when the script lock is poisoned.
    pub fn push(&self, outcome: EngineOutcome) {
        self.outcomes
            .lock()
            .expect("engine script lock")
            .push_back((outcome, true));
    }

    /// Queue `outcome` for the next execution, returning even while held.
    ///
    /// # Panics
    /// Panics when the script lock is poisoned.
    pub fn push_unheld(&self, outcome: EngineOutcome) {
        self.outcomes
            .lock()
            .expect("engine script lock")
            .push_back((outcome, false));
    }

    /// Hold scripted executions until [`release`](Self::release).
    pub fn hold(&self) {
        self.held.send_replace(true);
    }

    /// Let held executions return.
    pub fn release(&self) {
        self.held.send_replace(false);
    }

    /// Executions that have taken a scripted outcome so far.
    #[must_use]
    pub fn entered(&self) -> usize {
        self.entered.load(Ordering::SeqCst)
    }

    /// Pop the next outcome, waiting while held unless it was queued unheld.
    ///
    /// # Panics
    /// Panics when the script lock is poisoned.
    async fn next(&self) -> Option<EngineOutcome> {
        let (outcome, obeys_hold) = self
            .outcomes
            .lock()
            .expect("engine script lock")
            .pop_front()?;
        self.entered.fetch_add(1, Ordering::SeqCst);
        if obeys_hold {
            let mut held = self.held.subscribe();
            let _ = held.wait_for(|held| !held).await;
        }
        Some(outcome)
    }
}
