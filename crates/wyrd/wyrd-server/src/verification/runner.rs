//! The generic Verifier runner: claim, execute, publish, settle.
//!
//! [`VerifierRunner`] takes an execution permit before it claims, so a claim
//! is never made that this process cannot start. Each claimed run loads its
//! exact Verifier Card, goes through the one closed dispatch over
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
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::ids::VerificationResultId;
use wyrd_spec::reference::CardRef;
use wyrd_spec::verification::{VerificationError, VerificationVerdict};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verifier_runs::{
    ClaimedRun, RetryOutcome, RunInput, Settlement, TerminalStatus, VerifierRunQueue,
};
use wyrd_sql::{OperatorPool, SqlError, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
use super::drift::DriftEngine;
use super::engines::{self, EngineOutcome, VerifierReport};
#[cfg(feature = "test-support")]
use super::health::RuntimeCapability;
use super::permits::{VerifierPermit, VerifierPermits};
use super::publisher::ResultPublisher;
use super::results::{ResultPayloadBuilder, ResultRun};

/// Stable error code when the exact Verifier Card cannot be loaded or is not
/// a Verifier.
pub const VERIFIER_UNAVAILABLE: &str = "verifier_unavailable";
/// Stable error code when an engine exceeded its execution deadline.
pub const EXECUTION_TIMED_OUT: &str = "execution_timed_out";
/// Stable error code when a completed result could not be encoded.
pub const RESULT_INVALID: &str = "result_invalid";
/// Stable error code when a completed result was not durably acknowledged.
pub const RESULT_PUBLICATION_FAILED: &str = "result_publication_failed";

/// Tenants examined per claim round, most overdue first.
const TENANTS_PER_ROUND: i64 = 64;

/// The single transition one claimed run ends in.
#[derive(Debug, Clone, PartialEq)]
pub enum Transition {
    /// Every result batch was acknowledged; settle `completed` with this result.
    Complete {
        /// The published result.
        result_id: VerificationResultId,
        /// Its verdict.
        verdict: VerificationVerdict,
    },
    /// Try the same run and input again within its attempt budget.
    Retry(VerificationError),
    /// Settle without a verdict.
    Terminate(TerminalStatus, VerificationError),
    /// Return the run to its queue with its attempt refunded.
    Release,
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
    /// Global and per-tenant execution capacity.
    permits: VerifierPermits,
    /// Remote result publication.
    publisher: ResultPublisher,
    /// The Drift arm's engine.
    drift: DriftEngine,
    /// Runtime bounds.
    limits: RuntimeLimits,
    /// Test-only scripted engine outcomes.
    #[cfg(feature = "test-support")]
    script: Option<EngineScript>,
    /// Test-only crash switch.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
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
        permits: VerifierPermits,
        publisher: ResultPublisher,
        drift: DriftEngine,
        limits: RuntimeLimits,
    ) -> Self {
        Self {
            postgres,
            operator,
            queue,
            permits,
            publisher,
            drift,
            limits,
            #[cfg(feature = "test-support")]
            script: None,
            #[cfg(feature = "test-support")]
            crash: None,
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
        self.crash = Some(crash);
        self
    }

    /// Claim and execute runs until `stop` is cancelled, then drain.
    ///
    /// Each turn claims one run per tenant with capacity, round-robin, and
    /// spawns its execution; an empty round waits the poll interval or until
    /// an execution finishes. On `stop` no further claim is admitted (an
    /// uncommitted claim rolls back and a claim committed after `stop` is
    /// released unexecuted), executions spawned before `stop` get the drain
    /// grace to settle, and any still running are then cancelled and release
    /// their leases before this returns.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed a runner crash. The
    /// in-flight executions are aborted with the task and keep their leases
    /// until expiry.
    pub async fn run(self: Arc<Self>, stop: CancellationToken) {
        let abandon = CancellationToken::new();
        let mut work = JoinSet::new();
        while !stop.is_cancelled() {
            #[cfg(feature = "test-support")]
            if let Some(crash) = &self.crash {
                crash.check(RuntimeCapability::Runner);
            }
            let claimed = match self.claim_round(&stop, &abandon, &mut work).await {
                Ok(claimed) => claimed,
                Err(error) => {
                    tracing::warn!(%error, "verification claim round failed");
                    0
                }
            };
            while let Some(finished) = work.try_join_next() {
                reap(finished);
            }
            self.record_active();
            if claimed > 0 {
                continue;
            }
            tokio::select! {
                () = stop.cancelled() => {}
                () = tokio::time::sleep(self.limits.poll_interval) => {}
                Some(finished) = work.join_next(), if !work.is_empty() => reap(finished),
            }
        }
        let drained = tokio::time::timeout(self.limits.drain_grace, async {
            while let Some(finished) = work.join_next().await {
                reap(finished);
            }
        })
        .await;
        if drained.is_err() {
            tracing::warn!(
                in_flight = work.len(),
                "verification drain grace elapsed; releasing in-flight runs"
            );
            abandon.cancel();
            while let Some(finished) = work.join_next().await {
                reap(finished);
            }
        }
        self.record_active();
    }

    /// Claim at most one run for each runnable tenant that has capacity.
    ///
    /// Tenants come most overdue first. A permit is taken before the claim
    /// and dropped unused when the tenant had nothing claimable, so a
    /// saturated tenant never blocks another and the global ceiling bounds
    /// the process. Returns the number of runs claimed and spawned.
    ///
    /// `stop` closes admission at the durable boundary: a claim still in its
    /// transaction when `stop` fires is rolled back, and a claim whose commit
    /// completed after `stop` fired is immediately released with its attempt
    /// refunded instead of being spawned. Only claims spawned before `stop`
    /// fired are drained.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the runnable-tenant list or a claim fails;
    /// runs claimed earlier in the round are already spawned.
    async fn claim_round(
        self: &Arc<Self>,
        stop: &CancellationToken,
        abandon: &CancellationToken,
        work: &mut JoinSet<()>,
    ) -> Result<usize, SqlError> {
        if self.permits.saturated() {
            return Ok(0);
        }
        let tenants = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(0),
            tenants = self
                .queue
                .tenants_with_runnable_runs(&self.operator, TENANTS_PER_ROUND) => tenants?,
        };
        let mut claimed = 0;
        for tenant in tenants {
            if stop.is_cancelled() || self.permits.saturated() {
                break;
            }
            let Some(permit) = self.permits.try_acquire(tenant) else {
                continue;
            };
            let Some(run) = self.claim(tenant, stop).await? else {
                continue;
            };
            if stop.is_cancelled() {
                self.refund_late_claim(tenant, &run).await;
                break;
            }
            metrics::counter!(
                crate::app::metrics::VERIFICATION_RUN_ATTEMPTS_TOTAL,
                "implementation" => input_implementation(&run.input)
            )
            .increment(1);
            claimed += 1;
            work.spawn(Arc::clone(self).process(
                tenant,
                run,
                permit,
                stop.clone(),
                abandon.clone(),
            ));
        }
        Ok(claimed)
    }

    /// Claim the next runnable run of `tenant` under a fresh lease.
    ///
    /// Opening the tenant transaction and the queue claim race `stop`; when
    /// `stop` wins, the uncommitted claim transaction is dropped, which rolls
    /// it back, and `None` is returned. The commit itself is not raced, so
    /// its outcome is always known: a claim that commits is returned even if
    /// `stop` fired meanwhile, and the caller must then release it.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the tenant transaction or claim fails.
    async fn claim(
        &self,
        tenant: DataTenantId,
        stop: &CancellationToken,
    ) -> Result<Option<ClaimedRun>, SqlError> {
        let lease = chrono::Duration::from_std(self.limits.lease)
            .unwrap_or_else(|_| chrono::Duration::minutes(10));
        let uncommitted = async {
            let mut conn = self.postgres.tenant_conn(tenant).await?;
            let run = self.queue.claim(&mut conn, lease).await?;
            Ok::<_, SqlError>((conn, run))
        };
        let (conn, run) = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(None),
            claimed = uncommitted => claimed?,
        };
        conn.commit().await?;
        Ok(run)
    }

    /// Release `run`, claimed by a commit that completed after shutdown
    /// began, with its attempt refunded and without executing it.
    ///
    /// Uses the same fenced release transition as the drain. A failed release
    /// is logged; the lease then expires into a reclaim.
    async fn refund_late_claim(&self, tenant: DataTenantId, run: &ClaimedRun) {
        match self.settle(tenant, run, Transition::Release).await {
            Ok(outcome) => {
                tracing::info!(run_id = %run.lease.run_id, outcome, "claim committed after shutdown began; released");
            }
            Err(error) => {
                tracing::error!(run_id = %run.lease.run_id, %error, "releasing a claim committed after shutdown failed; the lease will expire");
            }
        }
    }

    /// Execute one claimed run and apply its single transition.
    ///
    /// Holds `permit` until settlement. Cancellation through `abandon`
    /// (shutdown past its grace) stops the execution and releases the lease;
    /// a retryable failure observed after `stop` is also released rather than
    /// charged an attempt, since the process, not the run, failed.
    async fn process(
        self: Arc<Self>,
        tenant: DataTenantId,
        run: ClaimedRun,
        permit: VerifierPermit,
        stop: CancellationToken,
        abandon: CancellationToken,
    ) {
        let _permit = permit;
        let started = Instant::now();
        let transition = tokio::select! {
            () = abandon.cancelled() => Transition::Release,
            transition = self.execute(tenant, &run) => transition,
        };
        let transition = match transition {
            Transition::Retry(_) if stop.is_cancelled() => Transition::Release,
            transition => transition,
        };
        let implementation = input_implementation(&run.input);
        let outcome = match self.settle(tenant, &run, transition).await {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::error!(run_id = %run.lease.run_id, %error, "verification settlement failed; the lease will expire");
                "settlement_failed"
            }
        };
        if outcome != "completed" && outcome != "released" {
            metrics::counter!(
                crate::app::metrics::VERIFICATION_RUN_FAILURES_TOTAL,
                "implementation" => implementation,
                "outcome" => outcome
            )
            .increment(1);
        }
        metrics::histogram!(
            crate::app::metrics::VERIFICATION_RUN_DURATION_SECONDS,
            "implementation" => implementation,
            "outcome" => outcome
        )
        .record(started.elapsed().as_secs_f64());
    }

    /// Load the Verifier, dispatch it, and publish a completed report.
    ///
    /// Never fails: every failure becomes the [`Transition`] it maps to.
    async fn execute(&self, tenant: DataTenantId, run: &ClaimedRun) -> Transition {
        let (verifier, implementation) = match self.load_verifier(tenant, run).await {
            Ok(loaded) => loaded,
            Err(transition) => return transition,
        };
        let started_at = Utc::now();
        let outcome = match tokio::time::timeout(
            self.limits.execution_timeout,
            self.dispatch(tenant, run, &verifier, &implementation),
        )
        .await
        {
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
                self.publish(tenant, run, &verifier, &report, started_at)
                    .await
            }
            EngineOutcome::Retry(error) => Transition::Retry(error),
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
        let unavailable = |message: String| failure(VERIFIER_UNAVAILABLE, &message);
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| Transition::Retry(unavailable(error.to_string())))?;
        let card = get_card_by_uid(&mut conn, &run.verifier_uid)
            .await
            .map_err(|error| {
                if error.status() >= 500 {
                    Transition::Retry(unavailable(error.to_string()))
                } else {
                    Transition::Terminate(TerminalStatus::Errored, unavailable(error.to_string()))
                }
            })?;
        drop(conn);
        let Spec::Verifier(spec) = card.spec else {
            return Err(Transition::Terminate(
                TerminalStatus::Errored,
                unavailable(format!("card {} is not a Verifier", run.verifier_uid)),
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
    /// Drift executes through the runner's [`DriftEngine`] for `tenant`,
    /// reading as the SYSTEM principal scoped to the exact `verifier`. Under
    /// `test-support` a scripted outcome, when queued, replaces the arm.
    async fn dispatch(
        &self,
        tenant: DataTenantId,
        run: &ClaimedRun,
        verifier: &CardRef,
        implementation: &VerifierImplementation,
    ) -> EngineOutcome {
        #[cfg(feature = "test-support")]
        if let Some(script) = &self.script
            && let Some(outcome) = script.next().await
        {
            return outcome;
        }
        match implementation {
            VerifierImplementation::Drift(spec) => {
                self.drift.verify(tenant, verifier, run, spec).await
            }
            VerifierImplementation::Eval(spec) => engines::eval(run, spec).await,
        }
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
                return Transition::Terminate(
                    TerminalStatus::Errored,
                    failure(RESULT_INVALID, &error.to_string()),
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
            },
            Ok(Err(error)) => {
                tracing::warn!(run_id = %run.lease.run_id, %error, "verification result publication failed");
                Transition::Retry(failure(RESULT_PUBLICATION_FAILED, &error.to_string()))
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
    /// `exhausted`, `cancelled`, `timed_out`, `errored`, `released`, or
    /// `stale_lease` when another claim already holds the run.
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
            Transition::Complete { result_id, verdict } => settled(
                self.queue
                    .complete(&mut conn, lease, result_id, verdict)
                    .await?,
                "completed",
            ),
            Transition::Retry(error) => match self.queue.retry(&mut conn, lease, &error).await? {
                RetryOutcome::Scheduled(_) => "retrying",
                RetryOutcome::Exhausted => "exhausted",
                RetryOutcome::StaleLease => "stale_lease",
            },
            Transition::Terminate(status, error) => settled(
                self.queue
                    .terminate(&mut conn, lease, status, &error)
                    .await?,
                terminal_label(status),
            ),
            Transition::Release => settled(self.queue.release(&mut conn, lease).await?, "released"),
        };
        conn.commit().await?;
        Ok(outcome)
    }

    /// Publish the active-execution gauge.
    fn record_active(&self) {
        metrics::gauge!(crate::app::metrics::VERIFICATION_ACTIVE_RUNS).set(f64::from(
            u32::try_from(self.permits.active()).unwrap_or(u32::MAX),
        ));
    }
}

/// Log an execution task that panicked; its lease expires into a reclaim.
fn reap(finished: Result<(), JoinError>) {
    if let Err(error) = finished
        && error.is_panic()
    {
        tracing::error!(%error, "verification execution panicked; its lease will expire");
    }
}

/// `label` when the settlement applied, otherwise `stale_lease`.
const fn settled(settlement: Settlement, label: &'static str) -> &'static str {
    match settlement {
        Settlement::Applied => label,
        Settlement::StaleLease => "stale_lease",
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

/// The implementation a frozen input belongs to, for telemetry labels.
const fn input_implementation(input: &RunInput) -> &'static str {
    match input {
        RunInput::DriftWindow(_) => "drift",
        RunInput::EvalRecord { .. } => "eval",
    }
}

/// A run failure with a stable `code` and diagnostic `message`.
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
