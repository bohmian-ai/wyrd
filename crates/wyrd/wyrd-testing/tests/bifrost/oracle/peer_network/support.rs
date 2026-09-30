//! Fixtures shared by the peer-network journeys.
//!
//! Everything here dials a running child over a real socket. Nothing composes
//! a server in-process, because a peer-plane claim proved against an
//! in-process router is not a claim about the deployed trust boundary.

use std::net::SocketAddr;

use vala_bifrost_redux::oracle::peer::{
    ReservationBinding, ReservationOperationV1, ReservationTicketClaims, reservation_body_digest,
};
use wyrd_spec::vala::api::NodeId;
use wyrd_testing::bifrost::process_cluster::{
    BifrostProcessCluster, MembershipEntry, PeerProbePlan,
};
use wyrd_tonic::prost::Message as _;
use wyrd_tonic::wyrd::v1 as proto;

use wyrd_testing::bifrost::peer_ca::{BifrostPeerCa, BifrostPeerLeaf};
use wyrd_testing::bifrost::process_cluster::ProcessNodeTarget;
use wyrd_tonic::tonic;
use wyrd_tonic::tonic::transport::Channel;

/// Boxed error carried by every peer-network fixture that can fail.
pub(crate) type PeerJourneyError = Box<dyn std::error::Error + Send + Sync>;

/// How long a peer-plane dial is allowed to take before it counts as refused.
///
/// Short on purpose: every scenario here either connects promptly on loopback
/// or is expected to be rejected, so a long wait only turns a clear refusal
/// into a slow one.
const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The client identity one dial presents to a peer listener.
///
/// Modelled as a closed set because these are exactly the trust outcomes the
/// listener must distinguish: a member of its own authority, a well-formed
/// identity from a foreign authority, an expired member, a same-authority leaf
/// for another name, and no identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialIdentity {
    /// A leaf issued by the cluster's own peer authority.
    Member,
    /// A leaf issued by an authority the cluster does not trust.
    Foreign,
    /// No client certificate, leaving the transport server-authenticated only.
    Anonymous,
    /// A leaf from the cluster's own authority whose validity window has closed.
    Expired,
    /// A valid leaf from the cluster's own authority for a name other than the
    /// fixed peer identity.
    Misnamed,
}

/// One dial against a child's private peer socket.
///
/// Owns the trust decisions a peer client makes — which root it verifies, which
/// name it requires, and which identity it presents — so a scenario states the
/// deviation it is testing and nothing else.
pub(crate) struct PeerDial<'a> {
    /// Authority whose leaves the cluster admits.
    authority: &'a BifrostPeerCa,
    /// Socket to dial.
    address: SocketAddr,
    /// Identity presented to the listener.
    identity: DialIdentity,
    /// Name the served certificate must carry, when it differs from the shared one.
    expected_server_name: Option<String>,
}

impl<'a> PeerDial<'a> {
    /// Starts a dial that a healthy peer listener must accept.
    #[must_use]
    pub(crate) fn member(authority: &'a BifrostPeerCa, address: SocketAddr) -> Self {
        Self {
            authority,
            address,
            identity: DialIdentity::Member,
            expected_server_name: None,
        }
    }

    /// Presents the named identity instead of a trusted member leaf.
    #[must_use]
    pub(crate) fn with_identity(mut self, identity: DialIdentity) -> Self {
        self.identity = identity;
        self
    }

    /// Requires a different name on the served certificate than the real one.
    #[must_use]
    pub(crate) fn expecting_server_name(mut self, server_name: &str) -> Self {
        self.expected_server_name = Some(server_name.to_owned());
        self
    }

    /// Completes the TLS handshake and returns the connected channel.
    ///
    /// # Errors
    ///
    /// Returns the transport failure when the endpoint cannot be built or the
    /// handshake is refused, which is the observable outcome every negative
    /// scenario asserts on.
    pub(crate) async fn connect(&self) -> Result<Channel, PeerJourneyError> {
        let server_name = self
            .expected_server_name
            .clone()
            .unwrap_or_else(|| self.authority.server_name().to_owned());
        let address = format!("https://{}", self.address);
        let root = self.authority.ca_certificate_pem().as_bytes().to_vec();
        let endpoint = match self.client_leaf()? {
            Some(leaf) => wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
                address,
                &root,
                server_name,
                leaf.certificate_pem().as_bytes(),
                leaf.private_key_pem().as_bytes(),
            )?,
            None => wyrd_tonic::transport::authenticated_tls_endpoint(address, &root, server_name)?,
        };
        Ok(endpoint
            .connect_timeout(DIAL_TIMEOUT)
            .timeout(DIAL_TIMEOUT)
            .connect()
            .await?)
    }

    /// Issues the client leaf this dial presents, if any.
    ///
    /// # Errors
    ///
    /// Returns a generation failure when the foreign authority or either leaf
    /// cannot be minted.
    fn client_leaf(&self) -> Result<Option<BifrostPeerLeaf>, PeerJourneyError> {
        match self.identity {
            DialIdentity::Member => Ok(Some(self.authority.issue_leaf("journey-member")?)),
            DialIdentity::Foreign => {
                // A complete, well-formed identity that simply chains elsewhere:
                // the listener must reject it on trust, not on shape.
                let foreign = BifrostPeerCa::generate(self.authority.server_name())?;
                Ok(Some(foreign.issue_leaf("journey-foreign")?))
            }
            DialIdentity::Anonymous => Ok(None),
            DialIdentity::Expired => {
                Ok(Some(self.authority.issue_expired_leaf("journey-expired")?))
            }
            DialIdentity::Misnamed => Ok(Some(
                self.authority
                    .issue_misnamed_leaf("journey-misnamed", "not-wyrd-peer.invalid")?,
            )),
        }
    }
}

/// Calls the cheapest `OraclePeerService` operation and returns its outcome.
///
/// `ReserveSlots` is unary and side-effect free when it is refused, so it is
/// the right probe for asking whether the service is mounted here and whether
/// this caller is authorized at all.
///
/// # Errors
///
/// Returns the gRPC status the peer answered with.
pub(crate) async fn probe_oracle_peer(channel: Channel) -> Result<(), tonic::Status> {
    let mut client =
        wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient::new(channel);
    client
        .reserve_slots(wyrd_tonic::wyrd::v1::ReserveNodeSlotsRequest::default())
        .await
        .map(|_| ())
}

/// Calls the cheapest `ScribeTailService` operation and returns its outcome.
///
/// # Errors
///
/// Returns the gRPC status the peer answered with.
pub(crate) async fn probe_scribe_tail(channel: Channel) -> Result<(), tonic::Status> {
    let mut client =
        wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient::new(channel);
    client
        .list_active_streams(wyrd_tonic::wyrd::v1::ListActiveStreamsRequest::default())
        .await
        .map(|_| ())
}

/// Calls the cheapest `OracleLifecycleService` operation and returns its outcome.
///
/// # Errors
///
/// Returns the gRPC status the peer answered with.
pub(crate) async fn probe_oracle_lifecycle(channel: Channel) -> Result<(), tonic::Status> {
    let mut client =
        wyrd_tonic::wyrd::v1::oracle_lifecycle_service_client::OracleLifecycleServiceClient::new(
            channel,
        );
    client
        .list_lifecycles(wyrd_tonic::wyrd::v1::ListOracleLifecyclesRequest::default())
        .await
        .map(|_| ())
}

/// Reports whether a target composes a public serving listener.
///
/// A dedicated Forge worker composes neither HTTP nor public gRPC: it pulls
/// work from Postgres and object storage and answers no caller.
#[must_use]
pub(crate) fn target_serves_public_listener(target: ProcessNodeTarget) -> bool {
    target != ProcessNodeTarget::ForgeWorker
}

/// Reports whether a target composes a private peer plane at all.
///
/// A Forge worker owns neither a Scribe nor an Oracle, so it has nothing to
/// answer on the peer plane and must not open a listener for it.
#[must_use]
pub(crate) fn target_serves_peer_plane(target: ProcessNodeTarget) -> bool {
    matches!(
        target,
        ProcessNodeTarget::All
            | ProcessNodeTarget::Server
            | ProcessNodeTarget::Oracle
            | ProcessNodeTarget::Scribe
    )
}

/// Reads how many request bodies the pod at `index` has polled on its peer plane.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
pub(crate) fn polls_at(
    cluster: &mut BifrostProcessCluster,
    index: usize,
) -> Result<u64, PeerJourneyError> {
    Ok(cluster.nodes_mut()[index].peer_body_polls()?)
}

/// Sends one probe from the pod at `index` and reports the destination's outcome.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
pub(crate) fn probe_from(
    cluster: &mut BifrostProcessCluster,
    index: usize,
    plan: &PeerProbePlan,
) -> Result<String, PeerJourneyError> {
    Ok(cluster.nodes_mut()[index].peer_probe(plan)?)
}

/// Sends one probe from the first pod and reports the destination's outcome.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
pub(crate) fn probe(
    cluster: &mut BifrostProcessCluster,
    plan: &PeerProbePlan,
) -> Result<String, PeerJourneyError> {
    probe_from(cluster, 0, plan)
}

/// The two fenced Oracle incarnations one reservation travels between.
///
/// Read from live membership rather than assumed, because a reservation ticket
/// binds both incarnations and a stale fence on either side is one of the
/// refusals under test.
pub(crate) struct ReservationPlane {
    /// Address the follower advertises on the private plane.
    pub(crate) destination: String,
    /// Leader node identity minting the tickets.
    pub(crate) leader_node_id: uuid::Uuid,
    /// Leader Oracle fence at observation time.
    pub(crate) leader_fence: u64,
    /// Follower node identity the tickets are addressed to.
    pub(crate) follower_node_id: uuid::Uuid,
    /// Follower Oracle fence at observation time.
    pub(crate) follower_fence: u64,
}

impl ReservationPlane {
    /// Projects both Oracle incarnations out of a freshly taken membership cut.
    ///
    /// The cut is re-inspected rather than read from the ready report: the
    /// leader answered readiness before the follower had registered, so its
    /// startup report knows only itself. A reservation ticket binds both
    /// fences, so the observation has to be as live as the tickets it feeds.
    ///
    /// # Errors
    ///
    /// Returns a message naming the role that is absent from membership.
    pub(crate) fn observe(cluster: &mut BifrostProcessCluster) -> Result<Self, PeerJourneyError> {
        let leader = cluster.nodes_mut()[0].inspect()?;
        let follower_node_id = cluster.nodes()[1].ready_report().node_id;
        let oracle = |node_id: uuid::Uuid| -> Result<&MembershipEntry, PeerJourneyError> {
            leader
                .membership
                .iter()
                .find(|entry| entry.node_id == node_id && entry.role == "oracle")
                .ok_or_else(|| format!("no live Oracle lease for {node_id}").into())
        };
        let follower = oracle(follower_node_id)?;
        Ok(Self {
            destination: follower.address.clone(),
            follower_fence: follower.fencing_token,
            follower_node_id,
            leader_fence: oracle(leader.node_id)?.fencing_token,
            leader_node_id: leader.node_id,
        })
    }

    /// Returns the binding a correct reserve context must carry.
    pub(crate) fn reserve_binding(&self, query_id: uuid::Uuid) -> ReservationBinding {
        ReservationBinding {
            operation: ReservationOperationV1::ReserveSlots,
            source_node_id: NodeId::new(self.leader_node_id),
            source_fence: self.leader_fence,
            destination_node_id: NodeId::new(self.follower_node_id),
            destination_fence: self.follower_fence,
            query_id,
        }
    }

    /// Returns the context-free reserve request the leader would send.
    pub(crate) fn reserve_request(&self, query_id: uuid::Uuid) -> proto::ReserveNodeSlotsRequest {
        proto::ReserveNodeSlotsRequest {
            query_id: query_id.as_bytes().to_vec(),
            leader_node_id: self.leader_node_id.to_string(),
            leader_fencing_token: self.leader_fence,
            expires_at_unix_ms: u64::try_from(
                (chrono::Utc::now() + chrono::Duration::seconds(30)).timestamp_millis(),
            )
            .unwrap_or_default(),
            context: None,
            graph: Some(proto::AnalyticalGraphRef {
                public_query_id: query_id.as_bytes().to_vec(),
                datafusion_query_id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            }),
        }
    }
}

/// Attaches the leader's context for `binding` and `body_digest` to `request`.
pub(crate) fn proto_with_context(
    request: proto::ReserveNodeSlotsRequest,
    binding: &ReservationBinding,
    body_digest: String,
) -> proto::ReserveNodeSlotsRequest {
    with_claims(
        request,
        &ReservationTicketClaims::for_binding(binding, body_digest, context_expiry()),
    )
}

/// Encodes one reserve request carrying the leader's context for `binding`.
///
/// The digest is taken over the context-free encoding, exactly what the
/// follower recomputes. `deviate` edits the claims after they are built so a
/// scenario can present an expired or incompatible context the leader would
/// never send.
///
/// # Errors
///
/// Returns the digest failure unchanged.
pub(crate) fn stamped(
    mut request: proto::ReserveNodeSlotsRequest,
    binding: &ReservationBinding,
    deviate: impl FnOnce(&mut ReservationTicketClaims),
) -> Result<Vec<u8>, PeerJourneyError> {
    request.context = None;
    let digest = reservation_body_digest(&request.encode_to_vec())
        .map_err(|error| format!("reserve body digest: {error}"))?;
    let mut claims = ReservationTicketClaims::for_binding(binding, digest, context_expiry());
    deviate(&mut claims);
    Ok(with_claims(request, &claims).encode_to_vec())
}

/// Returns the acceptance expiry a freshly built context carries.
fn context_expiry() -> i64 {
    (chrono::Utc::now() + chrono::Duration::seconds(10)).timestamp_millis()
}

/// Encodes `claims` as the request's context for the binding's operation.
///
/// Encoded directly rather than through `to_context`, which refuses claims
/// whose operation differs from the one named: a scenario presenting a
/// context built for another operation needs exactly those bytes on the wire.
///
fn with_claims(
    mut request: proto::ReserveNodeSlotsRequest,
    claims: &ReservationTicketClaims,
) -> proto::ReserveNodeSlotsRequest {
    request.context = Some(proto::PeerContext {
        claims_bytes: claims.encode_to_vec(),
    });
    request
}

/// Sends one reserve payload from the leader and returns the follower's verdict.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
pub(crate) fn reserve(
    cluster: &mut BifrostProcessCluster,
    destination: &str,
    payload: Vec<u8>,
) -> Result<String, PeerJourneyError> {
    probe(cluster, &PeerProbePlan::own(destination).carrying(payload))
}
