//! Authenticated ready-Oracle selection and private query forwarding.

use std::future::Future;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::ipc::reader::StreamReader;
use futures_util::StreamExt;
use rand::RngCore as _;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::cluster::{ClusterRegistry, ClusterSnapshot};
use vala_bifrost_redux::oracle::dispatcher::{OraclePeerCredentials, OraclePeerTls};
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, Oracle, OracleConfig, OraclePlanner, OracleQueryAttemptCut,
    OracleQueryStream,
};
use wyrd_runtime::Permission;
use wyrd_spec::vala::api::{BifrostQueryRequest, NodeId, QueryClass, QueryId, SignedPeerTicket};
use wyrd_spec::vala::error::BifrostError;
use wyrd_tonic::query_conversion::QueryStreamConverter;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::ForwardQueryRequest;
use wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient;

use super::peer_authority::{ForwardQueryClaims, OraclePeerAuthority};

/// Maximum time a signed forwarding envelope remains presentable to its selected peer.
const FORWARD_ENVELOPE_TTL: chrono::Duration = chrono::Duration::seconds(15);
/// Maximum encoded query batch accepted from the private forwarding stream.
const MAX_FORWARDED_BATCH_BYTES: usize = 64 * 1024 * 1024;

/// One already-connected remote Oracle selected before any request bytes are delivered.
struct ConnectedOracle {
    /// Connected tonic client that has not yet received the forwarding request.
    client: OraclePeerServiceClient<Channel>,
}

/// Canonical owner for ingress classification, ready-Oracle selection, and forwarding.
pub struct ReadyOracleForwarder {
    /// Current-ready membership authority captured once per query.
    cluster: Arc<ClusterRegistry>,
    /// Shared tenant-qualified catalog used for pre-attempt classification.
    catalog: Arc<BifrostCatalog>,
    /// Local Oracle leader operation, when this replica owns Oracle.
    local_oracle: Option<Arc<Oracle>>,
    /// Stable physical node identity.
    local_node_id: NodeId,
    /// Exact local Oracle fence, absent on non-Oracle replicas.
    local_fence: Option<u64>,
    /// Private service credential owner; never contains the public bearer.
    credentials: Arc<dyn OraclePeerCredentials>,
    /// Optional production TLS trust policy.
    tls: Option<OraclePeerTls>,
    /// Domain-separated signed-envelope authority.
    authority: Arc<OraclePeerAuthority>,
    /// Side-effect-free ingress classification owner.
    planner: OraclePlanner,
}

/// Presents the cluster-aware forwarder as the Gate's one SQL dispatch seam.
///
/// Gate owns authentication, admission closure, and request accounting; role
/// selection, ticket minting, fencing, and peer transport stay here, on the
/// server tier that owns Wyrd auth, tenancy, and audit.
#[async_trait::async_trait]
impl vala_bifrost_redux::contracts::OracleQueryDispatch for ReadyOracleForwarder {
    /// Forwards one authorized SQL request to the selected ready Oracle.
    ///
    /// # Errors
    ///
    /// Returns the stable validation, catalog, role, transport, security,
    /// admission, timeout, or execution failure produced by [`Self::forward`].
    async fn dispatch_sql(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
    ) -> Result<OracleQueryStream, BifrostError> {
        self.forward(context, request).await
    }
}

/// Explicit dependencies consumed by the one ready-Oracle forwarding owner.
pub struct ReadyOracleForwarderInputs {
    /// Current-ready membership authority.
    pub cluster: Arc<ClusterRegistry>,
    /// Shared tenant-qualified catalog.
    pub catalog: Arc<BifrostCatalog>,
    /// Local Oracle leader operation, when selected on this replica.
    pub local_oracle: Option<Arc<Oracle>>,
    /// Stable physical node identity.
    pub local_node_id: NodeId,
    /// Exact local Oracle fence, when selected.
    pub local_fence: Option<u64>,
    /// Private service credential owner.
    pub credentials: Arc<dyn OraclePeerCredentials>,
    /// Optional production TLS trust policy.
    pub tls: Option<OraclePeerTls>,
    /// Domain-separated signed-envelope authority.
    pub authority: Arc<OraclePeerAuthority>,
    /// Query floor and classification configuration.
    pub config: OracleConfig,
}

impl ReadyOracleForwarder {
    /// Composes the one forwarding owner from the shared production dependency graph.
    #[must_use]
    pub fn new(inputs: ReadyOracleForwarderInputs) -> Self {
        let ReadyOracleForwarderInputs {
            cluster,
            catalog,
            local_oracle,
            local_node_id,
            local_fence,
            credentials,
            tls,
            authority,
            config,
        } = inputs;
        Self {
            cluster,
            catalog,
            local_oracle,
            local_node_id,
            local_fence,
            credentials,
            tls,
            authority,
            planner: OraclePlanner::new(config),
        }
    }

    /// Routes one already-authenticated public query to an exact ready Oracle.
    ///
    /// Candidate connections are attempted before the immutable cut is created. Once a local
    /// or connected remote leader is selected and signed, the request is delivered exactly once;
    /// every delivery-time transport result is terminal and is never retried.
    ///
    /// # Errors
    /// Returns a closed validation, catalog, role, transport, security, or execution failure.
    pub async fn forward(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
    ) -> Result<OracleQueryStream, BifrostError> {
        Self::validate_context(&context)?;
        self.planner.validate_query(&request)?;
        let duration = request.deadline_ms.map_or(
            OracleConfig::default().default_deadline,
            Duration::from_millis,
        );
        let monotonic_deadline = Instant::now()
            .checked_add(duration)
            .ok_or(BifrostError::QueryTimeout)?;
        let now = chrono::Utc::now();
        let wall_deadline =
            now + chrono::Duration::from_std(duration).map_err(|_| BifrostError::QueryTimeout)?;
        let snapshot = self.cluster.snapshot();
        let planned = self
            .planner
            .classify_for_forwarding(
                &context,
                &request,
                monotonic_deadline,
                &self.catalog,
                &snapshot,
            )
            .await?;
        let query_class = planned.query_class();
        let mut candidates = eligible_oracle_candidates(&snapshot, query_class);
        if let Some(local) = candidates.iter().find(|lease| {
            lease.key.node_id == self.local_node_id
                && self
                    .local_oracle
                    .as_ref()
                    .is_some_and(|oracle| oracle.is_ready())
                && self.local_fence == Some(lease.fencing_token)
        }) {
            let claims = ForwardingAttempt {
                context,
                request,
                query_class,
                now,
                wall_deadline,
            }
            .into_claims(&snapshot, (local.key.node_id, local.fencing_token))?;
            let ticket = self
                .authority
                .mint_forward_query(&claims)
                .map_err(|_| BifrostError::QueryPeerSecurity)?;
            // The plan travels beside the ticket, never inside it: the ticket
            // still carries every authorization claim, and the pinned snapshot
            // never leaves this process.
            return self.accept(ticket, Some(planned)).await;
        }
        let remote_candidates = candidates
            .drain(..)
            .filter(|lease| lease.key.node_id != self.local_node_id)
            .map(|candidate| {
                (
                    candidate.key.node_id,
                    candidate.fencing_token,
                    candidate.address.clone(),
                )
            });
        route_remote_once(
            &snapshot,
            remote_candidates,
            monotonic_deadline,
            ForwardingAttempt {
                context,
                request,
                query_class,
                now,
                wall_deadline,
            },
            |_node_id, address| self.connect(address),
            |claims| {
                self.authority
                    .mint_forward_query(claims)
                    .map_err(|_| BifrostError::QueryPeerSecurity)
            },
            |_leader, client, ticket, visibility| {
                self.forward_remote(ConnectedOracle { client }, ticket, visibility)
            },
        )
        .await
    }

    /// Verifies and executes one signed envelope on this replica's local Oracle.
    ///
    /// `prepared` carries a catalog snapshot this process already pinned while
    /// classifying the same request, letting a local leader skip a redundant
    /// second pin. It is an in-process optimization only: the ticket remains the
    /// sole source of authorization, and a ticket arriving from a peer always
    /// passes `None` because the pinned file list never crosses the wire.
    ///
    /// # Errors
    /// Returns a closed authentication, policy, cut, role-fence, deadline, or query failure.
    pub async fn accept(
        &self,
        ticket: SignedPeerTicket,
        prepared: Option<vala_bifrost_redux::oracle::PlannedSqlCut>,
    ) -> Result<OracleQueryStream, BifrostError> {
        let fence = self
            .local_fence
            .ok_or(BifrostError::OracleRoleUnavailable)?;
        let claims = self
            .authority
            .verify_forward_query(&ticket, self.local_node_id, fence, chrono::Utc::now())
            .await
            .map_err(|_| BifrostError::QueryPeerSecurity)?;
        Self::validate_claims(&claims, self.local_node_id, fence)?;
        self.local_oracle
            .as_ref()
            .ok_or(BifrostError::OracleRoleUnavailable)?
            .query_sql_with_participant_cut(
                claims.context,
                claims.request,
                claims.participant_cut,
                claims.query_class,
                prepared,
            )
            .await
    }

    /// Connects one endpoint without sending any query envelope bytes.
    async fn connect(
        &self,
        address: String,
    ) -> Result<OraclePeerServiceClient<Channel>, BifrostError> {
        let endpoint = if let Some(tls) = &self.tls {
            wyrd_tonic::transport::authenticated_tls_endpoint(
                address,
                tls.ca_certificate_pem(),
                tls.server_name().to_owned(),
            )
            .map_err(|_| BifrostError::QueryPeerSecurity)?
        } else {
            wyrd_tonic::transport::plaintext_endpoint(address)
                .map_err(|_| BifrostError::OracleRoleUnavailable)?
        };
        endpoint
            .connect()
            .await
            .map(OraclePeerServiceClient::new)
            .map_err(|_| BifrostError::OracleRoleUnavailable)
    }

    /// Delivers one envelope exactly once over an already-connected authenticated channel.
    async fn forward_remote(
        &self,
        mut connected: ConnectedOracle,
        ticket: SignedPeerTicket,
        visibility: wyrd_spec::vala::api::VisibilityMode,
    ) -> Result<OracleQueryStream, BifrostError> {
        let bearer = self
            .credentials
            .bearer(false)
            .await
            .map_err(|_| BifrostError::QueryPeerSecurity)?;
        let metadata: MetadataValue<_> = format!("Bearer {bearer}")
            .parse()
            .map_err(|_| BifrostError::QueryPeerSecurity)?;
        let mut request = Request::new(ForwardQueryRequest {
            envelope: Some(ticket.into()),
        });
        request
            .metadata_mut()
            .insert("x-wyrd-access-token", metadata);
        let response = connected
            .client
            .forward_query(request)
            .await
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        let schema_fingerprint = response
            .metadata()
            .get("x-wyrd-schema-fingerprint")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or(BifrostError::QueryPeerSecurity)?
            .to_owned();
        let mut wire = response.into_inner();
        let frames = async_stream::stream! {
            let mut converter = QueryStreamConverter::new(visibility);
            while let Some(frame) = wire.next().await {
                let frame = match frame {
                    Ok(frame) => frame,
                    Err(_) => {
                        yield Err(BifrostError::QueryExecutionFailed);
                        break;
                    }
                };
                let batch_rows = match frame.frame.as_ref() {
                    Some(wyrd_tonic::wyrd::v1::query_stream_frame::Frame::Batch(batch)) => {
                        match forwarded_batch_rows(&batch.arrow_ipc_batch) {
                            Ok(rows) => Some(rows),
                            Err(error) => {
                                yield Err(error);
                                break;
                            }
                        }
                    }
                    _ => None,
                };
                match converter.convert(frame, batch_rows) {
                    Ok(frame) => yield Ok(frame),
                    Err(_) => {
                        yield Err(BifrostError::QueryPeerSecurity);
                        break;
                    }
                }
            }
        };
        Ok(OracleQueryStream::from_forwarded(
            schema_fingerprint,
            Box::pin(frames),
            tokio_util::sync::CancellationToken::new(),
        ))
    }

    /// Validates the signed public-caller policy projection.
    fn validate_context(context: &AuthorizedQueryContext) -> Result<(), BifrostError> {
        if context.principal.tenant_id != context.data_tenant_id
            || context.permission != Permission::bifrost_query_read().to_string()
            || !context
                .principal
                .effective_permissions
                .contains(&Permission::bifrost_query_read())
        {
            return Err(BifrostError::QueryPeerSecurity);
        }
        Ok(())
    }

    /// Validates every redundant cut/envelope binding before local leadership begins.
    fn validate_claims(
        claims: &ForwardQueryClaims,
        local_node_id: NodeId,
        local_fence: u64,
    ) -> Result<(), BifrostError> {
        Self::validate_context(&claims.context)?;
        claims
            .request
            .validate()
            .map_err(|_| BifrostError::QueryPeerSecurity)?;
        if claims.audience != local_node_id
            || claims.worker_fence != local_fence
            || claims.participant_cut.leader().node_id != local_node_id
            || claims.participant_cut.leader().fencing_token != local_fence
            || claims.participant_cut.fingerprint() != claims.participant_cut_fingerprint
            || claims.participant_cut.deadline().timestamp_millis() != claims.absolute_deadline_ms
            || claims.participant_cut.deadline() <= chrono::Utc::now()
            || claims.participant_cut.attempt_id().as_uuid().to_string()
                != claims.context.request_id.as_str()
        {
            return Err(BifrostError::QueryPeerSecurity);
        }
        Ok(())
    }
}

/// Returns ready Oracle leases for one class in stable snapshot order.
fn eligible_oracle_candidates(
    snapshot: &ClusterSnapshot,
    class: QueryClass,
) -> Vec<wyrd_spec::vala::api::ClusterRoleLease> {
    let mut candidates = snapshot
        .live_oracles()
        .into_iter()
        .filter(|lease| {
            lease.ready
                && match &lease.capabilities {
                    wyrd_spec::vala::api::ClusterCapabilities::OracleV1(capabilities) => {
                        capabilities.supported_classes.contains(&class)
                    }
                    wyrd_spec::vala::api::ClusterCapabilities::ScribeV1(_) => false,
                }
                && lease.fencing_token > 0
        })
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|lease| lease.key.node_id);
    candidates
}

/// Connects to at most two immutable-snapshot candidates before any envelope delivery.
async fn connect_before_delivery<T, I, F, Fut>(
    candidates: I,
    deadline: Instant,
    mut connect: F,
) -> Result<((NodeId, u64), T), BifrostError>
where
    I: IntoIterator<Item = (NodeId, u64, String)>,
    F: FnMut(NodeId, String) -> Fut,
    Fut: Future<Output = Result<T, BifrostError>>,
{
    for (node_id, fencing_token, address) in candidates.into_iter().take(2) {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        if let Ok(Ok(connected)) = tokio::time::timeout(remaining, connect(node_id, address)).await
        {
            return Ok(((node_id, fencing_token), connected));
        }
    }
    Err(BifrostError::OracleRoleUnavailable)
}

/// Owns remote selection, same-snapshot envelope construction, and one terminal delivery.
async fn route_remote_once<C, E, O, I, CF, CFut, EF, DF, DFut>(
    snapshot: &ClusterSnapshot,
    candidates: I,
    connect_deadline: Instant,
    attempt: ForwardingAttempt,
    connect: CF,
    make_envelope: EF,
    deliver: DF,
) -> Result<O, BifrostError>
where
    I: IntoIterator<Item = (NodeId, u64, String)>,
    CF: FnMut(NodeId, String) -> CFut,
    CFut: Future<Output = Result<C, BifrostError>>,
    EF: FnOnce(&ForwardQueryClaims) -> Result<E, BifrostError>,
    DF: FnOnce((NodeId, u64), C, E, wyrd_spec::vala::api::VisibilityMode) -> DFut,
    DFut: Future<Output = Result<O, BifrostError>>,
{
    let (leader, connected) =
        connect_before_delivery(candidates, connect_deadline, connect).await?;
    let claims = attempt.into_claims(snapshot, leader)?;
    let visibility = claims.request.visibility;
    let envelope = make_envelope(&claims)?;
    deliver(leader, connected, envelope, visibility).await
}

/// The caller-supplied inputs one forwarding attempt signs into its envelope.
///
/// Every field is fixed before a leader is chosen, so they travel as one value
/// through candidate connection and are consumed exactly once when the elected
/// leader is known. Keeping them together also guarantees a retry cannot mix
/// inputs from two different attempts.
pub(crate) struct ForwardingAttempt {
    /// Authenticated caller identity and request id for this attempt.
    pub context: AuthorizedQueryContext,
    /// The query being forwarded, unchanged from ingress.
    pub request: BifrostQueryRequest,
    /// Admission class governing the participant cut.
    pub query_class: QueryClass,
    /// Ingress wall clock, used for envelope TTL and snapshot-age accounting.
    pub now: chrono::DateTime<chrono::Utc>,
    /// One absolute deadline captured at ingress and propagated unchanged.
    pub wall_deadline: chrono::DateTime<chrono::Utc>,
}

impl ForwardingAttempt {
    /// Builds the one signed-envelope payload from the captured membership
    /// snapshot and the elected leader.
    ///
    /// Derives the participant cut from `snapshot` rather than re-reading
    /// membership, so the fingerprint the follower verifies describes exactly
    /// the cut the leader routed against.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryPeerSecurity`] when the request id is not a
    /// UUID, and [`BifrostError::OracleRoleUnavailable`] when the snapshot
    /// cannot yield a valid participant cut.
    fn into_claims(
        self,
        snapshot: &ClusterSnapshot,
        leader: (NodeId, u64),
    ) -> Result<ForwardQueryClaims, BifrostError> {
        let Self {
            context,
            request,
            query_class,
            now,
            wall_deadline,
        } = self;
        let observed_age = now
            .signed_duration_since(snapshot.observed_at())
            .to_std()
            .unwrap_or_default();
        let attempt_id = QueryId::new(
            uuid::Uuid::parse_str(context.request_id.as_str())
                .map_err(|_| BifrostError::QueryPeerSecurity)?,
        );
        let participant_cut = OracleQueryAttemptCut::try_from_snapshot(
            snapshot,
            attempt_id,
            leader.0,
            query_class,
            wall_deadline,
            now,
            observed_age.saturating_add(Duration::from_secs(1)),
        )
        .map_err(|_| BifrostError::OracleRoleUnavailable)?;
        let mut nonce = vec![0_u8; 16];
        rand::rng().fill_bytes(&mut nonce);
        Ok(ForwardQueryClaims {
            protocol_version: 1,
            audience: leader.0,
            worker_fence: leader.1,
            nonce,
            expires_at_ms: (now + FORWARD_ENVELOPE_TTL)
                .min(wall_deadline)
                .timestamp_millis(),
            context,
            request,
            query_class,
            participant_cut_fingerprint: participant_cut.fingerprint(),
            absolute_deadline_ms: participant_cut.deadline().timestamp_millis(),
            participant_cut,
        })
    }
}

/// Counts rows in one bounded Arrow IPC batch frame before protocol conversion.
fn forwarded_batch_rows(bytes: &[u8]) -> Result<u64, BifrostError> {
    if bytes.len() > MAX_FORWARDED_BATCH_BYTES {
        return Err(BifrostError::QueryExecutionFailed);
    }
    let mut reader = StreamReader::try_new(Cursor::new(bytes), None)
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    reader.try_fold(0_u64, |rows, batch| {
        let batch = batch.map_err(|_| BifrostError::QueryExecutionFailed)?;
        rows.checked_add(
            u64::try_from(batch.num_rows()).map_err(|_| BifrostError::QueryExecutionFailed)?,
        )
        .ok_or(BifrostError::QueryExecutionFailed)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::{Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AuthMethod, ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease,
        FreshnessPolicy, OracleCapabilitiesV1, VisibilityMode,
    };

    /// Builds one ready Oracle lease with deterministic snapshot ordering.
    fn oracle_lease(ordinal: u128, fencing_token: u64) -> ClusterRoleLease {
        let now = chrono::Utc::now();
        ClusterRoleLease {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::from_u128(ordinal)),
                role: ClusterRole::Oracle,
            },
            address: format!("http://127.0.0.1:{}", 20_000 + ordinal),
            fencing_token,
            capability_version: 1,
            capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 2.0,
                memory_budget_bytes: 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 512,
                raw_slots: 2,
                usable_slots: 2,
                supported_classes: vec![QueryClass::Interactive],
                max_workers_per_query: 2,
            }),
            ready: true,
            started_at: now,
            heartbeat_at: now,
        }
    }

    /// Builds one valid authenticated query envelope input.
    fn query_input() -> (AuthorizedQueryContext, BifrostQueryRequest) {
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let permission = Permission::bifrost_query_read();
        let principal = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant,
            Vec::new(),
            PermissionSet::from_iter([permission.clone()]),
        );
        let context = AuthorizedQueryContext::try_new(
            principal,
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            permission.to_string(),
        )
        .expect("tenant-bound query context");
        let request = BifrostQueryRequest {
            sql: "SELECT value FROM vala.bifrost.events".to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(5_000),
        };
        (context, request)
    }

    /// O2 retries only proven pre-delivery connect failure and never ambiguous delivery.
    #[tokio::test]
    async fn ready_oracle_forwarding_retry_boundary_is_causal_and_one_cut() {
        let observed_at = chrono::Utc::now();
        let leases = vec![
            oracle_lease(1, 11),
            oracle_lease(2, 22),
            oracle_lease(3, 33),
        ];
        let snapshot = ClusterSnapshot::observed(leases.clone(), observed_at);
        let candidates = || {
            eligible_oracle_candidates(&snapshot, QueryClass::Interactive)
                .into_iter()
                .map(|lease| (lease.key.node_id, lease.fencing_token, lease.address))
        };
        let first = leases[0].key.node_id;
        let second = leases[1].key.node_id;
        let expected_participants = leases
            .iter()
            .map(|lease| (lease.key.node_id, lease.fencing_token))
            .collect::<Vec<_>>();
        let wall_deadline = observed_at + chrono::Duration::seconds(5);
        let connect_order = Arc::new(Mutex::new(Vec::new()));
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let (context, request) = query_input();
        route_remote_once(
            &snapshot,
            candidates(),
            Instant::now() + Duration::from_secs(1),
            ForwardingAttempt {
                context,
                request,
                query_class: QueryClass::Interactive,
                now: observed_at,
                wall_deadline,
            },
            {
                let connect_order = Arc::clone(&connect_order);
                move |node_id, _address| {
                    connect_order
                        .lock()
                        .expect("connect-order lock")
                        .push(node_id);
                    async move {
                        if node_id == first {
                            Err(BifrostError::OracleRoleUnavailable)
                        } else {
                            Ok(())
                        }
                    }
                }
            },
            |claims| Ok(claims.clone()),
            {
                let delivered = Arc::clone(&delivered);
                move |leader, (), envelope: ForwardQueryClaims, _visibility| {
                    delivered.lock().expect("delivery lock").push((
                        leader,
                        envelope.participant_cut.leader().node_id,
                        envelope.participant_cut.leader().fencing_token,
                        envelope.participant_cut_fingerprint.clone(),
                        envelope
                            .participant_cut
                            .oracles()
                            .iter()
                            .map(|participant| (participant.node_id, participant.fencing_token))
                            .collect::<Vec<_>>(),
                    ));
                    async { Ok(()) }
                }
            },
        )
        .await
        .expect("second pre-delivery candidate receives the one envelope");
        assert_eq!(
            *connect_order.lock().expect("connect-order lock"),
            vec![first, second],
            "the third snapshot candidate is outside the two-connect ceiling"
        );
        {
            let delivered = delivered.lock().expect("delivery lock");
            assert_eq!(delivered.len(), 1);
            assert_eq!(delivered[0].0, (second, 22));
            assert_eq!((delivered[0].1, delivered[0].2), (second, 22));
            assert!(!delivered[0].3.is_empty());
            assert_eq!(delivered[0].4, expected_participants);
        }

        let ambiguous_connect_order = Arc::new(Mutex::new(Vec::new()));
        let ambiguous_deliveries = Arc::new(Mutex::new(Vec::new()));
        let (context, request) = query_input();
        let error = route_remote_once(
            &snapshot,
            candidates(),
            Instant::now() + Duration::from_secs(1),
            ForwardingAttempt {
                context,
                request,
                query_class: QueryClass::Interactive,
                now: observed_at,
                wall_deadline,
            },
            {
                let ambiguous_connect_order = Arc::clone(&ambiguous_connect_order);
                move |node_id, _address| {
                    ambiguous_connect_order
                        .lock()
                        .expect("ambiguous connect-order lock")
                        .push(node_id);
                    async { Ok(()) }
                }
            },
            |claims| Ok(claims.clone()),
            {
                let ambiguous_deliveries = Arc::clone(&ambiguous_deliveries);
                move |leader, (), envelope: ForwardQueryClaims, _visibility| {
                    ambiguous_deliveries
                        .lock()
                        .expect("ambiguous delivery lock")
                        .push((leader, envelope.participant_cut_fingerprint.clone()));
                    async { Err::<(), _>(BifrostError::QueryExecutionFailed) }
                }
            },
        )
        .await;
        assert!(matches!(error, Err(BifrostError::QueryExecutionFailed)));
        assert_eq!(
            *ambiguous_connect_order
                .lock()
                .expect("ambiguous connect-order lock"),
            vec![first],
            "delivery ambiguity cannot connect another candidate"
        );
        assert_eq!(
            ambiguous_deliveries
                .lock()
                .expect("ambiguous delivery lock")
                .len(),
            1,
            "the one envelope is delivered once and never replayed"
        );
        assert_eq!(
            ambiguous_deliveries
                .lock()
                .expect("ambiguous delivery lock")[0]
                .0,
            (first, 11)
        );
    }
}
