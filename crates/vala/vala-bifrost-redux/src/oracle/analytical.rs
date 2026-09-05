//! Oracle-owned, production-unreachable distributed analytical execution.
//!
//! Bifrost's production query path is Interactive: [`crate::oracle::Oracle`]
//! plans one physical plan and executes it on the leader, dispatching only leaf
//! scans to followers. This module owns the *inactive* Analytical alternative —
//! a full distributed physical plan whose stages execute on followers through
//! Wyrd's already-authenticated Oracle peer ingress.
//!
//! Nothing in production routing reaches this module. Its leader entry point is
//! [`AnalyticalExecutionHandle::lease_session`], reached only through Oracle's
//! test-support raw-SQL harness, so a query that arrives on the public
//! HTTP/gRPC/MCP surface still cannot select Analytical execution.
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use datafusion::error::DataFusionError;
use datafusion::execution::SessionState;
use datafusion::execution::runtime_env::RuntimeEnv;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::prelude::SessionContext;
use datafusion_distributed::SessionStateBuilderExt as _;
use datafusion_distributed::{DistributedExt as _, WorkerResolver};
use datafusion_distributed::{Worker, WorkerQueryContext, WorkerSessionBuilder};
use http::HeaderMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::NodeId;

pub use super::analytical_supervisor::AnalyticalSupervisor;

use super::AuthorizedQueryContext;
use super::analytical_supervisor::{
    AnalyticalAttemptGrant, AnalyticalAttemptGuard, AnalyticalAttemptKey, AnalyticalAttemptRelease,
    AnalyticalGraphGuard, AnalyticalGraphLifecycleOwner, AnalyticalSupervisorInspection, StageId,
    TaskId,
};
use super::analytical_transport::AnalyticalDestination;
use super::analytical_transport::{
    AnalyticalChannelResolver, AnalyticalCoordinatorIdentity, AnalyticalGraphExchanges,
    AnalyticalParticipantCut, AnalyticalStageSigning, StageWireIdentity, read_ticket,
};
use super::dispatcher::{
    BifrostPeerTls, CommittedGraphActivation, GraphLeaseRequest, OraclePeerCredentials,
    PendingGraphActivation, ReservationRegistry,
};
use super::participant_cut::OracleQueryAttemptCut;
use super::peer::{AuthorizedStage, OracleStageAuthority, PeerSecurityError, StageOperationV1};
use super::spill::OracleSpillRuntime;
use super::telemetry::{
    AnalyticalAttemptOutcome, AnalyticalStageOperation, record_stage_operation,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    AnalyticalGraphRef, FencingToken, QueryClass, QueryId, ReservationId, ReserveNodeSlotsRequest,
};

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
/// Exactly one value is reachable. A graph has one attempt: any failure after
/// selection is terminal, so no successor ordinal is representable and the wire
/// ordinal exists only because the distributed dependency encodes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnalyticalAttemptNumber(u8);

impl AnalyticalAttemptNumber {
    /// The first attempt of a graph.
    pub const ZERO: Self = Self(0);

    /// Returns the ordinal for wire encoding, claims binding, and evidence.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Adopts an attempt ordinal received on an authenticated stage operation.
    ///
    /// Returns `None` for any ordinal other than zero, so a forged successor is
    /// rejected at the parsing boundary rather than by a downstream policy.
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::ZERO),
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
    ///
    /// Exchanges allocate from this same pool rather than from a child budget.
    /// A separate exchange grant would be a second ceiling inside a query that
    /// already has one, and every byte an exchange holds is already charged to
    /// the pool installed here.
    runtime: Arc<RuntimeEnv>,
    /// The grant-derived session shape every descendant stage must execute in.
    ///
    /// A follower receives the leader's *plan*, not the leader's session, so
    /// without this it would run that plan under `DataFusion`'s own defaults:
    /// far larger batches than the admitted grant was sized for, held by sort
    /// merge reservations that cannot spill. The shape is the follower's own,
    /// derived from the envelope it admitted for this graph.
    shape: crate::resources::OracleSessionShape,
}

impl fmt::Debug for AnalyticalGraphRuntime {
    /// Names the owner without rendering the pool's internal accounting.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalGraphRuntime")
            .finish_non_exhaustive()
    }
}

impl AnalyticalGraphRuntime {
    /// Names the query-owned runtime one graph installs on its descendants.
    #[must_use]
    pub const fn new(
        runtime: Arc<RuntimeEnv>,
        shape: crate::resources::OracleSessionShape,
    ) -> Self {
        Self { runtime, shape }
    }

    /// Returns the query-owned runtime installed on follower descendants.
    #[must_use]
    pub const fn runtime(&self) -> &Arc<RuntimeEnv> {
        &self.runtime
    }

    /// Returns the session shape every descendant stage must execute in.
    #[must_use]
    pub const fn shape(&self) -> &crate::resources::OracleSessionShape {
        &self.shape
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

/// Interval between two checks that a released graph's children are gone.
const GRAPH_DRAIN_INTERVAL: Duration = Duration::from_millis(10);

/// Maximum number of drain checks before a graph is released regardless.
///
/// Ten milliseconds apart, this bounds the wait at five seconds: long enough
/// for upstream's own post-EOS task-cache eviction, short enough that a genuine
/// leak still surfaces inside one query's lifetime.
const GRAPH_DRAIN_POLLS: usize = 500;

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
    /// Outbound capability this node's own middle stages sign through.
    egress: Arc<AnalyticalStageEgress>,
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
        egress: Arc<AnalyticalStageEgress>,
    ) -> Self {
        Self {
            registry,
            leaf,
            egress,
        }
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
        let key = AnalyticalGraphKey::from_headers(&ctx.headers).map_err(|error| {
            DataFusionError::Execution(format!(
                "Oracle analytical stage carries no graph identity: {error}"
            ))
        })?;
        let graph = self.registry.resolve(key).map_err(|error| {
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
        let mut config = graph
            .shape()
            .apply(builder.config().clone().unwrap_or_default());
        config.set_distributed_user_codec(super::codec::OraclePhysicalExtensionCodec::analytical(
            self.leaf.clone(),
        ));
        // A stage that only feeds the leader never opens a channel of its own.
        // A stage in the middle of a deeper graph does: it pulls from another
        // follower, so this session needs the same signed transport the leader
        // uses. Installing it unconditionally keeps the two cases identical;
        // an unrecorded graph resolves to no minter and refuses instead.
        let resolver = self.egress.resolver(key).map_err(|error| {
            DataFusionError::Execution(format!(
                "Oracle analytical stage cannot address its peers: {error}"
            ))
        })?;
        if let Some(resolver) = resolver {
            let urls = self.egress.peer_urls(key).map_err(|error| {
                DataFusionError::Execution(format!(
                    "Oracle analytical stage cannot address its peers: {error}"
                ))
            })?;
            config.set_distributed_worker_resolver(AnalyticalWorkerResolver { urls });
            config.set_distributed_channel_resolver(resolver);
        }
        builder = builder.with_config(config);
        Ok(builder
            .with_runtime_env(Arc::clone(graph.runtime()))
            .build())
    }
}

/// Node-local capability a follower mints its own outbound stage tickets from.
///
/// A two-stage graph only ever pulls follower to leader, and the leader signs
/// those as the client. A deeper graph does not: a middle stage runs on a
/// follower and pulls from another follower, so that follower is itself a
/// coordinator and must sign. Everything it signs with is already verified —
/// both the identity and the addressable destinations come from the claims of
/// the ticket that authorized its own stage. This owner never reads live
/// membership: the participant cut was frozen by the leader at attempt start
/// and travels signed with every operation, so churn cannot move a destination
/// out from under an in-flight graph.
pub struct AnalyticalStageEgress {
    /// Server-owned authority holding this node's signing key.
    authority: Arc<dyn OracleStageAuthority>,
    /// This node's own identity, signed as the source of every outbound ticket.
    node_id: NodeId,
    /// This node's own current Oracle role fence.
    oracle_fence: u64,
    /// Ticket lifetime, kept far shorter than the graph's own deadline.
    ticket_ttl: chrono::Duration,
    /// Immutable peer identity every outbound channel is dialed through.
    peer_tls: BifrostPeerTls,
    /// Workload credential every outbound peer request presents.
    peer_credentials: Arc<dyn OraclePeerCredentials>,
    /// Per-graph outbound identity recorded when a stage was authorized.
    identities: Mutex<HashMap<AnalyticalGraphKey, AnalyticalEgressIdentity>>,
}

/// One graph's recorded egress state, read out from under the identity lock.
///
/// A copy rather than a borrow because the lock must not be held across
/// resolver construction, and both callers need the same three values.
struct RecordedEgress {
    /// Coordinator identity this node signs its own stage operations under.
    identity: Arc<AnalyticalCoordinatorIdentity>,
    /// Absolute graph deadline carried unchanged into every outbound ticket.
    deadline_ms: i64,
    /// Participant cut adopted from the ticket that authorized this graph.
    cut: Arc<AnalyticalParticipantCut>,
    /// The graph's own exchange registry, which owns every stream it opens.
    ///
    /// An outbound exchange stream is the only thing on this node that a
    /// settling graph cannot reach by joining an attempt: upstream hands the
    /// stream out and keeps its own reader task alive until every partition of
    /// it has been dropped, and a consumer that stopped polling will never drop
    /// it. Owning the stream is what lets the graph close it.
    exchanges: Arc<AnalyticalGraphExchanges>,
}

/// One graph's verified outbound identity, deadline, and frozen destinations.
struct AnalyticalEgressIdentity {
    /// Coordinator identity this node signs its own stage operations under.
    identity: Arc<AnalyticalCoordinatorIdentity>,
    /// Absolute graph deadline carried unchanged into every outbound ticket.
    deadline_ms: i64,
    /// Participant cut adopted from the ticket that authorized this graph.
    cut: Arc<AnalyticalParticipantCut>,
    /// The graph's own exchange registry, which owns every stream it opens.
    exchanges: Arc<AnalyticalGraphExchanges>,
}

impl fmt::Debug for AnalyticalStageEgress {
    /// Reports node identity without rendering the authority or live graphs.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalStageEgress")
            .field("node_id", &self.node_id.as_uuid())
            .field("oracle_fence", &self.oracle_fence)
            .finish_non_exhaustive()
    }
}

impl AnalyticalStageEgress {
    /// Composes the egress owner over this node's authority and peer identity.
    #[must_use]
    pub fn new(
        authority: Arc<dyn OracleStageAuthority>,
        node_id: NodeId,
        oracle_fence: u64,
        ticket_ttl: chrono::Duration,
        peer_tls: BifrostPeerTls,
        peer_credentials: Arc<dyn OraclePeerCredentials>,
    ) -> Self {
        Self {
            authority,
            node_id,
            oracle_fence,
            ticket_ttl,
            peer_tls,
            peer_credentials,
            identities: Mutex::new(HashMap::new()),
        }
    }

    /// Records the outbound identity a graph's own stages will sign under.
    ///
    /// Every field is taken from the verified claims rather than from headers,
    /// so a follower can only ever re-emit the tenant, graph, attempt,
    /// reservation, and permissions it was itself authorized for. The source
    /// identity is this node's own, because this node is the coordinator of
    /// whatever it sends next.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the identity lock is poisoned or
    /// the ticket's participant cut is oversized, malformed, or plaintext, and
    /// [`BifrostError::QueryPeerSecurity`] when the claims name an invalid
    /// tenant.
    fn record(
        &self,
        graph: AnalyticalGraphKey,
        authorized: &AuthorizedStage,
        exchanges: Arc<AnalyticalGraphExchanges>,
    ) -> Result<(), BifrostError> {
        let claims = &authorized.claims;
        let tenant_id = Uuid::from_slice(&claims.tenant_id)
            .map_err(|_| BifrostError::QueryPeerSecurity)
            .and_then(|uuid| {
                wyrd_spec::DataTenantId::new(uuid).map_err(|_| BifrostError::QueryPeerSecurity)
            })?;
        let entry = AnalyticalEgressIdentity {
            identity: Arc::new(AnalyticalCoordinatorIdentity {
                source_node_id: self.node_id,
                source_fence: self.oracle_fence,
                tenant_id,
                graph,
                snapshot_digest: claims.snapshot_digest.clone(),
                attempt: claims.attempt,
                reservation_id: claims.reservation_id.clone(),
                permission_digest: claims.permission_digest.clone(),
            }),
            deadline_ms: claims.absolute_deadline_ms,
            // Adopted, never widened: this node can address exactly the
            // destinations its own coordinator was authorized to address.
            cut: Arc::new(AnalyticalParticipantCut::adopt(&claims.participants)?),
            exchanges,
        };
        self.identities
            .lock()
            .map_err(|_| poisoned_ingress())?
            .insert(graph, entry);
        Ok(())
    }

    /// Releases one graph's outbound identity once nothing on this node owns it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the identity lock is poisoned.
    fn release(&self, graph: AnalyticalGraphKey) -> Result<(), BifrostError> {
        self.identities
            .lock()
            .map_err(|_| poisoned_ingress())?
            .remove(&graph);
        Ok(())
    }

    /// Builds the channel resolver one authorized graph's stages send through.
    ///
    /// Returns `None` when this node holds no recorded identity for `graph`,
    /// which is the correct refusal: a stage that was never authorized here has
    /// nothing to sign with, and upstream then fails to open a channel rather
    /// than emitting an unsigned request.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the identity lock is poisoned.
    fn resolver(
        &self,
        graph: AnalyticalGraphKey,
    ) -> Result<Option<AnalyticalChannelResolver>, BifrostError> {
        let Some(recorded) = self.recorded(graph)? else {
            return Ok(None);
        };
        Ok(Some(AnalyticalChannelResolver::new(
            recorded.identity,
            self.peer_tls.clone(),
            Arc::clone(&self.peer_credentials),
            // A follower adopts a cut that is already complete, so its cell is
            // published at construction and never observed unset.
            Arc::new(std::sync::OnceLock::from(recorded.cut)),
            recorded.exchanges,
            AnalyticalStageSigning {
                authority: Arc::clone(&self.authority),
                absolute_deadline_ms: recorded.deadline_ms,
                ticket_ttl: self.ticket_ttl,
            },
        )))
    }

    /// Returns the endpoints one authorized graph may address, for planning.
    ///
    /// These are the leader's frozen participants, not this node's membership
    /// view, so a stage planned here spans exactly the attempt's own cut.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the identity lock is poisoned.
    fn peer_urls(&self, graph: AnalyticalGraphKey) -> Result<Vec<Url>, BifrostError> {
        Ok(self
            .recorded(graph)?
            .map(|recorded| recorded.cut.urls())
            .unwrap_or_default())
    }

    /// Reads one graph's recorded identity, deadline, and frozen cut.
    ///
    /// Returns `None` when this node holds no recorded identity for `graph`.
    /// Split out because both the resolver and stage planning need the same
    /// entry and neither may hold the lock across the work that follows.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the identity lock is poisoned.
    fn recorded(&self, graph: AnalyticalGraphKey) -> Result<Option<RecordedEgress>, BifrostError> {
        let identities = self.identities.lock().map_err(|_| poisoned_ingress())?;
        Ok(identities.get(&graph).map(|entry| RecordedEgress {
            identity: Arc::clone(&entry.identity),
            deadline_ms: entry.deadline_ms,
            cut: Arc::clone(&entry.cut),
            exchanges: Arc::clone(&entry.exchanges),
        }))
    }
}

/// The immutable authority one activated graph retains for its whole life.
///
/// A graph is addressed by many separate stage messages, and only the first one
/// gets to say what the graph *is*. This is the exact union the follower keeps
/// from that moment: the half it already proved when it accepted the
/// reservation — identity, the reserving leader and fence, and the original
/// expiry — and the half the first verified stage ticket carried, which the
/// per-message binding check does not cover.
///
/// It is a projection of already-verified material, never a second signed
/// inventory. [`super::peer::StageTicketClaims`] remains the only thing a peer
/// signs; this is what the follower chose to remember from it.
#[derive(Debug)]
pub(crate) struct GraphLeaseBinding {
    /// Reservation this graph was activated from.
    reservation_id: ReservationId,
    /// Exact graph the reservation was taken for.
    graph: AnalyticalGraphRef,
    /// Query identity the reservation was bound to.
    query_id: QueryId,
    /// Node identity of the leader that took the reservation.
    ///
    /// Reservation ownership, not stage authority. It authorizes a later
    /// coordinator only as itself, and it is deliberately not inserted into the
    /// participant cut: the leader is not a destination of this graph.
    reserving_leader_node_id: NodeId,
    /// Role fence the reserving leader held when it took the reservation.
    reserving_leader_fence: FencingToken,
    /// The reservation's own expiry, which activation never extends.
    reservation_expires_at: DateTime<Utc>,
    /// Authenticated data tenant of the graph.
    tenant_id: DataTenantId,
    /// Client-visible query identity of the graph.
    public_query_id: Uuid,
    /// Private distributed-graph identity.
    datafusion_query_id: Uuid,
    /// This follower's own node identity, as the first ticket addressed it.
    destination_node_id: NodeId,
    /// This follower's own role fence, as the first ticket addressed it.
    destination_fence: u64,
    /// Membership digest of the immutable destination participant cut.
    participant_cut_fingerprint: String,
    /// The frozen destination cut itself, for exact source membership.
    participant_cut: AnalyticalParticipantCut,
    /// Pinned snapshot digest of the graph's cut.
    snapshot_digest: String,
    /// Permission digest the graph's authority was resolved under.
    permission_digest: String,
    /// Absolute wall-clock deadline of the whole graph.
    absolute_deadline_ms: i64,
}

impl GraphLeaseBinding {
    /// Derives the retained binding from a reservation and its first ticket.
    ///
    /// Both halves come from material this node already verified: the
    /// reservation it accepted, and the stage ticket the authority just
    /// returned. Nothing is read from wire framing, and no new wire field is
    /// required — the union already travels.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryPeerSecurity`] when the verified claims
    /// carry an unrepresentable identity, and [`BifrostError::Internal`] when
    /// the carried participant cut cannot be adopted.
    fn activate(
        activation: &PendingGraphActivation,
        request: &GraphLeaseRequest,
        authorized: &AuthorizedStage,
    ) -> Result<Self, BifrostError> {
        let claims = &authorized.claims;
        let (reserving_leader_node_id, reserving_leader_fence) = activation.reserving_leader();
        let participant_cut = AnalyticalParticipantCut::adopt(&claims.participants)?;
        Ok(Self {
            reservation_id: request.reservation_id,
            graph: request.graph,
            query_id: request.query_id,
            reserving_leader_node_id,
            reserving_leader_fence,
            reservation_expires_at: activation.expires_at(),
            tenant_id: authorized.tenant_id,
            public_query_id: request.graph.public_query_id,
            datafusion_query_id: request.graph.datafusion_query_id,
            destination_node_id: node_from_claim(&claims.destination_node_id)?,
            destination_fence: claims.destination_fence,
            participant_cut_fingerprint: participant_cut.fingerprint(),
            participant_cut,
            snapshot_digest: claims.snapshot_digest.clone(),
            permission_digest: claims.permission_digest.clone(),
            absolute_deadline_ms: claims.absolute_deadline_ms,
        })
    }

    /// Authorizes one later stage message against this graph's fixed authority.
    ///
    /// Two independent checks, in this order. First the presenting coordinator:
    /// its exact node and fence pair is valid only when it is the pair that
    /// reserved this graph, or an exact member of the immutable destination cut.
    /// Neither branch widens the other — the leader is not in the cut, and cut
    /// membership does not confer reservation ownership. Then the graph itself:
    /// every immutable field the first ticket fixed must still be identical.
    ///
    /// This is pure comparison. It runs before plan decode, cache lookup,
    /// runtime construction, provider resolution, and source IO, and a refusal
    /// leaves the live lease exactly as it was.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryPeerSecurity`] for an unauthorized source
    /// pair, any changed immutable field, or an unrepresentable identity.
    pub(crate) fn authorize(&self, authorized: &AuthorizedStage) -> Result<(), BifrostError> {
        let claims = &authorized.claims;
        let source_node_id = node_from_claim(&claims.source_node_id)?;
        let reserving_leader = source_node_id == self.reserving_leader_node_id
            && claims.source_fence == self.reserving_leader_fence;
        if !reserving_leader
            && !self
                .participant_cut
                .contains(source_node_id, claims.source_fence)
        {
            tracing::warn!(
                public_query_id = %self.public_query_id,
                "Oracle analytical stage source is neither the reserving leader nor a cut participant"
            );
            return Err(BifrostError::QueryPeerSecurity);
        }
        let carried = AnalyticalParticipantCut::adopt(&claims.participants)?;
        if authorized.tenant_id != self.tenant_id
            || claims.public_query_id != self.public_query_id.as_bytes()
            || claims.datafusion_query_id != self.datafusion_query_id.as_bytes()
            || node_from_claim(&claims.destination_node_id)? != self.destination_node_id
            || claims.destination_fence != self.destination_fence
            || claims.snapshot_digest != self.snapshot_digest
            || claims.permission_digest != self.permission_digest
            || claims.absolute_deadline_ms != self.absolute_deadline_ms
            || claims.reservation_id != self.reservation_id.as_uuid().to_string()
            || carried.fingerprint() != self.participant_cut_fingerprint
        {
            tracing::warn!(
                public_query_id = %self.public_query_id,
                "Oracle analytical stage message would change a live graph's fixed authority"
            );
            return Err(BifrostError::QueryPeerSecurity);
        }
        Ok(())
    }

    /// Returns the signed absolute wall-clock deadline of the whole graph.
    ///
    /// This is the graph's only time bound and it is immutable: every later
    /// stage message must carry the identical value or [`Self::authorize`]
    /// refuses it. The ingress reads it to know when a graph that no coordinator
    /// connection is holding must nevertheless be cancelled.
    pub(crate) const fn absolute_deadline_ms(&self) -> i64 {
        self.absolute_deadline_ms
    }

    /// Reports whether `request` names exactly this graph's reservation tuple.
    pub(crate) fn matches(&self, request: &GraphLeaseRequest) -> bool {
        self.reservation_id == request.reservation_id
            && self.graph == request.graph
            && self.query_id == request.query_id
    }
}

/// Reads one node identity from verified claims.
///
/// # Errors
///
/// Returns [`BifrostError::QueryPeerSecurity`] when the claim does not carry a
/// well-formed node UUID.
fn node_from_claim(claim: &[u8]) -> Result<NodeId, BifrostError> {
    Uuid::from_slice(claim)
        .map(NodeId::new)
        .map_err(|_| BifrostError::QueryPeerSecurity)
}

/// One follower's complete, exclusive ownership of one distributed graph.
///
/// Published only after every fallible activation step succeeded, and shared by
/// every later stage message for the graph. It owns the whole set at once — the
/// retained authority, the reservation residue, the supervisor's graph guard,
/// the query-owned runtime, the graph's cancellation child, its absolute
/// deadline, and its live attempts — so there is no window in which a graph is
/// visible while part of what it needs is missing, and no take-once hole a
/// second caller can find empty.
pub struct GraphLease {
    /// Fixed authority every later message for this graph is checked against.
    binding: GraphLeaseBinding,
    /// Reservation residue — the running permit — held for the graph's life.
    activation: CommittedGraphActivation,
    /// Graph this lease is the sole follower-local owner of.
    graph: AnalyticalGraphKey,
    /// Node supervisor holding this graph's admitted envelope.
    supervisor: Arc<AnalyticalSupervisor>,
    /// Outbound capability this graph's own middle stages sign through.
    egress: Arc<AnalyticalStageEgress>,
    /// Supervisor registration, released exactly once by settlement.
    guard: Mutex<Option<AnalyticalGraphGuard>>,
    /// Cancellation child covering every descendant of this graph.
    cancel: CancellationToken,
    /// Attempts admitted under this graph, joined before it may be released.
    attempts: Mutex<HashMap<AnalyticalAttemptKey, AnalyticalAttemptGuard>>,
    /// Whether settlement has already run, so it can never run twice.
    settled: AtomicBool,
    /// Every outbound exchange stream this graph opened, owned by the graph.
    exchanges: Arc<AnalyticalGraphExchanges>,
    /// Upstream worker whose task cache is scoped to exactly this graph.
    ///
    /// Upstream caches a stage's decoded task data on the worker that served
    /// it, and those cached plans own the exchange connections that charge this
    /// graph's envelope. A worker shared by the whole process would therefore
    /// hold this envelope's reservations until its own idle timeout expired,
    /// long after the graph ended. Owning one worker per graph makes the cache
    /// die with the graph, which is what lets settlement actually join it.
    /// Taken by [`Self::settle`] before the drain, so the drain observes the
    /// release rather than waiting on a cache nothing will clear.
    worker: Mutex<Option<Worker>>,
}

impl fmt::Debug for GraphLease {
    /// Reports the graph identity without rendering owned ownership.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GraphLease")
            .field("graph", &self.graph)
            .field("settled", &self.settled.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl GraphLease {
    /// Returns the fixed authority this graph was activated with.
    #[must_use]
    pub(crate) fn binding(&self) -> &GraphLeaseBinding {
        &self.binding
    }

    /// Returns this graph's own upstream worker while the graph is unsettled.
    ///
    /// A clone shares the graph's task cache, so an in-flight request keeps it
    /// alive for exactly as long as it is being served. Settlement takes the
    /// lease's own handle, after which this returns `None` and no new stage
    /// operation can enter the graph's cache.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the worker lock is poisoned.
    pub(crate) fn worker(&self) -> Result<Option<Worker>, BifrostError> {
        Ok(self
            .worker
            .lock()
            .map_err(|_| poisoned_ingress())?
            .as_ref()
            .cloned())
    }

    /// Returns the registry owning every outbound exchange this graph opened.
    ///
    /// Settlement closes them, which is the only way this node can end an
    /// upstream reader task it does not own.
    #[must_use]
    pub(crate) fn exchanges(&self) -> Arc<AnalyticalGraphExchanges> {
        Arc::clone(&self.exchanges)
    }

    /// Returns how many attempts of this graph are still admitted.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the attempt lock is poisoned.
    pub(crate) fn live_attempts(&self) -> Result<usize, BifrostError> {
        Ok(self.attempts.lock().map_err(|_| poisoned_ingress())?.len())
    }

    /// Admits one attempt of this graph and retains its guard.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] on a poisoned lock, and the
    /// supervisor's refusal when the slot is occupied or the graph is unknown.
    fn admit_attempt(
        &self,
        key: AnalyticalAttemptKey,
        grant: AnalyticalAttemptGrant,
    ) -> Result<(), BifrostError> {
        let mut attempts = self.attempts.lock().map_err(|_| poisoned_ingress())?;
        if attempts.contains_key(&key) {
            return Ok(());
        }
        let guard = self.supervisor.spawn_attempt(key, grant)?;
        attempts.insert(key, guard);
        Ok(())
    }

    /// Settles one named attempt of this graph.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] on a poisoned lock,
    /// [`BifrostError::QueryExecutionFailed`] when `key` names no live attempt
    /// of this graph, and the supervisor's refusal when settlement fails.
    async fn finish_attempt(
        &self,
        key: AnalyticalAttemptKey,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<(), BifrostError> {
        let guard = {
            let mut attempts = self.attempts.lock().map_err(|_| poisoned_ingress())?;
            attempts.remove(&key)
        };
        match guard {
            Some(guard) => guard.finish(outcome).await.map(|_| ()),
            None => Err(BifrostError::QueryExecutionFailed),
        }
    }

    /// Returns everything this graph owns, in one order, exactly once.
    ///
    /// The order is the invariant: stop admitting, cancel unless the graph
    /// succeeded, join every attempt and the descendants they own, wait for the
    /// envelope's nested children to go idle — which is what proves no cache
    /// entry, exchange, or spill write is still live — and only then release the
    /// supervisor guard that returns the envelope, the egress record, and, when
    /// the last holder of this lease goes away, the reservation's running
    /// permit.
    ///
    /// A cleanup timeout or failure is reported as a failure. It is never a
    /// successful release: the graph keeps every owner it still holds so the
    /// leak stays attributable to this node rather than becoming a poisoned
    /// governor later.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a lock is poisoned or the
    /// graph's descendants did not drain within the bounded wait, and the
    /// supervisor's or egress owner's refusal when a release fails.
    pub(crate) async fn settle(
        &self,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<(), BifrostError> {
        if self.settled.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        tracing::debug!(
            public_query_id = %self.graph.public_query_id,
            reservation_id = %self.activation.reservation_id().as_uuid(),
            reservation_expires_at = %self.binding.reservation_expires_at,
            outcome = ?outcome,
            "Oracle analytical follower is settling a graph"
        );
        if outcome != AnalyticalAttemptOutcome::Success {
            self.cancel.cancel();
        }
        let live = {
            let attempts = self.attempts.lock().map_err(|_| poisoned_ingress())?;
            attempts.keys().copied().collect::<Vec<_>>()
        };
        for key in live {
            self.finish_attempt(key, outcome).await?;
        }
        // Before the drain, never after. Upstream's cached stage plans own the
        // worker connections that hold this envelope's reservations, so the
        // cache has to go first or the drain would be waiting on bytes that
        // nothing in the graph's own lifetime will ever release.
        // Unconditional, and only after every attempt has been joined. The
        // graph is over on every terminal path, and its outbound exchanges are
        // the one thing joining an attempt cannot reach: upstream keeps a
        // reader task, and the buffers that task charges to this envelope,
        // alive until every partition stream it handed out has been dropped.
        // Cancelling here is what drops them. Cancelling any earlier would have
        // turned a successful graph into a cancelled one.
        self.cancel.cancel();
        // Cancellation alone cannot end an exchange whose consumer stopped
        // polling it, and upstream's reader task lives until every partition
        // stream it handed out is dropped. The graph owns those streams, so it
        // drops them here.
        self.exchanges.close();
        drop(self.worker.lock().map_err(|_| poisoned_ingress())?.take());
        self.drain().await?;
        let guard = self.guard.lock().map_err(|_| poisoned_ingress())?.take();
        if let Some(guard) = guard {
            guard.release()?;
        }
        self.egress.release(self.graph)?;
        Ok(())
    }

    /// Waits, bounded, for every nested child of the graph's envelope to end.
    ///
    /// Upstream drops a follower's stage plan from its own task cache after the
    /// coordinator channel ends, so the query envelope can still carry live
    /// `DataFusion` reservations for a short moment after every governed call
    /// for the graph has closed. Releasing into that moment would poison the
    /// process governor for a teardown that is merely in progress.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the supervisor's ownership state
    /// is poisoned or the graph did not drain within the bounded wait.
    async fn drain(&self) -> Result<(), BifrostError> {
        for _ in 0..GRAPH_DRAIN_POLLS {
            if self.supervisor.graph_children_idle(self.graph)? {
                return Ok(());
            }
            tokio::time::sleep(GRAPH_DRAIN_INTERVAL).await;
        }
        let (scratch_bytes, memory_bytes) = self.supervisor.graph_children_debt(self.graph)?;
        tracing::warn!(
            public_query_id = %self.graph.public_query_id,
            datafusion_query_id = %self.graph.datafusion_query_id,
            scratch_bytes,
            memory_bytes,
            "Oracle analytical graph did not drain before its follower release"
        );
        Err(BifrostError::Internal {
            detail: "Oracle analytical graph cleanup did not complete".to_owned(),
        })
    }
}

/// One graph's exact state in this follower's ownership map.
///
/// There is one entry per graph, under one mutex, holding everything the
/// follower needs to answer both questions it is ever asked about a graph: may
/// this message address it, and may it be released yet. Splitting the graph
/// guard from its open-connection count is what previously let a graph be
/// released while a coordinator could still address it.
#[derive(Debug)]
enum AnalyticalGraphEntry {
    /// The graph is live and may be addressed.
    Active {
        /// The graph's sole follower-local owner.
        lease: Arc<GraphLease>,
        /// Coordinator calls currently open for the graph.
        open_connections: usize,
    },
    /// The graph is settling or has failed to settle, and admits nothing new.
    Draining {
        /// The graph's owner, retained until settlement actually succeeds.
        lease: Arc<GraphLease>,
        /// Why cleanup failed, when it did. `Some` fails readiness.
        settlement_failure: Option<String>,
    },
}

/// One graph handed to the ingress-owned settlement driver.
///
/// Deliberately self-contained: the driver settles through the lease it is
/// given, not through a map it re-reads, so a settlement in flight cannot be
/// confused by a later entry for the same identity.
struct GraphSettlement {
    /// Graph whose entry the driver updates after settlement.
    graph: AnalyticalGraphKey,
    /// The graph's owner, settled by the driver.
    lease: Arc<GraphLease>,
    /// Terminal outcome the settlement is performed under.
    outcome: AnalyticalAttemptOutcome,
}

impl GraphSettlement {
    /// Drain this graph's lease and retain its identity with the outcome so the
    /// ingress driver can update the matching entry without a second inventory.
    ///
    /// # Errors
    /// The returned outcome contains the lease's task-drain or cleanup failure;
    /// failed cleanup remains owned and observable by the lease supervisor.
    async fn settle(self) -> (AnalyticalGraphKey, Result<(), BifrostError>) {
        let settled = self.lease.settle(self.outcome).await;
        (self.graph, settled)
    }
}

/// The closed set of commands the one settlement driver accepts.
///
/// Two, because the driver has exactly two reasons to run: a graph it must
/// settle now, and a change to the set of graphs whose deadlines it is
/// watching. Sharing one queue keeps the driver a single joined owner with a
/// single capacity root rather than two independently sized channels.
enum GraphSettlementCommand {
    /// A newly activated graph exists; rescan the deadline inventory.
    Wake,
    /// Settle exactly this graph, which has already left the active state.
    Settle(GraphSettlement),
}

/// Why the bounded settlement queue could not take a command.
///
/// Named rather than stringly because the two cases have different policies: a
/// lost settlement is a cleanup failure the graph must retain, while a full
/// queue is itself a wake and loses nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettlementQueueRefusal {
    /// Every slot is taken; the driver has unread commands to process.
    Full,
    /// The queue is closed or unreadable, so nothing will observe this graph.
    Closed,
}

impl SettlementQueueRefusal {
    /// Renders the retained cleanup-failure reason for this refusal.
    fn detail(self) -> String {
        match self {
            Self::Full => "Oracle analytical settlement queue is full".to_owned(),
            Self::Closed => "Oracle analytical settlement queue is closed".to_owned(),
        }
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
    /// Reservation owner this follower activates graph leases from.
    ///
    /// The same registry the fragment path reserves against. A graph does not
    /// get its own capacity book: it takes the envelope the leader already
    /// reserved on this node, which is what makes a follower's charge for a
    /// distributed plan the one the leader was told it would be.
    pub reservations: Arc<ReservationRegistry>,
    /// Process spill owner that bounds each query runtime's disk manager.
    pub spill: Arc<OracleSpillRuntime>,
    /// Capability every Analytical leaf decoded on this node resolves through.
    pub leaf: super::codec::AnalyticalLeafBinding,
    /// Outbound capability a middle stage on this node signs its peers with.
    pub egress: Arc<AnalyticalStageEgress>,
}

/// Authenticated follower entry point for Analytical stage operations.
///
/// This is the node-local owner that turns an authenticated stage operation
/// into supervised distributed work. The ordering it enforces is the whole
/// point of the type: nothing decodes a plan, reads the task cache, constructs
/// a provider, or touches storage until
/// [`OracleStageAuthority::authorize_stage`] has returned *and* the graph's own
/// retained authority has authorized the message, and the headers the upstream
/// worker resolves its runtime from are derived from the *verified* claims
/// rather than from anything the caller supplied.
///
/// The ingress owns the follower's graphs because a graph spans several
/// separate stage calls: it is activated by whichever authorized message for it
/// arrives first and released when the graph is torn down, not when any one call
/// returns. Activation, reuse, and the transition to draining are all serialized
/// on one graph mutex, which is what makes exactly-once activation and
/// exactly-once settlement signalling the same, single decision.
pub struct AnalyticalStageIngress {
    /// This follower's own node identity, bound as every ticket's audience.
    node_id: NodeId,
    /// This follower's own current Oracle role fence.
    oracle_fence: u64,
    /// Server-owned authority every stage operation is checked against.
    authority: Arc<dyn OracleStageAuthority>,
    /// Node-local supervisor owning graphs, attempts, and the runtime registry.
    supervisor: Arc<AnalyticalSupervisor>,
    /// Reservation owner this follower activates graph leases from.
    reservations: Arc<ReservationRegistry>,
    /// Process spill owner that bounds each query runtime's disk manager.
    spill: Arc<OracleSpillRuntime>,
    /// Capability an Analytical leaf needs to resolve its own source locally.
    ///
    /// Retained rather than consumed once, because every graph builds its own
    /// upstream worker from it and each of those workers owns a task cache that
    /// must not outlive its graph.
    leaf: super::codec::AnalyticalLeafBinding,
    /// This follower's graphs, each in exactly one state, under one mutex.
    graphs: Mutex<HashMap<AnalyticalGraphKey, AnalyticalGraphEntry>>,
    /// Outbound capability this node's own middle stages sign through.
    egress: Arc<AnalyticalStageEgress>,
    /// Bounded queue the caller-drop path hands graphs to for settlement.
    ///
    /// Cleared by shutdown, which is how the driver learns to finish. A send
    /// that cannot be made is recorded as a cleanup failure on the graph; it is
    /// never quietly downgraded to a spawn or a successful terminal.
    settlement: Mutex<Option<mpsc::Sender<GraphSettlementCommand>>>,
    /// The one settlement driver, owned and joined by this ingress.
    driver: Mutex<Option<JoinHandle<()>>>,
    /// Whether this ingress still admits new stage work.
    accepting: AtomicBool,
    /// Test-tier one-shot pause of one `ExecuteTask` after graph activation.
    #[cfg(feature = "test-support")]
    execute_pause: Mutex<Option<Arc<AnalyticalExecutePause>>>,
}

/// Test seam that holds one follower `ExecuteTask` open at a real boundary.
///
/// Armed by a journey before it starts a distributed query. The first
/// `ExecuteTask` this follower authorizes is held after its `GraphLease` is
/// active and before it can consume any source, which is the only window in
/// which peer loss or cancellation is observable as "the follower had the
/// graph and had produced nothing". Every later `ExecuteTask` passes straight
/// through, so arming one pause never stalls the rest of the graph.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct AnalyticalExecutePause {
    /// Ensures exactly one `ExecuteTask` is held.
    claimed: AtomicBool,
    /// Wakes the journey once that task is holding.
    paused: tokio::sync::Notify,
    /// Records permission for the held task to continue.
    released: AtomicBool,
    /// Wakes the held task once the journey is done with it.
    release: tokio::sync::Notify,
}

#[cfg(feature = "test-support")]
impl AnalyticalExecutePause {
    /// Waits until one `ExecuteTask` is held with its graph lease active.
    pub async fn wait_paused(&self) {
        while !self.claimed.load(Ordering::Acquire) {
            self.paused.notified().await;
        }
    }

    /// Reports whether an `ExecuteTask` has reached the seam.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.claimed.load(Ordering::Acquire)
    }

    /// Releases the held task so the graph may finish or fail normally.
    pub fn release(&self) {
        self.released.store(true, Ordering::Release);
        self.release.notify_waiters();
    }

    /// Holds only the first `ExecuteTask` and lets every later one proceed.
    async fn hold(&self) {
        if self
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.paused.notify_waiters();
        while !self.released.load(Ordering::Acquire) {
            self.release.notified().await;
        }
    }
}

impl fmt::Debug for AnalyticalStageIngress {
    /// Reports the configured budget without rendering owned dependencies.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalStageIngress")
            .field("accepting", &self.accepting.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl AnalyticalStageIngress {
    /// Settles every graph this node must release, and cancels each at its deadline.
    ///
    /// The single asynchronous owner of follower settlement. It has three reasons
    /// to wake — a command on its bounded queue, the earliest signed graph deadline
    /// coming due, and the completion of a settlement it already owns — and it
    /// serves all three from one task. The settlements it owns run concurrently in
    /// one driver-local `FuturesUnordered`, so a graph whose cleanup drains slowly
    /// cannot stop the driver from cancelling a different graph on time; they are
    /// still this task's own futures, so nothing is detached and shutdown's join is
    /// still complete.
    ///
    /// It holds only a [`Weak`] back-reference, so the ingress's own join handle
    /// cannot keep the ingress alive; a settlement that completes after the node is
    /// gone simply has no entry left to update. Returning ends the driver, which is
    /// what [`AnalyticalStageIngress::shutdown`] joins.
    async fn drive_graph_settlements(
        mut commands: mpsc::Receiver<GraphSettlementCommand>,
        ingress: Weak<Self>,
    ) {
        let mut settling = futures_util::stream::FuturesUnordered::new();
        let mut queue_closed = false;
        loop {
            // Every deadline is rescanned from the graph map on every wake rather
            // than tracked incrementally: the map is the one lifecycle inventory,
            // and a second copy of it could disagree with the state that authorizes
            // work against the same graph.
            let next_deadline = match ingress.upgrade() {
                Some(node) => match node.expire_due_graphs(Utc::now()) {
                    Ok((due, next)) => {
                        for settlement in due {
                            settling.push(settlement.settle());
                        }
                        next
                    }
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "Oracle analytical settlement driver cannot read graph deadlines"
                        );
                        None
                    }
                },
                None => None,
            };
            if queue_closed && settling.is_empty() {
                return;
            }
            let timer = next_deadline.map(|deadline_ms| {
                let remaining = deadline_ms.saturating_sub(Utc::now().timestamp_millis());
                tokio::time::sleep(Duration::from_millis(u64::try_from(remaining).unwrap_or(0)))
            });
            tokio::select! {
                command = commands.recv(), if !queue_closed => match command {
                    Some(GraphSettlementCommand::Wake) => {}
                    Some(GraphSettlementCommand::Settle(settlement)) => {
                        settling.push(settlement.settle());
                    }
                    None => queue_closed = true,
                },
                Some((graph, settled)) = futures_util::StreamExt::next(&mut settling) => {
                    if let Err(error) = &settled {
                        tracing::warn!(
                            error = %error,
                            public_query_id = %graph.public_query_id,
                            "Oracle analytical follower could not settle a closed graph"
                        );
                    }
                    if let Some(node) = ingress.upgrade() {
                        node.record_settlement(graph, &settled);
                    }
                }
                () = async {
                    timer
                        .expect("the timer branch is enabled only when a deadline exists")
                        .await;
                }, if timer.is_some() => {}
            }
        }
    }

    /// Builds the follower ingress and starts its one settlement driver.
    ///
    /// No upstream worker is built here. Each graph builds its own from
    /// [`AnalyticalSessionBuilder`] over the supervisor's runtime registry, so a
    /// stage whose graph is not registered — an invalidated attempt, a sibling
    /// graph, a forged identity — fails to build a session rather than silently
    /// falling back to a process runtime, and upstream's task cache is scoped to
    /// the graph that filled it rather than to this node's whole lifetime.
    ///
    /// The driver is started here and joined by [`Self::shutdown`], so no
    /// settlement task can outlive the ingress. It holds only a
    /// [`Weak`] reference back, so the ingress's own join handle does not form a
    /// reference cycle that would keep the node alive forever.
    ///
    /// # Panics
    ///
    /// Never panics. Constructed outside a Tokio runtime the driver is simply
    /// absent, and every settlement signal is then recorded as a cleanup
    /// failure rather than silently dropped.
    #[must_use]
    pub fn new(config: AnalyticalStageIngressConfig) -> Arc<Self> {
        let AnalyticalStageIngressConfig {
            node_id,
            oracle_fence,
            authority,
            supervisor,
            reservations,
            spill,
            leaf,
            egress,
        } = config;
        // Sized from the one capacity root that already bounds graph
        // admission, so the queue can always hold every graph this node is
        // permitted to own at once and there is no second capacity setting.
        // Two commands per graph: at most one activation wake and one terminal
        // settlement can be outstanding for any admitted graph at a time.
        let (sender, receiver) = mpsc::channel(
            reservations
                .max_concurrent_graphs()
                .saturating_mul(2)
                .max(1),
        );
        Arc::new_cyclic(|weak: &Weak<Self>| {
            let driver = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                Some(handle.spawn(Self::drive_graph_settlements(receiver, Weak::clone(weak))))
            } else {
                tracing::warn!(
                    "Oracle analytical follower ingress was built outside a runtime; \
                     caller-drop settlement is unavailable"
                );
                None
            };
            Self {
                node_id,
                oracle_fence,
                authority,
                supervisor,
                reservations,
                spill,
                leaf,
                graphs: Mutex::new(HashMap::new()),
                egress,
                settlement: Mutex::new(Some(sender)),
                driver: Mutex::new(driver),
                accepting: AtomicBool::new(true),
                #[cfg(feature = "test-support")]
                execute_pause: Mutex::new(None),
            }
        })
    }

    /// Builds one upstream worker bound to a single graph's lifetime.
    ///
    /// [`Worker::from_session_builder`] creates a fresh task cache per call, so
    /// each graph gets its own. That is the whole point: upstream's cache holds
    /// decoded stage plans, those plans own the worker connections that charge
    /// the graph's memory envelope, and a cache shared across graphs would keep
    /// one graph's envelope charged until an unrelated idle timeout expired.
    fn build_worker(&self) -> Worker {
        Worker::from_session_builder(AnalyticalSessionBuilder::new(
            Arc::clone(self.supervisor.registry()),
            self.leaf.clone(),
            Arc::clone(&self.egress),
        ))
    }

    /// Returns the upstream worker that must serve one graph's stage operation.
    ///
    /// `None` means the graph is not live here, or has already settled and
    /// dropped its cache. Either way no stage operation may enter it, which is
    /// what keeps a settled graph from being re-populated by a late message.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned.
    pub fn graph_worker(&self, graph: &AnalyticalGraphKey) -> Result<Option<Worker>, BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
        match graphs.get(graph) {
            Some(AnalyticalGraphEntry::Active { lease, .. }) => lease.worker(),
            Some(AnalyticalGraphEntry::Draining { .. }) | None => Ok(None),
        }
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
    /// or issues storage I/O until both the server authority and the graph's own
    /// retained binding have accepted the message. The first authorized message
    /// naming a graph — `SetPlan` or `ExecuteTask`, in either order — activates
    /// it: the reservation's envelope becomes a bounded runtime, the graph
    /// registers with the supervisor, and only then is one lease published.
    /// Every equivalent later or concurrent message reuses that exact lease.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryPeerSecurity`] for every authorization,
    /// identity, or binding failure, [`BifrostError::QueryAuditUnavailable`]
    /// when the required refusal audit could not commit,
    /// [`BifrostError::QueryAdmissionRejected`] when this follower cannot admit
    /// the graph's envelope, and [`BifrostError::QueryExecutionFailed`] when the
    /// named graph is already draining.
    #[tracing::instrument(
        name = "bifrost.oracle.analytical.stage",
        skip_all,
        fields(
            operation = operation.telemetry().as_str(),
            node_id = %self.node_id.as_uuid(),
            outcome = tracing::field::Empty
        )
    )]
    pub async fn authorize_stage_message(
        &self,
        operation: StageOperationV1,
        headers: &HeaderMap,
        framed_message: &[u8],
        now: DateTime<Utc>,
    ) -> Result<AnalyticalAttemptKey, BifrostError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(BifrostError::QueryAdmissionRejected);
        }
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
        let request = graph_lease_request(&authorized, key.graph())?;
        // Before anything is decoded, cached, resolved, or read: either this
        // message activates the graph under one serialized transaction, or the
        // graph's already-fixed authority accepts it unchanged.
        let lease = self.activate_or_reuse(key.graph(), &request, &authorized, now)?;
        // Recorded from the verified claims, after the graph accepted them, so a
        // stage that this node runs in the middle of a deeper graph can sign its
        // own outbound pulls with exactly the authority it was granted.
        self.egress
            .record(key.graph(), &authorized, lease.exchanges())?;
        match operation {
            StageOperationV1::SetPlan => {
                lease.admit_attempt(
                    key,
                    AnalyticalAttemptGrant {
                        // Spill attribution belongs to the graph, not the
                        // attempt. The graph runtime's disk manager was built
                        // from the leased envelope's exact scratch share, and
                        // every attempt of the graph spills through it;
                        // splitting a second per-attempt share off the same
                        // envelope would charge the same bytes twice.
                        scratch_bytes: 0,
                    },
                )?;
                record_stage_operation(AnalyticalStageOperation::SetPlan);
            }
            StageOperationV1::ExecuteTask => {
                // Graph activation, not attempt admission. Upstream sends its
                // plan on a spawned coordinator-channel task and lets
                // `Worker::execute_task` wait for that plan to arrive, so an
                // `ExecuteTask` legitimately reaches this follower before the
                // `SetPlan` that names the same graph. Requiring the graph to
                // already exist would turn upstream's documented ordering
                // tolerance into a refusal race; the attempt guard still waits
                // for `SetPlan`, which is the message that actually names one.
                record_stage_operation(AnalyticalStageOperation::ExecuteTask);
                // The lease is active and nothing has been read yet: exactly
                // the window a peer-loss journey needs to observe.
                #[cfg(feature = "test-support")]
                if let Some(pause) = self
                    .execute_pause
                    .lock()
                    .ok()
                    .and_then(|pause| pause.as_ref().map(Arc::clone))
                {
                    pause.hold().await;
                }
            }
        }
        tracing::Span::current().record("outcome", "authorized");
        Ok(key)
    }

    /// Arms the one-shot `ExecuteTask` pause used by peer-loss journeys.
    #[cfg(feature = "test-support")]
    pub fn bind_execute_pause_for_test(&self, pause: Arc<AnalyticalExecutePause>) {
        if let Ok(mut current) = self.execute_pause.lock() {
            *current = Some(pause);
        }
    }

    /// Activates one graph exactly once, or reuses the lease already published.
    ///
    /// The graph mutex is the single serialization point, and activation is
    /// synchronous beneath it, which is what makes "first message wins" true
    /// regardless of arrival order: a concurrent duplicate either finds the
    /// published lease or waits for the mutex and then finds it. Nothing is
    /// published until the runtime, the supervisor registration, and the lease
    /// itself have all succeeded; any failure hands the reservation back under
    /// its own unchanged expiry, so a serialized waiter may still activate it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when the reservation
    /// cannot be activated or a runtime cannot be built,
    /// [`BifrostError::QueryPeerSecurity`] when the message does not match the
    /// live graph's fixed authority, [`BifrostError::QueryExecutionFailed`] when
    /// the graph is draining, and [`BifrostError::Internal`] on a poisoned lock.
    fn activate_or_reuse(
        &self,
        graph: AnalyticalGraphKey,
        request: &GraphLeaseRequest,
        authorized: &AuthorizedStage,
        now: DateTime<Utc>,
    ) -> Result<Arc<GraphLease>, BifrostError> {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
        match graphs.get(&graph) {
            Some(AnalyticalGraphEntry::Active { lease, .. }) => {
                if !lease.binding().matches(request) {
                    return Err(BifrostError::QueryPeerSecurity);
                }
                lease.binding().authorize(authorized)?;
                return Ok(Arc::clone(lease));
            }
            Some(AnalyticalGraphEntry::Draining { .. }) => {
                return Err(BifrostError::QueryExecutionFailed);
            }
            None => {}
        }
        let activation = self
            .reservations
            .begin_graph_activation(request, now)
            .map_err(|_| BifrostError::QueryAdmissionRejected)?;
        let lease = match self.publish(graph, activation, request, authorized) {
            Ok(lease) => lease,
            Err((activation, error)) => {
                activation.rollback(now);
                return Err(error);
            }
        };
        graphs.insert(
            graph,
            AnalyticalGraphEntry::Active {
                lease: Arc::clone(&lease),
                open_connections: 0,
            },
        );
        // The driver watches deadlines from this map, so it has to learn that
        // the map changed. A full queue already means the driver has unread
        // commands and will rescan, so it is not a lost signal; a closed queue
        // means nothing will ever observe this graph again, and publishing an
        // active lease no owner can end is worse than refusing the stage.
        if self.offer_settlement(GraphSettlementCommand::Wake)
            == Err(SettlementQueueRefusal::Closed)
        {
            graphs.insert(
                graph,
                AnalyticalGraphEntry::Draining {
                    lease,
                    settlement_failure: Some(SettlementQueueRefusal::Closed.detail()),
                },
            );
            return Err(BifrostError::QueryExecutionFailed);
        }
        Ok(lease)
    }

    /// Performs every fallible activation step and builds the graph's lease.
    ///
    /// Split out so the failure type carries the reservation back to the caller:
    /// the runtime build, the retained binding, and the supervisor registration
    /// are each recoverable, and the activation is only spent when all three
    /// have succeeded.
    ///
    /// # Errors
    ///
    /// Returns the untouched activation with the failure that stopped it.
    // justification: the error half must carry the rollback-owning
    // `PendingGraphActivation` back beside the failure that stopped it, so the
    // caller can hand the reservation back under its own unchanged expiry. A
    // one-use alias would only rename that pair, not simplify it.
    #[allow(clippy::type_complexity)]
    fn publish(
        &self,
        graph: AnalyticalGraphKey,
        activation: PendingGraphActivation,
        request: &GraphLeaseRequest,
        authorized: &AuthorizedStage,
    ) -> Result<Arc<GraphLease>, (Box<PendingGraphActivation>, BifrostError)> {
        let binding = match GraphLeaseBinding::activate(&activation, request, authorized) {
            Ok(binding) => binding,
            Err(error) => return Err((Box::new(activation), error)),
        };
        if let Err(error) = binding.authorize(authorized) {
            return Err((Box::new(activation), error));
        }
        let envelope = activation.envelope();
        let runtime = match self
            .spill
            .build_query_runtime(envelope.memory_pool(), envelope.scratch_bytes)
        {
            Ok(runtime) => AnalyticalGraphRuntime::new(
                runtime,
                crate::resources::OracleSessionShape::for_grant(
                    envelope.granted_memory_bytes,
                    envelope.target_partitions,
                    envelope.target_partitions,
                ),
            ),
            Err(error) => return Err((Box::new(activation), error)),
        };
        let supervisor = Arc::clone(&self.supervisor);
        let registered = runtime.clone();
        let (committed, guard) = activation
            .commit(move |resources| supervisor.register_graph(graph, resources, registered))?;
        Ok(Arc::new(GraphLease {
            binding,
            activation: committed,
            graph,
            supervisor: Arc::clone(&self.supervisor),
            egress: Arc::clone(&self.egress),
            guard: Mutex::new(Some(guard)),
            // The graph's own token, not a fresh child: the leader session that
            // opens this graph's outbound exchanges binds to the same one, so
            // settling the graph ends them.
            cancel: match self.supervisor.graph_cancellation(graph) {
                Ok(Some(cancel)) => cancel,
                Ok(None) | Err(_) => self.supervisor.root_cancellation().child_token(),
            },
            attempts: Mutex::new(HashMap::new()),
            settled: AtomicBool::new(false),
            // The supervisor's, not a fresh one: a leader session composed for
            // this same graph opens its exchanges through the same registry.
            exchanges: self
                .supervisor
                .graph_exchanges(graph)
                .ok()
                .flatten()
                .unwrap_or_default(),
            worker: Mutex::new(Some(self.build_worker())),
        }))
    }

    /// Settles one attempt and settles its graph once nothing else holds it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned,
    /// [`BifrostError::QueryExecutionFailed`] when `key` names no live graph or
    /// attempt, and the settlement failure when the graph could not be released.
    pub async fn finish_attempt(
        &self,
        key: AnalyticalAttemptKey,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<(), BifrostError> {
        let lease = {
            let graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
            match graphs.get(&key.graph()) {
                Some(
                    AnalyticalGraphEntry::Active { lease, .. }
                    | AnalyticalGraphEntry::Draining { lease, .. },
                ) => Arc::clone(lease),
                None => return Err(BifrostError::QueryExecutionFailed),
            }
        };
        lease.finish_attempt(key, outcome).await?;
        self.settle_if_idle(key.graph(), outcome).await
    }

    /// Settles one graph inline when no attempt and no connection holds it.
    ///
    /// This is the normal terminal path. It uses the same explicit settlement
    /// the caller-drop driver and shutdown use, and it removes the graph only
    /// when that settlement actually succeeded.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] on a poisoned lock and the settlement
    /// failure when cleanup did not complete.
    async fn settle_if_idle(
        &self,
        graph: AnalyticalGraphKey,
        outcome: AnalyticalAttemptOutcome,
    ) -> Result<(), BifrostError> {
        let lease = {
            let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
            let Some(AnalyticalGraphEntry::Active {
                lease,
                open_connections,
            }) = graphs.get(&graph)
            else {
                return Ok(());
            };
            if *open_connections > 0 || lease.live_attempts()? > 0 {
                return Ok(());
            }
            let lease = Arc::clone(lease);
            graphs.insert(
                graph,
                AnalyticalGraphEntry::Draining {
                    lease: Arc::clone(&lease),
                    settlement_failure: None,
                },
            );
            lease
        };
        let settled = lease.settle(outcome).await;
        self.record_settlement(graph, &settled);
        settled
    }

    /// Records what settlement did to one graph's entry.
    ///
    /// Success is the only thing that removes a graph. A failure keeps every
    /// owner and stores the reason, so readiness and shutdown can both see it.
    fn record_settlement(&self, graph: AnalyticalGraphKey, settled: &Result<(), BifrostError>) {
        let Ok(mut graphs) = self.graphs.lock() else {
            return;
        };
        match settled {
            Ok(()) => {
                graphs.remove(&graph);
            }
            Err(error) => {
                if let Some(AnalyticalGraphEntry::Draining {
                    settlement_failure, ..
                }) = graphs.get_mut(&graph)
                {
                    *settlement_failure = Some(error.to_string());
                }
            }
        }
    }

    /// Retains this graph for as long as one coordinator call stays open.
    ///
    /// The returned lease is meant to be carried by the call's own request
    /// body, so the follower's ownership ends exactly when the coordinator's
    /// connection does. Nested calls for the same graph share one release.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned and
    /// [`BifrostError::QueryExecutionFailed`] when the graph is not live.
    pub fn retain_connection(
        self: &Arc<Self>,
        graph: AnalyticalGraphKey,
    ) -> Result<AnalyticalConnectionLease, BifrostError> {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
        let Some(AnalyticalGraphEntry::Active {
            open_connections, ..
        }) = graphs.get_mut(&graph)
        else {
            return Err(BifrostError::QueryExecutionFailed);
        };
        *open_connections += 1;
        Ok(AnalyticalConnectionLease {
            ingress: Arc::clone(self),
            graph,
        })
    }

    /// Closes one coordinator connection and signals settlement at zero.
    ///
    /// Deliberately synchronous and non-blocking: it is called from a `Drop`.
    /// The transition from one open connection to zero happens under the same
    /// graph mutex activation uses, so exactly one caller ever wins it, and the
    /// asynchronous work it implies is handed to the ingress's own driver rather
    /// than to a detached task nothing joins.
    ///
    /// A queue that is full or already closed is recorded as a cleanup failure
    /// on the retained graph. It never falls back to a spawn, to synchronous
    /// cleanup, to a release, or to a successful terminal.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    fn close_connection(&self, graph: AnalyticalGraphKey) -> Result<(), BifrostError> {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
        let Some(AnalyticalGraphEntry::Active {
            open_connections, ..
        }) = graphs.get_mut(&graph)
        else {
            return Ok(());
        };
        *open_connections = open_connections.saturating_sub(1);
        if *open_connections > 0 {
            return Ok(());
        }
        self.begin_draining(&mut graphs, graph, AnalyticalAttemptOutcome::Cancelled);
        Ok(())
    }

    /// Moves one live graph to draining and hands it to the settlement driver.
    ///
    /// Called only while the graph mutex is held, which is what makes the
    /// transition happen exactly once: a second path arriving later observes
    /// `Draining` and neither settles nor enqueues the graph again.
    fn begin_draining(
        &self,
        graphs: &mut HashMap<AnalyticalGraphKey, AnalyticalGraphEntry>,
        graph: AnalyticalGraphKey,
        outcome: AnalyticalAttemptOutcome,
    ) {
        let Some(settlement) = Self::begin_draining_locked(graphs, graph, outcome) else {
            return;
        };
        let Err(refusal) = self.offer_settlement(GraphSettlementCommand::Settle(settlement)) else {
            return;
        };
        if let Some(AnalyticalGraphEntry::Draining {
            settlement_failure, ..
        }) = graphs.get_mut(&graph)
        {
            *settlement_failure = Some(refusal.detail());
        }
    }

    /// Moves one active graph to draining and names the settlement it needs.
    ///
    /// Returns `None` when `graph` is not active here, which is what makes a
    /// second path arriving later a no-op rather than a second settlement. The
    /// entry is replaced rather than removed, so a graph already draining keeps
    /// every owner and every recorded failure it had.
    fn begin_draining_locked(
        graphs: &mut HashMap<AnalyticalGraphKey, AnalyticalGraphEntry>,
        graph: AnalyticalGraphKey,
        outcome: AnalyticalAttemptOutcome,
    ) -> Option<GraphSettlement> {
        let Some(AnalyticalGraphEntry::Active { lease, .. }) = graphs.get(&graph) else {
            return None;
        };
        let lease = Arc::clone(lease);
        graphs.insert(
            graph,
            AnalyticalGraphEntry::Draining {
                lease: Arc::clone(&lease),
                settlement_failure: None,
            },
        );
        Some(GraphSettlement {
            graph,
            lease,
            outcome,
        })
    }

    /// Offers one command to the bounded settlement queue without blocking.
    ///
    /// Returns the refusal rather than a rendered reason so each caller applies
    /// its own policy: a settlement the queue cannot take is a cleanup failure,
    /// while a wake the queue cannot take is already implied by the commands
    /// that filled it.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementQueueRefusal::Full`] when every slot is taken and
    /// [`SettlementQueueRefusal::Closed`] when the queue is closed, taken by
    /// shutdown, or unreadable.
    fn offer_settlement(
        &self,
        command: GraphSettlementCommand,
    ) -> Result<(), SettlementQueueRefusal> {
        let sender = self
            .settlement
            .lock()
            .map_err(|_| SettlementQueueRefusal::Closed)?
            .clone()
            .ok_or(SettlementQueueRefusal::Closed)?;
        sender.try_send(command).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => SettlementQueueRefusal::Full,
            mpsc::error::TrySendError::Closed(_) => SettlementQueueRefusal::Closed,
        })
    }

    /// Moves every graph past its signed deadline to draining, and reports the
    /// next deadline still to watch.
    ///
    /// One pass under one lock so the settlements the driver takes and the
    /// timer it then waits on describe the same instant. The returned
    /// settlements are handed straight to the driver's own concurrent set
    /// rather than through the queue, because the driver is already the caller.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    fn expire_due_graphs(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(Vec<GraphSettlement>, Option<i64>), BifrostError> {
        let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
        let now_ms = now.timestamp_millis();
        // ponytail: linear scan of the whole graph map, which
        // `ReservationRegistry::max_concurrent_graphs()` already bounds to this
        // node's admitted graph count; a deadline-ordered priority queue is the
        // upgrade only if profiling ever shows this scan mattering.
        let due = graphs
            .iter()
            .filter_map(|(graph, entry)| match entry {
                AnalyticalGraphEntry::Active { lease, .. } => {
                    (lease.binding().absolute_deadline_ms() <= now_ms).then_some(*graph)
                }
                AnalyticalGraphEntry::Draining { .. } => None,
            })
            .collect::<Vec<_>>();
        let settlements = due
            .into_iter()
            .filter_map(|graph| {
                Self::begin_draining_locked(&mut graphs, graph, AnalyticalAttemptOutcome::Cancelled)
            })
            .collect::<Vec<_>>();
        let next = graphs
            .values()
            .filter_map(|entry| match entry {
                AnalyticalGraphEntry::Active { lease, .. } => {
                    Some(lease.binding().absolute_deadline_ms())
                }
                AnalyticalGraphEntry::Draining { .. } => None,
            })
            .min();
        Ok((settlements, next))
    }

    /// Returns how many commands the bounded settlement queue can hold.
    ///
    /// Test-only. The queue's size is the property that keeps a valid
    /// settlement from being retained as a queue-full cleanup failure, and it
    /// is deliberately derived rather than configured, so the only way to
    /// assert it is to read it back.
    #[cfg(test)]
    fn settlement_capacity(&self) -> usize {
        self.settlement
            .lock()
            .ok()
            .and_then(|settlement| settlement.as_ref().map(mpsc::Sender::max_capacity))
            .unwrap_or_default()
    }

    /// Returns the published lease for one graph, for identity assertions.
    ///
    /// Test-only. Exactly-once activation is only observable by comparing the
    /// owner two callers received, and that owner is deliberately not part of
    /// the production surface.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph lock is poisoned.
    #[cfg(test)]
    fn published(&self, graph: AnalyticalGraphKey) -> Result<Arc<GraphLease>, BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
        match graphs.get(&graph) {
            Some(
                AnalyticalGraphEntry::Active { lease, .. }
                | AnalyticalGraphEntry::Draining { lease, .. },
            ) => Ok(Arc::clone(lease)),
            None => Err(BifrostError::QueryExecutionFailed),
        }
    }

    /// Reports what this follower still owns without releasing any of it.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned.
    pub fn live(&self) -> Result<AnalyticalLiveOwnership, BifrostError> {
        let graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
        let mut attempts = 0;
        let mut cleanup_failures = 0;
        for entry in graphs.values() {
            match entry {
                AnalyticalGraphEntry::Active { lease, .. } => attempts += lease.live_attempts()?,
                AnalyticalGraphEntry::Draining {
                    lease,
                    settlement_failure,
                } => {
                    attempts += lease.live_attempts()?;
                    if settlement_failure.is_some() {
                        cleanup_failures += 1;
                    }
                }
            }
        }
        Ok(AnalyticalLiveOwnership {
            attempts,
            graphs: graphs.len(),
            cleanup_failures,
        })
    }

    /// Closes admission, settles every remaining graph, and joins the driver.
    ///
    /// The order matters and is the whole contract: stop admitting, move every
    /// live graph to draining and offer it to the queue, close the queue so the
    /// driver finishes, join the driver so no settlement outlives this call, and
    /// only then look at what is still retained. Anything left is a cleanup
    /// failure that stayed owned rather than being forgotten.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when an ownership lock is poisoned.
    pub async fn shutdown(&self) -> Result<AnalyticalSupervisorInspection, BifrostError> {
        self.accepting.store(false, Ordering::Release);
        {
            let mut graphs = self.graphs.lock().map_err(|_| poisoned_ingress())?;
            let live = graphs
                .iter()
                .filter_map(|(graph, entry)| {
                    matches!(entry, AnalyticalGraphEntry::Active { .. }).then_some(*graph)
                })
                .collect::<Vec<_>>();
            for graph in live {
                self.begin_draining(&mut graphs, graph, AnalyticalAttemptOutcome::Cancelled);
            }
        }
        drop(
            self.settlement
                .lock()
                .map_err(|_| poisoned_ingress())?
                .take(),
        );
        let driver = self.driver.lock().map_err(|_| poisoned_ingress())?.take();
        if let Some(driver) = driver
            && let Err(error) = driver.await
        {
            tracing::warn!(
                error = %error,
                "Oracle analytical settlement driver did not join cleanly"
            );
        }
        let retained = self.live()?;
        if !retained.is_clean() {
            tracing::warn!(
                graphs = retained.graphs,
                attempts = retained.attempts,
                cleanup_failures = retained.cleanup_failures,
                "Oracle analytical follower shutdown retains unsettled graph ownership"
            );
        }
        self.supervisor.shutdown().await
    }
}

/// Leader-side ownership of every participant reservation one attempt took.
///
/// The leader reserves a whole query envelope on each participant before it
/// freezes the cut, and a plan does not necessarily reach every participant it
/// froze. Something must therefore return the reservations the plan never used,
/// or a follower holds an envelope until the reservation's own expiry — long
/// enough for the node to refuse real work and for a shutdown to report
/// retained resource state.
///
/// Releasing is idempotent: a participant that already leased its reservation
/// into graph ownership no longer holds the pending entry and answers
/// successfully, so this returns exactly the unused reservations without
/// needing to know which ones the plan reached.
pub struct AnalyticalParticipantReservations {
    /// Directory the reservations were taken through, cleared once released.
    transports: Option<Arc<super::dispatcher::OraclePeerTransportDirectory>>,
    /// Every accepted reservation and the exact release it needs.
    releases: Vec<AnalyticalRetainedRelease>,
}

/// One accepted reservation the leader must return, and the bound it expires under.
///
/// The follower's own `expires_at` travels with the release because a release
/// whose acknowledgement never arrived is not evidence the follower dropped the
/// reservation. Only the follower's stated expiry proves that, and only once
/// the leader has also watched a full pending TTL elapse on its own monotonic
/// clock — a leader whose wall clock runs ahead of the follower's would
/// otherwise declare the envelope free while the follower still holds it.
struct AnalyticalRetainedRelease {
    /// Participant the reservation was accepted by.
    candidate: super::dispatcher::DispatchCandidate,
    /// Exact idempotent release this reservation needs.
    request: wyrd_spec::vala::api::ReleaseNodeSlotsRequest,
    /// Wall-clock expiry the follower minted the pending reservation with.
    expires_at: DateTime<Utc>,
    /// Local monotonic instant the reserve response was received at.
    received: tokio::time::Instant,
}

impl AnalyticalRetainedRelease {
    /// Reports whether the follower must already have dropped this reservation.
    ///
    /// Both clocks must agree: the follower-stated wall-clock expiry has passed
    /// *and* a full [`super::dispatcher::PENDING_TTL`] has elapsed locally since
    /// the response was received. Requiring the later of the two is what keeps a
    /// leader with a fast clock from forgetting an envelope a follower still owns.
    fn conservatively_expired(&self) -> bool {
        Utc::now() >= self.expires_at
            && super::dispatcher::PENDING_TTL
                .to_std()
                .is_ok_and(|ttl| self.received.elapsed() >= ttl)
    }
}

impl fmt::Debug for AnalyticalParticipantReservations {
    /// Reports how many reservations are outstanding without rendering them.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalParticipantReservations")
            .field("outstanding", &self.releases.len())
            .finish_non_exhaustive()
    }
}

impl AnalyticalParticipantReservations {
    /// Returns the owner an attempt that reserved nothing still holds.
    const fn empty() -> Self {
        Self {
            transports: None,
            releases: Vec::new(),
        }
    }

    /// Returns every reserved participant to its owner, exactly once.
    ///
    /// A per-participant failure is not propagated: the attempt is already
    /// ending, and failing the terminal because one peer was unreachable would
    /// turn a completed query into an error. It is not forgotten either — the
    /// unacknowledged records are returned so the graph's lifecycle task can
    /// retain them until they are acknowledged or conservatively expire.
    async fn release(&mut self, deadline: tokio::time::Instant) -> Vec<AnalyticalRetainedRelease> {
        let Some(transports) = self.transports.take() else {
            return Vec::new();
        };
        let mut retained = Vec::new();
        for record in self.releases.drain(..) {
            if !release_within(&transports, &record, deadline).await {
                retained.push(record);
            }
        }
        self.transports = Some(transports);
        retained
    }
}

/// Issues one release attempt under the graph's own bound, once.
///
/// A peer future that never resolves is indistinguishable from a peer that is
/// merely slow, and awaiting it directly would park cleanup, settlement,
/// conservative-expiry evaluation, and shutdown behind one unreachable node.
/// Bounding every attempt by the earlier of the graph's absolute deadline and
/// the retained-release cadence is what keeps the sequence itself finite, and a
/// timeout is treated exactly like an unacknowledged release: the record stays
/// retained until it is acknowledged or conservatively expires.
///
/// Returns whether the participant acknowledged. Once the graph deadline has
/// elapsed this issues no RPC at all and reports the release as unacknowledged,
/// because the envelope's own bound is the last moment this graph may address a
/// peer.
async fn release_within(
    transports: &super::dispatcher::OraclePeerTransportDirectory,
    record: &AnalyticalRetainedRelease,
    deadline: tokio::time::Instant,
) -> bool {
    let now = tokio::time::Instant::now();
    if now >= deadline {
        tracing::warn!(
            node_id = %record.candidate.node_id.as_uuid(),
            "Oracle analytical leader passed its deadline before releasing a reservation"
        );
        return false;
    }
    let bound = deadline.min(now + RETAINED_RELEASE_RETRY);
    match tokio::time::timeout_at(
        bound,
        transports.release_graph_reservation(&record.candidate, record.request.clone()),
    )
    .await
    {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            tracing::warn!(
                error = ?error,
                node_id = %record.candidate.node_id.as_uuid(),
                "Oracle analytical leader could not release a participant reservation"
            );
            false
        }
        Err(_) => {
            tracing::warn!(
                node_id = %record.candidate.node_id.as_uuid(),
                "Oracle analytical leader's participant release did not answer in time"
            );
            false
        }
    }
}

/// What the attempt has asked its graph's lifecycle task to do next.
///
/// Monotonic: an attempt only ever moves forward through these, so the task can
/// treat a repeated value as already handled rather than reserving twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyticalGraphControl {
    /// The graph is registered and nothing has been reserved.
    Registered,
    /// Selection is final; reserve the participant cut before dispatch.
    ReserveRequested,
    /// The graph reached its first terminal; settle everything it owns.
    ///
    /// Carries the outcome the first signal chose. A later signal observes the
    /// graph already draining and never replaces it.
    Terminal(AnalyticalAttemptOutcome),
}

/// What the graph's lifecycle task has published back to its attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyticalGraphResult {
    /// Nothing has been reserved yet, so nothing may be dispatched.
    Pending,
    /// Every participant accepted and the complete cut is published.
    ReservationReady,
    /// A participant declined; every accepted reservation was released.
    ReservationFailed,
    /// Cleanup completed; every owner was released before the graph was removed.
    ///
    /// Carries the attempt's release evidence when the graph supervised one.
    SettledSuccess(Option<AnalyticalAttemptRelease>),
    /// Cleanup did not complete; the graph is retained as `Draining`.
    SettledFailure,
}

/// The attempt-side half of one graph's lifecycle task.
///
/// Deliberately holds no task handle and no control sender: both live in the
/// supervisor's graph entry, so a caller may ask for settlement and await its
/// result but can never take, abort, or reproduce the cleanup sequence. Dropping
/// this value signals cancellation and nothing else.
pub struct AnalyticalGraphSignals {
    /// Supervisor every control signal is issued through.
    supervisor: Arc<AnalyticalSupervisor>,
    /// The graph these signals address.
    graph: AnalyticalGraphKey,
    /// Clonable result channel this caller awaits settlement on.
    result: tokio::sync::watch::Receiver<AnalyticalGraphResult>,
    /// The graph-owned cell the reserved cut is published into.
    participants: Arc<std::sync::OnceLock<Arc<AnalyticalParticipantCut>>>,
}

impl fmt::Debug for AnalyticalGraphSignals {
    /// Reports whether the cut is published without rendering it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalGraphSignals")
            .field("published", &self.participants.get().is_some())
            .finish_non_exhaustive()
    }
}

impl AnalyticalGraphSignals {
    /// Returns the cell every channel this attempt resolves reads its cut from.
    #[must_use]
    pub(super) fn participants(&self) -> Arc<std::sync::OnceLock<Arc<AnalyticalParticipantCut>>> {
        Arc::clone(&self.participants)
    }

    /// Stores the admission owner on the graph itself, synchronously.
    ///
    /// The graph outlives every caller that could hold the permit, so the graph
    /// is where it belongs: a cleanup that cannot be confirmed keeps the entry,
    /// and keeps the query's admission charged with it.
    ///
    /// # Errors
    ///
    /// Returns the guard unchanged when the graph is gone or already retains a
    /// permit, so a refused transfer leaves the caller holding it rather than
    /// losing the query's admission.
    fn retain_admission(
        &self,
        admitted: super::admission::AdmittedQueryGuard,
    ) -> Result<(), Box<super::admission::AdmittedQueryGuard>> {
        self.supervisor.retain_admission(self.graph, admitted)
    }

    /// Stores this graph's physical-metric fold on the graph entry itself.
    ///
    /// # Errors
    ///
    /// Returns the supervisor's refusal unchanged when the graph is not
    /// registered active or already retains a fold.
    fn retain_metric_fold(&self, fold: AnalyticalGraphMetricFold) -> Result<(), BifrostError> {
        self.supervisor.retain_metric_fold(self.graph, fold)
    }

    /// Clones this graph's settlement receiver for one more observer.
    ///
    /// Every clone reads the same monotonic channel, which is how settlement
    /// evidence proves a raced success, cancellation, and failure all observe
    /// the one outcome the first terminal signal chose.
    #[cfg(test)]
    pub(super) fn settled_receiver(&self) -> tokio::sync::watch::Receiver<AnalyticalGraphResult> {
        self.result.clone()
    }

    /// Waits for this graph's lifecycle task to publish its settlement.
    ///
    /// Every caller holds its own clone of the same receiver, so a raced
    /// success, cancellation, and peer failure all observe the one settlement
    /// the first terminal signal chose.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] carrying the recorded cleanup failure
    /// when the graph is retained as draining, and
    /// [`BifrostError::QueryExecutionFailed`] when the task ended without
    /// publishing a settlement.
    async fn settled(&self) -> Result<Option<AnalyticalAttemptRelease>, BifrostError> {
        let mut result = self.result.clone();
        loop {
            match *result.borrow_and_update() {
                AnalyticalGraphResult::SettledSuccess(release) => return Ok(release),
                AnalyticalGraphResult::SettledFailure => {
                    let detail = self
                        .supervisor
                        .graph_settlement_failure(self.graph)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| {
                            "Oracle analytical graph cleanup did not complete".to_owned()
                        });
                    return Err(BifrostError::Internal { detail });
                }
                AnalyticalGraphResult::Pending
                | AnalyticalGraphResult::ReservationReady
                | AnalyticalGraphResult::ReservationFailed => {}
            }
            if result.changed().await.is_err() {
                return Err(BifrostError::QueryExecutionFailed);
            }
        }
    }

    /// Reserves the participant cut once and parks until it is published.
    ///
    /// Called exactly where selection becomes irreversible: after the physical
    /// plan is known to be supported and to carry an exchange, and before the
    /// first stage is dispatched. Repeat calls are idempotent — the task
    /// reserves once and republishes the same result — so an orchestration that
    /// re-enters this before dispatch cannot charge a follower twice.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when a participant
    /// declined its reservation, and [`BifrostError::QueryExecutionFailed`]
    /// when the graph's lifecycle task is gone.
    pub async fn publish_participants(&self) -> Result<(), BifrostError> {
        self.supervisor.signal_reserve(self.graph)?;
        let mut result = self.result.clone();
        loop {
            match *result.borrow_and_update() {
                AnalyticalGraphResult::Pending => {}
                AnalyticalGraphResult::ReservationReady => return Ok(()),
                AnalyticalGraphResult::ReservationFailed => {
                    return Err(BifrostError::QueryAdmissionRejected);
                }
                // A settlement may overwrite the reservation verdict, because
                // the result channel keeps only its latest value. The published
                // cut is the durable record: it is set once, only after every
                // participant accepted, so its absence *is* the refusal.
                AnalyticalGraphResult::SettledSuccess(_)
                | AnalyticalGraphResult::SettledFailure => {
                    return Err(if self.participants.get().is_some() {
                        BifrostError::QueryExecutionFailed
                    } else {
                        BifrostError::QueryAdmissionRejected
                    });
                }
            }
            if result.changed().await.is_err() {
                return Err(BifrostError::QueryExecutionFailed);
            }
        }
    }

    /// Asks the supervisor to settle this graph under `outcome`.
    ///
    /// Only the first signal chooses the outcome; a later one observes the
    /// graph already draining and awaits the same settlement.
    pub(super) fn terminal(&self, outcome: AnalyticalAttemptOutcome) {
        if let Err(error) = self.supervisor.signal_terminal(self.graph, outcome) {
            tracing::error!(
                %error,
                public_query_id = %self.graph.public_query_id,
                "Oracle analytical terminal signal was refused"
            );
        }
    }
}

impl Drop for AnalyticalGraphSignals {
    /// Signals cancellation, and only that.
    ///
    /// A dropped caller — a client that walked away, a raw stream drop — must
    /// not spawn, abort, release, or reproduce the cleanup sequence. The task
    /// stays owned by the supervisor, so it runs the one sequence and shutdown
    /// still joins it.
    fn drop(&mut self) {
        self.terminal(AnalyticalAttemptOutcome::Cancelled);
    }
}

/// The task-side half of one graph's lifecycle, owning every reservation it takes.
///
/// Reservation is graph-wide and bulk, and it outlives the attempt that asked
/// for it: a release whose acknowledgement never arrived has to be retried
/// after the query has already failed. Keeping that follow-up on this one task
/// is what avoids a detached timer, a second reservation registry, or a status
/// RPC.
pub(super) struct AnalyticalGraphLifecycle {
    /// The graph every reservation and retained release belongs to.
    graph: AnalyticalGraphKey,
    /// Supervisor the retained-cleanup state is made visible through.
    supervisor: Arc<AnalyticalSupervisor>,
    /// Directory every reserve and release is issued through.
    transports: Option<Arc<super::dispatcher::OraclePeerTransportDirectory>>,
    /// Frozen remote participants, already excluding this coordinator.
    remote: Vec<(Url, super::dispatcher::DispatchCandidate)>,
    /// The exact reserve request every participant receives.
    request: ReserveNodeSlotsRequest,
    /// The graph-owned cell the complete cut is published into.
    participants: Arc<std::sync::OnceLock<Arc<AnalyticalParticipantCut>>>,
    /// Control channel the attempt drives this task through.
    control: tokio::sync::watch::Receiver<AnalyticalGraphControl>,
    /// Result channel this task publishes its progress on.
    result: tokio::sync::watch::Sender<AnalyticalGraphResult>,
    /// The graph's own attempt, joined by this task and nothing else.
    attempt: Option<AnalyticalAttemptGuard>,
    /// The graph's own cancellation child, shared with every descendant.
    ///
    /// Held rather than resolved per use so a peer call can be interrupted
    /// after the graph entry has already moved to `Draining`, which is the
    /// exact window a cancelled reservation has to survive.
    cancel: CancellationToken,
    /// The absolute monotonic bound covering reserve, cleanup, and settlement.
    ///
    /// The query's own admission deadline, not a second timer: one envelope has
    /// one deadline, and every peer RPC this graph issues ends by it.
    deadline: tokio::time::Instant,
}

/// How often a retained release is retried while it is neither acknowledged nor expired.
///
/// A quarter of the pending TTL, so a follower that becomes reachable again is
/// acknowledged well inside the window rather than only at its end.
const RETAINED_RELEASE_RETRY: std::time::Duration = std::time::Duration::from_millis(500);

/// Detail one graph records while a participant release stays unacknowledged.
///
/// Shared by the drain that first observes it and by the settlement that fails
/// the query on it, so the residue a node reports and the failure its caller
/// receives name the same condition.
/// Detail one graph records when its physical metric fold outlives the deadline.
///
/// Named alongside the reservation detail because both describe the same class
/// of end: the deadline arrived before this node could confirm what a peer did,
/// so the graph is retained rather than reported clean.
const METRIC_FOLD_EXPIRED: &str =
    "Oracle analytical distributed metrics did not settle before the graph deadline";

/// Detail one graph records when the follower-metric rewrite itself fails.
///
/// Distinct from the deadline detail because the two are distinguishable and
/// operationally different: a timeout says a peer never answered, a refusal
/// says the dependency could not build the metric-carrying plan at all. Both
/// retain the graph; only this one names an error the dependency reported.
const METRIC_FOLD_REFUSED: &str = "Oracle analytical distributed metric rewrite failed";

const RETAINED_RELEASE_UNACKNOWLEDGED: &str =
    "Oracle analytical participant release was not acknowledged";

impl AnalyticalGraphLifecycle {
    /// Starts one graph's lifecycle task and returns its attempt-side half.
    ///
    /// Nothing is reserved here. The task exists from registration so the
    /// attempt has one owner to signal, and so the cell every channel resolves
    /// through is the graph's own from the moment the session is built.
    pub(super) fn start(
        graph: AnalyticalGraphKey,
        supervisor: Arc<AnalyticalSupervisor>,
        transports: Option<Arc<super::dispatcher::OraclePeerTransportDirectory>>,
        remote: Vec<(Url, super::dispatcher::DispatchCandidate)>,
        request: ReserveNodeSlotsRequest,
        deadline: tokio::time::Instant,
        owners: AnalyticalGraphLifecycleOwners,
    ) -> Result<AnalyticalGraphSignals, BifrostError> {
        let participants = Arc::new(std::sync::OnceLock::new());
        let (control, control_rx) = tokio::sync::watch::channel(AnalyticalGraphControl::Registered);
        let (result, result_rx) = tokio::sync::watch::channel(AnalyticalGraphResult::Pending);
        let AnalyticalGraphLifecycleOwners {
            attempt,
            graph_guard,
        } = owners;
        let cancel = supervisor
            .graph_cancellation(graph)?
            .ok_or(BifrostError::QueryExecutionFailed)?;
        // Disarmed the moment the task takes ownership: from here the one
        // cleanup sequence decides whether the graph is removed. An unwinding
        // or aborted task must leave the entry standing so the supervisor can
        // record the failure against it, and a guard's `Drop` would instead
        // return an envelope whose owners were never confirmed released.
        if let Some(guard) = graph_guard {
            guard.retain();
        }
        let lifecycle = Self {
            graph,
            supervisor: Arc::clone(&supervisor),
            transports,
            remote,
            request,
            participants: Arc::clone(&participants),
            control: control_rx,
            result,
            attempt,
            cancel,
            deadline,
        };
        let handle = tokio::spawn(lifecycle.run());
        supervisor.attach_lifecycle(
            graph,
            AnalyticalGraphLifecycleOwner::new(control, result_rx.clone(), handle),
        )?;
        Ok(AnalyticalGraphSignals {
            supervisor,
            graph,
            result: result_rx,
            participants,
        })
    }

    /// Runs the graph's whole reservation and cleanup lifecycle.
    ///
    /// Ends when the attempt signals its terminal or drops the control channel,
    /// and then only after every accepted reservation has been returned or has
    /// conservatively expired.
    async fn run(mut self) {
        let mut reservations = AnalyticalParticipantReservations::empty();
        let mut requested = false;
        let outcome = loop {
            if self.control.changed().await.is_err() {
                // Every signal half is gone and no terminal arrived: the caller
                // walked away, which is a cancellation rather than a success.
                break AnalyticalAttemptOutcome::Cancelled;
            }
            let control = *self.control.borrow_and_update();
            match control {
                AnalyticalGraphControl::Registered => {}
                AnalyticalGraphControl::ReserveRequested => {
                    if requested {
                        continue;
                    }
                    requested = true;
                    match self.reserve().await {
                        Ok(taken) => {
                            reservations = taken;
                            let _ = self.result.send(AnalyticalGraphResult::ReservationReady);
                        }
                        Err(taken) => {
                            reservations = taken;
                            let _ = self.result.send(AnalyticalGraphResult::ReservationFailed);
                            // A refused reservation is itself the terminal: no
                            // dispatch can follow it, so nothing is gained by
                            // waiting for a caller to say so.
                            break AnalyticalAttemptOutcome::Failed;
                        }
                    }
                }
                AnalyticalGraphControl::Terminal(outcome) => break outcome,
            }
        };
        self.settle(outcome, reservations).await;
    }

    /// Folds this graph's retained physical metrics inside its own deadline.
    ///
    /// The follower fold is unbounded on its own — upstream reports task
    /// metrics only once every coordinator channel closes — so a peer that
    /// never answers would otherwise hold settlement open forever. The graph's
    /// absolute deadline already covers execution, cleanup, and settlement, so
    /// it covers this too, and an expiry is a distributed execution failure
    /// rather than a silently zeroed metric.
    ///
    /// A graph that never distributed retains no fold and settles unchanged.
    ///
    /// A rewrite the dependency refuses is the same class of failure: the
    /// coordinator never obtained the plan that carries the executed stages'
    /// counters, so there is nothing to publish and no unexecuted plan may
    /// stand in for it.
    ///
    /// # Errors
    ///
    /// Returns the detail recorded against the graph when the fold does not
    /// complete before the deadline, or when the follower-metric rewrite
    /// itself fails.
    async fn fold_physical_metrics(&self) -> Result<(), String> {
        let Some(fold) = self.supervisor.take_metric_fold(self.graph) else {
            return Ok(());
        };
        let remaining = self
            .deadline
            .saturating_duration_since(tokio::time::Instant::now());
        let Ok(folded) = tokio::time::timeout(remaining, fold.settle()).await else {
            return Err(METRIC_FOLD_EXPIRED.to_owned());
        };
        let evidence = folded.map_err(|error| format!("{METRIC_FOLD_REFUSED}: {error}"))?;
        if let Some(evidence) = &evidence {
            super::telemetry::record_output_sort_spill(
                evidence.spill_count,
                evidence.spilled_bytes,
                evidence.spilled_rows,
            );
        }
        // Recorded whether or not the plan carried an output sort: a settled
        // graph is the fact a test-tier caller waits on, and a plan without a
        // sort settles just as completely as one with it.
        #[cfg(feature = "test-support")]
        self.supervisor.record_settlement(evidence);
        Ok(())
    }

    /// Performs the one cleanup sequence and publishes its settlement.
    ///
    /// The order is the invariant, and this is the only place it exists: move
    /// the graph out of `Active` so nothing new is admitted, cancel unless the
    /// graph succeeded, join the attempt and every descendant it retained,
    /// return every participant reservation, wait for the envelope's own nested
    /// children to go idle, release the graph, and only then release the
    /// admission owner. Cleanup that cannot be confirmed leaves every
    /// unresolved owner in the `Draining` entry and publishes a failure, so a
    /// success terminal is unreachable and readiness stays false.
    async fn settle(
        mut self,
        outcome: AnalyticalAttemptOutcome,
        mut reservations: AnalyticalParticipantReservations,
    ) {
        let _ = self.supervisor.signal_terminal(self.graph, outcome);
        if outcome != AnalyticalAttemptOutcome::Success
            && let Ok(Some(cancel)) = self.supervisor.graph_cancellation(self.graph)
        {
            cancel.cancel();
        }
        // Folded first, and only for a success terminal: the physical evidence
        // this publishes describes rows a caller actually received, and an
        // abandoned graph has no follower metrics coming. It runs before the
        // attempt is joined so spill evidence is in hand before scratch is
        // verified, success is published, and the graph is removed.
        let mut failure = if outcome == AnalyticalAttemptOutcome::Success {
            self.fold_physical_metrics().await.err()
        } else {
            None
        };
        let mut release = None;
        if let Some(attempt) = self.attempt.take() {
            match attempt.finish(outcome).await {
                Ok(settled) => release = Some(settled),
                Err(error) => {
                    if failure.is_none() {
                        failure = Some(error.to_string());
                    }
                }
            }
        }
        // Cancellation alone cannot end an exchange whose consumer stopped
        // polling it: upstream keeps a reader task, and the buffers it charges
        // to this query's pool, alive until every partition stream it handed
        // out is dropped. Closing the graph's own exchange registry is what
        // drops them, and it has to happen before the envelope's children are
        // waited on or the wait would be for bytes nothing will release.
        if let Ok(Some(exchanges)) = self.supervisor.graph_exchanges(self.graph) {
            exchanges.close();
        }
        let retained = reservations.release(self.deadline).await;
        // A cleanup failure that is not the reservation ambiguity retains this
        // graph however the follower answers, so only an otherwise-clean
        // settlement has anything to gain from following the records up.
        let otherwise_clean = failure.is_none();
        let unresolved = self.drain(retained).await;
        if failure.is_none() && !unresolved.is_empty() {
            failure = Some(RETAINED_RELEASE_UNACKNOWLEDGED.to_owned());
        }
        let mut running_query = None;
        if failure.is_none() {
            match self.release_graph().await {
                Ok(owner) => running_query = owner,
                Err(error) => failure = Some(error.to_string()),
            }
        }
        // Retired only here, after every attempt, exchange, participant,
        // runtime, scratch, and admission owner joined: the public entry
        // describes the graph, so it outlives the stream that signalled it.
        if let Some(mut owner) = running_query {
            owner.finish(if outcome == AnalyticalAttemptOutcome::Success {
                wyrd_spec::vala::api::QueryTerminalOutcome::Success
            } else {
                wyrd_spec::vala::api::QueryTerminalOutcome::Failed
            });
        }
        match failure {
            None => {
                let _ = self
                    .result
                    .send(AnalyticalGraphResult::SettledSuccess(release));
            }
            Some(detail) => {
                tracing::error!(
                    public_query_id = %self.graph.public_query_id,
                    datafusion_query_id = %self.graph.datafusion_query_id,
                    detail,
                    "Oracle analytical graph cleanup did not complete"
                );
                // Retained, never released: the graph keeps every owner it
                // still holds — its envelope and its admission permit included
                // — so the residue stays attributable to this node.
                self.supervisor.retain_graph_cleanup(self.graph, detail);
                let _ = self.result.send(AnalyticalGraphResult::SettledFailure);
            }
        }
        if otherwise_clean && !unresolved.is_empty() {
            self.expire(unresolved).await;
        }
    }

    /// Holds a post-deadline reservation until it must have expired remotely.
    ///
    /// The deadline ends this node's right to *address* a participant, not its
    /// ownership of what it took: until every record is authoritatively expired
    /// this node cannot say whether a follower still holds an envelope on its
    /// behalf, and the graph stays `Draining` for exactly that reason. So the
    /// caller has already been failed and no further RPC is issued; only the
    /// local two-clock predicate is re-evaluated, at the same retry cadence.
    ///
    /// Returns early once the supervisor stops accepting, because shutdown is
    /// joining this task and a record that has not expired yet is residue it
    /// must report rather than a wait it should serve. When every record does
    /// expire, the graph, its envelope, and the admission permit it retained
    /// are released together; a refusal there is recorded and leaves the entry
    /// standing, exactly as the settlement sequence would.
    async fn expire(&mut self, mut retained: Vec<AnalyticalRetainedRelease>) {
        while !retained.is_empty() {
            if !self.supervisor.is_healthy() {
                return;
            }
            tokio::time::sleep(RETAINED_RELEASE_RETRY).await;
            retained.retain(|record| !record.conservatively_expired());
        }
        // Released directly rather than through the deadline-bound wait: that
        // wait exists to hold an ordinary settlement inside the deadline, and
        // this one is past it by construction. The supervisor's own refusal
        // still covers a graph whose envelope a child has not returned.
        if let Err(error) = self.supervisor.release_graph(self.graph) {
            tracing::error!(
                public_query_id = %self.graph.public_query_id,
                error = %error,
                "Oracle analytical graph could not be released after its reservations expired"
            );
            self.supervisor
                .retain_graph_cleanup(self.graph, error.to_string());
        }
    }

    /// Waits, bounded, for the envelope's children and then releases the graph.
    ///
    /// Two bounds apply and the earlier one wins: the poll count, and the
    /// graph's own absolute deadline. The deadline is checked *before* the
    /// children are, because it is the bound the caller was promised and
    /// idleness is not: a wait that ran past the deadline has already overrun
    /// it, so finding the children idle at that point cannot turn the overrun
    /// into a success. Only the post-deadline conservative-expiry path may
    /// remove such a graph.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the graph deadline arrives first
    /// or a nested child of the query envelope is still live after the poll
    /// count, and the supervisor's refusal when the graph itself cannot be
    /// released.
    async fn release_graph(
        &mut self,
    ) -> Result<Option<super::query_stream::RunningQueryTerminalOwner>, BifrostError> {
        for _ in 0..GRAPH_DRAIN_POLLS {
            let now = tokio::time::Instant::now();
            if now >= self.deadline {
                break;
            }
            if self.supervisor.graph_children_idle(self.graph)? {
                #[cfg(feature = "test-support")]
                analytical_cleanup_pause_for_test().hold().await;
                // Removing the entry drops the envelope and only then the
                // admission permit it retained, so the counters that wake the
                // next query are returned last.
                return self.supervisor.release_graph(self.graph);
            }
            tokio::time::sleep_until(self.deadline.min(now + GRAPH_DRAIN_INTERVAL)).await;
        }
        let (scratch_bytes, memory_bytes) = self.supervisor.graph_children_debt(self.graph)?;
        tracing::warn!(
            public_query_id = %self.graph.public_query_id,
            datafusion_query_id = %self.graph.datafusion_query_id,
            scratch_bytes,
            memory_bytes,
            "Oracle analytical leader graph was not confirmed drained within its deadline"
        );
        Err(BifrostError::Internal {
            detail: "Oracle analytical graph cleanup did not complete".to_owned(),
        })
    }

    /// Reserves every remote participant and publishes the complete cut once.
    ///
    /// The cell is set only after the last participant has accepted and the
    /// destination map has been frozen with the follower-minted reservation
    /// identities, so a partial cut is never observable and no dispatch can
    /// address a participant that has not agreed to hold the envelope.
    ///
    /// # Errors
    ///
    /// Returns the accepted-reservation owner unchanged when a participant
    /// declined or the cut could not be frozen, so the caller releases exactly
    /// what was taken.
    async fn reserve(
        &self,
    ) -> Result<AnalyticalParticipantReservations, AnalyticalParticipantReservations> {
        if self.remote.is_empty() {
            let _ = self.participants.set(Arc::new(
                AnalyticalParticipantCut::freeze(HashMap::new())
                    .unwrap_or_else(|_| unreachable!("an empty cut always freezes")),
            ));
            return Ok(AnalyticalParticipantReservations::empty());
        }
        let Some(transports) = self.transports.as_ref() else {
            tracing::error!(
                "Oracle analytical leader has no peer transport to reserve participants through"
            );
            return Err(AnalyticalParticipantReservations::empty());
        };
        let mut destinations = HashMap::with_capacity(self.remote.len());
        // Held from the first acceptance, so a later participant's refusal
        // still returns everything already taken rather than stranding the
        // peers that said yes.
        let mut reserved = AnalyticalParticipantReservations {
            transports: Some(Arc::clone(transports)),
            releases: Vec::with_capacity(self.remote.len()),
        };
        for (url, candidate) in &self.remote {
            // Bounded on both edges: the graph's cancellation ends reservation
            // the moment the attempt is gone, and the envelope's own absolute
            // deadline ends it when a participant simply never answers. Either
            // is the existing reservation-failed path, and `reserved` already
            // carries every acceptance so far, so nothing taken is stranded.
            let pending = tokio::select! {
                biased;
                () = self.cancel.cancelled() => {
                    tracing::warn!(
                        node_id = ?candidate.node_id,
                        url = %url,
                        "Oracle analytical graph was cancelled while reserving a participant"
                    );
                    return Err(reserved);
                }
                answered = tokio::time::timeout_at(
                    self.deadline,
                    transports.reserve_graph(candidate, self.request.clone()),
                ) => match answered {
                    Ok(Ok(pending)) => pending,
                    Ok(Err(error)) => {
                        tracing::warn!(
                            node_id = ?candidate.node_id,
                            url = %url,
                            error = %error,
                            "Oracle analytical participant refused a graph reservation"
                        );
                        return Err(reserved);
                    }
                    Err(_) => {
                        tracing::warn!(
                            node_id = ?candidate.node_id,
                            url = %url,
                            "Oracle analytical participant did not answer a reservation in time"
                        );
                        return Err(reserved);
                    }
                },
            };
            reserved.releases.push(AnalyticalRetainedRelease {
                candidate: candidate.clone(),
                request: wyrd_spec::vala::api::ReleaseNodeSlotsRequest {
                    reservation_id: pending.reservation_id,
                    query_id: self.request.query_id,
                    leader_node_id: self.request.leader_node_id,
                    leader_fencing_token: self.request.leader_fencing_token,
                },
                expires_at: pending.expires_at,
                received: tokio::time::Instant::now(),
            });
            destinations.insert(
                url.clone(),
                AnalyticalDestination {
                    node_id: candidate.node_id,
                    fence: candidate.worker_fence,
                    reservation_id: pending.reservation_id.as_uuid().to_string(),
                },
            );
        }
        let cut = match AnalyticalParticipantCut::freeze(destinations) {
            Ok(cut) => cut,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Oracle analytical leader could not freeze its participant cut"
                );
                return Err(reserved);
            }
        };
        let _ = self.participants.set(Arc::new(cut));
        Ok(reserved)
    }

    /// Retains every unacknowledged release until it resolves, expires, or the
    /// graph's own deadline ends this node's right to address a peer.
    ///
    /// The graph stays supervisor-visible as draining for exactly as long as
    /// this runs, because until every record resolves this node cannot say
    /// whether a follower is still holding an envelope on its behalf.
    ///
    /// Returns the records still unresolved at the graph deadline, empty when
    /// every one of them resolved. Retrying past that deadline would be an
    /// unbounded wait on an unreachable peer, so the remote calls stop there
    /// and the entry stays `Draining`; the returned records are what the
    /// lifecycle task then expires locally.
    async fn drain(
        &self,
        mut retained: Vec<AnalyticalRetainedRelease>,
    ) -> Vec<AnalyticalRetainedRelease> {
        if retained.is_empty() {
            return Vec::new();
        }
        let _ = self
            .supervisor
            .signal_terminal(self.graph, AnalyticalAttemptOutcome::Failed);
        self.supervisor
            .retain_graph_cleanup(self.graph, RETAINED_RELEASE_UNACKNOWLEDGED.to_owned());
        while !retained.is_empty() {
            let now = tokio::time::Instant::now();
            if now >= self.deadline {
                tracing::warn!(
                    public_query_id = %self.graph.public_query_id,
                    outstanding = retained.len(),
                    "Oracle analytical leader reached its deadline holding participant reservations"
                );
                return retained;
            }
            tokio::time::sleep_until(self.deadline.min(now + RETAINED_RELEASE_RETRY)).await;
            let mut remaining = Vec::with_capacity(retained.len());
            for record in retained.drain(..) {
                let acknowledged = match self.transports.as_ref() {
                    Some(transports) => release_within(transports, &record, self.deadline).await,
                    None => true,
                };
                if !acknowledged && !record.conservatively_expired() {
                    remaining.push(record);
                }
            }
            retained = remaining;
        }
        self.supervisor.resolve_graph_cleanup(self.graph);
        Vec::new()
    }
}

/// Slot units one distributed graph reserves on each participant.
///
/// A graph occupies a participant for the whole plan, not for one leaf, so it
/// charges the Analytical class's full per-query demand. The receiving node
/// clamps this to its own running capacity before charging, so a smaller peer
/// still admits the graph rather than refusing a structurally unschedulable
/// demand.
pub(crate) const ANALYTICAL_GRAPH_SLOT_UNITS: u32 = 2;

/// Follower ownership of one graph, held for one open coordinator call.
///
/// Dropping it is the release signal. The release itself is asynchronous — an
/// attempt settles through the supervisor — so the drop spawns it onto the
/// current runtime rather than blocking whatever dropped the connection.
pub struct AnalyticalConnectionLease {
    /// Follower ingress that owns the graph this lease keeps alive.
    ingress: Arc<AnalyticalStageIngress>,
    /// Graph this lease is one open connection for.
    graph: AnalyticalGraphKey,
}

impl fmt::Debug for AnalyticalConnectionLease {
    /// Reports the graph without rendering the ingress.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalConnectionLease")
            .field("graph", &self.graph)
            .finish_non_exhaustive()
    }
}

impl Drop for AnalyticalConnectionLease {
    /// Closes this connection's share of the graph's follower ownership.
    ///
    /// Synchronous and non-blocking by construction: it decrements under the
    /// ingress's graph mutex and, at zero, hands the graph to the ingress's own
    /// settlement driver. It never spawns a detached task, never blocks on
    /// cleanup, and never releases the graph itself, so a dropped connection
    /// cannot outlive or race the owner that must join it.
    fn drop(&mut self) {
        if let Err(error) = self.ingress.close_connection(self.graph) {
            tracing::warn!(
                error = %error,
                "Oracle analytical follower could not close a stage connection"
            );
        }
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

/// Projects the graph-lease ownership tuple from one authorized stage operation.
///
/// Every field comes from the verified claims. The reservation identity in
/// particular is signed, so a coordinator cannot point a graph at a reservation
/// it was not granted, and the query identity is derived from the same claims
/// the graph key itself came from, so the reservation and the graph can only
/// ever be checked against each other.
///
/// # Errors
///
/// Returns [`BifrostError::QueryPeerSecurity`] when the claims carry a
/// reservation identity that is not a well-formed reservation UUID.
fn graph_lease_request(
    authorized: &AuthorizedStage,
    graph: AnalyticalGraphKey,
) -> Result<GraphLeaseRequest, BifrostError> {
    let reservation_id = Uuid::parse_str(&authorized.claims.reservation_id)
        .map(ReservationId::new)
        .map_err(|_| BifrostError::QueryPeerSecurity)?;
    Ok(GraphLeaseRequest {
        reservation_id,
        graph: AnalyticalGraphRef {
            public_query_id: graph.public_query_id.as_uuid(),
            datafusion_query_id: graph.datafusion_query_id.as_uuid(),
        },
        query_id: QueryId::new(graph.public_query_id.as_uuid()),
    })
}

/// Builds the stable internal error for a poisoned ingress ownership lock.
fn poisoned_ingress() -> BifrostError {
    BifrostError::Internal {
        detail: "Oracle analytical stage ingress lock is poisoned".to_owned(),
    }
}

/// One-shot pause held immediately before a clean Analytical graph release.
///
/// A journey needs a window in which every owner is still held but cleanup has
/// already begun, which is otherwise unobservable: the release is a single
/// synchronous step. Arming this pause stops exactly one graph on that step
/// until the test releases it, so status, ownership, and readiness can be read
/// while the query is genuinely mid-cleanup.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct AnalyticalCleanupPause {
    /// Whether one graph should still be stopped before its release.
    armed: std::sync::atomic::AtomicBool,
    /// Whether a graph has reached the pause.
    entered: std::sync::atomic::AtomicBool,
    /// Wakes a waiter once a graph reaches the pause.
    entered_notify: tokio::sync::Notify,
    /// Wakes the paused graph once the test releases it.
    release_notify: tokio::sync::Notify,
}

#[cfg(feature = "test-support")]
impl AnalyticalCleanupPause {
    /// Arms the pause for the next graph that reaches a clean release.
    pub fn arm(&self) {
        self.entered
            .store(false, std::sync::atomic::Ordering::Release);
        self.armed.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Waits until a graph is stopped on its release step.
    pub async fn wait_entered(&self) {
        while !self.entered.load(std::sync::atomic::Ordering::Acquire) {
            self.entered_notify.notified().await;
        }
    }

    /// Releases the paused graph and disarms the pause.
    pub fn release(&self) {
        self.armed
            .store(false, std::sync::atomic::Ordering::Release);
        self.release_notify.notify_waiters();
    }

    /// Stops one armed graph here, consuming the arming exactly once.
    ///
    /// Unarmed graphs pass straight through, so production release behavior is
    /// unchanged when nothing is testing it.
    async fn hold(&self) {
        if !self.armed.swap(false, std::sync::atomic::Ordering::AcqRel) {
            return;
        }
        let released = self.release_notify.notified();
        tokio::pin!(released);
        self.entered
            .store(true, std::sync::atomic::Ordering::Release);
        self.entered_notify.notify_waiters();
        released.await;
    }
}

/// Process-wide cleanup pause shared by the test harness and the lifecycle.
#[cfg(feature = "test-support")]
static ANALYTICAL_CLEANUP_PAUSE: std::sync::OnceLock<std::sync::Arc<AnalyticalCleanupPause>> =
    std::sync::OnceLock::new();

/// Returns the process-wide Analytical cleanup pause.
#[cfg(feature = "test-support")]
#[must_use]
pub fn analytical_cleanup_pause_for_test() -> std::sync::Arc<AnalyticalCleanupPause> {
    std::sync::Arc::clone(
        ANALYTICAL_CLEANUP_PAUSE
            .get_or_init(|| std::sync::Arc::new(AnalyticalCleanupPause::default())),
    )
}

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

    /// One full-grant session shape for fixtures that do not vary the grant.
    fn fixture_shape() -> crate::resources::OracleSessionShape {
        crate::resources::OracleSessionShape::for_grant(
            crate::resources::ORACLE_PARTITION_MEMORY_BYTES,
            4,
            4,
        )
    }

    use crate::resources::{OracleResourceRequest, OracleResources};

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
    use wyrd_spec::DataTenantId;

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
    /// Authority that signs and authorizes nothing, for egress-free fixtures.
    #[derive(Debug)]
    struct RefusingStageAuthority;

    #[async_trait]
    impl OracleStageAuthority for RefusingStageAuthority {
        /// Refuses to mint, because no fixture here sends a stage operation.
        ///
        /// # Errors
        /// Always returns [`PeerSecurityError::Operation`].
        fn mint_stage(
            &self,
            _operation: StageOperationV1,
            _claims: &super::super::peer::StageTicketClaims,
        ) -> Result<wyrd_spec::vala::api::SignedPeerTicket, PeerSecurityError> {
            Err(PeerSecurityError::Operation)
        }

        /// Refuses to authorize, because no fixture here receives one either.
        ///
        /// # Errors
        /// Always returns [`PeerSecurityError::Operation`].
        async fn authorize_stage(
            &self,
            _ticket: &wyrd_spec::vala::api::SignedPeerTicket,
            _binding: &super::super::peer::StageBinding,
            _body: &[u8],
            _now: DateTime<Utc>,
        ) -> Result<AuthorizedStage, PeerSecurityError> {
            Err(PeerSecurityError::Operation)
        }
    }

    /// Builds the egress owner a session fixture needs but never exercises.
    ///
    /// The peer directory is empty, so a fixture that unexpectedly opens an
    /// outbound stage channel fails to resolve a minter rather than emitting an
    /// unsigned request.
    fn fixture_egress() -> Arc<AnalyticalStageEgress> {
        Arc::new(AnalyticalStageEgress::new(
            Arc::new(RefusingStageAuthority),
            NodeId::new(Uuid::from_u128(0)),
            0,
            chrono::Duration::seconds(30),
            BifrostPeerTls::unreachable_for_test(),
            Arc::new(super::super::dispatcher::StaticOraclePeerCredentials::new(
                secrecy::SecretString::from("fixture-bearer"),
            )),
        ))
    }

    /// Builds the leaf binding a session fixture needs but never exercises.
    fn fixture_leaf_binding() -> super::super::codec::AnalyticalLeafBinding {
        super::super::codec::AnalyticalLeafBinding::new(
            wyrd_spec::vala::api::ClusterRole::Oracle,
            Arc::new(super::super::follower::UnresolvableSource),
            Arc::new(crate::oracle::AcceptingOracleAudit),
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
        let graph = AnalyticalGraphRuntime::new(Arc::clone(&query_runtime), fixture_shape());
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
            fixture_egress(),
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
        let graph = AnalyticalGraphRuntime::new(Arc::clone(&query_runtime), fixture_shape());
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

    /// A graph has exactly one attempt, and no successor is representable.
    ///
    /// # Panics
    ///
    /// Panics when a successor attempt becomes representable.
    #[test]
    fn analytical_attempt_number_permits_exactly_one_attempt() {
        assert_eq!(AnalyticalAttemptNumber::ZERO.as_u8(), 0);
        assert_eq!(
            AnalyticalAttemptNumber::from_u8(0),
            Some(AnalyticalAttemptNumber::ZERO)
        );
        assert_eq!(AnalyticalAttemptNumber::from_u8(1), None);
        assert_eq!(AnalyticalAttemptNumber::from_u8(2), None);
    }

    /// Follower source resolver that counts every provider resolution attempt.
    ///
    /// Refusal is the point: a stage message that is refused before consumption
    /// must never reach a provider at all, so a nonzero count is the failure.
    #[derive(Debug, Default)]
    struct CountingSource {
        /// Provider resolutions attempted under this fixture.
        resolutions: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl super::super::follower::FollowerSourceResolver for CountingSource {
        /// Counts the attempt and refuses without touching storage.
        ///
        /// # Errors
        /// Always returns the fixture refusal.
        async fn resolve(
            &self,
            _target_role: wyrd_spec::vala::api::ClusterRole,
            _assignment: &wyrd_spec::vala::api::FollowerScanAssignment,
            _session: &SessionState,
        ) -> Result<super::super::follower::ResolvedFollowerSource, String> {
            self.resolutions.fetch_add(1, Ordering::SeqCst);
            Err("counting fixture resolver refuses every assignment".to_owned())
        }
    }

    /// A stage authority that enforces exactly the production binding rules.
    ///
    /// Only signature custody is fixture-owned: the presented claims are decoded
    /// and run through [`StageTicketClaims::verify_binding`], so a mutated
    /// identity is refused here for the same reason the server authority would
    /// refuse it. Fields the binding does not cover — the participant cut and the
    /// absolute deadline — pass through verbatim, which is exactly the authority
    /// the graph lease must retain for itself.
    #[derive(Debug)]
    struct VerifyingStageAuthority;

    #[async_trait]
    impl OracleStageAuthority for VerifyingStageAuthority {
        /// Encodes the claims verbatim under a fixture key and signature.
        ///
        /// # Errors
        /// Never fails; the signature is fixture-owned.
        fn mint_stage(
            &self,
            _operation: StageOperationV1,
            claims: &super::super::peer::StageTicketClaims,
        ) -> Result<wyrd_spec::vala::api::SignedPeerTicket, PeerSecurityError> {
            Ok(wyrd_spec::vala::api::SignedPeerTicket {
                key_id: "fixture".to_owned(),
                claims_bytes: prost::Message::encode_to_vec(claims),
                signature: vec![0; 64],
            })
        }

        /// Verifies the presented claims bind the exact received bytes.
        ///
        /// # Errors
        /// Returns the production refusal for any bound-field mismatch.
        async fn authorize_stage(
            &self,
            ticket: &wyrd_spec::vala::api::SignedPeerTicket,
            binding: &super::super::peer::StageBinding,
            body: &[u8],
            _now: DateTime<Utc>,
        ) -> Result<AuthorizedStage, PeerSecurityError> {
            let claims = <super::super::peer::StageTicketClaims as prost::Message>::decode(
                ticket.claims_bytes.as_slice(),
            )
            .map_err(|_| PeerSecurityError::Claims)?;
            let digest = super::super::peer::stage_body_digest(body)?;
            claims.verify_binding(binding, &digest)?;
            Ok(AuthorizedStage {
                claims,
                tenant_id: binding.tenant_id,
            })
        }
    }

    /// Builds one live Oracle role owner from an injected resource observation.
    ///
    /// This is the production composition stage, not a stub: the returned owner
    /// issues real admitted query envelopes whose nested children the graph
    /// lease must return.
    ///
    /// # Panics
    ///
    /// Panics when the injected observation cannot compose an Oracle role.
    fn fixture_oracle_role() -> OracleResources {
        let snapshot = crate::resources::SystemResourceSnapshot {
            memory_limit_bytes: 4 * 1024 * 1024 * 1024,
            effective_cpu: 8,
            scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
            scratch_available_bytes: 2 * 1024 * 1024 * 1024,
            memory_source: crate::resources::ResourceSource::Injected,
            cpu_source: crate::resources::ResourceSource::Injected,
        };
        let policy = crate::resources::BifrostResourcePolicy {
            roles: [crate::resources::BifrostRole::Oracle]
                .into_iter()
                .collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: std::path::PathBuf::new(),
            volume_roots: None,
        };
        crate::resources::BifrostRuntimeResources::from_snapshot(snapshot, policy)
            .expect("an injected Oracle observation composes the production root")
            .compose_roles()
            .expect("role composition is issued from an unpoisoned root")
            .oracle()
            .expect("the Oracle role is active in this policy")
    }

    /// One stage message's complete signed identity, mutable field by field.
    ///
    /// Every field a graph lease must retain is settable here, so a test can
    /// substitute exactly one and assert the refusal is attributable to it.
    #[derive(Debug, Clone)]
    struct StageMessage {
        /// Governed operation this message carries.
        operation: StageOperationV1,
        /// Presenting coordinator's node identity.
        source_node_id: NodeId,
        /// Presenting coordinator's role fence.
        source_fence: u64,
        /// Authenticated data tenant of the graph.
        tenant_id: DataTenantId,
        /// Two-identity graph the operation belongs to.
        graph: AnalyticalGraphKey,
        /// Pinned snapshot digest of the graph's cut.
        snapshot_digest: String,
        /// Graph-local stage ordinal.
        stage_id: u32,
        /// Stage-local task ordinal.
        task_id: Option<u32>,
        /// Attempt ordinal within the graph.
        attempt: u32,
        /// Reservation the graph executes under.
        reservation_id: String,
        /// Permission digest resolved for the query.
        permission_digest: String,
        /// Absolute wall-clock deadline of the whole graph.
        absolute_deadline_ms: i64,
        /// Immutable destination participant cut carried by the ticket.
        participants: Vec<super::super::peer::StageParticipantV1>,
        /// Single-use nonce, varied so two messages are never replays.
        nonce: Vec<u8>,
    }

    /// Everything one graph-lease owner test needs to send authorized stages.
    struct GraphFixture {
        /// The follower ingress under test.
        ingress: Arc<AnalyticalStageIngress>,
        /// Reservation owner the ingress activates graph leases from.
        reservations: Arc<ReservationRegistry>,
        /// Node supervisor owning registered graphs and attempts.
        supervisor: Arc<AnalyticalSupervisor>,
        /// Provider resolutions attempted by any refused message.
        resolutions: Arc<AtomicUsize>,
        /// This follower's own node identity.
        node_id: NodeId,
        /// This follower's own role fence.
        fence: u64,
        /// The graph every fixture message names.
        graph: AnalyticalGraphKey,
        /// Tenant every fixture message is authenticated for.
        tenant_id: DataTenantId,
        /// The reserving leader's node identity, absent from the cut.
        leader_node_id: NodeId,
        /// The reserving leader's role fence.
        leader_fence: u64,
        /// Reservation the leader took on this follower for the graph.
        reservation_id: ReservationId,
        /// Immutable destination cut, which never names the leader.
        participants: Vec<super::super::peer::StageParticipantV1>,
        /// Absolute deadline every fixture message carries.
        deadline_ms: i64,
        /// Process spill owner the ingress builds each graph runtime from.
        spill: Arc<OracleSpillRuntime>,
        /// Scratch root kept alive for the spill owner.
        _root: tempfile::TempDir,
    }

    impl GraphFixture {
        /// Composes a follower ingress holding one live graph reservation.
        ///
        /// # Panics
        ///
        /// Panics when the Oracle role, spill owner, or reservation cannot be
        /// composed, which would make every assertion below vacuous.
        fn new(now: DateTime<Utc>) -> Self {
            Self::compose(
                now,
                2 * 1024 * 1024 * 1024,
                now + chrono::Duration::seconds(60),
            )
        }

        /// Composes the same fixture whose graph carries an exact deadline.
        ///
        /// The signed absolute deadline is the only bound an `ExecuteTask`-first
        /// graph has, so a test that proves that bound has to choose it.
        ///
        /// # Panics
        ///
        /// Panics when the Oracle role, spill owner, or reservation cannot be
        /// composed, which would make every assertion below vacuous.
        fn with_deadline(now: DateTime<Utc>, deadline: DateTime<Utc>) -> Self {
            Self::compose(now, 2 * 1024 * 1024 * 1024, deadline)
        }

        /// Composes the same fixture over an exact process spill limit.
        ///
        /// A limit below the reserved envelope's scratch share is how a test
        /// makes the graph runtime build — the first fallible activation step —
        /// fail for a real reason rather than through an injected seam.
        ///
        /// # Panics
        ///
        /// Panics when the Oracle role, spill owner, or reservation cannot be
        /// composed, which would make every assertion below vacuous.
        fn with_pod_spill_limit(now: DateTime<Utc>, pod_spill_limit_bytes: u64) -> Self {
            Self::compose(
                now,
                pod_spill_limit_bytes,
                now + chrono::Duration::seconds(60),
            )
        }

        /// Composes the fixture over an exact spill limit and graph deadline.
        ///
        /// # Panics
        ///
        /// Panics when the Oracle role, spill owner, or reservation cannot be
        /// composed, which would make every assertion below vacuous.
        fn compose(
            now: DateTime<Utc>,
            pod_spill_limit_bytes: u64,
            deadline: DateTime<Utc>,
        ) -> Self {
            let root = tempfile::tempdir().expect("fixture scratch root must exist");
            let spill = Arc::new(
                OracleSpillRuntime::new(root.path(), pod_spill_limit_bytes)
                    .expect("bounded spill owner must be created"),
            );
            let resolutions = Arc::new(AtomicUsize::new(0));
            let node_id = NodeId::new(Uuid::from_u128(2));
            let fence = 7;
            let supervisor = Arc::new(AnalyticalSupervisor::new());
            let reservations = Arc::new(ReservationRegistry::new(
                Arc::new(crate::oracle::OracleSlotManager::new(4, 4)),
                16,
            ));
            let ingress = AnalyticalStageIngress::new(AnalyticalStageIngressConfig {
                node_id,
                oracle_fence: fence,
                authority: Arc::new(VerifyingStageAuthority),
                supervisor: Arc::clone(&supervisor),
                reservations: Arc::clone(&reservations),
                spill: Arc::clone(&spill),
                leaf: super::super::codec::AnalyticalLeafBinding::new(
                    wyrd_spec::vala::api::ClusterRole::Oracle,
                    Arc::new(CountingSource {
                        resolutions: Arc::clone(&resolutions),
                    }),
                    Arc::new(crate::oracle::AcceptingOracleAudit),
                ),
                egress: fixture_egress(),
            });
            let graph = AnalyticalGraphKey::new(
                PublicQueryId::from_uuid(Uuid::from_u128(11)),
                DataFusionQueryId::from_uuid(Uuid::from_u128(12)),
            );
            let leader_node_id = NodeId::new(Uuid::from_u128(1));
            let leader_fence = 3;
            let resources = fixture_oracle_role()
                .try_acquire_query(OracleResourceRequest::for_class(
                    QueryClass::Analytical,
                    0.0,
                ))
                .expect("an idle Oracle admits one analytical query");
            let reservation = reservations
                .reserve(
                    &ReserveNodeSlotsRequest {
                        query_id: QueryId::new(graph.public_query_id.as_uuid()),
                        leader_node_id,
                        leader_fencing_token: leader_fence,
                        query_class: QueryClass::Analytical,
                        slot_units: ANALYTICAL_GRAPH_SLOT_UNITS,
                        expires_at: deadline,
                        graph: Some(AnalyticalGraphRef {
                            public_query_id: graph.public_query_id.as_uuid(),
                            datafusion_query_id: graph.datafusion_query_id.as_uuid(),
                        }),
                    },
                    now,
                    Some(super::super::dispatcher::ReservedCapacity::Graph(Box::new(
                        resources,
                    ))),
                )
                .expect("an idle follower accepts one graph reservation");
            // The cut names the middle-stage participants only. The reserving
            // leader is deliberately absent from it, which is what makes the two
            // source-authorization branches independently observable.
            let participants = vec![
                super::super::peer::StageParticipantV1 {
                    node_id: Uuid::from_u128(2).as_bytes().to_vec(),
                    fence: 7,
                    address: "https://follower-a.invalid/".to_owned(),
                    reservation_id: reservation.reservation_id.as_uuid().to_string(),
                },
                super::super::peer::StageParticipantV1 {
                    node_id: Uuid::from_u128(4).as_bytes().to_vec(),
                    fence: 9,
                    address: "https://follower-b.invalid/".to_owned(),
                    reservation_id: Uuid::from_u128(44).to_string(),
                },
            ];
            Self {
                ingress,
                reservations,
                supervisor,
                resolutions,
                node_id,
                fence,
                graph,
                tenant_id: DataTenantId::new_v7(),
                leader_node_id,
                leader_fence,
                reservation_id: reservation.reservation_id,
                participants,
                deadline_ms: deadline.timestamp_millis(),
                spill,
                _root: root,
            }
        }

        /// Reserves one more graph on the same follower and names its message.
        ///
        /// The second graph shares this fixture's ingress, driver, and settlement
        /// queue, which is the only way a test can observe how one graph's
        /// settlement affects another's.
        ///
        /// # Panics
        ///
        /// Panics when the follower cannot admit a second graph reservation.
        fn sibling(
            &self,
            now: DateTime<Utc>,
            deadline: DateTime<Utc>,
        ) -> (AnalyticalGraphKey, StageMessage) {
            let graph = AnalyticalGraphKey::new(
                PublicQueryId::from_uuid(Uuid::from_u128(21)),
                DataFusionQueryId::from_uuid(Uuid::from_u128(22)),
            );
            let resources = fixture_oracle_role()
                .try_acquire_query(OracleResourceRequest::for_class(
                    QueryClass::Analytical,
                    0.0,
                ))
                .expect("an idle Oracle admits one analytical query");
            let reservation = self
                .reservations
                .reserve(
                    &ReserveNodeSlotsRequest {
                        query_id: QueryId::new(graph.public_query_id.as_uuid()),
                        leader_node_id: self.leader_node_id,
                        leader_fencing_token: self.leader_fence,
                        query_class: QueryClass::Analytical,
                        slot_units: ANALYTICAL_GRAPH_SLOT_UNITS,
                        expires_at: deadline,
                        graph: Some(AnalyticalGraphRef {
                            public_query_id: graph.public_query_id.as_uuid(),
                            datafusion_query_id: graph.datafusion_query_id.as_uuid(),
                        }),
                    },
                    now,
                    Some(super::super::dispatcher::ReservedCapacity::Graph(Box::new(
                        resources,
                    ))),
                )
                .expect("an idle follower accepts a second graph reservation");
            let mut message = self.leader_message(StageOperationV1::ExecuteTask, 9);
            message.graph = graph;
            message.reservation_id = reservation.reservation_id.as_uuid().to_string();
            message.absolute_deadline_ms = deadline.timestamp_millis();
            (graph, message)
        }

        /// Composes this node's production Analytical handle over the fixture.
        ///
        /// The same owners the follower ingress already uses, so the handle's
        /// health is a projection of this fixture's real graph state rather
        /// than of a second, parallel inventory.
        ///
        /// # Panics
        ///
        /// Panics when the fixture's Oracle role cannot be composed.
        fn execution_handle(&self) -> AnalyticalExecutionHandle {
            self.execution_handle_with(None)
        }

        /// Composes the same handle over an exact peer transport directory.
        ///
        /// A leader only reserves, releases, or loses a peer through this
        /// directory, so a test that injects peer loss chooses it here.
        ///
        /// # Panics
        ///
        /// Panics when the fixture's Oracle role cannot be composed.
        fn execution_handle_with(
            &self,
            peer_transports: Option<Arc<super::super::dispatcher::OraclePeerTransportDirectory>>,
        ) -> AnalyticalExecutionHandle {
            AnalyticalExecutionHandle::new(
                AnalyticalExecutionOwners {
                    worker: Arc::clone(&self.ingress),
                    authority: Arc::new(VerifyingStageAuthority),
                    supervisor: Arc::clone(&self.supervisor),
                    spill: Arc::clone(&self.spill),
                    peer_transports,
                },
                AnalyticalExecutionConfig {
                    node_id: self.node_id,
                    oracle_fence: self.fence,
                    ticket_ttl: chrono::Duration::seconds(30),
                    scratch_bytes: 0,
                    peer_tls: BifrostPeerTls::unreachable_for_test(),
                    peer_credentials: Arc::new(
                        super::super::dispatcher::StaticOraclePeerCredentials::new(
                            secrecy::SecretString::from("fixture-bearer"),
                        ),
                    ),
                },
                fixture_leaf_binding(),
            )
        }

        /// Builds the message the reserving leader presents to activate the graph.
        fn leader_message(&self, operation: StageOperationV1, nonce: u8) -> StageMessage {
            StageMessage {
                operation,
                source_node_id: self.leader_node_id,
                source_fence: self.leader_fence,
                tenant_id: self.tenant_id,
                graph: self.graph,
                snapshot_digest: "fixture-snapshot".to_owned(),
                stage_id: 0,
                task_id: match operation {
                    StageOperationV1::SetPlan => None,
                    StageOperationV1::ExecuteTask => Some(0),
                },
                attempt: 0,
                reservation_id: self.reservation_id.as_uuid().to_string(),
                permission_digest: "fixture-permissions".to_owned(),
                absolute_deadline_ms: self.deadline_ms,
                participants: self.participants.clone(),
                nonce: vec![nonce],
            }
        }

        /// Sends one signed stage message through the production ingress path.
        ///
        /// # Errors
        ///
        /// Returns whatever the ingress refuses the message with.
        async fn send(
            &self,
            message: &StageMessage,
            now: DateTime<Utc>,
        ) -> Result<AnalyticalAttemptKey, BifrostError> {
            let identity = StageWireIdentity {
                source_node_id: message.source_node_id,
                source_fence: message.source_fence,
                tenant_id: message.tenant_id,
                graph: message.graph,
                snapshot_digest: message.snapshot_digest.clone(),
                stage_id: message.stage_id,
                task_id: message.task_id,
                attempt: message.attempt,
                reservation_id: message.reservation_id.clone(),
                permission_digest: message.permission_digest.clone(),
            };
            let mut headers = HeaderMap::new();
            identity
                .write(&mut headers)
                .expect("fixture identity must encode");
            let body = b"fixture-stage-body";
            let binding = identity.to_binding(message.operation, self.node_id, self.fence);
            let claims = super::super::peer::StageTicketClaims::for_binding(
                &binding,
                super::super::peer::stage_body_digest(body).expect("fixture body must digest"),
                message.nonce.clone(),
                message.absolute_deadline_ms,
                0,
                message.participants.clone(),
            );
            let ticket = VerifyingStageAuthority
                .mint_stage(message.operation, &claims)
                .expect("fixture ticket must mint");
            super::super::analytical_transport::write_ticket(&mut headers, &ticket)
                .expect("fixture ticket must encode");
            self.ingress
                .authorize_stage_message(message.operation, &headers, body, now)
                .await
        }
    }

    /// Builds one attempt identity with the fixture's shared digests.
    ///
    /// The two identities are what a graph is keyed by, so a lease test varies
    /// them per case while the digests stay fixed.
    fn fixture_attempt(public: u128, datafusion: u128) -> AnalyticalAttemptContext {
        AnalyticalAttemptContext {
            public_query_id: PublicQueryId::from_uuid(Uuid::from_u128(public)),
            datafusion_query_id: DataFusionQueryId::from_uuid(Uuid::from_u128(datafusion)),
            snapshot_digest: "fixture-snapshot".to_owned(),
            permission_digest: "fixture-permissions".to_owned(),
        }
    }

    /// Builds the minimum-grant session configuration a leader plans with.
    ///
    /// Production retains the config its physical root was built with, so a
    /// lease test hands `lease_session` the same shape rather than letting the
    /// handle invent one.
    fn min_grant_lease_config() -> datafusion::prelude::SessionConfig {
        crate::resources::OracleSessionShape::for_grant(
            crate::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            crate::resources::ORACLE_MIN_TARGET_PARTITIONS,
            4,
        )
        .session_config()
    }

    /// Builds one authenticated Analytical context for a leasing fixture.
    fn context_for_leasing() -> AuthorizedQueryContext {
        let tenant = DataTenantId::new_v7();
        AuthorizedQueryContext::try_new(
            wyrd_runtime::Principal {
                id: wyrd_runtime::PrincipalId::new(Uuid::now_v7()),
                kind: wyrd_runtime::PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: wyrd_runtime::permission::PermissionSet::default(),
            },
            tenant,
            wyrd_spec::request_id::RequestId::now_v7(),
            None,
            wyrd_spec::vala::api::AuthMethod::Internal,
            "bifrost_query:read",
        )
        .expect("the fixture principal authorizes one analytical read")
    }

    /// Proves the leased envelope moved exactly once and is not shared.
    ///
    /// Two things make the transfer safe rather than merely convenient: the
    /// guard it came from can no longer hand it out, and the next query still
    /// gets a pool of its own.
    ///
    /// # Panics
    ///
    /// Panics when the guard still holds an envelope or a second query is
    /// issued the same pool.
    fn assert_envelope_moved_once(
        oracle: &OracleResources,
        pool: &Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
        admitted: &mut super::super::admission::AdmittedQueryGuard,
    ) {
        assert!(
            admitted.take_query_resources().is_none(),
            "the admitted guard has no second envelope to hand out"
        );
        let other = oracle
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .expect("an idle Oracle admits a second analytical query");
        assert!(
            !Arc::ptr_eq(pool, &other.memory_pool()),
            "distinct queries never share one DataFusion pool"
        );
    }

    /// Leases one leader attempt over a transport accepting `accepted` reserves.
    ///
    /// Returns the admitted guard, the attempt ownership, and the transport, so
    /// a caller can inject peer loss and then observe exactly what was issued.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot admit or lease, which would make every
    /// assertion built on the returned ownership vacuous.
    fn lease_over_lossy_peers(
        fixture: &GraphFixture,
        oracle: &OracleResources,
        accepted: usize,
    ) -> Box<(
        super::super::admission::AdmittedQueryGuard,
        AnalyticalAttemptOwnership,
        Arc<ReservingTransport>,
    )> {
        lease_over_lossy_peers_until(
            fixture,
            oracle,
            accepted,
            tokio::time::Instant::now() + Duration::from_mins(1),
        )
    }

    /// Leases one leader attempt whose graph is bounded by `deadline`.
    ///
    /// Separated from the ordinary helper so a test can prove that an
    /// unanswerable peer is interrupted by the envelope's own bound rather than
    /// by a timer this fixture invented.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot admit or lease, which would make every
    /// assertion built on the returned ownership vacuous.
    fn lease_over_lossy_peers_until(
        fixture: &GraphFixture,
        oracle: &OracleResources,
        accepted: usize,
        deadline: tokio::time::Instant,
    ) -> Box<(
        super::super::admission::AdmittedQueryGuard,
        AnalyticalAttemptOwnership,
        Arc<ReservingTransport>,
    )> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(60);
        let transport = Arc::new(ReservingTransport::new(accepted, expires_at));
        let directory = Arc::new(
            super::super::dispatcher::OraclePeerTransportDirectory::new_for_test(
                fixture.node_id,
                Arc::clone(&transport) as Arc<dyn super::super::dispatcher::OraclePeerTransport>,
                Arc::clone(&transport) as Arc<dyn super::super::dispatcher::OraclePeerTransport>,
            ),
        );
        let handle = fixture.execution_handle_with(Some(directory));
        let resources = oracle
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .expect("an idle Oracle admits one analytical query");
        let (mut admitted, _shared, _cancel) = super::super::admission::admitted_guard_for_test();
        admitted.install_query_resources_for_test(resources);
        let attempt = fixture_attempt(201, 202);
        let cut = super::super::participant_cut::tests::analytical_cut(
            now,
            fixture.node_id.as_uuid().as_u128(),
            expires_at,
        );
        let (session, ownership) = handle
            .lease_session(super::AnalyticalLeaseInputs {
                attempt: &attempt,
                cut: &cut,
                context: &context_for_leasing(),
                admitted: &mut admitted,
                work_units: 4,
                config: min_grant_lease_config(),
                deadline,
            })
            .expect("the leader leases one session from its admitted envelope");
        drop(session);
        Box::new((admitted, ownership, transport))
    }

    /// Peer loss after selection is terminal, and no successor attempt exists.
    ///
    /// Two orders matter and are proven separately: peer loss before any result
    /// frame has left, and peer loss after one has. Neither may produce a second
    /// attempt, a second round of participant reservations, or a terminal that
    /// leaves the query's ownership behind. The retry ordinal, the retry
    /// entry point, and the `retried` telemetry outcome are all gone, so "one
    /// attempt" is a property of the types rather than of a caller's restraint.
    ///
    /// # Panics
    ///
    /// Panics when a successor is reachable or a terminal strands ownership.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn peer_loss_is_one_terminal_attempt() {
        assert_eq!(
            AnalyticalAttemptNumber::from_u8(1),
            None,
            "no successor attempt ordinal is representable"
        );
        assert!(
            !AnalyticalAttemptOutcome::ALL
                .iter()
                .any(|outcome| outcome.as_str() == "retried"),
            "no retry terminal outcome remains to be counted"
        );

        // Multi-threaded on purpose: the graph lifecycle task is polled off
        // this test's own stack, and one boxed future slot is reused by both
        // cases so two live fixtures never share one debug-build frame.
        for accepted in [0_usize, 2] {
            Box::pin(assert_peer_loss_is_terminal(accepted)).await;
        }
    }

    /// Proves one peer-loss ordering ends in exactly one failed terminal.
    ///
    /// `accepted` chooses the ordering: zero refuses the first reservation, so
    /// the loss lands before any result frame; two accepts the whole cut, so
    /// the attempt is egressed before it is lost. Neither may produce a second
    /// attempt or a second round of reservations.
    ///
    /// # Panics
    ///
    /// Panics when the loss is not terminal, a successor round is issued, or
    /// the terminal strands ownership.
    async fn assert_peer_loss_is_terminal(accepted: usize) {
        let fixture = GraphFixture::new(Utc::now());
        let oracle = fixture_oracle_role();
        let leased = lease_over_lossy_peers(&fixture, &oracle, accepted);
        let (admitted, ownership, transport) = *leased;
        let published = Box::pin(ownership.publish_participants()).await;
        if accepted == 0 {
            assert!(
                matches!(published, Err(BifrostError::QueryAdmissionRejected)),
                "peer loss before dispatch refuses the attempt outright"
            );
            assert!(
                !ownership.egressed(),
                "no result frame left before the peer was lost"
            );
        } else {
            published.expect("every addressed follower accepted its reservation");
            ownership.record_egress();
            assert!(ownership.egressed(), "one result frame has left this query");
        }
        Box::pin(assert_terminal_returns_everything(
            &fixture, &oracle, admitted, ownership,
        ))
        .await;
        assert_eq!(
            transport.reserves().len(),
            if accepted == 0 { 1 } else { 2 },
            "a lost peer starts no successor round of reservations"
        );
    }

    /// Settles one failed attempt and proves nothing survives it.
    ///
    /// # Panics
    ///
    /// Panics when the terminal is refused, a successor becomes spawnable, or
    /// the graph or query envelope is left charged.
    async fn assert_terminal_returns_everything(
        fixture: &GraphFixture,
        oracle: &OracleResources,
        admitted: super::super::admission::AdmittedQueryGuard,
        ownership: AnalyticalAttemptOwnership,
    ) {
        let key = ownership.key();
        ownership
            .settle(AnalyticalAttemptOutcome::Failed)
            .await
            .expect("a lost peer settles this attempt exactly once");
        drop(admitted);
        assert!(
            fixture
                .supervisor
                .spawn_attempt(key, AnalyticalAttemptGrant { scratch_bytes: 0 },)
                .is_err(),
            "the settled graph admits no further attempt"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("the supervisor reports live graphs"),
            0,
            "one terminal releases the graph"
        );
        assert_eq!(
            oracle
                .snapshot()
                .expect("the fixture root reports live ownership")
                .oracle_analytical_queries,
            0,
            "one terminal returns the query envelope to the process root"
        );
    }

    /// Awaits one cloned settlement receiver and returns what it observed.
    ///
    /// # Panics
    ///
    /// Panics when the lifecycle task ends without publishing a settlement,
    /// which would make every settlement assertion below vacuous.
    async fn observe_settlement(
        mut receiver: tokio::sync::watch::Receiver<AnalyticalGraphResult>,
    ) -> AnalyticalGraphResult {
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

    /// Names one stage of the graph the settlement sequence never joins.
    fn stray_attempt_key(graph: AnalyticalGraphKey) -> AnalyticalAttemptKey {
        AnalyticalAttemptKey::new(
            graph.public_query_id,
            graph.datafusion_query_id,
            StageId::new(1),
            Some(TaskId::new(0)),
            AnalyticalAttemptNumber::ZERO,
        )
    }

    /// One supervisor-owned lifecycle task settles every leader graph.
    ///
    /// Four properties are proven together because they are one design: the
    /// task is owned by the supervisor rather than by a caller, so a raw stream
    /// drop signals cancellation and leaves a task shutdown still joins; the
    /// first terminal signal alone chooses the outcome and every cloned result
    /// receiver observes that same settlement; successful cleanup releases the
    /// attempt, every participant reservation, and the graph exactly once
    /// before the graph entry is removed and success is published; and a
    /// cleanup that cannot be confirmed retains every owner in the `Draining`
    /// entry, publishes a failure instead of a success, and removes readiness.
    ///
    /// # Panics
    ///
    /// Panics when a caller can take the task, when a later terminal replaces
    /// the first, when a settled graph strands an owner, or when a failed
    /// cleanup reports success or leaves the node advertising readiness.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn leader_lifecycle_task_joins_every_owner_and_retains_failure() {
        Box::pin(assert_success_releases_every_owner_once()).await;
        Box::pin(assert_first_terminal_alone_chooses_the_settlement()).await;
        Box::pin(assert_stream_drop_leaves_a_supervisor_owned_task()).await;
        Box::pin(assert_cleanup_failure_retains_draining_ownership()).await;
        Box::pin(assert_release_stops_at_the_graph_deadline()).await;
        Box::pin(assert_an_idle_graph_past_the_deadline_is_not_released()).await;
    }

    /// Successful cleanup releases every owner once, then removes the graph.
    ///
    /// # Panics
    ///
    /// Panics when settlement strands an owner, releases a reservation twice,
    /// or removes the graph before its owners were returned.
    async fn assert_success_releases_every_owner_once() {
        let fixture = GraphFixture::new(Utc::now());
        let oracle = fixture_oracle_role();
        let leased = lease_over_lossy_peers(&fixture, &oracle, 2);
        let (admitted, ownership, transport) = *leased;
        ownership
            .publish_participants()
            .await
            .expect("every addressed follower accepted its reservation");
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("the supervisor reports live graphs"),
            1,
            "the graph is live while its attempt still owns it"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_attempts()
                .expect("the supervisor reports live attempts"),
            1,
            "the leader attempt is owned by the lifecycle task, not by the caller"
        );
        let release = ownership
            .settle(AnalyticalAttemptOutcome::Success)
            .await
            .expect("successful cleanup publishes a success settlement");
        assert_eq!(
            release.outcome,
            AnalyticalAttemptOutcome::Success,
            "the settlement carries the outcome the first terminal chose"
        );
        assert_eq!(
            transport.releases().len(),
            2,
            "each accepted reservation is returned exactly once"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_attempts()
                .expect("the supervisor reports live attempts"),
            0,
            "the attempt is joined before the graph is removed"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("the supervisor reports live graphs"),
            0,
            "successful cleanup removes the graph entry"
        );
        assert_eq!(
            fixture
                .supervisor
                .draining_graphs()
                .expect("the supervisor reports retained cleanup"),
            0,
            "a settled graph retains no cleanup"
        );
        drop(admitted);
        assert_eq!(
            oracle
                .snapshot()
                .expect("the fixture root reports live ownership")
                .oracle_analytical_queries,
            0,
            "the query envelope returns to the process root exactly once"
        );
    }

    /// The first terminal signal wins, and every observer sees that settlement.
    ///
    /// # Panics
    ///
    /// Panics when a later terminal replaces the first, or when two cloned
    /// receivers observe different settlements.
    async fn assert_first_terminal_alone_chooses_the_settlement() {
        let fixture = GraphFixture::new(Utc::now());
        let oracle = fixture_oracle_role();
        let leased = lease_over_lossy_peers(&fixture, &oracle, 2);
        let (admitted, ownership, _transport) = *leased;
        ownership
            .publish_participants()
            .await
            .expect("every addressed follower accepted its reservation");
        let observers = [
            ownership.signals.settled_receiver(),
            ownership.signals.settled_receiver(),
            ownership.signals.settled_receiver(),
        ];
        // Cancellation first, then a peer failure, then the success the caller
        // is about to ask for: only the first may choose the outcome.
        ownership
            .signals
            .terminal(AnalyticalAttemptOutcome::Cancelled);
        ownership.signals.terminal(AnalyticalAttemptOutcome::Failed);
        let release = ownership
            .settle(AnalyticalAttemptOutcome::Success)
            .await
            .expect("a raced terminal still settles this graph exactly once");
        assert_eq!(
            release.outcome,
            AnalyticalAttemptOutcome::Cancelled,
            "the first terminal signal alone chooses the outcome"
        );
        let settlement = AnalyticalGraphResult::SettledSuccess(Some(release));
        for observer in observers {
            assert_eq!(
                observe_settlement(observer).await,
                settlement,
                "every cloned receiver observes the same settlement"
            );
        }
        drop(admitted);
    }

    /// A dropped caller signals cancellation and leaves the task joinable.
    ///
    /// # Panics
    ///
    /// Panics when a raw drop strands the graph, or when shutdown cannot find
    /// and join the task the dropped caller left behind.
    async fn assert_stream_drop_leaves_a_supervisor_owned_task() {
        let fixture = GraphFixture::new(Utc::now());
        let oracle = fixture_oracle_role();
        let leased = lease_over_lossy_peers(&fixture, &oracle, 2);
        let (admitted, ownership, transport) = *leased;
        ownership
            .publish_participants()
            .await
            .expect("every addressed follower accepted its reservation");
        let observer = ownership.signals.settled_receiver();
        // The raw drop path: no settle, no await, nothing taken. The caller
        // holds no task handle to abort and no guard to release.
        drop(ownership);
        let settled = observe_settlement(observer).await;
        assert!(
            matches!(settled, AnalyticalGraphResult::SettledSuccess(Some(release))
                if release.outcome == AnalyticalAttemptOutcome::Cancelled),
            "a dropped caller is a cancellation the supervisor-owned task settles: {settled:?}"
        );
        assert_eq!(
            transport.releases().len(),
            2,
            "the dropped caller's reservations are still returned exactly once"
        );
        let inspection = fixture
            .supervisor
            .shutdown()
            .await
            .expect("shutdown joins every remaining lifecycle task");
        assert_eq!(
            (inspection.attempts_retained, inspection.graphs_retained),
            (0, 0),
            "shutdown joins the dropped caller's task and retains nothing"
        );
        drop(admitted);
        assert_eq!(
            oracle
                .snapshot()
                .expect("the fixture root reports live ownership")
                .oracle_analytical_queries,
            0,
            "a dropped caller still returns the query envelope"
        );
    }

    /// Cleanup that cannot be confirmed retains ownership and fails readiness.
    ///
    /// # Panics
    ///
    /// Panics when a failed cleanup publishes success, releases the graph, or
    /// leaves the node advertising readiness.
    async fn assert_cleanup_failure_retains_draining_ownership() {
        let fixture = GraphFixture::new(Utc::now());
        let oracle = fixture_oracle_role();
        let leased = lease_over_lossy_peers(&fixture, &oracle, 2);
        let (admitted, ownership, _transport) = *leased;
        ownership
            .publish_participants()
            .await
            .expect("every addressed follower accepted its reservation");
        // A stage the settlement sequence cannot reach: it is not this graph's
        // own attempt, so joining that attempt leaves it live and the graph
        // cannot be confirmed released.
        let graph = ownership.key().graph();
        let stray = fixture
            .supervisor
            .spawn_attempt(
                stray_attempt_key(graph),
                AnalyticalAttemptGrant { scratch_bytes: 0 },
            )
            .expect("an active graph admits one more stage");
        let error = ownership
            .settle(AnalyticalAttemptOutcome::Success)
            .await
            .expect_err("a cleanup failure prevents a success terminal");
        assert!(
            matches!(error, BifrostError::Internal { .. }),
            "an unconfirmed cleanup is reported, not logged and ignored: {error:?}"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("the supervisor reports live graphs"),
            1,
            "a failed cleanup retains the graph rather than releasing it"
        );
        assert_eq!(
            fixture
                .supervisor
                .draining_graphs()
                .expect("the supervisor reports retained cleanup"),
            1,
            "the retained graph is visible as draining"
        );
        assert!(
            fixture
                .supervisor
                .graph_settlement_failure(graph)
                .expect("the supervisor reports why cleanup failed")
                .is_some(),
            "the retained graph names why its cleanup could not be confirmed"
        );
        assert!(
            !fixture.execution_handle().is_healthy(),
            "a node retaining an unsettled graph does not advertise readiness"
        );
        drop(stray);
        let inspection = fixture
            .supervisor
            .shutdown()
            .await
            .expect("shutdown reaches the retained residue");
        assert_eq!(
            (inspection.graphs_released, inspection.graphs_retained),
            (0, 1),
            "shutdown reports the retained graph as residue instead of sweeping it"
        );
        drop(admitted);
    }

    /// A live envelope child at the deadline fails settlement then, not later.
    ///
    /// The wait for a graph's nested children is bounded twice: by its own poll
    /// count and by the envelope's absolute deadline. Only the second is a
    /// property of the query, so a child still live when the deadline arrives
    /// has to end the sequence immediately rather than let terminal publication
    /// run on past the bound the caller was promised.
    ///
    /// # Panics
    ///
    /// Panics when settlement outlives the graph deadline, reports success, or
    /// releases a graph whose envelope a child still holds.
    async fn assert_release_stops_at_the_graph_deadline() {
        let fixture = GraphFixture::new(Utc::now());
        let oracle = fixture_oracle_role();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(200);
        let leased = lease_over_lossy_peers_until(&fixture, &oracle, 2, deadline);
        let (admitted, ownership, _transport) = *leased;
        ownership
            .publish_participants()
            .await
            .expect("every addressed follower accepted its reservation");
        let graph = ownership.key().graph();
        // A nested child the settlement sequence cannot reach: the graph's own
        // attempt is joined, but the envelope it charged is still owed scratch.
        let stray = fixture
            .supervisor
            .spawn_attempt(
                stray_attempt_key(graph),
                AnalyticalAttemptGrant {
                    scratch_bytes: 4_096,
                },
            )
            .expect("an active graph admits one more stage");
        let error = ownership
            .settle(AnalyticalAttemptOutcome::Success)
            .await
            .expect_err("a live envelope child prevents a success terminal");
        let overrun = tokio::time::Instant::now().saturating_duration_since(deadline);
        assert!(
            matches!(error, BifrostError::Internal { .. }),
            "an undrained envelope is reported, not logged and ignored: {error:?}"
        );
        assert!(
            overrun < Duration::from_secs(1),
            "settlement ended at the graph deadline rather than at the drain poll count: {overrun:?}"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("the supervisor reports live graphs"),
            1,
            "an undrained graph is retained rather than released"
        );
        assert_eq!(
            fixture
                .supervisor
                .draining_graphs()
                .expect("the supervisor reports retained cleanup"),
            1,
            "the retained graph is visible as draining"
        );
        drop(stray);
        drop(admitted);
    }

    /// An idle graph reached only after the deadline is retained, not released.
    ///
    /// Idleness is not the bound. A cleanup that took until after the envelope's
    /// absolute deadline has already overrun it, so finding the children idle at
    /// that point cannot turn the overrun into a success: the graph is residue
    /// the supervisor must keep observable, and only the post-deadline
    /// conservative-expiry path may remove it.
    ///
    /// # Panics
    ///
    /// Panics when a settlement that began after the deadline publishes success
    /// or releases the graph, its envelope, and its admission.
    async fn assert_an_idle_graph_past_the_deadline_is_not_released() {
        let fixture = GraphFixture::new(Utc::now());
        let oracle = fixture_oracle_role();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(50);
        // Nothing is ever reserved: the only owner this settlement has left to
        // confirm is the envelope itself, and it is already idle.
        let leased = lease_over_lossy_peers_until(&fixture, &oracle, 0, deadline);
        let (admitted, ownership, transport) = *leased;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let error = ownership
            .settle(AnalyticalAttemptOutcome::Success)
            .await
            .expect_err("cleanup that began after the deadline cannot settle as a success");
        assert!(
            matches!(error, BifrostError::Internal { .. }),
            "an overrun cleanup is reported, not logged and ignored: {error:?}"
        );
        assert_eq!(
            transport.releases().len(),
            0,
            "a graph that reserved nothing releases nothing"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("the supervisor reports live graphs"),
            1,
            "an overrun cleanup retains the graph rather than releasing it"
        );
        assert_eq!(
            fixture
                .supervisor
                .draining_graphs()
                .expect("the supervisor reports retained cleanup"),
            1,
            "the retained graph stays observable as draining"
        );
        drop(admitted);
    }

    /// A leader query owns exactly one envelope, and its graph is that pool.
    ///
    /// Everything the leader builds after admission — the registered graph, its
    /// `RuntimeEnv`, and the session `DataFusion` plans and exchanges allocate
    /// from — has to resolve to the single pool the admitted guard already paid
    /// for. A second acquisition anywhere on that path would double-charge the
    /// process governor for one query, so this proves the envelope is
    /// transferred rather than re-acquired, that the transfer is exactly once,
    /// and that a different query still gets a different pool.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot admit, lease, or settle, which would make
    /// every ownership assertion below vacuous.
    #[tokio::test]
    async fn analytical_graph_reuses_one_query_pool_per_process() {
        let now = Utc::now();
        let deadline = now + chrono::Duration::seconds(60);
        let fixture = GraphFixture::new(now);
        let handle = fixture.execution_handle();
        let oracle = fixture_oracle_role();

        let resources = oracle
            .try_acquire_query(OracleResourceRequest::for_class(
                QueryClass::Analytical,
                0.0,
            ))
            .expect("an idle Oracle admits one analytical query");
        let pool = resources.memory_pool();
        let admitted_once = oracle
            .snapshot()
            .expect("the fixture root reports live ownership");
        assert_eq!(
            admitted_once.oracle_analytical_queries, 1,
            "admission charges the process root exactly one analytical envelope"
        );

        let (mut admitted, _shared, _cancel) = super::super::admission::admitted_guard_for_test();
        admitted.install_query_resources_for_test(resources);
        let attempt = fixture_attempt(101, 102);
        let cut = super::super::participant_cut::tests::analytical_cut(
            now,
            fixture.node_id.as_uuid().as_u128(),
            deadline,
        );
        let context = context_for_leasing();
        let (session, ownership) = handle
            .lease_session(super::AnalyticalLeaseInputs {
                attempt: &attempt,
                cut: &cut,
                context: &context,
                admitted: &mut admitted,
                work_units: 4,
                config: min_grant_lease_config(),
                deadline: tokio::time::Instant::now() + Duration::from_mins(1),
            })
            .expect("the leader leases one session from its admitted envelope");

        // Leasing is a transfer, not an acquisition: the root still sees one.
        assert_eq!(
            oracle
                .snapshot()
                .expect("the fixture root reports live ownership")
                .oracle_analytical_queries,
            1,
            "leasing a session must not admit a second leader envelope"
        );
        let graph = AnalyticalGraphKey::new(attempt.public_query_id, attempt.datafusion_query_id);
        let installed = fixture
            .supervisor
            .graph_runtime(graph)
            .expect("the leased graph is registered")
            .runtime()
            .memory_pool
            .clone();
        assert!(
            Arc::ptr_eq(&pool, &installed),
            "the registered graph installs the admitted envelope's own pool"
        );
        assert!(
            Arc::ptr_eq(&pool, &session.runtime_env().memory_pool),
            "the leader session plans and exchanges against that same pool"
        );

        // Operators and exchanges share one counter, so an allocation made
        // through the session is visible on the graph's pool.
        let consumer = datafusion::execution::memory_pool::MemoryConsumer::new("fixture-exchange");
        let reservation = consumer.register(&session.runtime_env().memory_pool);
        reservation
            .try_grow(4096)
            .expect("the admitted grant covers a small exchange allocation");
        assert!(
            installed.reserved() >= 4096,
            "an allocation made through the session is charged to the graph's pool"
        );

        assert_envelope_moved_once(&oracle, &pool, &mut admitted);

        drop(reservation);
        drop(session);
        ownership
            .settle(AnalyticalAttemptOutcome::Cancelled)
            .await
            .expect("a cancelled leader attempt settles its graph");
        drop(admitted);
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("the supervisor reports live graphs"),
            0,
            "settlement releases the graph that owned the envelope"
        );
        assert_eq!(
            oracle
                .snapshot()
                .expect("the fixture root reports live ownership")
                .oracle_analytical_queries,
            0,
            "the transferred envelope returns to the process root exactly once"
        );
    }

    /// A graph is released only after every child it owns has ended.
    ///
    /// Coordinator connections and attempts are separate holders of the same
    /// graph, and a graph may be released only when both are gone and the
    /// envelope's own nested children have drained. When that drain does not
    /// complete, the graph is not quietly forgotten: the follower keeps the
    /// supervisor guard, the reservation residue, and the reason, so the leak
    /// stays attributable to this node instead of poisoning the governor later.
    ///
    /// # Panics
    ///
    /// Panics when a graph is released beneath a live holder, when a settled
    /// graph is retained, or when a failed cleanup is reported as clean.
    #[tokio::test]
    async fn follower_graph_release_waits_for_children_and_retains_cleanup_failure() {
        let now = Utc::now();
        let fixture = GraphFixture::new(now);
        let plan = fixture.leader_message(StageOperationV1::SetPlan, 1);
        let attempt = fixture
            .send(&plan, now)
            .await
            .expect("the reserving leader activates the graph");
        let first = fixture
            .ingress
            .retain_connection(fixture.graph)
            .expect("a live graph retains a coordinator connection");
        let second = fixture
            .ingress
            .retain_connection(fixture.graph)
            .expect("nested calls for one graph share its ownership");

        // The attempt ends, but two coordinator connections still address the
        // graph, so nothing may be released yet.
        fixture
            .ingress
            .finish_attempt(attempt, AnalyticalAttemptOutcome::Success)
            .await
            .expect("the graph's only attempt settles");
        assert!(
            fixture.ingress.published(fixture.graph).is_ok(),
            "an open coordinator connection keeps the graph addressable"
        );
        drop(first);
        assert!(
            fixture.ingress.published(fixture.graph).is_ok(),
            "the graph is retained while its last connection is still open"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("graphs are readable"),
            1,
            "no supervisor guard was released beneath a live connection"
        );

        // The last connection closes. Its drop is synchronous and hands the
        // graph to the ingress's own driver, which shutdown joins.
        drop(second);
        let settled = fixture
            .ingress
            .shutdown()
            .await
            .expect("the follower shuts down without a poisoned lock");
        assert_eq!(
            settled.graphs_retained, 0,
            "a fully drained graph was released, not retained"
        );
        assert!(
            fixture
                .ingress
                .live()
                .expect("ownership is readable")
                .is_clean(),
            "the follower retains nothing after its last connection closed"
        );
        assert_eq!(
            fixture.reservations.cleanup_expired(now),
            0,
            "settlement returned the graph's reservation entry"
        );

        follower_retains_a_graph_whose_children_never_drain().await;
        follower_scopes_one_upstream_worker_to_each_graph().await;
    }

    /// Proves each graph owns its own upstream worker for exactly its lifetime.
    ///
    /// Upstream caches a stage's decoded task data on the worker that served
    /// it, and those cached plans own the exchange connections that charge the
    /// graph's envelope. A worker shared across graphs would therefore hold one
    /// graph's envelope charged for another graph's cache, and settlement could
    /// never join it. This pins the three properties settlement depends on: a
    /// live graph resolves a worker, two graphs never resolve the same one, and
    /// a settled graph resolves none at all.
    ///
    /// # Panics
    ///
    /// Panics when a live graph resolves no worker, two graphs share one, or a
    /// settled graph still resolves one.
    async fn follower_scopes_one_upstream_worker_to_each_graph() {
        let now = Utc::now();
        let fixture = GraphFixture::new(now);
        let attempt = fixture
            .send(&fixture.leader_message(StageOperationV1::SetPlan, 1), now)
            .await
            .expect("the reserving leader activates the graph");
        let live = fixture
            .ingress
            .graph_worker(&fixture.graph)
            .expect("ownership is readable")
            .expect("a live graph owns an upstream worker");

        // A second graph on the same follower must not share the first graph's
        // task cache, or settling either would wait on the other's plans.
        let sibling = GraphFixture::new(now);
        let sibling_attempt = sibling
            .send(&sibling.leader_message(StageOperationV1::SetPlan, 1), now)
            .await
            .expect("the reserving leader activates the sibling graph");
        let other = sibling
            .ingress
            .graph_worker(&sibling.graph)
            .expect("ownership is readable")
            .expect("a live sibling graph owns an upstream worker");
        drop(live);
        drop(other);

        // Settlement takes the graph's own worker, so nothing may enter its
        // cache afterwards and the cache itself is gone.
        fixture
            .ingress
            .finish_attempt(attempt, AnalyticalAttemptOutcome::Success)
            .await
            .expect("the graph's only attempt settles");
        assert!(
            fixture
                .ingress
                .graph_worker(&fixture.graph)
                .expect("ownership is readable")
                .is_none(),
            "a settled graph still resolved an upstream worker"
        );
        // The sibling is untouched by that settlement, which is the observable
        // consequence of the two graphs never having shared one worker.
        assert!(
            sibling
                .ingress
                .graph_worker(&sibling.graph)
                .expect("ownership is readable")
                .is_some(),
            "settling one graph took another graph's upstream worker"
        );
        sibling
            .ingress
            .finish_attempt(sibling_attempt, AnalyticalAttemptOutcome::Success)
            .await
            .expect("the sibling graph's only attempt settles");
    }

    /// An `ExecuteTask`-first graph settles no later than its signed deadline.
    ///
    /// Upstream sends its plan on a spawned coordinator-channel task, so a
    /// valid `ExecuteTask` may activate a graph before any `SetPlan` arrives.
    /// The end of that unary request is *not* a departure signal — upstream may
    /// keep producing rows after consuming the request body — so the graph
    /// stays owned. Its bound is therefore the one the leader already signed:
    /// the absolute deadline in the retained binding. The single ingress-owned
    /// driver observes it, moves the graph to draining, cancels it, and runs
    /// the same joined settlement every other terminal path runs.
    ///
    /// A sibling graph whose envelope keeps a live nested child holds its own
    /// settlement open across the target's whole deadline, which is what proves
    /// the driver settles concurrently rather than serially: a slow cleanup may
    /// not stop the driver from cancelling another graph on time.
    ///
    /// # Panics
    ///
    /// Panics when a live `ExecuteTask`-first graph is released by the end of
    /// its unary request, when it outlives its signed deadline, when a blocked
    /// sibling settlement delays it, or when the bounded settlement queue
    /// cannot hold one wake plus one settlement per admitted graph.
    #[tokio::test]
    async fn execute_task_first_without_set_plan_settles_at_signed_deadline() {
        let now = Utc::now();
        let target_deadline = now + chrono::Duration::milliseconds(400);
        let fixture = GraphFixture::with_deadline(now, target_deadline);

        // One wake and one terminal settlement per admitted graph must fit, or
        // a valid settlement would be retained as a queue-full cleanup failure.
        assert_eq!(
            fixture.ingress.settlement_capacity(),
            2 * fixture.reservations.max_concurrent_graphs(),
            "the settlement queue must hold one wake plus one settlement per admitted graph"
        );

        // The sibling activates first and is handed to the driver immediately,
        // with one real nested child that keeps its drain from completing.
        let (sibling_graph, sibling_message) =
            fixture.sibling(now, now + chrono::Duration::seconds(60));
        fixture
            .send(&sibling_message, now)
            .await
            .expect("the reserving leader activates the sibling graph");
        let sibling_runtime = fixture
            .supervisor
            .graph_runtime(sibling_graph)
            .expect("the sibling graph installed a query-owned runtime");
        let gate = datafusion::execution::memory_pool::MemoryConsumer::new("sibling-gate")
            .register(&sibling_runtime.runtime().memory_pool);
        gate.try_grow(1024)
            .expect("the admitted envelope funds one nested child");
        drop(
            fixture
                .ingress
                .retain_connection(sibling_graph)
                .expect("a live graph retains a coordinator connection"),
        );

        // The target activates from `ExecuteTask` alone. No coordinator channel
        // is ever opened for it, and its unary request has already returned.
        fixture
            .send(
                &fixture.leader_message(StageOperationV1::ExecuteTask, 2),
                now,
            )
            .await
            .expect("the reserving leader activates the graph from ExecuteTask alone");
        assert!(
            fixture
                .ingress
                .graph_worker(&fixture.graph)
                .expect("graph ownership is readable")
                .is_some(),
            "a completed unary ExecuteTask must not release valid work"
        );

        // Past the signed deadline the target settles and releases everything,
        // while the sibling's blocked settlement is still in flight.
        tokio::time::timeout(Duration::from_secs(3), async {
            while fixture.ingress.published(fixture.graph).is_ok() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the target graph settles at its signed deadline");
        assert!(
            fixture
                .ingress
                .graph_worker(&fixture.graph)
                .expect("graph ownership is readable")
                .is_none(),
            "a settled graph released its upstream worker"
        );
        assert!(
            fixture.supervisor.graph_runtime(fixture.graph).is_err(),
            "a settled graph released its supervisor registration and query runtime"
        );
        assert!(
            fixture.ingress.published(sibling_graph).is_ok(),
            "the blocked sibling is still owned by the same driver"
        );

        // Releasing the gate lets the sibling's own settlement complete, and
        // shutdown joins the one driver rather than a detached cleanup task.
        drop(gate);
        let inspection = tokio::time::timeout(Duration::from_secs(10), fixture.ingress.shutdown())
            .await
            .expect("the one settlement driver joins within the bound")
            .expect("the follower shuts down without a poisoned lock");
        assert_eq!(
            inspection.graphs_retained, 0,
            "both graphs were released, not retained"
        );
        assert!(
            fixture
                .ingress
                .live()
                .expect("ownership is readable")
                .is_clean(),
            "the follower retains nothing after both graphs settled"
        );
    }

    /// A retained follower cleanup failure alone fails production readiness.
    ///
    /// The node's Analytical handle is what production readiness consults, and
    /// a live graph is not a reason to stop serving: a healthy active graph and
    /// its live attempt leave the handle healthy. A graph the follower could
    /// *not* settle is different — it is a leak this node still owns and can
    /// name — so the same handle reports unhealthy, which is the exact
    /// predicate `Oracle::is_ready` adds to its startup and admission checks.
    ///
    /// # Panics
    ///
    /// Panics when an active graph fails readiness, when a retained cleanup
    /// failure is not counted, or when it leaves the handle healthy.
    #[tokio::test]
    async fn analytical_cleanup_failure_fails_production_readiness() {
        let now = Utc::now();
        let fixture = GraphFixture::new(now);
        let handle = fixture.execution_handle();
        assert!(
            handle.is_healthy(),
            "a node owning no graph at all is healthy"
        );

        let attempt = fixture
            .send(&fixture.leader_message(StageOperationV1::SetPlan, 1), now)
            .await
            .expect("the reserving leader activates the graph");
        assert!(
            handle.is_healthy(),
            "a live, healthy graph does not make the node unready"
        );

        // One real nested child of the graph's own envelope, so the drain that
        // settlement performs cannot complete for a reason the follower owns.
        let runtime = fixture
            .supervisor
            .graph_runtime(fixture.graph)
            .expect("the activated graph installed a query-owned runtime");
        let stuck = datafusion::execution::memory_pool::MemoryConsumer::new("stuck-child")
            .register(&runtime.runtime().memory_pool);
        stuck
            .try_grow(1024)
            .expect("the admitted envelope funds one nested child");
        fixture
            .ingress
            .finish_attempt(attempt, AnalyticalAttemptOutcome::Success)
            .await
            .expect_err("a graph whose children never drain cannot be settled");

        let retained = fixture.ingress.live().expect("ownership is readable");
        assert_eq!(
            retained.cleanup_failures, 1,
            "the follower named exactly one graph it could not release"
        );
        assert!(
            fixture.supervisor.is_healthy(),
            "the supervisor itself is still serviceable, so readiness must fail for the retained failure alone"
        );
        assert!(
            !handle.is_healthy(),
            "a retained follower cleanup failure fails production readiness"
        );
        drop(stuck);
    }

    /// Proves an undrainable graph is retained, named, and never released.
    ///
    /// Split from the release test only to keep each phase readable; it is the
    /// second half of the same scenario.
    ///
    /// # Panics
    ///
    /// Panics when a failed cleanup releases a graph or is reported as clean.
    async fn follower_retains_a_graph_whose_children_never_drain() {
        // A graph whose envelope keeps a live nested child cannot drain. The
        // follower must report that as a failure and keep everything it owns.
        let now = Utc::now();
        let fixture = GraphFixture::new(now);
        let attempt = fixture
            .send(&fixture.leader_message(StageOperationV1::SetPlan, 1), now)
            .await
            .expect("the reserving leader activates the graph");
        let runtime = fixture
            .supervisor
            .graph_runtime(fixture.graph)
            .expect("the activated graph installed a query-owned runtime");
        let stuck = datafusion::execution::memory_pool::MemoryConsumer::new("stuck-child")
            .register(&runtime.runtime().memory_pool);
        stuck
            .try_grow(1024)
            .expect("the admitted envelope funds one nested child");
        fixture
            .ingress
            .finish_attempt(attempt, AnalyticalAttemptOutcome::Success)
            .await
            .expect_err("a graph whose children never drain cannot be settled");
        let retained = fixture.ingress.live().expect("ownership is readable");
        assert_eq!(retained.graphs, 1, "the undrained graph is still owned");
        assert_eq!(
            retained.cleanup_failures, 1,
            "the follower recorded exactly why the graph could not be released"
        );
        assert!(
            !retained.is_clean(),
            "a retained cleanup failure fails follower readiness"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("graphs are readable"),
            1,
            "a failed settlement released no supervisor guard"
        );

        // Shutdown closes admission first, then reports what stayed retained.
        let refused = fixture
            .send(
                &fixture.leader_message(StageOperationV1::ExecuteTask, 2),
                now,
            )
            .await;
        let inspection = fixture
            .ingress
            .shutdown()
            .await
            .expect("the follower shuts down without a poisoned lock");
        assert!(
            matches!(refused, Err(BifrostError::QueryExecutionFailed)),
            "a draining graph admits nothing new"
        );
        assert_eq!(
            inspection.graphs_retained, 1,
            "shutdown reported the graph it could not release"
        );
        drop(stuck);
        assert!(
            matches!(
                fixture
                    .send(&fixture.leader_message(StageOperationV1::SetPlan, 3), now)
                    .await,
                Err(BifrostError::QueryAdmissionRejected)
            ),
            "a shut-down follower admits no further stage work"
        );
    }

    /// A failed activation publishes nothing and hands the reservation back.
    ///
    /// Activation is a transaction across three fallible steps — the graph
    /// runtime, the retained binding, and the supervisor registration — and a
    /// failure in any of them must leave the follower exactly as it was: no
    /// published graph, no charged envelope, and the pending reservation
    /// restored under its own unchanged expiry so a serialized waiter can still
    /// activate it. Past that expiry the reservation is not resurrected; its
    /// permit and envelope are released instead.
    ///
    /// # Panics
    ///
    /// Panics when a failed activation publishes a graph, consumes the
    /// reservation before its expiry, or resurrects it after.
    #[tokio::test]
    async fn graph_activation_failure_rolls_back_without_publication() {
        // A graph runtime that cannot be built inside the process spill limit.
        let now = Utc::now();
        let fixture = GraphFixture::with_pod_spill_limit(now, 1);
        let message = fixture.leader_message(StageOperationV1::SetPlan, 1);
        assert!(
            fixture.send(&message, now).await.is_err(),
            "a graph whose runtime cannot be built is not activated"
        );
        assert!(
            fixture.ingress.published(fixture.graph).is_err(),
            "a failed activation published no graph"
        );
        assert_eq!(
            fixture.reservations.graph_leases_activated_total(),
            0,
            "a failed activation charged no envelope"
        );
        assert_eq!(
            fixture.reservations.cleanup_expired(now),
            1,
            "the reservation was restored under its own unchanged expiry"
        );
        // The waiter observes no published completion; it simply looks the
        // restored reservation up again and fails for the same real reason.
        assert!(
            fixture
                .send(&fixture.leader_message(StageOperationV1::SetPlan, 2), now)
                .await
                .is_err(),
            "the restored reservation is still the one a later message finds"
        );
        assert_eq!(
            fixture.reservations.cleanup_expired(now),
            1,
            "a second failure did not consume the restored reservation either"
        );

        graph_rollback_restores_a_reservation_only_before_its_expiry().await;
    }

    /// Proves a refused registration restores an activatable reservation.
    ///
    /// Split from the rollback test only to keep each phase readable; it is the
    /// second half of the same scenario.
    ///
    /// # Panics
    ///
    /// Panics when a refused registration publishes a graph, when the restored
    /// reservation cannot be activated, or when an expired one is resurrected.
    async fn graph_rollback_restores_a_reservation_only_before_its_expiry() {
        // A supervisor registration that is refused because the graph is taken.
        let now = Utc::now();
        let fixture = GraphFixture::new(now);
        let occupied = fixture
            .supervisor
            .register_graph(
                fixture.graph,
                fixture_oracle_role()
                    .try_acquire_query(OracleResourceRequest::for_class(
                        QueryClass::Analytical,
                        0.0,
                    ))
                    .expect("an idle Oracle admits one analytical query"),
                AnalyticalGraphRuntime::new(
                    fixture
                        .spill
                        .build_query_runtime(
                            Arc::new(
                                datafusion::execution::memory_pool::UnboundedMemoryPool::default(),
                            ),
                            0,
                        )
                        .expect("an unbounded fixture runtime builds"),
                    fixture_shape(),
                ),
            )
            .expect("the fixture supervisor accepts one direct registration");
        assert!(
            fixture
                .send(&fixture.leader_message(StageOperationV1::SetPlan, 1), now)
                .await
                .is_err(),
            "a graph the supervisor refuses is not activated"
        );
        assert!(
            fixture.ingress.published(fixture.graph).is_err(),
            "a refused registration published no graph"
        );
        assert_eq!(
            fixture.reservations.graph_leases_activated_total(),
            0,
            "a refused registration charged no envelope"
        );
        occupied
            .release()
            .expect("the direct registration releases cleanly");
        fixture
            .send(&fixture.leader_message(StageOperationV1::SetPlan, 2), now)
            .await
            .expect("a serialized waiter activates the restored reservation");
        assert_eq!(
            fixture.reservations.graph_leases_activated_total(),
            1,
            "the restored reservation became exactly one graph"
        );

        // Past its own expiry the reservation is released, not resurrected.
        let now = Utc::now();
        let fixture = GraphFixture::new(now);
        let activation = fixture
            .reservations
            .begin_graph_activation(
                &GraphLeaseRequest {
                    reservation_id: fixture.reservation_id,
                    graph: AnalyticalGraphRef {
                        public_query_id: fixture.graph.public_query_id.as_uuid(),
                        datafusion_query_id: fixture.graph.datafusion_query_id.as_uuid(),
                    },
                    query_id: QueryId::new(fixture.graph.public_query_id.as_uuid()),
                },
                now,
            )
            .expect("the reserved graph begins activation");
        let expired = activation.expires_at() + chrono::Duration::seconds(1);
        activation.rollback(expired);
        assert_eq!(
            fixture.reservations.cleanup_expired(now),
            0,
            "a reservation rolled back past its expiry is released, not restored"
        );
    }

    /// A graph activates exactly once, whichever authorized message arrives first.
    ///
    /// Upstream sends its plan on a spawned coordinator-channel task, so
    /// `SetPlan` and `ExecuteTask` legitimately arrive in either order, and two
    /// equivalent messages can race. All three must converge on one activation:
    /// one envelope charge, one supervisor registration, and the identical
    /// owner handed to every caller. A message that names the same graph under a
    /// different reservation is refused immediately rather than activating a
    /// competing one.
    ///
    /// # Panics
    ///
    /// Panics when a graph is activated more than once, when two callers receive
    /// different owners, or when a mismatched reservation is admitted.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn graph_lease_activation_is_order_independent_and_shared() {
        for (first, second) in [
            (StageOperationV1::SetPlan, StageOperationV1::ExecuteTask),
            (StageOperationV1::ExecuteTask, StageOperationV1::SetPlan),
        ] {
            let now = Utc::now();
            let fixture = GraphFixture::new(now);
            fixture
                .send(&fixture.leader_message(first, 1), now)
                .await
                .expect("the first authorized message activates the graph");
            let activated = fixture
                .ingress
                .published(fixture.graph)
                .expect("activation published exactly one owner");
            fixture
                .send(&fixture.leader_message(second, 2), now)
                .await
                .expect("the second authorized message reuses the same graph");
            assert!(
                Arc::ptr_eq(
                    &activated,
                    &fixture
                        .ingress
                        .published(fixture.graph)
                        .expect("the graph is still published")
                ),
                "both orderings share the identical graph owner"
            );
            assert_eq!(
                fixture.reservations.graph_leases_activated_total(),
                1,
                "the follower charged the reserved envelope exactly once"
            );
            assert_eq!(
                fixture
                    .supervisor
                    .live_graphs()
                    .expect("graphs are readable"),
                1,
                "the graph registered with the node supervisor exactly once"
            );
        }

        // Two equivalent messages released together from separate tasks. The
        // barrier is the synchronization point, so the race is deterministic
        // rather than timing-dependent.
        let now = Utc::now();
        let fixture = Arc::new(GraphFixture::new(now));
        let gate = Arc::new(tokio::sync::Barrier::new(2));
        let racers = (1u8..=2)
            .map(|nonce| {
                let fixture = Arc::clone(&fixture);
                let gate = Arc::clone(&gate);
                tokio::spawn(async move {
                    let message = fixture.leader_message(StageOperationV1::SetPlan, nonce);
                    gate.wait().await;
                    fixture.send(&message, now).await.map(|_| ())
                })
            })
            .collect::<Vec<_>>();
        for racer in racers {
            racer
                .await
                .expect("a racing sender must not panic")
                .expect("a concurrent duplicate reuses the activated graph");
        }
        assert_eq!(
            fixture.reservations.graph_leases_activated_total(),
            1,
            "a concurrent duplicate did not charge a second envelope"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("graphs are readable"),
            1,
            "a concurrent duplicate did not register a second graph"
        );

        // A different reservation for the same graph is not a duplicate; it is a
        // competing owner, and it is refused before anything is decoded.
        let mut mismatched = fixture.leader_message(StageOperationV1::ExecuteTask, 3);
        mismatched.reservation_id = Uuid::from_u128(77).to_string();
        assert!(
            matches!(
                fixture.send(&mismatched, now).await,
                Err(BifrostError::QueryPeerSecurity)
            ),
            "a message naming a different reservation for a live graph is refused"
        );
        assert_eq!(
            fixture.reservations.graph_leases_activated_total(),
            1,
            "the refusal did not activate a competing graph"
        );
        assert_eq!(
            fixture.resolutions.load(Ordering::SeqCst),
            0,
            "no refusal reached a provider"
        );
    }

    /// A live graph lease retains its exact reservation and first-ticket authority.
    ///
    /// The retained binding is the union of the reservation half — identity,
    /// reserving leader and fence, original expiry, and envelope — and the
    /// immutable graph half of the first verified stage ticket. Every later
    /// message is authorized against it before anything is decoded, resolved, or
    /// read, and neither half may be widened or replaced by a later message.
    ///
    /// # Panics
    ///
    /// Panics when a mutated message is admitted, when a legitimate coordinator
    /// is refused, or when a refusal reaches a provider.
    #[tokio::test]
    async fn graph_lease_binding_mutation_is_refused_before_io() {
        let now = Utc::now();
        let fixture = GraphFixture::new(now);

        // The reserving leader activates the graph even though the destination
        // cut deliberately does not name it.
        let activate = fixture.leader_message(StageOperationV1::SetPlan, 1);
        fixture
            .send(&activate, now)
            .await
            .expect("the reserving leader activates the graph it reserved");
        assert_eq!(
            fixture.reservations.graph_leases_activated_total(),
            1,
            "one reservation became exactly one graph"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("graphs are readable"),
            1,
            "activation registered the graph with the node supervisor"
        );

        // A middle-stage participant named by the immutable cut is a valid
        // later coordinator for the same graph.
        let mut participant = fixture.leader_message(StageOperationV1::ExecuteTask, 2);
        participant.source_node_id = NodeId::new(Uuid::from_u128(4));
        participant.source_fence = 9;
        fixture
            .send(&participant, now)
            .await
            .expect("a coordinator named by the immutable cut addresses the live graph");

        // A source in neither authorization branch is refused, and neither
        // branch widens the other.
        let mut stranger = fixture.leader_message(StageOperationV1::ExecuteTask, 3);
        stranger.source_node_id = NodeId::new(Uuid::from_u128(99));
        stranger.source_fence = 1;
        assert!(
            fixture.send(&stranger, now).await.is_err(),
            "a coordinator that is neither the reserving leader nor an exact cut \
             participant is refused"
        );
        let mut restarted = fixture.leader_message(StageOperationV1::ExecuteTask, 4);
        restarted.source_node_id = NodeId::new(Uuid::from_u128(4));
        restarted.source_fence = 10;
        assert!(
            fixture.send(&restarted, now).await.is_err(),
            "a cut participant presenting a different fence is a different \
             incarnation and is refused"
        );
        let mut restarted_leader = fixture.leader_message(StageOperationV1::ExecuteTask, 5);
        restarted_leader.source_fence = 4;
        assert!(
            fixture.send(&restarted_leader, now).await.is_err(),
            "the reserving leader's authority is its exact node and fence pair"
        );

        // Every immutable field of the first ticket is retained, and mutating
        // any one of them refuses the message.
        let mut widened_deadline = fixture.leader_message(StageOperationV1::ExecuteTask, 6);
        widened_deadline.absolute_deadline_ms += 60_000;
        assert!(
            fixture.send(&widened_deadline, now).await.is_err(),
            "a later message may not extend the graph's absolute deadline"
        );
        let mut widened_cut = fixture.leader_message(StageOperationV1::ExecuteTask, 7);
        widened_cut
            .participants
            .push(super::super::peer::StageParticipantV1 {
                node_id: Uuid::from_u128(5).as_bytes().to_vec(),
                fence: 1,
                address: "https://intruder.invalid/".to_owned(),
                reservation_id: Uuid::from_u128(55).to_string(),
            });
        assert!(
            fixture.send(&widened_cut, now).await.is_err(),
            "a later message may not widen the immutable participant cut"
        );

        assert_eq!(
            fixture.reservations.graph_leases_activated_total(),
            1,
            "no refused message activated a second envelope"
        );
        assert_eq!(
            fixture
                .supervisor
                .live_graphs()
                .expect("graphs are readable"),
            1,
            "the live graph survived every refusal unchanged"
        );
        assert_eq!(
            fixture.resolutions.load(Ordering::SeqCst),
            0,
            "no refused message reached a provider, cache, or source"
        );
    }

    /// Builds the production admission owner with exactly one Analytical slot.
    ///
    /// One slot is what makes a second Analytical caller queue rather than
    /// proceed, which is the only honest way to observe that a retained graph
    /// is still holding this node's capacity.
    fn single_slot_analytical_admission() -> Arc<super::super::admission::OracleAdmission> {
        super::super::admission::admission_owner_for_test(
            super::super::admission::OracleAdmissionConfig {
                analytical_slots: 1,
                max_queue_wait: Duration::from_secs(30),
                ..super::super::admission::OracleAdmissionConfig::default()
            },
            super::super::admission::tests::test_resources(),
        )
    }

    /// Leases one leader graph from a real admission owner, production-shaped.
    ///
    /// The returned guard owns the Analytical attempt exactly as a live query
    /// stream's does, which is what lets the terminal path drive the real
    /// admission transfer rather than a test-held substitute.
    ///
    /// # Panics
    ///
    /// Panics when the owner cannot admit or the handle cannot lease, which
    /// would make every ownership assertion built on the result vacuous.
    async fn lease_over_production_admission(
        fixture: &GraphFixture,
        handle: &AnalyticalExecutionHandle,
        admission: &Arc<super::super::admission::OracleAdmission>,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> (
        Box<super::super::admission::AdmittedQueryGuard>,
        AnalyticalGraphKey,
    ) {
        let mut admitted = admission
            .admit(super::super::admission::PreparedAdmission {
                tenant: fixture.tenant_id,
                query_class: QueryClass::Analytical,
                local_ratio: 0.0,
                deadline: std::time::Instant::now() + Duration::from_mins(1),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("an idle Oracle admits one analytical query");
        let attempt = fixture_attempt(301, 302);
        let cut = super::super::participant_cut::tests::analytical_cut(
            now,
            fixture.node_id.as_uuid().as_u128(),
            expires_at,
        );
        let (session, ownership) = handle
            .lease_session(super::AnalyticalLeaseInputs {
                attempt: &attempt,
                cut: &cut,
                context: &context_for_leasing(),
                admitted: &mut admitted,
                work_units: 4,
                config: min_grant_lease_config(),
                deadline: tokio::time::Instant::now() + Duration::from_mins(1),
            })
            .expect("the leader leases one session from its admitted envelope");
        drop(session);
        let graph = ownership.key().graph();
        // Production shape: the stream owns the guard and the guard owns the
        // Analytical attempt, so settlement drives the real transfer.
        admitted.analytical = Some(ownership);
        (Box::new(admitted), graph)
    }

    /// A failed cleanup keeps this query's admission charged with its graph.
    ///
    /// Admission has to follow ownership, not the stream that started it. The
    /// production terminal moves the permit into the graph *before* it signals
    /// settlement, so a cleanup that cannot be confirmed leaves the envelope and
    /// the permit charged together: the class stays occupied, `active_queries`
    /// stays at one, and a queued Analytical caller keeps waiting for capacity
    /// this node has not actually returned. Releasing the permit at the terminal
    /// instead would hand that waiter a slot backed by a stranded envelope, and
    /// false readiness would not retract a grant already made.
    ///
    /// # Panics
    ///
    /// Panics when a failed settlement returns admission early, when a queued
    /// waiter is admitted against retained capacity, or when authoritative
    /// cleanup fails to return both owners in order.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_failed_graph_cleanup_keeps_its_query_admission_charged() {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(60);
        let fixture = GraphFixture::new(now);
        let handle = fixture.execution_handle();
        let admission = single_slot_analytical_admission();
        let (admitted, graph) =
            lease_over_production_admission(&fixture, &handle, &admission, now, expires_at).await;
        let admitted = *admitted;

        // The injected cleanup failure: a stage of this graph the settlement
        // sequence never joins, so the graph cannot be confirmed released.
        let stray = fixture
            .supervisor
            .spawn_attempt(
                stray_attempt_key(graph),
                AnalyticalAttemptGrant { scratch_bytes: 0 },
            )
            .expect("an active graph admits one more stage");

        let queued_admission = Arc::clone(&admission);
        let queued_tenant = fixture.tenant_id;
        let queued = tokio::spawn(async move {
            queued_admission
                .admit(super::super::admission::PreparedAdmission {
                    tenant: queued_tenant,
                    query_class: QueryClass::Analytical,
                    local_ratio: 0.0,
                    deadline: std::time::Instant::now() + Duration::from_secs(30),
                    cancellation: CancellationToken::new(),
                })
                .await
        });
        settle_lifecycle().await;
        assert!(
            !queued.is_finished(),
            "the second Analytical caller queues behind the charged class slot"
        );

        let mut stream_admission = Some(admitted);
        let settlement = super::super::query_stream::settle_analytical(
            &mut stream_admission,
            wyrd_spec::vala::api::QueryTerminalOutcome::Failed,
        )
        .await;
        assert!(
            !settlement.clean,
            "an unconfirmed cleanup cannot report a clean settlement"
        );
        assert!(
            settlement.transferred,
            "the graph took this query's admission owner before settlement"
        );
        assert!(
            stream_admission.is_none(),
            "a transferred permit is no longer the stream's to release"
        );
        assert!(
            fixture.supervisor.retains_admission_for_test(graph),
            "the retained graph holds the permit the stream handed over"
        );
        assert_eq!(
            admission.runtime_inspection().active_queries,
            1,
            "a failed settlement leaves the query's admission charged"
        );
        assert_eq!(
            fixture
                .supervisor
                .draining_graphs()
                .expect("the supervisor reports retained cleanup"),
            1,
            "the graph is retained as draining rather than removed"
        );
        assert!(
            !handle.is_healthy(),
            "a node retaining an unsettled graph does not advertise readiness"
        );
        settle_lifecycle().await;
        assert!(
            !queued.is_finished(),
            "false readiness does not retract capacity, so the waiter stays queued"
        );

        // Cleanup becomes authoritative: the stage that blocked it is gone, and
        // removing the graph returns its envelope first and its permit second.
        drop(stray);
        fixture
            .supervisor
            .release_graph(graph)
            .expect("an idle graph releases once its last stage is gone");
        let queued = queued
            .await
            .expect("the queued admission task completes")
            .expect("the queued caller is admitted once the graph returns its owners");
        assert_eq!(
            admission.runtime_inspection().active_queries,
            1,
            "the returned capacity is now charged to the caller that waited for it"
        );
        queued.release();
        assert_eq!(
            admission.runtime_inspection().active_queries,
            0,
            "every admission this query held returns exactly once"
        );
    }

    /// An exceptional lifecycle end retains its graph instead of releasing it.
    ///
    /// A task that is aborted or panics never ran the one cleanup sequence, so
    /// nothing it owned was returned. Two things must therefore hold: the
    /// reservations it was holding produce no detached release — there is no
    /// `Drop` cleanup left to spawn one — and the graph, its envelope, and the
    /// query's admission permit stay registered as attributable residue rather
    /// than being swept away by a guard's `Drop` or by shutdown.
    ///
    /// # Panics
    ///
    /// Panics when a dropped reservation owner issues an RPC, when an aborted
    /// lifecycle silently releases its graph, or when shutdown sweeps residue
    /// it should have reported.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_aborted_lifecycle_task_retains_its_graph_instead_of_releasing_it() {
        let fixture = GraphFixture::new(Utc::now());
        let oracle = fixture_oracle_role();
        let leased = lease_over_lossy_peers(&fixture, &oracle, 2);
        let (admitted, ownership, transport) = *leased;
        ownership
            .publish_participants()
            .await
            .expect("every addressed follower accepted its reservation");
        let graph = ownership.key().graph();
        assert!(
            ownership.retain_admission(admitted).is_ok(),
            "the live graph takes this query's admission permit"
        );
        assert_eq!(
            transport.releases().len(),
            0,
            "nothing is released while the graph is still live"
        );

        // The exceptional end: the task stops mid-lifecycle without running its
        // cleanup sequence, exactly as a panic would.
        fixture.supervisor.abort_lifecycle_task_for_test(graph);
        drop(ownership);
        settle_lifecycle().await;
        assert_eq!(
            transport.releases().len(),
            0,
            "a dropped reservation owner spawns no detached release of its own"
        );
        assert!(
            fixture.supervisor.retains_admission_for_test(graph),
            "the graph still holds the query's admission permit"
        );

        let inspection = fixture
            .supervisor
            .shutdown()
            .await
            .expect("shutdown joins the aborted task rather than hanging on it");
        assert_eq!(
            (inspection.graphs_released, inspection.graphs_retained),
            (0, 1),
            "the shutdown sweep reports the retained graph instead of removing it"
        );
        assert!(
            fixture
                .supervisor
                .graph_settlement_failure(graph)
                .expect("the supervisor reports why cleanup failed")
                .is_some(),
            "an aborted lifecycle is recorded as a cleanup failure"
        );
        assert_eq!(
            fixture
                .supervisor
                .draining_graphs()
                .expect("the supervisor reports retained cleanup"),
            1,
            "the retained graph is visible as draining, which is what fails readiness"
        );
        assert!(
            !fixture.execution_handle().is_healthy(),
            "a node retaining an unsettled graph does not advertise readiness"
        );
        assert!(
            fixture.supervisor.retains_admission_for_test(graph),
            "retained admission stays charged with the graph it belongs to"
        );
        assert_eq!(
            transport.releases().len(),
            0,
            "no release RPC was ever issued outside the lifecycle sequence"
        );
    }

    /// Every peer call this graph makes ends by the envelope's own deadline.
    ///
    /// A permanently pending peer is the only failure a reservation owner
    /// cannot distinguish from a slow one, and awaiting it directly would park
    /// reservation, cleanup, settlement, and shutdown behind one unreachable
    /// node. Four bounds are proven on one clock: graph cancellation interrupts
    /// an in-flight reservation, the graph deadline interrupts one nothing
    /// cancels, a release attempt cannot outlive its own bound, and no peer
    /// call at all begins after the deadline has passed. Settlement then
    /// reports failure rather than hanging, and shutdown reports the residue.
    ///
    /// # Panics
    ///
    /// Panics when a peer call outlives its bound, when an unanswerable peer
    /// parks settlement or shutdown, or when unresolved residue is reported as
    /// a clean terminal.
    #[tokio::test(start_paused = true)]
    async fn graph_peer_operations_are_bounded_by_the_graph_deadline() {
        Box::pin(assert_cancellation_interrupts_a_pending_reservation()).await;
        Box::pin(assert_the_deadline_interrupts_a_pending_reservation()).await;
        Box::pin(assert_release_is_bounded_and_residue_is_reported()).await;
        Box::pin(assert_post_deadline_expiry_returns_the_graph()).await;
    }

    /// Graph cancellation ends a reservation no participant will ever answer.
    ///
    /// # Panics
    ///
    /// Panics when a cancelled graph still addresses a further participant or
    /// never publishes its reservation verdict.
    async fn assert_cancellation_interrupts_a_pending_reservation() {
        let expires_at = Utc::now() + chrono::Duration::seconds(60);
        let fixture = ReservationFixture::start_bounded(
            2,
            expires_at,
            tokio::time::Instant::now() + Duration::from_mins(10),
        );
        fixture.transport.hang_reserves();
        fixture
            .graph
            .supervisor
            .signal_reserve(fixture.graph.graph)
            .expect("an active graph accepts one reserve request");
        settle_lifecycle().await;
        assert_eq!(
            fixture.transport.reserves().len(),
            1,
            "reservation parks on the first unanswerable participant"
        );
        fixture
            .graph
            .supervisor
            .graph_cancellation(fixture.graph.graph)
            .expect("the registered graph owns a cancellation child")
            .expect("the registered graph owns a cancellation child")
            .cancel();
        let cancelled_at = tokio::time::Instant::now();
        let refused = fixture
            .signals
            .publish_participants()
            .await
            .expect_err("a cancelled reservation cannot publish a cut");
        assert!(
            matches!(refused, BifrostError::QueryAdmissionRejected),
            "an interrupted reservation takes the existing refusal path: {refused:?}"
        );
        assert!(
            tokio::time::Instant::now() < cancelled_at + Duration::from_secs(1),
            "cancellation ends the reservation immediately, not at the far deadline"
        );
        assert_eq!(
            fixture.transport.reserves().len(),
            1,
            "cancellation stops reservation instead of charging the next participant"
        );
        assert!(
            fixture.signals.participants().get().is_none(),
            "no partial cut is ever published"
        );
    }

    /// The graph deadline ends a reservation nothing else interrupts.
    ///
    /// # Panics
    ///
    /// Panics when an unanswerable participant parks reservation past the
    /// envelope's own bound.
    async fn assert_the_deadline_interrupts_a_pending_reservation() {
        let expires_at = Utc::now() + chrono::Duration::seconds(60);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let fixture = ReservationFixture::start_bounded(2, expires_at, deadline);
        fixture.transport.hang_reserves();
        let refused = fixture
            .signals
            .publish_participants()
            .await
            .expect_err("a reservation nothing answers cannot publish a cut");
        assert!(
            matches!(refused, BifrostError::QueryAdmissionRejected),
            "a timed-out reservation takes the existing refusal path: {refused:?}"
        );
        let ended = tokio::time::Instant::now();
        assert!(
            ended >= deadline && ended < deadline + Duration::from_secs(1),
            "the reservation ended at the graph's own deadline, not at some other bound"
        );
        assert!(
            fixture
                .transport
                .call_instants()
                .iter()
                .all(|began| *began < deadline),
            "no peer call begins after the graph deadline"
        );
    }

    /// A release nothing acknowledges is bounded, retained, and then reported.
    ///
    /// # Panics
    ///
    /// Panics when a release attempt outlives its bound, when a peer call
    /// begins after the deadline, when unresolved residue settles as a success,
    /// or when shutdown awaits an unanswerable peer instead of reporting it.
    async fn assert_release_is_bounded_and_residue_is_reported() {
        // Far beyond the graph deadline on purpose: conservative expiry cannot
        // resolve these records, so the only thing that can end the retry is
        // the graph's own bound.
        let expires_at = Utc::now() + chrono::Duration::hours(1);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let fixture = ReservationFixture::start_bounded(2, expires_at, deadline);
        fixture
            .signals
            .publish_participants()
            .await
            .expect("every addressed follower accepted its reservation");
        fixture.transport.hang_releases();
        fixture.signals.terminal(AnalyticalAttemptOutcome::Failed);
        let error = fixture
            .signals
            .settled()
            .await
            .expect_err("unresolved reservations cannot settle as a success");
        assert!(
            matches!(error, BifrostError::Internal { .. }),
            "residue is reported to the caller rather than logged: {error:?}"
        );
        let ended = tokio::time::Instant::now();
        assert!(
            ended >= deadline && ended < deadline + RETAINED_RELEASE_RETRY,
            "cleanup retried the ambiguous release to the graph deadline and stopped there"
        );
        let releases = fixture.transport.releases().len();
        assert!(
            releases >= 2,
            "each accepted reservation was attempted at least once: {releases}"
        );
        assert!(
            fixture
                .transport
                .call_instants()
                .iter()
                .all(|began| *began < deadline),
            "no peer call begins after the graph deadline"
        );
        assert_eq!(
            fixture.draining(),
            1,
            "an unacknowledged release keeps the graph attributable as draining"
        );
        let inspection = fixture
            .graph
            .supervisor
            .shutdown()
            .await
            .expect("shutdown joins the lifecycle task instead of awaiting a peer");
        assert_eq!(
            (inspection.graphs_released, inspection.graphs_retained),
            (0, 1),
            "shutdown reports the retained residue rather than sweeping it away"
        );
        assert!(
            fixture
                .transport
                .call_instants()
                .iter()
                .all(|began| *began < deadline),
            "shutdown issues no peer call past the graph deadline either"
        );
    }

    /// Authoritative expiry after the deadline still returns the retained graph.
    ///
    /// The deadline ends this node's right to address a peer, not its ownership
    /// of what it took. A record whose conservative expiry falls after the
    /// deadline is therefore still the lifecycle task's to resolve: the caller
    /// is failed on time and every RPC stops, but the local expiry check keeps
    /// running until the follower must have dropped the envelope, and only then
    /// are the graph and its admission returned.
    ///
    /// # Panics
    ///
    /// Panics when the caller is failed late, when a peer call is issued past
    /// the deadline, or when authoritative expiry leaves the graph and its
    /// admission charged forever.
    async fn assert_post_deadline_expiry_returns_the_graph() {
        // Already elapsed on the wall clock, so the local pending TTL measured
        // from the reserve response is the bound that decides — and it falls
        // after this graph's own deadline.
        let expires_at = Utc::now() - chrono::Duration::seconds(1);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        let fixture = ReservationFixture::start_bounded(2, expires_at, deadline);
        fixture
            .signals
            .publish_participants()
            .await
            .expect("every addressed follower accepted its reservation");
        fixture.transport.hang_releases();
        fixture.signals.terminal(AnalyticalAttemptOutcome::Failed);
        let error = fixture
            .signals
            .settled()
            .await
            .expect_err("unresolved reservations cannot settle as a success");
        assert!(
            matches!(error, BifrostError::Internal { .. }),
            "the caller is failed at the deadline rather than held to expiry: {error:?}"
        );
        let ended = tokio::time::Instant::now();
        assert!(
            ended >= deadline && ended < deadline + RETAINED_RELEASE_RETRY,
            "the caller's failure is published at the graph deadline"
        );
        assert_eq!(
            fixture.draining(),
            1,
            "the unacknowledged release keeps the graph attributable as draining"
        );
        let attempted = fixture.transport.releases().len();
        // Past both the deadline and the pending TTL the response was received
        // under: the record is now authoritatively expired.
        tokio::time::sleep(Duration::from_secs(3)).await;
        settle_lifecycle().await;
        assert_eq!(
            fixture.transport.releases().len(),
            attempted,
            "no peer call is issued after the graph deadline"
        );
        assert!(
            fixture
                .transport
                .call_instants()
                .iter()
                .all(|began| *began < deadline),
            "conservative expiry is a local check, not another round of RPCs"
        );
        assert_eq!(
            fixture.draining(),
            0,
            "authoritative expiry clears the reservation ambiguity"
        );
        assert_eq!(
            fixture
                .graph
                .supervisor
                .live_graphs()
                .expect("the supervisor reports live graphs"),
            0,
            "the retained graph and its admission are returned once nothing remote owns them"
        );
    }

    /// Deterministic peer transport recording every reserve and release it sees.
    ///
    /// Both outcomes are chosen by the test rather than by timing, which is what
    /// makes partial reservation failure and ambiguous release acknowledgement
    /// observable without a sleep or a live peer.
    struct ReservingTransport {
        /// Participants a reserve was issued to, in issue order.
        reserved: std::sync::Mutex<Vec<NodeId>>,
        /// Participants a release was issued to, with the reservation named.
        released: std::sync::Mutex<Vec<(NodeId, String)>>,
        /// The monotonic instant every peer call began at, reserve or release.
        ///
        /// A bound is only provable against the clock the bound is expressed
        /// in, so the call sites are stamped rather than merely counted.
        began: std::sync::Mutex<Vec<tokio::time::Instant>>,
        /// While set, a reserve is accepted into a future that never resolves.
        reserve_hangs: std::sync::atomic::AtomicBool,
        /// While set, a release is accepted into a future that never resolves.
        release_hangs: std::sync::atomic::AtomicBool,
        /// How many reserves are accepted before the rest are refused.
        accepted: usize,
        /// While set, every release answers with an unacknowledged failure.
        release_fails: std::sync::atomic::AtomicBool,
        /// Wall-clock expiry every accepted reservation is minted with.
        expires_at: DateTime<Utc>,
    }

    impl ReservingTransport {
        /// Builds a transport accepting exactly `accepted` reservations.
        fn new(accepted: usize, expires_at: DateTime<Utc>) -> Self {
            Self {
                reserved: std::sync::Mutex::new(Vec::new()),
                released: std::sync::Mutex::new(Vec::new()),
                began: std::sync::Mutex::new(Vec::new()),
                reserve_hangs: std::sync::atomic::AtomicBool::new(false),
                release_hangs: std::sync::atomic::AtomicBool::new(false),
                accepted,
                release_fails: std::sync::atomic::AtomicBool::new(false),
                expires_at,
            }
        }

        /// Makes every later reserve park forever instead of answering.
        fn hang_reserves(&self) {
            self.reserve_hangs
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }

        /// Makes every later release park forever instead of answering.
        fn hang_releases(&self) {
            self.release_hangs
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }

        /// Returns the monotonic instant each peer call began at.
        fn call_instants(&self) -> Vec<tokio::time::Instant> {
            self.began.lock().expect("call log is readable").clone()
        }

        /// Records that one peer call is starting now.
        fn record_call(&self) {
            self.began
                .lock()
                .expect("call log is writable")
                .push(tokio::time::Instant::now());
        }

        /// Returns the participants a reserve was issued to.
        fn reserves(&self) -> Vec<NodeId> {
            self.reserved
                .lock()
                .expect("reserve log is readable")
                .clone()
        }

        /// Returns the releases issued so far, participant and reservation.
        fn releases(&self) -> Vec<(NodeId, String)> {
            self.released
                .lock()
                .expect("release log is readable")
                .clone()
        }
    }

    #[async_trait]
    impl super::super::dispatcher::OraclePeerTransport for ReservingTransport {
        /// Accepts the first `accepted` reservations and refuses the rest.
        async fn reserve(
            &self,
            worker: NodeId,
            _request: ReserveNodeSlotsRequest,
        ) -> Result<
            wyrd_spec::vala::api::ReserveNodeSlotsResponse,
            super::super::dispatcher::DispatchError,
        > {
            self.record_call();
            let ordinal = {
                let mut reserved = self.reserved.lock().expect("reserve log is writable");
                let ordinal = reserved.len();
                reserved.push(worker);
                ordinal
            };
            if self.reserve_hangs.load(std::sync::atomic::Ordering::SeqCst) {
                std::future::pending::<()>().await;
            }
            if ordinal >= self.accepted {
                return Ok(wyrd_spec::vala::api::ReserveNodeSlotsResponse::Rejected(
                    wyrd_spec::vala::api::ReservationRejected {
                        retry_after_ms: 1_000,
                    },
                ));
            }
            Ok(wyrd_spec::vala::api::ReserveNodeSlotsResponse::Pending(
                wyrd_spec::vala::api::PendingNodeReservation {
                    reservation_id: wyrd_spec::vala::api::ReservationId::new(Uuid::from_u128(
                        200 + ordinal as u128,
                    )),
                    expires_at: self.expires_at,
                },
            ))
        }

        /// Records the release and answers as the test currently dictates.
        async fn release(
            &self,
            worker: NodeId,
            request: wyrd_spec::vala::api::ReleaseNodeSlotsRequest,
        ) -> Result<(), super::super::dispatcher::DispatchError> {
            self.record_call();
            {
                self.released
                    .lock()
                    .expect("release log is writable")
                    .push((worker, request.reservation_id.as_uuid().to_string()));
            }
            if self.release_hangs.load(std::sync::atomic::Ordering::SeqCst) {
                std::future::pending::<()>().await;
            }
            if self.release_fails.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(super::super::dispatcher::DispatchError::Terminal);
            }
            Ok(())
        }

        /// Never reached: this transport exists for reservation ownership only.
        async fn execute(
            &self,
            _worker: NodeId,
            _request: wyrd_spec::vala::api::ExecuteFragmentRequest,
            _admitted_grant: Option<super::super::dispatcher::LeaderAdmittedGrant>,
        ) -> Result<
            super::super::dispatcher::WorkerAttemptStream,
            super::super::dispatcher::DispatchError,
        > {
            unreachable!("the reservation owner never dispatches a fragment")
        }
    }

    /// Everything one reservation-lifecycle assertion needs, composed once.
    struct ReservationFixture {
        /// The graph fixture supplying the supervisor and execution handle.
        graph: GraphFixture,
        /// The recording transport every reserve and release is issued through.
        transport: Arc<ReservingTransport>,
        /// The two remote participants this attempt may address.
        remote: Vec<(Url, super::super::dispatcher::DispatchCandidate)>,
        /// The attempt-side half of the started lifecycle task.
        signals: AnalyticalGraphSignals,
        /// Process root the registered graph's envelope was admitted from.
        ///
        /// Retained because releasing the graph returns the envelope to *this*
        /// root; dropping it early would make every ownership assertion vacuous.
        _oracle: OracleResources,
    }

    impl ReservationFixture {
        /// Starts one graph lifecycle over a transport with the given behavior.
        ///
        /// # Panics
        ///
        /// Panics when the fixture endpoints are not valid URLs, which would
        /// make every assertion below vacuous.
        fn start(accepted: usize, expires_at: DateTime<Utc>) -> Self {
            Self::start_bounded(
                accepted,
                expires_at,
                tokio::time::Instant::now() + Duration::from_mins(1),
            )
        }

        /// Starts one graph lifecycle bounded by an explicit monotonic deadline.
        ///
        /// # Panics
        ///
        /// Panics when the fixture endpoints are not valid URLs, which would
        /// make every assertion below vacuous.
        fn start_bounded(
            accepted: usize,
            expires_at: DateTime<Utc>,
            deadline: tokio::time::Instant,
        ) -> Self {
            let graph = GraphFixture::new(Utc::now());
            let transport = Arc::new(ReservingTransport::new(accepted, expires_at));
            let directory = Arc::new(
                super::super::dispatcher::OraclePeerTransportDirectory::new_for_test(
                    graph.node_id,
                    Arc::clone(&transport)
                        as Arc<dyn super::super::dispatcher::OraclePeerTransport>,
                    Arc::clone(&transport)
                        as Arc<dyn super::super::dispatcher::OraclePeerTransport>,
                ),
            );
            let remote = [
                (31_u128, 5_u64, "https://follower-a.invalid/"),
                (32, 6, "https://follower-b.invalid/"),
            ]
            .into_iter()
            .map(|(node, fence, endpoint)| {
                (
                    Url::parse(endpoint).expect("a fixture endpoint is a valid URL"),
                    super::super::dispatcher::DispatchCandidate {
                        node_id: NodeId::new(Uuid::from_u128(node)),
                        role: wyrd_spec::vala::api::ClusterRole::Oracle,
                        worker_fence: fence,
                        endpoint: Some(endpoint.to_owned()),
                    },
                )
            })
            .collect::<Vec<_>>();
            // Registered exactly as production registers, because the graph
            // entry is the lifecycle registry: a task with no entry could not
            // move its graph to draining, retain a failed cleanup, or be joined
            // by shutdown.
            let oracle = fixture_oracle_role();
            let resources = oracle
                .try_acquire_query(OracleResourceRequest::for_class(
                    QueryClass::Analytical,
                    0.0,
                ))
                .expect("an idle Oracle admits one analytical query");
            let runtime = graph
                .spill
                .build_query_runtime(resources.memory_pool(), resources.scratch_bytes)
                .expect("the fixture spill owner builds one query runtime");
            let graph_guard = graph
                .supervisor
                .register_graph(
                    graph.graph,
                    resources,
                    AnalyticalGraphRuntime::new(runtime, fixture_shape()),
                )
                .map_err(|(_, error)| error)
                .expect("an empty supervisor registers one graph");
            let signals = AnalyticalGraphLifecycle::start(
                graph.graph,
                Arc::clone(&graph.supervisor),
                Some(directory),
                remote.clone(),
                ReserveNodeSlotsRequest {
                    query_id: QueryId::new(graph.graph.public_query_id.as_uuid()),
                    leader_node_id: graph.leader_node_id,
                    leader_fencing_token: graph.leader_fence,
                    query_class: QueryClass::Analytical,
                    slot_units: ANALYTICAL_GRAPH_SLOT_UNITS,
                    expires_at,
                    graph: Some(AnalyticalGraphRef {
                        public_query_id: graph.graph.public_query_id.as_uuid(),
                        datafusion_query_id: graph.graph.datafusion_query_id.as_uuid(),
                    }),
                },
                deadline,
                AnalyticalGraphLifecycleOwners {
                    attempt: None,
                    graph_guard: Some(graph_guard),
                },
            )
            .expect("the registered graph accepts one lifecycle task");
            Self {
                graph,
                transport,
                remote,
                signals,
                _oracle: oracle,
            }
        }

        /// Counts the graphs this fixture's supervisor retains cleanup for.
        fn draining(&self) -> usize {
            self.graph
                .supervisor
                .draining_graphs()
                .expect("the draining registry is readable")
        }

        /// Builds a resolver over this graph's own participant cell.
        fn resolver(&self) -> AnalyticalChannelResolver {
            AnalyticalChannelResolver::new(
                Arc::new(AnalyticalCoordinatorIdentity {
                    source_node_id: self.graph.node_id,
                    source_fence: self.graph.fence,
                    tenant_id: self.graph.tenant_id,
                    graph: self.graph.graph,
                    snapshot_digest: "fixture-snapshot".to_owned(),
                    attempt: 0,
                    reservation_id: String::new(),
                    permission_digest: "fixture-permissions".to_owned(),
                }),
                BifrostPeerTls::unreachable_for_test(),
                Arc::new(super::super::dispatcher::StaticOraclePeerCredentials::new(
                    secrecy::SecretString::from("fixture-bearer"),
                )),
                self.signals.participants(),
                Arc::default(),
                AnalyticalStageSigning {
                    authority: Arc::new(VerifyingStageAuthority),
                    absolute_deadline_ms: self.graph.deadline_ms,
                    ticket_ttl: chrono::Duration::seconds(30),
                },
            )
        }
    }

    /// Yields until every already-runnable lifecycle step has run.
    ///
    /// The lifecycle task is a peer of the test task, so a plain assertion after
    /// a signal would race it. This drains the ready queue instead of sleeping,
    /// which keeps every assertion below deterministic.
    async fn settle_lifecycle() {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
    }

    /// Nothing is reserved until selection becomes irreversible.
    ///
    /// Planning, an unsupported physical shape, and a no-exchange fallback all
    /// leave the lifecycle unsignalled, so no follower is charged for a plan
    /// that may never be selected — and the cell every channel resolves through
    /// fails closed rather than dialing while it is unset.
    ///
    /// # Panics
    ///
    /// Panics when a reserve is issued early or an unset cell resolves.
    async fn assert_nothing_reserved_before_selection(fixture: &ReservationFixture) {
        settle_lifecycle().await;
        assert!(
            fixture.transport.reserves().is_empty(),
            "nothing is reserved before selection is final"
        );
        assert!(
            fixture.signals.participants().get().is_none(),
            "the graph's participant cell is unset while nothing is reserved"
        );
        let Err(unresolved) = fixture.resolver().resolve(&fixture.remote[0].0) else {
            panic!("an unpublished cut resolves to no destination");
        };
        assert!(
            unresolved
                .to_string()
                .contains("no published participant cut"),
            "resolution fails closed on the unset cell rather than dialing: {unresolved}"
        );
    }

    /// One reserve per participant, all acknowledged before the cut exists.
    ///
    /// # Panics
    ///
    /// Panics when a participant is reserved more than once, when the published
    /// cut does not carry the follower's own reservation identity, or when a
    /// repeated publish charges a follower again.
    async fn assert_reserved_once_and_published(fixture: &ReservationFixture) {
        let url = fixture.remote[0].0.clone();
        fixture
            .signals
            .publish_participants()
            .await
            .expect("every participant accepts its reservation");
        assert_eq!(
            fixture.transport.reserves(),
            vec![fixture.remote[0].1.node_id, fixture.remote[1].1.node_id],
            "each remote participant receives exactly one reserve"
        );
        let published = fixture
            .signals
            .participants()
            .get()
            .cloned()
            .expect("the complete cut is published before any dispatch");
        assert_eq!(
            published
                .destination(&url)
                .expect("the reserved participant is in the published cut")
                .reservation_id,
            Uuid::from_u128(200).to_string(),
            "the follower's own server-generated reservation is carried unchanged"
        );
        assert!(
            fixture.resolver().resolve(&url).is_ok(),
            "a published cut resolves the destination it authorizes"
        );
        fixture
            .signals
            .publish_participants()
            .await
            .expect("republishing the same cut is idempotent");
        settle_lifecycle().await;
        assert_eq!(
            fixture.transport.reserves().len(),
            2,
            "a repeated publish charges no participant a second time"
        );
    }

    /// A refusal after an acceptance returns exactly what was taken.
    ///
    /// # Panics
    ///
    /// Panics when the attempt is not refused, when a release is inexact, when
    /// a partial cut is published, or when acknowledged cleanup is retained.
    async fn assert_partial_reservation_releases_exactly(expires_at: DateTime<Utc>) {
        let refused = ReservationFixture::start(1, expires_at);
        let error = refused
            .signals
            .publish_participants()
            .await
            .expect_err("a declined participant fails the attempt");
        assert!(
            matches!(error, BifrostError::QueryAdmissionRejected),
            "a declined participant is an admission refusal: {error:?}"
        );
        assert_eq!(
            refused.transport.releases(),
            vec![(
                refused.remote[0].1.node_id,
                Uuid::from_u128(200).to_string()
            )],
            "exactly the one accepted reservation is returned"
        );
        assert!(
            refused.signals.participants().get().is_none(),
            "a partial reservation publishes no cut, so nothing may be dispatched"
        );
        settle_lifecycle().await;
        assert!(
            refused.graph.execution_handle().is_healthy(),
            "an acknowledged release leaves no retained cleanup"
        );
    }

    /// An unacknowledged release keeps the graph draining until it is answered.
    ///
    /// # Panics
    ///
    /// Panics when an ambiguous release is forgotten, when the node advertises
    /// readiness while retaining one, or when an acknowledgement does not clear it.
    async fn assert_ambiguous_release_retains_until_acknowledged(expires_at: DateTime<Utc>) {
        let ambiguous = ReservationFixture::start(1, expires_at);
        ambiguous
            .transport
            .release_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            ambiguous.signals.publish_participants().await.is_err(),
            "the declined participant still fails the attempt"
        );
        settle_lifecycle().await;
        assert_eq!(
            ambiguous.draining(),
            1,
            "an unacknowledged release keeps the graph supervisor-visible"
        );
        assert!(
            !ambiguous.graph.execution_handle().is_healthy(),
            "a node retaining a follower's envelope does not advertise readiness"
        );
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        assert_eq!(
            ambiguous.draining(),
            1,
            "the follower-stated expiry has not passed, so the record is retained"
        );
        ambiguous
            .transport
            .release_fails
            .store(false, std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        settle_lifecycle().await;
        assert_eq!(
            ambiguous.draining(),
            0,
            "an acknowledgement clears the retained release immediately"
        );
    }

    /// With no acknowledgement, only both bounds together free the envelope.
    ///
    /// # Panics
    ///
    /// Panics when an elapsed wall clock alone frees the record, or when the
    /// record survives both the stated expiry and a full local pending TTL.
    async fn assert_expiry_needs_both_clocks() {
        let expired = ReservationFixture::start(1, Utc::now() - chrono::Duration::seconds(1));
        expired
            .transport
            .release_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            expired.signals.publish_participants().await.is_err(),
            "the declined participant still fails the attempt"
        );
        settle_lifecycle().await;
        assert_eq!(
            expired.draining(),
            1,
            "an already-elapsed wall clock alone does not free the envelope"
        );
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        settle_lifecycle().await;
        assert_eq!(
            expired.draining(),
            0,
            "both the stated expiry and a full local pending TTL free the envelope"
        );
    }

    /// The participant cut is reserved exactly once, immediately before dispatch.
    ///
    /// Four orderings are load-bearing and none is observable from the
    /// reservation count alone. Nothing is reserved until selection is final.
    /// The graph-owned cut cell stays unset until *every* participant has
    /// accepted, so no channel resolves against a partial cut and none is dialed
    /// while it is unset. A refusal after earlier acceptances returns exactly
    /// the reservations that were taken and publishes nothing. And a release
    /// whose acknowledgement never arrived keeps the graph draining until either
    /// the follower answers or both the follower-stated expiry and a full local
    /// pending TTL have passed.
    ///
    /// # Panics
    ///
    /// Panics when a reservation is taken early, taken twice, published
    /// partially, released inexactly, or forgotten before it can be proven gone.
    #[tokio::test(start_paused = true)]
    async fn participant_cut_is_reserved_once_immediately_before_dispatch() {
        let expires_at = Utc::now() + chrono::Duration::seconds(60);
        // The pending bound the leader retains is the dispatcher's own, not a
        // second copy of the same duration.
        assert_eq!(
            super::super::dispatcher::PENDING_TTL,
            chrono::Duration::seconds(2),
            "the retained-release bound is the canonical two-second pending TTL"
        );
        let selected = ReservationFixture::start(2, expires_at);
        assert_nothing_reserved_before_selection(&selected).await;
        assert_reserved_once_and_published(&selected).await;
        drop(selected);
        assert_partial_reservation_releases_exactly(expires_at).await;
        assert_ambiguous_release_retains_until_acknowledged(expires_at).await;
        assert_expiry_needs_both_clocks().await;
    }

    /// Builds one frozen destination distinguishable by node identity.
    fn frozen_destination(endpoint: &str) -> super::super::dispatcher::DispatchCandidate {
        super::super::dispatcher::DispatchCandidate {
            node_id: NodeId::new(uuid::Uuid::now_v7()),
            role: wyrd_spec::vala::api::ClusterRole::Oracle,
            worker_fence: 7,
            endpoint: Some(endpoint.to_owned()),
        }
    }

    /// Builds one destination-bound remote placeholder over a one-column schema.
    fn bound_placeholder(
        scan_id: &str,
        destination: &super::super::dispatcher::DispatchCandidate,
    ) -> super::super::codec::RemoteSourcePlaceholderExec {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        super::super::codec::RemoteSourcePlaceholderExec::new(scan_id, "fingerprint", schema)
            .with_source(super::super::codec::PlannedRemoteSource {
                destination: destination.clone(),
                tenant: wyrd_spec::DataTenantId::new_v7(),
                table: "vala.traces.spans".to_owned(),
                tier: super::super::RemotePersistedTier::Iceberg,
            })
    }

    /// Builds the signed assignment a bound placeholder resolves through, with
    /// distinguishable files so a task's share is identifiable by path.
    fn split_fixture_assignment(
        scan_id: &str,
        files: usize,
    ) -> wyrd_spec::vala::api::FollowerScanAssignment {
        wyrd_spec::vala::api::FollowerScanAssignment {
            scan_id: scan_id.to_owned(),
            binding: wyrd_spec::vala::api::TenantTableBinding {
                tenant_id: wyrd_spec::DataTenantId::new_v7(),
                namespace: "traces".to_owned(),
                table: "spans".to_owned(),
            },
            persisted: wyrd_spec::vala::api::PersistedFileAssignment {
                files: (0..files)
                    .map(|index| {
                        super::super::test_persisted_descriptor(&format!("memory:///f{index}"))
                    })
                    .collect(),
            },
            scribe_provider_cut: None,
            schema_fingerprint: "fingerprint".to_owned(),
            required_columns: vec!["value".to_owned()],
            predicates: Vec::new(),
        }
    }

    /// A substituted leaf receives the cut's budget and is never forced past it.
    ///
    /// The placeholder can read its own local plan, so a stage the planner
    /// leaves on the leader is correct rather than unreadable. Forcing a
    /// minimum here would inject a boundary above every scannable cut and
    /// distribute queries the leader could answer alone.
    async fn assert_leaf_holds_exactly_the_cut_budget(
        leaf: &Arc<dyn ExecutionPlan>,
        budget: usize,
        expected: usize,
    ) {
        // `budget` stands for both participants and scannable units; the count
        // is their minimum, so passing it twice names the cut's real ceiling.
        let response = datafusion_distributed::DesiredTaskCountHandler::handle(
            &AnalyticalCutTaskCount::new(budget, budget),
            datafusion_distributed::DesiredTaskCountEvent {
                plan: leaf,
                session_config: &datafusion::prelude::SessionConfig::new(),
            },
        )
        .await
        .expect("a leaf node is always answered")
        .expect("the cut count never fails");
        assert!(
            matches!(
                response.task_count,
                datafusion_distributed::TaskCountAnnotation::Desired(tasks) if tasks == expected
            ),
            "a leaf must hold exactly {expected} tasks, saw {:?}",
            response.task_count
        );
    }

    /// Only the frozen participant receives a remote-bearing stage, and a
    /// contradictory or unauthorized stage fails before any task is submitted.
    fn assert_stage_routes_only_to_frozen_destination(
        leaf: &Arc<dyn ExecutionPlan>,
        destination: &super::super::dispatcher::DispatchCandidate,
        url: &Url,
    ) {
        let stage: Arc<dyn ExecutionPlan> = Arc::new(
            datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec::new(
                Arc::clone(leaf),
            ),
        );
        let router = OracleRouteTasks {
            destinations: vec![(url.clone(), destination.clone())],
        };
        let task_ctx = Arc::new(datafusion::execution::TaskContext::default());
        let assigned = datafusion_distributed::RouteTasksHandler::handle(
            &router,
            datafusion_distributed::RouteTasksEvent {
                task_ctx: Arc::clone(&task_ctx),
                plan: &stage,
                task_count: 2,
            },
        )
        .expect("a remote-bearing stage is routed")
        .expect("an in-roster destination routes");
        assert_eq!(assigned.urls, vec![url.clone(), url.clone()]);

        // A stage with no remote leaf reads only through exchanges. It is
        // spread over the frozen roster in the roster's own order rather than
        // deferred: upstream answers an unanswered event with a random
        // assignment, which can route one task's plan push and its execution to
        // different peers and leave the executing worker waiting for a plan
        // that was written elsewhere.
        let local: Arc<dyn ExecutionPlan> = Arc::new(
            datafusion::physical_plan::empty::EmptyExec::new(leaf.schema()),
        );
        let consumer = datafusion_distributed::RouteTasksHandler::handle(
            &router,
            datafusion_distributed::RouteTasksEvent {
                task_ctx: Arc::clone(&task_ctx),
                plan: &local,
                task_count: 2,
            },
        )
        .expect("a consumer stage is placed on the frozen roster")
        .expect("the frozen roster places every task");
        assert_eq!(consumer.urls, vec![url.clone(), url.clone()]);

        // Only an empty roster leaves a stage to the ordinary bounded pool.
        assert!(
            datafusion_distributed::RouteTasksHandler::handle(
                &OracleRouteTasks {
                    destinations: Vec::new(),
                },
                datafusion_distributed::RouteTasksEvent {
                    task_ctx: Arc::clone(&task_ctx),
                    plan: &local,
                    task_count: 1,
                },
            )
            .is_none(),
            "a stage with no frozen roster defers to the bounded worker pool"
        );

        // Two distinct destinations in one stage fail before dispatch.
        let other = frozen_destination("http://peer-b:9000/");
        let conflicted = datafusion::physical_plan::union::UnionExec::try_new(vec![
            Arc::clone(leaf),
            Arc::new(bound_placeholder("oracle:spans:persisted", &other)) as Arc<dyn ExecutionPlan>,
        ])
        .expect("two identical schemas union");
        assert!(
            datafusion_distributed::RouteTasksHandler::handle(
                &router,
                datafusion_distributed::RouteTasksEvent {
                    task_ctx: Arc::clone(&task_ctx),
                    plan: &conflicted,
                    task_count: 2,
                },
            )
            .expect("a remote-bearing stage is answered")
            .is_err(),
            "a stage naming two frozen destinations must fail before dispatch"
        );

        // A destination outside the frozen roster is refused the same way.
        let stranger: Arc<dyn ExecutionPlan> =
            Arc::new(bound_placeholder("oracle:spans:persisted", &other));
        assert!(
            datafusion_distributed::RouteTasksHandler::handle(
                &router,
                datafusion_distributed::RouteTasksEvent {
                    task_ctx: Arc::clone(&task_ctx),
                    plan: &stranger,
                    task_count: 1,
                },
            )
            .expect("a remote-bearing stage is answered")
            .is_err(),
            "a destination outside the frozen roster must fail before dispatch"
        );
    }

    /// The stage's tasks divide the one bound source instead of each reading
    /// all of it, and a live Scribe source resolves exactly once.
    fn assert_task_variants_divide_the_bound_source(
        placeholder: &super::super::codec::RemoteSourcePlaceholderExec,
        destination: &super::super::dispatcher::DispatchCandidate,
    ) {
        let assignment = split_fixture_assignment("oracle:spans:persisted", 5);
        let mut seen = Vec::new();
        for task in 0..2_usize {
            let variant = placeholder.clone().with_task_share(task, 2);
            seen.extend(
                variant
                    .narrow(assignment.clone())
                    .persisted
                    .files
                    .iter()
                    .map(|file| file.path().to_owned()),
            );
        }
        seen.sort();
        assert_eq!(
            seen,
            vec![
                "memory:///f0".to_owned(),
                "memory:///f1".to_owned(),
                "memory:///f2".to_owned(),
                "memory:///f3".to_owned(),
                "memory:///f4".to_owned(),
            ],
            "every signed file belongs to exactly one task variant"
        );

        let scribe = bound_placeholder("oracle:spans:scribe:node:1:live", destination);
        let variants = AnalyticalLeafSplit::variants(&scribe, 2);
        assert_eq!(variants.len(), 2, "one variant per stage task");
        assert!(
            variants[0]
                .downcast_ref::<super::super::codec::RemoteSourcePlaceholderExec>()
                .is_some(),
            "task index zero retains the live Scribe source"
        );
        assert!(
            variants[1]
                .downcast_ref::<datafusion::physical_plan::empty::EmptyExec>()
                .is_some(),
            "only task index zero resolves a live Scribe source"
        );
        assert!(
            AnalyticalLeafSplit::variants(placeholder, 2)
                .iter()
                .all(|variant| variant
                    .downcast_ref::<super::super::codec::RemoteSourcePlaceholderExec>()
                    .is_some()),
            "a file-backed leaf keeps one share-carrying placeholder per task"
        );
    }

    /// A single-partition remote leaf keeps a forced, destination-bound stage.
    ///
    /// These properties are proved together because they are one contract: a
    /// leaf the leader cannot read must not collapse back onto the leader, its
    /// stage must route only to the participant the roster froze for it, a
    /// contradictory or unauthorized stage must fail before any task is
    /// submitted, and the stage's tasks must divide the one bound source
    /// rather than each reading all of it.
    #[tokio::test]
    async fn single_partition_remote_leaf_keeps_destination_bound_stage() {
        let destination = frozen_destination("http://peer-a:9000/");
        let url = Url::parse("http://peer-a:9000/").expect("frozen endpoint parses");
        let placeholder = bound_placeholder("oracle:spans:persisted", &destination);
        let leaf: Arc<dyn ExecutionPlan> = Arc::new(placeholder.clone());

        assert_leaf_holds_exactly_the_cut_budget(&leaf, 3, 3).await;
        // A leader-owned leaf beside it stays single-task, so its stage can
        // never be dispatched to a peer whose codec could not decode it.
        let leader_owned: Arc<dyn ExecutionPlan> = Arc::new(
            datafusion::physical_plan::empty::EmptyExec::new(placeholder.schema()),
        );
        assert_leaf_holds_exactly_the_cut_budget(&leader_owned, 3, 1).await;
        assert_stage_routes_only_to_frozen_destination(&leaf, &destination, &url);
        assert_task_variants_divide_the_bound_source(&placeholder, &destination);
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
/// The handler answers for leaf nodes and for the hash repartition that opens a
/// shuffle. Those are the two places a task count is genuinely decided: a scan
/// cannot usefully exceed its file count, and an aggregate cannot usefully
/// exceed the participants frozen for the attempt. Every other node reconciles
/// from its children. Registered as a *custom* handler, it is consulted before
/// the built-in byte estimator and therefore replaces it.
struct AnalyticalCutTaskCount {
    /// Tasks one scan stage of this attempt may occupy, at least one.
    tasks: usize,
    /// Participants frozen for this attempt, the ceiling for a shuffled stage.
    participants: usize,
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
            participants: participants.max(1),
        }
    }
}

/// Routes every task of one isolated stage to the participant frozen for its
/// remote leaves.
///
/// This is the sole destination validator and router for Oracle sources. It
/// runs at the actual routing boundary, while the coordinator prepares worker
/// placement and before any task is submitted or any source IO happens, so a
/// stage that names an unauthorized or contradictory destination fails before
/// a follower is ever contacted. Stages the planner created for its own
/// join, aggregate, or shuffle boundaries carry no remote leaf and are deferred
/// to the ordinary bounded worker pool.
struct OracleRouteTasks {
    /// The complete frozen roster, in the same order the planner sees it.
    ///
    /// Membership here is authorization: a leaf naming a peer absent from this
    /// set was not admitted by the pinned participant cut.
    destinations: Vec<(Url, super::dispatcher::DispatchCandidate)>,
}

impl OracleRouteTasks {
    /// Collects every remote leaf destination reachable from one stage head.
    ///
    /// Called only from [`RouteTasksHandler::handle`]; the same traversal must
    /// not run at any other call site, because a check that happens before the
    /// routing boundary can be invalidated by the coordinator's own placement.
    /// The walk itself is [`super::remote_placeholders`], the one the binder
    /// also uses, so a plan shape either side cannot see is a single defect
    /// rather than two divergent ones.
    fn stage_destinations(
        plan: &Arc<dyn ExecutionPlan>,
    ) -> Vec<super::dispatcher::DispatchCandidate> {
        super::remote_placeholders(plan.as_ref())
            .iter()
            .filter_map(|placeholder| placeholder.destination().cloned())
            .collect()
    }
}

impl datafusion_distributed::RouteTasksHandler for OracleRouteTasks {
    /// Assigns every task slot of a remote-bearing stage to its frozen peer.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error, before task submission, when a
    /// stage names more than one distinct destination or names a peer that is
    /// not in the frozen roster. Both mean the plan would read a source through
    /// a participant the cut never authorized.
    fn handle(
        &self,
        ev: datafusion_distributed::RouteTasksEvent<'_>,
    ) -> Option<Result<datafusion_distributed::RouteTasksEventResponse, DataFusionError>> {
        let destinations = Self::stage_destinations(ev.plan);
        // A stage that names no frozen source is a consumer stage: it reads
        // only through exchanges. Upstream's fallback for an unanswered event
        // is a random assignment across the worker set, which routes a stage's
        // plan push and its task execution to different peers and leaves the
        // executing worker waiting for a plan that was written elsewhere.
        // Spreading the tasks over the frozen roster in its own canonical order
        // keeps every task on an authorized participant and keeps the two
        // halves of one task on the same one.
        let Some(first) = destinations.first().cloned() else {
            if self.destinations.is_empty() {
                return None;
            }
            return Some(Ok(datafusion_distributed::RouteTasksEventResponse::new(
                (0..ev.task_count)
                    .map(|task| self.destinations[task % self.destinations.len()].0.clone())
                    .collect(),
            )));
        };
        if destinations.iter().any(|candidate| *candidate != first) {
            return Some(Err(DataFusionError::Execution(
                "Oracle stage names more than one frozen destination".to_owned(),
            )));
        }
        let Some((url, _)) = self
            .destinations
            .iter()
            .find(|(_, candidate)| *candidate == first)
        else {
            return Some(Err(DataFusionError::Execution(
                "Oracle stage names a destination outside the frozen roster".to_owned(),
            )));
        };
        Some(Ok(datafusion_distributed::RouteTasksEventResponse::new(
            vec![url.clone(); ev.task_count],
        )))
    }
}

/// Splits one Analytical leaf's signed files across its stage's final tasks.
///
/// Without this, every task of a leaf stage decodes the same plan and therefore
/// the same complete assignment, so a fixture with one table read once per task
/// returns each row `task_count` times. Upstream calls this after a stage's task
/// count is final, which is the only point at which the split is knowable.
struct AnalyticalLeafSplit;

impl AnalyticalLeafSplit {
    /// Builds one leaf variant per final stage task.
    ///
    /// A file-backed leaf yields one share-carrying placeholder per task. A
    /// Scribe leaf yields its placeholder at task index zero and a native
    /// schema-compatible `EmptyExec` everywhere else, because one live memtable
    /// cut resolves exactly once and cannot be divided.
    fn variants(
        placeholder: &super::codec::RemoteSourcePlaceholderExec,
        tasks: usize,
    ) -> Vec<Arc<dyn ExecutionPlan>> {
        let scribe = placeholder.is_scribe();
        (0..tasks)
            .map(|task| {
                if scribe && task > 0 {
                    return Arc::new(datafusion::physical_plan::empty::EmptyExec::new(
                        placeholder.schema(),
                    )) as Arc<dyn ExecutionPlan>;
                }
                Arc::new(placeholder.clone().with_task_share(task, tasks)) as Arc<dyn ExecutionPlan>
            })
            .collect()
    }
}

impl datafusion_distributed::ScaleUpLeafNodeHandler for AnalyticalLeafSplit {
    /// Replaces a remote source placeholder with one variant per stage task.
    ///
    /// The variants share schema and partition count — upstream requires both —
    /// and differ only in the share of the bound source each names. The share
    /// is recorded here as an index and a count rather than applied to files,
    /// because the files do not exist yet: a planning placeholder carries no
    /// assignment, and the codec narrows the bound assignment by this same
    /// share when the variant is encoded. A leaf whose files do not divide
    /// leaves empty slices on the trailing tasks rather than a wider share on
    /// any of them.
    ///
    /// A Scribe leaf resolves one live memtable cut and cannot be divided at
    /// all, so task index zero alone retains the placeholder and every other
    /// task becomes a schema-compatible native `EmptyExec`. That is what keeps
    /// a live tail from being read once per task.
    fn handle(
        &self,
        ev: datafusion_distributed::ScaleUpLeafNodeEvent<'_>,
    ) -> Option<Result<datafusion_distributed::ScaleUpLeafNodeEventResponse, DataFusionError>> {
        let placeholder = ev
            .plan
            .downcast_ref::<super::codec::RemoteSourcePlaceholderExec>()?;
        let variants = Self::variants(placeholder, ev.task_count.max(1));
        Some(
            datafusion_distributed::DistributedLeafExec::try_new(Arc::clone(ev.plan), variants)
                .map(|exec| {
                    datafusion_distributed::ScaleUpLeafNodeEventResponse::new(
                        Arc::new(exec) as Arc<dyn ExecutionPlan>
                    )
                }),
        )
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
            // A hash repartition is the head of a shuffle, and the stage it
            // opens is bounded by participants rather than by files. Scan
            // parallelism cannot exceed the number of objects to open, but a
            // grouped aggregate spreads by hash of its key, so a cut holding
            // one file can still occupy every frozen peer above the shuffle.
            // Without this the whole plan inherits the leaf's single task and
            // the pinned revision elides the boundary, collapsing a genuinely
            // distributable aggregate back onto the leader.
            return ev
                .plan
                .downcast_ref::<datafusion::physical_plan::repartition::RepartitionExec>()
                .filter(|repartition| {
                    matches!(
                        repartition.partitioning(),
                        datafusion::physical_plan::Partitioning::Hash(_, _)
                    )
                })
                .map(|_| {
                    Ok(
                        datafusion_distributed::DesiredTaskCountEventResponse::desired(
                            self.participants,
                        ),
                    )
                });
        }
        // Only a substituted source may occupy more than one task. Every other
        // leaf — the drained local live tail above all — is leader-owned and
        // carries no wire encoding, so distributing its stage would ask the
        // codec to serialize a plan that exists only on this node.
        //
        // A substituted leaf gets the cut's own budget and nothing more. It is
        // deliberately not forced past one task: the placeholder can read its
        // local plan here, so a stage the pinned revision chooses to leave on
        // the leader is correct rather than unreadable, and forcing a minimum
        // would distribute a query the leader could have answered alone.
        let tasks = match ev
            .plan
            .downcast_ref::<super::codec::RemoteSourcePlaceholderExec>()
        {
            Some(_) => self.tasks,
            None => 1,
        };
        Some(Ok(
            datafusion_distributed::DesiredTaskCountEventResponse::desired(tasks),
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
    /// Scratch child every attempt of a graph charges on this node.
    pub scratch_bytes: u64,
    /// Immutable peer identity every leader-side channel is dialed through.
    pub peer_tls: BifrostPeerTls,
    /// Workload credential every leader-side peer request presents.
    pub peer_credentials: Arc<dyn OraclePeerCredentials>,
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
    /// Graphs retained because their cleanup did not complete.
    ///
    /// A non-zero count is a real leak this node still owns and can name, not a
    /// transient teardown, so it fails readiness rather than being logged away.
    pub cleanup_failures: usize,
}

impl AnalyticalLiveOwnership {
    /// Reports whether this half of the node retains nothing.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.attempts == 0 && self.graphs == 0 && self.cleanup_failures == 0
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
    /// Directory this leader reserves each graph participant's envelope through.
    ///
    /// Absent only where no peer transport was composed, which is a node that
    /// cannot address a participant at all; a leader without it can execute
    /// nothing remote and refuses rather than freezing an unreserved cut.
    peer_transports: Option<Arc<super::dispatcher::OraclePeerTransportDirectory>>,
    /// Node-scoped identity, budget, and peer-transport configuration.
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

/// The node-scoped owners one Analytical handle is composed from.
///
/// Named together because they are exactly the set a node already has when it
/// composes its Analytical half, and naming them keeps two same-typed shared
/// owners from being transposable at the call site.
pub struct AnalyticalExecutionOwners {
    /// This node's follower ingress, which also hosts the upstream worker.
    pub worker: Arc<AnalyticalStageIngress>,
    /// Server-owned authority every stage operation is signed and checked by.
    pub authority: Arc<dyn OracleStageAuthority>,
    /// Node-local supervisor owning graphs, attempts, and the runtime registry.
    pub supervisor: Arc<AnalyticalSupervisor>,
    /// Process spill owner that bounds each query runtime's disk manager.
    pub spill: Arc<OracleSpillRuntime>,
    /// Peer transports the leader reserves participant capacity through.
    pub peer_transports: Option<Arc<super::dispatcher::OraclePeerTransportDirectory>>,
}

impl fmt::Debug for AnalyticalExecutionOwners {
    /// Reports presence without rendering owned dependencies.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalExecutionOwners")
            .field("peer_transports", &self.peer_transports.is_some())
            .finish_non_exhaustive()
    }
}

impl AnalyticalExecutionHandle {
    /// Composes the handle over this node's existing Analytical owners.
    #[must_use]
    pub fn new(
        owners: AnalyticalExecutionOwners,
        config: AnalyticalExecutionConfig,
        leaf: super::codec::AnalyticalLeafBinding,
    ) -> Self {
        let AnalyticalExecutionOwners {
            worker,
            authority,
            supervisor,
            spill,
            peer_transports,
        } = owners;
        Self {
            worker,
            authority,
            supervisor,
            spill,
            peer_transports,
            config,
            leaf,
        }
    }

    /// Reports whether this node can still admit Analytical work.
    ///
    /// Two conditions, both read from owners that already exist. The supervisor
    /// must still be serviceable, and the follower must not be retaining a graph
    /// whose cleanup failed: that graph keeps a supervisor guard, a reservation
    /// residue, and a charged envelope this node can name but cannot return, so
    /// continuing to advertise readiness would send new work to a node that has
    /// already stranded some. Live, healthy graphs and attempts are deliberately
    /// not consulted — ordinary service would otherwise fail readiness — and an
    /// unreadable ownership lock fails closed for the same reason.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.supervisor.is_healthy()
            && self
                .supervisor
                .draining_graphs()
                .is_ok_and(|draining| draining == 0)
            && self
                .worker
                .live()
                .is_ok_and(|follower| follower.cleanup_failures == 0)
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
                // Leader-side cleanup is joined inline by the attempt owner, so
                // there is no retained-failure state on this half to report.
                cleanup_failures: 0,
            },
            follower: self.worker.live()?,
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
    /// The leader admits nothing here. The query envelope this attempt already
    /// holds is *moved* out of `admitted` and into the supervisor graph, which
    /// owns it until the graph is released. One query therefore charges one
    /// leader envelope and installs one `DataFusion` pool, and every operator
    /// and exchange on this node allocates from that same pool.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when `admitted` holds
    /// no live envelope to transfer, [`BifrostError::Internal`] when the
    /// supervisor is shutting down or a participant endpoint is not a valid
    /// URL, and [`BifrostError::QueryExecutionFailed`] when `DataFusion` cannot
    /// build the bounded query runtime.
    pub(super) fn lease_session(
        &self,
        inputs: AnalyticalLeaseInputs<'_>,
    ) -> Result<(SessionContext, AnalyticalAttemptOwnership), BifrostError> {
        let AnalyticalLeaseInputs {
            attempt,
            cut,
            context,
            admitted,
            work_units,
            config,
            deadline,
        } = inputs;
        let graph = AnalyticalGraphKey::new(attempt.public_query_id, attempt.datafusion_query_id);
        // Read from the immutable cut, not from a reservation: node identity,
        // endpoint, and fence are all frozen before anything is reserved, so
        // planning has everything it needs while the followers are still
        // uncharged. Nothing here issues an RPC.
        let remote = self.remote_participants(cut.oracles())?;
        // Transferred, never re-acquired: this query's envelope moves from the
        // admission guard into the graph, so nothing downstream can charge the
        // process governor a second time for the same query.
        let resources = admitted
            .take_query_resources()
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let granted_memory_bytes = resources.granted_memory_bytes;
        let target_partitions = resources.target_partitions;
        let runtime = self
            .spill
            .build_query_runtime(resources.memory_pool(), resources.scratch_bytes)?;
        let graph_guard = match self.supervisor.register_graph(
            graph,
            resources,
            AnalyticalGraphRuntime::new(
                runtime,
                crate::resources::OracleSessionShape::for_grant(
                    granted_memory_bytes,
                    target_partitions,
                    target_partitions,
                ),
            ),
        ) {
            Ok(guard) => guard,
            // The refusal hands the envelope straight back rather than
            // consuming it, so it is restored to the same permit it was taken
            // from. Dropping it here would strand this query's whole grant on
            // a node that never registered a graph to release it.
            Err((returned, error)) => {
                admitted.restore_query_resources(*returned);
                return Err(error);
            }
        };
        let attempt_guard = self.supervisor.spawn_attempt(
            AnalyticalAttemptKey::new(
                attempt.public_query_id,
                attempt.datafusion_query_id,
                StageId::new(0),
                None,
                AnalyticalAttemptNumber::ZERO,
            ),
            AnalyticalAttemptGrant {
                scratch_bytes: self.config.scratch_bytes,
            },
        )?;
        let key = attempt_guard.key();
        let cancel = attempt_guard.cancellation().clone();
        let egressed = attempt_guard.egress_flag();
        let signals = AnalyticalGraphLifecycle::start(
            graph,
            Arc::clone(&self.supervisor),
            self.peer_transports.clone(),
            remote.clone(),
            ReserveNodeSlotsRequest {
                query_id: QueryId::new(graph.public_query_id.as_uuid()),
                leader_node_id: self.config.node_id,
                leader_fencing_token: self.config.oracle_fence,
                query_class: QueryClass::Analytical,
                slot_units: ANALYTICAL_GRAPH_SLOT_UNITS,
                expires_at: cut.deadline(),
                graph: Some(AnalyticalGraphRef {
                    public_query_id: graph.public_query_id.as_uuid(),
                    datafusion_query_id: graph.datafusion_query_id.as_uuid(),
                }),
            },
            deadline,
            AnalyticalGraphLifecycleOwners {
                attempt: Some(attempt_guard),
                graph_guard: Some(graph_guard),
            },
        )?;
        let session = self.leader_session(
            AnalyticalSessionInputs {
                context,
                snapshot_digest: &attempt.snapshot_digest,
                urls: remote.into_iter().map(|(url, _)| url).collect(),
                participants: signals.participants(),
                permission_digest: &attempt.permission_digest,
                config,
                work_units,
            },
            graph,
            cut.deadline().timestamp_millis(),
        )?;
        Ok((
            session,
            AnalyticalAttemptOwnership {
                key,
                cancel,
                egressed,
                signals,
            },
        ))
    }

    /// Composes the planning-only session the pinned distributed build runs in.
    ///
    /// This session exists so a candidate plan can be *proposed* before this
    /// query owns anything Analytical. It reuses the admitted query's own
    /// state — catalog, registered providers, runtime, and grant-derived
    /// config — and adds exactly what the pinned planner consults while it
    /// transforms: the frozen worker set, the cut-derived task count, the leaf
    /// split, and this node's codec. It deliberately installs no channel
    /// resolver: channels are resolved from the execution `TaskContext`, which
    /// only the authoritative leader session supplies, so nothing built here
    /// can dial a peer.
    ///
    /// Composing it performs no supervisor mutation, no resource transfer, no
    /// participant publication, and no IO.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a participant endpoint in the
    /// frozen cut is not a valid URL.
    pub(super) fn planning_session(
        &self,
        local: &SessionContext,
        oracles: &[super::participant_cut::OracleQueryParticipant],
        work_units: usize,
    ) -> Result<SessionContext, BifrostError> {
        let destinations = self.remote_participants(oracles)?;
        let urls = destinations
            .iter()
            .map(|(url, _)| url.clone())
            .collect::<Vec<_>>();
        let mut config = local.copied_config();
        config.set_distributed_desired_task_count_handler(AnalyticalCutTaskCount::new(
            urls.len(),
            work_units,
        ));
        config.set_distributed_scale_up_leaf_node_handler(AnalyticalLeafSplit);
        config.set_distributed_worker_resolver(AnalyticalWorkerResolver { urls });
        config.set_distributed_route_tasks_handler(OracleRouteTasks { destinations });
        // Bifrost's scan is one union of this table's leader-owned source kinds
        // — published Iceberg, leader hot files, and the drained local live
        // tail. Upstream's isolator would give that union one task per child
        // and therefore place a plan the leader alone can read below a network
        // boundary, where the codec is asked to serialize leaves that exist
        // only on this node.
        config
            .set_distributed_children_isolator_unions(false)
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        let mut codec = super::codec::OraclePhysicalExtensionCodec::analytical(self.leaf.clone());
        if let Some(bindings) = config.get_extension::<super::bindings::OracleExecutionLock>() {
            codec = codec.with_bindings(bindings);
        }
        config.set_distributed_user_codec(codec);
        let state = datafusion::execution::session_state::SessionStateBuilder::new_from_existing(
            local.state(),
        )
        .with_config(config)
        .with_distributed_planner()
        .build();
        Ok(SessionContext::new_with_state(state))
    }

    /// Moves one query's running-registry owner onto its already-registered graph.
    ///
    /// # Errors
    ///
    /// Returns `owner` unchanged when the graph cannot accept it, so a refused
    /// transfer leaves the caller holding the only thing that can retire the
    /// public entry.
    pub(super) fn retain_running_query(
        &self,
        graph: AnalyticalGraphKey,
        owner: super::query_stream::RunningQueryTerminalOwner,
    ) -> Result<(), Box<super::query_stream::RunningQueryTerminalOwner>> {
        self.supervisor.retain_running_query(graph, owner)
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
        deadline_ms: i64,
    ) -> Result<SessionContext, BifrostError> {
        let AnalyticalSessionInputs {
            context,
            snapshot_digest,
            urls,
            participants,
            permission_digest,
            mut config,
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
            // Placeholder only. The resolver substitutes each destination's own
            // frozen reservation before any channel to it is built, because a
            // follower honours only the reservation it granted itself.
            reservation_id: String::new(),
            permission_digest: permission_digest.to_owned(),
        });
        // The cell, not a cut: the frozen set is published into it once every
        // participant has accepted its reservation, which is after this session
        // is built. Everything downstream — this leader's own channels and every
        // follower that becomes a coordinator beneath it — then addresses that
        // exact set, so no membership change can add, remove, or re-fence a
        // destination mid-attempt, and no channel resolves before it exists.
        let resolver = AnalyticalChannelResolver::new(
            identity,
            self.config.peer_tls.clone(),
            Arc::clone(&self.config.peer_credentials),
            participants,
            self.supervisor.graph_exchanges(graph)?.unwrap_or_default(),
            AnalyticalStageSigning {
                authority: Arc::clone(&self.authority),
                absolute_deadline_ms: deadline_ms,
                ticket_ttl: self.config.ticket_ttl,
            },
        );
        config.set_distributed_desired_task_count_handler(AnalyticalCutTaskCount::new(
            urls.len(),
            work_units,
        ));
        config.set_distributed_scale_up_leaf_node_handler(AnalyticalLeafSplit);
        config.set_distributed_worker_resolver(AnalyticalWorkerResolver { urls });
        config.set_distributed_channel_resolver(resolver);
        // No codec is installed here. `config` is the planning session's own
        // copied configuration, which already carries this node's Oracle codec
        // — bound to the execution lock, which this one would not be. Upstream
        // stores user codecs as an ordered list and encodes the position of the
        // codec that matched, so a second entry shifts every position past the
        // one a follower builds from its own single install, and the follower
        // then refuses the plan with "Can't find required codec in codec list".
        let state = datafusion::execution::session_state::SessionStateBuilder::new()
            .with_default_features()
            .with_config(config)
            .with_runtime_env(Arc::clone(runtime.runtime()))
            .with_distributed_planner()
            .build();
        Ok(SessionContext::new_with_state(state))
    }

    /// Returns the frozen non-leader participants a cut may delegate a source to.
    ///
    /// This is the roster the planner freezes destinations from, in cut order.
    /// It is deliberately distinct from [`AnalyticalWorkerResolver`]'s
    /// undifferentiated pool: a source placeholder names exactly one of these,
    /// and the route handler refuses a stage that names anything else.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a frozen participant endpoint is
    /// not a valid URL.
    pub(super) fn frozen_destinations(
        &self,
        oracles: &[super::participant_cut::OracleQueryParticipant],
    ) -> Result<Vec<super::dispatcher::DispatchCandidate>, BifrostError> {
        Ok(self
            .remote_participants(oracles)?
            .into_iter()
            .map(|(_, candidate)| candidate)
            .collect())
    }

    /// Projects the remote participants this attempt may address, without IO.
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
    /// Nothing is reserved here: identity, endpoint, and fence are already
    /// immutable in the cut, which is what lets planning run to completion
    /// before any follower is charged.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a participant endpoint is not a
    /// valid URL, which would otherwise leave a worker unreachable and unsigned.
    fn remote_participants(
        &self,
        oracles: &[super::participant_cut::OracleQueryParticipant],
    ) -> Result<Vec<(Url, super::dispatcher::DispatchCandidate)>, BifrostError> {
        oracles
            .iter()
            .filter(|participant| participant.node_id != self.config.node_id)
            .map(|participant| {
                let url =
                    Url::parse(&participant.endpoint).map_err(|error| BifrostError::Internal {
                        detail: format!(
                            "Oracle analytical participant endpoint is not a valid URL: {error}"
                        ),
                    })?;
                Ok((
                    url,
                    super::dispatcher::DispatchCandidate {
                        node_id: participant.node_id,
                        role: wyrd_spec::vala::api::ClusterRole::Oracle,
                        worker_fence: participant.fencing_token,
                        endpoint: Some(participant.endpoint.clone()),
                    },
                ))
            })
            .collect()
    }
}

/// Everything one admitted attempt needs to lease its distributed session.
///
/// The values are fixed before the leader's envelope moves onto the graph
/// supervisor, so they travel as one group rather than as seven parameters that
/// a caller could reorder.
pub(super) struct AnalyticalLeaseInputs<'a> {
    /// Public and `DataFusion` query identities for this attempt.
    pub(super) attempt: &'a AnalyticalAttemptContext,
    /// The immutable signed participant cut every stage reads.
    pub(super) cut: &'a OracleQueryAttemptCut,
    /// Authenticated principal, tenant, and audit correlation.
    pub(super) context: &'a AuthorizedQueryContext,
    /// Admission guard whose query envelope is transferred onto the graph.
    pub(super) admitted: &'a mut super::admission::AdmittedQueryGuard,
    /// Scannable work units the frozen cut selected.
    pub(super) work_units: usize,
    /// The exact `SessionConfig` the retained physical root was built with.
    pub(super) config: datafusion::prelude::SessionConfig,
    /// One absolute execution deadline shared by every stage.
    pub(super) deadline: tokio::time::Instant,
}

/// The leader-session inputs one attempt composes its distributed session from.
///
/// These are read from the request before its admitted envelope is moved onto
/// the supervisor, which is why they travel as a group rather than as a
/// borrowed request.
struct AnalyticalSessionInputs<'a> {
    /// Authenticated principal, tenant, and audit correlation.
    context: &'a AuthorizedQueryContext,
    /// Pinned snapshot digest of the attempt's cut.
    snapshot_digest: &'a str,
    /// Frozen participant endpoints this attempt plans across.
    ///
    /// Read from the immutable cut rather than from the reservation, because
    /// planning happens before anything is reserved.
    urls: Vec<Url>,
    /// Graph-owned cell every channel this attempt opens resolves its cut from.
    participants: Arc<std::sync::OnceLock<Arc<AnalyticalParticipantCut>>>,
    /// Digest of the leader-authorized permissions for this query.
    permission_digest: &'a str,
    /// The exact `SessionConfig` the retained root was planned with.
    ///
    /// Reused verbatim so admission supplies the runtime and pool only; the
    /// channel resolver is the one thing this session adds to it.
    config: datafusion::prelude::SessionConfig,
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
    /// The attempt every descendant of this ownership binds to.
    key: AnalyticalAttemptKey,
    /// Cancellation child covering every descendant of the attempt.
    cancel: CancellationToken,
    /// Egress fence shared with the supervisor's attempt state.
    ///
    /// Cloned from the attempt guard the lifecycle task now owns, because the
    /// fence outlives this caller: settlement evidence still has to say whether
    /// rows left the node.
    egressed: Arc<std::sync::atomic::AtomicBool>,
    /// The graph lifecycle task's caller-side half.
    ///
    /// The only handle a caller has on the graph. It carries no task and no
    /// guard: the supervisor owns those, so this value can ask for settlement
    /// and await it but can never perform it.
    pub signals: AnalyticalGraphSignals,
}

impl AnalyticalAttemptOwnership {
    /// Returns the exact attempt every descendant of this ownership binds to.
    #[must_use]
    pub const fn key(&self) -> AnalyticalAttemptKey {
        self.key
    }

    /// Returns the cancellation child covering every descendant of the attempt.
    #[must_use]
    pub const fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Records that result data produced under this attempt left the node.
    pub fn record_egress(&self) {
        self.egressed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Reports whether result data already left the node under this attempt.
    #[must_use]
    pub fn egressed(&self) -> bool {
        self.egressed.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Moves the admission owner into the graph that holds this query's envelope.
    ///
    /// Both seams use this: the inactive attempt, which holds no query stream,
    /// and the production stream at its terminal. Storing the permit on the
    /// graph is what makes admission follow ownership — a graph retained as
    /// `Draining` keeps the envelope *and* the counters charged, so a queued
    /// waiter is not handed capacity this node has not actually returned.
    ///
    /// The guard must already be cycle-free: take this ownership out of it
    /// first, or the graph would come to own a handle to itself.
    ///
    /// # Errors
    ///
    /// Returns the guard unchanged when the graph is gone or already retains a
    /// permit, leaving the caller responsible for releasing it.
    pub(super) fn retain_admission(
        &self,
        admitted: super::admission::AdmittedQueryGuard,
    ) -> Result<(), Box<super::admission::AdmittedQueryGuard>> {
        self.signals.retain_admission(admitted)
    }

    /// Hands this query's started plan and scan sink to its graph lifecycle.
    ///
    /// The fold has to outlive the result stream and be bounded by something,
    /// and the graph is the only owner that is both: it is still registered
    /// after the stream is dropped, and it already holds the query's absolute
    /// deadline.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the graph is no
    /// longer registered active or already retains a fold, and
    /// [`BifrostError::Internal`] when the graph lock is poisoned.
    pub(super) fn retain_metric_fold(
        &self,
        fold: AnalyticalGraphMetricFold,
    ) -> Result<(), BifrostError> {
        self.signals.retain_metric_fold(fold)
    }

    /// Reserves and publishes this graph's participant cut, once, before dispatch.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when a participant
    /// declined, and [`BifrostError::QueryExecutionFailed`] when the graph's
    /// lifecycle task is gone.
    pub async fn publish_participants(&self) -> Result<(), BifrostError> {
        self.signals.publish_participants().await
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
        self.signals.terminal(outcome);
        self.signals
            .settled()
            .await?
            .ok_or(BifrostError::QueryExecutionFailed)
    }
}

/// One completed query's own physical-plan identity and retained spill evidence.
///
/// Folded once, from the executed plan, after its stream is dropped. Every
/// field is read off the production plan rather than reconstructed, so a
/// journey asserting on it is asserting on what actually ran.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AnalyticalPhysicalEvidence {
    /// Output-sort field names, in output order.
    pub sort_schema: Vec<String>,
    /// Output-sort ordering, rendered exactly as the plan holds it.
    pub sort_ordering: String,
    /// Times the output sort spilled a run to scratch.
    pub spill_count: u64,
    /// Bytes the output sort wrote to scratch.
    pub spilled_bytes: u64,
    /// Rows the output sort wrote to scratch.
    pub spilled_rows: u64,
    /// Distinct grouping-column types every aggregate in the plan groups on.
    ///
    /// A plan that groups on the wide sort key rather than the narrow join key
    /// would carry `Utf8` here, which is the shape the baseline refuses.
    pub aggregate_group_types: Vec<String>,
    /// Field names of each equi-join's left-input child, in plan order.
    ///
    /// Hash and sort-merge joins both count: the retained planning shape
    /// disables hash joins at the memory floor, so which operator a query
    /// carries is decided by the grant rather than by the statement.
    pub join_build_schemas: Vec<Vec<String>>,
}

/// One graph's deferred physical-metric fold, awaited inside its own deadline.
///
/// Retained on the graph rather than awaited in the result stream's tail
/// because the fold is unbounded on its own: upstream reports follower metrics
/// only after the coordinator channel closes, and a follower that never answers
/// would otherwise hold the query's terminal open forever. The graph's
/// lifecycle already owns one absolute deadline covering execution, cleanup,
/// and settlement, so the fold becomes one more descendant of it.
pub(super) struct AnalyticalGraphMetricFold {
    /// The follower-metric fold, yielding the metric-carrying plan it built.
    ///
    /// Upstream returns the executed stages' metrics by *rewriting* the plan,
    /// not by mutating the one this process planned: a local stage is
    /// serialized to its worker and executed as a separate instance, so the
    /// planned nodes never see a counter. The rewritten plan is therefore the
    /// only place the executed sort's spill counters exist.
    ///
    /// The rewrite's own error is carried rather than erased: the plan this
    /// struct also holds was never executed as the coordinator's metric
    /// carrier, so substituting it for a failed rewrite would publish an
    /// unexecuted plan's evidence as the query's.
    fold:
        futures_util::future::BoxFuture<'static, datafusion::error::Result<Arc<dyn ExecutionPlan>>>,
}

impl fmt::Debug for AnalyticalGraphMetricFold {
    /// Reports that a fold is retained without rendering the plan or future.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalGraphMetricFold")
            .finish_non_exhaustive()
    }
}

impl AnalyticalGraphMetricFold {
    /// Composes one graph's fold over an executed plan and its scan sink.
    pub(super) fn new(
        plan: Arc<dyn ExecutionPlan>,
        sink: Arc<super::exec::RemoteScanMetrics>,
    ) -> Self {
        Self {
            fold: Box::pin(super::exec::record_distributed_scan_metrics(plan, sink)),
        }
    }

    /// Composes one fold over an arbitrary future, for lifecycle-order tests.
    #[cfg(test)]
    pub(super) fn from_future(
        fold: futures_util::future::BoxFuture<
            'static,
            datafusion::error::Result<Arc<dyn ExecutionPlan>>,
        >,
    ) -> Self {
        Self { fold }
    }

    /// Awaits the follower fold, then reads the plan's own output-sort evidence.
    ///
    /// The caller bounds this; nothing here imposes a second timer.
    ///
    /// # Errors
    ///
    /// Returns the rewrite's own error. A failed rewrite yields no evidence at
    /// all: the plan this process built was never the coordinator's metric
    /// carrier, so there is no second plan to read instead.
    pub(super) async fn settle(
        self,
    ) -> datafusion::error::Result<Option<AnalyticalPhysicalEvidence>> {
        let executed = self.fold.await?;
        // The rewritten plan is the only place the executed stages exist as one
        // tree, and the evidence read below is a projection of it. Rendering it
        // here is what makes a missing sort, aggregate, or join diagnosable
        // without re-running the query under a different build.
        tracing::debug!(
            target: "wyrd::oracle::analytical",
            plan = %datafusion::physical_plan::displayable(executed.as_ref()).indent(true),
            "analytical metric-carrier plan"
        );
        Ok(super::exec::output_sort_evidence(&executed))
    }
}

/// The two guards one leader graph hands to its lifecycle task at registration.
///
/// Named rather than passed positionally because both are `Option`-shaped in
/// the task and transposing them would move the release order — attempt before
/// graph — that keeps the query envelope from being stranded.
pub(super) struct AnalyticalGraphLifecycleOwners {
    /// The graph's own attempt, joined by the lifecycle task.
    pub(super) attempt: Option<AnalyticalAttemptGuard>,
    /// The graph's registration, disarmed once the lifecycle task owns it.
    ///
    /// Handing it over is what stops a caller's `Drop` from removing a graph
    /// the task is still settling; the task itself releases through the
    /// supervisor rather than through this guard.
    pub(super) graph_guard: Option<AnalyticalGraphGuard>,
}
