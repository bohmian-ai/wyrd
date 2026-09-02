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
use std::sync::{Arc, Mutex};

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
use url::Url;
use uuid::Uuid;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::NodeId;

pub use super::analytical_supervisor::AnalyticalSupervisor;

use super::AuthorizedQueryContext;
use super::analytical_supervisor::{
    AnalyticalAttemptGrant, AnalyticalAttemptGuard, AnalyticalAttemptKey, AnalyticalAttemptRelease,
    AnalyticalGraphGuard, AnalyticalSupervisorInspection, StageId, TaskId,
};
use super::analytical_transport::AnalyticalDestination;
use super::analytical_transport::{
    AnalyticalChannelResolver, AnalyticalCoordinatorIdentity, AnalyticalParticipantCut,
    AnalyticalStageSigning, StageWireIdentity, read_ticket,
};
use super::dispatcher::{
    BifrostPeerTls, GraphLeaseRequest, OraclePeerCredentials, ReservationRegistry,
};
use super::participant_cut::OracleQueryAttemptCut;
use super::peer::{AuthorizedStage, OracleStageAuthority, PeerSecurityError, StageOperationV1};
use super::spill::OracleSpillRuntime;
use super::telemetry::{
    AnalyticalAttemptOutcome, AnalyticalStageOperation, record_stage_operation,
};
use crate::resources::{OracleResourceRequest, OracleResources};
use wyrd_spec::vala::api::{
    AnalyticalGraphRef, QueryClass, QueryId, ReservationId, ReserveNodeSlotsRequest,
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
        let mut config = builder.config().clone().unwrap_or_default();
        crate::resources::OracleSessionShape::apply(&mut config);
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
}

/// One graph's verified outbound identity, deadline, and frozen destinations.
struct AnalyticalEgressIdentity {
    /// Coordinator identity this node signs its own stage operations under.
    identity: Arc<AnalyticalCoordinatorIdentity>,
    /// Absolute graph deadline carried unchanged into every outbound ticket.
    deadline_ms: i64,
    /// Participant cut adopted from the ticket that authorized this graph.
    cut: Arc<AnalyticalParticipantCut>,
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
            recorded.cut,
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

    /// Returns the reservation this graph was activated from.
    pub(crate) fn reservation_id(&self) -> ReservationId {
        self.reservation_id
    }

    /// Returns the reservation's original expiry, retained unchanged.
    pub(crate) fn reservation_expires_at(&self) -> DateTime<Utc> {
        self.reservation_expires_at
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
        if !reserving_leader && !self.participant_cut.contains(source_node_id, claims.source_fence)
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
    /// Query-owned runtime every follower descendant of this graph installs.
    runtime: AnalyticalGraphRuntime,
    /// Cancellation child covering every descendant of this graph.
    cancel: CancellationToken,
    /// Attempts admitted under this graph, joined before it may be released.
    attempts: Mutex<HashMap<AnalyticalAttemptKey, AnalyticalAttemptGuard>>,
    /// Whether settlement has already run, so it can never run twice.
    settled: AtomicBool,
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

    /// Returns the reservation residue this graph still holds.
    #[must_use]
    pub(crate) fn activation(&self) -> &CommittedGraphActivation {
        &self.activation
    }

    /// Returns the query-owned runtime this graph's descendants install.
    #[must_use]
    pub(crate) fn runtime(&self) -> &AnalyticalGraphRuntime {
        &self.runtime
    }

    /// Returns this graph's cancellation child.
    #[must_use]
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
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
        if outcome != AnalyticalAttemptOutcome::Succeeded {
            self.cancel.cancel();
        }
        let live = {
            let attempts = self.attempts.lock().map_err(|_| poisoned_ingress())?;
            attempts.keys().copied().collect::<Vec<_>>()
        };
        for key in live {
            self.finish_attempt(key, outcome).await?;
        }
        self.drain().await?;
        let guard = self
            .guard
            .lock()
            .map_err(|_| poisoned_ingress())?
            .take();
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
        tracing::warn!(
            public_query_id = %self.graph.public_query_id,
            datafusion_query_id = %self.graph.datafusion_query_id,
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
    /// Exchange-buffer child every attempt of a graph on this node charges.
    pub exchange_buffer_bytes: usize,
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
    /// Exchange-buffer child every attempt of a graph on this node charges.
    exchange_buffer_bytes: usize,
    /// Upstream worker whose sessions install this node's query-owned runtimes.
    worker: Worker,
    /// This follower's graphs, each in exactly one state, under one mutex.
    graphs: Mutex<HashMap<AnalyticalGraphKey, AnalyticalGraphEntry>>,
    /// Outbound capability this node's own middle stages sign through.
    egress: Arc<AnalyticalStageEgress>,
    /// Bounded queue the caller-drop path hands graphs to for settlement.
    ///
    /// Cleared by shutdown, which is how the driver learns to finish. A send
    /// that cannot be made is recorded as a cleanup failure on the graph; it is
    /// never quietly downgraded to a spawn or a successful terminal.
    settlement: Mutex<Option<mpsc::Sender<GraphSettlement>>>,
    /// The one settlement driver, owned and joined by this ingress.
    driver: Mutex<Option<JoinHandle<()>>>,
    /// Whether this ingress still admits new stage work.
    accepting: AtomicBool,
}

impl fmt::Debug for AnalyticalStageIngress {
    /// Reports the configured budget without rendering owned dependencies.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalStageIngress")
            .field("exchange_buffer_bytes", &self.exchange_buffer_bytes)
            .field("accepting", &self.accepting.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl AnalyticalStageIngress {
    /// Builds the follower ingress and starts its one settlement driver.
    ///
    /// The upstream worker is constructed from [`AnalyticalSessionBuilder`] over
    /// the supervisor's own runtime registry, so a stage whose graph is not
    /// registered — an invalidated attempt, a sibling graph, a forged identity —
    /// fails to build a session rather than silently falling back to a process
    /// runtime.
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
            exchange_buffer_bytes,
            leaf,
            egress,
        } = config;
        let worker = Worker::from_session_builder(AnalyticalSessionBuilder::new(
            Arc::clone(supervisor.registry()),
            leaf,
            Arc::clone(&egress),
        ));
        // Sized from the one capacity root that already bounds graph
        // admission, so the queue can always hold every graph this node is
        // permitted to own at once and there is no second capacity setting.
        let (sender, receiver) = mpsc::channel(reservations.max_concurrent_graphs());
        Arc::new_cyclic(|weak: &Weak<Self>| {
            let driver = match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    let ingress = Weak::clone(weak);
                    Some(handle.spawn(drive_graph_settlements(receiver, ingress)))
                }
                Err(_) => {
                    tracing::warn!(
                        "Oracle analytical follower ingress was built outside a runtime; \
                         caller-drop settlement is unavailable"
                    );
                    None
                }
            };
            Self {
                node_id,
                oracle_fence,
                authority,
                supervisor,
                reservations,
                spill,
                exchange_buffer_bytes,
                worker,
                graphs: Mutex::new(HashMap::new()),
                egress,
                settlement: Mutex::new(Some(sender)),
                driver: Mutex::new(driver),
                accepting: AtomicBool::new(true),
            }
        })
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
        self.egress.record(key.graph(), &authorized)?;
        match operation {
            StageOperationV1::SetPlan => {
                lease.admit_attempt(
                    key,
                    AnalyticalAttemptGrant {
                        exchange_buffer_bytes: self.exchange_buffer_bytes,
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
            }
        }
        tracing::Span::current().record("outcome", "authorized");
        Ok(key)
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
    #[allow(clippy::type_complexity)]
    fn publish(
        &self,
        graph: AnalyticalGraphKey,
        activation: PendingGraphActivation,
        request: &GraphLeaseRequest,
        authorized: &AuthorizedStage,
    ) -> Result<Arc<GraphLease>, (PendingGraphActivation, BifrostError)> {
        let binding = match GraphLeaseBinding::activate(&activation, request, authorized) {
            Ok(binding) => binding,
            Err(error) => return Err((activation, error)),
        };
        if let Err(error) = binding.authorize(authorized) {
            return Err((activation, error));
        }
        let envelope = activation.envelope();
        let runtime = match self
            .spill
            .build_query_runtime(envelope.memory_pool(), envelope.scratch_bytes)
        {
            Ok(runtime) => AnalyticalGraphRuntime::new(runtime, self.exchange_buffer_bytes),
            Err(error) => return Err((activation, error)),
        };
        let supervisor = Arc::clone(&self.supervisor);
        let registered = runtime.clone();
        let (committed, guard) = activation.commit(move |resources| {
            supervisor.register_graph(graph, resources, registered)
        })?;
        Ok(Arc::new(GraphLease {
            binding,
            activation: committed,
            graph,
            supervisor: Arc::clone(&self.supervisor),
            egress: Arc::clone(&self.egress),
            guard: Mutex::new(Some(guard)),
            runtime,
            cancel: self.supervisor.root_cancellation().child_token(),
            attempts: Mutex::new(HashMap::new()),
            settled: AtomicBool::new(false),
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
        let Some(AnalyticalGraphEntry::Active { lease, .. }) = graphs.remove(&graph) else {
            return;
        };
        let failure = self.signal_settlement(GraphSettlement {
            graph,
            lease: Arc::clone(&lease),
            outcome,
        });
        graphs.insert(
            graph,
            AnalyticalGraphEntry::Draining {
                lease,
                settlement_failure: failure,
            },
        );
    }

    /// Offers one graph to the bounded settlement queue without blocking.
    ///
    /// Returns the cleanup failure to record when the queue cannot take it.
    fn signal_settlement(&self, message: GraphSettlement) -> Option<String> {
        let sender = match self.settlement.lock() {
            Ok(settlement) => settlement.clone(),
            Err(_) => {
                return Some("Oracle analytical settlement queue lock is poisoned".to_owned());
            }
        };
        let Some(sender) = sender else {
            return Some("Oracle analytical settlement queue is closed".to_owned());
        };
        match sender.try_send(message) {
            Ok(()) => None,
            Err(mpsc::error::TrySendError::Full(_)) => {
                Some("Oracle analytical settlement queue is full".to_owned())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Some("Oracle analytical settlement queue is closed".to_owned())
            }
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

/// Settles every graph the caller-drop path hands to this node, one at a time.
///
/// The single asynchronous owner of caller-drop settlement. It holds only a
/// [`Weak`] back-reference, so the ingress's own join handle cannot keep the
/// ingress alive; a settlement that completes after the node is gone simply has
/// no entry left to update. Returning ends the driver, which is what
/// [`AnalyticalStageIngress::shutdown`] joins.
async fn drive_graph_settlements(
    mut settlements: mpsc::Receiver<GraphSettlement>,
    ingress: Weak<AnalyticalStageIngress>,
) {
    while let Some(message) = settlements.recv().await {
        let settled = message.lease.settle(message.outcome).await;
        if let Err(error) = &settled {
            tracing::warn!(
                error = %error,
                public_query_id = %message.graph.public_query_id,
                "Oracle analytical follower could not settle a closed graph"
            );
        }
        if let Some(ingress) = ingress.upgrade() {
            ingress.record_settlement(message.graph, &settled);
        }
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
    /// Each reserved participant and the exact release its reservation needs.
    releases: Vec<(
        super::dispatcher::DispatchCandidate,
        wyrd_spec::vala::api::ReleaseNodeSlotsRequest,
    )>,
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
    /// Returns every reserved participant to its owner, exactly once.
    ///
    /// A per-participant failure is logged rather than propagated: the attempt
    /// is already ending, the reservation expires on its own, and failing the
    /// terminal because one peer was unreachable would turn a completed query
    /// into an error.
    async fn release(&mut self) {
        let Some(transports) = self.transports.take() else {
            return;
        };
        for (candidate, request) in self.releases.drain(..) {
            if let Err(error) = transports
                .release_graph_reservation(&candidate, request)
                .await
            {
                tracing::warn!(
                    error = ?error,
                    node_id = %candidate.node_id.as_uuid(),
                    "Oracle analytical leader could not release a participant reservation"
                );
            }
        }
    }
}

impl Drop for AnalyticalParticipantReservations {
    /// Returns any reservation an unsettled attempt still holds.
    ///
    /// The settled path releases inline and leaves nothing to do here. This
    /// covers the attempt that was dropped instead — a panic, an early return,
    /// a caller that abandoned the session — where the alternative is holding a
    /// follower's envelope until its reservation expires.
    fn drop(&mut self) {
        if self.transports.is_none() {
            return;
        }
        let mut outstanding = Self {
            transports: self.transports.take(),
            releases: std::mem::take(&mut self.releases),
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(
                "Oracle analytical leader dropped participant reservations outside a runtime"
            );
            return;
        };
        handle.spawn(async move { outstanding.release().await });
    }
}

/// Slot units one distributed graph reserves on each participant.
///
/// A graph occupies a participant for the whole plan, not for one leaf, so it
/// charges the Analytical class's full per-query demand. The receiving node
/// clamps this to its own running capacity before charging, so a smaller peer
/// still admits the graph rather than refusing a structurally unschedulable
/// demand.
const ANALYTICAL_GRAPH_SLOT_UNITS: u32 = 2;

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
    /// Releases this connection's share of the graph's follower ownership.
    fn drop(&mut self) {
        let ingress = Arc::clone(&self.ingress);
        let graph = self.graph;
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(
                "Oracle analytical follower dropped a stage connection outside a runtime"
            );
            return;
        };
        handle.spawn(async move {
            if let Err(error) = ingress.release_connection(graph).await {
                tracing::warn!(
                    error = %error,
                    "Oracle analytical follower could not release a closed stage connection"
                );
            }
        });
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
            roles: [crate::resources::BifrostRole::Oracle].into_iter().collect(),
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
        /// Spill owner kept alive for the ingress's runtime construction.
        _spill: Arc<OracleSpillRuntime>,
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
            let root = tempfile::tempdir().expect("fixture scratch root must exist");
            let spill = Arc::new(
                OracleSpillRuntime::new(root.path(), 2 * 1024 * 1024 * 1024)
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
            let ingress = Arc::new(AnalyticalStageIngress::new(AnalyticalStageIngressConfig {
                node_id,
                oracle_fence: fence,
                authority: Arc::new(VerifyingStageAuthority),
                supervisor: Arc::clone(&supervisor),
                reservations: Arc::clone(&reservations),
                spill: Arc::clone(&spill),
                exchange_buffer_bytes: 64 * 1024,
                leaf: super::super::codec::AnalyticalLeafBinding::new(
                    wyrd_spec::vala::api::ClusterRole::Oracle,
                    Arc::new(CountingSource {
                        resolutions: Arc::clone(&resolutions),
                    }),
                    Arc::new(crate::oracle::AcceptingOracleAudit),
                ),
                egress: fixture_egress(),
            }));
            let graph = AnalyticalGraphKey::new(
                PublicQueryId::from_uuid(Uuid::from_u128(11)),
                DataFusionQueryId::from_uuid(Uuid::from_u128(12)),
            );
            let leader_node_id = NodeId::new(Uuid::from_u128(1));
            let leader_fence = 3;
            let deadline = now + chrono::Duration::seconds(60);
            let resources = fixture_oracle_role()
                .try_acquire_query(OracleResourceRequest::for_class(QueryClass::Analytical, 0.0))
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
                _spill: spill,
                _root: root,
            }
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
            fixture.supervisor.live_graphs().expect("graphs are readable"),
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
            fixture.supervisor.live_graphs().expect("graphs are readable"),
            1,
            "the live graph survived every refusal unchanged"
        );
        assert_eq!(
            fixture.resolutions.load(Ordering::SeqCst),
            0,
            "no refused message reached a provider, cache, or source"
        );
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

/// Splits one Analytical leaf's signed files across its stage's final tasks.
///
/// Without this, every task of a leaf stage decodes the same plan and therefore
/// the same complete assignment, so a fixture with one table read once per task
/// returns each row `task_count` times. Upstream calls this after a stage's task
/// count is final, which is the only point at which the split is knowable.
struct AnalyticalLeafSplit;

impl datafusion_distributed::ScaleUpLeafNodeHandler for AnalyticalLeafSplit {
    /// Replaces an assignment-bearing placeholder with one variant per task.
    ///
    /// The variants share schema and partition count — upstream requires both —
    /// and differ only in the slice of signed file descriptors each carries. A
    /// placeholder with no assignment is not ours to split, and a leaf whose
    /// files do not divide is left with empty slices on the trailing tasks
    /// rather than a wider share on any of them.
    fn handle(
        &self,
        ev: datafusion_distributed::ScaleUpLeafNodeEvent<'_>,
    ) -> Option<Result<datafusion_distributed::ScaleUpLeafNodeEventResponse, DataFusionError>> {
        let placeholder = ev
            .plan
            .downcast_ref::<super::codec::RemoteSourcePlaceholderExec>()?;
        let assignment = placeholder.assignment()?;
        let tasks = ev.task_count.max(1);
        let variants = (0..tasks)
            .map(|task| {
                let mut narrowed = assignment.clone();
                narrowed.persisted.files = assignment
                    .persisted
                    .files
                    .iter()
                    .skip(task)
                    .step_by(tasks)
                    .cloned()
                    .collect();
                Arc::new(placeholder.clone().with_assignment(narrowed)) as Arc<dyn ExecutionPlan>
            })
            .collect::<Vec<_>>();
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

impl AnalyticalExecutionHandle {
    /// Composes the handle over this node's existing Analytical owners.
    #[must_use]
    pub fn new(
        worker: Arc<AnalyticalStageIngress>,
        authority: Arc<dyn OracleStageAuthority>,
        supervisor: Arc<AnalyticalSupervisor>,
        spill: Arc<OracleSpillRuntime>,
        oracle_resources: OracleResources,
        peer_transports: Option<Arc<super::dispatcher::OraclePeerTransportDirectory>>,
        config: AnalyticalExecutionConfig,
        leaf: super::codec::AnalyticalLeafBinding,
    ) -> Self {
        Self {
            worker,
            authority,
            supervisor,
            spill,
            oracle_resources,
            peer_transports,
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
    pub async fn lease_session(
        &self,
        attempt: &AnalyticalAttemptContext,
        cut: &OracleQueryAttemptCut,
        context: &AuthorizedQueryContext,
        work_units: usize,
    ) -> Result<(SessionContext, AnalyticalAttemptOwnership), BifrostError> {
        let graph = AnalyticalGraphKey::new(attempt.public_query_id, attempt.datafusion_query_id);
        // Reserved before anything is admitted locally. A participant that
        // declines must fail the attempt while the leader still owns nothing,
        // not after it has charged its own envelope and registered a graph.
        let (destinations, participants) = self.reserve_destinations(cut, graph).await?;
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
                snapshot_digest: &attempt.snapshot_digest,
                destinations,
                permission_digest: &attempt.permission_digest,
                granted_memory_bytes,
                target_partitions,
                work_units,
            },
            graph,
            cut.deadline().timestamp_millis(),
        )?;
        Ok((
            session,
            AnalyticalAttemptOwnership {
                graph: graph_guard,
                attempt: attempt_guard,
                participants,
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
        deadline_ms: i64,
    ) -> Result<SessionContext, BifrostError> {
        let AnalyticalSessionInputs {
            context,
            snapshot_digest,
            destinations,
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
            // Placeholder only. The resolver substitutes each destination's own
            // frozen reservation before any channel to it is built, because a
            // follower honours only the reservation it granted itself.
            reservation_id: String::new(),
            permission_digest: permission_digest.to_owned(),
        });
        // Frozen here, once, for the whole attempt: everything downstream —
        // this leader's own channels and every follower that becomes a
        // coordinator beneath it — addresses this exact set, so no membership
        // change can add, remove, or re-fence a destination mid-attempt.
        let participants = Arc::new(AnalyticalParticipantCut::freeze(destinations)?);
        let urls = participants.urls();
        let resolver = AnalyticalChannelResolver::new(
            identity,
            self.config.peer_tls.clone(),
            Arc::clone(&self.config.peer_credentials),
            Arc::clone(&participants),
            AnalyticalStageSigning {
                authority: Arc::clone(&self.authority),
                absolute_deadline_ms: deadline_ms,
                ticket_ttl: self.config.ticket_ttl,
            },
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
        config.set_distributed_scale_up_leaf_node_handler(AnalyticalLeafSplit);
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

    /// Reserves every remote participant's graph envelope and freezes their identities.
    ///
    /// One reservation per participant, taken before the leader admits anything
    /// of its own, and carried into the frozen cut so each follower is later
    /// charged against the reservation it granted rather than one the
    /// coordinator invented.
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
    /// valid URL — which would otherwise leave a worker unreachable and
    /// unsigned — or when this node composed no peer transport, and
    /// [`BifrostError::QueryAdmissionRejected`] when a participant declined its
    /// reservation.
    async fn reserve_destinations(
        &self,
        cut: &OracleQueryAttemptCut,
        graph: AnalyticalGraphKey,
    ) -> Result<
        (
            HashMap<Url, AnalyticalDestination>,
            AnalyticalParticipantReservations,
        ),
        BifrostError,
    > {
        let remote = cut
            .oracles()
            .iter()
            .filter(|participant| participant.node_id != self.config.node_id)
            .collect::<Vec<_>>();
        if remote.is_empty() {
            return Ok((
                HashMap::new(),
                AnalyticalParticipantReservations {
                    transports: None,
                    releases: Vec::new(),
                },
            ));
        }
        let Some(transports) = self.peer_transports.as_ref() else {
            return Err(BifrostError::Internal {
                detail: "Oracle analytical leader has no peer transport to reserve participants                          through"
                    .to_owned(),
            });
        };
        let request = ReserveNodeSlotsRequest {
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
        };
        let mut destinations = HashMap::with_capacity(remote.len());
        // Held from the first successful reservation, so a later participant's
        // refusal still returns everything already taken rather than stranding
        // the peers that said yes.
        let mut reserved = AnalyticalParticipantReservations {
            transports: Some(Arc::clone(transports)),
            releases: Vec::with_capacity(remote.len()),
        };
        for participant in remote {
            let url =
                Url::parse(&participant.endpoint).map_err(|error| BifrostError::Internal {
                    detail: format!(
                        "Oracle analytical participant endpoint is not a valid URL: {error}"
                    ),
                })?;
            let candidate = super::dispatcher::DispatchCandidate {
                node_id: participant.node_id,
                role: wyrd_spec::vala::api::ClusterRole::Oracle,
                worker_fence: participant.fencing_token,
                endpoint: Some(participant.endpoint.clone()),
            };
            let pending = transports
                .reserve_graph(&candidate, request.clone())
                .await
                .map_err(|_| BifrostError::QueryAdmissionRejected)?;
            reserved.releases.push((
                candidate,
                wyrd_spec::vala::api::ReleaseNodeSlotsRequest {
                    reservation_id: pending.reservation_id,
                    query_id: request.query_id,
                    leader_node_id: request.leader_node_id,
                    leader_fencing_token: request.leader_fencing_token,
                },
            ));
            destinations.insert(
                url,
                AnalyticalDestination {
                    node_id: participant.node_id,
                    fence: participant.fencing_token,
                    reservation_id: pending.reservation_id.as_uuid().to_string(),
                },
            );
        }
        Ok((destinations, reserved))
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
    /// Pinned snapshot digest of the attempt's cut.
    snapshot_digest: &'a str,
    /// Reserved participants this attempt may address, and nothing beyond them.
    destinations: HashMap<Url, AnalyticalDestination>,
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
    /// Participant reservations this leader must return when the attempt ends.
    ///
    /// Declared last so it releases after the local guards: a participant is
    /// told to drop the reservation only once this node has stopped addressing
    /// it.
    pub participants: AnalyticalParticipantReservations,
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
        // The retry reuses the graph and, with it, every participant
        // reservation the first attempt took. Re-reserving would charge each
        // follower a second envelope for a plan it is already holding one for.
        let Self {
            attempt,
            graph,
            participants,
        } = self;
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
        Ok(Self {
            attempt,
            graph,
            participants,
        })
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
        let Self {
            attempt,
            graph,
            mut participants,
        } = self;
        let release = attempt.finish(outcome).await?;
        graph.release()?;
        participants.release().await;
        Ok(release)
    }
}
