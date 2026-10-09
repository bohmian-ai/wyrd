//! The generic Verifier runner: claim, execute, stage, settle.
//!
//! [`VerifierRunner`] claims and starts runs without execution-count permits.
//! The claim transaction also resolves the run's exact Verifier, from the
//! process's [`VerifierCache`] or, on a miss, from the registry on the same
//! connection, so a run holds a connection only to claim, settle, and renew.
//! Each run goes through the one closed dispatch over
//! [`VerifierImplementation`] and ends in exactly one fenced transition the
//! runner applies itself. A completed report becomes one result payload,
//! staged on the process [`ScribeOutbox`] attributed to the tenant's SYSTEM
//! principal and the exact Verifier, and the run completes once it is staged;
//! delivery belongs to the outbox and never re-executes the run. A retryable
//! failure is retried within the run's attempt budget; a terminal failure is
//! terminated without a verdict; work abandoned by shutdown is released with
//! its attempt refunded; and work whose lease is lost stops without settling.
//! While a run is in flight its lease is renewed by [`LeaseRenewal`]. Claims,
//! renewals, and settlements are engine mechanics, not authorization
//! decisions, so none of them writes audit.

#[cfg(feature = "test-support")]
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::Mutex;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
#[cfg(feature = "test-support")]
use tokio::sync::watch::Sender;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use tracing::field::{Empty, display};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::card::operator::VerifierCounts;
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::ids::VerificationResultId;
use wyrd_spec::reference::CardRef;
use wyrd_spec::verification::{VerificationError, VerificationVerdict, VerifierKind};
use wyrd_sql::queries::cards::fetch_card_row;
use wyrd_sql::queries::verifier_runs::{
    ClaimedRun, RetryOutcome, TerminalStatus, TraceWaitOutcome, VerifierRunQueue,
};
use wyrd_sql::{OperatorPool, ParsedCardRow, SqlError, TenantConn, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
use super::cache::{CachedVerifier, VerifierCache};
use super::claims::{ClaimLoop, LeasedWork, settled};
use super::drift::DriftEngine;
use super::engines::{EngineOutcome, VerifierReport};
use super::eval::EvalEngine;
use super::health::RuntimeCapability;
use super::leases::LeaseRenewal;
use super::results::{ResultPayloadBuilder, ResultRun};
use super::telemetry::{ExecutionMode, ExecutionTelemetry, Phase};
use crate::scribe_outbox::{ScribeOutbox, ScribeWrite, VerifierAttribution};

/// Stable error code when the exact Verifier Card cannot be loaded or is not
/// a Verifier.
pub const VERIFIER_UNAVAILABLE: &str = "verifier_unavailable";
/// Stable error code when an engine exceeded its execution deadline.
pub const EXECUTION_TIMED_OUT: &str = "execution_timed_out";
/// Stable error code when a completed result could not be encoded.
pub const RESULT_INVALID: &str = "result_invalid";
/// Stable error code when the tenant has no active `UUIDv7` SYSTEM principal
/// to attribute a result to.
pub const SYSTEM_PRINCIPAL_MISSING: &str = "system_principal_missing";

/// First wait before a failed settlement is tried again.
const PERSIST_INITIAL: Duration = Duration::from_millis(50);
/// Longest wait between settlement attempts.
const PERSIST_MAX: Duration = Duration::from_secs(5);

/// The single transition one claimed run ends in.
#[derive(Debug, Clone, PartialEq)]
pub enum Transition {
    /// The result was built; settle `completed` with it, then stage it.
    Complete {
        /// The staged result.
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
    /// The lease is no longer held; another claimant owns the run, so nothing
    /// is settled.
    LeaseLost,
}

/// One claimed run, its resolved Verifier, and the attempt span opened before
/// its claim.
///
/// The `verification.attempt` span is created before the claim statement
/// runs and closes when the attempt settles, so one trace covers claim,
/// Verifier resolution, evidence read, engine execution, result staging, and
/// settlement. It carries only scrubbed identifiers and
/// bounded labels.
pub struct AttemptClaim {
    /// The claimed run.
    run: ClaimedRun,
    /// The exact Verifier, or the error a run without one terminates with.
    verifier: Result<Arc<CachedVerifier>, VerificationError>,
    /// When the claim transaction started and finished resolving the
    /// Verifier, recorded as the attempt's `load` phase.
    loaded: (Instant, Instant),
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
    /// Drift and Eval read their inputs as the tenant's SYSTEM principal the
    /// claim returned. Each arm
    /// records its input-read, preparation, and wait intervals on
    /// `telemetry`. Every failure is carried in the returned
    /// [`EngineOutcome`], not raised.
    ///
    /// Each arm's future is boxed. The arms embed the Bifrost query stream
    /// futures, and every wrapper above this call (phase timing, timeout,
    /// lease select, claim task) holds its child inline, so an unboxed arm
    /// multiplies into a claim task that overflows a worker thread's stack.
    async fn execute(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        implementation: &VerifierImplementation,
        telemetry: &ExecutionTelemetry,
    ) -> EngineOutcome {
        // Boxed: each engine's query chain is large, and inlining it into
        // every runner frame overflows a worker stack in unoptimized builds.
        match implementation {
            VerifierImplementation::Drift(spec) => {
                Box::pin(self.drift.verify(tenant, run, spec, telemetry)).await
            }
            VerifierImplementation::Eval(spec) => {
                Box::pin(self.eval.execute(tenant, run, spec, telemetry)).await
            }
        }
    }
}

/// Owner of claiming, executing, staging, and settling Verifier runs.
pub struct VerifierRunner {
    /// Wyrd Postgres owner that opens every tenant-scoped claim and
    /// settlement transaction.
    postgres: WyrdPostgres,
    /// Operator pool for the cross-tenant runnable list.
    operator: OperatorPool,
    /// Queue policies and transitions.
    queue: VerifierRunQueue,
    /// Shared claim loop owning durable claims, shutdown, and drain.
    claims: ClaimLoop,
    /// The process's Scribe outbox every result is staged on.
    outbox: Arc<ScribeOutbox>,
    /// The Drift and Eval arms a claimed run dispatches to.
    engines: VerifierEngines,
    /// Parsed Verifier Cards by tenant and UID.
    cache: VerifierCache,
    /// Renewal of every in-flight run's lease.
    renewal: Arc<LeaseRenewal>,
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
    /// leases, renewals, retries, and settlements are decided and stamped by
    /// PostgreSQL. It keeps [`Instant`] for its own elapsed-time telemetry and
    /// the producer's wall clock only for the event facts a result carries.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        operator: OperatorPool,
        queue: VerifierRunQueue,
        outbox: Arc<ScribeOutbox>,
        engines: VerifierEngines,
        limits: RuntimeLimits,
    ) -> Self {
        Self {
            claims: ClaimLoop::new(postgres.clone(), None, &limits),
            renewal: Arc::new(LeaseRenewal::new(postgres.clone(), queue, limits.lease)),
            postgres,
            operator,
            queue,
            outbox,
            engines,
            cache: VerifierCache::default(),
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

    /// Resolve the claimed run's exact Verifier inside the claim transaction.
    ///
    /// A Verifier the claim reports deleted or missing resolves to the
    /// [`VERIFIER_UNAVAILABLE`] error the run terminates with. A cached
    /// Verifier is returned without a read; on a miss the Card is read on
    /// `conn`, parsed, and cached for its tenant.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the Card read fails, which aborts the claim
    /// transaction so the claim rolls back without consuming an attempt.
    async fn resolve(
        &self,
        conn: &mut TenantConn<'_>,
        run: &ClaimedRun,
    ) -> Result<Result<Arc<CachedVerifier>, VerificationError>, SqlError> {
        let unavailable = |cause: &dyn std::fmt::Display| {
            tracing::warn!(run_id = %run.lease.run_id, %cause, "the Verifier Card cannot be loaded");
            failure(VERIFIER_UNAVAILABLE, "the Verifier Card cannot be loaded")
        };
        if !run.verifier_present {
            return Ok(Err(unavailable(&"the Verifier Card is deleted")));
        }
        let tenant = conn.data_tenant_id();
        if let Some(cached) = self.cache.get(tenant, &run.verifier_uid) {
            return Ok(Ok(cached));
        }
        tracing::debug!(run_id = %run.lease.run_id, "Verifier Card cache miss");
        let Some(row) = fetch_card_row(conn, &run.verifier_uid).await? else {
            return Ok(Err(unavailable(&"the Verifier Card is deleted")));
        };
        let card = match ParsedCardRow::try_from(row) {
            Ok(card) => card,
            Err(error) => return Ok(Err(unavailable(&error))),
        };
        let Spec::Verifier(spec) = card.spec else {
            return Ok(Err(unavailable(&"the Card is not a Verifier")));
        };
        let reference = CardRef {
            kind: CardKind::Verifier,
            name: card.name,
            version: card.version,
            space: Some(card.space),
            uid: Some(card.card_uid),
        };
        Ok(Ok(self.cache.insert(
            tenant,
            run.verifier_uid.clone(),
            CachedVerifier::new(reference, spec.implementation),
        )))
    }

    /// Execute the run and map the attempt to a transition.
    ///
    /// A run without a loadable Verifier terminates; otherwise the Verifier
    /// executes and a completed report is built into its result write, which
    /// the caller stages only once the lease-fenced settlement completes the
    /// run. Never fails: every failure becomes the [`Transition`] it maps to.
    async fn execute(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        verifier: Result<Arc<CachedVerifier>, VerificationError>,
        telemetry: &ExecutionTelemetry,
    ) -> (Transition, Option<ScribeWrite>) {
        let verifier = match verifier {
            Ok(verifier) => verifier,
            Err(error) => return (Transition::Terminate(TerminalStatus::Errored, error), None),
        };
        telemetry.classify(VerifierKind::of(&verifier.implementation));
        let started_at = Utc::now();
        let executed = telemetry
            .phase(
                Phase::Engine,
                tokio::time::timeout(
                    self.limits.execution_timeout,
                    self.dispatch(tenant, run, &verifier.implementation, telemetry),
                )
                .instrument(tracing::info_span!("verification.engine")),
            )
            .await;
        let Ok(outcome) = executed else {
            return (
                Transition::Terminate(
                    TerminalStatus::TimedOut,
                    failure(
                        EXECUTION_TIMED_OUT,
                        "the Verifier exceeded its execution deadline",
                    ),
                ),
                None,
            );
        };
        let transition = match outcome {
            EngineOutcome::Completed(report) => {
                return result(run, &verifier.reference, &report, started_at);
            }
            EngineOutcome::Retry(error) => Transition::Retry(error),
            EngineOutcome::AwaitingTrace(error) => Transition::AwaitTrace(error),
            EngineOutcome::Deferred(error) => Transition::Defer(error),
            EngineOutcome::Terminal(status, error) => Transition::Terminate(status, error),
        };
        (transition, None)
    }

    /// The one closed dispatch over Verifier implementations.
    ///
    /// The run executes through [`VerifierEngines`] for `tenant`. Under `test-support` a scripted outcome, when queued,
    /// replaces the arm.
    async fn dispatch(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
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
            .execute(tenant, run, implementation, telemetry)
            .await
    }

    /// Apply `transition` to `run` in one tenant transaction.
    ///
    /// Returns the stable outcome label: `completed`, `retrying`,
    /// `exhausted`, `awaiting_trace`, `cancelled`, `timed_out`, `errored`,
    /// `released`, `deferred`, or `stale_lease` when another claim already
    /// holds the run. [`Transition::LeaseLost`] opens no transaction and is
    /// `stale_lease`.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction fails; nothing is applied.
    pub async fn settle(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        transition: Transition,
    ) -> Result<&'static str, SqlError> {
        if transition == Transition::LeaseLost {
            return Ok("stale_lease");
        }
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

    /// Every tenant with runnable runs, most overdue first.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant read fails.
    async fn due_tenants(&self) -> Result<Vec<DataTenantId>, SqlError> {
        Ok(self
            .queue
            .tenants_with_runnable_runs(&self.operator)
            .await?)
    }

    /// Claim the tenant's next runnable run under a fresh lease and resolve
    /// its Verifier in the same transaction.
    ///
    /// Opens the run's `verification.attempt` span before the claim statement
    /// and records the claimed run's scrubbed identity on it; the span then
    /// travels with the claim to settlement.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the claim or the Verifier read fails.
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
        let claimed = async {
            let Some(run) = self.queue.claim(conn, lease).await? else {
                return Ok::<_, SqlError>(None);
            };
            let resolving = Instant::now();
            let verifier = self
                .resolve(conn, &run)
                .instrument(tracing::info_span!("verification.load"))
                .await?;
            Ok(Some((run, verifier, (resolving, Instant::now()))))
        }
        .instrument(span.clone())
        .await?;
        Ok(claimed.map(|(run, verifier, loaded)| {
            span.record("run_id", display(run.lease.run_id));
            span.record("attempt", run.attempt);
            span.record("origin", run.origin.as_str());
            AttemptClaim {
                run,
                verifier,
                loaded,
                span,
            }
        }))
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
        run: &ClaimedRun,
        verifier: Result<Arc<CachedVerifier>, VerificationError>,
        loaded: (Instant, Instant),
        stop: &CancellationToken,
        abandon: &CancellationToken,
    ) -> bool {
        let started = Instant::now();
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Queued);
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
        let settled = telemetry
            .phase(
                Phase::Settlement,
                persist(lost, || self.settle(tenant, run, transition.clone()))
                    .instrument(tracing::info_span!("verification.settle")),
            )
            .await;
        drop(held);
        let outcome = match settled {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::error!(run_id = %run.lease.run_id, %error, "verification settlement failed; the lease will expire");
                "settlement_failed"
            }
        };
        // Staged only once this lease completed the run, so a stale holder's
        // or an unsettled attempt's result never reaches Bifrost.
        if outcome == "completed"
            && let Some(result) = result
        {
            self.outbox.stage(tenant, result);
        }
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
        refused
    }
}

/// Build `report`'s result payload once into the write that completes `run`.
///
/// Mints one result ID and one producer event time shared by every row — a
/// result fact this process observes, not a coordination instant — attributed
/// to the tenant SYSTEM principal and the exact `verifier`. The write is
/// returned rather than staged: only a settlement that still holds the lease
/// may stage it, so a holder that lost its lease writes no result.
///
/// A missing SYSTEM principal or an unbuildable report terminates `errored`
/// with no write.
fn result(
    run: &ClaimedRun,
    verifier: &CardRef,
    report: &VerifierReport,
    started_at: DateTime<Utc>,
) -> (Transition, Option<ScribeWrite>) {
    let principal = match system_principal(run) {
        Ok(principal) => principal,
        Err(transition) => return (transition, None),
    };
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
            return (
                Transition::Terminate(
                    TerminalStatus::Errored,
                    failure(RESULT_INVALID, "the verification result cannot be encoded"),
                ),
                None,
            );
        }
    };
    let verdict = payload.verdict();
    (
        Transition::Complete {
            result_id,
            verdict,
            summary: report.summary(),
            counts: report.counts(),
        },
        Some(ScribeWrite::Result {
            payload,
            attribution: VerifierAttribution {
                verifier: verifier.clone(),
                principal,
            },
        }),
    )
}

/// The tenant SYSTEM principal the claim returned, when it is an active
/// `UUIDv7` principal results can be attributed to.
///
/// # Errors
/// Terminates `errored` with [`SYSTEM_PRINCIPAL_MISSING`] otherwise.
fn system_principal(run: &ClaimedRun) -> Result<PrincipalId, Transition> {
    run.system_principal
        .filter(|principal| principal.as_uuid().get_version_num() == 7)
        .ok_or_else(|| {
            Transition::Terminate(
                TerminalStatus::Errored,
                failure(
                    SYSTEM_PRINCIPAL_MISSING,
                    "the tenant has no SYSTEM principal to attribute the result to",
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
        tracing::warn!(%error, "a verification run transaction failed; retrying while the lease holds");
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
            Self::Retry(error)
            | Self::AwaitTrace(error)
            | Self::Terminate(_, error)
            | Self::Defer(error) => Some(error),
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
