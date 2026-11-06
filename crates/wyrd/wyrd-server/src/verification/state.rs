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
    AcceptedPublication, ExecutionIdentity, ExecutionState, ExecutionStateError, InvocationRecord,
    TerminalFailure, TerminalSelection,
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
    /// in this attempt.
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
    /// accept it. The outbox writer prepares the revision's row, so this
    /// caller does no encoding. It holds no connection while it waits, so it
    /// may wait unpolled until a waiter drives it.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] when the revision is refused, such as
    /// when another writer's rows already hold it or it cannot be encoded,
    /// or unacknowledged by the writer's deadline, and
    /// [`StateError::Unavailable`] when no outbox capacity frees by the
    /// deadline.
    async fn write(
        self: Arc<Self>,
        revision: i64,
        state: ExecutionState,
    ) -> Result<(), StateError> {
        let slot = self
            .outbox
            .reserve_until(1, self.deadline.into_std())
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
        match tokio::time::timeout_at(self.deadline, acked).await {
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
    /// The prepared row, or why it cannot be prepared.
    prepared: OnceLock<Result<StagedRow, String>>,
}

impl Checkpoint {
    /// Bytes an entry of the state is estimated to hold beyond its JSON
    /// values.
    const ENTRY_BYTES: usize = 512;

    /// `state` frozen as `revision` of the execution `row` identifies, not
    /// prepared yet.
    #[must_use]
    pub fn new(revision: i64, state: ExecutionState, row: Arc<StateRow>) -> Self {
        Self {
            revision,
            state,
            row,
            prepared: OnceLock::new(),
        }
    }

    /// A revision whose row is already prepared, such as one written in
    /// another state format.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn prepared(revision: i64, row: Arc<StateRow>, prepared: StagedRow) -> Self {
        Self {
            revision,
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

    /// Memory the outbox charges while the revision waits: the frozen state
    /// it holds, and so the row it prepares, which grow with its entries and
    /// its JSON publications' bytes. Stable for the revision's life, so
    /// admission and release charge the same.
    // ponytail: estimated from entry counts and JSON bytes without encoding; measure the prepared row if admission undercounts.
    #[must_use]
    pub fn weight(&self) -> usize {
        let state = &self.state;
        let objects: usize = state.publications.iter().map(|p| p.objects.len()).sum();
        let json: u64 = state
            .publications
            .iter()
            .filter(|p| p.objects.is_empty())
            .map(|p| p.retained_bytes)
            .sum();
        let entries = 1 + state.invocations.len() + state.publications.len() + objects;
        // A JSON value is held as its tree and its encoded Variant.
        Self::ENTRY_BYTES.saturating_mul(entries).saturating_add(
            usize::try_from(json)
                .unwrap_or(usize::MAX)
                .saturating_mul(2),
        )
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

    /// The terminal outcome the state selected, selecting `outcome` with a
    /// new result identity first when it has none, returned once an
    /// accepted revision holds it; once selected, every later entry delivers
    /// the same result.
    ///
    /// # Errors
    /// Returns the [`Self::durable`] errors.
    pub async fn select(
        &self,
        outcome: &VerificationOutcome,
        started_at: DateTime<Utc>,
    ) -> Result<TerminalSelection, StateError> {
        let (selected, ticket) = {
            let mut held = self.held.lock().await;
            if let Some(selected) = &held.state.terminal {
                (selected.clone(), held.changes)
            } else {
                let selected = TerminalSelection {
                    result_id: VerificationResultId::new_v7(),
                    started_at,
                    ended_at: Utc::now(),
                    outcome: outcome.clone(),
                };
                held.state.terminal = Some(selected.clone());
                (selected, held.changed())
            }
        };
        self.durable(ticket).await?;
        Ok(selected)
    }

    /// Record that the execution ended without an outcome, as `status` with
    /// `error`, returning once an accepted revision holds it. A state that
    /// already selected an outcome or recorded a failure keeps it.
    ///
    /// # Errors
    /// Returns the [`Self::durable`] errors.
    pub async fn fail(
        &self,
        status: VerificationExecutionStatus,
        error: &VerificationError,
    ) -> Result<(), StateError> {
        let ticket = {
            let mut held = self.held.lock().await;
            if held.state.terminal.is_some() || held.state.failure.is_some() {
                held.changes
            } else {
                held.state.failure = Some(TerminalFailure {
                    status,
                    error: error.clone(),
                    ended_at: Utc::now(),
                });
                held.changed()
            }
        };
        self.durable(ticket).await
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
    /// with the same arguments replays, another holder of the name in the
    /// attempt conflicts, and a publication over a cumulative ceiling is
    /// refused. Every answer but a refusal over a ceiling is returned only
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
        if let Err(error) = self.durable(ticket).await {
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
        self.durable(ticket)
            .await
            .map_err(|error| error.to_string())
    }

    /// Wait until an accepted revision holds change `ticket`.
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
    async fn durable(&self, ticket: u64) -> Result<(), StateError> {
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
                            .write(revision, held.state.clone())
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
/// the same call and arguments, a conflict for any other holder of the name
/// in the producer attempt, or `None` while the name is free.
fn receipt(state: &ExecutionState, call: PublicationCall<'_>) -> Option<PublicationAcceptance> {
    let wanted = call.publication;
    state
        .publications
        .iter()
        .find(|accepted| {
            accepted.publication.producer == wanted.producer
                && accepted.publication.name == wanted.name
        })
        .map(|accepted| {
            if accepted.call_id == call.call_id && accepted.arguments == *call.arguments {
                PublicationAcceptance::Replayed(accepted.publication.content.kind())
            } else {
                PublicationAcceptance::Conflict
            }
        })
}

/// The producer of `task`'s `attempt`, which runs without fanout or repeat.
fn producer(task: &TaskId, attempt: u32) -> Producer {
    Producer {
        step_id: task.clone(),
        item_index: None,
        iteration_index: None,
        attempt,
    }
}

#[async_trait::async_trait]
impl vala_verify::blocks::Checkpoints for RunState {
    /// Record that `attempt` of `task` started, returning once an accepted
    /// revision holds it.
    ///
    /// # Errors
    /// Returns a bounded reason when no accepted revision will hold it.
    async fn begin(&self, task: &TaskId, attempt: u32) -> Result<(), String> {
        self.change(|state| {
            state.sequence += 1;
            state.invocations.push(InvocationRecord {
                sequence: state.sequence,
                producer: producer(task, attempt),
                settlement: None,
            });
            Ok(true)
        })
        .await
    }

    /// Remove the unsettled `attempt` of `task`, refused capacity before it
    /// ran, so a later attempt replays it, returning once an accepted
    /// revision no longer holds it.
    ///
    /// # Errors
    /// Returns a bounded reason when no accepted revision will reflect it.
    async fn forget(&self, task: &TaskId, attempt: u32) -> Result<(), String> {
        let wanted = producer(task, attempt);
        self.change(|state| {
            let recorded = state.invocations.len();
            state
                .invocations
                .retain(|record| record.producer != wanted || record.settlement.is_some());
            Ok(state.invocations.len() != recorded)
        })
        .await
    }

    /// Commit `settled` as the settlement of `attempt` of `task`, once,
    /// returning once an accepted revision holds it.
    ///
    /// # Errors
    /// Returns a bounded reason when the attempt is not recorded unsettled or
    /// no accepted revision will hold the settlement.
    async fn settle(
        &self,
        task: &TaskId,
        attempt: u32,
        settled: &InvocationSettlement,
    ) -> Result<(), String> {
        let wanted = producer(task, attempt);
        self.change(|state| {
            let record = state
                .invocations
                .iter_mut()
                .find(|record| record.producer == wanted && record.settlement.is_none())
                .ok_or_else(|| "the invocation attempt is not recorded unsettled".to_owned())?;
            record.settlement = Some(settled.clone());
            Ok(true)
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

    use vala_verify::blocks::Checkpoints as _;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{Attribution, PrincipalId};
    use wyrd_spec::ids::{CardUid, ExecutionId};
    use wyrd_spec::verification::execution::VerificationOutcome;
    use wyrd_spec::verification::{TaskId, VerificationVerdict};

    use super::{RunState, StateRow};
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
        let state = RunState::fresh(
            outbox,
            DataTenantId::new_v7(),
            row,
            tokio::time::Instant::now() + Duration::from_secs(30),
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
        let mut factual = Box::pin(state.begin(&factual_id, 1));
        assert!(pending(&mut factual).await);
        arrived(&scribe).await;

        let mut sensitive = Box::pin(state.begin(&sensitive_id, 1));
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
        let mut first = Box::pin(state.begin(&factual_id, 1));
        assert!(pending(&mut first).await);
        arrived(&scribe).await;
        drop(first);
        let mut second = Box::pin(state.begin(&sensitive_id, 1));
        assert!(pending(&mut second).await);
        drop(second);
        let mut third = Box::pin(state.begin(&tone_id, 1));
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
            first.expect("selected").result_id,
            second.expect("selected").result_id
        );
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
        assert!(state.begin(&factual_id, 1).await.is_err());
        assert!(state.begin(&sensitive_id, 1).await.is_err());
        assert!(revisions(&scribe).is_empty());
    }
}
