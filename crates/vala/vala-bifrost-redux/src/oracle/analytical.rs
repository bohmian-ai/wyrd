//! Oracle-owned, production-unreachable distributed analytical execution.
//!
//! Bifrost's production query path is Interactive: [`crate::oracle::Oracle`]
//! plans one physical plan and executes it on the leader, dispatching only leaf
//! scans to followers. This module owns the *inactive* Analytical alternative —
//! a full distributed physical plan whose stages execute on followers through
//! Wyrd's already-authenticated Oracle peer ingress.
//!
//! Nothing in production routing reaches this module. The only entry point is
//! [`AnalyticalExecutionHandle::execute_inactive`], which is `pub(crate)` and
//! re-exported to the test-support surface, so a query that arrives on the
//! public HTTP/gRPC/MCP surface still cannot select Analytical execution.
//!
//! # Why the upstream crate rather than a Wyrd scheduler
//!
//! `datafusion-distributed` is pinned into the same native `DataFusion` 55 /
//! Arrow 59.2 cone Bifrost already uses (proved by
//! `tests/integration/distributed_compat.rs`). It supplies the distributed
//! physical planner, stage graph, network boundaries, and worker task cache.
//! Wyrd supplies exactly the three things the upstream crate deliberately
//! leaves to its embedder, and this module owns all three:
//!
//! 1. **Authority.** Wyrd's own [`AnalyticalChannelResolver`] is the coordinator
//!    transport, so every stage operation carries a Wyrd-signed ticket in its
//!    [`HeaderMap`], and the same headers reach the worker before any plan is
//!    decoded.
//! 2. **Runtime.** [`AnalyticalSessionBuilder`] resolves the *query-owned*
//!    [`RuntimeEnv`] — the memory pool and bounded scratch already admitted for
//!    this query — and installs it on the follower session, replacing the
//!    worker's process-lifetime runtime.
//! 3. **Futures.** Upstream returns its task streams to the caller rather than
//!    detaching them, so [`AnalyticalSupervisor`] can retain, cancel, and join
//!    every descendant by exact attempt.
//!
//! [`AnalyticalSupervisor`]: crate::oracle::analytical_supervisor::AnalyticalSupervisor

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use datafusion::error::DataFusionError;
use datafusion::execution::runtime_env::RuntimeEnv;
use datafusion::execution::{SendableRecordBatchStream, SessionState, TaskContext};
use datafusion_distributed::{
    CoordinatorToWorkerMsg, ExecuteTaskRequest, SetPlanRequest, Worker, WorkerQueryContext,
    WorkerSessionBuilder, WorkerToCoordinatorMsg,
};
use futures_util::stream::BoxStream;
use http::HeaderMap;
use uuid::Uuid;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::SignedPeerTicket;

use super::analytical_supervisor::{
    AnalyticalAttemptGrant, AnalyticalAttemptGuard, AnalyticalAttemptKey, AnalyticalGraphGuard,
    AnalyticalSupervisor, AnalyticalSupervisorInspection, StageId, TaskId,
};
use super::peer::{
    AuthorizedStage, OracleStageAuthority, PeerSecurityError, StageBinding, StageOperationV1,
};
use super::spill::OracleSpillRuntime;
use super::telemetry::{
    AnalyticalAttemptOutcome, AnalyticalStageOperation, record_stage_operation,
};
use crate::resources::{OracleResourceRequest, OracleResources};
use wyrd_spec::vala::api::QueryClass;

/// Header carrying the client-visible query identity on every stage operation.
pub(crate) const PUBLIC_QUERY_ID_HEADER: &str = "wyrd-oracle-public-query-id";

/// Header carrying the private distributed-graph identity on every stage operation.
pub(crate) const DATAFUSION_QUERY_ID_HEADER: &str = "wyrd-oracle-datafusion-query-id";

/// The one client-visible identity of an authenticated Oracle query.
///
/// Oracle allocates exactly one of these when an authenticated public query
/// starts, before any execution class is chosen. It owns the complete
/// client-visible lifecycle: running-query lookup, cancellation, WAL read
/// acceptance, terminal result, stable error, and audit correlation. T2
/// projects only this identity onto public surfaces.
///
/// It is deliberately a distinct newtype from [`DataFusionQueryId`]: the two
/// name different lifecycles, and a function that accepts one must not silently
/// accept the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PublicQueryId(Uuid);

impl PublicQueryId {
    /// Adopts the identity already minted for an authenticated public query.
    ///
    /// Oracle's public entry derives one request-scoped UUID before classifying
    /// the query; this constructor names that value as the public identity
    /// rather than allocating a competing one.
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Returns the raw UUID for wire encoding and trace correlation.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for PublicQueryId {
    /// Renders the public identity for headers, traces, and audit rows.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// The private identity of exactly one distributed physical plan and stage graph.
///
/// Oracle allocates this only after a validated exchange-bearing distributed
/// physical plan exists and Analytical execution has been irreversibly selected,
/// and before admission, ticket issuance, cache registration, or attempt
/// creation. Planning failure or an Interactive fallback allocates none.
///
/// It never becomes a public request field, a client control, or an
/// independently durable query record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataFusionQueryId(Uuid);

impl DataFusionQueryId {
    /// Allocates the one private graph identity for a selected Analytical query.
    ///
    /// Call this exactly once per public query, at the point Analytical
    /// execution becomes irreversible.
    #[must_use]
    pub fn allocate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Adopts a graph identity received on an authenticated stage operation.
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Returns the raw UUID for wire encoding and upstream task keys.
    ///
    /// Upstream `datafusion-distributed` keys its task cache by a bare
    /// [`Uuid`]; this is the only value Wyrd ever gives it, so an upstream task
    /// key can never collide across two distinct Wyrd graphs.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for DataFusionQueryId {
    /// Renders the private graph identity for headers and internal traces.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// The attempt ordinal within one distributed graph.
///
/// Exactly two values are reachable: attempt zero, and the one permitted
/// authenticated pre-egress retry. A third attempt is not representable through
/// [`AnalyticalAttemptNumber::retry`], which is the only way to advance it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnalyticalAttemptNumber(u8);

impl AnalyticalAttemptNumber {
    /// The first attempt of a graph.
    pub const ZERO: Self = Self(0);

    /// The one permitted retry of a graph.
    pub const ONE: Self = Self(1);

    /// Advances to the one permitted retry.
    ///
    /// Returns `None` for the retry itself, which is what makes "at most one
    /// retry" a property of the type rather than of a caller's discipline.
    #[must_use]
    pub const fn retry(self) -> Option<Self> {
        match self.0 {
            0 => Some(Self::ONE),
            _ => None,
        }
    }

    /// Returns the ordinal for wire encoding, claims binding, and evidence.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Adopts an attempt ordinal received on an authenticated stage operation.
    ///
    /// Returns `None` for any ordinal above the one permitted retry, so a
    /// forged third attempt is rejected at the parsing boundary.
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::ZERO),
            1 => Some(Self::ONE),
            _ => None,
        }
    }
}

/// The two-identity parent of every stage, task, exchange, and lease.
///
/// Stage and task identifiers are graph-local and are never resolved without
/// both parents, so a sibling graph under the same public query cannot reach
/// this graph's cached plans, exchanges, or leases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnalyticalGraphKey {
    /// The client-visible query this graph belongs to.
    pub public_query_id: PublicQueryId,
    /// The one private distributed plan identity.
    pub datafusion_query_id: DataFusionQueryId,
}

impl AnalyticalGraphKey {
    /// Names one distributed graph by both of its parent identities.
    #[must_use]
    pub const fn new(
        public_query_id: PublicQueryId,
        datafusion_query_id: DataFusionQueryId,
    ) -> Self {
        Self {
            public_query_id,
            datafusion_query_id,
        }
    }

    /// Renders both identities into the stage-operation header pair.
    ///
    /// This is the exact header set every Wyrd stage operation carries, and the
    /// only thing [`AnalyticalSessionBuilder`] consults to resolve a follower's
    /// query-owned runtime.
    ///
    /// # Panics
    ///
    /// Never panics: both identities render as hyphenated UUIDs, which are
    /// always valid header values.
    #[must_use]
    pub fn to_headers(self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let public = self.public_query_id.to_string();
        let private = self.datafusion_query_id.to_string();
        headers.insert(
            PUBLIC_QUERY_ID_HEADER,
            public
                .parse()
                .unwrap_or_else(|_| unreachable!("a hyphenated UUID is a valid header value")),
        );
        headers.insert(
            DATAFUSION_QUERY_ID_HEADER,
            private
                .parse()
                .unwrap_or_else(|_| unreachable!("a hyphenated UUID is a valid header value")),
        );
        headers
    }

    /// Recovers both identities from a stage operation's headers.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when either header is
    /// missing or is not a hyphenated UUID. Both identities are required: a
    /// request that names only one is refused rather than defaulted.
    pub fn from_headers(headers: &HeaderMap) -> Result<Self, BifrostError> {
        let read = |name: &str| -> Result<Uuid, BifrostError> {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| Uuid::parse_str(value).ok())
                .ok_or(BifrostError::QueryExecutionFailed)
        };
        Ok(Self::new(
            PublicQueryId::from_uuid(read(PUBLIC_QUERY_ID_HEADER)?),
            DataFusionQueryId::from_uuid(read(DATAFUSION_QUERY_ID_HEADER)?),
        ))
    }
}

/// The query-owned execution material a follower must install for one graph.
///
/// Every value here is derived from the grant the leader already admitted for
/// this query. A follower that cannot resolve one of these fails closed rather
/// than falling back to a process-wide default.
#[derive(Clone)]
pub struct AnalyticalGraphRuntime {
    /// The query's own `RuntimeEnv`: its admitted memory pool and the disk
    /// manager bounded by its admitted scratch share.
    runtime: Arc<RuntimeEnv>,
    /// Bytes the query's live exchange connection buffers may hold, charged as
    /// a child of the query's memory grant rather than as new capacity.
    exchange_buffer_budget_bytes: usize,
}

impl fmt::Debug for AnalyticalGraphRuntime {
    /// Reports the budget without rendering the pool's internal accounting.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalGraphRuntime")
            .field(
                "exchange_buffer_budget_bytes",
                &self.exchange_buffer_budget_bytes,
            )
            .finish_non_exhaustive()
    }
}

impl AnalyticalGraphRuntime {
    /// Names the query-owned runtime and exchange budget for one graph.
    #[must_use]
    pub fn new(runtime: Arc<RuntimeEnv>, exchange_buffer_budget_bytes: usize) -> Self {
        Self {
            runtime,
            exchange_buffer_budget_bytes,
        }
    }

    /// Returns the query-owned runtime installed on follower descendants.
    #[must_use]
    pub fn runtime(&self) -> &Arc<RuntimeEnv> {
        &self.runtime
    }

    /// Returns the bytes live exchange buffers may retain for this graph.
    #[must_use]
    pub const fn exchange_buffer_budget_bytes(&self) -> usize {
        self.exchange_buffer_budget_bytes
    }
}

/// The node-local map from an authenticated graph identity to its query-owned
/// execution material.
///
/// A follower registers an entry only after a stage operation's ticket has been
/// verified and its reservation resolved; the registry itself is not an
/// authority. Removing the entry is what makes a graph's follower work
/// unresolvable, which is how attempt invalidation becomes observable to
/// upstream session construction.
#[derive(Debug, Default)]
pub struct AnalyticalRuntimeRegistry {
    /// Live graphs and the query-owned material each one installs.
    entries: Mutex<HashMap<AnalyticalGraphKey, AnalyticalGraphRuntime>>,
}

impl AnalyticalRuntimeRegistry {
    /// Creates an empty node-local registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the query-owned material for one authorized graph.
    ///
    /// Re-registering the same graph replaces its material, which is what an
    /// authenticated retry does after attempt zero has fully drained.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the registry lock is poisoned,
    /// which fails the operation closed rather than executing without a
    /// query-owned runtime.
    pub fn register(
        &self,
        key: AnalyticalGraphKey,
        runtime: AnalyticalGraphRuntime,
    ) -> Result<(), BifrostError> {
        let mut entries = self.entries.lock().map_err(|_| poisoned_registry())?;
        entries.insert(key, runtime);
        Ok(())
    }

    /// Resolves the query-owned material for one graph.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the graph is not
    /// registered — an invalidated attempt, a sibling graph, or a forged
    /// identity — and [`BifrostError::Internal`] on lock poisoning.
    pub fn resolve(&self, key: AnalyticalGraphKey) -> Result<AnalyticalGraphRuntime, BifrostError> {
        let entries = self.entries.lock().map_err(|_| poisoned_registry())?;
        entries
            .get(&key)
            .cloned()
            .ok_or(BifrostError::QueryExecutionFailed)
    }

    /// Removes one graph's material, making its follower work unresolvable.
    ///
    /// Returns whether an entry was present, so a caller can distinguish a
    /// first invalidation from a repeated one.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] on lock poisoning.
    pub fn invalidate(&self, key: AnalyticalGraphKey) -> Result<bool, BifrostError> {
        let mut entries = self.entries.lock().map_err(|_| poisoned_registry())?;
        Ok(entries.remove(&key).is_some())
    }

    /// Returns the number of graphs currently holding query-owned material.
    ///
    /// Terminal-cleanup evidence asserts this reaches zero on every node.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] on lock poisoning.
    pub fn len(&self) -> Result<usize, BifrostError> {
        let entries = self.entries.lock().map_err(|_| poisoned_registry())?;
        Ok(entries.len())
    }

    /// Returns whether no graph currently holds query-owned material.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] on lock poisoning.
    pub fn is_empty(&self) -> Result<bool, BifrostError> {
        Ok(self.len()? == 0)
    }
}

/// Builds the stable internal error for a poisoned registry lock.
fn poisoned_registry() -> BifrostError {
    BifrostError::Internal {
        detail: "Oracle analytical runtime registry lock is poisoned".to_owned(),
    }
}

/// Installs the query-owned runtime on every follower session for a graph.
///
/// This is Wyrd's implementation of the upstream [`WorkerSessionBuilder`] seam.
/// Upstream hands it a builder already carrying the *worker's* process-lifetime
/// runtime; this builder replaces that runtime with the one the leader admitted
/// for this exact query, so every descendant operator on the follower — including
/// spilling operators — charges the query's pool and writes into the query's
/// bounded scratch.
///
/// A graph that is not registered is refused. There is deliberately no fallback
/// to the process runtime: an unresolvable graph is an invalidated attempt, a
/// sibling graph, or a forged identity, and none of those may execute.
pub struct AnalyticalSessionBuilder {
    /// Node-local graph material resolved per stage operation.
    registry: Arc<AnalyticalRuntimeRegistry>,
}

impl fmt::Debug for AnalyticalSessionBuilder {
    /// Renders the builder without exposing live graph identities.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalSessionBuilder")
            .finish_non_exhaustive()
    }
}

impl AnalyticalSessionBuilder {
    /// Binds the builder to one node-local runtime registry.
    #[must_use]
    pub fn new(registry: Arc<AnalyticalRuntimeRegistry>) -> Self {
        Self { registry }
    }

    /// Resolves one graph's material from a stage operation's headers.
    ///
    /// Kept synchronous and separate from [`WorkerSessionBuilder`] so the
    /// identity-to-runtime decision can be exercised without an upstream
    /// session at all.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when either identity
    /// header is absent or malformed, or when the named graph holds no
    /// registered material.
    pub fn resolve_headers(
        &self,
        headers: &HeaderMap,
    ) -> Result<AnalyticalGraphRuntime, BifrostError> {
        self.registry
            .resolve(AnalyticalGraphKey::from_headers(headers)?)
    }
}

#[async_trait]
impl WorkerSessionBuilder for AnalyticalSessionBuilder {
    /// Replaces the worker's process runtime with this query's own runtime.
    ///
    /// # Errors
    ///
    /// Returns [`DataFusionError::Execution`] when the stage operation's
    /// headers do not resolve to a registered graph. Upstream surfaces this as
    /// the task's terminal error, so no partition of an unresolvable graph ever
    /// executes.
    async fn build_session_state(
        &self,
        ctx: WorkerQueryContext,
    ) -> Result<SessionState, DataFusionError> {
        let graph = self.resolve_headers(&ctx.headers).map_err(|error| {
            DataFusionError::Execution(format!(
                "Oracle analytical stage has no query-owned runtime: {error}"
            ))
        })?;
        Ok(ctx
            .builder
            .with_runtime_env(Arc::clone(graph.runtime()))
            .build())
    }
}

/// Complete construction inputs for one follower [`AnalyticalStageIngress`].
///
/// Naming the dependencies keeps the two shared owners — the server authority
/// and the node supervisor — from being transposable at a call site, and keeps
/// the resource root and spill owner explicit rather than reachable through a
/// process global.
pub struct AnalyticalStageIngressConfig {
    /// Server-owned authority every stage operation is checked against.
    pub authority: Arc<dyn OracleStageAuthority>,
    /// Node-local supervisor owning graphs, attempts, and the runtime registry.
    pub supervisor: Arc<AnalyticalSupervisor>,
    /// Root Oracle capability this follower admits query envelopes from.
    pub oracle_resources: OracleResources,
    /// Process spill owner that bounds each query runtime's disk manager.
    pub spill: Arc<OracleSpillRuntime>,
    /// Exchange-buffer child every attempt of a graph on this node charges.
    pub exchange_buffer_bytes: usize,
}

/// Authenticated follower entry point for Analytical stage operations.
///
/// This is the node-local owner that turns an authenticated stage operation
/// into supervised distributed work. The ordering it enforces is the whole
/// point of the type: nothing decodes a plan, reads the task cache, constructs
/// a provider, or touches storage until
/// [`OracleStageAuthority::authorize_stage`] has returned, and the headers the
/// upstream worker resolves its runtime from are derived from the *verified*
/// claims rather than from anything the caller supplied.
///
/// The ingress owns the follower's graph and attempt guards because a graph
/// spans several separate stage calls: it is registered on the first `SetPlan`
/// that names it and released when the graph is torn down, not when any one
/// call returns.
pub struct AnalyticalStageIngress {
    /// Server-owned authority every stage operation is checked against.
    authority: Arc<dyn OracleStageAuthority>,
    /// Node-local supervisor owning graphs, attempts, and the runtime registry.
    supervisor: Arc<AnalyticalSupervisor>,
    /// Root Oracle capability this follower admits query envelopes from.
    oracle_resources: OracleResources,
    /// Process spill owner that bounds each query runtime's disk manager.
    spill: Arc<OracleSpillRuntime>,
    /// Exchange-buffer child every attempt of a graph on this node charges.
    exchange_buffer_bytes: usize,
    /// Upstream worker whose sessions install this node's query-owned runtimes.
    worker: Worker,
    /// Graph ownership tokens held for as long as the coordinator may address them.
    graphs: Mutex<HashMap<AnalyticalGraphKey, AnalyticalGraphGuard>>,
    /// Attempt ownership tokens held for as long as their stage work may run.
    attempts: Mutex<HashMap<AnalyticalAttemptKey, AnalyticalAttemptGuard>>,
}

impl fmt::Debug for AnalyticalStageIngress {
    /// Reports the configured budget without rendering owned dependencies.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalStageIngress")
            .field("exchange_buffer_bytes", &self.exchange_buffer_bytes)
            .finish_non_exhaustive()
    }
}

impl AnalyticalStageIngress {
    /// Builds the follower ingress over the supervisor's own runtime registry.
    ///
    /// The upstream worker is constructed from [`AnalyticalSessionBuilder`] over
    /// that exact registry, so a stage whose graph is not registered — an
    /// invalidated attempt, a sibling graph, a forged identity — fails to build
    /// a session rather than silently falling back to a process runtime.
    #[must_use]
    pub fn new(config: AnalyticalStageIngressConfig) -> Self {
        let AnalyticalStageIngressConfig {
            authority,
            supervisor,
            oracle_resources,
            spill,
            exchange_buffer_bytes,
        } = config;
        let worker = Worker::from_session_builder(AnalyticalSessionBuilder::new(Arc::clone(
            supervisor.registry(),
        )));
        Self {
            authority,
            supervisor,
            oracle_resources,
            spill,
            exchange_buffer_bytes,
            worker,
            graphs: Mutex::new(HashMap::new()),
            attempts: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the upstream worker for local-worker context composition.
    #[must_use]
    pub fn worker(&self) -> &Worker {
        &self.worker
    }

    /// Authorizes and installs one stage subplan, opening its coordinator channel.
    ///
    /// `plan_bytes` are the exact encoded subplan the digest in `ticket` commits
    /// to; they are the bytes upstream will decode, and they are not decoded
    /// here. On the first authorized operation naming a graph this admits the
    /// follower's own query envelope, builds the query-owned runtime bounded by
    /// that envelope's scratch share, registers the graph, and admits the
    /// attempt — all before any plan is touched.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryPeerSecurity`] when authorization fails for
    /// any reason, [`BifrostError::QueryAuditUnavailable`] when the required
    /// refusal audit could not commit, [`BifrostError::QueryAdmissionRejected`]
    /// when this follower cannot admit the graph's envelope, and
    /// [`BifrostError::QueryExecutionFailed`] when upstream refuses the plan.
    pub async fn set_plan(
        &self,
        ticket: &SignedPeerTicket,
        binding: &StageBinding,
        request: SetPlanRequest,
        plan_bytes: &[u8],
        coordinator_stream: BoxStream<'static, Result<CoordinatorToWorkerMsg, DataFusionError>>,
        now: DateTime<Utc>,
    ) -> Result<BoxStream<'static, Result<WorkerToCoordinatorMsg, DataFusionError>>, BifrostError>
    {
        let authorized = self
            .authorize(StageOperationV1::SetPlan, ticket, binding, plan_bytes, now)
            .await?;
        let key = attempt_key(&authorized)?;
        self.admit_graph(key.graph())?;
        self.admit_attempt(key)?;
        record_stage_operation(AnalyticalStageOperation::SetPlan);
        self.worker
            .coordinator_channel(key.graph().to_headers(), request, coordinator_stream)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    public_query_id = %key.public_query_id,
                    datafusion_query_id = %key.datafusion_query_id,
                    stage = %key.stage,
                    "Oracle analytical stage plan installation failed"
                );
                BifrostError::QueryExecutionFailed
            })
    }

    /// Authorizes and executes one stage task's partition range.
    ///
    /// The returned streams and task context are the caller's to retain, cancel,
    /// and join; nothing is detached here.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryPeerSecurity`] when authorization fails,
    /// [`BifrostError::QueryAuditUnavailable`] when the refusal audit could not
    /// commit, and [`BifrostError::QueryExecutionFailed`] when the graph is not
    /// registered or upstream refuses the partition range.
    pub async fn execute_task(
        &self,
        ticket: &SignedPeerTicket,
        binding: &StageBinding,
        request: ExecuteTaskRequest,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(Vec<SendableRecordBatchStream>, Arc<TaskContext>), BifrostError> {
        let authorized = self
            .authorize(StageOperationV1::ExecuteTask, ticket, binding, body, now)
            .await?;
        let key = attempt_key(&authorized)?;
        self.supervisor.graph_runtime(key.graph())?;
        record_stage_operation(AnalyticalStageOperation::ExecuteTask);
        self.worker.execute_task(request).await.map_err(|error| {
            tracing::warn!(
                %error,
                public_query_id = %key.public_query_id,
                datafusion_query_id = %key.datafusion_query_id,
                stage = %key.stage,
                task = key.task.map(TaskId::as_usize),
                "Oracle analytical stage task execution failed"
            );
            BifrostError::QueryExecutionFailed
        })
    }

    /// Settles one attempt and releases the graph once its last attempt drains.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned,
    /// and the supervisor's refusal when `key` names no live attempt.
    pub async fn finish_attempt(
        &self,
        key: AnalyticalAttemptKey,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<(), BifrostError> {
        let guard = {
            let mut attempts = self.attempts.lock().map_err(|_| poisoned_ingress())?;
            attempts.remove(&key)
        };
        match guard {
            Some(guard) => {
                guard.finish(outcome).await?;
            }
            None => return Err(BifrostError::QueryExecutionFailed),
        }
        let graph = key.graph();
        let release = {
            let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
            let attempts = self.attempts.lock().map_err(|_| poisoned_ingress())?;
            if attempts.keys().any(|live| live.graph() == graph) {
                None
            } else {
                graphs.remove(&graph)
            }
        };
        if let Some(release) = release {
            release.release()?;
        }
        Ok(())
    }

    /// Releases every graph and attempt this follower still owns.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned.
    pub async fn shutdown(&self) -> Result<AnalyticalSupervisorInspection, BifrostError> {
        {
            let mut attempts = self.attempts.lock().map_err(|_| poisoned_ingress())?;
            attempts.clear();
        }
        {
            let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
            graphs.clear();
        }
        self.supervisor.shutdown().await
    }

    /// Runs the complete authority check for one stage operation.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAuditUnavailable`] when the durable refusal
    /// record could not commit, and [`BifrostError::QueryPeerSecurity`] for
    /// every other refusal. A refusal never returns claims.
    async fn authorize(
        &self,
        operation: StageOperationV1,
        ticket: &SignedPeerTicket,
        binding: &StageBinding,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AuthorizedStage, BifrostError> {
        if binding.operation != operation {
            return Err(BifrostError::QueryPeerSecurity);
        }
        self.authority
            .authorize_stage(ticket, binding, body, now)
            .await
            .map_err(|error| match error {
                PeerSecurityError::AuditUnavailable => BifrostError::QueryAuditUnavailable,
                _ => BifrostError::QueryPeerSecurity,
            })
    }

    /// Admits this follower's own query envelope for a graph it has not seen.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when the root capability
    /// cannot admit an analytical query envelope, [`BifrostError::Internal`] on
    /// a poisoned ownership lock or when the graph runtime cannot be built.
    fn admit_graph(&self, graph: AnalyticalGraphKey) -> Result<(), BifrostError> {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
        if graphs.contains_key(&graph) {
            return Ok(());
        }
        let resources = self
            .oracle_resources
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .map_err(|_| BifrostError::QueryAdmissionRejected)?;
        let runtime = self
            .spill
            .build_query_runtime(resources.memory_pool(), resources.scratch_bytes)?;
        let guard = self.supervisor.register_graph(
            graph,
            resources,
            AnalyticalGraphRuntime::new(runtime, self.exchange_buffer_bytes),
        )?;
        graphs.insert(graph, guard);
        Ok(())
    }

    /// Admits one attempt of an already registered graph and retains its guard.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] on a poisoned ownership lock, and the
    /// supervisor's refusal when the slot is occupied or the graph is unknown.
    fn admit_attempt(&self, key: AnalyticalAttemptKey) -> Result<(), BifrostError> {
        let mut attempts = self.attempts.lock().map_err(|_| poisoned_ingress())?;
        if attempts.contains_key(&key) {
            return Ok(());
        }
        let guard = self.supervisor.spawn_attempt(
            key,
            AnalyticalAttemptGrant {
                exchange_buffer_bytes: self.exchange_buffer_bytes,
                scratch_bytes: 0,
            },
        )?;
        attempts.insert(key, guard);
        Ok(())
    }
}

/// Projects the supervised attempt identity from one authorized stage operation.
///
/// Every field comes from the verified claims, never from the wire framing, so
/// a graph, stage, task, or attempt the signature did not cover cannot be
/// addressed.
///
/// # Errors
///
/// Returns [`BifrostError::QueryPeerSecurity`] when the verified claims carry an
/// identity or attempt ordinal that is not representable — a forged third
/// attempt among them.
fn attempt_key(authorized: &AuthorizedStage) -> Result<AnalyticalAttemptKey, BifrostError> {
    let claims = &authorized.claims;
    let public_query_id =
        Uuid::from_slice(&claims.public_query_id).map_err(|_| BifrostError::QueryPeerSecurity)?;
    let datafusion_query_id = Uuid::from_slice(&claims.datafusion_query_id)
        .map_err(|_| BifrostError::QueryPeerSecurity)?;
    let attempt = u8::try_from(claims.attempt)
        .ok()
        .and_then(AnalyticalAttemptNumber::from_u8)
        .ok_or(BifrostError::QueryPeerSecurity)?;
    let stage = StageId::new(
        usize::try_from(claims.stage_id).map_err(|_| BifrostError::QueryPeerSecurity)?,
    );
    let task = if claims.has_task {
        Some(TaskId::new(
            usize::try_from(claims.task_id).map_err(|_| BifrostError::QueryPeerSecurity)?,
        ))
    } else {
        None
    };
    Ok(AnalyticalAttemptKey::new(
        PublicQueryId::from_uuid(public_query_id),
        DataFusionQueryId::from_uuid(datafusion_query_id),
        stage,
        task,
        attempt,
    ))
}

/// Builds the stable internal error for a poisoned ingress ownership lock.
fn poisoned_ingress() -> BifrostError {
    BifrostError::Internal {
        detail: "Oracle analytical stage ingress lock is poisoned".to_owned(),
    }
}

/// Rebuilds a follower session over one graph's material without upstream.
///
/// Used by the extension-surface proof so the *same* installation logic can be
/// checked against a plain [`SessionStateBuilder`], which is what makes a
/// regression in [`AnalyticalSessionBuilder`] attributable to Wyrd rather than
/// to an upstream change.
#[cfg(test)]
fn install_graph_runtime(
    builder: datafusion::execution::SessionStateBuilder,
    graph: &AnalyticalGraphRuntime,
) -> SessionState {
    builder
        .with_runtime_env(Arc::clone(graph.runtime()))
        .build()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
    use arrow::record_batch::RecordBatch;
    use datafusion::common::Result as DataFusionResult;
    use datafusion::common::tree_node::TreeNodeRecursion;
    use datafusion::datasource::memory::MemorySourceConfig;
    use datafusion::execution::SessionStateBuilder;
    use datafusion::execution::memory_pool::GreedyMemoryPool;
    use datafusion::execution::runtime_env::RuntimeEnvBuilder;
    use datafusion::execution::{SendableRecordBatchStream, TaskContext};
    use datafusion::physical_expr::{EquivalenceProperties, Partitioning, PhysicalExpr};
    use datafusion::physical_plan::execution_plan::{Boundedness, EmissionType, SchedulingType};
    use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
    use datafusion::physical_plan::{
        DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, PlanProperties,
    };
    use datafusion_distributed::{
        ExecuteTaskRequest, MaybeEncoded, ProducerHead, SetPlanRequest, TaskKey, Worker,
    };
    use futures_util::StreamExt;
    use futures_util::stream;
    use url::Url;

    use super::*;

    /// Counts batches only as the *caller* drives the returned stream.
    ///
    /// The upstream worker returns its per-partition streams to the caller
    /// instead of spawning them, so nothing this operator produces may be
    /// observable until the caller polls. The counter makes that difference
    /// visible: if upstream ever detached the work, the pre-poll assertion in
    /// the extension proof would see a nonzero count.
    #[derive(Debug)]
    struct PollObservedExec {
        /// The wrapped source whose batches are counted.
        input: Arc<dyn ExecutionPlan>,
        /// Batches yielded to the caller so far, across all partitions.
        yielded: Arc<AtomicUsize>,
        /// Cached properties mirroring the wrapped source.
        properties: Arc<PlanProperties>,
    }

    impl PollObservedExec {
        /// Wraps one source with a caller-driven batch counter.
        fn new(input: Arc<dyn ExecutionPlan>, yielded: Arc<AtomicUsize>) -> Self {
            let partitions = input.output_partitioning().partition_count();
            let properties = Arc::new(
                PlanProperties::new(
                    EquivalenceProperties::new(input.schema()),
                    Partitioning::UnknownPartitioning(partitions),
                    EmissionType::Incremental,
                    Boundedness::Bounded,
                )
                .with_scheduling_type(SchedulingType::Cooperative),
            );
            Self {
                input,
                yielded,
                properties,
            }
        }
    }

    impl DisplayAs for PollObservedExec {
        /// Renders the counting boundary.
        fn fmt_as(
            &self,
            _format: DisplayFormatType,
            formatter: &mut fmt::Formatter<'_>,
        ) -> fmt::Result {
            write!(formatter, "PollObservedExec")
        }
    }

    impl ExecutionPlan for PollObservedExec {
        /// Owns no physical expression.
        ///
        /// # Errors
        /// Never returns an error; the signature is fixed by the trait.
        fn apply_expressions(
            &self,
            _f: &mut dyn FnMut(&Arc<dyn PhysicalExpr>) -> DataFusionResult<TreeNodeRecursion>,
        ) -> DataFusionResult<TreeNodeRecursion> {
            Ok(TreeNodeRecursion::Continue)
        }

        /// Returns the stable operator name.
        fn name(&self) -> &'static str {
            "PollObservedExec"
        }

        /// Returns the cached properties mirroring the wrapped source.
        fn properties(&self) -> &Arc<PlanProperties> {
            &self.properties
        }

        /// Returns the wrapped source as the sole child.
        fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
            vec![&self.input]
        }

        /// Rebuilds the counter around exactly one replacement child.
        ///
        /// # Errors
        /// Returns a plan error unless exactly one child is supplied.
        fn with_new_children(
            self: Arc<Self>,
            children: Vec<Arc<dyn ExecutionPlan>>,
        ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
            let [input] = children.try_into().map_err(|_| {
                DataFusionError::Plan("PollObservedExec requires one child".to_owned())
            })?;
            Ok(Arc::new(Self::new(input, Arc::clone(&self.yielded))))
        }

        /// Returns a stream that counts a batch only when the caller pulls it.
        ///
        /// # Errors
        /// Returns whatever the wrapped source's execution returns.
        fn execute(
            &self,
            partition: usize,
            task: Arc<TaskContext>,
        ) -> DataFusionResult<SendableRecordBatchStream> {
            let schema = self.schema();
            let yielded = Arc::clone(&self.yielded);
            let input = self.input.execute(partition, task)?;
            let counted = input.map(move |batch| {
                if batch.is_ok() {
                    yielded.fetch_add(1, Ordering::SeqCst);
                }
                batch
            });
            Ok(Box::pin(RecordBatchStreamAdapter::new(schema, counted)))
        }
    }

    /// Builds the two-row fixture batch and its schema.
    fn fixture_batch() -> (SchemaRef, RecordBatch) {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![7_i64, 9]))],
        )
        .expect("the fixture batch matches its own schema");
        (schema, batch)
    }

    /// Builds a single-partition in-memory source wrapped in the poll counter.
    fn fixture_plan(yielded: Arc<AtomicUsize>) -> Arc<dyn ExecutionPlan> {
        let (schema, batch) = fixture_batch();
        let source = MemorySourceConfig::try_new_exec(&[vec![batch]], schema, None)
            .expect("a single-partition memory source accepts one matching batch");
        Arc::new(PollObservedExec::new(source, yielded))
    }

    /// Builds a query-owned runtime distinguishable from any process default.
    fn query_owned_runtime() -> Arc<RuntimeEnv> {
        RuntimeEnvBuilder::new()
            .with_memory_pool(Arc::new(GreedyMemoryPool::new(64 * 1024 * 1024)))
            .build()
            .map(Arc::new)
            .expect("a bounded query runtime is constructible")
    }

    /// Builds the upstream task key naming one stage task of a Wyrd graph.
    ///
    /// The upstream cache is keyed by a bare UUID, and this is the only value
    /// Wyrd ever supplies for it, so two Wyrd graphs can never collide there.
    fn task_key_for(key: AnalyticalGraphKey) -> TaskKey {
        TaskKey {
            query_id: key.datafusion_query_id.as_uuid(),
            stage_id: 0,
            task_number: 0,
        }
    }

    /// Builds the upstream set-plan request for one graph and fixture plan.
    fn set_plan_request(key: AnalyticalGraphKey, plan: Arc<dyn ExecutionPlan>) -> SetPlanRequest {
        SetPlanRequest {
            task_key: task_key_for(key),
            task_count: 1,
            plan: MaybeEncoded::Decoded(plan),
            work_unit_feed_declarations: Vec::new(),
            target_worker_url: Url::parse("http://follower.invalid")
                .expect("the fixture worker URL parses"),
            query_start_time_ns: 0,
        }
    }

    /// Drains caller-owned partition streams into the fixture's row values.
    ///
    /// # Panics
    ///
    /// Panics when a stream fails or its column is not the fixture's Int64.
    async fn collect_fixture_rows(streams: Vec<SendableRecordBatchStream>) -> Vec<i64> {
        let mut rows = Vec::new();
        for stream in streams {
            let batches = stream
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<DataFusionResult<Vec<_>>>()
                .expect("the caller-owned stream yields the fixture batches");
            for batch in batches {
                let column = batch
                    .column_by_name("value")
                    .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
                    .expect("the fixture column decodes as a native Arrow Int64 array");
                rows.extend_from_slice(column.values());
            }
        }
        rows
    }

    /// The pinned upstream worker retains all three seams Wyrd's handle needs.
    ///
    /// This is the feasibility gate for the whole Analytical path. It fails if
    /// upstream ever stops handing Wyrd:
    ///
    /// 1. the stage operation's **headers**, before a plan is decoded, so a
    ///    Wyrd-signed ticket can be the authority for follower work;
    /// 2. a **session-construction hook** whose runtime wins over the worker's
    ///    process runtime, so every follower descendant charges the query's own
    ///    memory pool and scratch; and
    /// 3. the task's **streams and task context returned to the caller** rather
    ///    than detached, so Wyrd's supervisor can join every descendant.
    ///
    /// # Panics
    ///
    /// Panics when any of the three seams no longer holds, which means the
    /// pinned upstream revision can no longer host Wyrd's Analytical path.
    #[tokio::test]
    async fn upstream_stage_hooks_retain_authority_runtime_and_futures() {
        let registry = Arc::new(AnalyticalRuntimeRegistry::new());
        let key = AnalyticalGraphKey::new(
            PublicQueryId::from_uuid(Uuid::new_v4()),
            DataFusionQueryId::allocate(),
        );
        let query_runtime = query_owned_runtime();
        let graph = AnalyticalGraphRuntime::new(Arc::clone(&query_runtime), 4 * 1024 * 1024);
        registry
            .register(key, graph.clone())
            .expect("the authorized graph registers its query-owned material");

        // Seam 2 preconditions: the worker's process runtime is a *different*
        // owner, so a passing assertion below cannot be an accident.
        let process_runtime = Arc::new(RuntimeEnv::default());
        assert!(
            !Arc::ptr_eq(&process_runtime, &query_runtime),
            "the fixture must distinguish the process runtime from the query runtime"
        );
        let worker =
            Worker::from_session_builder(AnalyticalSessionBuilder::new(Arc::clone(&registry)))
                .with_runtime_env(Arc::clone(&process_runtime));

        // Seam 1: Wyrd's own headers are the only thing that names the graph,
        // and they reach the worker before the plan is decoded.
        let yielded = Arc::new(AtomicUsize::new(0));
        let metrics_stream = worker
            .coordinator_channel(
                key.to_headers(),
                set_plan_request(key, fixture_plan(Arc::clone(&yielded))),
                stream::empty().boxed(),
            )
            .await
            .expect("the authorized stage sets its plan");

        let (streams, task_ctx) = worker
            .execute_task(ExecuteTaskRequest {
                task_key: task_key_for(key),
                target_partition_start: 0,
                target_partition_end: 1,
                producer_head: ProducerHead::None,
            })
            .await
            .expect("the authorized stage executes its task");

        // Seam 2: the follower descendant's task context carries the *query's*
        // runtime, not the worker's process runtime.
        assert!(
            Arc::ptr_eq(&task_ctx.runtime_env(), &query_runtime),
            "the follower task must execute on the query-owned runtime"
        );
        assert!(
            !Arc::ptr_eq(&task_ctx.runtime_env(), &process_runtime),
            "the follower task must not fall back to the worker process runtime"
        );

        // Seam 3: the returned streams are the caller's. Nothing has been
        // produced yet, because nothing has polled them.
        assert_eq!(streams.len(), 1, "one partition was requested");
        assert_eq!(
            yielded.load(Ordering::SeqCst),
            0,
            "upstream must not detach the task; no batch may exist before the caller polls"
        );

        let rows = collect_fixture_rows(streams).await;
        assert_eq!(rows, vec![7, 9], "the caller drove the task to completion");
        assert_eq!(
            yielded.load(Ordering::SeqCst),
            1,
            "exactly the caller's polling produced the fixture batch"
        );
        drop(metrics_stream);

        // Seam 1, negative: an unregistered graph cannot execute at all. This
        // is what makes attempt invalidation and sibling-graph isolation
        // enforceable at the session boundary.
        let sibling = AnalyticalGraphKey::new(key.public_query_id, DataFusionQueryId::allocate());
        let sibling_error = worker
            .coordinator_channel(
                sibling.to_headers(),
                set_plan_request(sibling, fixture_plan(Arc::new(AtomicUsize::new(0)))),
                stream::empty().boxed(),
            )
            .await
            .err()
            .expect("a sibling graph under the same public query cannot execute");
        assert!(
            sibling_error.to_string().contains("no query-owned runtime"),
            "the refusal must name the missing query-owned runtime, got: {sibling_error}"
        );
    }

    /// Wyrd's own runtime installation is what wins, independent of upstream.
    ///
    /// Attributes a regression in the extension proof: if this passes while the
    /// proof fails, upstream changed; if both fail, Wyrd's installation broke.
    ///
    /// # Panics
    ///
    /// Panics when the installed runtime is not the query-owned one.
    #[test]
    fn analytical_graph_runtime_installation_overrides_the_process_runtime() {
        let query_runtime = query_owned_runtime();
        let graph = AnalyticalGraphRuntime::new(Arc::clone(&query_runtime), 1);
        let state = install_graph_runtime(
            SessionStateBuilder::new()
                .with_default_features()
                .with_runtime_env(Arc::new(RuntimeEnv::default())),
            &graph,
        );
        assert!(Arc::ptr_eq(state.runtime_env(), &query_runtime));
    }

    /// Both identities are required, and neither substitutes for the other.
    ///
    /// # Panics
    ///
    /// Panics when a half-named graph resolves or when the round trip loses an
    /// identity.
    #[test]
    fn analytical_graph_key_requires_both_query_identities() {
        let key = AnalyticalGraphKey::new(
            PublicQueryId::from_uuid(Uuid::new_v4()),
            DataFusionQueryId::allocate(),
        );
        let headers = key.to_headers();
        assert_eq!(
            AnalyticalGraphKey::from_headers(&headers).expect("both identities round-trip"),
            key
        );

        let mut public_only = HeaderMap::new();
        public_only.insert(
            PUBLIC_QUERY_ID_HEADER,
            key.public_query_id
                .to_string()
                .parse()
                .expect("a UUID is a valid header value"),
        );
        assert!(
            AnalyticalGraphKey::from_headers(&public_only).is_err(),
            "a stage operation naming only the public query is refused"
        );

        let mut private_only = HeaderMap::new();
        private_only.insert(
            DATAFUSION_QUERY_ID_HEADER,
            key.datafusion_query_id
                .to_string()
                .parse()
                .expect("a UUID is a valid header value"),
        );
        assert!(
            AnalyticalGraphKey::from_headers(&private_only).is_err(),
            "a stage operation naming only the private graph is refused"
        );
    }

    /// At most one retry exists, and it is unreachable a second time.
    ///
    /// # Panics
    ///
    /// Panics when a third attempt becomes representable.
    #[test]
    fn analytical_attempt_number_permits_exactly_one_retry() {
        assert_eq!(AnalyticalAttemptNumber::ZERO.as_u8(), 0);
        assert_eq!(
            AnalyticalAttemptNumber::ZERO.retry(),
            Some(AnalyticalAttemptNumber::ONE)
        );
        assert_eq!(AnalyticalAttemptNumber::ONE.retry(), None);
        assert_eq!(AnalyticalAttemptNumber::from_u8(2), None);
    }
}
