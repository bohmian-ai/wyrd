//! Fixtures shared by the peer-network journeys.
//!
//! Everything here dials a running child over a real socket. Nothing composes
//! a server in-process, because a peer-plane claim proved against an
//! in-process router is not a claim about the deployed trust boundary.

use std::net::SocketAddr;

use ed25519_dalek::{Signer as _, SigningKey};
use vala_bifrost_redux::oracle::peer::{
    ReservationBinding, ReservationOperationV1, ReservationTicketClaims, peer_signing_input,
    reservation_body_digest,
};
use wyrd_spec::vala::api::NodeId;
use wyrd_testing::bifrost::peer_keyring::TestPeerKeyring;
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
/// identity from a foreign authority, and no identity at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialIdentity {
    /// A leaf issued by the cluster's own peer authority.
    Member,
    /// A leaf issued by an authority the cluster does not trust.
    Foreign,
    /// No client certificate, leaving the transport server-authenticated only.
    Anonymous,
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

    /// Returns the binding a correct reserve ticket must carry.
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

    /// Returns the ticket-free reserve request the leader would send.
    pub(crate) fn reserve_request(&self, query_id: uuid::Uuid) -> proto::ReserveNodeSlotsRequest {
        proto::ReserveNodeSlotsRequest {
            query_id: query_id.as_bytes().to_vec(),
            leader_node_id: self.leader_node_id.to_string(),
            leader_fencing_token: self.leader_fence,
            query_class: proto::QueryClass::Interactive as i32,
            slot_units: 1,
            expires_at_unix_ms: u64::try_from(
                (chrono::Utc::now() + chrono::Duration::seconds(30)).timestamp_millis(),
            )
            .unwrap_or_default(),
            ticket: None,
        }
    }
}

/// The signing keys one journey can present, by rotation state.
///
/// Held as raw signing keys rather than through the server authority because
/// the point of the table is to present keys the server would never issue: a
/// retired one, an expired one, one no manifest publishes, and the workload
/// key that must never be interchangeable with any of them.
pub(crate) struct KeyringSigners {
    /// Currently issuing key; every correct ticket is signed with it.
    pub(crate) active: (String, SigningKey),
    /// Retired key still inside its published verification window.
    pub(crate) retired_valid: (String, SigningKey),
    /// Retired key whose verification window has closed.
    pub(crate) retired_expired: (String, SigningKey),
    /// Well-formed key that appears in no manifest.
    pub(crate) unpublished: (String, SigningKey),
}

impl KeyringSigners {
    /// Copies every rotation-state key out of the cluster's shared keyring.
    pub(crate) fn from(keyring: &TestPeerKeyring) -> Self {
        let pair = |key: &wyrd_testing::bifrost::peer_keyring::TestPeerTicketKey| {
            (key.key_id().to_owned(), key.signing_key().clone())
        };
        Self {
            active: pair(keyring.active()),
            retired_valid: pair(keyring.retired_valid()),
            retired_expired: pair(keyring.retired_expired()),
            unpublished: pair(keyring.unpublished()),
        }
    }
}

/// Signs one reservation ticket with an arbitrary key.
///
/// Mirrors what the server authority does, but takes the key as an argument so
/// a scenario can present a retired, expired, unpublished, or foreign key
/// without the server ever agreeing to mint it.
pub(crate) fn sign_ticket(
    key: &(String, SigningKey),
    binding: &ReservationBinding,
    body_digest: String,
) -> proto::SignedPeerTicket {
    let claims = ReservationTicketClaims::for_binding(
        binding,
        body_digest,
        uuid::Uuid::new_v4().as_bytes().to_vec(),
        (chrono::Utc::now() + chrono::Duration::seconds(10)).timestamp_millis(),
    );
    let claims_bytes = claims.encode_to_vec();
    let signature = key
        .1
        .sign(&peer_signing_input(
            binding.operation.domain(),
            &key.0,
            &claims_bytes,
        ))
        .to_bytes()
        .to_vec();
    proto::SignedPeerTicket {
        key_id: key.0.clone(),
        claims_bytes,
        signature,
    }
}

/// Encodes one reserve request with `ticket` stamped onto it.
///
/// The digest is always taken over the ticket-free encoding, which is exactly
/// what the follower recomputes.
///
/// # Errors
///
/// Returns the digest failure unchanged.
pub(crate) fn stamped(
    mut request: proto::ReserveNodeSlotsRequest,
    ticket: impl FnOnce(String) -> proto::SignedPeerTicket,
) -> Result<Vec<u8>, PeerJourneyError> {
    request.ticket = None;
    let digest = reservation_body_digest(&request.encode_to_vec())
        .map_err(|error| format!("reserve body digest: {error}"))?;
    request.ticket = Some(ticket(digest));
    Ok(request.encode_to_vec())
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
