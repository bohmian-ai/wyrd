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
use datafusion::execution::{SendableRecordBatchStream, SessionState};
use datafusion::physical_plan::{ExecutionPlan, execute_stream};
use datafusion::prelude::SessionContext;
use datafusion_distributed::SessionStateBuilderExt as _;
use datafusion_distributed::{DistributedExt as _, WorkerResolver};
use datafusion_distributed::{Worker, WorkerQueryContext, WorkerSessionBuilder};
use http::HeaderMap;
use std::time::Instant;
use url::Url;
use uuid::Uuid;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::NodeId;

pub use super::analytical_supervisor::AnalyticalSupervisor;

use super::analytical_supervisor::{
    AnalyticalAttemptGrant, AnalyticalAttemptGuard, AnalyticalAttemptKey, AnalyticalAttemptRelease,
    AnalyticalGraphGuard, AnalyticalSupervisorInspection, StageId, TaskId,
};
use super::analytical_transport::{
    AnalyticalChannelResolver, AnalyticalCoordinatorIdentity, AnalyticalStageMinter,
    StageWireIdentity, read_ticket,
};
use super::participant_cut::OracleQueryAttemptCut;
use super::peer::{AuthorizedStage, OracleStageAuthority, PeerSecurityError, StageOperationV1};
use super::spill::OracleSpillRuntime;
use super::telemetry::{
    AnalyticalAttemptOutcome, AnalyticalStageOperation, record_stage_operation,
};
use super::{AuthorizedQueryContext, map_datafusion_error};
use crate::resources::OracleQueryResources;
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
    /// Capability an Analytical leaf needs to resolve its own source locally.
    leaf: super::codec::AnalyticalLeafBinding,
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
    pub fn new(
        registry: Arc<AnalyticalRuntimeRegistry>,
        leaf: super::codec::AnalyticalLeafBinding,
    ) -> Self {
        Self { registry, leaf }
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
        // Parquet pushdown, page indexes, and bloom filters are leader
        // decisions that a follower must repeat: the leader planned the stage
        // graph against them, and a follower that reads without them scans more
        // than the plan says it does. The session's own codec is installed here
        // too, because a leaf's assignment travels inside the plan and only
        // this codec knows how to rebuild it.
        let mut builder = ctx.builder;
        let mut config = builder.config().clone().unwrap_or_default();
        crate::resources::OracleSessionShape::apply(&mut config);
        config.set_distributed_user_codec(super::codec::OraclePhysicalExtensionCodec::analytical(
            self.leaf.clone(),
        ));
        builder = builder.with_config(config);
        Ok(builder
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
    /// This follower's own node identity, bound as every ticket's audience.
    pub node_id: NodeId,
    /// This follower's own current Oracle role fence.
    pub oracle_fence: u64,
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
    /// Capability every Analytical leaf decoded on this node resolves through.
    pub leaf: super::codec::AnalyticalLeafBinding,
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
    /// This follower's own node identity, bound as every ticket's audience.
    node_id: NodeId,
    /// This follower's own current Oracle role fence.
    oracle_fence: u64,
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
            node_id,
            oracle_fence,
            authority,
            supervisor,
            oracle_resources,
            spill,
            exchange_buffer_bytes,
            leaf,
        } = config;
        let worker = Worker::from_session_builder(AnalyticalSessionBuilder::new(
            Arc::clone(supervisor.registry()),
            leaf,
        ));
        Self {
            node_id,
            oracle_fence,
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

    /// Authorizes one governed stage message and admits the work it names.
    ///
    /// `framed_message` is the complete encoded gRPC message — five-byte prefix
    /// included — exactly as it arrived on the wire. It is the digest input and
    /// nothing else: it is not decoded here, and it does not establish peer
    /// identity, which peer mTLS and workload authentication already did.
    ///
    /// Everything downstream depends on this returning first. Nothing decodes a
    /// plan, consults the task cache, constructs a provider, admits a resource,
    /// or issues storage I/O until it has. On the first authorized `SetPlan`
    /// naming a graph this admits the follower's own analytical query envelope,
    /// builds a runtime whose disk manager is bounded by that envelope's scratch
    /// share, registers the graph, and admits the attempt.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryPeerSecurity`] for every authorization,
    /// identity, or binding failure, [`BifrostError::QueryAuditUnavailable`]
    /// when the required refusal audit could not commit,
    /// [`BifrostError::QueryAdmissionRejected`] when this follower cannot admit
    /// the graph's envelope.
    pub async fn authorize_stage_message(
        &self,
        operation: StageOperationV1,
        headers: &HeaderMap,
        framed_message: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AnalyticalAttemptKey, BifrostError> {
        let identity =
            StageWireIdentity::read(headers).map_err(|_| BifrostError::QueryPeerSecurity)?;
        let ticket = read_ticket(headers).map_err(|_| BifrostError::QueryPeerSecurity)?;
        let binding = identity.to_binding(operation, self.node_id, self.oracle_fence);
        let authorized = self
            .authority
            .authorize_stage(&ticket, &binding, framed_message, now)
            .await
            .map_err(|error| {
                tracing::warn!(
                    operation = ?operation,
                    error = %error,
                    "Oracle analytical stage authority refused a stage message"
                );
                match error {
                    PeerSecurityError::AuditUnavailable => BifrostError::QueryAuditUnavailable,
                    _ => BifrostError::QueryPeerSecurity,
                }
            })?;
        let key = attempt_key(&authorized)?;
        match operation {
            StageOperationV1::SetPlan => {
                self.admit_graph(key.graph())?;
                self.admit_attempt(key)?;
                record_stage_operation(AnalyticalStageOperation::SetPlan);
            }
            StageOperationV1::ExecuteTask => {
                // Graph admission, not attempt admission. Upstream sends its
                // plan on a spawned coordinator-channel task and lets
                // `Worker::execute_task` wait for that plan to arrive, so an
                // `ExecuteTask` legitimately reaches this follower before the
                // `SetPlan` that names the same graph. Requiring the graph to
                // already be registered would turn upstream's documented
                // ordering tolerance into a refusal race. The ticket has
                // already bound this graph, tenant, fence, and reservation, so
                // admitting the envelope here is the same authorized act
                // `SetPlan` performs; the attempt guard still waits for
                // `SetPlan`, which is the message that actually names one.
                self.admit_graph(key.graph())?;
                record_stage_operation(AnalyticalStageOperation::ExecuteTask);
            }
        }
        Ok(key)
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

    /// Reports what this follower still owns without releasing any of it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned.
    pub fn live(&self) -> Result<AnalyticalLiveOwnership, BifrostError> {
        Ok(AnalyticalLiveOwnership {
            attempts: self.attempts.lock().map_err(|_| poisoned_ingress())?.len(),
            graphs: self.graphs.lock().map_err(|_| poisoned_ingress())?.len(),
        })
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
    /// Builds the leaf binding a session fixture needs but never exercises.
    fn fixture_leaf_binding() -> super::super::codec::AnalyticalLeafBinding {
        super::super::codec::AnalyticalLeafBinding::new(
            wyrd_spec::vala::api::ClusterRole::Oracle,
            Arc::new(super::super::follower::UnresolvableSource),
        )
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
        let worker = Worker::from_session_builder(AnalyticalSessionBuilder::new(
            Arc::clone(&registry),
            fixture_leaf_binding(),
        ))
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

/// The exact worker set one Analytical attempt may place tasks on.
///
/// Upstream calls this during task-count assignment and again immediately
/// before execution. Both calls must see the *same* set: the attempt's
/// membership is the immutable [`OracleQueryAttemptCut`], never a live cluster
/// view, so a topology change during the attempt cannot move work onto a node
/// the attempt was never authorized to reach.
#[derive(Debug)]
pub struct AnalyticalWorkerResolver {
    /// Private endpoints of the attempt's ready Oracle participants.
    urls: Vec<Url>,
}

/// Oracle's own answer to "how many tasks should this scan stage run on?".
///
/// Upstream's built-in estimator derives a task count from the total Parquet
/// bytes a leaf will read, divided by a fixed per-partition byte budget. That
/// heuristic is wrong for Bifrost: Oracle has already frozen a participant cut
/// and already knows how many Oracle peers may legally execute this attempt and
/// how many scannable units the pinned cut holds. A byte heuristic would make
/// distribution a property of how much data happens to be in the table rather
/// than a property of the cut, so a correctly-planned distributed query over a
/// small table would silently collapse onto the leader.
///
/// The handler answers only for leaf nodes, matching the contract of upstream's
/// built-in estimator: answering for an inner node would override the task
/// count its children reconciled. Registered as a *custom* handler, it is
/// consulted before the built-in byte estimator and therefore replaces it.
struct AnalyticalCutTaskCount {
    /// Tasks one scan stage of this attempt may occupy, at least one.
    tasks: usize,
}

impl AnalyticalCutTaskCount {
    /// Derives the per-stage task count from the frozen cut and its pinned work.
    ///
    /// Parallelism is capped at one task per scannable unit, because splitting
    /// two fragments across six peers buys network hops and empty streams
    /// rather than throughput, and at the participant count, because a task
    /// cannot run on a peer that is not in the cut. A cut that pinned nothing
    /// scannable still yields one task so the empty plan executes normally.
    fn new(participants: usize, work_units: usize) -> Self {
        Self {
            tasks: participants.min(work_units).max(1),
        }
    }
}

#[async_trait]
impl datafusion_distributed::DesiredTaskCountHandler for AnalyticalCutTaskCount {
    /// Returns the cut-derived desired task count for every leaf node.
    ///
    /// # Errors
    ///
    /// Never fails: the count was validated when the attempt cut was frozen.
    async fn handle(
        &self,
        ev: datafusion_distributed::DesiredTaskCountEvent<'_>,
    ) -> Option<Result<datafusion_distributed::DesiredTaskCountEventResponse, DataFusionError>>
    {
        if !ev.plan.children().is_empty() {
            return None;
        }
        Some(Ok(
            datafusion_distributed::DesiredTaskCountEventResponse::desired(self.tasks),
        ))
    }
}

impl WorkerResolver for AnalyticalWorkerResolver {
    /// Returns the attempt's frozen worker set.
    ///
    /// # Errors
    ///
    /// Never fails: the set was validated when the attempt cut was frozen.
    fn get_urls(&self) -> Result<Vec<Url>, DataFusionError> {
        Ok(self.urls.clone())
    }
}

/// Node-scoped configuration for the inactive Analytical execution handle.
pub struct AnalyticalExecutionConfig {
    /// This node's own identity, minted as every ticket's source.
    pub node_id: NodeId,
    /// This node's own current Oracle role fence.
    pub oracle_fence: u64,
    /// Short acceptance window a minted stage ticket is valid for.
    pub ticket_ttl: chrono::Duration,
    /// Exchange-buffer child every attempt of a graph charges on this node.
    pub exchange_buffer_bytes: usize,
    /// Scratch child every attempt of a graph charges on this node.
    pub scratch_bytes: u64,
}

/// Complete inputs for one inactive Analytical execution.
///
/// The plan is already physical and already distributed: classification,
/// planning, and admission happened on the Interactive path before Analytical
/// was selected, and this owner neither re-plans nor re-admits. Its digest
/// travels with the request so a retry can prove it is re-executing the same
/// plan rather than a newly planned one.
pub struct AnalyticalExecutionRequest {
    /// Authenticated principal, tenant, and audit correlation.
    pub context: AuthorizedQueryContext,
    /// The one client-visible identity of this query.
    pub public_query_id: PublicQueryId,
    /// The private distributed-graph identity of this attempt.
    pub datafusion_query_id: DataFusionQueryId,
    /// Immutable membership and deadline every stage of the attempt shares.
    pub cut: OracleQueryAttemptCut,
    /// The distributed physical plan to execute.
    pub physical_plan: Arc<dyn ExecutionPlan>,
    /// Digest pinning the exact plan, identical across the one allowed retry.
    pub physical_plan_digest: [u8; 32],
    /// The query envelope already admitted for this execution.
    pub resources: OracleQueryResources,
    /// Absolute execution deadline, identical across the one allowed retry.
    pub deadline: Instant,
    /// Pinned snapshot digest of the attempt's immutable cut.
    pub snapshot_digest: String,
    /// Reservation this graph's follower work charges against.
    pub reservation_id: String,
    /// Digest of the leader-authorized permissions for this query.
    pub permission_digest: String,
}

/// What one inactive Analytical execution left behind once it drained.
///
/// Every field is observed *after* the attempt settled, so a nonzero retained
/// count is a leak rather than work still in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticalExecutionEvidence {
    /// Attempt ordinal that produced the returned stream.
    pub attempt: u32,
    /// Supervisor state observed after the attempt settled.
    pub supervisor: AnalyticalSupervisorInspection,
}

/// Live Analytical ownership one half of a node still holds.
///
/// Both counts are read under their owners' locks at one instant. They are the
/// terminal-cleanup assertion: a settled node reads zero for both, and a
/// non-zero count names exactly which half stranded something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyticalLiveOwnership {
    /// Attempts still supervised.
    pub attempts: usize,
    /// Graphs still holding a query-owned runtime and admitted envelope.
    pub graphs: usize,
}

impl AnalyticalLiveOwnership {
    /// Reports whether this half of the node retains nothing.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.attempts == 0 && self.graphs == 0
    }
}

/// Live Analytical ownership across both halves of one node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyticalLiveInspection {
    /// What this node still owns as a coordinator.
    pub leader: AnalyticalLiveOwnership,
    /// What this node still owns as a follower.
    pub follower: AnalyticalLiveOwnership,
}

impl AnalyticalLiveInspection {
    /// Reports whether the node retains no Analytical ownership at all.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.leader.is_clean() && self.follower.is_clean()
    }
}

/// What the handle owned at the moment it was shut down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticalShutdownInspection {
    /// Leader-side supervisor state released by shutdown.
    pub leader: AnalyticalSupervisorInspection,
    /// Follower-side ingress state released by shutdown.
    pub follower: AnalyticalSupervisorInspection,
}

/// Production-unreachable owner of one node's distributed Analytical execution.
///
/// Nothing in Oracle's routing constructs or calls this owner; it exists so the
/// distributed path can be proved end to end before it is ever selectable. It
/// composes the owners that already exist — the follower ingress, the stage
/// authority, the node supervisor, the spill owner, and Oracle telemetry — and
/// adds only the leader-side session and channel composition.
pub struct AnalyticalExecutionHandle {
    /// This node's own follower ingress, so a leader can also serve stages.
    worker: Arc<AnalyticalStageIngress>,
    /// Server-owned authority that mints every outbound stage ticket.
    authority: Arc<dyn OracleStageAuthority>,
    /// Node-local supervisor owning leader-side graphs and attempts.
    supervisor: Arc<AnalyticalSupervisor>,
    /// Process spill owner bounding every query runtime this handle builds.
    spill: Arc<OracleSpillRuntime>,
    /// Root Oracle capability this handle admits leader graph envelopes from.
    oracle_resources: OracleResources,
    /// Node-scoped identity and budget configuration.
    config: AnalyticalExecutionConfig,
    /// Capability every Analytical leaf this node encodes or decodes resolves through.
    leaf: super::codec::AnalyticalLeafBinding,
}

impl fmt::Debug for AnalyticalExecutionHandle {
    /// Reports node identity without rendering owned dependencies.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalExecutionHandle")
            .field("node_id", &self.config.node_id.as_uuid())
            .field("oracle_fence", &self.config.oracle_fence)
            .finish_non_exhaustive()
    }
}

impl AnalyticalExecutionHandle {
    /// Composes the handle over this node's existing Analytical owners.
    #[must_use]
    pub fn new(
        worker: Arc<AnalyticalStageIngress>,
        authority: Arc<dyn OracleStageAuthority>,
        supervisor: Arc<AnalyticalSupervisor>,
        spill: Arc<OracleSpillRuntime>,
        oracle_resources: OracleResources,
        config: AnalyticalExecutionConfig,
        leaf: super::codec::AnalyticalLeafBinding,
    ) -> Self {
        Self {
            worker,
            authority,
            supervisor,
            spill,
            oracle_resources,
            config,
            leaf,
        }
    }

    /// Reports whether this node can still admit Analytical work.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.supervisor.is_healthy()
    }

    /// Returns the follower ingress this node serves stage operations through.
    #[must_use]
    pub fn worker(&self) -> &Arc<AnalyticalStageIngress> {
        &self.worker
    }

    /// Returns this node's leader-side supervisor.
    #[must_use]
    pub fn supervisor(&self) -> &Arc<AnalyticalSupervisor> {
        &self.supervisor
    }

    /// Returns the exact child grant every attempt of this node is admitted with.
    ///
    /// A retry must be admitted with the same grant its predecessor held, so
    /// this is read from the node's configuration rather than chosen per call.
    #[must_use]
    pub const fn attempt_grant(&self) -> AnalyticalAttemptGrant {
        AnalyticalAttemptGrant {
            exchange_buffer_bytes: self.config.exchange_buffer_bytes,
            scratch_bytes: self.config.scratch_bytes,
        }
    }

    /// Reports what this node still owns, leader and follower, without releasing it.
    ///
    /// This is the terminal-cleanup probe: after every query on a node has
    /// settled, both halves must read zero. Unlike [`Self::shutdown`] it takes
    /// nothing away, so a journey can assert a clean node and then keep using it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned.
    pub fn live(&self) -> Result<AnalyticalLiveInspection, BifrostError> {
        Ok(AnalyticalLiveInspection {
            leader: AnalyticalLiveOwnership {
                attempts: self.supervisor.live_attempts()?,
                graphs: self.supervisor.live_graphs()?,
            },
            follower: self.worker.live()?,
        })
    }

    /// Executes one distributed physical plan without being reachable from routing.
    ///
    /// The whole point of this operation is what it installs before executing.
    /// The graph is registered on the supervisor with a runtime built from
    /// *this query's* admitted pool and scratch share, so every follower
    /// descendant resolves that runtime rather than a process-lifetime one. The
    /// attempt is then admitted, which splits the exchange-buffer and scratch
    /// children from the same envelope, and the channel resolver is bound to
    /// the attempt's frozen worker set so no stage can be placed on a node the
    /// cut never authorized.
    ///
    /// The returned stream is lazy. The graph and attempt guards travel with it
    /// and settle when it drains, so admission and the supervisor's accounting
    /// outlive the last batch rather than the last call.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the supervisor is shutting down
    /// or a participant endpoint is not a valid URL,
    /// [`BifrostError::QueryAdmissionRejected`] when the attempt's children
    /// cannot be split from the admitted envelope, and
    /// [`BifrostError::QueryExecutionFailed`] when `DataFusion` cannot build
    /// the query runtime or start the plan.
    pub async fn execute_inactive(
        &self,
        request: AnalyticalExecutionRequest,
    ) -> Result<AnalyticalExecution, BifrostError> {
        let graph = AnalyticalGraphKey::new(request.public_query_id, request.datafusion_query_id);
        let runtime = self.spill.build_query_runtime(
            request.resources.memory_pool(),
            request.resources.scratch_bytes,
        )?;
        let granted_memory_bytes = request.resources.granted_memory_bytes;
        let target_partitions = request.resources.target_partitions;
        let graph_guard = self.supervisor.register_graph(
            graph,
            request.resources,
            AnalyticalGraphRuntime::new(runtime, self.config.exchange_buffer_bytes),
        )?;
        let attempt_key = AnalyticalAttemptKey::new(
            request.public_query_id,
            request.datafusion_query_id,
            StageId::new(0),
            None,
            AnalyticalAttemptNumber::ZERO,
        );
        let attempt = self.supervisor.spawn_attempt(
            attempt_key,
            AnalyticalAttemptGrant {
                exchange_buffer_bytes: self.config.exchange_buffer_bytes,
                scratch_bytes: self.config.scratch_bytes,
            },
        )?;
        let session = self.leader_session(
            AnalyticalSessionInputs {
                context: &request.context,
                cut: &request.cut,
                snapshot_digest: &request.snapshot_digest,
                reservation_id: &request.reservation_id,
                permission_digest: &request.permission_digest,
                granted_memory_bytes,
                target_partitions,
                work_units: request.cut.oracles().len(),
            },
            graph,
        )?;
        let batches = execute_stream(Arc::clone(&request.physical_plan), session.task_ctx())
            .map_err(|error| map_datafusion_error(&error))?;
        record_stage_operation(AnalyticalStageOperation::SetPlan);
        Ok(AnalyticalExecution {
            batches,
            ownership: AnalyticalAttemptOwnership {
                graph: graph_guard,
                attempt,
            },
        })
    }

    /// Installs one inactive Analytical attempt and returns its leader session.
    ///
    /// This is the seam Oracle's raw-SQL harness leases through. Everything
    /// before it — validation, classification, the participant cut, providers,
    /// audit — is the production path unchanged; everything after it plans and
    /// executes distributed because of the three things installed here: the
    /// query-owned runtime resolved from the registered graph, the frozen
    /// worker set, and the signing channel resolver.
    ///
    /// The leader admits its own Analytical query envelope here, exactly as a
    /// follower does when it first sees a graph. The Interactive admission that
    /// carried the query this far is left untouched, so an inactive Analytical
    /// attempt charges a second envelope. That is deliberate for T1: the path
    /// is unreachable from routing, and sharing one envelope across both would
    /// mean reshaping production admission for a path production cannot select.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when the root
    /// capability cannot admit an Analytical envelope,
    /// [`BifrostError::Internal`] when the supervisor is shutting down or a
    /// participant endpoint is not a valid URL, and
    /// [`BifrostError::QueryExecutionFailed`] when `DataFusion` cannot build
    /// the bounded query runtime.
    pub fn lease_session(
        &self,
        attempt: &AnalyticalAttemptContext,
        cut: &OracleQueryAttemptCut,
        context: &AuthorizedQueryContext,
        work_units: usize,
    ) -> Result<(SessionContext, AnalyticalAttemptOwnership), BifrostError> {
        let graph = AnalyticalGraphKey::new(attempt.public_query_id, attempt.datafusion_query_id);
        let resources = self
            .oracle_resources
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                1.0,
            ))
            .map_err(|_| BifrostError::QueryAdmissionRejected)?;
        let granted_memory_bytes = resources.granted_memory_bytes;
        let target_partitions = resources.target_partitions;
        let runtime = self
            .spill
            .build_query_runtime(resources.memory_pool(), resources.scratch_bytes)?;
        let graph_guard = self.supervisor.register_graph(
            graph,
            resources,
            AnalyticalGraphRuntime::new(runtime, self.config.exchange_buffer_bytes),
        )?;
        let attempt_guard = self.supervisor.spawn_attempt(
            AnalyticalAttemptKey::new(
                attempt.public_query_id,
                attempt.datafusion_query_id,
                StageId::new(0),
                None,
                AnalyticalAttemptNumber::ZERO,
            ),
            AnalyticalAttemptGrant {
                exchange_buffer_bytes: self.config.exchange_buffer_bytes,
                scratch_bytes: self.config.scratch_bytes,
            },
        )?;
        let session = self.leader_session(
            AnalyticalSessionInputs {
                context,
                cut,
                snapshot_digest: &attempt.snapshot_digest,
                reservation_id: &attempt.reservation_id,
                permission_digest: &attempt.permission_digest,
                granted_memory_bytes,
                target_partitions,
                work_units,
            },
            graph,
        )?;
        Ok((
            session,
            AnalyticalAttemptOwnership {
                graph: graph_guard,
                attempt: attempt_guard,
            },
        ))
    }

    /// Releases every leader and follower owner this handle still holds.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned.
    pub async fn shutdown(self) -> Result<AnalyticalShutdownInspection, BifrostError> {
        let follower = self.worker.shutdown().await?;
        let leader = self.supervisor.shutdown().await?;
        Ok(AnalyticalShutdownInspection { leader, follower })
    }

    /// Builds the leader session one attempt plans and executes through.
    ///
    /// The session carries three things nothing else installs: the query-owned
    /// runtime resolved from the registered graph, the frozen worker set, and
    /// the signing channel resolver bound to this query's coordinator identity.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a participant endpoint is not a
    /// valid URL or the graph's runtime is not registered.
    fn leader_session(
        &self,
        inputs: AnalyticalSessionInputs<'_>,
        graph: AnalyticalGraphKey,
    ) -> Result<SessionContext, BifrostError> {
        let AnalyticalSessionInputs {
            context,
            cut,
            snapshot_digest,
            reservation_id,
            permission_digest,
            granted_memory_bytes,
            target_partitions,
            work_units,
        } = inputs;
        let runtime = self.supervisor.graph_runtime(graph)?;
        let identity = Arc::new(AnalyticalCoordinatorIdentity {
            source_node_id: self.config.node_id,
            source_fence: self.config.oracle_fence,
            tenant_id: context.data_tenant_id,
            graph,
            snapshot_digest: snapshot_digest.to_owned(),
            attempt: u32::from(AnalyticalAttemptNumber::ZERO.as_u8()),
            reservation_id: reservation_id.to_owned(),
            permission_digest: permission_digest.to_owned(),
        });
        let destinations = self.destinations(cut)?;
        let urls = destinations.keys().cloned().collect::<Vec<_>>();
        let authority = Arc::clone(&self.authority);
        let deadline_ms = cut.deadline().timestamp_millis();
        let ticket_ttl = self.config.ticket_ttl;
        let resolver = AnalyticalChannelResolver::new(
            identity,
            Arc::new(move |url: &Url| {
                destinations.get(url).map(|(node_id, fence)| {
                    Arc::new(AnalyticalStageMinter::new(
                        Arc::clone(&authority),
                        *node_id,
                        *fence,
                        deadline_ms,
                        ticket_ttl,
                    ))
                })
            }),
        );
        let shape = crate::resources::OracleSessionShape::for_grant(
            granted_memory_bytes,
            target_partitions,
            work_units,
        );
        let mut config = shape.session_config();
        config.set_distributed_desired_task_count_handler(AnalyticalCutTaskCount::new(
            urls.len(),
            work_units,
        ));
        config.set_distributed_worker_resolver(AnalyticalWorkerResolver { urls });
        config.set_distributed_channel_resolver(resolver);
        config.set_distributed_user_codec(super::codec::OraclePhysicalExtensionCodec::analytical(
            self.leaf.clone(),
        ));
        let state = datafusion::execution::session_state::SessionStateBuilder::new()
            .with_default_features()
            .with_config(config)
            .with_runtime_env(Arc::clone(runtime.runtime()))
            .with_distributed_planner()
            .build();
        Ok(SessionContext::new_with_state(state))
    }

    /// Maps each remote participant endpoint to the fenced identity a ticket binds to.
    ///
    /// The coordinator excludes itself. A leader is already an Oracle in its own
    /// pinned cut, so keeping it in the worker set would make this node dispatch
    /// stage operations to its own ingress — which would admit a *second*
    /// Analytical query envelope for a query whose envelope this node already
    /// holds, and would ask the supervisor to register a graph it has already
    /// registered for the leader lease. A cut with no remote Oracle therefore
    /// yields an empty worker set, and the attempt executes entirely on the
    /// leader, which is the correct shape for a single-node deployment.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a participant endpoint is not a
    /// valid URL, which would otherwise leave a worker unreachable and unsigned.
    fn destinations(
        &self,
        cut: &OracleQueryAttemptCut,
    ) -> Result<HashMap<Url, (NodeId, u64)>, BifrostError> {
        cut.oracles()
            .iter()
            .filter(|participant| participant.node_id != self.config.node_id)
            .map(|participant| {
                let url =
                    Url::parse(&participant.endpoint).map_err(|error| BifrostError::Internal {
                        detail: format!(
                            "Oracle analytical participant endpoint is not a valid URL: {error}"
                        ),
                    })?;
                Ok((url, (participant.node_id, participant.fencing_token)))
            })
            .collect()
    }
}

/// The leader-session inputs one attempt composes its distributed session from.
///
/// These are read from the request before its admitted envelope is moved onto
/// the supervisor, which is why they travel as a group rather than as a
/// borrowed request.
struct AnalyticalSessionInputs<'a> {
    /// Authenticated principal, tenant, and audit correlation.
    context: &'a AuthorizedQueryContext,
    /// Immutable membership and deadline of the attempt.
    cut: &'a OracleQueryAttemptCut,
    /// Pinned snapshot digest of the attempt's cut.
    snapshot_digest: &'a str,
    /// Reservation the graph's follower work charges against.
    reservation_id: &'a str,
    /// Digest of the leader-authorized permissions for this query.
    permission_digest: &'a str,
    /// Ceiling this query's pool may grow to.
    granted_memory_bytes: usize,
    /// Query-local target partition count.
    target_partitions: usize,
    /// Scannable work the pinned cut offers, used to shape parallelism.
    work_units: usize,
}

/// The per-query identities one inactive Analytical attempt is leased under.
///
/// These are the parts of the coordinator identity that a caller allocates
/// rather than the handle: the two query identities and the three digests the
/// leader already resolved for the attempt.
#[derive(Debug, Clone)]
pub struct AnalyticalAttemptContext {
    /// The one client-visible identity of this query.
    pub public_query_id: PublicQueryId,
    /// The private distributed-graph identity of this attempt.
    pub datafusion_query_id: DataFusionQueryId,
    /// Pinned snapshot digest of the attempt's immutable cut.
    pub snapshot_digest: String,
    /// Reservation this graph's follower work charges against.
    pub reservation_id: String,
    /// Digest of the leader-authorized permissions for this query.
    pub permission_digest: String,
}

/// Graph and attempt ownership retained for one inactive Analytical attempt.
///
/// The two guards are kept together because they release in a fixed order:
/// the attempt returns its exchange-buffer and scratch children to the query
/// envelope, and only then does the graph return the envelope itself. Dropping
/// this value settles both, which is what makes a cancelled or abandoned
/// attempt return capacity instead of stranding it.
pub struct AnalyticalAttemptOwnership {
    /// Attempt ownership retained for as long as stage work may run.
    ///
    /// Declared before the graph on purpose: Rust drops struct fields in
    /// declaration order, and a graph asked to release while one of its
    /// attempts is still live refuses. Reversing these two fields is not a
    /// style choice; it strands the query envelope.
    pub attempt: AnalyticalAttemptGuard,
    /// Graph ownership retained for as long as stages may be addressed.
    pub graph: AnalyticalGraphGuard,
}

impl AnalyticalAttemptOwnership {
    /// Returns the exact attempt every descendant of this ownership binds to.
    #[must_use]
    pub fn key(&self) -> AnalyticalAttemptKey {
        self.attempt.key()
    }

    /// Records that result data produced under this attempt left the node.
    pub fn record_egress(&self) {
        self.attempt.record_egress();
    }

    /// Reports whether result data already left the node under this attempt.
    #[must_use]
    pub fn egressed(&self) -> bool {
        self.attempt.egressed()
    }

    /// Admits the one permitted retry, draining this attempt first.
    ///
    /// Three refusals are structural rather than advisory, and each is checked
    /// before anything is torn down so a refused retry leaves the caller's
    /// attempt exactly as it was:
    ///
    /// * a retry after result data has already egressed is refused, because a
    ///   successor would re-emit rows the client has seen;
    /// * a second retry is refused by [`AnalyticalAttemptNumber::retry`],
    ///   which has no successor for the retry itself;
    /// * the successor is admitted only after this attempt settles, and the
    ///   supervisor independently refuses a second live attempt in the same
    ///   slot, so the drain-before-admit ordering cannot be skipped by a
    ///   caller that settles out of order.
    ///
    /// The graph is carried through untouched: the retry reuses the same
    /// query-owned runtime, admitted envelope, cut, and deadline, which is what
    /// makes it a retry rather than a second query.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when result data already egressed,
    /// when this attempt is already the one permitted retry, or when the
    /// supervisor refuses the successor, and the error reported by
    /// [`AnalyticalAttemptGuard::finish`] when this attempt cannot settle.
    pub async fn retry_pre_egress(
        self,
        grant: AnalyticalAttemptGrant,
    ) -> Result<Self, BifrostError> {
        if self.egressed() {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical attempt cannot retry after result-data egress"
                    .to_owned(),
            });
        }
        let key = self.key();
        let Some(next) = key.attempt.retry() else {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical graph has already consumed its one permitted retry"
                    .to_owned(),
            });
        };
        let Self { attempt, graph } = self;
        let supervisor = attempt.supervisor();
        attempt.finish(AnalyticalAttemptOutcome::Retried).await?;
        let attempt = supervisor.spawn_attempt(
            AnalyticalAttemptKey::new(
                key.public_query_id,
                key.datafusion_query_id,
                key.stage,
                key.task,
                next,
            ),
            grant,
        )?;
        Ok(Self { attempt, graph })
    }

    /// Settles the attempt and then releases its graph, in that order.
    ///
    /// Dropping this value also releases both, but only settling joins the
    /// attempt's retained drivers first. A leader stream that ended — for any
    /// reason — calls this so follower work is cancelled and joined before the
    /// query envelope is returned, rather than leaving an abandoned attempt to
    /// be swept by a drop.
    ///
    /// # Errors
    ///
    /// Returns the error reported by
    /// [`AnalyticalSupervisor::finish_attempt`] when the attempt is no longer
    /// supervised or a lock is poisoned, and the error reported by
    /// [`AnalyticalGraphGuard::release`] when the graph cannot be released.
    pub async fn settle(
        self,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<AnalyticalAttemptRelease, BifrostError> {
        let Self { attempt, graph } = self;
        let release = attempt.finish(outcome).await?;
        graph.release()?;
        Ok(release)
    }
}

/// One started Analytical execution and the owners that settle with it.
///
/// The guards are returned rather than detached so the caller cannot drain the
/// batches while the graph or attempt has already been released; dropping this
/// value before draining abandons the attempt, which the supervisor records as
/// cancelled rather than silently forgetting.
pub struct AnalyticalExecution {
    /// Lazy distributed batch stream produced by the attempt.
    pub batches: SendableRecordBatchStream,
    /// Graph and attempt ownership settling with the drained stream.
    pub ownership: AnalyticalAttemptOwnership,
}
