//! The generic Verifier runner: claim, execute, publish, settle.
//!
//! [`VerifierRunner`] claims and starts runs without execution-count permits.
//! Each claimed run loads its exact Verifier Card, goes through the one closed dispatch over
//! [`VerifierImplementation`], and ends in exactly one fenced transition the
//! runner applies itself: a completed report is published as the tenant's
//! SYSTEM writer and then completed; a retryable failure is retried within
//! the run's attempt budget; a terminal failure is terminated without a
//! verdict; and work abandoned by shutdown is released with its attempt
//! refunded. Claims and settlements are engine mechanics, not authorization
//! decisions, so none of them writes audit.

#[cfg(feature = "test-support")]
use std::collections::VecDeque;
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::Mutex;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use chrono::{DateTime, Utc};
#[cfg(feature = "test-support")]
use tokio::sync::watch::Sender;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use tracing::field::{Empty, display};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::operator::VerifierCounts;
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::ids::VerificationResultId;
use wyrd_spec::reference::CardRef;
use wyrd_spec::verification::{VerificationError, VerificationVerdict, VerifierKind};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verifier_runs::{
    ClaimedRun, RetryOutcome, TerminalStatus, TraceWaitOutcome, VerifierRunQueue,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
use super::claims::{ClaimLoop, LeasedWork, settled};
use super::drift::DriftEngine;
use super::engines::{EngineOutcome, VerifierReport};
use super::eval::EvalEngine;
use super::health::RuntimeCapability;
use super::publisher::ResultPublisher;
use super::results::{ResultPayloadBuilder, ResultRun};
use super::telemetry::{ExecutionMode, ExecutionTelemetry, Phase};

/// Stable error code when the exact Verifier Card cannot be loaded or is not
/// a Verifier.
pub const VERIFIER_UNAVAILABLE: &str = "verifier_unavailable";
/// Stable error code when an engine exceeded its execution deadline.
pub const EXECUTION_TIMED_OUT: &str = "execution_timed_out";
/// Stable error code when a completed result could not be encoded.
pub const RESULT_INVALID: &str = "result_invalid";
/// Stable error code when a completed result was not durably acknowledged.
pub const RESULT_PUBLICATION_FAILED: &str = "result_publication_failed";

/// The single transition one claimed run ends in.
#[derive(Debug, Clone, PartialEq)]
pub enum Transition {
    /// Every result batch was acknowledged; settle `completed` with this result.
    Complete {
        /// The published result.
        result_id: VerificationResultId,
        /// Its verdict.
        verdict: VerificationVerdict,
        /// Bounded human-readable summary frozen into failure dispatches.
        summary: String,
        /// The same result's counts frozen into failure dispatches.
        counts: VerifierCounts,
    },
    /// Try the same run and input again within its attempt budget.
    Retry(VerificationError),
    /// Requeue the Eval run, attempt refunded, until its trace deadline.
    AwaitTrace(VerificationError),
    /// Settle without a verdict.
    Terminate(TerminalStatus, VerificationError),
    /// Return the run to its queue with its attempt refunded.
    Release,
    /// Return the run to its queue with its attempt refunded, due after one
    /// poll interval, because an input read met admission backpressure.
    Defer(VerificationError),
}

/// One claimed run and the attempt span opened before its claim.
///
/// The `verification.attempt` span is created before the claim statement
/// runs and closes when the attempt settles, so one trace covers claim,
/// Verifier load, evidence read, engine execution, result publication, and
/// settlement. It carries only scrubbed identifiers and bounded labels.
pub struct AttemptClaim {
    /// The claimed run.
    run: ClaimedRun,
    /// The attempt's root span.
    span: tracing::Span,
}

/// The closed set of engine arms a Verifier run executes through.
///
/// One arm exists per [`VerifierImplementation`] variant, so adding a variant
/// fails to compile until its engine is supplied here.
pub struct VerifierEngines {
    /// The Drift arm's engine.
    drift: DriftEngine,
    /// The continuous Eval arm.
    eval: EvalEngine,
}

impl VerifierEngines {
    /// Groups the Drift and Eval arms the runner dispatches to.
    #[must_use]
    pub const fn new(drift: DriftEngine, eval: EvalEngine) -> Self {
        Self { drift, eval }
    }

    /// Executes one claimed run through the arm its implementation names.
    ///
    /// Drift reads as the SYSTEM principal scoped to the exact `verifier`;
    /// Eval executes the run's continuous evaluation for `tenant`. Each arm
    /// records its input-read, preparation, and wait intervals on
    /// `telemetry`. Every failure is carried in the returned
    /// [`EngineOutcome`], not raised.
    async fn execute(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        verifier: &CardRef,
        implementation: &VerifierImplementation,
        telemetry: &ExecutionTelemetry,
    ) -> EngineOutcome {
        // Boxed: each engine's query chain is large, and inlining it into
        // every runner frame overflows a worker stack in unoptimized builds.
        match implementation {
            VerifierImplementation::Drift(spec) => {
                Box::pin(self.drift.verify(tenant, verifier, run, spec, telemetry)).await
            }
            VerifierImplementation::Eval(spec) => {
                Box::pin(self.eval.execute(tenant, run, spec, telemetry)).await
            }
        }
    }
}

/// Owner of claiming, executing, publishing, and settling Verifier runs.
pub struct VerifierRunner {
    /// Wyrd Postgres owner that opens every tenant-scoped claim, Card read,
    /// and settlement transaction.
    postgres: WyrdPostgres,
    /// Operator pool for the cross-tenant runnable list.
    operator: OperatorPool,
    /// Queue policies and transitions.
    queue: VerifierRunQueue,
    /// Shared claim loop owning durable claims, shutdown, and drain.
    claims: ClaimLoop,
    /// Remote result publication.
    publisher: ResultPublisher,
    /// The Drift and Eval arms a claimed run dispatches to.
    engines: VerifierEngines,
    /// Runtime bounds.
    limits: RuntimeLimits,
    /// Test-only scripted engine outcomes.
    #[cfg(feature = "test-support")]
    script: Option<EngineScript>,
}

impl VerifierRunner {
    /// Build a runner over the Wyrd Postgres owner and the operator pool.
    ///
    /// The runner carries no coordination clock: queue availability, claims,
    /// leases, retries, and settlements are decided and stamped by PostgreSQL.
    /// It keeps [`Instant`] for its own elapsed-time telemetry and the
    /// producer's wall clock only for the event facts a result carries.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        operator: OperatorPool,
        queue: VerifierRunQueue,
        publisher: ResultPublisher,
        engines: VerifierEngines,
        limits: RuntimeLimits,
    ) -> Self {
        Self {
            claims: ClaimLoop::new(postgres.clone(), None, &limits),
            postgres,
            operator,
            queue,
            publisher,
            engines,
            limits,
            #[cfg(feature = "test-support")]
            script: None,
        }
    }

    /// Consult `script` before the real engine arms.
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

    /// Claim and execute runs until `stop` is cancelled, then drain.
    ///
    /// Delegates to the shared [`ClaimLoop`]: each turn claims one run per
    /// tenant with capacity, most overdue first, and spawns its execution; on
    /// `stop` no further claim is admitted (an uncommitted claim rolls back
    /// and a claim committed after `stop` is released unexecuted), executions
    /// spawned before `stop` get the drain grace to settle, and any still
    /// running are then cancelled and release their leases before this
    /// returns.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed a runner crash. The
    /// in-flight executions are aborted with the task and keep their leases
    /// until expiry.
    pub async fn run(self: Arc<Self>, stop: CancellationToken) {
        self.claims.run(&self, stop).await;
    }

    /// Load the Verifier, dispatch it, and publish a completed report.
    ///
    /// Never fails: every failure becomes the [`Transition`] it maps to.
    ///
    /// Each owned phase — `load`, `engine`, and `publication` — runs in its
    /// own child span and is recorded on `telemetry`; the execution is
    /// classified by its exact Verifier as soon as that loads.
    async fn execute(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        telemetry: &ExecutionTelemetry,
    ) -> Transition {
        let loaded = telemetry
            .phase(
                Phase::Load,
                self.load_verifier(tenant, run)
                    .instrument(tracing::info_span!("verification.load")),
            )
            .await;
        let (verifier, implementation) = match loaded {
            Ok(loaded) => loaded,
            Err(transition) => return transition,
        };
        telemetry.classify(VerifierKind::of(&implementation));
        let started_at = Utc::now();
        let executed = telemetry
            .phase(
                Phase::Engine,
                tokio::time::timeout(
                    self.limits.execution_timeout,
                    self.dispatch(tenant, run, &verifier, &implementation, telemetry),
                )
                .instrument(tracing::info_span!("verification.engine")),
            )
            .await;
        let outcome = match executed {
            Ok(outcome) => outcome,
            Err(_) => {
                return Transition::Terminate(
                    TerminalStatus::TimedOut,
                    failure(
                        EXECUTION_TIMED_OUT,
                        "the Verifier exceeded its execution deadline",
                    ),
                );
            }
        };
        match outcome {
            EngineOutcome::Completed(report) => {
                telemetry
                    .phase(
                        Phase::Publication,
                        self.publish(tenant, run, &verifier, &report, started_at)
                            .instrument(tracing::info_span!("verification.publish")),
                    )
                    .await
            }
            EngineOutcome::Retry(error) => Transition::Retry(error),
            EngineOutcome::AwaitingTrace(error) => Transition::AwaitTrace(error),
            EngineOutcome::Deferred(error) => Transition::Defer(error),
            EngineOutcome::Terminal(status, error) => Transition::Terminate(status, error),
        }
    }

    /// Load the run's exact Verifier Card and its implementation.
    ///
    /// # Errors
    /// Returns the transition for an unloadable Verifier: a transient registry
    /// failure retries, and a missing, unparseable, or non-Verifier card
    /// terminates `errored`.
    async fn load_verifier(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
    ) -> Result<(CardRef, VerifierImplementation), Transition> {
        let unavailable = |cause: &dyn std::fmt::Display| {
            tracing::warn!(run_id = %run.lease.run_id, %cause, "loading the Verifier Card failed");
            failure(VERIFIER_UNAVAILABLE, "the Verifier Card cannot be loaded")
        };
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| Transition::Retry(unavailable(&error)))?;
        let card = get_card_by_uid(&mut conn, &run.verifier_uid)
            .await
            .map_err(|error| {
                if error.status() >= 500 {
                    Transition::Retry(unavailable(&error))
                } else {
                    Transition::Terminate(TerminalStatus::Errored, unavailable(&error))
                }
            })?;
        drop(conn);
        let Spec::Verifier(spec) = card.spec else {
            return Err(Transition::Terminate(
                TerminalStatus::Errored,
                failure(
                    VERIFIER_UNAVAILABLE,
                    &format!("card {} is not a Verifier", run.verifier_uid),
                ),
            ));
        };
        let verifier = CardRef {
            kind: CardKind::Verifier,
            name: card.name,
            version: card.version,
            space: Some(card.space),
            uid: Some(card.card_uid),
        };
        Ok((verifier, spec.implementation))
    }

    /// The one closed dispatch over Verifier implementations.
    ///
    /// The run executes through [`VerifierEngines`] for `tenant` and the exact
    /// `verifier`. Under `test-support` a scripted outcome, when queued,
    /// replaces the arm.
    async fn dispatch(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        verifier: &CardRef,
        implementation: &VerifierImplementation,
        telemetry: &ExecutionTelemetry,
    ) -> EngineOutcome {
        #[cfg(feature = "test-support")]
        if let Some(script) = &self.script
            && let Some(outcome) = script.next().await
        {
            return outcome;
        }
        self.engines
            .execute(tenant, run, verifier, implementation, telemetry)
            .await
    }

    /// Publish `report` as the run's result and map the attempt to a transition.
    ///
    /// Mints one result ID and one producer event time shared by every row —
    /// a result fact this process observes, not a coordination instant —
    /// writes every batch through the publisher, and completes only after the
    /// summary is acknowledged. An encoding failure terminates `errored`; an
    /// unacknowledged or timed-out publication retries.
    async fn publish(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        verifier: &CardRef,
        report: &VerifierReport,
        started_at: DateTime<Utc>,
    ) -> Transition {
        let result_id = VerificationResultId::new_v7();
        let event_time = Utc::now();
        let verifier_ref = CardRef {
            uid: None,
            ..verifier.clone()
        }
        .to_string();
        let payload = match ResultPayloadBuilder::new(
            ResultRun::from(run),
            &verifier_ref,
            result_id,
            event_time,
            started_at,
            event_time,
        )
        .build(report)
        {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(run_id = %run.lease.run_id, %error, "verification result encoding failed");
                return Transition::Terminate(
                    TerminalStatus::Errored,
                    failure(RESULT_INVALID, "the verification result cannot be encoded"),
                );
            }
        };
        let published = tokio::time::timeout(
            self.limits.publication_timeout,
            self.publisher.publish(tenant, verifier, &payload),
        )
        .await;
        match published {
            Ok(Ok(())) => Transition::Complete {
                result_id,
                verdict: payload.verdict(),
                summary: report.summary(),
                counts: report.counts(),
            },
            Ok(Err(error)) => {
                tracing::warn!(run_id = %run.lease.run_id, %error, "verification result publication failed");
                Transition::Retry(failure(
                    RESULT_PUBLICATION_FAILED,
                    "result publication was not acknowledged",
                ))
            }
            Err(_) => Transition::Retry(failure(
                RESULT_PUBLICATION_FAILED,
                "result publication exceeded its deadline",
            )),
        }
    }

    /// Apply `transition` to `run` in one tenant transaction.
    ///
    /// Returns the stable outcome label: `completed`, `retrying`,
    /// `exhausted`, `awaiting_trace`, `cancelled`, `timed_out`, `errored`,
    /// `released`, `deferred`, or `stale_lease` when another claim already
    /// holds the run.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction fails; nothing is applied
    /// and the lease expires into a reclaim.
    pub async fn settle(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        transition: Transition,
    ) -> Result<&'static str, SqlError> {
        let lease = run.lease;
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let outcome = match transition {
            Transition::Complete {
                result_id,
                verdict,
                summary,
                counts,
            } => settled(
                self.queue
                    .complete(&mut conn, lease, result_id, verdict, &summary, counts)
                    .await?,
                "completed",
            ),
            Transition::Retry(error) => {
                tracing::warn!(
                    run_id = %lease.run_id,
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
            Transition::AwaitTrace(error) => {
                let (poll, deadline) = (
                    chrono::Duration::from_std(self.limits.trace_poll)
                        .unwrap_or_else(|_| chrono::Duration::seconds(5)),
                    chrono::Duration::from_std(self.limits.trace_deadline)
                        .unwrap_or_else(|_| chrono::Duration::minutes(5)),
                );
                match self
                    .queue
                    .await_trace(&mut conn, lease, poll, deadline, &error)
                    .await?
                {
                    TraceWaitOutcome::Requeued(_) => "awaiting_trace",
                    TraceWaitOutcome::TimedOut => "timed_out",
                    TraceWaitOutcome::StaleLease => "stale_lease",
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
                    run_id = %lease.run_id,
                    code = %error.code,
                    message = %error.message,
                    "verification input read deferred by admission backpressure"
                );
                let delay = chrono::Duration::from_std(self.limits.poll_interval)
                    .unwrap_or_else(|_| chrono::Duration::seconds(1));
                settled(
                    self.queue.release(&mut conn, lease, delay).await?,
                    "deferred",
                )
            }
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

    /// Tenants with runnable runs, most overdue first.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant read fails.
    async fn due_tenants(&self, limit: i64) -> Result<Vec<DataTenantId>, SqlError> {
        Ok(self
            .queue
            .tenants_with_runnable_runs(&self.operator, limit)
            .await?)
    }

    /// Claim the tenant's next runnable run under a fresh lease.
    ///
    /// Opens the run's `verification.attempt` span before the claim statement
    /// and records the claimed run's scrubbed identity on it; the span then
    /// travels with the claim to settlement.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the claim fails.
    async fn claim(&self, conn: &mut TenantConn<'_>) -> Result<Option<AttemptClaim>, SqlError> {
        let lease = chrono::Duration::from_std(self.limits.lease)
            .unwrap_or_else(|_| chrono::Duration::minutes(10));
        let span = tracing::info_span!(
            "verification.attempt",
            run_id = Empty,
            attempt = Empty,
            kind = Empty,
            mode = ExecutionMode::Queued.as_str(),
            origin = Empty,
            task_start_delay_us = Empty,
            outcome = Empty,
            error_code = Empty,
            otel.status_code = Empty,
        );
        let claimed = self
            .queue
            .claim(conn, lease)
            .instrument(span.clone())
            .await?;
        Ok(claimed.map(|run| {
            span.record("run_id", display(run.lease.run_id));
            span.record("attempt", run.attempt);
            span.record("origin", run.origin.as_str());
            AttemptClaim { run, span }
        }))
    }

    /// Execute one claimed run and apply its single transition.
    ///
    /// Records the spawn-to-first-execution delay since `spawned_at` on the
    /// attempt span. The claim loop tracks the task until settlement. Cancellation through `abandon`
    /// (shutdown past its grace) stops the execution and releases the lease;
    /// a retryable failure observed after `stop` is also released rather than
    /// charged an attempt, since the process, not the run, failed.
    async fn process(
        self: Arc<Self>,
        tenant: DataTenantId,
        claim: AttemptClaim,
        stop: CancellationToken,
        abandon: CancellationToken,
        spawned_at: Instant,
    ) {
        let AttemptClaim { run, span } = claim;
        span.record(
            "task_start_delay_us",
            u64::try_from(spawned_at.elapsed().as_micros()).unwrap_or(u64::MAX),
        );
        self.attempt(tenant, &run, &stop, &abandon)
            .instrument(span)
            .await;
    }

    /// Release a run claimed after shutdown began, with its attempt refunded
    /// and without executing it.
    ///
    /// Uses the same fenced release transition as the drain. A failed release
    /// is logged; the lease then expires into a reclaim.
    async fn release_late(&self, tenant: DataTenantId, claim: &AttemptClaim) {
        let run = &claim.run;
        match self.settle(tenant, run, Transition::Release).await {
            Ok(outcome) => {
                tracing::info!(run_id = %run.lease.run_id, outcome, "claim committed after shutdown began; released");
            }
            Err(error) => {
                tracing::error!(run_id = %run.lease.run_id, %error, "releasing a claim committed after shutdown failed; the lease will expire");
            }
        }
    }
}

impl VerifierRunner {
    /// Execute one claimed run inside its attempt span and settle it.
    ///
    /// One [`ExecutionTelemetry`] owns the attempt's metrics: it counts the
    /// attempt once classified, holds the per-kind active gauge until it
    /// drops, and emits the phase, overhead, failure, and claim-to-settlement
    /// series. This method adds the queued-only series: the
    /// PostgreSQL-measured queue wait, the `settlement` phase, and — only once
    /// a terminal outcome is durably settled — the run's trigger-to-terminal
    /// latency: its age at claim plus this attempt's elapsed [`Instant`] time.
    async fn attempt(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        stop: &CancellationToken,
        abandon: &CancellationToken,
    ) {
        let started = Instant::now();
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Queued);
        let origin = run.origin.as_str();
        let transition = tokio::select! {
            () = abandon.cancelled() => Transition::Release,
            transition = self.execute(tenant, run, &telemetry) => transition,
        };
        let transition = match transition {
            Transition::Retry(_) if stop.is_cancelled() => Transition::Release,
            transition => transition,
        };
        if let Some(error) = transition.error() {
            tracing::Span::current().record("error_code", error.code.as_str());
        }
        let settled = telemetry
            .phase(
                Phase::Settlement,
                self.settle(tenant, run, transition)
                    .instrument(tracing::info_span!("verification.settle")),
            )
            .await;
        let outcome = match settled {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::error!(run_id = %run.lease.run_id, %error, "verification settlement failed; the lease will expire");
                "settlement_failed"
            }
        };
        let span = tracing::Span::current();
        span.record("outcome", outcome);
        let failed = !matches!(
            outcome,
            "completed" | "released" | "awaiting_trace" | "deferred"
        );
        if failed {
            span.record("otel.status_code", "ERROR");
        }
        let kind = telemetry.kind().as_str();
        metrics::histogram!(
            crate::app::metrics::VERIFICATION_QUEUE_WAIT_SECONDS,
            "kind" => kind,
            "origin" => origin
        )
        .record(run.queue_wait.as_secs_f64());
        if matches!(
            outcome,
            "completed" | "exhausted" | "cancelled" | "timed_out" | "errored"
        ) {
            metrics::histogram!(
                crate::app::metrics::VERIFICATION_TRIGGER_TO_TERMINAL_SECONDS,
                "kind" => kind,
                "origin" => origin,
                "outcome" => outcome
            )
            .record((run.age + started.elapsed()).as_secs_f64());
        }
        telemetry.finish(outcome, failed);
    }
}

impl Transition {
    /// The stable error this transition settles with, when it carries one.
    fn error(&self) -> Option<&VerificationError> {
        match self {
            Self::Retry(error)
            | Self::AwaitTrace(error)
            | Self::Terminate(_, error)
            | Self::Defer(error) => Some(error),
            Self::Complete { .. } | Self::Release => None,
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

/// A run failure with a stable `code` and fixed public `message`.
///
/// The error is persisted and exposed in run status, so `message` names only
/// the failed operation; dependency causes go to the diagnostic log instead.
fn failure(code: &str, message: &str) -> VerificationError {
    VerificationError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

/// Test-only queue of engine outcomes consulted before the real arms.
///
/// Each execution pops the next scripted outcome; an empty script falls
/// through to the real dispatch. While held, executions that popped an
/// outcome wait until [`release`](Self::release) or cancellation, so a test
/// can observe or shut down in-flight work deterministically.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone)]
pub struct EngineScript {
    /// Scripted outcomes, oldest first.
    outcomes: Arc<Mutex<VecDeque<EngineOutcome>>>,
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
            .push_back(outcome);
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

    /// Pop the next outcome, waiting while held.
    ///
    /// # Panics
    /// Panics when the script lock is poisoned.
    async fn next(&self) -> Option<EngineOutcome> {
        let outcome = self
            .outcomes
            .lock()
            .expect("engine script lock")
            .pop_front()?;
        self.entered.fetch_add(1, Ordering::SeqCst);
        let mut held = self.held.subscribe();
        let _ = held.wait_for(|held| !held).await;
        Some(outcome)
    }
}
