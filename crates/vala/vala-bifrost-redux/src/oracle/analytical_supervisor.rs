//! Attempt-scoped supervision for Oracle's inactive Analytical execution path.
//!
//! A distributed graph fans work out across followers, so nothing about its
//! cleanup is implied by a leader stream ending. This module owns the one place
//! that knows an attempt exists: it registers the attempt's query-owned runtime,
//! splits the attempt's exchange and scratch children from the grant admission
//! already charged, hands out a cancellation child, retains every driver future
//! started under that attempt, and releases all of it exactly once on every
//! terminal path — success, retry, cancellation, deadline, error, or shutdown.
//!
//! Two properties are structural rather than conventional:
//!
//! * **Two-identity keying.** Every attempt is keyed by both the client-visible
//!   [`PublicQueryId`] and the private [`DataFusionQueryId`]. A sibling graph
//!   planned under the same public query — a retry's fresh plan, or a second
//!   distributed attempt — cannot settle, cancel, or resolve this graph's
//!   attempts, because its key simply is not equal.
//! * **Release-once.** Attempt state lives in the supervisor's map and settling
//!   removes it under the lock before any resource is dropped. A second settle
//!   of the same key finds nothing and refuses, so no child reservation is
//!   returned twice and no gauge is decremented twice.
//!
//! The supervisor is inactive in T1: production Oracle routing still selects the
//! Interactive path, and only [`super::analytical`]'s inactive handle and the
//! crate's tests construct one.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wyrd_spec::vala::BifrostError;

use super::analytical::{
    AnalyticalAttemptNumber, AnalyticalGraphKey, AnalyticalGraphRuntime, AnalyticalRuntimeRegistry,
    DataFusionQueryId, PublicQueryId,
};
use super::telemetry::{AnalyticalAttemptOutcome, AnalyticalAttemptTelemetry};
use crate::resources::{OracleQueryResources, OracleQueryScratchReservation};

/// A graph-local stage ordinal.
///
/// Stage numbering restarts per distributed graph, so a stage identifier is
/// meaningless without its [`AnalyticalGraphKey`]. This newtype exists to make
/// that dependence unavoidable at every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StageId(usize);

impl StageId {
    /// Adopts a graph-local stage ordinal.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the ordinal for wire binding, evidence, and scrubbed traces.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl fmt::Display for StageId {
    /// Renders the bare ordinal for scrubbed traces and evidence.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// A stage-local task ordinal.
///
/// Task numbering restarts per stage, so like [`StageId`] it is only meaningful
/// beneath its graph. `None` in an attempt key denotes stage-scoped work — plan
/// installation — that precedes any individual task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(usize);

impl TaskId {
    /// Adopts a stage-local task ordinal.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the ordinal for wire binding, evidence, and scrubbed traces.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl fmt::Display for TaskId {
    /// Renders the bare ordinal for scrubbed traces and evidence.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// The exact identity of one supervised unit of Analytical work.
///
/// The key is both parents plus the graph-local coordinates and the attempt
/// ordinal. Equality is therefore the complete authorization-relevant identity:
/// there is no prefix of this key under which two different graphs, stages,
/// tasks, or attempts collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnalyticalAttemptKey {
    /// The client-visible query the attempt serves.
    pub public_query_id: PublicQueryId,
    /// The private distributed plan identity the attempt belongs to.
    pub datafusion_query_id: DataFusionQueryId,
    /// The graph-local stage the attempt executes.
    pub stage: StageId,
    /// The stage-local task, or `None` for stage-scoped plan installation.
    pub task: Option<TaskId>,
    /// The attempt ordinal within the graph.
    pub attempt: AnalyticalAttemptNumber,
}

impl AnalyticalAttemptKey {
    /// Names one attempt by both query identities and its graph coordinates.
    #[must_use]
    pub const fn new(
        public_query_id: PublicQueryId,
        datafusion_query_id: DataFusionQueryId,
        stage: StageId,
        task: Option<TaskId>,
        attempt: AnalyticalAttemptNumber,
    ) -> Self {
        Self {
            public_query_id,
            datafusion_query_id,
            stage,
            task,
            attempt,
        }
    }

    /// Projects the two-identity graph this attempt belongs to.
    #[must_use]
    pub const fn graph(&self) -> AnalyticalGraphKey {
        AnalyticalGraphKey {
            public_query_id: self.public_query_id,
            datafusion_query_id: self.datafusion_query_id,
        }
    }

    /// Projects the attempt-independent slot this attempt occupies.
    ///
    /// Two attempts of the same work share a slot. The supervisor admits at most
    /// one live attempt per slot, which is what forces a retry to wait for its
    /// predecessor to drain rather than run beside it.
    const fn slot(&self) -> AnalyticalAttemptSlot {
        AnalyticalAttemptSlot {
            graph: self.graph(),
            stage: self.stage,
            task: self.task,
        }
    }
}

/// The attempt-independent identity of one unit of supervised work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AnalyticalAttemptSlot {
    /// The two-identity graph the work belongs to.
    graph: AnalyticalGraphKey,
    /// The graph-local stage.
    stage: StageId,
    /// The stage-local task, or `None` for stage-scoped work.
    task: Option<TaskId>,
}

/// The exact child grant one attempt splits from the admitted query envelope.
///
/// Scratch is a child of capacity admission already charged for this query.
/// Splitting it never admits new root capacity, so an attempt that cannot fit
/// inside its own query's envelope is refused rather than growing it. Memory is
/// deliberately absent: operators and exchanges allocate from the query's one
/// installed pool, so a second per-attempt memory ceiling would only be able to
/// refuse work the pool already governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyticalAttemptGrant {
    /// Bytes of the query's admitted scratch share the attempt may spill into.
    pub scratch_bytes: u64,
}

/// Everything one live distributed graph owns for the whole plan's life.
///
/// The admitted query envelope lives here rather than at a call site because a
/// follower learns about a graph on one stage operation and executes its tasks
/// on later, separate ones. Nothing but this owner keeps the envelope alive
/// between them, so releasing the graph is the single act that returns it.
struct AnalyticalGraphState {
    /// The admitted query envelope every attempt of this graph splits from.
    resources: OracleQueryResources,
    /// The query's admission permit, held for exactly as long as the graph is.
    ///
    /// Declared after `resources` on purpose: removing the graph drops the
    /// envelope first and the permit second, so admission counters never wake a
    /// queued waiter while this query's envelope is still charged. A graph that
    /// is retained as `Draining` therefore keeps both, which is what makes a
    /// failed cleanup visible as charged capacity rather than as a free slot.
    retained_admission: Option<super::admission::AdmittedQueryGuard>,
    /// The query-owned runtime every follower of this graph installs.
    runtime: AnalyticalGraphRuntime,
    /// Every outbound exchange stream this graph opened, owned by the graph.
    ///
    /// Shared by the leader session that opens them and the lease that settles
    /// the graph, so one graph has exactly one place its exchanges live.
    exchanges: Arc<super::analytical_transport::AnalyticalGraphExchanges>,
    /// Cancellation child covering every descendant of this graph.
    ///
    /// Owned by the graph rather than by each attempt because a graph's
    /// outbound exchanges outlive the attempt that opened them: upstream keeps
    /// its reader task, and the buffers it charges to this envelope, alive
    /// until every partition stream it handed out is dropped. Both the leader
    /// session that opens those exchanges and the lease that settles the graph
    /// bind to this one token.
    cancel: CancellationToken,
    /// The physical-metric fold this graph settles inside its own deadline.
    ///
    /// Registered by the leader session once its plan has started, taken once
    /// by the lifecycle task. A graph that never distributed has none.
    metric_fold: Option<super::analytical::AnalyticalGraphMetricFold>,
    /// The leader lifecycle task supervising this graph, when it has one.
    ///
    /// A follower graph has none: it is settled by the coordinator call that
    /// activated it. A leader graph attaches one at registration, and the
    /// supervisor — never a caller — owns its join handle, which is what keeps
    /// the task joinable after the query stream that signalled it is gone.
    lifecycle: Option<AnalyticalGraphLifecycleOwner>,
}

/// The supervisor-side half of one leader graph's lifecycle task.
///
/// Held in the graph entry rather than by the attempt, so a caller that drops
/// its stream signals settlement without being able to take, abort, or await
/// the task. Only shutdown takes the handle, and only after signalling.
pub struct AnalyticalGraphLifecycleOwner {
    /// Control channel the graph's terminal is signalled through.
    control: tokio::sync::watch::Sender<super::analytical::AnalyticalGraphControl>,
    /// Clonable result channel every caller awaits settlement on.
    result: tokio::sync::watch::Receiver<super::analytical::AnalyticalGraphResult>,
    /// The task itself, taken only by shutdown.
    handle: Option<JoinHandle<()>>,
}

impl fmt::Debug for AnalyticalGraphLifecycleOwner {
    /// Reports whether the task is still joinable without rendering channels.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalGraphLifecycleOwner")
            .field("joinable", &self.handle.is_some())
            .finish_non_exhaustive()
    }
}

impl AnalyticalGraphLifecycleOwner {
    /// Composes the supervisor's half of one started lifecycle task.
    #[must_use]
    pub fn new(
        control: tokio::sync::watch::Sender<super::analytical::AnalyticalGraphControl>,
        result: tokio::sync::watch::Receiver<super::analytical::AnalyticalGraphResult>,
        handle: JoinHandle<()>,
    ) -> Self {
        Self {
            control,
            result,
            handle: Some(handle),
        }
    }
}

/// One registered graph's exact lifecycle state in the supervisor's map.
///
/// The follower ingress already distinguishes these two states; the leader now
/// does too, because a graph whose cleanup has begun must keep every owner it
/// still holds while refusing new work. Collapsing them into one map value is
/// what previously let a settling graph look identical to a live one.
enum AnalyticalGraphEntry {
    /// The graph is live: it admits attempts and accepts a terminal signal.
    Active(AnalyticalGraphState),
    /// A terminal was signalled; the graph admits nothing and is settling.
    Draining {
        /// Everything the graph still owns until its cleanup succeeds.
        state: AnalyticalGraphState,
        /// Why cleanup could not be confirmed, when it could not. `Some`
        /// fails readiness and keeps the residue attributable to this node.
        settlement_failure: Option<String>,
    },
}

impl AnalyticalGraphEntry {
    /// Borrows the graph's owned state in either lifecycle state.
    const fn state(&self) -> &AnalyticalGraphState {
        match self {
            Self::Active(state) | Self::Draining { state, .. } => state,
        }
    }

    /// Mutably borrows the graph's owned state in either lifecycle state.
    const fn state_mut(&mut self) -> &mut AnalyticalGraphState {
        match self {
            Self::Active(state) | Self::Draining { state, .. } => state,
        }
    }
}

/// Everything one live attempt owns and must return exactly once.
struct AnalyticalAttemptState {
    /// Cancellation child covering every descendant started under the attempt.
    cancel: CancellationToken,
    /// Driver futures retained so settlement can join rather than orphan them.
    drivers: Vec<JoinHandle<()>>,
    /// The attempt's scratch child of the query scratch envelope.
    scratch: OracleQueryScratchReservation,
    /// In-flight gauge and duration accounting released on the terminal path.
    telemetry: AnalyticalAttemptTelemetry,
    /// Exact grant sizes retained so release evidence can restate them.
    grant: AnalyticalAttemptGrant,
    /// Whether result data produced under this attempt already left the node.
    ///
    /// Shared with the attempt's guard so the fence survives into settlement
    /// evidence: a retry admitted after egress would re-emit rows a client has
    /// already seen, so the successor is refused rather than merely discouraged.
    egressed: Arc<AtomicBool>,
}

/// Proof that one attempt released every resource it held.
///
/// Terminal-cleanup evidence compares these values against the grant the
/// attempt was admitted with, so a partial release is visible as a mismatch
/// rather than only as a later poisoned governor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyticalAttemptRelease {
    /// The attempt that settled.
    pub key: AnalyticalAttemptKey,
    /// The terminal outcome recorded for the attempt.
    pub outcome: AnalyticalAttemptOutcome,
    /// Driver futures joined before the attempt's resources were returned.
    pub drivers_joined: usize,
    /// Scratch bytes returned to the query envelope.
    pub scratch_bytes: u64,
    /// Whether result data left the node under this attempt.
    pub egressed: bool,
}

/// Post-shutdown proof that a supervisor retains nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyticalSupervisorInspection {
    /// Attempts settled by the shutdown itself.
    pub attempts_settled: usize,
    /// Graphs whose runtime and admitted envelope the shutdown released.
    pub graphs_released: usize,
    /// Attempts still retained after shutdown. Terminal evidence asserts zero.
    pub attempts_retained: usize,
    /// Graphs still holding query-owned runtimes. Terminal evidence asserts zero.
    pub graphs_retained: usize,
}

/// Node-local owner of every live Analytical attempt.
///
/// One supervisor exists per Oracle node and is shared by the leader's inactive
/// execution handle and by the follower ingress that installs stage plans. It
/// owns the node's [`AnalyticalRuntimeRegistry`], so registering an attempt and
/// making its graph resolvable to upstream session construction are the same
/// operation, and invalidating the graph happens on the same terminal path that
/// returns the attempt's resources.
pub struct AnalyticalSupervisor {
    /// Query-owned runtimes resolvable by authenticated follower stage work.
    registry: Arc<AnalyticalRuntimeRegistry>,
    /// Admitted query envelopes keyed by graph, one per live distributed plan.
    ///
    /// The value carries the graph's lifecycle state, so `Active` and
    /// `Draining` are one authoritative registry rather than a live map beside
    /// a separate set of names.
    graphs: Mutex<HashMap<AnalyticalGraphKey, AnalyticalGraphEntry>>,
    /// Live attempts keyed by complete two-identity attempt identity.
    attempts: Mutex<HashMap<AnalyticalAttemptKey, AnalyticalAttemptState>>,
    /// Node-scoped cancellation parent of every attempt's cancellation child.
    root_cancel: CancellationToken,
    /// Whether the supervisor still admits new attempts.
    accepting: AtomicBool,
    /// Most recently settled graph's own output-sort evidence.
    ///
    /// Test-tier only. Production reports the same values as counters; a
    /// process-cluster journey needs the exact per-operator numbers its own
    /// query produced, which no counter family can attribute.
    #[cfg(feature = "test-support")]
    physical_evidence: Mutex<Option<super::analytical::AnalyticalPhysicalEvidence>>,
}

impl fmt::Debug for AnalyticalSupervisor {
    /// Reports liveness without rendering retained resource internals.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalSupervisor")
            .field("accepting", &self.accepting.load(Ordering::Acquire))
            .field("cancelled", &self.root_cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl AnalyticalSupervisor {
    /// Creates a node-local supervisor over its own empty runtime registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: Arc::new(AnalyticalRuntimeRegistry::new()),
            graphs: Mutex::new(HashMap::new()),
            attempts: Mutex::new(HashMap::new()),
            root_cancel: CancellationToken::new(),
            accepting: AtomicBool::new(true),
            #[cfg(feature = "test-support")]
            physical_evidence: Mutex::new(None),
        }
    }

    /// Returns the registry authenticated follower work resolves through.
    #[must_use]
    pub fn registry(&self) -> &Arc<AnalyticalRuntimeRegistry> {
        &self.registry
    }

    /// Returns the node-scoped cancellation parent for descendant composition.
    #[must_use]
    pub fn root_cancellation(&self) -> &CancellationToken {
        &self.root_cancel
    }

    /// Reports whether the supervisor still admits and supervises attempts.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.accepting.load(Ordering::Acquire) && !self.root_cancel.is_cancelled()
    }

    /// Returns the number of live attempts, for terminal-cleanup evidence.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the attempt lock is poisoned.
    pub fn live_attempts(&self) -> Result<usize, BifrostError> {
        Ok(self
            .attempts
            .lock()
            .map_err(|_| poisoned_supervisor())?
            .len())
    }

    /// Registers one distributed graph and the query envelope its attempts split.
    ///
    /// Registration is what makes the graph resolvable to upstream session
    /// construction, so an authenticated follower can install this query's own
    /// `RuntimeEnv` instead of a process default. The returned guard owns the
    /// admitted envelope: releasing it is the single act that returns the
    /// query's memory, scratch, and slot units, and it refuses while any attempt
    /// of the graph is still live.
    ///
    /// Every refusal hands `resources` back unchanged. Registration is one step
    /// of a larger activation transaction, and the reservation that transaction
    /// restores is only usable again if it is restored complete; dropping the
    /// admitted envelope here would silently downgrade a recoverable failure
    /// into a lost query envelope.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the supervisor is shutting down,
    /// when the graph is already registered — a duplicate identity, which must
    /// never silently replace a live plan — or when a lock is poisoned, each
    /// paired with the returned envelope.
    pub fn register_graph(
        self: &Arc<Self>,
        graph: AnalyticalGraphKey,
        resources: OracleQueryResources,
        runtime: AnalyticalGraphRuntime,
    ) -> Result<AnalyticalGraphGuard, (Box<OracleQueryResources>, BifrostError)> {
        if !self.is_healthy() {
            return Err((
                Box::new(resources),
                BifrostError::Internal {
                    detail: "Oracle analytical supervisor is shutting down".to_owned(),
                },
            ));
        }
        let Ok(mut graphs) = self.graphs.lock() else {
            return Err((Box::new(resources), poisoned_supervisor()));
        };
        if graphs.contains_key(&graph) {
            return Err((
                Box::new(resources),
                BifrostError::Internal {
                    detail: "Oracle analytical graph is already registered".to_owned(),
                },
            ));
        }
        if let Err(error) = self.registry.register(graph, runtime.clone()) {
            return Err((Box::new(resources), error));
        }
        graphs.insert(
            graph,
            AnalyticalGraphEntry::Active(AnalyticalGraphState {
                resources,
                retained_admission: None,
                runtime,
                exchanges: Arc::default(),
                cancel: self.root_cancel.child_token(),
                metric_fold: None,
                lifecycle: None,
            }),
        );
        drop(graphs);
        tracing::debug!(
            public_query_id = %graph.public_query_id,
            datafusion_query_id = %graph.datafusion_query_id,
            "Oracle analytical graph registered"
        );
        Ok(AnalyticalGraphGuard {
            supervisor: Arc::clone(self),
            graph,
            released: false,
        })
    }

    /// Returns one registered graph's own exchange registry.
    ///
    /// `None` when the graph is not registered here, which is the correct
    /// answer: an unregistered graph opens no exchange.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    pub fn graph_exchanges(
        &self,
        graph: AnalyticalGraphKey,
    ) -> Result<Option<Arc<super::analytical_transport::AnalyticalGraphExchanges>>, BifrostError>
    {
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        Ok(graphs
            .get(&graph)
            .map(|entry| Arc::clone(&entry.state().exchanges)))
    }

    /// Returns one registered graph's own cancellation child.
    ///
    /// `None` when the graph is not registered here, which is the correct
    /// answer: an unregistered graph owns no descendant to cancel.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    pub fn graph_cancellation(
        &self,
        graph: AnalyticalGraphKey,
    ) -> Result<Option<CancellationToken>, BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        Ok(graphs.get(&graph).map(|entry| entry.state().cancel.clone()))
    }

    /// Reports whether one registered graph's envelope has no nested child left.
    ///
    /// An unregistered graph is idle by definition: there is no envelope left to
    /// outlive. Callers use this to wait out a teardown they cannot observe
    /// directly before releasing the graph, so a poison still means a leak.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock or the envelope's
    /// own scratch attribution lock is poisoned.
    pub fn graph_children_idle(&self, graph: AnalyticalGraphKey) -> Result<bool, BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        let Some(entry) = graphs.get(&graph) else {
            return Ok(true);
        };
        entry
            .state()
            .resources
            .nested_idle()
            .map_err(|_| poisoned_supervisor())
    }

    /// Reports what one registered graph's envelope still owes its children.
    ///
    /// Returns the scratch and memory bytes a nested child still holds, in that
    /// order. An unregistered graph owes nothing. This is what makes a drain
    /// timeout name the resource that stayed rather than only its existence.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph or scratch attribution
    /// lock is poisoned.
    pub fn graph_children_debt(
        &self,
        graph: AnalyticalGraphKey,
    ) -> Result<(u64, usize), BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        let Some(entry) = graphs.get(&graph) else {
            return Ok((0, 0));
        };
        entry
            .state()
            .resources
            .nested_debt()
            .map_err(|_| poisoned_supervisor())
    }

    /// Returns the number of registered graphs, for terminal-cleanup evidence.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    pub fn live_graphs(&self) -> Result<usize, BifrostError> {
        Ok(self.graphs.lock().map_err(|_| poisoned_supervisor())?.len())
    }

    /// Counts graphs whose cleanup could not be confirmed complete.
    ///
    /// Only a `Draining` entry carrying a recorded failure is counted. A graph
    /// merely passing through settlement is not residue, so a normal terminal
    /// never removes this node's readiness; an unacknowledged participant
    /// release or a failed cleanup does, and stays counted until it resolves.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    pub fn draining_graphs(&self) -> Result<usize, BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        Ok(graphs
            .values()
            .filter(|entry| {
                matches!(
                    entry,
                    AnalyticalGraphEntry::Draining {
                        settlement_failure: Some(_),
                        ..
                    }
                )
            })
            .count())
    }

    /// Records why one draining graph's cleanup could not be confirmed.
    ///
    /// A poisoned lock is logged rather than propagated: this is called from a
    /// lifecycle task that has no caller to fail, and the alternative is losing
    /// the record entirely. A graph that is no longer registered is likewise
    /// only logged; nothing this node owns is left to attribute the residue to.
    pub fn retain_graph_cleanup(&self, graph: AnalyticalGraphKey, detail: String) {
        let Ok(mut graphs) = self.graphs.lock() else {
            tracing::error!(
                public_query_id = %graph.public_query_id,
                "Oracle analytical graph registry is poisoned"
            );
            return;
        };
        if let Some(AnalyticalGraphEntry::Draining {
            settlement_failure, ..
        }) = graphs.get_mut(&graph)
        {
            *settlement_failure = Some(detail);
        }
    }

    /// Stores one query's admission permit on the graph that owns its envelope.
    ///
    /// The permit outlives the stream that held it because the graph outlives
    /// the stream: cleanup that cannot be confirmed retains the envelope, and
    /// returning the admission counters while that envelope is still charged
    /// would hand a queued waiter capacity this node does not have. Accepting
    /// on `Draining` as well as `Active` is deliberate — a stream observes the
    /// terminal after the lifecycle task has already begun settling.
    ///
    /// The guard must already be cycle-free: its Analytical ownership names
    /// this supervisor, so storing a guard that still holds it would make the
    /// graph own a handle to itself.
    ///
    /// # Errors
    ///
    /// Returns `admitted` unchanged when the graph lock is poisoned, when the
    /// graph is not registered, or when it already retains a permit, so a
    /// refused transfer can never silently lose the query's admission.
    pub(super) fn retain_admission(
        &self,
        graph: AnalyticalGraphKey,
        admitted: super::admission::AdmittedQueryGuard,
    ) -> Result<(), Box<super::admission::AdmittedQueryGuard>> {
        let Ok(mut graphs) = self.graphs.lock() else {
            tracing::error!(
                public_query_id = %graph.public_query_id,
                "Oracle analytical graph registry is poisoned"
            );
            return Err(Box::new(admitted));
        };
        let Some(entry) = graphs.get_mut(&graph) else {
            return Err(Box::new(admitted));
        };
        let state = entry.state_mut();
        if state.retained_admission.is_some() {
            return Err(Box::new(admitted));
        }
        state.retained_admission = Some(admitted);
        Ok(())
    }

    /// Stores one started plan's physical-metric fold on the graph itself.
    ///
    /// The graph outlives the result stream, and the fold cannot run until that
    /// stream is dropped, so the graph is the only owner that can both hold it
    /// and bound it. Registering it here is what removes the unbounded await
    /// that previously lived in the stream's own tail.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned and
    /// [`BifrostError::QueryExecutionFailed`] when the graph is not registered
    /// active or already retains a fold, so a second plan cannot replace the
    /// evidence of the one that ran.
    pub(super) fn retain_metric_fold(
        &self,
        graph: AnalyticalGraphKey,
        fold: super::analytical::AnalyticalGraphMetricFold,
    ) -> Result<(), BifrostError> {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        let Some(AnalyticalGraphEntry::Active(state)) = graphs.get_mut(&graph) else {
            return Err(BifrostError::QueryExecutionFailed);
        };
        if state.metric_fold.is_some() {
            return Err(BifrostError::QueryExecutionFailed);
        }
        state.metric_fold = Some(fold);
        Ok(())
    }

    /// Takes one graph's retained fold, leaving nothing behind to run twice.
    ///
    /// Taken on `Draining` as well as `Active` because settlement moves the
    /// entry before it folds.
    pub(super) fn take_metric_fold(
        &self,
        graph: AnalyticalGraphKey,
    ) -> Option<super::analytical::AnalyticalGraphMetricFold> {
        let mut graphs = self.graphs.lock().ok()?;
        graphs.get_mut(&graph)?.state_mut().metric_fold.take()
    }

    /// Retains the most recently settled graph's output-sort evidence.
    ///
    /// Test-tier only: production publishes the same evidence as counters from
    /// [`super::telemetry::record_output_sort_spill`]. A journey needs the exact
    /// per-operator values its own query produced, and a counter family cannot
    /// answer "which sort" — so the settled evidence is kept verbatim for the
    /// process-cluster control protocol to project.
    #[cfg(feature = "test-support")]
    pub(super) fn record_physical_evidence(
        &self,
        evidence: super::analytical::AnalyticalPhysicalEvidence,
    ) {
        if let Ok(mut retained) = self.physical_evidence.lock() {
            *retained = Some(evidence);
        }
    }

    /// Reports the most recently settled graph's output-sort evidence.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn settled_physical_evidence(
        &self,
    ) -> Option<super::analytical::AnalyticalPhysicalEvidence> {
        self.physical_evidence
            .lock()
            .ok()
            .and_then(|held| held.clone())
    }

    /// Clears one graph's retained cleanup once every release has resolved.
    pub fn resolve_graph_cleanup(&self, graph: AnalyticalGraphKey) {
        let Ok(mut graphs) = self.graphs.lock() else {
            tracing::error!(
                public_query_id = %graph.public_query_id,
                "Oracle analytical graph registry is poisoned"
            );
            return;
        };
        if let Some(AnalyticalGraphEntry::Draining {
            settlement_failure, ..
        }) = graphs.get_mut(&graph)
        {
            *settlement_failure = None;
        }
    }

    /// Reports why one graph's cleanup failed, when it did.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    pub fn graph_settlement_failure(
        &self,
        graph: AnalyticalGraphKey,
    ) -> Result<Option<String>, BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        Ok(match graphs.get(&graph) {
            Some(AnalyticalGraphEntry::Draining {
                settlement_failure, ..
            }) => settlement_failure.clone(),
            _ => None,
        })
    }

    /// Attaches one leader graph's lifecycle task to its registered entry.
    ///
    /// The supervisor becomes the task's only owner here. A caller keeps the
    /// attempt-side signal halves, so it can ask for settlement and await the
    /// result, but it can never take, abort, or await the task itself — which
    /// is what makes a dropped query stream leave a joinable task behind.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the graph is not
    /// registered active, and [`BifrostError::Internal`] when the graph lock is
    /// poisoned or a lifecycle is already attached.
    pub fn attach_lifecycle(
        &self,
        graph: AnalyticalGraphKey,
        lifecycle: AnalyticalGraphLifecycleOwner,
    ) -> Result<(), BifrostError> {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        let Some(AnalyticalGraphEntry::Active(state)) = graphs.get_mut(&graph) else {
            return Err(BifrostError::QueryExecutionFailed);
        };
        if state.lifecycle.is_some() {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical graph already owns a lifecycle task".to_owned(),
            });
        }
        state.lifecycle = Some(lifecycle);
        Ok(())
    }

    /// Signals one graph's terminal and returns the settlement to await.
    ///
    /// The first signal moves `Active` to `Draining` under the graph lock and
    /// chooses the outcome; every later signal observes `Draining`, leaves the
    /// outcome unchanged, and is handed the same result receiver. A graph that
    /// is no longer registered has already settled, so `None` is the correct
    /// answer rather than an error.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    pub fn signal_terminal(
        &self,
        graph: AnalyticalGraphKey,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<
        Option<tokio::sync::watch::Receiver<super::analytical::AnalyticalGraphResult>>,
        BifrostError,
    > {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        let Some(entry) = graphs.remove(&graph) else {
            return Ok(None);
        };
        let (state, settlement_failure, first) = match entry {
            AnalyticalGraphEntry::Active(state) => (state, None, true),
            AnalyticalGraphEntry::Draining {
                state,
                settlement_failure,
            } => (state, settlement_failure, false),
        };
        let result = state.lifecycle.as_ref().map(|lifecycle| {
            if first {
                let _ = lifecycle
                    .control
                    .send(super::analytical::AnalyticalGraphControl::Terminal(outcome));
            }
            lifecycle.result.clone()
        });
        graphs.insert(
            graph,
            AnalyticalGraphEntry::Draining {
                state,
                settlement_failure,
            },
        );
        Ok(result)
    }

    /// Asks one graph's lifecycle task to reserve its participant cut.
    ///
    /// Idempotent by construction: the control channel is monotonic, so a
    /// repeated request is the value the task already handled rather than a
    /// second reservation round.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the graph is no
    /// longer active or owns no lifecycle task, and [`BifrostError::Internal`]
    /// when the graph lock is poisoned.
    pub fn signal_reserve(&self, graph: AnalyticalGraphKey) -> Result<(), BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        let Some(AnalyticalGraphEntry::Active(state)) = graphs.get(&graph) else {
            return Err(BifrostError::QueryExecutionFailed);
        };
        let Some(lifecycle) = state.lifecycle.as_ref() else {
            return Err(BifrostError::QueryExecutionFailed);
        };
        lifecycle
            .control
            .send(super::analytical::AnalyticalGraphControl::ReserveRequested)
            .map_err(|_| BifrostError::QueryExecutionFailed)
    }

    /// Takes one draining graph's lifecycle task so shutdown can join it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    fn take_lifecycle_task(
        &self,
        graph: AnalyticalGraphKey,
    ) -> Result<Option<JoinHandle<()>>, BifrostError> {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        Ok(graphs
            .get_mut(&graph)
            .and_then(|entry| entry.state_mut().lifecycle.as_mut())
            .and_then(|lifecycle| lifecycle.handle.take()))
    }

    /// Reports whether one registered graph still holds a query's admission permit.
    ///
    /// The permit is otherwise unobservable, and "the graph kept it" is exactly
    /// the property a retained cleanup has to prove.
    #[cfg(test)]
    pub(super) fn retains_admission_for_test(&self, graph: AnalyticalGraphKey) -> bool {
        self.graphs.lock().is_ok_and(|graphs| {
            graphs
                .get(&graph)
                .is_some_and(|entry| entry.state().retained_admission.is_some())
        })
    }

    /// Aborts one graph's lifecycle task in place, leaving it joinable.
    ///
    /// The one caller is the test that proves an exceptional lifecycle end is
    /// treated as cleanup failure. The handle stays in the entry so shutdown
    /// still takes and joins it, which is exactly the path production takes
    /// when a task panics.
    #[cfg(test)]
    pub(super) fn abort_lifecycle_task_for_test(&self, graph: AnalyticalGraphKey) {
        let Ok(graphs) = self.graphs.lock() else {
            return;
        };
        if let Some(handle) = graphs
            .get(&graph)
            .and_then(|entry| entry.state().lifecycle.as_ref())
            .and_then(|lifecycle| lifecycle.handle.as_ref())
        {
            handle.abort();
        }
    }

    /// Resolves the query-owned runtime one registered graph installs.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the graph is not
    /// registered, and [`BifrostError::Internal`] on lock poisoning.
    pub fn graph_runtime(
        &self,
        graph: AnalyticalGraphKey,
    ) -> Result<AnalyticalGraphRuntime, BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        graphs
            .get(&graph)
            .map(|entry| entry.state().runtime.clone())
            .ok_or(BifrostError::QueryExecutionFailed)
    }

    /// Releases one graph's runtime and admitted envelope.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an attempt of the graph or a
    /// nested child of its envelope is still live — releasing the envelope
    /// beneath either would poison the resource root — or when a lock is
    /// poisoned. Returns [`BifrostError::QueryExecutionFailed`] when the graph
    /// is not registered.
    pub fn release_graph(&self, graph: AnalyticalGraphKey) -> Result<(), BifrostError> {
        {
            let attempts = self.attempts.lock().map_err(|_| poisoned_supervisor())?;
            if attempts.keys().any(|live| live.graph() == graph) {
                return Err(BifrostError::Internal {
                    detail: "Oracle analytical graph still owns a live attempt".to_owned(),
                });
            }
        }
        // The envelope's own nested children outlive the attempts that made
        // them: upstream drops a follower's stage plan after the coordinator
        // channel ends, so a cache entry, exchange buffer, or spill write can
        // still hold this envelope for a moment. Returning it now would report
        // a live reservation back to the process governor and poison it.
        if !self.graph_children_idle(graph)? {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical graph still owns a live envelope child".to_owned(),
            });
        }
        let removed = {
            let mut graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
            graphs.remove(&graph)
        };
        if removed.is_none() {
            return Err(BifrostError::QueryExecutionFailed);
        }
        self.registry.invalidate(graph)?;
        tracing::debug!(
            public_query_id = %graph.public_query_id,
            datafusion_query_id = %graph.datafusion_query_id,
            "Oracle analytical graph released"
        );
        Ok(())
    }

    /// Admits one attempt of an already registered graph, with its child grants.
    ///
    /// The registration is complete before the guard is returned: the exchange
    /// and scratch children are split from the graph's admitted envelope, a
    /// cancellation child is derived from the node root, and the in-flight gauge
    /// is raised. Every one of those is released together by
    /// [`AnalyticalSupervisor::finish_attempt`] or by dropping the guard. The
    /// graph itself must already be registered; its runtime and envelope
    /// outlive individual attempts, including a retry.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the supervisor is shutting down,
    /// when a live attempt already occupies this stage/task slot — which is how
    /// a retry is held until its predecessor drains — or when a lock is
    /// poisoned. Returns [`BifrostError::QueryExecutionFailed`] when the graph
    /// is not registered, or when its admitted envelope cannot cover the
    /// requested exchange or scratch child.
    pub fn spawn_attempt(
        self: &Arc<Self>,
        key: AnalyticalAttemptKey,
        grant: AnalyticalAttemptGrant,
    ) -> Result<AnalyticalAttemptGuard, BifrostError> {
        if !self.is_healthy() {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical supervisor is shutting down".to_owned(),
            });
        }
        let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
        let Some(AnalyticalGraphEntry::Active(graph)) = graphs.get(&key.graph()) else {
            // A draining graph is deliberately indistinguishable from an
            // unregistered one here: its cleanup already began, so admitting
            // another attempt beneath it would strand work the settlement
            // sequence has stopped waiting for.
            return Err(BifrostError::QueryExecutionFailed);
        };
        let mut attempts = self.attempts.lock().map_err(|_| poisoned_supervisor())?;
        if attempts.contains_key(&key) {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical attempt is already supervised".to_owned(),
            });
        }
        let slot = key.slot();
        if attempts.keys().any(|live| live.slot() == slot) {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical attempt must drain before its retry is admitted"
                    .to_owned(),
            });
        }
        let scratch = graph
            .resources
            .try_split_scratch(grant.scratch_bytes)
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        let cancel = self.root_cancel.child_token();
        let egressed = Arc::new(AtomicBool::new(false));
        let telemetry = AnalyticalAttemptTelemetry::start(
            &key.public_query_id.to_string(),
            &key.datafusion_query_id.to_string(),
            key.attempt.as_u8(),
        );
        tracing::debug!(
            public_query_id = %key.public_query_id,
            datafusion_query_id = %key.datafusion_query_id,
            stage = %key.stage,
            task = key.task.map(TaskId::as_usize),
            attempt = key.attempt.as_u8(),
            scratch_bytes = grant.scratch_bytes,
            "Oracle analytical attempt admitted"
        );
        attempts.insert(
            key,
            AnalyticalAttemptState {
                cancel: cancel.clone(),
                drivers: Vec::new(),
                scratch,
                telemetry,
                grant,
                egressed: Arc::clone(&egressed),
            },
        );
        drop(attempts);
        drop(graphs);
        Ok(AnalyticalAttemptGuard {
            supervisor: Arc::clone(self),
            key,
            cancel,
            egressed,
            settled: false,
        })
    }

    /// Retains one driver future so settlement joins it instead of orphaning it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the attempt lock is poisoned, and
    /// [`BifrostError::QueryExecutionFailed`] when the attempt is no longer
    /// supervised — a stale or sibling graph — which fails the caller closed
    /// rather than letting an unowned future run on.
    pub fn retain_driver(
        &self,
        key: AnalyticalAttemptKey,
        driver: JoinHandle<()>,
    ) -> Result<(), BifrostError> {
        let mut attempts = self.attempts.lock().map_err(|_| poisoned_supervisor())?;
        let Some(state) = attempts.get_mut(&key) else {
            return Err(BifrostError::QueryExecutionFailed);
        };
        state.drivers.push(driver);
        Ok(())
    }

    /// Cancels one attempt, joins its drivers, and returns everything it held.
    ///
    /// The state is removed from the map under the lock before anything is
    /// awaited or dropped, so a concurrent or repeated settlement of the same
    /// key observes an absent attempt and refuses. This is what makes release
    /// exactly-once rather than merely usually-once.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when `key` names no live
    /// attempt, and [`BifrostError::Internal`] when the attempt or registry lock
    /// is poisoned.
    ///
    /// # Panics
    ///
    /// Does not panic. A driver that panicked is reported through its join
    /// result and logged; it does not propagate into settlement.
    pub async fn finish_attempt(
        &self,
        key: AnalyticalAttemptKey,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<AnalyticalAttemptRelease, BifrostError> {
        let Some(mut state) = self.take_attempt(key)? else {
            return Err(BifrostError::QueryExecutionFailed);
        };
        state.cancel.cancel();
        let mut drivers_joined = 0;
        for driver in state.drivers.drain(..) {
            match driver.await {
                Ok(()) => drivers_joined += 1,
                Err(error) if error.is_cancelled() => drivers_joined += 1,
                Err(error) => {
                    tracing::error!(
                        %error,
                        public_query_id = %key.public_query_id,
                        datafusion_query_id = %key.datafusion_query_id,
                        stage = %key.stage,
                        attempt = key.attempt.as_u8(),
                        "Oracle analytical driver failed to settle cleanly"
                    );
                }
            }
        }
        let release = AnalyticalAttemptRelease {
            key,
            outcome,
            drivers_joined,
            scratch_bytes: state.grant.scratch_bytes,
            egressed: state.egressed.load(Ordering::Acquire),
        };
        state.telemetry.finish(outcome);
        let AnalyticalAttemptState { scratch, .. } = state;
        drop(scratch);
        tracing::debug!(
            public_query_id = %key.public_query_id,
            datafusion_query_id = %key.datafusion_query_id,
            stage = %key.stage,
            task = key.task.map(TaskId::as_usize),
            attempt = key.attempt.as_u8(),
            outcome = outcome.as_str(),
            drivers_joined,
            "Oracle analytical attempt settled"
        );
        Ok(release)
    }

    /// Stops admission, cancels the node root, settles every live attempt, and
    /// releases every registered graph's runtime and admitted envelope.
    ///
    /// Every graph that still owns a lifecycle task is signalled first and its
    /// task is joined, because shutdown is the only owner permitted to take
    /// that handle. Joining before the attempt and graph sweep below is what
    /// makes a task that is mid-cleanup finish its own sequence rather than
    /// racing this one.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the attempt or registry lock is
    /// poisoned, leaving the supervisor closed to new work.
    pub async fn shutdown(&self) -> Result<AnalyticalSupervisorInspection, BifrostError> {
        self.accepting.store(false, Ordering::Release);
        self.root_cancel.cancel();
        let signalled: Vec<AnalyticalGraphKey> = {
            let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
            graphs.keys().copied().collect()
        };
        for graph in signalled {
            self.signal_terminal(graph, AnalyticalAttemptOutcome::Cancelled)?;
            // A task that panicked or was cancelled never ran its own cleanup
            // sequence, so whatever it still owned is unreleased. Cancellation
            // is not exempt: the sequence is the only thing that returns a
            // reservation or an envelope, and it did not finish.
            if let Some(handle) = self.take_lifecycle_task(graph)?
                && let Err(error) = handle.await
            {
                tracing::error!(
                    %error,
                    public_query_id = %graph.public_query_id,
                    "Oracle analytical graph lifecycle task failed to join"
                );
                self.retain_graph_cleanup(
                    graph,
                    format!("Oracle analytical graph lifecycle task did not settle: {error}"),
                );
            }
        }
        let live: Vec<AnalyticalAttemptKey> = {
            let attempts = self.attempts.lock().map_err(|_| poisoned_supervisor())?;
            attempts.keys().copied().collect()
        };
        let mut attempts_settled = 0;
        for key in live {
            if self
                .finish_attempt(key, AnalyticalAttemptOutcome::Cancelled)
                .await
                .is_ok()
            {
                attempts_settled += 1;
            }
        }
        let graphs: Vec<AnalyticalGraphKey> = {
            let graphs = self.graphs.lock().map_err(|_| poisoned_supervisor())?;
            graphs.keys().copied().collect()
        };
        let mut graphs_released = 0;
        for graph in graphs {
            // A graph with recorded cleanup failure is residue, not a sweepable
            // entry: releasing it here would return an envelope whose owners
            // were never confirmed released and hide the failure from the
            // report this shutdown produces.
            if self.graph_settlement_failure(graph)?.is_some() {
                continue;
            }
            if self.release_graph(graph).is_ok() {
                graphs_released += 1;
            }
        }
        Ok(AnalyticalSupervisorInspection {
            attempts_settled,
            graphs_released,
            attempts_retained: self.live_attempts()?,
            graphs_retained: self.live_graphs()?,
        })
    }

    /// Removes one attempt's state under the lock, or reports its absence.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the attempt lock is poisoned.
    fn take_attempt(
        &self,
        key: AnalyticalAttemptKey,
    ) -> Result<Option<AnalyticalAttemptState>, BifrostError> {
        let mut attempts = self.attempts.lock().map_err(|_| poisoned_supervisor())?;
        Ok(attempts.remove(&key))
    }

    /// Cancels and releases one attempt without awaiting its drivers.
    ///
    /// This is the drop path. It aborts rather than joins, because a `Drop`
    /// implementation cannot await; callers that need the join guarantee must
    /// use [`AnalyticalSupervisor::finish_attempt`].
    fn abandon_attempt(&self, key: AnalyticalAttemptKey) {
        let taken = match self.take_attempt(key) {
            Ok(taken) => taken,
            Err(error) => {
                tracing::error!(%error, "Oracle analytical attempt abandon failed");
                return;
            }
        };
        let Some(mut state) = taken else {
            return;
        };
        state.cancel.cancel();
        for driver in &state.drivers {
            driver.abort();
        }
        state.telemetry.finish(AnalyticalAttemptOutcome::Cancelled);
        tracing::warn!(
            public_query_id = %key.public_query_id,
            datafusion_query_id = %key.datafusion_query_id,
            stage = %key.stage,
            attempt = key.attempt.as_u8(),
            "Oracle analytical attempt abandoned without an explicit terminal"
        );
    }
}

impl Default for AnalyticalSupervisor {
    /// Creates an empty node-local supervisor.
    fn default() -> Self {
        Self::new()
    }
}

/// RAII ownership of one registered distributed graph.
///
/// Holding the guard is what keeps the graph's admitted query envelope alive
/// and its query-owned runtime resolvable to followers. Dropping it releases
/// both, which is why a follower keeps it for as long as the coordinator may
/// still address the graph rather than for the length of any one stage call.
pub struct AnalyticalGraphGuard {
    /// The supervisor that owns the graph's state.
    supervisor: Arc<AnalyticalSupervisor>,
    /// The two-identity graph this guard owns.
    graph: AnalyticalGraphKey,
    /// Whether an explicit release already returned the envelope.
    released: bool,
}

impl fmt::Debug for AnalyticalGraphGuard {
    /// Reports the graph identity without rendering supervisor internals.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalGraphGuard")
            .field("graph", &self.graph)
            .field("released", &self.released)
            .finish_non_exhaustive()
    }
}

impl AnalyticalGraphGuard {
    /// Returns the graph identity every attempt of this plan must carry.
    #[must_use]
    pub const fn graph(&self) -> AnalyticalGraphKey {
        self.graph
    }

    /// Releases the graph's runtime and admitted envelope.
    ///
    /// # Errors
    ///
    /// Returns the error reported by [`AnalyticalSupervisor::release_graph`],
    /// notably a refusal while an attempt of the graph is still live.
    pub fn release(mut self) -> Result<(), BifrostError> {
        self.released = true;
        self.supervisor.release_graph(self.graph)
    }

    /// Gives up the guard without releasing the graph it owns.
    ///
    /// The one caller is a cleanup sequence that failed: the supervisor keeps
    /// the graph's entry, envelope, and runtime as `Draining` so the residue
    /// stays named and attributable rather than being returned to the process
    /// governor while a child may still hold it.
    pub fn retain(mut self) {
        self.released = true;
    }
}

impl Drop for AnalyticalGraphGuard {
    /// Releases a graph whose owner vanished without an explicit release.
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Err(error) = self.supervisor.release_graph(self.graph) {
            tracing::error!(
                %error,
                public_query_id = %self.graph.public_query_id,
                datafusion_query_id = %self.graph.datafusion_query_id,
                "Oracle analytical graph release failed on drop"
            );
        }
    }
}

/// RAII ownership of one supervised attempt.
///
/// Holding the guard is what keeps the attempt's runtime resolvable and its
/// child grants charged. Dropping it without an explicit terminal cancels and
/// releases the attempt rather than leaking it, but only
/// [`AnalyticalAttemptGuard::finish`] joins the attempt's drivers.
pub struct AnalyticalAttemptGuard {
    /// The supervisor that owns the attempt's state.
    supervisor: Arc<AnalyticalSupervisor>,
    /// The complete two-identity attempt identity.
    key: AnalyticalAttemptKey,
    /// The attempt's cancellation child, shared with every descendant.
    cancel: CancellationToken,
    /// Egress fence shared with the supervisor's attempt state.
    egressed: Arc<AtomicBool>,
    /// Whether an explicit terminal already released the attempt.
    settled: bool,
}

impl fmt::Debug for AnalyticalAttemptGuard {
    /// Reports the attempt identity without rendering supervisor internals.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalAttemptGuard")
            .field("key", &self.key)
            .field("settled", &self.settled)
            .finish_non_exhaustive()
    }
}

impl AnalyticalAttemptGuard {
    /// Returns the attempt identity every descendant must bind to.
    #[must_use]
    pub const fn key(&self) -> AnalyticalAttemptKey {
        self.key
    }

    /// Returns the cancellation child covering every descendant of the attempt.
    #[must_use]
    pub const fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Returns the supervisor that owns this attempt, for admitting its retry.
    #[must_use]
    pub fn supervisor(&self) -> Arc<AnalyticalSupervisor> {
        Arc::clone(&self.supervisor)
    }

    /// Records that result data produced under this attempt left the node.
    ///
    /// Idempotent, and deliberately one-way: once an attempt has egressed, no
    /// later observation can un-fence it.
    pub fn record_egress(&self) {
        self.egressed.store(true, Ordering::Release);
    }

    /// Returns the shared egress fence, for an owner that outlives this guard.
    #[must_use]
    pub fn egress_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.egressed)
    }

    /// Reports whether result data already left the node under this attempt.
    #[must_use]
    pub fn egressed(&self) -> bool {
        self.egressed.load(Ordering::Acquire)
    }

    /// Retains one driver future under this exact attempt.
    ///
    /// # Errors
    ///
    /// Returns the error reported by
    /// [`AnalyticalSupervisor::retain_driver`].
    pub fn retain_driver(&self, driver: JoinHandle<()>) -> Result<(), BifrostError> {
        self.supervisor.retain_driver(self.key, driver)
    }

    /// Settles the attempt, joining its drivers and returning its grants.
    ///
    /// # Errors
    ///
    /// Returns the error reported by
    /// [`AnalyticalSupervisor::finish_attempt`].
    pub async fn finish(
        mut self,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<AnalyticalAttemptRelease, BifrostError> {
        self.settled = true;
        self.supervisor.finish_attempt(self.key, outcome).await
    }
}

impl Drop for AnalyticalAttemptGuard {
    /// Cancels and releases an attempt whose owner vanished without a terminal.
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        self.supervisor.abandon_attempt(self.key);
    }
}

/// Builds the stable internal error for a poisoned supervisor lock.
fn poisoned_supervisor() -> BifrostError {
    BifrostError::Internal {
        detail: "Oracle analytical supervisor lock is poisoned".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use wyrd_spec::vala::api::QueryClass;

    use super::super::spill::OracleSpillRuntime;
    use super::*;
    use crate::resources::{
        BifrostResourcePolicy, BifrostRole, BifrostRuntimeResources, OracleResourceRequest,
        OracleResources, ResourceSource, SystemResourceSnapshot,
    };

    /// Bytes each fixture attempt charges as its exchange-buffer child.
    const FIXTURE_EXCHANGE_BYTES: usize = 64 * 1024;

    /// Bytes each fixture attempt charges as its scratch child.
    const FIXTURE_SCRATCH_BYTES: u64 = 4 * 1024;

    /// Builds one live Oracle role owner from an injected resource observation.
    ///
    /// This is the production composition stage, not a stub: the returned owner
    /// issues real admitted query envelopes whose nested children the supervisor
    /// splits and must return.
    ///
    /// # Panics
    ///
    /// Panics when the injected observation cannot compose an Oracle role.
    fn oracle_role() -> OracleResources {
        let snapshot = SystemResourceSnapshot {
            memory_limit_bytes: 4 * 1024 * 1024 * 1024,
            effective_cpu: 8,
            scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
            scratch_available_bytes: 2 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        };
        let policy = BifrostResourcePolicy {
            roles: [BifrostRole::Oracle].into_iter().collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: PathBuf::new(),
            volume_roots: None,
        };
        BifrostRuntimeResources::from_snapshot(snapshot, policy)
            .expect("an injected Oracle observation composes the production root")
            .compose_roles()
            .expect("role composition is issued from an unpoisoned root")
            .oracle()
            .expect("the Oracle role is active in this policy")
    }

    /// Builds the query-owned runtime an admitted analytical query installs.
    ///
    /// The runtime carries the query's own admitted pool and a disk manager
    /// bounded by its own scratch share, so a follower that installs it cannot
    /// reach process-wide capacity.
    ///
    /// # Panics
    ///
    /// Panics when the pod spill owner or the query runtime cannot be built.
    fn query_runtime(
        resources: &OracleQueryResources,
        spill: &OracleSpillRuntime,
    ) -> AnalyticalGraphRuntime {
        let runtime = spill
            .build_query_runtime(resources.memory_pool(), resources.scratch_bytes)
            .expect("an admitted scratch share builds a bounded query runtime");
        AnalyticalGraphRuntime::new(
            runtime,
            crate::resources::OracleSessionShape::for_grant(
                resources.granted_memory_bytes,
                resources.target_partitions,
                resources.target_partitions,
            ),
        )
    }

    /// Names one stage-scoped attempt zero for `graph`.
    fn attempt_zero(graph: AnalyticalGraphKey, stage: usize) -> AnalyticalAttemptKey {
        AnalyticalAttemptKey::new(
            graph.public_query_id,
            graph.datafusion_query_id,
            StageId::new(stage),
            Some(TaskId::new(0)),
            AnalyticalAttemptNumber::ZERO,
        )
    }

    /// Builds the exact fixture grant both supervisor tests charge.
    const fn fixture_grant() -> AnalyticalAttemptGrant {
        AnalyticalAttemptGrant {
            scratch_bytes: FIXTURE_SCRATCH_BYTES,
        }
    }

    /// Builds the two-column ordered plan whose one output sort carries evidence.
    ///
    /// Never executed: the claim under test is the settlement order and the
    /// deadline that bounds it, and a `SortExec` that never ran still reports
    /// its own identity and a zero spill set, which is exactly what the
    /// lifecycle folds.
    ///
    /// # Panics
    ///
    /// Panics when the fixed ordering cannot be built.
    #[cfg(feature = "test-support")]
    fn ordered_plan() -> Arc<dyn datafusion::physical_plan::ExecutionPlan> {
        use arrow::datatypes::{DataType, Field, Schema};
        use datafusion::physical_expr::LexOrdering;
        use datafusion::physical_expr::PhysicalSortExpr;
        use datafusion::physical_expr::expressions::Column;
        use datafusion::physical_plan::empty::EmptyExec;
        use datafusion::physical_plan::sorts::sort::SortExec;

        let schema = Arc::new(Schema::new(vec![
            Field::new("filter_key", DataType::Utf8, false),
            Field::new("matched", DataType::Int64, false),
        ]));
        let source = Arc::new(EmptyExec::new(Arc::clone(&schema)));
        let ordering = LexOrdering::new(vec![PhysicalSortExpr::new(
            Arc::new(Column::new("filter_key", 0)),
            arrow::compute::SortOptions {
                descending: false,
                nulls_first: false,
            },
        )])
        .expect("a one-column ordering is non-empty");
        Arc::new(SortExec::new(ordering, source))
    }

    /// Registers one graph, its attempt, and its lifecycle over a held fold.
    ///
    /// Returns the signal half the caller drives, so the test states only the
    /// terminal it signals and the moment it releases the fold.
    ///
    /// # Panics
    ///
    /// Panics when the fixture graph, attempt, or lifecycle cannot be composed.
    #[cfg(feature = "test-support")]
    fn lifecycle_over_fold(
        supervisor: &Arc<AnalyticalSupervisor>,
        graph: AnalyticalGraphKey,
        resources: OracleQueryResources,
        runtime: AnalyticalGraphRuntime,
        deadline: tokio::time::Instant,
        fold: futures_util::future::BoxFuture<
            'static,
            Option<Arc<dyn datafusion::physical_plan::ExecutionPlan>>,
        >,
    ) -> super::super::analytical::AnalyticalGraphSignals {
        use super::super::analytical::{
            AnalyticalGraphLifecycle, AnalyticalGraphLifecycleOwners, AnalyticalGraphMetricFold,
        };
        use wyrd_spec::vala::api::{AnalyticalGraphRef, QueryId, ReserveNodeSlotsRequest};

        let graph_guard = supervisor
            .register_graph(graph, resources, runtime)
            .map_err(|(_, error)| error)
            .expect("an idle supervisor registers one graph");
        let attempt = supervisor
            .spawn_attempt(attempt_zero(graph, 0), fixture_grant())
            .expect("a registered graph admits its own attempt");
        let signals = AnalyticalGraphLifecycle::start(
            graph,
            Arc::clone(supervisor),
            None,
            Vec::new(),
            ReserveNodeSlotsRequest {
                query_id: QueryId::new(graph.public_query_id.as_uuid()),
                leader_node_id: uuid::Uuid::now_v7().into(),
                leader_fencing_token: 1,
                query_class: QueryClass::Analytical,
                slot_units: 2,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(30),
                graph: Some(AnalyticalGraphRef {
                    public_query_id: graph.public_query_id.as_uuid(),
                    datafusion_query_id: graph.datafusion_query_id.as_uuid(),
                }),
            },
            deadline,
            AnalyticalGraphLifecycleOwners {
                attempt: Some(attempt),
                graph_guard: Some(graph_guard),
            },
        )
        .expect("a registered graph starts its lifecycle task");
        supervisor
            .retain_metric_fold(
                graph,
                AnalyticalGraphMetricFold::from_future(ordered_plan(), fold),
            )
            .expect("an active graph retains exactly one metric fold");
        signals
    }

    /// Awaits one graph's published settlement.
    ///
    /// # Panics
    ///
    /// Panics when the lifecycle task ends without publishing a settlement.
    #[cfg(feature = "test-support")]
    async fn published_settlement(
        signals: &super::super::analytical::AnalyticalGraphSignals,
    ) -> super::super::analytical::AnalyticalGraphResult {
        use super::super::analytical::AnalyticalGraphResult;

        let mut receiver = signals.settled_receiver();
        loop {
            let observed = *receiver.borrow_and_update();
            if matches!(
                observed,
                AnalyticalGraphResult::SettledSuccess(_) | AnalyticalGraphResult::SettledFailure
            ) {
                return observed;
            }
            receiver
                .changed()
                .await
                .expect("the lifecycle task publishes a settlement before it ends");
        }
    }

    /// The graph deadline, not the metric fold, decides when settlement ends.
    ///
    /// Drives two graphs whose distributed metric fold the test holds. The
    /// first fold is never released: the deadline expires, the graph takes the
    /// failed-terminal `Draining` route, no successful release is published,
    /// readiness stays false, and shutdown still observes and joins the graph.
    /// The second releases its fold inside the deadline and settles
    /// successfully, with the folded output-sort evidence already retained by
    /// the time the release is published and the graph removed.
    ///
    /// Required mutation RED: await the fold without
    /// [`tokio::time::timeout`] and the first case never settles at all;
    /// fold after `attempt.finish` and the second case publishes its release
    /// before the evidence exists.
    ///
    /// # Panics
    ///
    /// Panics when a fixture owner cannot be composed or a settlement diverges.
    #[cfg(feature = "test-support")]
    #[tokio::test(start_paused = true)]
    async fn distributed_metrics_settle_within_the_graph_deadline() {
        use super::super::analytical::AnalyticalGraphResult;

        let oracle = oracle_role();
        let scratch_root = tempfile::tempdir().expect("fixture scratch root");
        let spill = OracleSpillRuntime::new(scratch_root.path(), 2 * 1024 * 1024 * 1024)
            .expect("a positive pod ceiling builds the process spill owner");
        let supervisor = Arc::new(AnalyticalSupervisor::new());
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);

        let held = Arc::new(tokio::sync::Notify::new());
        let expired_graph = AnalyticalGraphKey {
            public_query_id: PublicQueryId::from_uuid(uuid::Uuid::now_v7()),
            datafusion_query_id: DataFusionQueryId::allocate(),
        };
        let expired_resources = oracle
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .expect("an idle Oracle admits the held query");
        let expired_runtime = query_runtime(&expired_resources, &spill);
        let never = Arc::clone(&held);
        let expired = lifecycle_over_fold(
            &supervisor,
            expired_graph,
            expired_resources,
            expired_runtime,
            deadline,
            Box::pin(async move {
                // Released only by a companion this case never runs, so the
                // deadline is the only thing that can end the wait.
                never.notified().await;
                None
            }),
        );
        expired.terminal(AnalyticalAttemptOutcome::Success);

        assert_eq!(
            published_settlement(&expired).await,
            AnalyticalGraphResult::SettledFailure,
            "a fold that outlives the graph deadline cannot publish a success"
        );
        assert_eq!(
            supervisor
                .draining_graphs()
                .expect("the supervisor reports its retained graphs"),
            1,
            "the expired graph is retained as draining with its owners"
        );
        assert!(
            supervisor
                .graph_settlement_failure(expired_graph)
                .expect("the supervisor reports its recorded failure")
                .is_some(),
            "the retained graph names why its cleanup did not complete"
        );

        let settled_graph = AnalyticalGraphKey {
            public_query_id: PublicQueryId::from_uuid(uuid::Uuid::now_v7()),
            datafusion_query_id: DataFusionQueryId::allocate(),
        };
        let settled_resources = oracle
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .expect("an idle Oracle admits the released query");
        let settled_runtime = query_runtime(&settled_resources, &spill);
        let settled = lifecycle_over_fold(
            &supervisor,
            settled_graph,
            settled_resources,
            settled_runtime,
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            Box::pin(async { None }),
        );
        settled.terminal(AnalyticalAttemptOutcome::Success);

        let release = match published_settlement(&settled).await {
            AnalyticalGraphResult::SettledSuccess(release) => release,
            other => panic!("a fold released inside the deadline settles cleanly: {other:?}"),
        };
        assert!(
            release.is_some(),
            "a clean settlement publishes its attempt's release evidence"
        );
        let evidence = supervisor
            .settled_physical_evidence()
            .expect("the settled graph folded its own output-sort evidence");
        assert_eq!(
            evidence.sort_schema,
            vec!["filter_key".to_owned(), "matched".to_owned()],
            "the folded evidence names the query's own output schema"
        );
        assert!(
            supervisor.live_graphs().expect("graphs are inspectable") == 1,
            "only the retained draining graph survives its own settlement"
        );

        let inspection = supervisor
            .shutdown()
            .await
            .expect("shutdown joins every remaining lifecycle task");
        assert_eq!(
            inspection.graphs_retained, 1,
            "shutdown observes and reports the retained draining graph"
        );
    }

    /// Two `DataFusion` graphs under one public query never reach each other.
    ///
    /// Admits one attempt under graph A and a same-coordinate attempt under a
    /// sibling graph B that shares A's public query id, then asserts that
    /// settling B leaves A live, that B's cancellation does not reach A, and
    /// that each graph resolves only its own query-owned runtime.
    ///
    /// Required mutation RED: drop `datafusion_query_id` from
    /// [`AnalyticalAttemptKey`] or [`AnalyticalGraphKey`] and the two keys
    /// collapse — settling B settles A, and the liveness and runtime-identity
    /// assertions fail.
    ///
    /// # Panics
    ///
    /// Panics when either attempt cannot be admitted or a settlement diverges.
    #[tokio::test]
    async fn analytical_stage_identity_separates_public_and_datafusion_queries() {
        let oracle = oracle_role();
        let scratch_root = tempfile::tempdir().expect("fixture scratch root");
        let spill = OracleSpillRuntime::new(scratch_root.path(), 2 * 1024 * 1024 * 1024)
            .expect("a positive pod ceiling builds the process spill owner");
        let resources = oracle
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .expect("an idle Oracle admits one analytical query");
        let resources_b = oracle
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .expect("an idle Oracle admits a second analytical query");
        let runtime_a = query_runtime(&resources, &spill);
        let runtime_b = query_runtime(&resources_b, &spill);
        let supervisor = Arc::new(AnalyticalSupervisor::new());

        let public_query_id = PublicQueryId::from_uuid(uuid::Uuid::now_v7());
        let graph_a = AnalyticalGraphKey {
            public_query_id,
            datafusion_query_id: DataFusionQueryId::allocate(),
        };
        let graph_b = AnalyticalGraphKey {
            public_query_id,
            datafusion_query_id: DataFusionQueryId::allocate(),
        };
        assert_ne!(
            graph_a, graph_b,
            "two distributed plans under one public query are distinct graphs"
        );

        let key_a = attempt_zero(graph_a, 1);
        let key_b = attempt_zero(graph_b, 1);
        let graph_guard_a = supervisor
            .register_graph(graph_a, resources, runtime_a)
            .expect("graph A registers its own admitted envelope");
        let graph_guard_b = supervisor
            .register_graph(graph_b, resources_b, runtime_b)
            .expect("a sibling graph registers its own admitted envelope");
        let guard_a = supervisor
            .spawn_attempt(key_a, fixture_grant())
            .expect("graph A admits its first attempt");
        let guard_b = supervisor
            .spawn_attempt(key_b, fixture_grant())
            .expect("an identically numbered stage under a sibling graph is a distinct slot");
        assert_eq!(
            supervisor.live_attempts().expect("live attempts"),
            2,
            "graph-local stage and task ordinals must not collide across graphs"
        );

        let release_b = guard_b
            .finish(AnalyticalAttemptOutcome::Success)
            .await
            .expect("graph B settles its own attempt");
        assert_eq!(release_b.key, key_b);
        graph_guard_b
            .release()
            .expect("graph B releases once its last attempt has drained");
        assert!(
            !guard_a.cancellation().is_cancelled(),
            "settling a sibling graph must not cancel this graph's attempt"
        );
        assert_eq!(
            supervisor.live_attempts().expect("live attempts"),
            1,
            "graph A's attempt survives a sibling graph's terminal"
        );
        assert!(
            supervisor.registry().resolve(graph_b).is_err(),
            "an invalidated sibling graph resolves no query-owned runtime"
        );
        assert!(
            supervisor.registry().resolve(graph_a).is_ok(),
            "graph A keeps its own query-owned runtime"
        );
        assert!(
            supervisor
                .finish_attempt(key_b, AnalyticalAttemptOutcome::Cancelled)
                .await
                .is_err(),
            "a settled sibling attempt cannot be settled again through graph A's supervisor"
        );

        guard_a
            .finish(AnalyticalAttemptOutcome::Success)
            .await
            .expect("graph A settles its own attempt");
        drop(graph_guard_a);
        let inspection = supervisor
            .shutdown()
            .await
            .expect("shutdown inspects cleanly");
        assert_eq!(inspection.attempts_retained, 0);
        assert_eq!(inspection.graphs_retained, 0);
    }

    /// One attempt installs the query's own runtime and releases it exactly once.
    ///
    /// Admits an attempt against a real analytical grant and asserts the graph
    /// resolves to the query's own `RuntimeEnv` and pool rather than a process
    /// default, that the exchange child is charged against that pool, and that a
    /// second settlement of the same key is refused without returning any byte
    /// twice — proven by the query owner releasing cleanly at the end, which it
    /// refuses to do while a nested child survives.
    ///
    /// Required mutation RED: substitute a process runtime in
    /// [`AnalyticalSupervisor::spawn_attempt`] and the pool-identity assertion
    /// fails; release the state before removing it from the map and the second
    /// settlement succeeds, double-returning the child and failing the
    /// reservation assertions.
    ///
    /// # Panics
    ///
    /// Panics when admission, installation, or release diverges.
    /// Proves an exchange meets the query's one ceiling and no second one.
    ///
    /// Spawning an attempt splits no exchange child off the envelope, so a
    /// pinned exchange consumer registered on the installed runtime charges the
    /// query pool directly and returns every byte when it is dropped.
    ///
    /// # Panics
    ///
    /// Panics when the attempt pre-charged the pool, the allocation is refused,
    /// or it is not charged to the installed pool.
    fn assert_exchange_allocates_from_installed_pool(
        runtime: &Arc<datafusion::execution::runtime_env::RuntimeEnv>,
        pool: &Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    ) {
        assert_eq!(
            pool.reserved(),
            0,
            "spawning an attempt splits no exchange child off the query pool"
        );
        let exchange = datafusion::execution::memory_pool::MemoryConsumer::new("fixture-exchange");
        let exchange = exchange.register(&runtime.memory_pool);
        exchange
            .try_grow(FIXTURE_EXCHANGE_BYTES)
            .expect("the admitted grant covers one exchange allocation");
        assert!(
            pool.reserved() >= FIXTURE_EXCHANGE_BYTES,
            "the pinned exchange consumer allocates from the installed query pool"
        );
    }

    #[tokio::test]
    async fn analytical_attempt_installs_query_owned_runtime_and_releases_once() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let metrics_guard = metrics::set_default_local_recorder(&recorder);
        let oracle = oracle_role();
        let scratch_root = tempfile::tempdir().expect("fixture scratch root");
        let spill = OracleSpillRuntime::new(scratch_root.path(), 2 * 1024 * 1024 * 1024)
            .expect("a positive pod ceiling builds the process spill owner");
        let resources = oracle
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .expect("an idle Oracle admits one analytical query");
        let pool = resources.memory_pool();
        let runtime = query_runtime(&resources, &spill);
        let supervisor = Arc::new(AnalyticalSupervisor::new());

        let graph = AnalyticalGraphKey {
            public_query_id: PublicQueryId::from_uuid(uuid::Uuid::now_v7()),
            datafusion_query_id: DataFusionQueryId::allocate(),
        };
        let key = attempt_zero(graph, 0);
        let graph_guard = supervisor
            .register_graph(graph, resources, runtime.clone())
            .expect("the graph registers its admitted envelope");
        let guard = supervisor
            .spawn_attempt(key, fixture_grant())
            .expect("the admitted envelope covers the fixture grant");

        let resolved = supervisor
            .registry()
            .resolve(graph)
            .expect("a registered graph is resolvable");
        assert!(
            Arc::ptr_eq(resolved.runtime(), runtime.runtime()),
            "a follower resolves this query's own RuntimeEnv, never a process default"
        );
        assert!(
            Arc::ptr_eq(&resolved.runtime().memory_pool, &pool),
            "the installed runtime carries this query's admitted memory pool"
        );
        assert_exchange_allocates_from_installed_pool(resolved.runtime(), &pool);

        let release = guard
            .finish(AnalyticalAttemptOutcome::Success)
            .await
            .expect("the attempt settles once");
        assert_eq!(release.scratch_bytes, FIXTURE_SCRATCH_BYTES);
        assert_eq!(release.drivers_joined, 0);
        assert_eq!(
            pool.reserved(),
            0,
            "the exchange consumer returns every byte to the query pool"
        );

        assert!(
            supervisor
                .finish_attempt(key, AnalyticalAttemptOutcome::Cancelled)
                .await
                .is_err(),
            "a settled attempt cannot release its resources a second time"
        );
        assert_eq!(
            pool.reserved(),
            0,
            "a refused second settlement returns nothing"
        );
        graph_guard
            .release()
            .expect("the graph releases once its last attempt has drained");
        assert_eq!(
            oracle
                .snapshot()
                .expect("a released query owner leaves an unpoisoned root")
                .oracle_query_memory_used_bytes,
            0,
            "the query owner returns its whole envelope, which it refuses to do \
             while a nested exchange or scratch child survives"
        );
        drop(metrics_guard);

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .gauges
                .get("bifrost_oracle_analytical_attempts_active")
                .copied(),
            Some(0.0),
            "the in-flight attempt gauge returns to baseline on the terminal path: {snapshot:?}"
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_oracle_analytical_attempts_total{outcome=\"success\"}")
                .copied(),
            Some(1),
            "exactly one terminal outcome is recorded: {snapshot:?}"
        );
    }
}
