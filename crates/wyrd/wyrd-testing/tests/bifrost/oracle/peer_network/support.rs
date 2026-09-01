//! Fixtures shared by the peer-network journeys.
//!
//! Everything here dials a running child over a real socket. Nothing composes
//! a server in-process, because a peer-plane claim proved against an
//! in-process router is not a claim about the deployed trust boundary.

use std::net::SocketAddr;

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
