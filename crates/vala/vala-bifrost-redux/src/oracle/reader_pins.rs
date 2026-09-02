//! The one process-global owner of this Oracle epoch's reader authority.
//!
//! Forge maintenance runs in a different process from every reader, so an
//! admitted query has to leave durable evidence or destructive maintenance
//! would be deciding from age and liveness alone. Publishing a row per query
//! would put a Postgres write on every read, so this module aggregates instead:
//! one [`OracleReaderAuthority`] per fenced Oracle epoch owns one
//! [`OracleTableCoordinator`] per durable table, each coordinator reduces its
//! process-local cuts into the smallest frontier that covers them all, and only
//! a first protection or a genuine widening reaches Postgres. A query whose
//! snapshot the confirmed frontier already covers is admitted from memory and
//! performs no SQL at all.
//!
//! Two rules make that safe. Protection is synchronous: every table in a query
//! cut is durably protected before the query opens a manifest, a delete object,
//! or a data object. And the epoch is bounded: the authority holds a renewable
//! Postgres lease and self-fences — closing admission, cancelling, and joining
//! every descendant — before that lease can stop authorizing source IO.
//!
//! Nothing here decides whether a snapshot may be expired. It publishes what
//! this epoch needs; the maintenance owner corroborates that against Iceberg.
//! It is also not a live-tail lease registry: a Scribe live tail retains Arrow
//! batches and locally staged resources, names no Forge-collectable
//! object-store key, and is not represented here.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;
use vala_sql::queries::oracle_reader_authority::{
    BifrostTableMaintenanceAuthority, OracleReaderEpochs, OracleTableProtections,
};
use vala_sql::row_types::oracle_reader_authority::{
    OracleEpochState, OracleLeaseSample, ProtectionCas, ProtectionFrontier, ProtectionMember,
    ProtectionRecord, TableAuthorityIdentity,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, OracleReaderEpochPhase,
    OracleTableProtectionPhase,
};

use crate::catalog::{BIFROST_CATALOG_NAME, PinnedSealedTable};

/// Bounded authority one acquisition or renewal buys this epoch.
pub(crate) const EPOCH_LEASE: std::time::Duration = std::time::Duration::from_secs(30);
/// Delay between renewal attempts while the epoch holds authority.
pub(crate) const EPOCH_RENEWAL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
/// How far before database expiry readiness and admission must already be shut.
///
/// The margin has to cover cancelling every descendant and joining it, so it is
/// the join budget plus the slack an ordinary cancellation needs, not a round
/// number chosen for comfort.
pub(crate) const EPOCH_SELF_FENCE_MARGIN: std::time::Duration = std::time::Duration::from_secs(10);
/// Margin the readiness cutoff keeps ahead of the no-IO deadline.
///
/// Named as one constant because it is a difference of two durations that must
/// never be computed at a call site where an accidental underflow would widen
/// the authority window rather than narrow it.
pub(crate) const EPOCH_READINESS_MARGIN: std::time::Duration = std::time::Duration::from_secs(8);
/// Bounded time descendants have to finish after loss cancels the epoch root.
pub(crate) const EPOCH_JOIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(4);
/// Unconditional subtraction covering SQL round trip and scheduling jitter.
///
/// This is not a measured clock-skew verdict and no local wall clock takes part
/// in it. It is the fixed pessimism applied to a database-reported remaining
/// lease before that remainder is projected onto the local monotonic clock.
pub(crate) const EPOCH_DATABASE_TIME_ALLOWANCE: std::time::Duration =
    std::time::Duration::from_secs(2);

/// Principal recorded for background epoch and protection transitions.
const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

/// Constructs an internal Bifrost failure without leaking row payloads.
fn internal(detail: impl Into<String>) -> BifrostError {
    BifrostError::Internal {
        detail: detail.into(),
    }
}

/// One process-local query cut on one table, as the frontier consumes it.
///
/// The ancestry path is captured from the immutable Iceberg metadata the cut
/// was named from, newest-to-oldest and inclusive of the cut itself. Carrying
/// it is what lets two cuts be compared for ancestry without a second metadata
/// read and without ever inferring lineage from snapshot-ID order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalReaderCut {
    /// Exact snapshot this query reads.
    pub snapshot_id: i64,
    /// Iceberg timestamp of that snapshot.
    pub timestamp_ms: i64,
    /// Inclusive newest-to-oldest parent walk starting at `snapshot_id`.
    pub ancestry_path: Vec<i64>,
}

/// Reduces every active local cut on one table into its smallest frontier.
///
/// Cuts are folded newest-first. A cut already named in some retained head's
/// ancestry extends that chain's protected endpoint instead of starting a new
/// one; a cut on no existing chain becomes a new member. Incomparable lineages
/// therefore survive as separate members, which is the whole reason a frontier
/// is a set rather than a single oldest snapshot.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when a produced member fails the durable
/// validation rules, which means the ancestry a cut supplied was malformed.
/// One protection chain under construction inside frontier reduction.
///
/// A chain is a proven lineage: its `head` is the newest active cut whose own
/// ancestry path witnesses every older cut folded into it, and the two
/// `protected_*` fields record how far down that path the fold has reached.
struct Chain<'cut> {
    /// Newest active cut on the chain, whose path proves the chain.
    head: &'cut LocalReaderCut,
    /// Index in `head.ancestry_path` of the oldest active cut so far.
    protected_index: usize,
    /// Timestamp of the oldest active cut so far.
    protected_timestamp_ms: i64,
}

pub fn frontier_from_active_cuts(
    identity: &TableAuthorityIdentity,
    cuts: &BTreeMap<u64, LocalReaderCut>,
) -> Result<ProtectionFrontier, BifrostError> {
    frontier_from_cuts(identity, cuts.values().collect())
}

/// Reduces an exact selection of active cuts into its smallest frontier.
///
/// Narrowing needs the frontier a table *would* have without one cut before it
/// removes that cut locally, so the selection is passed in rather than derived
/// from the coordinator's current map.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when a produced member fails the durable
/// validation rules, which means the ancestry a cut supplied was malformed.
fn frontier_from_cuts(
    identity: &TableAuthorityIdentity,
    mut ordered: Vec<&LocalReaderCut>,
) -> Result<ProtectionFrontier, BifrostError> {
    ordered.sort_by(|left, right| {
        right
            .timestamp_ms
            .cmp(&left.timestamp_ms)
            .then_with(|| right.snapshot_id.cmp(&left.snapshot_id))
    });

    let mut chains: Vec<Chain<'_>> = Vec::new();
    for cut in ordered {
        let extended = chains.iter_mut().find_map(|chain| {
            chain
                .head
                .ancestry_path
                .iter()
                .position(|id| *id == cut.snapshot_id)
                .map(|index| (chain, index))
        });
        match extended {
            Some((chain, index)) => {
                if index > chain.protected_index {
                    chain.protected_index = index;
                    chain.protected_timestamp_ms = cut.timestamp_ms;
                }
            }
            None => chains.push(Chain {
                head: cut,
                protected_index: 0,
                protected_timestamp_ms: cut.timestamp_ms,
            }),
        }
    }

    let mut members = Vec::with_capacity(chains.len());
    for chain in chains {
        let path = chain
            .head
            .ancestry_path
            .get(..=chain.protected_index)
            .ok_or_else(|| internal("Oracle reader cut ancestry index escaped its own path"))?
            .to_vec();
        members.push(
            ProtectionMember::new(
                identity,
                path,
                chain.head.timestamp_ms,
                chain.protected_timestamp_ms,
            )
            .map_err(|error| internal(error.to_string()))?,
        );
    }
    ProtectionFrontier::new(identity, members).map_err(|error| internal(error.to_string()))
}

/// Derives one query's per-table cut from a prepared reader identity.
///
/// Runs before any snapshot-dependent IO: the identity already carries the
/// snapshot, its timestamp, and its ancestry from the immutable metadata
/// document, so nothing here opens an object. A prepared identity with no
/// snapshot contributes nothing, because there is no snapshot for maintenance
/// to protect.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when the identity names a snapshot it
/// could not date or whose ancestry does not begin at that snapshot. Both are
/// unprovable coverage claims, and a query is about to read that snapshot.
pub fn local_cut_from_prepared(
    prepared: &crate::catalog::PreparedReaderIdentity,
) -> Result<Option<(TableAuthorityIdentity, LocalReaderCut)>, BifrostError> {
    let Some(snapshot_id) = prepared.snapshot_id else {
        return Ok(None);
    };
    let timestamp_ms = prepared.snapshot_timestamp_ms.ok_or_else(|| {
        internal(format!(
            "Oracle prepared snapshot {snapshot_id} that its own table metadata cannot date, so \
             it cannot be protected from maintenance"
        ))
    })?;
    if prepared.ancestry_path.first() != Some(&snapshot_id) {
        return Err(internal(format!(
            "Oracle prepared snapshot {snapshot_id} with an ancestry that does not begin at it"
        )));
    }
    let identity = TableAuthorityIdentity {
        tenant: prepared.binding.tenant,
        table_uid: *prepared.table_uid.as_bytes(),
        catalog_name: BIFROST_CATALOG_NAME.to_owned(),
        namespace_name: prepared.binding.table_ref.namespace.as_str().to_owned(),
        table_name: prepared.binding.table_ref.name.clone(),
    };
    Ok(Some((
        identity,
        LocalReaderCut {
            snapshot_id,
            timestamp_ms,
            ancestry_path: prepared.ancestry_path.clone(),
        },
    )))
}

/// Derives one follower's per-table cut from the leader's signed evidence.
///
/// Returns `None` for a cut that names no snapshot, which is how the leader
/// describes a scan that reads only a live writer tail.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when the signed ancestry does not begin
/// at the signed snapshot, when its declared digest version is not the one this
/// build proves, or when recomputing the member digest does not reproduce the
/// signed value. All three mean the follower cannot prove the lineage it is
/// about to claim protection over.
pub fn local_cut_from_follower(
    identity: &TableAuthorityIdentity,
    cut: &wyrd_spec::vala::api::FollowerReaderCut,
) -> Result<Option<LocalReaderCut>, BifrostError> {
    if !cut.protects_a_snapshot() {
        return Ok(None);
    }
    if cut.ancestry_path.first() != Some(&cut.snapshot_id) {
        return Err(internal(format!(
            "Oracle follower cut {} has an ancestry that does not begin at it",
            cut.snapshot_id
        )));
    }
    let expected_version =
        u32::try_from(vala_sql::row_types::oracle_reader_authority::ANCESTRY_DIGEST_VERSION)
            .map_err(|_| internal("Oracle ancestry digest version does not fit its wire type"))?;
    if cut.ancestry_digest_version != expected_version {
        return Err(internal(format!(
            "Oracle follower cut declares ancestry digest version {} but this node proves {}",
            cut.ancestry_digest_version, expected_version
        )));
    }
    let member = ProtectionMember::new(
        identity,
        cut.ancestry_path.clone(),
        cut.snapshot_timestamp_ms,
        cut.snapshot_timestamp_ms,
    )
    .map_err(|error| internal(error.to_string()))?;
    let mut recomputed = String::with_capacity(64);
    for byte in member.ancestry_digest {
        use std::fmt::Write as _;
        let _ = write!(recomputed, "{byte:02x}");
    }
    if recomputed != cut.ancestry_digest_hex {
        return Err(internal(
            "Oracle follower cut ancestry does not reproduce its own signed digest",
        ));
    }
    Ok(Some(LocalReaderCut {
        snapshot_id: cut.snapshot_id,
        timestamp_ms: cut.snapshot_timestamp_ms,
        ancestry_path: cut.ancestry_path.clone(),
    }))
}

/// What one process does when its epoch cannot join descendants in time.
///
/// Production aborts: a process that still has an unjoined task holding an open
/// object read after its lease expired can no longer prove it will not read,
/// and there is no weaker action that restores that proof. Tests inject a
/// recording terminator so the same ordering can be observed without ending the
/// test process.
pub trait OracleEpochTerminator: Send + Sync + std::fmt::Debug {
    /// Ends this process's ability to perform any further source IO.
    fn terminate(&self, detail: &str);
}

/// Production terminator that aborts the process.
#[derive(Debug, Default)]
pub struct AbortingEpochTerminator;

impl OracleEpochTerminator for AbortingEpochTerminator {
    /// Aborts immediately after recording why the epoch could not self-fence.
    fn terminate(&self, detail: &str) {
        tracing::error!(
            detail,
            "Oracle epoch could not join snapshot-dependent work before its lease expired"
        );
        std::process::abort();
    }
}

/// Test terminator that records invocations instead of ending the process.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct RecordingEpochTerminator {
    /// Number of times the epoch failed to join within its budget.
    invocations: std::sync::atomic::AtomicUsize,
}

#[cfg(any(test, feature = "test-support"))]
impl RecordingEpochTerminator {
    /// Reports how many times termination was demanded.
    #[must_use]
    pub fn invocations(&self) -> usize {
        self.invocations.load(Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl OracleEpochTerminator for RecordingEpochTerminator {
    /// Records the demand and returns, leaving the test process alive.
    fn terminate(&self, detail: &str) {
        tracing::error!(detail, "recording Oracle epoch terminator invoked");
        self.invocations.fetch_add(1, Ordering::SeqCst);
    }
}

/// Local phase of one epoch, independent of what Postgres last confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpochPhase {
    /// The lease exists and no read has been admitted under it.
    Acquired,
    /// Dependencies are established and admission is open.
    Active,
    /// Loss or retirement was selected; admission is closed.
    Closed,
}

/// Deadlines derived once per confirmed lease, on the monotonic clock.
///
/// Every deadline is computed from the remaining lease the database itself
/// reported, projected onto the instant sampled immediately before the SQL was
/// issued. None of them is ever recomputed from local wall time or extended
/// from the instant the response happened to arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpochDeadlines {
    /// Conservative instant after which no new source IO may start or expose.
    pub no_io: tokio::time::Instant,
    /// Instant at which readiness and admission must already be closed.
    pub admission_cutoff: tokio::time::Instant,
    /// Instant by which every snapshot-dependent descendant must be joined.
    pub join: tokio::time::Instant,
}

impl EpochDeadlines {
    /// Converts one database lease sample into local monotonic deadlines.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the database reported a
    /// remainder at or below the self-fence margin, or a malformed or
    /// unrepresentable duration. All three are immediate lease loss rather than
    /// a shorter lease.
    pub fn from_sample(
        sampled_before: tokio::time::Instant,
        sample: OracleLeaseSample,
    ) -> Result<Self, BifrostError> {
        let remaining = sample
            .lease_expires_at
            .signed_duration_since(sample.database_now)
            .to_std()
            .map_err(|_| internal("Oracle epoch lease has already expired in database time"))?;
        if remaining <= EPOCH_SELF_FENCE_MARGIN {
            return Err(internal(
                "Oracle epoch lease remainder is inside its self-fence margin",
            ));
        }
        let no_io = sampled_before
            .checked_add(remaining)
            .and_then(|instant| instant.checked_sub(EPOCH_DATABASE_TIME_ALLOWANCE))
            .ok_or_else(|| internal("Oracle epoch lease deadline is not representable"))?;
        let admission_cutoff = no_io
            .checked_sub(EPOCH_READINESS_MARGIN)
            .ok_or_else(|| internal("Oracle epoch admission cutoff is not representable"))?;
        let join = admission_cutoff
            .checked_add(EPOCH_JOIN_BUDGET)
            .ok_or_else(|| internal("Oracle epoch join deadline is not representable"))?;
        Ok(Self {
            no_io,
            admission_cutoff,
            join,
        })
    }
}

/// Mutable epoch state guarded by one mutex.
///
/// The renewal task holds this mutex across its database transaction and the
/// replacement of the confirmed revision and deadlines, so a renewal that is
/// already in flight is always observed before loss selects itself, and no
/// renewal can start after loss.
#[derive(Debug)]
struct EpochLifecycle {
    /// Local admission phase.
    phase: EpochPhase,
    /// Exact revision Postgres last confirmed for this epoch.
    confirmed_revision: i64,
    /// Deadlines derived from that confirmation.
    deadlines: EpochDeadlines,
    /// Whether loss has already been selected exactly once.
    loss_selected: bool,
}

/// One table's process-local cuts and confirmed durable protection.
///
/// One async mutex covers both halves because they are one decision: whether a
/// new cut needs a durable widening is answered by comparing it against the
/// confirmed frontier, and the answer is only valid while no other admission or
/// narrowing can change either side. The coordinator is the value behind that
/// mutex; its identity is the map key the authority holds it under.
#[derive(Debug, Default)]
struct OracleTableCoordinator {
    /// Monotonic identity issued to each local cut on this table.
    next_cut_id: u64,
    /// Every unreleased local cut, keyed by the identity issued when taken.
    active: BTreeMap<u64, LocalReaderCut>,
    /// The exact protection revision this epoch has confirmed, when any.
    confirmed: Option<ProtectionRecord>,
}

/// One reserved instruction to remove a query's pins from its tables.
///
/// The permit is reserved at admission, before any cut is published, so a
/// saturated queue can never lose a release: the query could not have been
/// admitted without one.
#[derive(Debug)]
struct NarrowingCommand {
    /// Exact per-table pins to remove.
    holdings: Vec<(TableAuthorityIdentity, u64)>,
    /// Reservation consumed by this command.
    _permit: OwnedSemaphorePermit,
}

/// Clonable proof that one query may still perform snapshot-dependent IO.
///
/// Both cancellation tokens and the conservative no-IO deadline are inputs, so
/// either cancellation refuses new IO and the earlier of the two bounds every
/// operation. The permit is checked immediately before an operation starts and
/// again before its result is exposed, which is what stops a read begun before
/// epoch loss from returning bytes afterwards.
#[derive(Debug, Clone)]
pub struct ReaderIoPermit {
    /// Cancellation root of the owning epoch.
    epoch_cancel: CancellationToken,
    /// Cancellation token of the owning query.
    query_cancel: CancellationToken,
    /// Conservative local instant after which this epoch performs no source IO.
    no_io_deadline: tokio::time::Instant,
}

impl ReaderIoPermit {
    /// Builds a permit bound to one query under one epoch.
    #[must_use]
    pub fn new(
        epoch_cancel: CancellationToken,
        query_cancel: CancellationToken,
        no_io_deadline: tokio::time::Instant,
    ) -> Self {
        Self {
            epoch_cancel,
            query_cancel,
            no_io_deadline,
        }
    }

    /// Authorizes one snapshot-dependent operation about to start.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the query or epoch has been
    /// cancelled, or when the conservative no-IO deadline has passed.
    pub fn begin_io(&self) -> Result<(), BifrostError> {
        self.check("start")
    }

    /// Authorizes exposing one already-completed operation's result.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] under the same conditions as
    /// [`ReaderIoPermit::begin_io`]. A read that started legally but completed
    /// after the epoch lost authority fails here rather than returning bytes.
    pub fn expose_result(&self) -> Result<(), BifrostError> {
        self.check("expose")
    }

    /// Shared gate for both boundaries.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] naming which boundary refused.
    fn check(&self, boundary: &str) -> Result<(), BifrostError> {
        if self.query_cancel.is_cancelled() {
            return Err(internal(format!(
                "Oracle refused to {boundary} snapshot IO for a cancelled query"
            )));
        }
        if self.epoch_cancel.is_cancelled() {
            return Err(internal(format!(
                "Oracle refused to {boundary} snapshot IO after its reader epoch was fenced"
            )));
        }
        if tokio::time::Instant::now() >= self.no_io_deadline {
            return Err(internal(format!(
                "Oracle refused to {boundary} snapshot IO past its reader epoch's no-IO deadline"
            )));
        }
        Ok(())
    }

    /// Builds a permit that authorizes IO for the life of one focused test.
    ///
    /// Test-only: no epoch, no query, and a deadline far past any test's
    /// runtime. It exists so catalog-level tests can exercise materialization
    /// without standing up a durable reader epoch, and it is never reachable
    /// from a production build.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn unfenced_for_test() -> Self {
        Self::new(
            CancellationToken::new(),
            CancellationToken::new(),
            tokio::time::Instant::now() + std::time::Duration::from_hours(1),
        )
    }

    /// Reports the conservative instant this permit stops authorizing IO.
    #[must_use]
    pub fn no_io_deadline(&self) -> tokio::time::Instant {
        self.no_io_deadline
    }
}

/// Process-local ownership of one query's complete protected cut.
///
/// The guard is the query's claim, and its lifetime is the cut's lifetime by
/// construction: a plan abandoned before execution, a stream dropped mid-scan,
/// and a cancelled attempt all release through the same path without anyone
/// remembering to. Release enqueues the reservation taken at admission rather
/// than performing SQL inline, because the last descendant may terminate on a
/// path that cannot await.
pub struct ReaderQueryGuard {
    /// Authority the reserved release command is sent to.
    authority: Arc<OracleReaderAuthority>,
    /// Exact per-table pins this query holds.
    holdings: Vec<(TableAuthorityIdentity, u64)>,
    /// Reservation consumed when the pins are enqueued for removal.
    release: Option<OwnedSemaphorePermit>,
    /// Query-scoped cancellation shared with every permit this guard issued.
    ///
    /// Cancelling it on release is what makes the permit strictly weaker than
    /// the guard: once the cut is no longer protected, no permit derived from
    /// it can authorize another read or expose a read already in flight.
    query_cancel: CancellationToken,
}

impl std::fmt::Debug for ReaderQueryGuard {
    /// Prints the held table count without the identities themselves.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReaderQueryGuard")
            .field("tables", &self.holdings.len())
            .field("released", &self.release.is_none())
            .field("cancelled", &self.query_cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl Drop for ReaderQueryGuard {
    /// Enqueues this query's exact pin removals using its reserved permit.
    ///
    /// A closed queue means the authority is already shutting down and has
    /// taken ownership of draining; the pins it would remove are removed there.
    fn drop(&mut self) {
        self.query_cancel.cancel();
        let Some(permit) = self.release.take() else {
            return;
        };
        let command = NarrowingCommand {
            holdings: std::mem::take(&mut self.holdings),
            _permit: permit,
        };
        if self.authority.narrowing_tx.try_send(command).is_err() {
            tracing::debug!(
                "Oracle reader guard released after its authority closed; shutdown owns the drain"
            );
        }
    }
}

/// The one process-global reader-authority owner for a fenced Oracle epoch.
///
/// It owns three things and nothing else: the epoch's bounded lease, one
/// coordinator per durable table, and the bounded worker that applies batched
/// narrowing. Everything a query needs — admission, the durable widening, the
/// IO permit, and the release — is reached through it, so there is one place
/// where "may this read happen" is answered.
pub struct OracleReaderAuthority {
    /// Tenant-scoped SQL handle every protection transaction opens from.
    vala: vala_sql::ValaPostgres,
    /// Read-only cross-tenant pool used solely by recovery enumeration.
    operator_pool: vala_sql::OperatorPool,
    /// Physical node holding this epoch.
    node_id: uuid::Uuid,
    /// Exact `cluster_nodes` Oracle fence this epoch was acquired under.
    fencing_token: i64,
    /// Lease phase, confirmed revision, and deadlines under one mutex.
    lifecycle: Mutex<EpochLifecycle>,
    /// Cancellation root every descendant of this epoch observes.
    epoch_cancel: CancellationToken,
    /// Fires the instant loss is selected, before its audited edge commits.
    ///
    /// Separate from `epoch_cancel` because readiness must close at selection
    /// while descendants keep running until the loss edge is durable.
    loss_selected_notify: CancellationToken,
    /// Stops the lease supervisor's renewal cadence without aborting it.
    renewal_cancel: CancellationToken,
    /// One coordinator per durable table, keyed in canonical lock order.
    coordinators: Mutex<BTreeMap<TableAuthorityIdentity, Arc<Mutex<OracleTableCoordinator>>>>,
    /// Release reservations, one per concurrently admissible query.
    release_permits: Arc<Semaphore>,
    /// Exact reservation count, which is also the fully-drained permit total.
    release_capacity: usize,
    /// Bounded queue the narrowing worker drains.
    narrowing_tx: mpsc::Sender<NarrowingCommand>,
    /// Signal telling the narrowing worker to drain what remains and stop.
    narrowing_drain: CancellationToken,
    /// Whether new admission is still open.
    admission_open: AtomicBool,
    /// What this process does when it cannot join descendants in time.
    terminator: Arc<dyn OracleEpochTerminator>,
    /// Bounded worker draining reserved narrowing commands.
    ///
    /// Owned here rather than by the engine because retirement's ordering is
    /// this type's invariant: the narrowing worker must be drained and joined
    /// after descendants are joined and before any table is released, and no
    /// caller can be relied on to sequence that from outside.
    narrowing_worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Renewal and self-fence supervisor for this epoch's lease.
    ///
    /// Held separately from the narrowing worker because retirement must join
    /// it first, while resolving who owns this epoch's loss, and long before
    /// any protection is released.
    lease_worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Test-only latch making the confirmed lease stop extending.
    #[cfg(any(test, feature = "test-support"))]
    lease_collapsed: AtomicBool,
    /// Test-only count of protection commits to refuse before the next one.
    #[cfg(any(test, feature = "test-support"))]
    protection_faults: std::sync::atomic::AtomicUsize,
}

impl std::fmt::Debug for OracleReaderAuthority {
    /// Prints epoch identity without any tenant, table, or snapshot payload.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OracleReaderAuthority")
            .field("node_id", &self.node_id)
            .field("fencing_token", &self.fencing_token)
            .finish_non_exhaustive()
    }
}

/// Everything one authority needs that is not derived from its own lease.
pub struct OracleReaderAuthorityConfig {
    /// Tenant-scoped SQL handle every protection transaction opens from.
    pub vala: vala_sql::ValaPostgres,
    /// Read-only cross-tenant pool used solely by recovery enumeration.
    pub operator_pool: vala_sql::OperatorPool,
    /// Physical node holding this epoch.
    pub node_id: uuid::Uuid,
    /// Exact `cluster_nodes` Oracle fence this epoch is acquired under.
    pub fencing_token: u64,
    /// Maximum queries this Oracle admits at once, which bounds the queue.
    pub max_concurrent_queries: usize,
    /// What this process does when it cannot join descendants in time.
    pub terminator: Arc<dyn OracleEpochTerminator>,
    /// Process shutdown token that stops this epoch's lease supervisor.
    pub shutdown: CancellationToken,
}

impl OracleReaderAuthority {
    /// Acquires this epoch's lease and starts its supervised narrowing worker.
    ///
    /// Acquisition is synchronous and audited: the epoch exists in Postgres at
    /// revision 1 before this returns, so nothing can be admitted under an
    /// epoch Forge cannot see. The worker is spawned here rather than by the
    /// caller because a worker that is built but never polled is
    /// indistinguishable from one that is running yet drains nothing.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when no Tokio runtime is entered,
    /// when the concurrency bound is zero, when the fence is not representable,
    /// when the `cluster_nodes` fence has already been replaced, when an epoch
    /// row already exists at this exact new fence, or when the acquisition
    /// transaction or its audit fails.
    pub async fn start(config: OracleReaderAuthorityConfig) -> Result<Arc<Self>, BifrostError> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| internal("Oracle reader authority requires an active Tokio runtime"))?;
        if config.max_concurrent_queries == 0 {
            return Err(internal(
                "Oracle reader authority needs a positive query concurrency bound",
            ));
        }
        let fencing_token = i64::try_from(config.fencing_token)
            .map_err(|_| internal("Oracle role fence exceeds the durable epoch fence width"))?;

        let sampled_before = tokio::time::Instant::now();
        let sample = {
            let mut conn = system_conn(&config.vala).await?;
            let sample = OracleReaderEpochs::new(&mut conn)
                .map_err(|error| internal(error.to_string()))?
                .acquire(config.node_id, fencing_token, EPOCH_LEASE)
                .await
                .map_err(|error| internal(error.to_string()))?;
            append_epoch_audit(
                &mut conn,
                config.node_id,
                fencing_token,
                OracleReaderEpochPhase::Acquired,
                sample.state_revision,
            )
            .await?;
            conn.commit()
                .await
                .map_err(|error| internal(error.to_string()))?;
            sample
        };
        let deadlines = EpochDeadlines::from_sample(sampled_before, sample)?;

        let (narrowing_tx, narrowing_rx) = mpsc::channel(config.max_concurrent_queries);
        let authority = Arc::new(Self {
            vala: config.vala,
            operator_pool: config.operator_pool,
            node_id: config.node_id,
            fencing_token,
            lifecycle: Mutex::new(EpochLifecycle {
                phase: EpochPhase::Acquired,
                confirmed_revision: sample.state_revision,
                deadlines,
                loss_selected: false,
            }),
            epoch_cancel: CancellationToken::new(),
            loss_selected_notify: CancellationToken::new(),
            renewal_cancel: CancellationToken::new(),
            coordinators: Mutex::new(BTreeMap::new()),
            release_permits: Arc::new(Semaphore::new(config.max_concurrent_queries)),
            release_capacity: config.max_concurrent_queries,
            narrowing_tx,
            narrowing_drain: CancellationToken::new(),
            admission_open: AtomicBool::new(false),
            terminator: config.terminator,
            narrowing_worker: Mutex::new(None),
            lease_worker: Mutex::new(None),
            #[cfg(any(test, feature = "test-support"))]
            lease_collapsed: AtomicBool::new(false),
            #[cfg(any(test, feature = "test-support"))]
            protection_faults: std::sync::atomic::AtomicUsize::new(0),
        });
        let narrowing = runtime.spawn(Arc::clone(&authority).run_narrowing(narrowing_rx));
        let lease = runtime.spawn(Arc::clone(&authority).supervise_lease(config.shutdown));
        *authority.narrowing_worker.lock().await = Some(narrowing);
        *authority.lease_worker.lock().await = Some(lease);
        Ok(authority)
    }

    /// Borrows the cancellation root every descendant of this epoch observes.
    #[must_use]
    pub fn epoch_cancel(&self) -> &CancellationToken {
        &self.epoch_cancel
    }

    /// Borrows the token fired the instant this epoch's loss is selected.
    ///
    /// Consumers that must stop advertising authority — the continuity monitor
    /// above all — wait on this rather than on `epoch_cancel`, which is only
    /// cancelled after the audited loss edge is attempted.
    #[must_use]
    pub fn loss_selected_notify(&self) -> &CancellationToken {
        &self.loss_selected_notify
    }

    /// Reports the exact fence this epoch holds.
    #[must_use]
    pub fn fencing_token(&self) -> i64 {
        self.fencing_token
    }

    /// Reports the node this epoch belongs to.
    #[must_use]
    pub fn node_id(&self) -> uuid::Uuid {
        self.node_id
    }

    /// Borrows the terminator this epoch will invoke if a join overruns.
    #[must_use]
    pub fn terminator(&self) -> &Arc<dyn OracleEpochTerminator> {
        &self.terminator
    }

    /// Commits `acquired -> active` and opens admission.
    ///
    /// The caller must already have established every dependency the epoch
    /// needs: catalog, object store, read-audit relay, peer trust, recovery,
    /// renewal supervision, cancellation, and the narrowing worker. Readiness
    /// is set only after this commit, so an Oracle can never report ready under
    /// an epoch Postgres has not yet activated.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the fence was replaced, when no
    /// `acquired` row matches the confirmed revision, or when the transaction
    /// or its audit fails. Admission stays closed on every failure.
    pub async fn activate(&self) -> Result<(), BifrostError> {
        let mut lifecycle = self.lifecycle.lock().await;
        if lifecycle.phase != EpochPhase::Acquired {
            return Err(internal("Oracle reader epoch is no longer activatable"));
        }
        let sampled_before = tokio::time::Instant::now();
        let mut conn = system_conn(&self.vala).await?;
        let sample = OracleReaderEpochs::new(&mut conn)
            .map_err(|error| internal(error.to_string()))?
            .activate(
                self.node_id,
                self.fencing_token,
                lifecycle.confirmed_revision,
            )
            .await
            .map_err(|error| internal(error.to_string()))?;
        append_epoch_audit(
            &mut conn,
            self.node_id,
            self.fencing_token,
            OracleReaderEpochPhase::Activated,
            sample.state_revision,
        )
        .await?;
        conn.commit()
            .await
            .map_err(|error| internal(error.to_string()))?;
        lifecycle.deadlines = EpochDeadlines::from_sample(sampled_before, sample)?;
        lifecycle.confirmed_revision = sample.state_revision;
        lifecycle.phase = EpochPhase::Active;
        drop(lifecycle);
        self.admission_open.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Extends the lease once, or selects loss.
    ///
    /// The lifecycle mutex is held across the transaction and the replacement
    /// of the confirmed revision and deadlines, so an in-flight renewal is
    /// always observed before loss can select itself. A predicate that matches
    /// no row is lease loss, not a retryable failure.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the exact fence was replaced,
    /// when the renewal predicate matched nothing, or when the derived
    /// remainder is already inside the self-fence margin. Every one of those
    /// means the epoch has lost authority.
    pub async fn renew(&self) -> Result<(), BifrostError> {
        let mut lifecycle = self.lifecycle.lock().await;
        if lifecycle.phase == EpochPhase::Closed || lifecycle.loss_selected {
            return Err(internal("Oracle reader epoch has already lost authority"));
        }
        let sampled_before = tokio::time::Instant::now();
        let mut conn = system_conn(&self.vala).await?;
        let renewed = OracleReaderEpochs::new(&mut conn)
            .map_err(|error| internal(error.to_string()))?
            .renew(
                self.node_id,
                self.fencing_token,
                lifecycle.confirmed_revision,
                EPOCH_LEASE,
            )
            .await
            .map_err(|error| internal(error.to_string()))?;
        let sample =
            renewed.ok_or_else(|| internal("Oracle reader epoch could not renew its lease"))?;
        conn.commit()
            .await
            .map_err(|error| internal(error.to_string()))?;
        lifecycle.deadlines = EpochDeadlines::from_sample(sampled_before, sample)?;
        lifecycle.confirmed_revision = sample.state_revision;
        Ok(())
    }

    /// Reports the deadlines derived from the last confirmed lease.
    pub async fn deadlines(&self) -> EpochDeadlines {
        let deadlines = self.lifecycle.lock().await.deadlines;
        #[cfg(any(test, feature = "test-support"))]
        if self.lease_collapsed.load(Ordering::SeqCst) {
            let now = tokio::time::Instant::now();
            return EpochDeadlines {
                no_io: now,
                admission_cutoff: now,
                join: now,
            };
        }
        deadlines
    }

    /// Reports whether this epoch is currently admitting queries.
    #[must_use]
    pub fn admits(&self) -> bool {
        self.admission_open.load(Ordering::SeqCst)
    }

    /// Collapses this epoch's readiness and no-IO deadlines onto the current
    /// instant, and reports whether the collapse was applied.
    ///
    /// Test-only. A lease runs out in production because Postgres stopped
    /// extending it, which a test cannot reproduce without either a fake clock
    /// — unusable against a live server and a live database, whose pending IO
    /// makes the paused runtime auto-advance through unrelated timers — or a
    /// wait as long as the lease itself. Collapsing the derived deadlines
    /// leaves the supervisor observing exactly the state a real shortfall
    /// produces, so everything after it is the production self-fence path.
    ///
    /// The collapse latches, because a renewal that lands first would
    /// otherwise re-derive future deadlines from its fresh lease and hide the
    /// shortfall the caller is provoking.
    #[cfg(any(test, feature = "test-support"))]
    pub fn collapse_lease_for_test(&self) {
        self.lease_collapsed.store(true, Ordering::SeqCst);
    }

    /// Makes the next `count` protection commits fail before touching Postgres.
    ///
    /// Test-only. A narrowing that must be retried needs a commit that fails
    /// once and then succeeds, which no durable state can produce on its own:
    /// every reachable Postgres failure either persists or resolves outside the
    /// bounded retry it is supposed to exercise.
    #[cfg(any(test, feature = "test-support"))]
    pub fn inject_protection_faults_for_test(&self, count: usize) {
        self.protection_faults
            .store(count, std::sync::atomic::Ordering::SeqCst);
    }

    /// Reports how many injected protection-commit faults remain unconsumed.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn pending_protection_faults_for_test(&self) -> usize {
        self.protection_faults
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Closes admission exactly once and reports whether this call did it.
    ///
    /// Loss is selected under the lifecycle mutex so that exactly one caller
    /// owns the ordering that follows: cancel renewal, join it, commit the
    /// audited state edge at the latest confirmed revision, then cancel the
    /// epoch root.
    pub async fn select_loss(&self) -> bool {
        let mut lifecycle = self.lifecycle.lock().await;
        if lifecycle.loss_selected {
            return false;
        }
        lifecycle.loss_selected = true;
        lifecycle.phase = EpochPhase::Closed;
        drop(lifecycle);
        self.admission_open.store(false, Ordering::SeqCst);
        // Both before any awaited SQL: readiness must be gone at selection,
        // not once the audited loss edge has been accepted.
        self.loss_selected_notify.cancel();
        true
    }

    /// Commits the audited state edge loss requires for the current phase.
    ///
    /// An epoch that activated commits `draining`; one that never activated has
    /// admitted no read and proceeds straight to `invalidated` rather than
    /// inventing a draining edge it never earned.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the transaction, its audit, or
    /// the fence check fails. The epoch keeps its protection on failure.
    pub async fn commit_loss_edge(&self) -> Result<(), BifrostError> {
        let mut lifecycle = self.lifecycle.lock().await;
        let target = if self.epoch_activated().await? {
            OracleEpochState::Draining
        } else {
            OracleEpochState::Invalidated
        };
        let revision = self
            .commit_epoch_transition(lifecycle.confirmed_revision, target)
            .await?;
        lifecycle.confirmed_revision = revision;
        Ok(())
    }

    /// Reports whether Postgres shows this epoch as having activated.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the epoch row is missing or
    /// unreadable, which is contradictory evidence rather than an inactive
    /// epoch.
    async fn epoch_activated(&self) -> Result<bool, BifrostError> {
        let mut conn = system_conn(&self.vala).await?;
        let row = OracleReaderEpochs::new(&mut conn)
            .map_err(|error| internal(error.to_string()))?
            .read(self.node_id, self.fencing_token)
            .await
            .map_err(|error| internal(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| internal(error.to_string()))?;
        let row = row.ok_or_else(|| internal("Oracle reader epoch row disappeared"))?;
        Ok(row.activated_at.is_some())
    }

    /// Commits one audited epoch state edge and returns its new revision.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the fence was replaced, the
    /// predicate matched no row, or the transaction or audit failed. State and
    /// audit roll back together, so a failure emits neither.
    async fn commit_epoch_transition(
        &self,
        expected_revision: i64,
        target: OracleEpochState,
    ) -> Result<i64, BifrostError> {
        let mut conn = system_conn(&self.vala).await?;
        let revision = OracleReaderEpochs::new(&mut conn)
            .map_err(|error| internal(error.to_string()))?
            .transition(self.node_id, self.fencing_token, expected_revision, target)
            .await
            .map_err(|error| internal(error.to_string()))?;
        let phase = match target {
            OracleEpochState::Draining => OracleReaderEpochPhase::Draining,
            OracleEpochState::Invalidated => OracleReaderEpochPhase::Invalidated,
            OracleEpochState::Acquired | OracleEpochState::Active => {
                return Err(internal(
                    "Oracle reader epoch acquisition and activation have their own statements",
                ));
            }
        };
        append_epoch_audit(&mut conn, self.node_id, self.fencing_token, phase, revision).await?;
        conn.commit()
            .await
            .map_err(|error| internal(error.to_string()))?;
        Ok(revision)
    }
}

/// Opens one `SYSTEM_OWNER` transaction for an epoch statement.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when the connection cannot be acquired or
/// bound to the system tenant.
async fn system_conn(
    vala: &vala_sql::ValaPostgres,
) -> Result<vala_sql::TenantConn<'_>, BifrostError> {
    vala.tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .map_err(|error| internal(error.to_string()))
}

/// Appends the one canonical audit row for an epoch lifecycle transition.
///
/// Renewal deliberately has no call site here: it changes only the lease window
/// and revision, and auditing it every five seconds would bury the transitions
/// that actually change what Forge may destroy.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when the append fails; the caller's
/// transaction then rolls back the state edge with it.
async fn append_epoch_audit(
    conn: &mut vala_sql::TenantConn<'_>,
    node_id: uuid::Uuid,
    fencing_token: i64,
    phase: OracleReaderEpochPhase,
    state_revision: i64,
) -> Result<(), BifrostError> {
    let operation = match phase {
        OracleReaderEpochPhase::Acquired => "oracle.reader_epoch.acquired",
        OracleReaderEpochPhase::Activated => "oracle.reader_epoch.activated",
        OracleReaderEpochPhase::Draining => "oracle.reader_epoch.draining",
        OracleReaderEpochPhase::Invalidated => "oracle.reader_epoch.invalidated",
        OracleReaderEpochPhase::Retired => "oracle.reader_epoch.retired",
    };
    let event = AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: operation.to_owned(),
        resource: format!("oracle/reader_epoch/{node_id}/{fencing_token}"),
        card_ref: None,
        principal_id: SYSTEM_PRINCIPAL,
        principal_kind: PrincipalKindTag::Service,
        auth_method: AuthMethod::Internal,
        permission: "bifrost:oracle".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: operation.to_owned(),
        detail: Some(AuditDetail::OracleReaderEpoch {
            node_id,
            fencing_token,
            phase,
            state_revision,
        }),
    };
    vala_sql::queries::audit_outbox::append_audit(conn, &event)
        .await
        .map(|_| ())
        .map_err(|error| internal(error.to_string()))
}

/// Appends the one canonical audit row for a table protection transition.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when the append fails; the caller's
/// transaction then rolls back the protection change with it.
async fn append_protection_audit(
    conn: &mut vala_sql::TenantConn<'_>,
    identity: &TableAuthorityIdentity,
    node_id: uuid::Uuid,
    fencing_token: i64,
    phase: OracleTableProtectionPhase,
    revision: i64,
    frontier: &ProtectionFrontier,
) -> Result<(), BifrostError> {
    let operation = match phase {
        OracleTableProtectionPhase::Expanded => "oracle.table_protection.expanded",
        OracleTableProtectionPhase::Narrowed => "oracle.table_protection.narrowed",
        OracleTableProtectionPhase::Released => "oracle.table_protection.released",
    };
    let group = format!(
        "{}/{}/{}/{}",
        identity.tenant, identity.catalog_name, identity.namespace_name, identity.table_name
    );
    let mut protected_snapshot_ids: Vec<i64> = frontier
        .members
        .iter()
        .map(|member| member.protected_snapshot_id)
        .collect();
    protected_snapshot_ids.sort_unstable();
    let event = AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: operation.to_owned(),
        resource: group.clone(),
        card_ref: None,
        principal_id: SYSTEM_PRINCIPAL,
        principal_kind: PrincipalKindTag::Service,
        auth_method: AuthMethod::Internal,
        permission: "bifrost:oracle".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: operation.to_owned(),
        detail: Some(AuditDetail::OracleTableProtection {
            node_id,
            fencing_token,
            phase,
            group,
            revision,
            protected_snapshot_ids,
        }),
    };
    vala_sql::queries::audit_outbox::append_audit(conn, &event)
        .await
        .map(|_| ())
        .map_err(|error| internal(error.to_string()))
}

impl OracleReaderAuthority {
    /// Protects one query's complete immutable cut before it reads anything.
    ///
    /// Tables are locked in canonical `(tenant, table UID)` order and every
    /// coordinator guard is retained through local insertion, the coverage
    /// decision, the durable widening, and confirmed-revision replacement, so a
    /// request for two tables in the opposite order still serializes and no
    /// caller ever waits on a lower key while holding a higher one. All needed
    /// expansions commit before the guard is returned, which is what makes
    /// protection strictly precede source IO.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when admission is closed, when the
    /// epoch is past its admission cutoff, when a cut names a snapshot its own
    /// metadata cannot date or cannot walk, when the table has no maintenance
    /// authority row, or when a required durable widening could not commit.
    /// Every failure leaves prior conservative protection in place.
    pub async fn acquire_guard(
        self: &Arc<Self>,
        prepared: &[crate::catalog::PreparedReaderIdentity],
    ) -> Result<(ReaderQueryGuard, ReaderIoPermit), BifrostError> {
        let mut requested: BTreeMap<TableAuthorityIdentity, LocalReaderCut> = BTreeMap::new();
        for cut in prepared {
            let Some((identity, local)) = local_cut_from_prepared(cut)? else {
                continue;
            };
            // One query cannot hold two different cuts of one table: the cut is
            // immutable and per-table, so a repeat is the same snapshot.
            requested.entry(identity).or_insert(local);
        }
        self.protect(requested).await
    }

    /// Protects an already-derived cut set, for tests that need exact ancestry.
    ///
    /// Production callers reach [`OracleReaderAuthority::protect`] through a
    /// prepared catalog identity or a signed follower cut, both of which can
    /// only describe lineages a real table actually has. A test that must pin
    /// forked or deeply nested histories needs to name the cuts directly, so
    /// this is the one seam that accepts them — and it is compiled only under
    /// the test-support feature.
    ///
    /// # Errors
    ///
    /// Returns every failure [`OracleReaderAuthority::acquire_guard`] returns.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn acquire_guard_for_cuts(
        self: &Arc<Self>,
        cuts: Vec<(TableAuthorityIdentity, LocalReaderCut)>,
    ) -> Result<(ReaderQueryGuard, ReaderIoPermit), BifrostError> {
        self.protect(cuts.into_iter().collect()).await
    }

    /// Protects one follower fragment's assigned cuts under this node's epoch.
    ///
    /// A follower never resolves its own snapshot: the leader signed exactly
    /// which snapshot each scan reads, so the follower re-derives the same
    /// local cut from that signed evidence and protects it under its own epoch
    /// before it opens anything. The digest is recomputed here rather than
    /// trusted, so a tampered ancestry cannot widen what this node claims to
    /// have proven.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a signed cut's ancestry does not
    /// reproduce its own digest, and every failure
    /// [`OracleReaderAuthority::acquire_guard`] returns.
    pub async fn acquire_follower_guard(
        self: &Arc<Self>,
        cuts: &[(
            TableAuthorityIdentity,
            wyrd_spec::vala::api::FollowerReaderCut,
        )],
    ) -> Result<(ReaderQueryGuard, ReaderIoPermit), BifrostError> {
        let mut requested: BTreeMap<TableAuthorityIdentity, LocalReaderCut> = BTreeMap::new();
        for (identity, cut) in cuts {
            let Some(local) = local_cut_from_follower(identity, cut)? else {
                continue;
            };
            requested.entry(identity.clone()).or_insert(local);
        }
        self.protect(requested).await
    }

    /// Commits the durable protection one already-derived cut set requires.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when admission is closed, when the
    /// epoch is past its admission cutoff, when a table has no maintenance
    /// authority row, or when a required durable widening could not commit.
    /// Every failure leaves prior conservative protection in place.
    async fn protect(
        self: &Arc<Self>,
        requested: BTreeMap<TableAuthorityIdentity, LocalReaderCut>,
    ) -> Result<(ReaderQueryGuard, ReaderIoPermit), BifrostError> {
        let (no_io_deadline, admission_cutoff) = {
            let lifecycle = self.lifecycle.lock().await;
            (
                lifecycle.deadlines.no_io,
                lifecycle.deadlines.admission_cutoff,
            )
        };
        if !self.admits() {
            return Err(internal(
                "Oracle reader authority is no longer admitting queries",
            ));
        }
        if tokio::time::Instant::now() >= admission_cutoff {
            return Err(internal(
                "Oracle reader authority reached its epoch's admission cutoff",
            ));
        }
        // Reserved before any cut is published so a saturated queue can never
        // lose a release: a query that could not reserve is never admitted.
        let release = Arc::clone(&self.release_permits)
            .acquire_owned()
            .await
            .map_err(|_| internal("Oracle reader authority release queue is closed"))?;

        let mut coordinators = Vec::with_capacity(requested.len());
        for identity in requested.keys() {
            coordinators.push(self.coordinator(identity).await);
        }
        let mut guards: Vec<OwnedMutexGuard<OracleTableCoordinator>> =
            Vec::with_capacity(coordinators.len());
        for coordinator in coordinators {
            guards.push(coordinator.lock_owned().await);
        }

        let mut holdings = Vec::with_capacity(requested.len());
        let mut failure = None;
        for ((identity, local), guard) in requested.into_iter().zip(guards.iter_mut()) {
            let cut_id = guard.next_cut_id;
            guard.next_cut_id = guard.next_cut_id.wrapping_add(1);
            guard.active.insert(cut_id, local);
            if let Err(error) = self.ensure_covered(&identity, guard).await {
                guard.active.remove(&cut_id);
                failure = Some(error);
                break;
            }
            holdings.push((identity, cut_id));
        }
        drop(guards);

        // The guard is constructed even on failure so that dropping it enqueues
        // the exact narrowing for whichever tables did widen. A partially
        // admitted plan never leaves a pin nobody will remove.
        let query_cancel = CancellationToken::new();
        let guard = ReaderQueryGuard {
            authority: Arc::clone(self),
            holdings,
            release: Some(release),
            query_cancel: query_cancel.clone(),
        };
        if let Some(error) = failure {
            drop(guard);
            return Err(error);
        }

        Ok((
            guard,
            ReaderIoPermit::new(self.epoch_cancel.clone(), query_cancel, no_io_deadline),
        ))
    }

    /// Returns the coordinator for one table, creating it on first use.
    async fn coordinator(
        &self,
        identity: &TableAuthorityIdentity,
    ) -> Arc<Mutex<OracleTableCoordinator>> {
        let mut coordinators = self.coordinators.lock().await;
        Arc::clone(
            coordinators
                .entry(identity.clone())
                .or_insert_with(|| Arc::new(Mutex::new(OracleTableCoordinator::default()))),
        )
    }

    /// Commits a durable widening unless the confirmed frontier already covers.
    ///
    /// A covered admission performs zero SQL calls and zero mutations: coverage
    /// is decided entirely against the confirmed in-memory revision, which is
    /// the whole point of aggregating rather than publishing a row per query.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the required frontier cannot be
    /// computed, when the table has no maintenance authority row, or when the
    /// bounded compare-and-set could not commit. The coordinator keeps its
    /// prior confirmed revision on every failure.
    async fn ensure_covered(
        &self,
        identity: &TableAuthorityIdentity,
        coordinator: &mut OracleTableCoordinator,
    ) -> Result<(), BifrostError> {
        let required = frontier_from_active_cuts(identity, &coordinator.active)?;
        if let Some(confirmed) = coordinator.confirmed.as_ref()
            && covers_all(&confirmed.frontier, &required)
        {
            return Ok(());
        }
        let committed = self
            .commit_frontier(
                identity,
                coordinator.confirmed.as_ref().map(|record| record.revision),
                &required,
                OracleTableProtectionPhase::Expanded,
            )
            .await?;
        coordinator.confirmed = committed;
        Ok(())
    }

    /// Commits one table's next frontier revision, with one bounded retry.
    ///
    /// A revision mismatch returns the current complete record. The winner is
    /// adopted when it already covers this epoch's complete local requirement;
    /// otherwise exactly one further compare-and-set is attempted against the
    /// returned revision. A second mismatch fails admission before any IO
    /// rather than looping against a contended table.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the maintenance authority row is
    /// missing or identity-mismatched, when a statement or audit fails, or when
    /// the second attempt also conflicts.
    async fn commit_frontier(
        &self,
        identity: &TableAuthorityIdentity,
        expected_revision: Option<i64>,
        required: &ProtectionFrontier,
        phase: OracleTableProtectionPhase,
    ) -> Result<Option<ProtectionRecord>, BifrostError> {
        #[cfg(any(test, feature = "test-support"))]
        if self
            .protection_faults
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |remaining| remaining.checked_sub(1),
            )
            .is_ok()
        {
            return Err(internal("injected Oracle protection commit fault"));
        }
        let mut expected = expected_revision;
        for attempt in 0..2 {
            let mut conn = self
                .vala
                .tenant_conn(identity.tenant)
                .await
                .map_err(|error| internal(error.to_string()))?;
            BifrostTableMaintenanceAuthority::new(&mut conn)
                .lock(identity)
                .await
                .map_err(|error| internal(error.to_string()))?;
            let outcome = OracleTableProtections::new(&mut conn)
                .commit(
                    identity,
                    self.node_id,
                    self.fencing_token,
                    expected,
                    required,
                )
                .await
                .map_err(|error| internal(error.to_string()))?;
            match outcome {
                ProtectionCas::Committed(record) => {
                    append_protection_audit(
                        &mut conn,
                        identity,
                        self.node_id,
                        self.fencing_token,
                        phase,
                        record.revision,
                        required,
                    )
                    .await?;
                    conn.commit()
                        .await
                        .map_err(|error| internal(error.to_string()))?;
                    return Ok(if required.is_empty() {
                        None
                    } else {
                        Some(*record)
                    });
                }
                ProtectionCas::Conflict(current) => {
                    // Dropping the connection rolls the failed attempt back;
                    // `TenantConn` has no explicit rollback.
                    drop(conn);
                    if attempt == 1 {
                        return Err(internal(
                            "Oracle reader protection lost its bounded compare-and-set twice",
                        ));
                    }
                    if let Some(record) = current.as_deref()
                        && covers_all(&record.frontier, required)
                    {
                        return Ok(Some(record.clone()));
                    }
                    expected = current.as_deref().map(|record| record.revision);
                }
            }
        }
        Err(internal(
            "Oracle reader protection exhausted its bounded compare-and-set attempts",
        ))
    }
}

/// Reports whether `confirmed` already protects everything `required` needs.
///
/// Coverage is decided chain by chain against proven ancestry: every snapshot
/// on every required path must be named by some confirmed path. Timestamps and
/// snapshot-ID order take no part, because an unrelated lineage can carry both
/// a newer timestamp and a larger identifier while protecting nothing.
fn covers_all(confirmed: &ProtectionFrontier, required: &ProtectionFrontier) -> bool {
    required.members.iter().all(|member| {
        member
            .ancestry_path
            .iter()
            .all(|snapshot_id| confirmed.covers(*snapshot_id))
    })
}

impl OracleReaderAuthority {
    /// Drains reserved release commands until the sender closes.
    ///
    /// Narrowing is batched and delayed on purpose: excess retention is safe
    /// and protection expansion is not delayable, so the cheap side of the
    /// trade is the one that waits. Each command locks its tables in the same
    /// canonical order admission uses, removes only that query's pins, and
    /// commits the remaining frontier. A failed mutation keeps the prior
    /// confirmed frontier rather than advertising an unconfirmed narrowing.
    async fn run_narrowing(self: Arc<Self>, mut commands: mpsc::Receiver<NarrowingCommand>) {
        loop {
            tokio::select! {
                biased;
                command = commands.recv() => {
                    let Some(command) = command else { return };
                    self.apply_narrowing(command).await;
                }
                () = self.narrowing_drain.cancelled() => break,
            }
        }
        // Reached only after descendants are joined, so no sender can add
        // another command and what is queued here is the complete remainder.
        while let Ok(command) = commands.try_recv() {
            self.apply_narrowing(command).await;
        }
    }

    /// Applies one query's exact pin removals across its tables, with one
    /// bounded retry of whatever failed.
    ///
    /// A narrowing that fails leaves its cut pinned, so the same command is
    /// still exactly applicable. It is retried once — and only while this epoch
    /// still admits, because an epoch that has lost authority must not touch
    /// protection it no longer owns. A second failure keeps the wider durable
    /// set, which retirement or expiry recovery reclaims.
    async fn apply_narrowing(&self, command: NarrowingCommand) {
        let mut holdings = command.holdings;
        holdings.sort_by(|left, right| left.0.cmp(&right.0));
        let mut failed = Vec::new();
        for (identity, cut_id) in holdings {
            if let Err(error) = self.narrow_table(&identity, cut_id).await {
                tracing::warn!(
                    error = %error,
                    "Oracle kept its previous reader protection after a failed narrowing"
                );
                failed.push((identity, cut_id));
            }
        }
        if failed.is_empty() || !self.admits() {
            return;
        }
        for (identity, cut_id) in failed {
            if let Err(error) = self.narrow_table(&identity, cut_id).await {
                tracing::warn!(
                    error = %error,
                    "Oracle kept its previous reader protection after a retried narrowing"
                );
            }
        }
    }

    /// Removes one pin from one table and commits the remaining frontier.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the remaining frontier cannot be
    /// computed or its commit fails. The coordinator's confirmed revision is
    /// left untouched in both cases, so Forge keeps seeing the wider set.
    async fn narrow_table(
        &self,
        identity: &TableAuthorityIdentity,
        cut_id: u64,
    ) -> Result<(), BifrostError> {
        let coordinator = self.coordinator(identity).await;
        let mut guard = coordinator.lock_owned().await;
        if !guard.active.contains_key(&cut_id) {
            return Ok(());
        }
        // Computed before any local mutation, so a failed commit leaves this
        // cut exactly where it was: still pinned locally and still covered
        // durably, which is the only state a retry can start from.
        let remaining = frontier_from_cuts(
            identity,
            guard
                .active
                .iter()
                .filter(|(id, _)| **id != cut_id)
                .map(|(_, cut)| cut)
                .collect(),
        )?;
        let Some(confirmed) = guard.confirmed.as_ref() else {
            guard.active.remove(&cut_id);
            return Ok(());
        };
        // A cut whose removal changes nothing durable — a duplicate of what a
        // concurrent query still pins — is removed locally and costs no
        // statement, no compare-and-set, and no audit row.
        if !remaining.is_empty() && confirmed.frontier == remaining {
            guard.active.remove(&cut_id);
            return Ok(());
        }
        let phase = if remaining.is_empty() {
            OracleTableProtectionPhase::Released
        } else {
            OracleTableProtectionPhase::Narrowed
        };
        let expected = confirmed.revision;
        let record = self
            .commit_frontier(identity, Some(expected), &remaining, phase)
            .await?;
        guard.active.remove(&cut_id);
        guard.confirmed = record;
        Ok(())
    }

    /// Closes admission, drains the narrowing queue, and releases every table.
    ///
    /// Ordering is the point. Admission closes first, then the queue is drained
    /// so no reserved release is lost, then each remaining table is released in
    /// canonical order, then the audited `invalidated` edge commits, and only
    /// then is the epoch row deleted — and only after Postgres proves no header
    /// remains for this exact epoch. A failed table release retains the epoch
    /// and every remaining protection for retry.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a release, the invalidation
    /// edge, or retirement fails. Protection is retained on every failure.
    pub async fn retire(self: &Arc<Self>) -> Result<(), BifrostError> {
        self.settle_lease_loss().await?;
        self.epoch_cancel.cancel();
        // Ordered, not incidental: descendants are joined before the narrowing
        // worker is stopped, and both happen before the first table release, so
        // no table is released while a reader of it can still be running and no
        // reserved narrowing is abandoned unapplied.
        if !self.join_descendants().await {
            tracing::error!(
                node_id = %self.node_id,
                fencing_token = self.fencing_token,
                "Oracle retired its reader epoch without joining every descendant"
            );
            self.terminator
                .terminate("Oracle reader epoch descendants outlived retirement's join budget");
        }
        self.stop_narrowing_worker().await;

        let identities: Vec<TableAuthorityIdentity> =
            self.coordinators.lock().await.keys().cloned().collect();
        for identity in identities {
            let coordinator = self.coordinator(&identity).await;
            let mut guard = coordinator.lock_owned().await;
            guard.active.clear();
            let Some(confirmed) = guard.confirmed.as_ref() else {
                continue;
            };
            let expected = confirmed.revision;
            let record = self
                .commit_frontier(
                    &identity,
                    Some(expected),
                    &ProtectionFrontier::default(),
                    OracleTableProtectionPhase::Released,
                )
                .await?;
            guard.confirmed = record;
        }

        // `commit_loss_edge` already committed `draining` for an epoch that
        // activated, or `invalidated` for one that never did. Only the former
        // still owes the invalidation edge.
        let confirmed_revision = self.lifecycle.lock().await.confirmed_revision;
        let invalidated = if self.epoch_activated().await? {
            self.commit_epoch_transition(confirmed_revision, OracleEpochState::Invalidated)
                .await?
        } else {
            confirmed_revision
        };
        self.lifecycle.lock().await.confirmed_revision = invalidated;

        let mut conn = system_conn(&self.vala).await?;
        OracleReaderEpochs::new(&mut conn)
            .map_err(|error| internal(error.to_string()))?
            .retire(self.node_id, self.fencing_token, invalidated)
            .await
            .map_err(|error| internal(error.to_string()))?;
        append_epoch_audit(
            &mut conn,
            self.node_id,
            self.fencing_token,
            OracleReaderEpochPhase::Retired,
            invalidated,
        )
        .await?;
        conn.commit()
            .await
            .map_err(|error| internal(error.to_string()))?;
        Ok(())
    }

    /// Closes the narrowing queue, then joins both of this epoch's workers.
    ///
    /// The narrowing worker is drained rather than cancelled, because its
    /// queue may still hold reserved releases that must reach Postgres. The
    /// lease supervisor is not touched here: it was already joined while
    /// retirement resolved who owns this epoch's loss.
    async fn stop_narrowing_worker(&self) {
        let Some(narrowing) = self.narrowing_worker.lock().await.take() else {
            return;
        };
        self.narrowing_drain.cancel();
        if let Err(error) = narrowing.await
            && !error.is_cancelled()
        {
            tracing::error!(
                node_id = %self.node_id,
                "Oracle reader epoch narrowing worker did not terminate cleanly"
            );
        }
    }

    /// Waits for the lease supervisor to finish, without cancelling or
    /// aborting it.
    ///
    /// A supervisor that selected this epoch's loss is in the middle of the
    /// audited loss edge; aborting it would abandon that transaction and leave
    /// the durable epoch behind for lease expiry to reclaim.
    async fn join_lease_worker(&self) {
        let Some(lease) = self.lease_worker.lock().await.take() else {
            return;
        };
        if let Err(error) = lease.await
            && !error.is_cancelled()
        {
            tracing::error!(
                node_id = %self.node_id,
                "Oracle reader epoch lease supervisor did not terminate cleanly"
            );
        }
    }

    /// Resolves which owner committed this epoch's loss edge before retirement
    /// releases anything.
    ///
    /// Retirement and the lease supervisor can both reach loss; exactly one
    /// selects it, and only that one owes the audited edge. When retirement
    /// wins it stops the renewal cadence, joins the supervisor, and commits
    /// the edge itself. When the supervisor won, retirement waits for it and
    /// then verifies the durable epoch actually reached a loss state, because
    /// releasing protection under an epoch that never recorded its loss would
    /// leave nothing to reclaim it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the loss edge cannot be
    /// committed, when the epoch row cannot be read, or when it is still in a
    /// state that has not lost authority.
    async fn settle_lease_loss(&self) -> Result<(), BifrostError> {
        if self.select_loss().await {
            self.renewal_cancel.cancel();
            self.join_lease_worker().await;
            return self.commit_loss_edge().await;
        }
        self.join_lease_worker().await;
        let mut conn = system_conn(&self.vala).await?;
        let row = OracleReaderEpochs::new(&mut conn)
            .map_err(|error| internal(error.to_string()))?
            .read(self.node_id, self.fencing_token)
            .await
            .map_err(|error| internal(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| internal(error.to_string()))?;
        let row = row.ok_or_else(|| internal("Oracle reader epoch row disappeared"))?;
        if !matches!(
            row.state,
            OracleEpochState::Draining | OracleEpochState::Invalidated
        ) {
            return Err(internal(
                "Oracle reader epoch loss was selected but never durably recorded",
            ));
        }
        let mut lifecycle = self.lifecycle.lock().await;
        lifecycle.confirmed_revision = row.state_revision;
        Ok(())
    }

    /// Borrows the read-only pool reader recovery enumerates through.
    #[must_use]
    pub fn operator_pool(&self) -> &vala_sql::OperatorPool {
        &self.operator_pool
    }

    /// Renews the lease on a fixed cadence and self-fences when it cannot.
    ///
    /// This is the only writer of the epoch's authority window while the Oracle
    /// serves reads. It self-fences on two independent grounds: a renewal that
    /// Postgres refused, and a local monotonic clock that has reached the
    /// admission cutoff derived from the last confirmed lease. The second
    /// matters because a supervisor that is merely starved never learns the
    /// first, and the cutoff rather than the no-IO deadline because admission
    /// must close while there is still time to stop reads that already began.
    ///
    /// It returns without self-fencing when the process is shutting down or
    /// when retirement cancelled renewal, because in both of those cases the
    /// caller that cancelled owns this epoch's loss.
    async fn supervise_lease(self: Arc<Self>, shutdown: CancellationToken) {
        loop {
            let admission_cutoff = self.deadlines().await.admission_cutoff;
            // Re-checked before the select so a cutoff that has already passed
            // self-fences immediately instead of racing the renewal cadence.
            if tokio::time::Instant::now() < admission_cutoff {
                tokio::select! {
                    () = shutdown.cancelled() => return,
                    () = self.renewal_cancel.cancelled() => return,
                    () = tokio::time::sleep_until(admission_cutoff) => {}
                    () = tokio::time::sleep(EPOCH_RENEWAL_INTERVAL) => {
                        if let Err(error) = self.renew().await {
                            tracing::error!(
                                error = %error,
                                node_id = %self.node_id,
                                fencing_token = self.fencing_token,
                                "Oracle self-fenced its reader epoch after a failed renewal"
                            );
                            self.self_fence().await;
                            return;
                        }
                        continue;
                    }
                }
            }
            {
                tracing::error!(
                    node_id = %self.node_id,
                    fencing_token = self.fencing_token,
                    "Oracle self-fenced its reader epoch at its admission cutoff"
                );
                self.self_fence().await;
                return;
            }
        }
    }

    /// Closes admission, commits the loss edge, cancels, and joins descendants.
    ///
    /// Nothing here is skipped on failure: an epoch that cannot commit its loss
    /// edge still cancels, because this process must stop reading under a lease
    /// it no longer holds regardless of what Postgres accepted.
    pub async fn self_fence(self: &Arc<Self>) {
        if !self.select_loss().await {
            return;
        }
        if let Err(error) = self.commit_loss_edge().await {
            tracing::error!(error = %error, "Oracle could not record its reader epoch loss");
        }
        self.epoch_cancel.cancel();
        if !self.join_descendants().await {
            tracing::error!(
                node_id = %self.node_id,
                fencing_token = self.fencing_token,
                "Oracle could not join its reader epoch descendants within the join budget"
            );
            self.terminator
                .terminate("Oracle reader epoch descendants outlived the join budget");
        }
    }

    /// Waits for every admitted query's reservation to come back.
    ///
    /// A returned reservation is proof that the query's guard dropped, which is
    /// exactly the descendant this epoch had to join. Reporting `false` means
    /// this process cannot prove its readers stopped, which is the one
    /// condition the terminator exists for.
    async fn join_descendants(&self) -> bool {
        let capacity = u32::try_from(self.release_capacity).unwrap_or(u32::MAX);
        tokio::time::timeout(
            EPOCH_JOIN_BUDGET,
            self.release_permits.acquire_many(capacity),
        )
        .await
        .is_ok_and(|permit| permit.is_ok())
    }
}

/// Read-only enumeration and cleanup of epochs whose lease already expired.
///
/// Recovery is deliberately not a method on a live authority: the epochs it
/// removes belong to processes that are gone, and the only capability it needs
/// is a cross-tenant read plus the system-tenant writes that invalidate and
/// retire. Keeping it separate means a crashed node's protection is reclaimable
/// by any Oracle without that Oracle pretending to own the dead node's lease.
pub struct OracleEpochRecovery {
    /// Read-only cross-tenant pool the expired-epoch enumeration reads through.
    operator_pool: vala_sql::OperatorPool,
    /// Tenant-scoped SQL handle every reclamation transaction opens from.
    vala: vala_sql::ValaPostgres,
}

impl std::fmt::Debug for OracleEpochRecovery {
    /// Prints the owner's name without any pool or connection detail.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OracleEpochRecovery")
            .finish_non_exhaustive()
    }
}

impl OracleEpochRecovery {
    /// Builds a recovery owner over the pools it reclaims through.
    #[must_use]
    pub fn new(operator_pool: vala_sql::OperatorPool, vala: vala_sql::ValaPostgres) -> Self {
        Self {
            operator_pool,
            vala,
        }
    }

    /// Reclaims up to `limit` epochs whose database lease already expired.
    ///
    /// Each epoch is invalidated under a predicate Postgres itself evaluates
    /// against `statement_timestamp()`, so a live epoch is never reclaimed by a
    /// recovering peer's opinion of the time. Only after invalidation are that
    /// epoch's per-table protections released and its row retired. A failure on
    /// one epoch leaves it and its protections intact for the next sweep.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the enumeration itself fails. A
    /// per-epoch failure is logged and skipped rather than aborting the sweep,
    /// since one unreclaimable epoch must not block the others.
    pub async fn reclaim_expired(&self, limit: i64) -> Result<usize, BifrostError> {
        let expired = vala_sql::queries::oracle_reader_authority::list_expired_epochs_for_operator(
            &self.operator_pool,
            limit,
        )
        .await
        .map_err(|error| internal(error.to_string()))?;
        let mut reclaimed = 0;
        for epoch in expired {
            // An expired epoch this node cannot reclaim still holds protection,
            // so the sweep reports the failure instead of counting past it. A
            // caller that starts serving anyway would publish readiness over
            // retention nothing released.
            self.reclaim(epoch.node_id, epoch.fencing_token)
                .await
                .map_err(|error| {
                    internal(format!(
                        "Oracle epoch recovery could not reclaim expired epoch                          {}/{}: {error}",
                        epoch.node_id, epoch.fencing_token
                    ))
                })?;
            reclaimed += 1;
        }
        Ok(reclaimed)
    }

    /// Invalidates one expired epoch, releases its tables, and retires it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the epoch is no longer expired,
    /// when a protection release fails, or when retirement fails. Protection is
    /// retained on every failure.
    async fn reclaim(&self, node_id: uuid::Uuid, fencing_token: i64) -> Result<(), BifrostError> {
        let revision = {
            let mut conn = system_conn(&self.vala).await?;
            let revision = OracleReaderEpochs::new(&mut conn)
                .map_err(|error| internal(error.to_string()))?
                .invalidate_expired(node_id, fencing_token)
                .await
                .map_err(|error| internal(error.to_string()))?
                .ok_or_else(|| {
                    internal("Oracle reader epoch is no longer expired and keeps its authority")
                })?;
            append_epoch_audit(
                &mut conn,
                node_id,
                fencing_token,
                OracleReaderEpochPhase::Invalidated,
                revision,
            )
            .await?;
            conn.commit()
                .await
                .map_err(|error| internal(error.to_string()))?;
            revision
        };

        let keys = vala_sql::queries::oracle_reader_authority::
            enumerate_epoch_protection_keys_for_operator(
                &self.operator_pool,
                node_id,
                fencing_token,
            )
            .await
            .map_err(|error| internal(error.to_string()))?;
        for key in keys {
            self.release_table(&key, node_id, fencing_token).await?;
        }

        let mut conn = system_conn(&self.vala).await?;
        OracleReaderEpochs::new(&mut conn)
            .map_err(|error| internal(error.to_string()))?
            .retire(node_id, fencing_token, revision)
            .await
            .map_err(|error| internal(error.to_string()))?;
        append_epoch_audit(
            &mut conn,
            node_id,
            fencing_token,
            OracleReaderEpochPhase::Retired,
            revision,
        )
        .await?;
        conn.commit()
            .await
            .map_err(|error| internal(error.to_string()))?;
        Ok(())
    }

    /// Releases one dead epoch's protection on one table under its authority row.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the maintenance authority row is
    /// missing, when the release compare-and-set loses, or when the transaction
    /// or its audit fails. The protection survives every failure.
    async fn release_table(
        &self,
        key: &vala_sql::row_types::oracle_reader_authority::ProtectionKey,
        node_id: uuid::Uuid,
        fencing_token: i64,
    ) -> Result<(), BifrostError> {
        let mut conn = self
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(|error| internal(error.to_string()))?;
        // The identity payload is read before any lock so the canonical
        // authority-row-then-protection order still holds.
        let Some((identity, revision)) = OracleTableProtections::new(&mut conn)
            .read_key_identity(key)
            .await
            .map_err(|error| internal(error.to_string()))?
        else {
            return Ok(());
        };
        BifrostTableMaintenanceAuthority::new(&mut conn)
            .lock(&identity)
            .await
            .map_err(|error| internal(error.to_string()))?;
        let outcome = OracleTableProtections::new(&mut conn)
            .commit(
                &identity,
                node_id,
                fencing_token,
                Some(revision),
                &ProtectionFrontier::default(),
            )
            .await
            .map_err(|error| internal(error.to_string()))?;
        if matches!(outcome, ProtectionCas::Conflict(_)) {
            return Err(internal(
                "Oracle epoch recovery lost the release compare-and-set",
            ));
        }
        append_protection_audit(
            &mut conn,
            &identity,
            node_id,
            fencing_token,
            OracleTableProtectionPhase::Released,
            revision,
            &ProtectionFrontier::default(),
        )
        .await?;
        conn.commit()
            .await
            .map_err(|error| internal(error.to_string()))?;
        Ok(())
    }
}

/// Builds the signed reader cut one follower must protect before it reads.
///
/// A follower protects exactly the snapshot it was assigned, so the cut's
/// retained head and protected endpoint are the same snapshot and its ancestry
/// path is that single entry. Signing the digest here binds the cut to the
/// table's durable UID and tenant, so a peer cannot present the same snapshot
/// identifier under a different table's identity.
///
/// Returns `None` when the pinned table has no Iceberg snapshot: there is
/// nothing snapshot-dependent for the follower to protect.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when the pinned snapshot is absent from
/// its own metadata, which would leave the cut undatable.
pub fn follower_reader_cut(
    pinned: &PinnedSealedTable,
    planned_under_fence: u64,
) -> Result<Option<wyrd_spec::vala::api::FollowerReaderCut>, BifrostError> {
    let Some(snapshot_id) = pinned.snapshot_id else {
        return Ok(None);
    };
    let metadata = pinned.iceberg_table.metadata();
    let snapshot = metadata.snapshot_by_id(snapshot_id).ok_or_else(|| {
        internal(format!(
            "Oracle pinned snapshot {snapshot_id} is absent from its own table metadata"
        ))
    })?;
    let identity = TableAuthorityIdentity {
        tenant: pinned.binding.tenant,
        table_uid: *pinned.table_uid.as_bytes(),
        catalog_name: BIFROST_CATALOG_NAME.to_owned(),
        namespace_name: pinned.binding.table_ref.namespace.as_str().to_owned(),
        table_name: pinned.binding.table_ref.name.clone(),
    };
    let timestamp_ms = snapshot.timestamp_ms();
    let member = ProtectionMember::new(&identity, vec![snapshot_id], timestamp_ms, timestamp_ms)
        .map_err(|error| internal(error.to_string()))?;
    let mut ancestry_digest_hex = String::with_capacity(64);
    for byte in member.ancestry_digest {
        use std::fmt::Write as _;
        let _ = write!(ancestry_digest_hex, "{byte:02x}");
    }
    Ok(Some(wyrd_spec::vala::api::FollowerReaderCut {
        table_uid: uuid::Uuid::from_bytes(*pinned.table_uid.as_bytes()),
        snapshot_id,
        snapshot_timestamp_ms: timestamp_ms,
        retained_head_snapshot_id: snapshot_id,
        ancestry_path: vec![snapshot_id],
        ancestry_digest_version: u32::try_from(
            vala_sql::row_types::oracle_reader_authority::ANCESTRY_DIGEST_VERSION,
        )
        .unwrap_or(1),
        ancestry_digest_hex,
        target_epoch_fence: planned_under_fence,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vala_sql::row_types::oracle_reader_authority::ProtectionRecord;

    /// Builds one fixed identity every vector in this module is bound to.
    fn identity() -> TableAuthorityIdentity {
        TableAuthorityIdentity {
            tenant: DataTenantId::SYSTEM_OWNER,
            table_uid: [3; 16],
            catalog_name: BIFROST_CATALOG_NAME.to_owned(),
            namespace_name: "vala.bifrost".to_owned(),
            table_name: "events".to_owned(),
        }
    }

    /// Builds one local cut with an explicit ancestry.
    fn cut(snapshot_id: i64, timestamp_ms: i64, ancestry: &[i64]) -> LocalReaderCut {
        LocalReaderCut {
            snapshot_id,
            timestamp_ms,
            ancestry_path: ancestry.to_vec(),
        }
    }

    /// Collects active cuts into the map the reducer consumes.
    fn active(cuts: Vec<LocalReaderCut>) -> BTreeMap<u64, LocalReaderCut> {
        cuts.into_iter()
            .enumerate()
            .map(|(index, cut)| (index as u64, cut))
            .collect()
    }

    #[test]
    fn frontier_encoding_and_corruption_fail_closed() {
        let id = identity();

        // Comparable cuts collapse to one chain from newest head to oldest cut.
        let comparable = frontier_from_active_cuts(
            &id,
            &active(vec![
                cut(30, 300, &[30, 20, 10]),
                cut(10, 100, &[10]),
                cut(20, 200, &[20, 10]),
            ]),
        )
        .expect("comparable cuts reduce");
        assert_eq!(comparable.members.len(), 1);
        let member = &comparable.members[0];
        assert_eq!(member.retained_head_snapshot_id, 30);
        assert_eq!(member.protected_snapshot_id, 10);
        assert_eq!(member.ancestry_path, vec![30, 20, 10]);
        assert_eq!(member.retained_head_timestamp_ms, 300);
        assert_eq!(member.protected_snapshot_timestamp_ms, 100);

        // Incomparable lineages survive as separate members: no single snapshot
        // covers both, so collapsing them would stop protecting one.
        let forked = frontier_from_active_cuts(
            &id,
            &active(vec![cut(41, 410, &[41, 10]), cut(52, 520, &[52, 11])]),
        )
        .expect("forked cuts reduce");
        assert_eq!(forked.members.len(), 2);
        assert!(forked.covers(41));
        assert!(forked.covers(52));
        // A cut protects the snapshot it reads, not its whole history: the
        // ancestors are lineage evidence, and retaining them is Forge's
        // decision, not something one reader silently pins.
        assert!(!forked.covers(10));
        assert!(!forked.covers(12));

        // A singleton chain has the same head and protected endpoint.
        let singleton =
            frontier_from_active_cuts(&id, &active(vec![cut(7, 70, &[7])])).expect("singleton");
        assert_eq!(singleton.members.len(), 1);
        assert_eq!(singleton.members[0].retained_head_snapshot_id, 7);
        assert_eq!(singleton.members[0].protected_snapshot_id, 7);

        // An empty active set is a release, not an absence of evidence.
        assert!(
            frontier_from_active_cuts(&id, &active(Vec::new()))
                .expect("empty reduces")
                .is_empty()
        );

        // Every reduced member reproduces its own digest under this identity
        // and fails under any other table's.
        let mut other = id.clone();
        other.table_uid = [4; 16];
        for member in &comparable.members {
            assert!(member.validate(&id).is_ok());
            assert!(member.validate(&other).is_err());
        }

        // Coverage is decided from proven ancestry, never snapshot-ID order: a
        // larger identifier on an unrelated lineage protects nothing.
        assert!(covers_all(&comparable, &comparable));
        assert!(covers_all(&forked, &forked));
        // 30's chain reaches 10, so it already covers a lone cut at 20.
        let lone_twenty =
            frontier_from_active_cuts(&id, &active(vec![cut(20, 200, &[20, 10])])).expect("lone");
        assert!(covers_all(&comparable, &lone_twenty));
        // Nothing on 30's chain reaches the unrelated snapshot 7.
        assert!(!covers_all(&comparable, &singleton));
        assert!(!covers_all(&singleton, &comparable));

        // A record whose header digest does not reproduce is corruption.
        let record = ProtectionRecord {
            revision: 1,
            frontier_encoding_version: 1,
            frontier_digest: comparable.digest(&id),
            updated_at: chrono::Utc::now(),
            frontier: comparable.clone(),
        };
        assert!(record.validate(&id).is_ok());
        let mut corrupt = record;
        corrupt.frontier_digest[0] ^= 0xff;
        assert!(corrupt.validate(&id).is_err());

        // A malformed ancestry is rejected rather than silently truncated.
        assert!(frontier_from_active_cuts(&id, &active(vec![cut(9, 90, &[])])).is_err());
    }

    #[test]
    fn epoch_deadlines_apply_the_fixed_database_time_allowance() {
        let before = tokio::time::Instant::now();
        let now = chrono::Utc::now();
        let deadlines = EpochDeadlines::from_sample(
            before,
            OracleLeaseSample {
                database_now: now,
                lease_expires_at: now + chrono::Duration::seconds(30),
                state_revision: 1,
            },
        )
        .expect("a full lease converts");
        assert_eq!(deadlines.no_io, before + std::time::Duration::from_secs(28));
        assert_eq!(
            deadlines.admission_cutoff,
            before + std::time::Duration::from_secs(20)
        );
        assert_eq!(deadlines.join, before + std::time::Duration::from_secs(24));

        // A remainder already inside the self-fence margin is immediate loss.
        assert!(
            EpochDeadlines::from_sample(
                before,
                OracleLeaseSample {
                    database_now: now,
                    lease_expires_at: now + chrono::Duration::seconds(9),
                    state_revision: 2,
                },
            )
            .is_err()
        );
        assert!(
            EpochDeadlines::from_sample(
                before,
                OracleLeaseSample {
                    database_now: now,
                    lease_expires_at: now - chrono::Duration::seconds(1),
                    state_revision: 3,
                },
            )
            .is_err()
        );
    }
}
