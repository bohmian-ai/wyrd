//! The recoverable state of one running Verifier execution.
//!
//! [`RunState`] is the one owner of a queued execution's [`ExecutionState`]
//! while it runs; direct executions have no recoverable state. Steps validate and change
//! the live state in memory under a short lock and return at once: no step
//! waits for SQL, a lease confirmation, outbox capacity, or a Scribe
//! acknowledgement. A background flush persists the state: it freezes every
//! change so far into the next complete revision, one at a time, so changes
//! made while a revision is in flight coalesce into the one after it. Every
//! revision is one `bifrost.verification.execution_events` row written under
//! a batch id derived from its execution and revision, so the first batch
//! Scribe fences keeps the revision and another writer's different rows for
//! it are refused as a batch conflict. Recovery finds the accepted revision
//! from those generic batch fences, which commit before any acknowledgement
//! and are never removed, and reads exactly that revision's row: no fence is
//! authoritative absence, and a fenced revision not visible yet, such as one
//! a Scribe unavailable to the read still holds, is
//! [`StateError::Unavailable`], never older or empty state. Live progress not
//! yet accepted is lost with its process.
//!
//! A writer stages each revision on a reserved outbox slot, waiting for
//! capacity in background, and retries until it is accepted or its lease is
//! lost; a refused revision, an unknown acceptance, or a lost lease breaks
//! the writer, whose next step then fails. Its terminal custody awaits
//! [`RunState::persisted`] off its computation. A delayed old
//! worker can race a replacement for one revision; whichever batch is fenced
//! first keeps it, and the other writer breaks.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use chrono::{DateTime, Utc};
use sha2::{Digest as _, Sha256};
use tokio::sync::{Notify, oneshot};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::contracts::committed_batches;
use vala_bifrost_redux::tables::staged::StagedRow;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{
    ApprovalId, CardUid, EvidenceId, ExecutionId, SlotName, VerificationResultId,
};
use wyrd_spec::storage::StoredObjectRef;
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    ExecutedOutcome, InvocationSettlement, Producer, Publication, PublicationKind,
};
use wyrd_spec::verification::state::{
    AcceptedPublication, ApprovalOccurrence, ControlRecord, ExecutionIdentity, ExecutionState,
    ExecutionStateError, InvocationRecord, RecordedApproval, TerminalFailure, TerminalSelection,
};
use wyrd_spec::verification::{VerificationError, VerificationExecutionStatus};

use super::inputs::InputResolver;
use super::results::{ResultPayloadBuilder, ResultRun};
use crate::scribe_outbox::{ScribeOutbox, ScribeSink, ScribeWrite, revision_batch_id};
use crate::state::AppState;
use wyrd_runtime::outbox::Reservation;
use wyrd_runtime::resources::ByteCharge;

/// The table every revision is a row of, and the logical table of every
/// revision's batch fence.
pub(crate) const STATE_TABLE: &str = "bifrost.verification.execution_events";

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

/// The server-issued id of `execution`'s approval `occurrence`: derived from
/// the step, fanout item, and repeat iteration, so distinct occurrences never
/// share an id and the same occurrence keeps its id across attempts.
///
/// # Panics
/// Never: the digest is stamped as a version 7 UUID before it is wrapped.
#[must_use]
pub fn approval_id(execution: ExecutionId, occurrence: &ApprovalOccurrence) -> ApprovalId {
    let index = |part: Option<u32>| {
        part.map_or([0; 5], |at| {
            let [a, b, c, d] = at.to_be_bytes();
            [1, a, b, c, d]
        })
    };
    let step = occurrence.step.as_str().as_bytes();
    let digest = Sha256::new()
        .chain_update(b"wyrd.verification.approval.v1")
        .chain_update(execution.as_uuid().as_bytes())
        .chain_update((step.len() as u64).to_be_bytes())
        .chain_update(step)
        .chain_update(index(occurrence.item))
        .chain_update(index(occurrence.iteration))
        .finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    ApprovalId::new(uuid::Uuid::from_bytes(bytes))
        .expect("a version 7 shaped UUID is a valid approval id")
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

/// How long a queued writer waits for outbox capacity in one reservation
/// before it waits again; its lease's loss ends the wait at once.
const CAPACITY_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

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
    /// Whether the background flush is running; at most one per writer, so
    /// at most one frozen revision of the execution exists at a time.
    writing: bool,
    /// Set once a revision was refused, its acceptance is unknown, or the
    /// writer's lease was lost; this writer records nothing more.
    broken: bool,
    /// The process memory charge for `state`, or `None` for a writer whose
    /// outbox charges no ledger. Charged once for a recovered state and
    /// grown by each retained value as it is recorded, never by recounting
    /// the state; it lasts as long as `state`, through every revision's
    /// acknowledgement, and returns when the writer drops.
    live: Option<ByteCharge>,
}

impl Held {
    /// `state`, accepted at `revision` and charged `live`, with nothing
    /// pending.
    const fn new(state: ExecutionState, revision: i64, live: Option<ByteCharge>) -> Self {
        Self {
            state,
            changes: 0,
            revision,
            covered: 0,
            writing: false,
            broken: false,
            live,
        }
    }

    /// Grow the live charge by `bytes` a change just retained. The growth
    /// is mandatory, since the change is already made, so it is charged even
    /// past the limit and later fallible admissions refuse sooner instead.
    fn retain(&mut self, bytes: u64) {
        if let Some(live) = &mut self.live {
            live.grow_infallible(bytes);
        }
    }

    /// A charge for one copy of the live state, taken from the live
    /// charge's ledger before the copy is made; `None` when uncharged.
    fn charge_copy(&self) -> Option<ByteCharge> {
        self.live
            .as_ref()
            .map(|live| live.ledger().charge_infallible(live.bytes()))
    }
}

/// Bytes `value` retains in a live state: its compact JSON length plus
/// [`Checkpoint::ENTRY_BYTES`] for its in-memory structure.
fn retained(value: &impl serde::Serialize) -> u64 {
    super::encoded_len(value, false).saturating_add(Checkpoint::ENTRY_BYTES as u64)
}

/// A copy read out of a live state with the process memory charge taken
/// for it before it was made; the charge returns when the copy drops.
#[derive(Debug)]
pub struct Charged<T> {
    /// The copy.
    value: T,
    /// Its charge, or `None` when the state is uncharged.
    charge: Option<ByteCharge>,
}

impl<T> Charged<T> {
    /// `value` read from no charged state, holding no charge.
    pub const fn uncharged(value: T) -> Self {
        Self {
            value,
            charge: None,
        }
    }

    /// The copy and its charge, for a caller that moves the copy into an
    /// owner and keeps the charge beside it.
    pub fn into_parts(self) -> (T, Option<ByteCharge>) {
        (self.value, self.charge)
    }
}

impl<T> std::ops::Deref for Charged<T> {
    type Target = T;

    /// The copy.
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: PartialEq> PartialEq for Charged<T> {
    /// Copies compare by value; their charges do not.
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

/// One writer's live state and its background persistence: everything a
/// revision is written with, shared with the flush that writes it.
struct Writer {
    /// The outbox every revision is staged on.
    outbox: Arc<ScribeOutbox>,
    /// The execution's tenant.
    tenant: DataTenantId,
    /// The identities every revision's row records.
    row: Arc<StateRow>,
    /// The execution's lease-loss token, which ends its writes and fails its
    /// later changes.
    lost: CancellationToken,
    /// The live state and its acceptance, locked only to read or change
    /// memory, never across encoding or IO.
    held: Mutex<Held>,
    /// Notified whenever the flush accepts a revision, breaks, or stops.
    flushed: Notify,
}

impl Writer {
    /// The live state under its short lock.
    fn lock(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether this writer may change the state: nothing broke it and its
    /// lease has not been lost.
    fn current(&self, held: &Held) -> bool {
        !held.broken && !self.lost.is_cancelled()
    }

    /// Record one change applied to `held`'s state and start the background
    /// flush unless one is running.
    fn changed(self: &Arc<Self>, held: &mut Held) {
        held.changes += 1;
        if !held.writing {
            held.writing = true;
            tokio::spawn(Arc::clone(self).flush());
        }
    }

    /// Persist every change in background until an accepted revision holds
    /// the latest one or the writer breaks.
    ///
    /// Each turn first reserves the revision's outbox slot, then freezes the
    /// live state, every change so far, into the next revision under the
    /// short lock and stages it at once, so a frozen copy never waits for
    /// capacity uncharged. The copy is charged the live state's size before
    /// it is made, and that charge passes to the outbox as part of the
    /// revision's weight. Changes made while a revision is in flight
    /// coalesce into the one after it, and only one frozen copy exists at a
    /// time.
    async fn flush(self: Arc<Self>) {
        loop {
            {
                let mut held = self.lock();
                if held.broken || held.covered >= held.changes {
                    held.writing = false;
                    drop(held);
                    self.flushed.notify_waiters();
                    return;
                }
            }
            let written = match self.reserve().await {
                Some(slot) => {
                    let (revision, covers, frozen, state) = {
                        let held = self.lock();
                        let frozen = held.charge_copy();
                        (held.revision + 1, held.changes, frozen, held.state.clone())
                    };
                    let accepted = self.write(slot, revision, state, frozen).await;
                    Some((revision, covers, accepted))
                }
                None => None,
            };
            {
                let mut held = self.lock();
                match written {
                    Some((revision, covers, true)) => {
                        held.revision = revision;
                        held.covered = covers;
                    }
                    Some((revision, _, false)) => {
                        tracing::warn!(execution_id = %self.row.execution_id, revision, "an execution state revision was not accepted; its writer stops");
                        held.broken = true;
                    }
                    None => {
                        tracing::warn!(execution_id = %self.row.execution_id, "the execution's lease was lost before its next revision was staged; its writer stops");
                        held.broken = true;
                    }
                }
            }
            self.flushed.notify_waiters();
        }
    }

    /// Reserve one outbox slot for the next revision, waiting for capacity
    /// while the outbox is full, so a revision is never shed; `None` once
    /// the lease is lost. Nothing here holds the state's lock.
    async fn reserve(&self) -> Option<Reservation<ScribeSink>> {
        loop {
            tokio::select! {
                biased;
                () = self.lost.cancelled() => return None,
                reserved = self.outbox.reserve_until(1, std::time::Instant::now() + CAPACITY_WAIT) => {
                    if let Ok(slot) = reserved {
                        return Some(slot);
                    }
                }
            }
        }
    }

    /// Stage `state` as `revision` on its reserved `slot` under the
    /// revision's batch identity and wait for Scribe to accept it, returning
    /// whether it did.
    ///
    /// Staging charges the revision's weight to the outbox and the process
    /// memory ledger until it is written, taking over `frozen`, the charge
    /// the copy was given before it was made; its lease's loss ends the
    /// wait. The outbox writer prepares the row, so this does no encoding,
    /// and nothing here holds the state's lock.
    async fn write(
        &self,
        slot: Reservation<ScribeSink>,
        revision: i64,
        state: ExecutionState,
        frozen: Option<ByteCharge>,
    ) -> bool {
        let (sender, acked) = oneshot::channel();
        let write = ScribeWrite::ExecutionState {
            batch: revision_batch(self.row.execution_id, revision),
            checkpoint: Arc::new(Checkpoint::new(revision, state, Arc::clone(&self.row))),
            attribution: self.row.attribution,
            acked: Mutex::new(Some(sender)),
        };
        match frozen {
            Some(frozen) => slot.stage_charged(self.tenant, write, frozen),
            None => slot.stage(self.tenant, write),
        }
        tokio::select! {
            biased;
            () = self.lost.cancelled() => false,
            acked = acked => acked.is_ok(),
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
        // A state that does not encode is refused by its preparation; what
        // was counted before the error still bounds its memory.
        let counted = usize::try_from(super::encoded_len(state, false)).unwrap_or(usize::MAX);
        counted
            .saturating_mul(Self::COPIES)
            .saturating_add(Self::ENTRY_BYTES.saturating_mul(Self::entries(state)))
    }

    /// The entries `state` holds that [`Self::ENTRY_BYTES`] is charged for.
    fn entries(state: &ExecutionState) -> usize {
        let objects: usize = state.publications.iter().map(|p| p.objects.len()).sum();
        1 + state.invocations.len() + state.publications.len() + objects
    }

    /// The memory a live `state` is charged: its compact JSON length plus
    /// [`Self::ENTRY_BYTES`] per entry, the size [`retained`] grows it by.
    fn live(state: &ExecutionState) -> u64 {
        let entries = u64::try_from(Self::entries(state)).unwrap_or(u64::MAX);
        super::encoded_len(state, false)
            .saturating_add((Self::ENTRY_BYTES as u64).saturating_mul(entries))
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
///
/// Steps change the live state in memory and return at once; a background
/// flush persists it as revisions. A queued execution's terminal custody
/// awaits [`Self::persisted`] off its computation.
pub struct RunState {
    /// The live state and its background persistence.
    writer: Arc<Writer>,
    /// The instant this writer's attempt ends.
    deadline: tokio::time::Instant,
}

impl RunState {
    /// Recover `tenant`'s execution state for a new writer whose attempt
    /// ends at `deadline`, through `records`, whose read authority must cover
    /// `bifrost.verification.execution_events`. `lost` is its lease-loss
    /// token.
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
        lost: CancellationToken,
        records: &InputResolver<'_>,
    ) -> Result<Self, StateError> {
        let (revision, state) = Self::accepted(app, tenant, row.execution_id, records).await?;
        Ok(Self::at(
            outbox, tenant, row, deadline, lost, state, revision,
        ))
    }

    /// A writer of `state`, accepted at `revision`, which records `row`'s
    /// identity unless the state already holds one. `lost` is the
    /// execution's lease-loss token.
    ///
    /// The state, already in memory, is charged to the outbox's process
    /// memory ledger once, infallibly, for as long as the writer holds it.
    pub(crate) fn at(
        outbox: Arc<ScribeOutbox>,
        tenant: DataTenantId,
        row: StateRow,
        deadline: tokio::time::Instant,
        lost: CancellationToken,
        mut state: ExecutionState,
        revision: i64,
    ) -> Self {
        state.identity.get_or_insert_with(|| row.identity.clone());
        let live = outbox
            .memory()
            .map(|memory| memory.charge_infallible(Checkpoint::live(&state)));
        Self {
            writer: Arc::new(Writer {
                outbox,
                tenant,
                row: Arc::new(row),
                lost,
                held: Mutex::new(Held::new(state, revision, live)),
                flushed: Notify::new(),
            }),
            deadline,
        }
    }

    /// The instant this writer's attempt ends.
    #[must_use]
    pub const fn deadline(&self) -> tokio::time::Instant {
        self.deadline
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

    /// An uncharged copy of the live state, which may hold changes not yet
    /// accepted, for tests to inspect.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn snapshot(&self) -> ExecutionState {
        self.writer.lock().state.clone()
    }

    /// The terminal decision the live state holds, if any.
    #[must_use]
    pub fn decision(&self) -> Option<Decision> {
        Decision::of(&self.writer.lock().state)
    }

    /// The approval occurrences the live state records.
    #[must_use]
    pub fn approvals(&self) -> Vec<RecordedApproval> {
        self.writer.lock().state.approvals.clone()
    }

    /// `engine` restoring this execution from the live state, recording
    /// into `store`, with the charge for the restoration copy it takes; the
    /// caller holds the charge while the engine runs.
    ///
    /// The copy is charged the live state's size before it is made.
    pub fn restore(
        &self,
        engine: vala_verify::blocks::VerificationEngine,
        store: Arc<dyn vala_verify::blocks::Checkpoints>,
    ) -> Charged<vala_verify::blocks::VerificationEngine> {
        let held = self.writer.lock();
        let charge = held.charge_copy();
        Charged {
            value: engine.with_checkpoints(store, &held.state),
            charge,
        }
    }

    /// Wait until an accepted revision holds every change made so far,
    /// without driving anything: the background flush writes them.
    ///
    /// A queued execution's terminal custody awaits this off its
    /// computation before its result is delivered; a cancelled waiter
    /// retracts nothing.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] once the writer broke or lost its
    /// lease.
    pub async fn persisted(&self) -> Result<(), StateError> {
        loop {
            let notified = self.writer.flushed.notified();
            let mut notified = std::pin::pin!(notified);
            notified.as_mut().enable();
            {
                let held = self.writer.lock();
                if !self.writer.current(&held) {
                    return Err(StateError::Stale);
                }
                if held.covered >= held.changes {
                    return Ok(());
                }
            }
            notified.await;
        }
    }

    /// The terminal decision the state holds, selecting `outcome` with a
    /// new result identity first when it holds none. A recorded failure is
    /// never superseded: once either is decided, every later entry returns
    /// the same decision. The decision is live; [`Self::persisted`] awaits
    /// its acceptance.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] when a new decision cannot be recorded
    /// because the writer broke or lost its lease.
    pub fn select(
        &self,
        outcome: &ExecutedOutcome,
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
    }

    /// The terminal decision the state holds, recording that the execution
    /// ended without an outcome, as `status` with `error`, first when it
    /// holds none. A selected outcome or an earlier failure is kept.
    ///
    /// # Errors
    /// As [`Self::select`].
    pub fn fail(
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
    }

    /// The decision the state holds, else `decision()` recorded as it. The
    /// check and the change share one memory lock, so two deciders never
    /// both decide.
    ///
    /// # Errors
    /// As [`Self::select`].
    fn decide(&self, decision: impl FnOnce() -> Decision) -> Result<Decision, StateError> {
        let mut held = self.writer.lock();
        if let Some(decided) = Decision::of(&held.state) {
            return Ok(decided);
        }
        if !self.writer.current(&held) {
            return Err(StateError::Stale);
        }
        let decided = decision();
        // ponytail: counted under the lock; a terminal record is one bounded
        // value, so the pass is short.
        match &decided {
            Decision::Selected(selected) => {
                held.retain(retained(selected));
                held.state.terminal = Some(selected.clone());
            }
            Decision::Failed(failure) => {
                held.retain(retained(failure));
                held.state.failure = Some(failure.clone());
            }
        }
        self.writer.changed(&mut held);
        Ok(decided)
    }

    /// Record that the execution parks waiting for the deferred inputs
    /// `waiting` and the undecided approval occurrences `approvals`, because
    /// no runnable step remains; [`Self::persisted`] awaits the parked
    /// revision before the queue delivery is retired.
    ///
    /// Each occurrence not yet recorded is issued its [`approval_id`], so
    /// the same occurrence keeps one id across attempts and a decision can
    /// only address an occurrence this execution recorded.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] once the writer broke or lost its
    /// lease.
    pub fn wait(
        &self,
        waiting: BTreeSet<SlotName>,
        approvals: Vec<ApprovalOccurrence>,
    ) -> Result<(), StateError> {
        let grows: Vec<u64> = approvals.iter().map(retained).collect();
        let waits = retained(&waiting);
        let mut held = self.writer.lock();
        if !self.writer.current(&held) {
            return Err(StateError::Stale);
        }
        let mut changed = held.state.waiting != waiting;
        if changed {
            held.retain(waits);
        }
        held.state.waiting = waiting;
        for (occurrence, grows) in approvals.into_iter().zip(grows) {
            if record_approval(&mut held.state, self.writer.row.execution_id, occurrence) {
                held.retain(grows);
                changed = true;
            }
        }
        if changed {
            self.writer.changed(&mut held);
        }
        Ok(())
    }

    /// Record that deferred input `slot` was supplied from published
    /// evidence `evidence`, which also ends any wait for it, so a
    /// resumption reads exactly that evidence.
    ///
    /// # Errors
    /// Returns [`StateError::Stale`] once the writer broke or lost its
    /// lease.
    pub fn accept_evidence(&self, slot: &SlotName, evidence: EvidenceId) -> Result<(), StateError> {
        let grows = retained(&(slot, evidence));
        let mut held = self.writer.lock();
        if !self.writer.current(&held) {
            return Err(StateError::Stale);
        }
        let waited = held.state.waiting.remove(slot);
        if held.state.evidence.insert(slot.clone(), evidence) != Some(evidence) || waited {
            held.retain(grows);
            self.writer.changed(&mut held);
        }
        Ok(())
    }

    /// The receipt `call` already holds in the live state, when it holds
    /// one, without changing the state; [`PublicationAcceptance::StaleLease`]
    /// once the writer broke or lost its lease, so a replay is never the
    /// first success of a cancelled, expired, or reclaimed run.
    #[must_use]
    pub fn publication_receipt(&self, call: PublicationCall<'_>) -> Option<PublicationAcceptance> {
        let held = self.writer.lock();
        if !self.writer.current(&held) {
            return Some(PublicationAcceptance::StaleLease);
        }
        receipt(&held.state, call)
    }

    /// Accept `call`'s publication into the live state, once, within
    /// `budget`: the same call with the same name and arguments replays,
    /// another holder of the name in the attempt, or the same call under
    /// another name or arguments, conflicts, and a publication over a
    /// cumulative ceiling is refused. A writer that broke or lost its lease
    /// answers [`PublicationAcceptance::StaleLease`] and changes nothing.
    #[must_use]
    pub fn accept_publication(
        &self,
        call: PublicationCall<'_>,
        budget: PublicationBudget,
    ) -> PublicationAcceptance {
        let grows = retained(&(call.publication, call.call_id, call.objects)).saturating_add(
            (Checkpoint::ENTRY_BYTES as u64)
                .saturating_mul(u64::try_from(call.objects.len()).unwrap_or(u64::MAX)),
        );
        let mut held = self.writer.lock();
        if !self.writer.current(&held) {
            return PublicationAcceptance::StaleLease;
        }
        if let Some(found) = receipt(&held.state, call) {
            return found;
        }
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
        held.retain(grows);
        self.writer.changed(&mut held);
        PublicationAcceptance::Accepted
    }

    /// Every publication `step` recorded in the live state, grouped by item,
    /// iteration, and attempt, in acceptance order within an attempt. Each
    /// was accepted before its step settled, so a dependent of the settled
    /// step reads its publications from memory.
    ///
    /// The copy is charged to the live state's ledger before it is made and
    /// holds that charge until it drops.
    #[must_use]
    pub fn publications(&self, step: &TaskId) -> Charged<Vec<AcceptedPublication>> {
        let held = self.writer.lock();
        let selected: Vec<&AcceptedPublication> = held
            .state
            .publications
            .iter()
            .filter(|accepted| accepted.publication.producer.step_id == *step)
            .collect();
        // ponytail: counted under the lock; the publication budget bounds
        // the pass.
        let charge = held.live.as_ref().map(|live| {
            let entries: u64 = selected
                .iter()
                .map(|accepted| 1 + accepted.objects.len() as u64)
                .sum();
            live.ledger().charge_infallible(
                super::encoded_len(&selected, false)
                    .saturating_add((Checkpoint::ENTRY_BYTES as u64).saturating_mul(entries)),
            )
        });
        let mut found: Vec<_> = selected.into_iter().cloned().collect();
        drop(held);
        found.sort_by_key(|accepted| {
            let producer = &accepted.publication.producer;
            (
                producer.item_index,
                producer.iteration_index,
                producer.attempt,
                accepted.sequence,
            )
        });
        Charged {
            value: found,
            charge,
        }
    }

    /// Apply `change` to the live state as one change; the background flush
    /// persists it. `change` returns `false` when it changed nothing, and
    /// otherwise retained `grows` more bytes, which the live charge takes.
    ///
    /// # Errors
    /// Returns a bounded reason when the writer broke or lost its lease, or
    /// `change`'s own refusal.
    fn change(
        &self,
        grows: u64,
        change: impl FnOnce(&mut ExecutionState) -> Result<bool, String>,
    ) -> Result<(), String> {
        let mut held = self.writer.lock();
        if !self.writer.current(&held) {
            return Err(StateError::Stale.to_string());
        }
        if change(&mut held.state)? {
            held.retain(grows);
            self.writer.changed(&mut held);
        }
        Ok(())
    }
}

/// Record `occurrence` in `state` under its [`approval_id`] for `execution`
/// unless it is already recorded, and report whether `state` changed.
fn record_approval(
    state: &mut ExecutionState,
    execution: ExecutionId,
    occurrence: ApprovalOccurrence,
) -> bool {
    if state
        .approvals
        .iter()
        .any(|recorded| recorded.occurrence == occurrence)
    {
        return false;
    }
    state.approvals.push(RecordedApproval {
        approval_id: approval_id(execution, &occurrence),
        occurrence,
    });
    true
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
    /// Record in the live state that the attempt `producer` started.
    ///
    /// # Errors
    /// Returns a bounded reason when the writer broke or lost its lease.
    async fn begin(&self, producer: &Producer) -> Result<(), String> {
        self.change(retained(producer), |state| {
            state.sequence += 1;
            state.invocations.push(InvocationRecord {
                sequence: state.sequence,
                producer: producer.clone(),
                settlement: None,
                tool_calls_spent: 0,
                released: false,
            });
            Ok(true)
        })
    }

    /// Record in the live state that the unsettled attempt `producer`,
    /// refused capacity or its definition after its host work settled, had
    /// spent `tool_calls_spent` tool requests, so a reclaimed invocation
    /// resumes from that count.
    ///
    /// An attempt that retained nothing is released: it spends no retry, so
    /// a later attempt replays it. One that already accepted a publication
    /// stays a spent attempt: its retained work stays attributed to it, so
    /// restoration starts fresh work at the next attempt.
    ///
    /// # Errors
    /// Returns a bounded reason when the writer broke or lost its lease.
    async fn forget(&self, producer: &Producer, tool_calls_spent: u32) -> Result<(), String> {
        self.change(0, |state| {
            let retained = state
                .publications
                .iter()
                .any(|accepted| &accepted.publication.producer == producer);
            let Some(record) = state.invocations.iter_mut().find(|record| {
                &record.producer == producer && record.settlement.is_none() && !record.released
            }) else {
                return Ok(false);
            };
            record.tool_calls_spent = record.tool_calls_spent.max(tool_calls_spent);
            record.released = !retained;
            Ok(true)
        })
    }

    /// Raise the live record of the unsettled attempt `producer` to
    /// `tool_calls_spent` requests; the background flush persists it.
    ///
    /// A broken writer records nothing; its next awaited change reports it.
    fn spent(&self, producer: &Producer, tool_calls_spent: u32) {
        let recorded = self.change(0, |state| {
            let Some(record) = state.invocations.iter_mut().find(|record| {
                &record.producer == producer && record.settlement.is_none() && !record.released
            }) else {
                return Ok(false);
            };
            let raised = record.tool_calls_spent < tool_calls_spent;
            record.tool_calls_spent = record.tool_calls_spent.max(tool_calls_spent);
            Ok(raised)
        });
        if let Err(reason) = recorded {
            tracing::debug!(%reason, "a tool spend was not recorded");
        }
    }

    /// Record `settled` in the live state as the settlement of the attempt
    /// `producer`, once.
    ///
    /// # Errors
    /// Returns a bounded reason when the attempt is not recorded unsettled or
    /// the writer broke or lost its lease.
    async fn settle(
        &self,
        producer: &Producer,
        settled: &InvocationSettlement,
    ) -> Result<(), String> {
        self.change(retained(settled), |state| {
            let record = state
                .invocations
                .iter_mut()
                .find(|record| {
                    &record.producer == producer && record.settlement.is_none() && !record.released
                })
                .ok_or_else(|| "the invocation attempt is not recorded unsettled".to_owned())?;
            record.settlement = Some(settled.clone());
            Ok(true)
        })
    }

    /// Record the scheduling decision `control` in the live state once;
    /// recording the same decision again changes nothing.
    ///
    /// # Errors
    /// Returns a bounded reason when another decision about the same step is
    /// already recorded, since a recorded decision is final, or the writer
    /// broke or lost its lease.
    async fn control(&self, control: &ControlRecord) -> Result<(), String> {
        self.change(retained(control), |state| {
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
    }

    /// Record the reached approval `occurrence` in the live state under its
    /// stable [`approval_id`], so it can be decided before the execution
    /// parks; recording it again changes nothing.
    ///
    /// # Errors
    /// Returns a bounded reason when the writer broke or lost its lease.
    async fn reached(&self, occurrence: &ApprovalOccurrence) -> Result<(), String> {
        let execution = self.writer.row.execution_id;
        self.change(retained(occurrence), |state| {
            Ok(record_approval(state, execution, occurrence.clone()))
        })
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
    use std::collections::{BTreeMap, BTreeSet};
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
        ExecutedOutcome, Publication, PublicationContent, PublicationKind,
    };
    use wyrd_spec::verification::state::{ApprovalOccurrence, ExecutionState};
    use wyrd_spec::verification::{TaskId, VerificationVerdict};

    use tokio_util::sync::CancellationToken;

    use super::{
        Decision, PublicationAcceptance, PublicationBudget, PublicationCall, RunState, StateError,
        StateRow, approval_id,
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

    /// A fresh execution's state, written through a Scribe that holds every
    /// revision until released.
    ///
    /// # Panics
    /// Panics when a fresh UUID is not a Card UID, or when called outside a
    /// Tokio runtime, where the outbox cannot spawn its writer.
    fn held_state() -> (RunState, Arc<TableGate>) {
        held_at(ExecutionState::fresh(), 0, CancellationToken::new())
    }

    /// A writer of `state`, accepted at `revision`, written through a Scribe
    /// that holds every revision until released, under the lease-loss token
    /// `lost`.
    ///
    /// # Panics
    /// Panics as [`held_state`] does.
    fn held_at(
        state: ExecutionState,
        revision: i64,
        lost: CancellationToken,
    ) -> (RunState, Arc<TableGate>) {
        let scribe = Arc::new(TableGate::new(ScribeTable::ExecutionEvents));
        let outbox = ScribeSink::local_outbox(Arc::clone(&scribe) as _, None);
        (writing(outbox, state, revision, lost), scribe)
    }

    /// A writer of `state` at `revision` behind a held Scribe whose outbox
    /// charges `memory`.
    fn charged_at(
        memory: &wyrd_runtime::resources::ByteLedger,
        state: ExecutionState,
        revision: i64,
    ) -> (RunState, Arc<TableGate>) {
        let scribe = Arc::new(TableGate::new(ScribeTable::ExecutionEvents));
        let outbox = ScribeSink::bounded_outbox(
            Some(crate::scribe_outbox::ScribeRoute::Local(
                Arc::clone(&scribe) as _,
                None,
            )),
            vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            ScribeSink::DEFAULT_LIMITS,
            None,
            Some(memory),
        );
        (
            writing(outbox, state, revision, CancellationToken::new()),
            scribe,
        )
    }

    /// A writer of `state` at `revision` staging on `outbox`, whose lease
    /// is lost with `lost`.
    fn writing(
        outbox: Arc<crate::scribe_outbox::ScribeOutbox>,
        state: ExecutionState,
        revision: i64,
        lost: CancellationToken,
    ) -> RunState {
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
        RunState::at(
            outbox,
            DataTenantId::new_v7(),
            row,
            tokio::time::Instant::now() + Duration::from_secs(30),
            lost,
            state,
            revision,
        )
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

    /// Waits for `state`'s outcome of [`RunState::persisted`].
    ///
    /// # Panics
    /// Panics when it does not settle within ten seconds.
    async fn settled(state: &RunState) -> Result<(), StateError> {
        tokio::time::timeout(Duration::from_secs(10), state.persisted())
            .await
            .expect("persistence settles")
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

    /// Proves an approval occurrence keeps its id across attempts and that
    /// another step, fanout item, repeat iteration, or execution never shares
    /// it, including item zero against no fanout.
    ///
    /// # Panics
    /// Panics when two distinct occurrences share an id or one occurrence
    /// derives two.
    #[test]
    fn each_approval_occurrence_has_its_own_stable_id() {
        let execution = ExecutionId::new_v7();
        let at = |step: &str, item: Option<u32>, iteration: Option<u32>| ApprovalOccurrence {
            step: task(step),
            item,
            iteration,
            message: "Ship?".to_owned(),
        };
        let occurrences = [
            at("sign-off", None, None),
            at("sign-off", Some(0), None),
            at("sign-off", Some(1), None),
            at("sign-off", None, Some(0)),
            at("sign-off", Some(0), Some(1)),
            at("review", None, None),
        ];
        let ids: BTreeSet<_> = occurrences
            .iter()
            .map(|occurrence| approval_id(execution, occurrence))
            .collect();
        assert_eq!(
            ids.len(),
            occurrences.len(),
            "every occurrence has its own id"
        );
        assert_eq!(
            approval_id(execution, &occurrences[0]),
            approval_id(execution, &occurrences[0]),
            "an occurrence keeps its id across attempts"
        );
        assert_ne!(
            approval_id(execution, &occurrences[0]),
            approval_id(ExecutionId::new_v7(), &occurrences[0]),
            "another execution never shares an occurrence id"
        );
    }

    /// Proves an engine transition completes from memory while Scribe holds
    /// every revision.
    ///
    /// # Panics
    /// Panics when a transition waits for a persistence acknowledgement.
    #[tokio::test]
    async fn transitions_complete_while_persistence_is_held() {
        let (state, _scribe) = held_state();
        tokio::time::timeout(
            Duration::from_secs(1),
            state.begin(&Producer::plain(&task("factual-check"), 1)),
        )
        .await
        .expect("begin completes from memory")
        .expect("begin is recorded");
    }

    /// Proves siblings record progress from memory while a revision is held,
    /// their changes coalesce into the one next revision, and a cancelled
    /// persistence waiter neither retracts a change nor strands another.
    ///
    /// # Panics
    /// Panics when a step waits, the changes take more than two revisions,
    /// or persistence is reported before the last change is accepted.
    #[tokio::test]
    async fn held_changes_coalesce_into_the_next_revision() {
        let (state, scribe) = held_state();
        let names = ["factual-check", "sensitive-check", "tone-check"];
        state
            .begin(&Producer::plain(&task(names[0]), 1))
            .await
            .expect("recorded");
        arrived(&scribe).await;
        let mut cancelled = Box::pin(state.persisted());
        assert!(pending(&mut cancelled).await, "revision 1 is held");
        drop(cancelled);
        for name in &names[1..] {
            tokio::time::timeout(
                Duration::from_secs(1),
                state.begin(&Producer::plain(&task(name), 1)),
            )
            .await
            .expect("a sibling records while a revision is held")
            .expect("recorded");
        }
        assert_eq!(state.snapshot().invocations.len(), 3);

        let mut waiter = Box::pin(state.persisted());
        scribe.release.add_permits(1);
        arrived(&scribe).await;
        assert!(
            pending(&mut waiter).await,
            "revision 1 does not hold the siblings"
        );
        scribe.release.add_permits(1);
        tokio::time::timeout(Duration::from_secs(10), waiter)
            .await
            .expect("persistence settles")
            .expect("revision 2 holds every change");

        let accepted = revisions(&scribe);
        assert_eq!(accepted.len(), 2, "three changes take two revisions");
        assert!(accepted[0].contains(names[0]) && !accepted[0].contains(names[1]));
        for name in names {
            assert!(accepted[1].contains(name), "{name} is in revision 2");
        }
    }

    /// Proves repeated terminal selections share one result identity,
    /// decided from memory, and one revision persists it.
    ///
    /// # Panics
    /// Panics when the identities differ or more than one revision is written.
    #[tokio::test]
    async fn duplicate_selections_share_one_result() {
        let (state, scribe) = held_state();
        let outcome = ExecutedOutcome {
            verdict: VerificationVerdict::Passed,
            requirements: Vec::new(),
            steps: Vec::new(),
            evidence: BTreeMap::new(),
        };
        let started = chrono::Utc::now();
        let first = state.select(&outcome, started).expect("selected");
        let second = state.select(&outcome, started).expect("selected");
        assert_eq!(first, second, "both share one selected result");
        scribe.release.add_permits(1);
        settled(&state).await.expect("the selection is accepted");
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
            .expect("the failure is recorded");
        assert!(matches!(failed, Decision::Failed(_)));
        let outcome = ExecutedOutcome {
            verdict: VerificationVerdict::Passed,
            requirements: Vec::new(),
            steps: Vec::new(),
            evidence: BTreeMap::new(),
        };
        let selected = state
            .select(&outcome, chrono::Utc::now())
            .expect("the decision reads");
        let refailed = state
            .fail(
                wyrd_spec::verification::VerificationExecutionStatus::TimedOut,
                &error("later"),
            )
            .expect("the decision reads");
        assert_eq!((&selected, &refailed), (&failed, &failed));
        assert!(state.snapshot().terminal.is_none());
        settled(&state).await.expect("the failure is accepted");
        assert_eq!(revisions(&scribe).len(), 1);
    }

    /// Proves a refused revision breaks a queued writer: its persistence
    /// waiter fails and its later changes are refused.
    ///
    /// # Panics
    /// Panics when the writer keeps accepting changes or reports persistence.
    #[tokio::test]
    async fn a_refused_revision_breaks_a_queued_writer() {
        let (state, scribe) = held_at(ExecutionState::fresh(), 0, CancellationToken::new());
        scribe
            .inner
            .refuse_next(vala_bifrost_redux::contracts::ScribeError::BatchConflict);
        scribe
            .release
            .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
        state
            .begin(&Producer::plain(&task("factual-check"), 1))
            .await
            .expect("recorded in memory");
        assert!(matches!(settled(&state).await, Err(StateError::Stale)));
        assert!(
            state
                .begin(&Producer::plain(&task("sensitive-check"), 1))
                .await
                .is_err()
        );
        assert!(revisions(&scribe).is_empty());
    }

    /// Proves a lost lease ends a queued writer's held revision and refuses
    /// every later change, decision, and publication.
    ///
    /// # Panics
    /// Panics when the lost writer still records or reports persistence.
    #[tokio::test]
    async fn a_lost_lease_stops_a_queued_writer() {
        let lost = CancellationToken::new();
        let (state, scribe) = held_at(ExecutionState::fresh(), 0, lost.clone());
        state
            .begin(&Producer::plain(&task("factual-check"), 1))
            .await
            .expect("recorded in memory");
        arrived(&scribe).await;
        lost.cancel();
        assert!(matches!(settled(&state).await, Err(StateError::Stale)));
        assert!(
            state
                .begin(&Producer::plain(&task("sensitive-check"), 1))
                .await
                .is_err()
        );
        let error = wyrd_spec::verification::VerificationError {
            code: "engine".to_owned(),
            message: "engine".to_owned(),
        };
        assert!(matches!(
            state.fail(
                wyrd_spec::verification::VerificationExecutionStatus::Errored,
                &error
            ),
            Err(StateError::Stale)
        ));
        assert_eq!(
            publish(&state, ("draft", 1), "p1", "a", serde_json::json!(1)),
            PublicationAcceptance::StaleLease
        );
        assert_eq!(state.snapshot().invocations.len(), 1);
    }

    /// Accept JSON `value` under `name` for provider call `call` of `task`'s
    /// `attempt` into `state`.
    ///
    /// # Panics
    /// Panics when `name` is invalid or `value` does not serialize.
    fn publish(
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
        state.accept_publication(
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
            publish(&live, draft, "p1", "a", serde_json::json!(1)),
            PublicationAcceptance::Accepted
        );
        assert_eq!(
            publish(&live, draft, "p1", "b", serde_json::json!(1)),
            PublicationAcceptance::Conflict,
            "a live call cannot take a second name"
        );
        let (restored, scribe) = held_at(live.snapshot(), 1, CancellationToken::new());
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
                publish(&restored, draft, call, name, serde_json::json!(value)),
                expected,
                "{call} {name} {value}"
            );
        }
        assert_eq!(
            publish(&restored, ("draft", 2), "p1", "a", serde_json::json!(3)),
            PublicationAcceptance::Accepted,
            "a fresh attempt may reuse the name and call ID"
        );
        let kept: Vec<(u32, String, Value)> = restored
            .snapshot()
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

    /// Waits until `memory` holds `bytes`.
    ///
    /// # Panics
    /// Panics when it does not within ten seconds.
    async fn charged(memory: &wyrd_runtime::resources::ByteLedger, bytes: u64) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let changed = memory.changed();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if memory.used() == bytes {
                    return;
                }
                changed.await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("memory holds {} bytes, not {bytes}", memory.used()));
    }

    /// Proves the live state is charged to process memory for as long as
    /// its writer holds it: a recovered state is charged on arrival, every
    /// later change grows the charge while an older revision is held, a
    /// frozen revision is charged on top until Scribe accepts it, and the
    /// acknowledgement returns only the frozen copy. Siblings cannot be
    /// promised the live bytes, and dropping the writer returns them once.
    ///
    /// # Panics
    /// Panics when the live state goes uncharged at any point, an
    /// acknowledgement returns live bytes, or the final release differs.
    #[tokio::test]
    async fn live_state_stays_charged_through_acknowledged_revisions() {
        let limit = 1 << 30;
        let memory = wyrd_runtime::resources::ByteLedger::new(limit);
        let mut recovered = ExecutionState::fresh();
        recovered.waiting = (0..64)
            .map(|at| wyrd_spec::ids::SlotName::new(format!("slot_{at}")).expect("slot"))
            .collect();
        let (state, scribe) = charged_at(&memory, recovered.clone(), 1);
        let mut live = super::Checkpoint::live(&state.snapshot());
        assert!(live > crate::verification::encoded_len(&recovered, false));
        assert_eq!(memory.used(), live, "a recovered state is charged");

        let first = Producer::plain(&task("factual"), 1);
        state.begin(&first).await.expect("recorded");
        live += super::retained(&first);
        arrived(&scribe).await;
        assert!(
            memory.used() > live,
            "the held revision is charged beside the live state"
        );
        let second = Producer::plain(&task("tone"), 1);
        state.begin(&second).await.expect("recorded");
        live += super::retained(&second);
        assert!(memory.used() > live, "later growth is charged while held");

        scribe
            .release
            .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
        settled(&state).await.expect("both changes are accepted");
        charged(&memory, live).await;
        assert!(
            memory.try_charge(limit - live + 1).is_err(),
            "a sibling cannot be promised the live bytes"
        );

        let read = state.publications(&task("factual"));
        assert!(read.is_empty());
        drop(read);
        drop(state);
        charged(&memory, 0).await;
    }

    /// Proves a publication read out of the live state is charged before it
    /// is copied and holds that charge until the copy drops.
    ///
    /// # Panics
    /// Panics when the copy is uncharged or keeps its charge once dropped.
    #[tokio::test]
    async fn publication_reads_hold_their_copy_charge() {
        let memory = wyrd_runtime::resources::ByteLedger::new(1 << 30);
        let (state, scribe) = charged_at(&memory, ExecutionState::fresh(), 0);
        scribe
            .release
            .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
        let value = serde_json::json!("x".repeat(4_096));
        assert_eq!(
            publish(&state, ("draft", 1), "c1", "notes", value),
            PublicationAcceptance::Accepted
        );
        settled(&state).await.expect("accepted");
        let live = memory.used();

        let read = state.publications(&task("draft"));
        assert_eq!(read.len(), 1);
        assert!(
            memory.used() > live + 4_096,
            "the copy is charged beside the live state"
        );
        drop(read);
        assert_eq!(memory.used(), live, "the copy's charge returns with it");
    }
}
