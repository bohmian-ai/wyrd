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
//! ends in exactly one fenced transition the runner applies itself. Every
//! terminal decision ends in one operation, [`conclude`]: the decision the
//! execution's state already accepted wins, else the offered one is recorded
//! in memory, and the claim, still holding its claim permit and its renewed
//! lease, awaits the state's background persistence off its computation. A
//! selected outcome then becomes one result payload, staged on a reserved
//! outbox slot of the process [`ScribeOutbox`] attributed to the execution's
//! subject and SYSTEM, waiting for capacity in background, and only once
//! Scribe acknowledges it does the matching fenced transaction settle it, so
//! a visible settlement always has its records durable.
//! A retryable failure is retried within the attempt budget; work abandoned
//! by shutdown is released with its attempt refunded; and work whose lease is
//! lost stops without settling. While an execution is in
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
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_verify::blocks::{Checkpoints, DriftChecks, EngineError};
use wyrd_runtime::AcceptedAuthority;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, PrincipalId};
use wyrd_spec::card::agent::ExecutionRoute;
use wyrd_spec::card::operator::DriftCounts;
use wyrd_spec::card::verifier::VerifierSpec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, ExecutionId, SlotName, VerificationResultId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::verification::execution::{ExecutedOutcome, InputDeclaration};
use wyrd_spec::verification::state::{ExecutionIdentity, TerminalSelection};
use wyrd_spec::verification::{DriftWindow, VerificationError, VerificationVerdict};
use wyrd_spec::verification::{TaskId, VerificationExecutionStatus, VerifierReadiness};
use wyrd_sql::queries::auth::service_accounts::system_principal_id;
use wyrd_sql::queries::cards::get_card_for_reconciliation;
use wyrd_sql::queries::verifier_executions::{
    ClaimedExecution, Completion, EnqueueRefusal, ExecutionInput, RetryOutcome, TerminalDelivery,
    TerminalStatus, VerifierExecutionQueue,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
use super::authority::{SystemReadAuthority, SystemReadAuthorityError};
use super::cache::{Db, Target, exact_ref};
use super::claims::{ClaimLoop, LeasedWork, settled};
use super::completions::Completions;
use super::drift::DriftEngine;
use super::engines::{self, EngineOutcome};
use super::health::RuntimeCapability;
use super::hosted::blocks::{Execution, HostedBlocks};
use super::inputs::{InputResolver, InputRow};
use super::leases::{HeldLease, LeaseRenewal};
use super::results::{ResultPayloadBuilder, ResultRun};
use super::state::{Decision, RunState, StateError, StateRow};
use super::telemetry::{ExecutionMode, ExecutionTelemetry, Phase};
use crate::components::auth::Caller;
use crate::components::gateway::GatewayCallContext;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::scribe_outbox::{ScribeOutbox, ScribeWrite};
use crate::state::AppState;

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
/// Stable error code when Scribe did not accept a selected result, so the
/// attempt is retried within its budget.
pub const RESULT_UNDELIVERED: &str = "result_undelivered";
/// Stable error code when a block was refused process capacity, so the
/// execution is deferred.
pub const RUNTIME_BUSY: &str = "verification_runtime_busy";
/// Stable error code when the submitted evidence does not conform to the
/// Verifier's input slots or exceeds an input bound, so no block ran.
pub const INPUT_INVALID: &str = "input_invalid";
/// Stable error code when the evidence metadata admission needs could not
/// be read, so no block ran.
pub const EVIDENCE_UNAVAILABLE: &str = "evidence_unavailable";
/// Stable error code when an exact Agent or Prompt Card the Verifier
/// requires is unavailable, such as deleted before its work started.
pub const CAPABILITY_UNAVAILABLE: &str = "capability_unavailable";
/// Stable error code when an execution's recoverable state cannot be read or
/// written now, so the execution is deferred rather than started fresh.
pub const STATE_UNAVAILABLE: &str = "execution_state_unavailable";
/// Stable error code when an execution's recoverable state is in a format
/// this worker does not run, so the execution is deferred unchanged.
pub const STATE_INCOMPATIBLE: &str = "execution_state_incompatible";
/// Stable error code when the tenant has no active `UUIDv7` SYSTEM principal
/// to execute and attribute the result as.
pub const SYSTEM_PRINCIPAL_MISSING: &str = "system_principal_missing";

/// How long, by the claim's PostgreSQL age, a queued execution waits without
/// spending attempts for its input record to publish: the same bound an
/// incomplete group selection retries within.
const PUBLICATION_BOUND: Duration = Duration::from_mins(5);

/// First wait before a failed settlement is tried again.
const PERSIST_INITIAL: Duration = Duration::from_millis(50);
/// How long a result write waits for outbox capacity in one reservation
/// before it waits again; the lease's loss ends the wait at once.
const CAPACITY_WAIT: Duration = Duration::from_secs(60);
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
    /// Settle without a verdict, the decision accepted in the execution's
    /// state when the execution has one.
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
    /// The exact Verifier, or why the run cannot run and what recording
    /// that needs.
    verifier: Result<Arc<Target>, Refused>,
    /// When the claim transaction started and finished resolving the
    /// Verifier, recorded as the attempt's `load` phase. The start anchors
    /// the claimed remaining deadline, so loading and handoff spend it.
    loaded: (Instant, Instant),
    /// The attempt's root span.
    span: tracing::Span,
}

/// A claimed execution whose target cannot run.
pub struct Refused {
    /// The error the execution ends with.
    error: VerificationError,
    /// What recording it needs.
    custody: Custody,
}

/// What recording the terminal decision of an execution without a resolved
/// target needs, read in its claim transaction.
pub struct Custody {
    /// The tenant SYSTEM principal the record is written as, when active.
    system: Option<PrincipalId>,
    /// The exact Verifier and subject Cards, deleted ones included, for an
    /// identity the state does not hold yet; `None` when either is unreadable.
    cards: Option<(CardRef, CardRef)>,
}

/// The terminal decision one claim offers its execution's state.
enum Offer {
    /// The decision the recovered state already accepted.
    Accepted(Decision),
    /// A completed outcome, computed from its instant.
    Selected(ExecutedOutcome, DateTime<Utc>),
    /// A failure without an outcome.
    Failed(TerminalStatus, VerificationError),
}

/// A claimed execution's prepared input: its drift checks, pinned evidence,
/// accepted routes, and application Run.
type Prepared = (
    DriftChecks,
    BTreeMap<SlotName, InputDeclaration>,
    BTreeMap<TaskId, ExecutionRoute>,
    Option<RunId>,
);

/// The frozen identities one execution's records carry.
#[derive(Clone, Copy)]
struct Frozen<'a> {
    /// The identities every result row repeats.
    run: ResultRun<'a>,
    /// The Verifier's UID.
    verifier_uid: &'a CardUid,
    /// The subject the execution serves.
    subject: PrincipalId,
}

impl<'a> Frozen<'a> {
    /// The identities claimed attempt `run` froze.
    fn attempt(run: &'a ClaimedExecution) -> Self {
        let (run_id, window) = input_parts(&run.input);
        Self {
            run: ResultRun {
                execution_id: run.lease.execution_id,
                run_id,
                verifier_version: &run.verifier_version,
                card_uid: &run.card_uid,
                owner_card_uid: owner(run),
                group_id: run.group_id.as_ref(),
                deployment_environment: run.deployment_environment.as_deref(),
                binding_id: run.binding_id,
                trigger: run.trigger.as_ref(),
                window,
            },
            verifier_uid: &run.verifier_card_uid,
            subject: run.attribution.sub_principal_id,
        }
    }

    /// The exact identity of `verifier` judging `subject` under these
    /// frozen group, owner, and environment.
    fn identity(self, (verifier, subject): (CardRef, CardRef)) -> ExecutionIdentity {
        ExecutionIdentity {
            verifier,
            subject,
            deployment_environment: self.run.deployment_environment.map(str::to_owned),
            group_id: self.run.group_id.cloned(),
            owner_card_uid: self.run.owner_card_uid.cloned(),
        }
    }

    /// The state row every revision records, written as `system` under
    /// `identity`.
    fn row(self, system: PrincipalId, identity: ExecutionIdentity) -> StateRow {
        StateRow {
            execution_id: self.run.execution_id,
            verifier_uid: self.verifier_uid.clone(),
            card_uid: self.run.card_uid.clone(),
            owner_card_uid: self.run.owner_card_uid.cloned(),
            run_id: self.run.run_id.cloned(),
            deployment_environment: self.run.deployment_environment.map(str::to_owned),
            attribution: Attribution::new(self.subject, system),
            identity,
        }
    }
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
    /// returns. Lease renewal runs beside the loop and stops once it
    /// returns.
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
    /// A refused target is [recorded](Self::record) without computing. An
    /// execution without a SYSTEM principal terminates errored with no state
    /// record. The attempt then recovers its accepted revision, whose writer
    /// stops once `lease` is lost: state that cannot be read or is in another
    /// format defers the execution, so it never restarts as fresh. A decision
    /// the state already accepted is concluded again without running the
    /// engine; an [`exhausted`](ClaimedExecution::exhausted) claim of state
    /// that decided nothing errors. An execution whose `deadline` has already
    /// passed times out without dispatching anything. Otherwise the input is
    /// [prepared](Self::prepare) and the engine runs from the recovered
    /// invocations, both until `deadline`; a completed outcome, a terminal
    /// failure, or a retryable failure of the final attempt is
    /// [concluded](conclude), and an earlier retryable failure or a deferral
    /// settles as it is. Never fails: every failure becomes the
    /// [`Transition`] it maps to.
    async fn execute(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        verifier: Result<Arc<Target>, Refused>,
        deadline: tokio::time::Instant,
        lease: &HeldLease,
        telemetry: &ExecutionTelemetry,
    ) -> Transition {
        let frozen = Frozen::attempt(run);
        let verifier = match verifier {
            Ok(verifier) => verifier,
            Err(refused) => {
                let Refused { error, custody } = refused;
                return self
                    .record(
                        tenant,
                        frozen,
                        &custody,
                        (TerminalStatus::Errored, error),
                        deadline,
                        lease,
                    )
                    .await;
            }
        };
        let Some(system) = system_principal(verifier.system_principal) else {
            return system_missing();
        };
        let state = match self
            .state(tenant, run, frozen, &verifier, system, deadline, lease)
            .await
        {
            Ok(state) => Arc::new(state),
            Err(transition) => return transition,
        };
        let offer = if let Some(decision) = Decision::of(&state.snapshot()) {
            Offer::Accepted(decision)
        } else if run.exhausted {
            Offer::Failed(
                TerminalStatus::Errored,
                failure(
                    "lease_expired",
                    "the execution's lease expired after its final attempt",
                ),
            )
        } else {
            let started_at = Utc::now();
            let outcome = if deadline <= tokio::time::Instant::now() {
                timed_out()
            } else {
                tokio::time::timeout_at(
                    deadline,
                    self.compute(tenant, run, &verifier.spec, system, &state, telemetry),
                )
                .await
                .unwrap_or_else(|_| timed_out())
            };
            match outcome {
                EngineOutcome::Completed(outcome) => Offer::Selected(outcome, started_at),
                EngineOutcome::Terminal(status, error) => Offer::Failed(status, error),
                EngineOutcome::Retry(error) if run.attempt >= run.max_attempts => {
                    Offer::Failed(TerminalStatus::Errored, error)
                }
                EngineOutcome::Retry(error) => {
                    return persisted(&state, lease, frozen.run.execution_id)
                        .await
                        .err()
                        .unwrap_or(Transition::Retry(error));
                }
                EngineOutcome::Deferred(error) => {
                    return persisted(&state, lease, frozen.run.execution_id)
                        .await
                        .err()
                        .unwrap_or(Transition::Defer(error));
                }
            }
        };
        conclude(tenant, frozen, system, &state, offer, lease, &self.outbox).await
    }

    /// Record `failure` as the decision of a claimed execution whose target
    /// was refused, which computes nothing, through [`conclude`], as the
    /// tenant SYSTEM principal in `custody`, until `deadline`.
    ///
    /// The accepted state is recovered first, and a decision it already
    /// holds wins; otherwise the failure is recorded under the state's
    /// identity, else the custody Cards' with the frozen group and owner. A
    /// missing SYSTEM principal or identity terminates with no state record,
    /// and unreadable state defers, so nothing is assumed absent.
    async fn record(
        &self,
        tenant: DataTenantId,
        frozen: Frozen<'_>,
        custody: &Custody,
        (status, error): (TerminalStatus, VerificationError),
        deadline: tokio::time::Instant,
        lease: &HeldLease,
    ) -> Transition {
        let execution_id = frozen.run.execution_id;
        let Some(system) = system_principal(custody.system) else {
            tracing::warn!(%execution_id, "the tenant has no SYSTEM principal; the failure is not recorded in the execution state");
            return Transition::Terminate(status, error);
        };
        let recovered = async {
            let query = self
                .reader(tenant, system, None)
                .await
                .map_err(|error| StateError::Unavailable(error.to_string()))?;
            RunState::accepted(
                &self.state,
                tenant,
                execution_id,
                &InputResolver::new(&query, None),
            )
            .await
        };
        let (revision, accepted) = match recovered.await {
            Ok(recovered) => recovered,
            Err(unavailable) => return state_failure(execution_id, &unavailable),
        };
        let identity = accepted
            .identity
            .clone()
            .or_else(|| custody.cards.clone().map(|cards| frozen.identity(cards)));
        let Some(identity) = identity else {
            tracing::warn!(%execution_id, "the execution's identity cannot be read; the failure is not recorded in the execution state");
            return Transition::Terminate(status, error);
        };
        let offer = match Decision::of(&accepted) {
            Some(decision) => Offer::Accepted(decision),
            None => Offer::Failed(status, error),
        };
        let state = RunState::at(
            Arc::clone(&self.outbox),
            tenant,
            frozen.row(system, identity),
            deadline,
            Some(lease.lost().clone()),
            accepted,
            revision,
        );
        conclude(tenant, frozen, system, &state, offer, lease, &self.outbox).await
    }

    /// Recover the execution's accepted state as `system` for this attempt,
    /// whose engine runs until `deadline` and whose writer stops once `lease`
    /// is lost.
    ///
    /// # Errors
    /// Returns the [`Transition`] a state that cannot be recovered ends this
    /// attempt in.
    #[expect(clippy::too_many_arguments, reason = "one attempt input per argument")]
    async fn state(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        frozen: Frozen<'_>,
        verifier: &Target,
        system: PrincipalId,
        deadline: tokio::time::Instant,
        lease: &HeldLease,
    ) -> Result<RunState, Transition> {
        let execution_id = run.lease.execution_id;
        let query = self
            .reader(tenant, system, Some(&run.authority))
            .await
            .map_err(|error| {
                state_failure(execution_id, &StateError::Unavailable(error.to_string()))
            })?;
        RunState::recover(
            &self.state,
            Arc::clone(&self.outbox),
            tenant,
            frozen.row(
                system,
                frozen.identity((verifier.verifier.clone(), verifier.subject.clone())),
            ),
            deadline,
            Some(lease.lost().clone()),
            &InputResolver::new(&query, None),
        )
        .await
        .map_err(|error| state_failure(execution_id, &error))
    }

    /// Prepare the claimed execution's frozen input, then run the engine
    /// over it, both in its `engine` phase. Under `test-support` a scripted
    /// outcome, when queued, replaces both.
    async fn compute(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        spec: &Arc<VerifierSpec>,
        system: PrincipalId,
        state: &Arc<RunState>,
        telemetry: &ExecutionTelemetry,
    ) -> EngineOutcome {
        let engine = async {
            #[cfg(feature = "test-support")]
            if let Some(script) = &self.script
                && let Some(outcome) = script.next().await
            {
                return outcome;
            }
            match self.prepare(tenant, run, spec, system, telemetry).await {
                Ok(prepared) => {
                    self.dispatch(run, spec, system, state, telemetry, prepared)
                        .await
                }
                Err(outcome) => outcome,
            }
        };
        telemetry
            .phase(
                Phase::Engine,
                engine.instrument(tracing::info_span!("verification.engine")),
            )
            .await
    }

    /// Read the claimed execution's frozen input before its engine runs.
    ///
    /// A scheduled window first scores every drift requirement over its
    /// frozen population as the tenant's SYSTEM read authority, then runs
    /// with no pinned evidence; a submission runs over its recorded inputs
    /// and route overrides; a group selection runs over its resolved source
    /// lists; all three are read from the execution's input record on every
    /// attempt.
    ///
    /// # Errors
    /// Returns the [`EngineOutcome`] the execution ends this attempt in.
    async fn prepare(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        spec: &Arc<VerifierSpec>,
        system: PrincipalId,
        telemetry: &ExecutionTelemetry,
    ) -> Result<Prepared, EngineOutcome> {
        Ok(match &run.input {
            ExecutionInput::Scheduled { .. } => {
                let selected = self
                    .recorded(tenant, run, system, async |_, row| row.schedule().map(Some))
                    .await?;
                let drift = self
                    .drift
                    .checks(tenant, run, system, spec, &selected, telemetry)
                    .await?;
                (drift, BTreeMap::new(), BTreeMap::new(), None)
            }
            ExecutionInput::Submitted { run_id } => {
                let (evidence, routes) = self.supplied(tenant, run, system).await?;
                (
                    DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                    evidence,
                    routes,
                    run_id.clone(),
                )
            }
            ExecutionInput::Grouped | ExecutionInput::PullRequest { .. } => {
                let (evidence, routes) = self.supplied(tenant, run, system).await?;
                (
                    DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                    evidence,
                    routes,
                    None,
                )
            }
        })
    }

    /// Run `spec` over the claimed execution's `prepared` input.
    ///
    /// The engine executes as the authority accepted with the execution,
    /// executed by `system`, with model calls attributed to the verified Card
    /// and the submission's application Run. Every attempt before the last
    /// defers a not-yet-landed trace, so the queue retries the same execution
    /// under its backoff; the last attempt judges it inconclusive and
    /// completes under the Verifier's decision rule. The engine records its
    /// progress in `state` and runs until its writer's deadline.
    async fn dispatch(
        &self,
        run: &ClaimedExecution,
        spec: &Arc<VerifierSpec>,
        system: PrincipalId,
        state: &Arc<RunState>,
        telemetry: &ExecutionTelemetry,
        (drift, evidence, routes, run_id): Prepared,
    ) -> EngineOutcome {
        let caller = Caller::accepted(&run.authority, system);
        let engine = match telemetry.time(Phase::Setup, || {
            self.blocks.engine(
                &self.state,
                Execution {
                    caller: &caller,
                    verifier: &run.verifier_card_uid,
                    routes: &routes,
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
                    execution: Some(Arc::clone(state)),
                    mode: ExecutionMode::Queued,
                },
            )
        }) {
            Ok(engine) => engine,
            Err(error) => {
                tracing::warn!(execution_id = %run.lease.execution_id, %error, "the execution engine cannot be built");
                return EngineOutcome::Retry(failure(
                    EXECUTION_INTERRUPTED,
                    "the verification engine cannot be built",
                ));
            }
        };
        let recorded = state.snapshot();
        let engine = engine.with_checkpoints(Arc::clone(state) as Arc<dyn Checkpoints>, &recorded);
        match engine
            .run(Arc::clone(spec), evidence, state.deadline())
            .await
        {
            Ok(outcome) => EngineOutcome::Completed(outcome),
            Err(error) => engine_failure(run, &error),
        }
    }

    /// Resolve a submitted or group execution's evidence and accepted routes
    /// from its input record through [`Self::recorded`].
    ///
    /// # Errors
    /// Returns the [`EngineOutcome`] the execution ends this attempt in.
    async fn supplied(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        system: PrincipalId,
    ) -> Result<
        (
            BTreeMap<SlotName, InputDeclaration>,
            BTreeMap<TaskId, ExecutionRoute>,
        ),
        EngineOutcome,
    > {
        self.recorded(tenant, run, system, async |records, row| {
            let routes = row.routes()?;
            Ok(records
                .evidence(&row)
                .await?
                .map(|evidence| (evidence, routes)))
        })
        .await
    }

    /// Read the execution's input record as the tenant's `system` principal
    /// granted only the verification input and output tables and serving the
    /// accepted authority, and resolve it through `resolve`, which answers
    /// `None` while a record it needs is not visible.
    ///
    /// A record not yet visible defers the execution without spending an
    /// attempt while it is younger than [`PUBLICATION_BOUND`], and retries
    /// within the attempt budget afterwards, so a record that never publishes
    /// ends the execution `errored` rather than deferring it forever. A
    /// failed read retries.
    ///
    /// # Errors
    /// Returns the [`EngineOutcome`] the execution ends this attempt in.
    async fn recorded<T>(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        system: PrincipalId,
        resolve: impl AsyncFnOnce(&InputResolver<'_>, InputRow) -> Result<Option<T>, WyrdError>,
    ) -> Result<T, EngineOutcome> {
        let unreadable = |error: &dyn std::fmt::Display| {
            tracing::warn!(execution_id = %run.lease.execution_id, %error, "the execution's input record cannot be read");
            EngineOutcome::Retry(failure(
                EVIDENCE_UNAVAILABLE,
                "the execution's input record cannot be read",
            ))
        };
        let unpublished = || {
            let error = failure(
                EVIDENCE_UNAVAILABLE,
                "the execution's input record is not visible yet",
            );
            if run.age < PUBLICATION_BOUND {
                EngineOutcome::Deferred(error)
            } else {
                EngineOutcome::Retry(error)
            }
        };
        let query = self
            .reader(tenant, system, Some(&run.authority))
            .await
            .map_err(|error| unreadable(&error))?;
        let records = InputResolver::new(&query, None);
        let row = records
            .record(run.lease.execution_id)
            .await
            .map_err(|error| unreadable(&error))?
            .ok_or_else(unpublished)?;
        resolve(&records, row)
            .await
            .map_err(|error| unreadable(&error))?
            .ok_or_else(unpublished)
    }

    /// A query caller reading as the tenant's `system` principal, granted only
    /// the verification input, output, and execution state tables, and
    /// serving `served`, a run's accepted authority, when given.
    ///
    /// # Errors
    /// Returns the [`SystemReadAuthorityError`] when the authority cannot be
    /// built.
    async fn reader(
        &self,
        tenant: DataTenantId,
        system: PrincipalId,
        served: Option<&AcceptedAuthority>,
    ) -> Result<ScheduledQueryCaller, SystemReadAuthorityError> {
        let tables = ["execution_inputs", "execution_outputs", "execution_events"]
            .map(|name| TableRef::new(BifrostNamespace::Verification, name));
        let mut read =
            SystemReadAuthority::resolve(&self.state, tenant, Some(system), &tables).await?;
        if let Some(served) = served {
            read = read.serving(served);
        }
        Ok(ScheduledQueryCaller::new(
            self.state.clone(),
            read.into_context(),
            CancellationToken::new(),
        ))
    }

    /// Apply `transition` to claimed `run` in one tenant transaction.
    ///
    /// A [`Transition::Complete`], whose result Scribe already acknowledged,
    /// first joins the group commit through [`Completions`], which settles it
    /// in one transaction with the tenant's other completing runs; only when
    /// no batch committed it does it settle alone here. A failed commit
    /// retried under the same lease completes the run with the same result
    /// ID, and a lost lease leaves the result written for a run another claim
    /// re-executes.
    ///
    /// Returns the stable outcome label: `completed`, `retrying`,
    /// `exhausted`, `cancelled`, `timed_out`, `errored`,
    /// `released`, `deferred`, or `stale_lease` when another claim already
    /// holds the run. [`Transition::LeaseLost`] opens no transaction and is
    /// `stale_lease`.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction fails; nothing is applied.
    async fn settle(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        transition: Transition,
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
            if let Some(settlement) = self.completions.complete(tenant, completion).await {
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
                    .terminate(
                        &mut conn,
                        lease,
                        status,
                        &error,
                        TerminalDelivery::Acknowledged,
                    )
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
                settled(
                    self.queue.release(&mut conn, lease, self.poll()).await?,
                    "deferred",
                )
            }
            Transition::LeaseLost => "stale_lease",
        };
        conn.commit().await?;
        Ok(outcome)
    }

    /// The poll interval, after which deferred work is due again.
    fn poll(&self) -> chrono::Duration {
        chrono::Duration::from_std(self.limits.poll_interval)
            .unwrap_or_else(|_| chrono::Duration::seconds(1))
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
    /// each under its own fresh lease, and resolve what each needs in the
    /// same transaction. A run whose target is refused reads the tenant
    /// SYSTEM principal and exact Cards its record needs.
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
    /// Returns [`SqlError`] when a claim, SYSTEM, or Verifier read fails;
    /// the caller's transaction then rolls every claim of the batch back.
    async fn claim(
        &self,
        conn: &mut TenantConn<'_>,
        limit: u32,
    ) -> Result<Vec<AttemptClaim>, SqlError> {
        let lease = chrono::Duration::from_std(self.limits.lease)
            .unwrap_or_else(|_| chrono::Duration::minutes(10));
        let mut claims = Vec::with_capacity(limit as usize);
        let resolving = Instant::now();
        let deadline = chrono::Duration::from_std(self.limits.execution_timeout)
            .unwrap_or_else(|_| chrono::Duration::minutes(5));
        let claimed = self.queue.claim(conn, lease, deadline, limit).await?;
        let statement_us = u64::try_from(resolving.elapsed().as_micros()).unwrap_or(u64::MAX);
        let batch = claimed.len();
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
                let verifier = match self.resolve(conn, &run).await? {
                    Ok(target) => Ok(target),
                    Err(error) => Err(Refused {
                        error,
                        custody: custody(conn, &run.verifier_card_uid, &run.card_uid).await?,
                    }),
                };
                // Bound before `verification.load` ends, so the load span is routed too.
                if let Ok(target) = &verifier
                    && let Some(system) = system_principal(target.system_principal)
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
    /// Records a run's spawn-to-first-execution delay since `spawned_at` on
    /// its attempt span. The claim loop tracks the task until settlement. Cancellation through `abandon`
    /// (shutdown past its grace) stops the execution and releases the lease;
    /// a retryable failure observed after `stop` is also released rather than
    /// charged an attempt, since the process, not the run, failed. Resolves
    /// to `true` when admission backpressure deferred the work.
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
    /// A settled deferral spends no attempt, so it records only its span's
    /// outcome and none of these series. An exhausted claim, which has no
    /// attempt to refund or spend, leaves its lease to expire into another
    /// exhausted claim instead of releasing, retrying, or deferring, so a
    /// dependency that is unavailable now never exhausts it falsely.
    /// Returns whether admission backpressure deferred the run.
    async fn attempt(
        &self,
        tenant: DataTenantId,
        run: &ClaimedExecution,
        verifier: Result<Arc<Target>, Refused>,
        loaded: (Instant, Instant),
        stop: &CancellationToken,
        abandon: &CancellationToken,
    ) -> bool {
        let started = Instant::now();
        // `remaining` and `age` were read by the claim statement, which
        // began after `loaded.0`; target loading and handoff spend them. The
        // Verifier's `limits.timeout_seconds` counts from admission, so queue
        // and retry waits spend it and a reclaim never replenishes it.
        let claimed = tokio::time::Instant::from_std(loaded.0);
        let mut deadline = claimed + run.remaining;
        if let Some(seconds) = verifier
            .as_ref()
            .ok()
            .and_then(|target| target.spec.limits.as_ref()?.timeout_seconds)
        {
            let limit = Duration::from_secs(u64::from(seconds)).saturating_sub(run.age);
            deadline = deadline.min(claimed + limit);
        }
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Queued, Some(loaded.0));
        telemetry.record_phase(Phase::Load, loaded.0, loaded.1);
        let origin = run.origin.as_str();
        let held = Arc::new(self.renewal.hold(tenant, run.lease.token, abandon));
        let lost = held.lost();
        let transition = tokio::select! {
            biased;
            () = abandon.cancelled() => Transition::Release,
            () = lost.cancelled() => Transition::LeaseLost,
            executed = self.execute(tenant, run, verifier, deadline, &held, &telemetry) => executed,
        };
        let transition = match transition {
            Transition::Release | Transition::Retry(_) | Transition::Defer(_) if run.exhausted => {
                Transition::LeaseLost
            }
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
        if outcome == "deferred" {
            return refused;
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

/// End one claim with its execution's one terminal decision, the operation
/// every attempt settles through.
///
/// `offer` is resolved against `state` in memory, which keeps a decision it
/// already accepted, and the claim then awaits [`RunState::persisted`] off
/// its computation, holding its claim permit and its renewed `lease`, so a
/// redelivery reuses the same decision and result identity. A failure then
/// settles [`Transition::Terminate`]. A selected outcome is built into its
/// result write and settles only once Scribe accepts it, through
/// [`deliver`]. A decision whose persistence fails maps through
/// [`state_failure`], and a lost lease settles nothing.
async fn conclude(
    tenant: DataTenantId,
    frozen: Frozen<'_>,
    system: PrincipalId,
    state: &RunState,
    offer: Offer,
    lease: &HeldLease,
    outbox: &Arc<ScribeOutbox>,
) -> Transition {
    let execution_id = frozen.run.execution_id;
    let decided = match offer {
        Offer::Accepted(decision) => Ok(decision),
        Offer::Selected(outcome, started_at) => state.select(&outcome, started_at),
        Offer::Failed(status, error) => state.fail(execution_status(status), &error),
    };
    let decided = match decided {
        Ok(decided) => decided,
        Err(_) if lease.lost().is_cancelled() => return Transition::LeaseLost,
        Err(error) => return state_failure(execution_id, &error),
    };
    if let Err(transition) = persisted(state, lease, execution_id).await {
        return transition;
    }
    match decided {
        Decision::Failed(failure) => {
            Transition::Terminate(terminal_status(failure.status), failure.error)
        }
        Decision::Selected(selected) => match result(frozen, &selected, system) {
            Ok((transition, write)) => deliver(tenant, transition, write, lease, outbox).await,
            Err(transition) => transition,
        },
    }
}

/// Wait until every change `state` holds is persisted, off the claim's
/// computation and under its renewed `lease`, so the next claim recovers
/// what this attempt recorded instead of an older revision.
///
/// # Errors
/// Returns [`Transition::LeaseLost`] when the lease is lost first, and the
/// [`state_failure`] of `execution_id` when persistence fails.
async fn persisted(
    state: &RunState,
    lease: &HeldLease,
    execution_id: ExecutionId,
) -> Result<(), Transition> {
    tokio::select! {
        biased;
        () = lease.lost().cancelled() => Err(Transition::LeaseLost),
        persisted = state.persisted() => match persisted {
            Ok(()) => Ok(()),
            Err(_) if lease.lost().is_cancelled() => Err(Transition::LeaseLost),
            Err(error) => Err(state_failure(execution_id, &error)),
        },
    }
}

/// Hand a selected outcome's `write` to Scribe and return the transition
/// the claim settles with once Scribe answers.
///
/// The write takes a reserved outbox slot, waiting in background while the
/// outbox is full, so it is never shed. `transition` stands once Scribe
/// acknowledges every row of the result, so the work settles only with its
/// result durable. A result Scribe rejects, or whose acknowledgement is
/// lost, retries the attempt with [`RESULT_UNDELIVERED`] within its budget,
/// and the next claim delivers the same selected result again. A lost
/// `lease` ends the wait; each caller races its own abandoning shutdown
/// around this one.
async fn deliver(
    tenant: DataTenantId,
    transition: Transition,
    mut write: ScribeWrite,
    lease: &HeldLease,
    outbox: &Arc<ScribeOutbox>,
) -> Transition {
    let (sender, acked) = tokio::sync::oneshot::channel();
    if let ScribeWrite::Result { acked, .. } = &mut write {
        *acked
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
    }
    let lost = lease.lost();
    let slot = loop {
        tokio::select! {
            biased;
            () = lost.cancelled() => return Transition::LeaseLost,
            reserved = outbox.reserve_until(1, Instant::now() + CAPACITY_WAIT) => {
                if let Ok(slot) = reserved {
                    break slot;
                }
            }
        }
    };
    slot.stage(tenant, write);
    tokio::select! {
        biased;
        () = lost.cancelled() => Transition::LeaseLost,
        delivered = acked => match delivered {
            Ok(()) => transition,
            Err(_) => Transition::Retry(failure(
                RESULT_UNDELIVERED,
                "Scribe did not accept the verification result",
            )),
        },
    }
}

/// Build the outcome `selected` into the write that completes the execution
/// `frozen` identifies.
///
/// The payload carries the selected result ID, which the completing
/// settlement records as the execution's result, and the selected interval,
/// so every delivery of one selected outcome writes the same result. The
/// write is attributed to the execution's subject with `system` as actor,
/// and returned rather than staged, so [`deliver`] stages it only while the
/// attempt still holds its lease.
///
/// # Errors
/// Returns the `errored` termination of an outcome that cannot be encoded,
/// with no write.
fn result(
    frozen: Frozen<'_>,
    selected: &TerminalSelection,
    system: PrincipalId,
) -> Result<(Transition, ScribeWrite), Transition> {
    let outcome = &selected.outcome;
    let built = ResultPayloadBuilder::new(
        frozen.run,
        frozen.verifier_uid,
        selected.started_at,
        selected.ended_at,
    )
    .build_as(selected.result_id, outcome);
    let payload = built.map_err(|error| {
        tracing::warn!(execution_id = %frozen.run.execution_id, %error, "verification result encoding failed");
        Transition::Terminate(
            TerminalStatus::Errored,
            failure(RESULT_INVALID, "the verification result cannot be encoded"),
        )
    })?;
    Ok((
        Transition::Complete {
            result_id: payload.result_id(),
            verdict: payload.verdict(),
            summary: engines::summary(outcome),
            drift: engines::drift_counts(outcome),
        },
        ScribeWrite::Result {
            payload,
            attribution: Attribution::new(frozen.subject, system),
            acked: Mutex::new(None),
        },
    ))
}

/// What recording the decision of an execution that will not compute needs,
/// read on `conn`: the tenant SYSTEM principal and the exact `verifier` and
/// `subject` Cards, deleted ones included. An unreadable Card is logged and
/// leaves the Cards out.
///
/// # Errors
/// Returns [`SqlError`] when the SYSTEM principal read fails.
async fn custody(
    conn: &mut TenantConn<'_>,
    verifier: &CardUid,
    subject: &CardUid,
) -> Result<Custody, SqlError> {
    let system = system_principal_id(conn)
        .await
        .map_err(SqlError::Query)?
        .map(PrincipalId::new);
    let mut cards = Vec::with_capacity(2);
    for uid in [verifier, subject] {
        match get_card_for_reconciliation(conn, uid).await {
            Ok(row) => cards.push(exact_ref(&row)),
            Err(error) => {
                tracing::warn!(card_uid = %uid, %error, "an execution's Card cannot be read for its terminal record");
            }
        }
    }
    let cards = <[CardRef; 2]>::try_from(cards)
        .ok()
        .map(|[verifier, subject]| (verifier, subject));
    Ok(Custody { system, cards })
}

/// The application Run and scheduled window `input` froze.
const fn input_parts(input: &ExecutionInput) -> (Option<&RunId>, Option<&DriftWindow>) {
    match input {
        ExecutionInput::Scheduled { window } => (None, Some(window)),
        ExecutionInput::Submitted { run_id } => (run_id.as_ref(), None),
        ExecutionInput::Grouped | ExecutionInput::PullRequest { .. } => (None, None),
    }
}

/// The owner Service `run`'s rows record: the binding owner, else the
/// accepted caller's owning root.
fn owner(run: &ClaimedExecution) -> Option<&wyrd_spec::ids::CardUid> {
    run.owner_card_uid
        .as_ref()
        .or_else(|| run.authority.owning_root())
}

/// The transition of a claim whose execution state cannot be used: state
/// not readable now or in another format defers, and state whose change was
/// not accepted retries, without ever starting fresh.
fn state_failure(execution_id: ExecutionId, error: &StateError) -> Transition {
    tracing::warn!(%execution_id, %error, "the execution state cannot be used");
    match error {
        StateError::Unavailable(_) => Transition::Defer(failure(
            STATE_UNAVAILABLE,
            "the execution state cannot be read or written now",
        )),
        StateError::Incompatible(_) => Transition::Defer(failure(
            STATE_INCOMPATIBLE,
            "the execution state is in a format this worker does not run",
        )),
        StateError::Stale => Transition::Retry(failure(
            EXECUTION_INTERRUPTED,
            "the execution state change was not accepted",
        )),
    }
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
        EngineError::DefinitionUnavailable(reason) => EngineOutcome::Terminal(
            TerminalStatus::Errored,
            failure(CAPABILITY_UNAVAILABLE, reason),
        ),
        EngineError::InputInvalid(reason) | EngineError::InputTooLarge(reason) => {
            EngineOutcome::Terminal(TerminalStatus::Errored, failure(INPUT_INVALID, reason))
        }
        EngineError::EvidenceUnavailable(_) => EngineOutcome::Retry(failure(
            EVIDENCE_UNAVAILABLE,
            "the verification evidence cannot be admitted now",
        )),
        EngineError::Checkpoint(_) => EngineOutcome::Retry(failure(
            EXECUTION_INTERRUPTED,
            "a verification checkpoint cannot commit",
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

/// `system`, when it is an active `UUIDv7` SYSTEM principal an execution can
/// run and be attributed as.
fn system_principal(system: Option<PrincipalId>) -> Option<PrincipalId> {
    system.filter(|principal| principal.as_uuid().get_version_num() == 7)
}

/// How an execution without a SYSTEM principal ends: errored, with no state
/// record, since there is no principal to write it as.
fn system_missing() -> Transition {
    Transition::Terminate(
        TerminalStatus::Errored,
        failure(
            SYSTEM_PRINCIPAL_MISSING,
            "the tenant has no SYSTEM principal to execute the Verifier as",
        ),
    )
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

/// The execution status a terminal status records.
const fn execution_status(status: TerminalStatus) -> VerificationExecutionStatus {
    match status {
        TerminalStatus::Cancelled => VerificationExecutionStatus::Cancelled,
        TerminalStatus::TimedOut => VerificationExecutionStatus::TimedOut,
        TerminalStatus::Errored => VerificationExecutionStatus::Errored,
    }
}

/// The terminal status a recorded failure's `status` settles; any status
/// but a cancellation or timeout errors.
const fn terminal_status(status: VerificationExecutionStatus) -> TerminalStatus {
    match status {
        VerificationExecutionStatus::Cancelled => TerminalStatus::Cancelled,
        VerificationExecutionStatus::TimedOut => TerminalStatus::TimedOut,
        _ => TerminalStatus::Errored,
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
