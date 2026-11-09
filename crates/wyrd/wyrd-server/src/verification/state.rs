//! The recoverable state of one running Verifier execution.
//!
//! [`RunState`] is the one owner of an execution's [`ExecutionState`] while
//! it runs, for direct and queued executions alike. Every revision is one
//! `bifrost.verification.execution_events` row written under a batch id
//! derived from its execution and revision, so the first batch Scribe fences
//! keeps the revision and another writer's different rows for it are refused
//! as a batch conflict. Recovery finds the accepted revision from those
//! generic batch fences, which commit before any acknowledgement and are
//! never removed, and reads exactly that revision's row: no fence is
//! authoritative absence, and a fenced revision not visible yet, such as one
//! a Scribe unavailable to the read still holds, is
//! [`StateError::Unavailable`], never older or empty state. A direct
//! execution's identity is new, so it starts fresh with no read. Steps change
//! the live state in memory under a short lock and wait, outside it, for an
//! accepted revision holding their change before its effect escapes. At most
//! one revision per execution is pending: it freezes every change made so
//! far into one complete revision staged on the process Scribe outbox, while
//! later changes accumulate for the revision after it. A revision refused or
//! whose acceptance is unknown breaks the state: nothing more is released,
//! and the attempt recovers the accepted revision again. A delayed old
//! worker that still passes its lease check can race a replacement for one
//! revision; whichever batch is fenced first keeps it, and the other writer
//! breaks.

use std::sync::{Arc, Mutex, OnceLock};

use chrono::{DateTime, Utc};
use futures_util::FutureExt as _;
use futures_util::future::{BoxFuture, Shared};
use sha2::{Digest as _, Sha256};
use tokio::sync::oneshot;
use vala_bifrost_redux::contracts::committed_batches;
use vala_bifrost_redux::tables::staged::StagedRow;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, ExecutionId, VerificationResultId};
use wyrd_spec::storage::StoredObjectRef;
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    InvocationSettlement, Producer, Publication, PublicationKind, VerificationOutcome,
};
use wyrd_spec::verification::state::{
    AcceptedPublication, ControlRecord, ExecutionIdentity, ExecutionState, ExecutionStateError,
    InvocationRecord, TerminalFailure, TerminalSelection,
};
use wyrd_spec::verification::{VerificationError, VerificationExecutionStatus};

use super::inputs::InputResolver;
use super::leases::HeldLease;
use super::results::{ResultPayloadBuilder, ResultRun};
use crate::scribe_outbox::{ScribeOutbox, ScribeWrite, revision_batch_id};
use crate::state::AppState;

/// The table every revision is a row of, and the logical table of every
/// revision's batch fence.
const STATE_TABLE: &str = "bifrost.verification.execution_events";

/// How many revisions one fence lookup asks about while recovery finds the
/// accepted revision.
// ponytail: a linear walk of PROBE revisions per statement; gallop if executions grow far past it.
const PROBE: i64 = 256;

/// How long a terminal decision's revision is awaited at least, so a
/// decision made at or past its writer's deadline, such as a timeout, is
/// still recorded.
pub(super) const DECISION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The batch identity of `revision` of `execution`'s state, which the
/// outbox writes under its [`revision_fence`].
///
/// Derived from both, so every writer of one revision offers the same batch
/// id: an honest retry dedups and a different revision's rows conflict.
#[must_use]
pub fn revision_batch(execution: ExecutionId, revision: i64) -> uuid::Uuid {
    let digest = Sha256::new()
        .chain_update(execution.as_uuid().as_bytes())
        .chain_update(revision.to_be_bytes())
        .finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    uuid::Builder::from_custom_bytes(bytes).into_uuid()
}

/// The Scribe batch id `tenant`'s `revision` of `execution` is written
/// under, whose batch fence is exactly whether the revision was accepted.
#[must_use]
pub fn revision_fence(tenant: DataTenantId, execution: ExecutionId, revision: i64) -> uuid::Uuid {
    revision_batch_id(tenant, revision_batch(execution, revision))
}

/// Why an execution's state cannot be used.
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateError {
    /// The accepted revision cannot be read or written now: its fence or
    /// row is not reachable or not visible yet, or Scribe has no capacity.
    /// Nothing was assumed absent; recovery is retried.
    #[error("the execution state is unavailable: {0}")]
    Unavailable(String),
    /// The accepted revision is in a format this worker does not run.
    #[error(transparent)]
    Incompatible(#[from] ExecutionStateError),
    /// A revision was refused, could not be staged, or its acceptance is
    /// unknown; this writer releases nothing more.
    #[error("the execution state no longer accepts this writer's changes")]
    Stale,
}

/// The one terminal decision an execution's state holds: a selected
/// outcome or a recorded failure, whichever was accepted first.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// The outcome the execution completes with.
    Selected(TerminalSelection),
    /// How the execution ended without an outcome.
    Failed(TerminalFailure),
}

impl Decision {
    /// The decision `state` holds, if any. A state holding both, which no
    /// writer produces, is read as its selection.
    #[must_use]
    pub fn of(state: &ExecutionState) -> Option<Self> {
        state
            .terminal
            .clone()
            .map(Self::Selected)
            .or_else(|| state.failure.clone().map(Self::Failed))
    }
}

/// The execution identities every revision's row records.
#[derive(Debug, Clone)]
pub struct StateRow {
    /// The execution.
    pub execution_id: ExecutionId,
    /// The Verifier running it.
    pub verifier_uid: CardUid,
    /// The Card verified.
    pub card_uid: CardUid,
    /// The owner Service, when known.
    pub owner_card_uid: Option<CardUid>,
    /// The application Run judged, when there is one.
    pub run_id: Option<RunId>,
    /// The accepted deployment environment tag.
    pub deployment_environment: Option<String>,
    /// The execution's subject and actor, which Scribe stamps on each row.
    pub attribution: Attribution,
    /// The exact identity every revision records.
    pub identity: ExecutionIdentity,
}

/// A stand-in identity naming `verifier` and `subject` by UID, for writers
/// whose exact names do not matter.
///
/// # Panics
/// Panics only if its static name or version failed to parse, which they
/// never do.
#[cfg(any(test, feature = "test-support"))]
#[must_use]
pub fn stand_in_identity(verifier: &CardUid, subject: &CardUid) -> ExecutionIdentity {
    let exact = |kind, name: &str, uid: &CardUid| wyrd_spec::reference::CardRef {
        kind,
        name: wyrd_spec::ids::CardName::new(name).expect("a static Card name is valid"),
        version: wyrd_semver::VersionBlock::parse("1.0.0").expect("a static version is valid"),
        uid: Some(uid.clone()),
    };
    ExecutionIdentity {
        verifier: exact(
            wyrd_spec::envelope::CardKind::Verifier,
            "verifier",
            verifier,
        ),
        subject: exact(wyrd_spec::envelope::CardKind::Model, "subject", subject),
        deployment_environment: None,
        group_id: None,
        owner_card_uid: None,
    }
}

/// The call that asks to accept one publication.
#[derive(Debug, Clone, Copy)]
pub struct PublicationCall<'a> {
    /// The publication to accept.
    pub publication: &'a Publication,
    /// The accepting call's identity, stable across its replays.
    pub call_id: &'a str,
    /// Digest of the call's canonical arguments.
    pub arguments: &'a Sha256Hex,
    /// The completed File descriptor of each manifest file, in manifest
    /// order; empty for a JSON publication.
    pub objects: &'a [StoredObjectRef],
    /// The bytes the publication retains: a JSON value's serialized length,
    /// or the sum of its manifest's file sizes.
    pub retained_bytes: u64,
}

/// The effective cumulative ceilings of one execution's publications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationBudget {
    /// Most publications the execution may accept across all attempts.
    pub max_count: u32,
    /// Most bytes its accepted publications may retain together.
    pub max_bytes: u64,
}

/// The cumulative ceiling a refused publication would cross.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationLimit {
    /// [`PublicationBudget::max_count`].
    Count,
    /// [`PublicationBudget::max_bytes`].
    Bytes,
}

/// Outcome of accepting one publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationAcceptance {
    /// The publication is part of the accepted state.
    Accepted,
    /// The same call with the same arguments already published it; the
    /// accepted kind is its receipt.
    Replayed(PublicationKind),
    /// Another call, or the same call with other arguments, holds the name
    /// in this attempt, or the same call already published another name.
    Conflict,
    /// Accepting it would cross a cumulative ceiling; nothing changed.
    OverBudget(PublicationLimit),
    /// The writer no longer holds the state, its lease, or its deadline;
    /// nothing changed.
    StaleLease,
}

/// One revision's acceptance, shared by every caller waiting on it, so a
/// cancelled waiter leaves the others to drive it.
type Acceptance = Shared<BoxFuture<'static, Result<(), StateError>>>;

/// The one frozen revision of an execution awaiting acceptance.
#[derive(Clone)]
struct Pending {
    /// Its revision.
    revision: i64,
    /// The last change it holds: it holds every ticket up to this one.
    covers: u64,
    /// Its acceptance.
    accepted: Acceptance,
}

/// The live state, and how much of it Scribe accepted from this writer.
struct Held {
    /// The live state steps read and change.
    state: ExecutionState,
    /// Changes applied to `state` since the claim; a change's ticket is this
    /// count once it is applied.
    changes: u64,
    /// The accepted revision; 0 before any.
    revision: i64,
    /// The last change the accepted revision holds.
    covered: u64,
    /// The frozen revision awaiting acceptance, at most one.
    pending: Option<Pending>,
    /// Set once a revision was refused, could not be staged, or its
    /// acceptance is unknown.
    broken: bool,
}

impl Held {
    /// `state`, accepted at `revision`, with nothing pending.
    const fn new(state: ExecutionState, revision: i64) -> Self {
        Self {
            state,
            changes: 0,
            revision,
            covered: 0,
            pending: None,
            broken: false,
        }
    }

    /// Records one change applied to the live state and returns its ticket.
    const fn changed(&mut self) -> u64 {
        self.changes += 1;
        self.changes
    }
}

/// How one writer stages revisions: everything a revision is written with
/// except its state, shared with the pending revision's acceptance.
struct StateScribe {
    /// The outbox every revision is staged on.
    outbox: Arc<ScribeOutbox>,
    /// The execution's tenant.
    tenant: DataTenantId,
    /// The identities every revision's row records.
    row: Arc<StateRow>,
    /// The instant this writer's attempt ends; no revision is awaited past
    /// it.
    deadline: tokio::time::Instant,
    /// The queued execution's lease, confirmed before each revision is
    /// frozen so a cancelled, expired, or reclaimed run stops changing its
    /// state; `None` for a direct execution.
    lease: Option<Arc<HeldLease>>,
}

impl StateScribe {
    /// Whether this writer may freeze another revision: its lease, when it
    /// has one, still holds its run.
    async fn confirmed(&self) -> bool {
        match &self.lease {
            Some(lease) => matches!(lease.confirm().await, Ok(true)),
            None => true,
        }
    }

    /// Stage `state` as `revision` under the revision's batch identity,
    /// which every retry and every writer of it keeps, and wait for Scribe to
    /// accept it until `deadline`. The outbox writer prepares the revision's row, so this
    /// caller does no encoding. It holds no connection while it waits, so it
    /// may wait unpolled until a waiter drives it.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] when the revision is refused, such as
    /// when another writer's rows already hold it or it cannot be encoded,
    /// or unacknowledged by `deadline`, and [`StateError::Unavailable`] when
    /// no outbox capacity frees by it.
    async fn write(
        self: Arc<Self>,
        revision: i64,
        state: ExecutionState,
        deadline: tokio::time::Instant,
    ) -> Result<(), StateError> {
        let slot = self
            .outbox
            .reserve_until(1, deadline.into_std())
            .await
            .map_err(|full| StateError::Unavailable(full.to_string()))?;
        let (sender, acked) = oneshot::channel();
        slot.stage(
            self.tenant,
            ScribeWrite::ExecutionState {
                batch: revision_batch(self.row.execution_id, revision),
                checkpoint: Arc::new(Checkpoint::new(revision, state, Arc::clone(&self.row))),
                attribution: self.row.attribution,
                acked: Mutex::new(Some(sender)),
            },
        );
        match tokio::time::timeout_at(deadline, acked).await {
            Ok(Ok(())) => Ok(()),
            _ => Err(StateError::Stale),
        }
    }
}

/// One frozen revision staged on the Scribe outbox, whose writer prepares
/// its row on the outbox's bounded encoding pool, once, so every retry of
/// the revision submits the same row.
#[derive(Debug)]
pub struct Checkpoint {
    /// The revision this is.
    revision: i64,
    /// The complete state it records.
    state: ExecutionState,
    /// The identities its row records.
    row: Arc<StateRow>,
    /// The memory it is charged, fixed when it is frozen.
    weight: usize,
    /// The prepared row, or why it cannot be prepared.
    prepared: OnceLock<Result<StagedRow, String>>,
}

impl Checkpoint {
    /// Bytes each entry of the state is charged beyond its encoded length,
    /// for its in-memory structure and its prepared row's per-value overhead.
    const ENTRY_BYTES: usize = 512;

    /// Times its encoded length the state is charged: the frozen state, the
    /// JSON tree and Variant its preparation builds, and the prepared row
    /// holding that Variant all hold its values at once.
    const COPIES: usize = 4;

    /// `state` frozen as `revision` of the execution `row` identifies, not
    /// prepared yet.
    #[must_use]
    pub fn new(revision: i64, state: ExecutionState, row: Arc<StateRow>) -> Self {
        let weight = Self::charge(&state);
        Self {
            revision,
            state,
            row,
            weight,
            prepared: OnceLock::new(),
        }
    }

    /// The memory `state` is charged: [`Self::COPIES`] times its compact
    /// JSON length, which every settlement, captured value, reason, outcome,
    /// identity, failure, and publication descriptor adds to, plus
    /// [`Self::ENTRY_BYTES`] per entry.
    ///
    /// The length is counted as the state streams into a sink that keeps no
    /// byte, so the charge follows every retained value without building the
    /// row its preparation does later on the outbox's encoding pool.
    // ponytail: a counting pass over the state per revision; charge per-entry deltas if large states make it costly.
    fn charge(state: &ExecutionState) -> usize {
        /// A sink counting the bytes written to it.
        struct Counted(usize);
        impl std::io::Write for Counted {
            /// Count `bytes`, saturating, and keep none of them.
            ///
            /// # Errors
            /// Never fails: every byte is accepted.
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.saturating_add(bytes.len());
                Ok(bytes.len())
            }
            /// Nothing is buffered, so there is nothing to flush.
            ///
            /// # Errors
            /// Never fails.
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut counted = Counted(0);
        // A state that does not encode is refused by its preparation; what
        // was counted before the error still bounds its memory.
        let _ = serde_json::to_writer(&mut counted, state);
        let objects: usize = state.publications.iter().map(|p| p.objects.len()).sum();
        let entries = 1 + state.invocations.len() + state.publications.len() + objects;
        counted
            .0
            .saturating_mul(Self::COPIES)
            .saturating_add(Self::ENTRY_BYTES.saturating_mul(entries))
    }

    /// A revision whose row is already prepared, such as one written in
    /// another state format.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn prepared(revision: i64, row: Arc<StateRow>, prepared: StagedRow) -> Self {
        Self {
            revision,
            weight: prepared.owned_bytes(),
            state: ExecutionState::fresh(),
            row,
            prepared: OnceLock::from(Ok(prepared)),
        }
    }

    /// The execution whose revision this is.
    #[must_use]
    pub fn execution_id(&self) -> ExecutionId {
        self.row.execution_id
    }

    /// Memory the outbox charges while the revision waits, fixed when it
    /// was frozen, so admission, every retry, and release charge the same.
    #[must_use]
    pub const fn weight(&self) -> usize {
        self.weight
    }

    /// The revision's `bifrost.verification.execution_events` row, prepared
    /// on first use.
    ///
    /// # Errors
    /// Returns why the state cannot be encoded as its row.
    pub fn row(&self) -> Result<&StagedRow, &str> {
        self.prepared
            .get_or_init(|| self.prepare())
            .as_ref()
            .map_err(String::as_str)
    }

    /// Encodes the state as its revision's row.
    ///
    /// # Errors
    /// Returns the encoding error's message when the state does not encode
    /// as JSON, or its checkpoint row does not build, such as when the value
    /// exceeds a Variant bound.
    fn prepare(&self) -> Result<StagedRow, String> {
        let value = serde_json::to_value(&self.state).map_err(|error| error.to_string())?;
        let row = &self.row;
        ResultPayloadBuilder::new(
            ResultRun {
                execution_id: row.execution_id,
                run_id: row.run_id.as_ref(),
                verifier_version: "",
                card_uid: &row.card_uid,
                owner_card_uid: row.owner_card_uid.as_ref(),
                group_id: None,
                deployment_environment: row.deployment_environment.as_deref(),
                binding_id: None,
                trigger: None,
                window: None,
            },
            &row.verifier_uid,
            Utc::now(),
            Utc::now(),
        )
        .checkpoint(self.revision, self.state.terminal.is_some(), &value)
        .map(|row| row.row)
        .map_err(|error| error.to_string())
    }
}

/// The recoverable state of one running execution and its only writer.
pub struct RunState {
    /// How revisions are staged.
    scribe: Arc<StateScribe>,
    /// The live state and its acceptance, locked only to read or change
    /// memory, never across IO.
    held: tokio::sync::Mutex<Held>,
}

impl RunState {
    /// Recover `tenant`'s execution state for a new writer whose attempt
    /// ends at `deadline`, through `records`, whose read authority must cover
    /// `bifrost.verification.execution_events`. `lease`, when the execution
    /// is queued, is confirmed before each revision is frozen.
    ///
    /// # Errors
    /// Returns [`StateError::Unavailable`] when the accepted revision cannot
    /// be found or read, and [`StateError::Incompatible`] for a revision in
    /// another format.
    pub async fn recover(
        app: &AppState,
        outbox: Arc<ScribeOutbox>,
        tenant: DataTenantId,
        row: StateRow,
        deadline: tokio::time::Instant,
        lease: Option<Arc<HeldLease>>,
        records: &InputResolver<'_>,
    ) -> Result<Self, StateError> {
        let (revision, state) = Self::accepted(app, tenant, row.execution_id, records).await?;
        Ok(Self::at(
            outbox, tenant, row, deadline, lease, state, revision,
        ))
    }

    /// The state of an execution identity no writer ever advanced, such as
    /// a direct execution's, for a new writer with no lease, waiting for each
    /// revision's acceptance until `deadline`.
    ///
    /// A direct execution's identity is new to its request and never
    /// resumed, so its state starts fresh and nothing is read.
    #[must_use]
    pub fn fresh(
        outbox: Arc<ScribeOutbox>,
        tenant: DataTenantId,
        row: StateRow,
        deadline: tokio::time::Instant,
    ) -> Self {
        Self::at(
            outbox,
            tenant,
            row,
            deadline,
            None,
            ExecutionState::fresh(),
            0,
        )
    }

    /// A writer of `state`, accepted at `revision`, which records `row`'s
    /// identity unless the state already holds one.
    pub(crate) fn at(
        outbox: Arc<ScribeOutbox>,
        tenant: DataTenantId,
        row: StateRow,
        deadline: tokio::time::Instant,
        lease: Option<Arc<HeldLease>>,
        mut state: ExecutionState,
        revision: i64,
    ) -> Self {
        state.identity.get_or_insert_with(|| row.identity.clone());
        Self {
            scribe: Arc::new(StateScribe {
                outbox,
                tenant,
                row: Arc::new(row),
                deadline,
                lease,
            }),
            held: tokio::sync::Mutex::new(Held::new(state, revision)),
        }
    }

    /// The instant this writer's attempt ends; no revision is awaited past it.
    #[must_use]
    pub fn deadline(&self) -> tokio::time::Instant {
        self.scribe.deadline
    }

    /// The accepted revision and state of `execution`, without writing it,
    /// so inspecting a running execution never shuts out its writer; fresh
    /// at revision 0 when no revision was ever accepted.
    ///
    /// # Errors
    /// As [`Self::recover`].
    pub async fn accepted(
        app: &AppState,
        tenant: DataTenantId,
        execution: ExecutionId,
        records: &InputResolver<'_>,
    ) -> Result<(i64, ExecutionState), StateError> {
        let revision = Self::accepted_revision(app, tenant, execution).await?;
        if revision == 0 {
            return Ok((0, ExecutionState::fresh()));
        }
        let value = records
            .state(execution, revision)
            .await
            .map_err(|error| StateError::Unavailable(error.to_string()))?
            .ok_or_else(|| {
                StateError::Unavailable(format!("revision {revision} is not visible yet"))
            })?;
        Ok((revision, ExecutionState::decode(value)?))
    }

    /// The last revision of `execution` whose batch fence committed; 0 when
    /// none did.
    ///
    /// A revision is frozen only after the one before it was accepted, so the
    /// accepted revisions are exactly the fenced prefix from revision 1.
    ///
    /// # Errors
    /// Returns [`StateError::Unavailable`] when the fences cannot be read.
    async fn accepted_revision(
        app: &AppState,
        tenant: DataTenantId,
        execution: ExecutionId,
    ) -> Result<i64, StateError> {
        let unavailable =
            |error: &dyn std::fmt::Display| StateError::Unavailable(error.to_string());
        let mut conn = app
            .postgres
            .vala()
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let mut revision = 0;
        loop {
            let probed: Vec<_> = (revision + 1..=revision + PROBE)
                .map(|next| revision_fence(tenant, execution, next))
                .collect();
            let fenced = committed_batches(&mut conn, STATE_TABLE, &probed)
                .await
                .map_err(|error| unavailable(&error))?;
            let accepted = probed
                .iter()
                .take_while(|batch| fenced.contains(batch))
                .count();
            revision += i64::try_from(accepted).unwrap_or(PROBE);
            if accepted < probed.len() {
                return Ok(revision);
            }
        }
    }

    /// A copy of the live state, which holds changes still awaiting
    /// acceptance once steps run.
    pub async fn snapshot(&self) -> ExecutionState {
        self.held.lock().await.state.clone()
    }

    /// The terminal decision the state holds, selecting `outcome` with a
    /// new result identity first when it holds none, returned once an
    /// accepted revision holds it. A recorded failure is never superseded:
    /// once either is decided, every later entry delivers the same decision.
    ///
    /// # Errors
    /// Returns the [`Self::durable`] errors.
    pub async fn select(
        &self,
        outcome: &VerificationOutcome,
        started_at: DateTime<Utc>,
    ) -> Result<Decision, StateError> {
        self.decide(|| {
            Decision::Selected(TerminalSelection {
                result_id: VerificationResultId::new_v7(),
                started_at,
                ended_at: Utc::now(),
                outcome: outcome.clone(),
            })
        })
        .await
    }

    /// The terminal decision the state holds, recording that the execution
    /// ended without an outcome, as `status` with `error`, first when it
    /// holds none, returned once an accepted revision holds it. A selected
    /// outcome or an earlier failure is kept.
    ///
    /// # Errors
    /// Returns the [`Self::durable`] errors.
    pub async fn fail(
        &self,
        status: VerificationExecutionStatus,
        error: &VerificationError,
    ) -> Result<Decision, StateError> {
        self.decide(|| {
            Decision::Failed(TerminalFailure {
                status,
                error: error.clone(),
                ended_at: Utc::now(),
            })
        })
        .await
    }

    /// The decision the state holds, else `decision()` recorded as it,
    /// returned once an accepted revision holds it. The check and the
    /// change share one memory lock, so two deciders never both decide. A
    /// revision holding the decision is awaited for at least
    /// [`DECISION_TIMEOUT`], even past the writer's deadline.
    ///
    /// # Errors
    /// Returns the [`Self::durable`] errors.
    async fn decide(&self, decision: impl FnOnce() -> Decision) -> Result<Decision, StateError> {
        let (decided, ticket) = {
            let mut held = self.held.lock().await;
            if let Some(decided) = Decision::of(&held.state) {
                (decided, held.changes)
            } else {
                let decided = decision();
                match &decided {
                    Decision::Selected(selected) => {
                        held.state.terminal = Some(selected.clone());
                    }
                    Decision::Failed(failure) => held.state.failure = Some(failure.clone()),
                }
                (decided, held.changed())
            }
        };
        let deadline = self
            .scribe
            .deadline
            .max(tokio::time::Instant::now() + DECISION_TIMEOUT);
        self.durable(ticket, deadline).await?;
        Ok(decided)
    }

    /// The receipt `call` already holds, when it holds one, once an
    /// accepted revision holds it, without changing the state.
    pub async fn publication_receipt(
        &self,
        call: PublicationCall<'_>,
    ) -> Option<PublicationAcceptance> {
        let (found, ticket) = {
            let held = self.held.lock().await;
            if held.broken {
                return Some(PublicationAcceptance::StaleLease);
            }
            (receipt(&held.state, call)?, held.changes)
        };
        Some(self.receipt_once_accepted(found, ticket).await)
    }

    /// Accept `call`'s publication, once, within `budget`: the same call
    /// with the same name and arguments replays, another holder of the name
    /// in the attempt, or the same call under another name or arguments,
    /// conflicts, and a publication over a cumulative ceiling is refused. Every answer but a refusal over a ceiling is returned only
    /// once an accepted revision holds the publication it names.
    pub async fn accept_publication(
        &self,
        call: PublicationCall<'_>,
        budget: PublicationBudget,
    ) -> PublicationAcceptance {
        let (found, ticket) = {
            let mut held = self.held.lock().await;
            if held.broken {
                return PublicationAcceptance::StaleLease;
            }
            if let Some(found) = receipt(&held.state, call) {
                (found, held.changes)
            } else {
                let publications = &held.state.publications;
                if publications.len() >= budget.max_count as usize {
                    return PublicationAcceptance::OverBudget(PublicationLimit::Count);
                }
                let retained = publications
                    .iter()
                    .map(|accepted| accepted.retained_bytes)
                    .sum::<u64>();
                if retained.saturating_add(call.retained_bytes) > budget.max_bytes {
                    return PublicationAcceptance::OverBudget(PublicationLimit::Bytes);
                }
                let state = &mut held.state;
                state.sequence += 1;
                state.publications.push(AcceptedPublication {
                    sequence: state.sequence,
                    publication: call.publication.clone(),
                    call_id: call.call_id.to_owned(),
                    arguments: *call.arguments,
                    objects: call.objects.to_vec(),
                    retained_bytes: call.retained_bytes,
                });
                (PublicationAcceptance::Accepted, held.changed())
            }
        };
        self.receipt_once_accepted(found, ticket).await
    }

    /// `found` once an accepted revision holds change `ticket` and the
    /// writer's lease, when it has one, still holds its run, or
    /// [`PublicationAcceptance::StaleLease`] otherwise.
    ///
    /// The lease is confirmed outside the memory lock even when an accepted
    /// revision already held the change, so a replay is never the first
    /// success of a cancelled, expired, or reclaimed run.
    async fn receipt_once_accepted(
        &self,
        found: PublicationAcceptance,
        ticket: u64,
    ) -> PublicationAcceptance {
        if let Err(error) = self.durable(ticket, self.scribe.deadline).await {
            tracing::warn!(execution_id = %self.scribe.row.execution_id, %error, "a publication was not accepted");
            return PublicationAcceptance::StaleLease;
        }
        if !self.scribe.confirmed().await {
            self.held.lock().await.broken = true;
            return PublicationAcceptance::StaleLease;
        }
        found
    }

    /// Every publication `step` recorded in the live state, grouped by item,
    /// iteration, and attempt, in acceptance order within an attempt. Each
    /// was accepted before its step settled, so a dependent of the settled
    /// step reads only accepted publications.
    pub async fn publications(&self, step: &TaskId) -> Vec<AcceptedPublication> {
        let mut found: Vec<_> = self
            .held
            .lock()
            .await
            .state
            .publications
            .iter()
            .filter(|accepted| accepted.publication.producer.step_id == *step)
            .cloned()
            .collect();
        found.sort_by_key(|accepted| {
            let producer = &accepted.publication.producer;
            (
                producer.item_index,
                producer.iteration_index,
                producer.attempt,
                accepted.sequence,
            )
        });
        found
    }

    /// Apply `change` to the live state as one change and wait until an
    /// accepted revision holds it. `change` returns `false` when it changed
    /// nothing, which waits for nothing.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] when the state is broken, and the
    /// [`Self::durable`] errors.
    async fn change(
        &self,
        change: impl FnOnce(&mut ExecutionState) -> Result<bool, String>,
    ) -> Result<(), String> {
        let ticket = {
            let mut held = self.held.lock().await;
            if held.broken {
                return Err(StateError::Stale.to_string());
            }
            if !change(&mut held.state)? {
                return Ok(());
            }
            held.changed()
        };
        self.durable(ticket, self.scribe.deadline)
            .await
            .map_err(|error| error.to_string())
    }

    /// Wait until an accepted revision holds change `ticket`, awaiting a
    /// revision this call freezes until `deadline`.
    ///
    /// Joins the pending revision, or, when none is pending, confirms the
    /// lease outside the lock and freezes the live state, every change so
    /// far, into the next revision. A revision
    /// frozen before `ticket` is awaited and then the next one frozen, so
    /// changes made while a revision is pending share the revision after it.
    /// The memory lock is held only to read and update this bookkeeping and
    /// to copy the frozen state; staging and acceptance run outside it, in
    /// the shared acceptance any waiter drives, so a cancelled waiter
    /// neither retracts the revision nor strands the others.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] when the state is broken or the lease
    /// no longer holds, and the [`StateScribe::write`] errors of a revision
    /// holding `ticket`; each breaks the state.
    async fn durable(&self, ticket: u64, deadline: tokio::time::Instant) -> Result<(), StateError> {
        let mut confirmed = false;
        loop {
            let pending = {
                let mut held = self.held.lock().await;
                if held.broken {
                    return Err(StateError::Stale);
                }
                if held.covered >= ticket {
                    return Ok(());
                }
                if let Some(pending) = &held.pending {
                    confirmed = false;
                    pending.clone()
                } else if !confirmed {
                    drop(held);
                    if !self.scribe.confirmed().await {
                        self.held.lock().await.broken = true;
                        return Err(StateError::Stale);
                    }
                    confirmed = true;
                    continue;
                } else {
                    confirmed = false;
                    let revision = held.revision + 1;
                    let pending = Pending {
                        revision,
                        covers: held.changes,
                        accepted: Arc::clone(&self.scribe)
                            .write(revision, held.state.clone(), deadline)
                            .boxed()
                            .shared(),
                    };
                    held.pending = Some(pending.clone());
                    pending
                }
            };
            let accepted = pending.accepted.await;
            let mut held = self.held.lock().await;
            if held
                .pending
                .as_ref()
                .is_some_and(|current| current.revision == pending.revision)
            {
                held.pending = None;
                if accepted.is_ok() {
                    held.revision = pending.revision;
                    held.covered = pending.covers;
                } else {
                    held.broken = true;
                }
            }
            accepted?;
        }
    }
}

/// The receipt `call` holds in `state`: replayed with the accepted kind for
/// the same call, name, and arguments, a conflict for any other holder of
/// the name in the producer attempt or for the same call under another name
/// or arguments, or `None` while both the name and the call are free.
fn receipt(state: &ExecutionState, call: PublicationCall<'_>) -> Option<PublicationAcceptance> {
    let wanted = call.publication;
    state
        .publications
        .iter()
        .find(|accepted| {
            accepted.publication.producer == wanted.producer
                && (accepted.publication.name == wanted.name || accepted.call_id == call.call_id)
        })
        .map(|accepted| {
            if accepted.call_id == call.call_id
                && accepted.publication.name == wanted.name
                && accepted.arguments == *call.arguments
            {
                PublicationAcceptance::Replayed(accepted.publication.content.kind())
            } else {
                PublicationAcceptance::Conflict
            }
        })
}

#[async_trait::async_trait]
impl vala_verify::blocks::Checkpoints for RunState {
    /// Record that the attempt `producer` started, returning once an
    /// accepted revision holds it.
    ///
    /// # Errors
    /// Returns a bounded reason when no accepted revision will hold it.
    async fn begin(&self, producer: &Producer) -> Result<(), String> {
        self.change(|state| {
            state.sequence += 1;
            state.invocations.push(InvocationRecord {
                sequence: state.sequence,
                producer: producer.clone(),
                settlement: None,
            });
            Ok(true)
        })
        .await
    }

    /// Remove the unsettled attempt `producer`, refused capacity or its
    /// definition before it retained anything, so a later attempt replays
    /// it, returning once an accepted revision no longer holds it.
    ///
    /// An attempt that already accepted a publication keeps its record: its
    /// retained work stays attributed to it, so the attempt is spent and
    /// restoration starts fresh work at the next attempt.
    ///
    /// # Errors
    /// Returns a bounded reason when no accepted revision will reflect it.
    async fn forget(&self, producer: &Producer) -> Result<(), String> {
        self.change(|state| {
            if state
                .publications
                .iter()
                .any(|accepted| &accepted.publication.producer == producer)
            {
                return Ok(false);
            }
            let recorded = state.invocations.len();
            state
                .invocations
                .retain(|record| &record.producer != producer || record.settlement.is_some());
            Ok(state.invocations.len() != recorded)
        })
        .await
    }

    /// Commit `settled` as the settlement of the attempt `producer`, once,
    /// returning once an accepted revision holds it.
    ///
    /// # Errors
    /// Returns a bounded reason when the attempt is not recorded unsettled or
    /// no accepted revision will hold the settlement.
    async fn settle(
        &self,
        producer: &Producer,
        settled: &InvocationSettlement,
    ) -> Result<(), String> {
        self.change(|state| {
            let record = state
                .invocations
                .iter_mut()
                .find(|record| &record.producer == producer && record.settlement.is_none())
                .ok_or_else(|| "the invocation attempt is not recorded unsettled".to_owned())?;
            record.settlement = Some(settled.clone());
            Ok(true)
        })
        .await
    }

    /// Record the scheduling decision `control` once, returning once an
    /// accepted revision holds it; recording the same decision again changes
    /// nothing.
    ///
    /// # Errors
    /// Returns a bounded reason when another decision about the same step is
    /// already recorded, since a recorded decision is final, or no accepted
    /// revision will hold it.
    async fn control(&self, control: &ControlRecord) -> Result<(), String> {
        self.change(|state| {
            match state
                .controls
                .iter()
                .find(|recorded| recorded.decides_same(control))
            {
                Some(recorded) if recorded == control => Ok(false),
                Some(_) => {
                    Err("a different decision about the step is already recorded".to_owned())
                }
                None => {
                    state.controls.push(control.clone());
                    Ok(true)
                }
            }
        })
        .await
    }
}

/// A [`WyrdError`] for state an inspection read cannot use now.
impl From<StateError> for WyrdError {
    fn from(error: StateError) -> Self {
        tracing::warn!(%error, "the execution state cannot be read");
        Self::from(wyrd_spec::vala::BifrostError::QueryVisibilityUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::Value;
    use vala_verify::blocks::Checkpoints as _;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::Sha256Hex;
    use wyrd_spec::auth::{Attribution, PrincipalId};
    use wyrd_spec::ids::{CardUid, ExecutionId};
    use wyrd_spec::verification::PublicationName;
    use wyrd_spec::verification::execution::Producer;
    use wyrd_spec::verification::execution::{
        Publication, PublicationContent, PublicationKind, VerificationOutcome,
    };
    use wyrd_spec::verification::state::ExecutionState;
    use wyrd_spec::verification::{TaskId, VerificationVerdict};

    use super::{
        Decision, PublicationAcceptance, PublicationBudget, PublicationCall, RunState, StateRow,
    };
    use crate::components::gateway::recording::TableGate;
    use crate::scribe_outbox::{ScribeSink, ScribeTable};

    /// A fresh Card UID.
    ///
    /// # Panics
    /// Panics when a `UUIDv7` is not a Card UID.
    fn uid() -> CardUid {
        CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid")
    }

    /// A fresh direct execution's state, written through a Scribe that holds
    /// every revision until released.
    ///
    /// # Panics
    /// Panics when a fresh UUID is not a Card UID, or when called outside a
    /// Tokio runtime, where the outbox cannot spawn its writer.
    fn held_state() -> (RunState, Arc<TableGate>) {
        held_at(ExecutionState::fresh(), 0)
    }

    /// A direct execution's writer of `state`, accepted at `revision`,
    /// written through a Scribe that holds every revision until released.
    ///
    /// # Panics
    /// Panics as [`held_state`] does.
    fn held_at(state: ExecutionState, revision: i64) -> (RunState, Arc<TableGate>) {
        let scribe = Arc::new(TableGate::new(ScribeTable::ExecutionEvents));
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _, None);
        let (verifier, subject) = (uid(), uid());
        let row = StateRow {
            execution_id: ExecutionId::new_v7(),
            identity: super::stand_in_identity(&verifier, &subject),
            verifier_uid: verifier,
            card_uid: subject,
            owner_card_uid: None,
            run_id: None,
            deployment_environment: None,
            attribution: Attribution::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalId::new(uuid::Uuid::now_v7()),
            ),
        };
        let state = RunState::at(
            outbox,
            DataTenantId::new_v7(),
            row,
            tokio::time::Instant::now() + Duration::from_secs(30),
            None,
            state,
            revision,
        );
        (state, scribe)
    }

    /// A step's task ID.
    ///
    /// # Panics
    /// Panics when `name` is not a task ID.
    fn task(name: &str) -> TaskId {
        TaskId::new(name).expect("task id")
    }

    /// Waits until the held Scribe receives a revision.
    ///
    /// # Panics
    /// Panics when none arrives within ten seconds.
    async fn arrived(scribe: &TableGate) {
        tokio::time::timeout(Duration::from_secs(10), scribe.arrived.notified())
            .await
            .expect("a revision reaches Scribe");
    }

    /// Whether polling `future` once leaves it pending.
    async fn pending<F: Future + Unpin>(future: &mut F) -> bool {
        tokio::time::timeout(Duration::ZERO, future).await.is_err()
    }

    /// The text of every revision Scribe accepted, in order.
    fn revisions(scribe: &TableGate) -> Vec<String> {
        scribe
            .inner
            .received()
            .iter()
            .filter(|frame| frame.table == ScribeTable::ExecutionEvents.fqn())
            .map(|frame| String::from_utf8_lossy(&frame.ipc).into_owned())
            .collect()
    }

    /// Proves a sibling step records its progress while another step's
    /// revision awaits Scribe, and that each step returns only once a
    /// revision holding its own change is accepted.
    ///
    /// # Panics
    /// Panics when the sibling cannot record, a step returns before its
    /// change is accepted, or a revision holds the wrong changes.
    #[tokio::test]
    async fn siblings_record_progress_while_a_revision_is_held() {
        let (factual_id, sensitive_id) = (task("factual-check"), task("sensitive-check"));
        let (state, scribe) = held_state();
        let factual_id_attempt = Producer::plain(&factual_id, 1);
        let mut factual = Box::pin(state.begin(&factual_id_attempt));
        assert!(pending(&mut factual).await);
        arrived(&scribe).await;

        let sensitive_id_attempt = Producer::plain(&sensitive_id, 1);
        let mut sensitive = Box::pin(state.begin(&sensitive_id_attempt));
        assert!(
            pending(&mut sensitive).await,
            "B waits for its own revision"
        );
        let live = tokio::time::timeout(Duration::from_secs(1), state.snapshot())
            .await
            .expect("memory is not locked across a held revision");
        assert_eq!(live.invocations.len(), 2, "B recorded its start");

        scribe.release.add_permits(1);
        factual.await.expect("A's revision is accepted");
        assert!(
            pending(&mut sensitive).await,
            "A's revision does not hold B"
        );
        arrived(&scribe).await;
        scribe.release.add_permits(1);
        sensitive.await.expect("B's revision is accepted");

        let accepted = revisions(&scribe);
        assert_eq!(accepted.len(), 2);
        assert!(accepted[0].contains("factual-check") && !accepted[0].contains("sensitive-check"));
        assert!(accepted[1].contains("factual-check") && accepted[1].contains("sensitive-check"));
    }

    /// Proves changes recorded while a revision is held share the next
    /// revision, a cancelled waiter neither retracts its change nor strands
    /// another waiter, and a revision whose every waiter left is still
    /// driven by the next one.
    ///
    /// # Panics
    /// Panics when the changes take more than two revisions, a cancelled
    /// change is lost, or a remaining waiter is not released.
    #[tokio::test]
    async fn held_changes_share_the_next_revision_across_cancelled_waiters() {
        let (factual_id, sensitive_id, tone_id) = (
            task("factual-check"),
            task("sensitive-check"),
            task("tone-check"),
        );
        let (state, scribe) = held_state();
        let factual_id_attempt = Producer::plain(&factual_id, 1);
        let mut first = Box::pin(state.begin(&factual_id_attempt));
        assert!(pending(&mut first).await);
        arrived(&scribe).await;
        drop(first);
        let sensitive_id_attempt = Producer::plain(&sensitive_id, 1);
        let mut second = Box::pin(state.begin(&sensitive_id_attempt));
        assert!(pending(&mut second).await);
        drop(second);
        let tone_id_attempt = Producer::plain(&tone_id, 1);
        let mut third = Box::pin(state.begin(&tone_id_attempt));
        assert!(pending(&mut third).await);

        scribe.release.add_permits(2);
        third
            .await
            .expect("the remaining waiter drives both revisions");
        let accepted = revisions(&scribe);
        assert_eq!(accepted.len(), 2, "three changes take two revisions");
        assert!(!accepted[0].contains("sensitive-check"));
        for name in ["factual-check", "sensitive-check", "tone-check"] {
            assert!(accepted[1].contains(name), "{name} is in the batch");
        }
    }

    /// Proves concurrent terminal selections share one result identity and
    /// neither returns before its revision is accepted.
    ///
    /// # Panics
    /// Panics when a selection returns early or the identities differ.
    #[tokio::test]
    async fn duplicate_selections_share_one_accepted_result() {
        let (state, scribe) = held_state();
        let outcome = VerificationOutcome {
            verdict: VerificationVerdict::Passed,
            requirements: Vec::new(),
            steps: Vec::new(),
            evidence: BTreeMap::new(),
        };
        let started = chrono::Utc::now();
        let mut first = Box::pin(state.select(&outcome, started));
        assert!(pending(&mut first).await);
        arrived(&scribe).await;
        let mut second = Box::pin(state.select(&outcome, started));
        assert!(
            pending(&mut second).await,
            "a pending selection is not a result"
        );

        scribe.release.add_permits(1);
        let (first, second) = (first.await, second.await);
        assert_eq!(
            first.expect("selected"),
            second.expect("selected"),
            "both share one selected result"
        );
        assert_eq!(revisions(&scribe).len(), 1);
    }

    /// Proves a recorded failure is the execution's one decision: a later
    /// selection and a later failure both answer it and record nothing.
    ///
    /// # Panics
    /// Panics when a later decision supersedes the failure or is recorded.
    #[tokio::test]
    async fn a_recorded_failure_is_never_superseded() {
        let (state, scribe) = held_state();
        scribe.release.add_permits(1);
        let error = |code: &str| wyrd_spec::verification::VerificationError {
            code: code.to_owned(),
            message: code.to_owned(),
        };
        let failed = state
            .fail(
                wyrd_spec::verification::VerificationExecutionStatus::Errored,
                &error("engine"),
            )
            .await
            .expect("the failure is accepted");
        assert!(matches!(failed, Decision::Failed(_)));
        let outcome = VerificationOutcome {
            verdict: VerificationVerdict::Passed,
            requirements: Vec::new(),
            steps: Vec::new(),
            evidence: BTreeMap::new(),
        };
        let selected = state
            .select(&outcome, chrono::Utc::now())
            .await
            .expect("the decision reads");
        let refailed = state
            .fail(
                wyrd_spec::verification::VerificationExecutionStatus::TimedOut,
                &error("later"),
            )
            .await
            .expect("the decision reads");
        assert_eq!((&selected, &refailed), (&failed, &failed));
        assert!(state.snapshot().await.terminal.is_none());
        assert_eq!(revisions(&scribe).len(), 1);
    }

    /// Proves a refused revision releases no waiter and stops the writer.
    ///
    /// # Panics
    /// Panics when a waiter is released or a later change is accepted.
    #[tokio::test]
    async fn a_refused_revision_releases_nothing() {
        let (factual_id, sensitive_id) = (task("factual-check"), task("sensitive-check"));
        let (state, scribe) = held_state();
        scribe
            .inner
            .refuse_next(vala_bifrost_redux::contracts::ScribeError::BatchConflict);
        scribe
            .release
            .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
        assert!(state.begin(&Producer::plain(&factual_id, 1)).await.is_err());
        assert!(
            state
                .begin(&Producer::plain(&sensitive_id, 1))
                .await
                .is_err()
        );
        assert!(revisions(&scribe).is_empty());
    }

    /// Accept JSON `value` under `name` for provider call `call` of `task`'s
    /// `attempt` into `state`.
    ///
    /// # Panics
    /// Panics when `name` is invalid or `value` does not serialize.
    async fn publish(
        state: &RunState,
        (task_id, attempt): (&str, u32),
        call: &str,
        name: &str,
        value: Value,
    ) -> PublicationAcceptance {
        let arguments = serde_json::json!({ "name": name, "value": value });
        let publication = Publication {
            name: PublicationName::new(name).expect("name"),
            producer: Producer {
                step_id: task(task_id),
                item_index: None,
                iteration_index: None,
                attempt,
            },
            content: PublicationContent::Json { value },
        };
        state
            .accept_publication(
                PublicationCall {
                    publication: &publication,
                    call_id: call,
                    arguments: &Sha256Hex::digest(
                        &serde_jcs::to_vec(&arguments).expect("arguments serialize"),
                    ),
                    objects: &[],
                    retained_bytes: 1,
                },
                PublicationBudget {
                    max_count: 16,
                    max_bytes: 1024,
                },
            )
            .await
    }

    /// Proves one provider call keeps the one name and arguments it was
    /// accepted with, live and in a state restored from its accepted
    /// revision: its identical replay answers the original receipt, the same
    /// call under another name or with another value conflicts, another call
    /// cannot take the name, a fresh attempt may, and nothing else is kept.
    ///
    /// # Panics
    /// Panics when a call acquires a second name or value, or a fresh
    /// attempt cannot reuse the name.
    #[tokio::test]
    async fn a_call_keeps_its_original_publication() {
        let (live, scribe) = held_state();
        scribe
            .release
            .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
        let draft = ("draft", 1);
        assert_eq!(
            publish(&live, draft, "p1", "a", serde_json::json!(1)).await,
            PublicationAcceptance::Accepted
        );
        assert_eq!(
            publish(&live, draft, "p1", "b", serde_json::json!(1)).await,
            PublicationAcceptance::Conflict,
            "a live call cannot take a second name"
        );
        let (restored, scribe) = held_at(live.snapshot().await, 1);
        scribe
            .release
            .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
        for (call, name, value, expected) in [
            (
                "p1",
                "a",
                1,
                PublicationAcceptance::Replayed(PublicationKind::Json),
            ),
            ("p1", "b", 1, PublicationAcceptance::Conflict),
            ("p1", "a", 2, PublicationAcceptance::Conflict),
            ("p2", "a", 1, PublicationAcceptance::Conflict),
        ] {
            assert_eq!(
                publish(&restored, draft, call, name, serde_json::json!(value)).await,
                expected,
                "{call} {name} {value}"
            );
        }
        assert_eq!(
            publish(&restored, ("draft", 2), "p1", "a", serde_json::json!(3)).await,
            PublicationAcceptance::Accepted,
            "a fresh attempt may reuse the name and call ID"
        );
        let kept: Vec<(u32, String, Value)> = restored
            .snapshot()
            .await
            .publications
            .into_iter()
            .map(|accepted| {
                let PublicationContent::Json { value } = accepted.publication.content else {
                    panic!("only JSON was published");
                };
                (
                    accepted.publication.producer.attempt,
                    accepted.publication.name.as_str().to_owned(),
                    value,
                )
            })
            .collect();
        assert_eq!(
            kept,
            [
                (1, "a".to_owned(), serde_json::json!(1)),
                (2, "a".to_owned(), serde_json::json!(3)),
            ]
        );
    }
}
