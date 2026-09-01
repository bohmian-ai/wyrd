//! Peer workload authentication, and the ordering it must hold against decode.

use ed25519_dalek::{Signer as _, SigningKey};
use secrecy::ExposeSecret as _;
use vala_bifrost_redux::oracle::peer::{
    ReservationBinding, ReservationOperationV1, ReservationTicketClaims, peer_signing_input,
    reservation_body_digest,
};
use wyrd_spec::vala::api::NodeId;
use wyrd_testing::bifrost::peer_keyring::TestPeerKeyring;
use wyrd_testing::bifrost::process_cluster::{
    BifrostProcessCluster, MembershipEntry, PeerProbeCredential, PeerProbeFraming, PeerProbePlan,
    PeerProbeService, ProcessNodeTarget,
};
use wyrd_testing::server::PeerPrincipalShape;
use wyrd_tonic::prost::Message as _;
use wyrd_tonic::wyrd::v1 as proto;

use super::support::PeerJourneyError;

/// Path of the compiled child every simulated pod runs.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

/// Both private adapters share one authentication layer that runs to a verdict
/// before any request body is polled, admits exactly the configured peer
/// Service principal, and is indifferent to first-frame layout.
///
/// # Panics
///
/// Panics when any scenario in the table fails, naming the scenario.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn peer_authentication_precedes_body_admission() {
    prove_peer_authentication_precedes_body_admission()
        .await
        .expect("peer authentication journey");
}

/// Drives every peer-authentication scenario against one live topology.
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_peer_authentication_precedes_body_admission() -> Result<(), PeerJourneyError> {
    // Two Oracles so one pod probes another over the real peer listener, and
    // one Scribe so the topology owns a catalog and a tail source exactly as a
    // deployment that serves queries does.
    let mut cluster = BifrostProcessCluster::start(
        NODE_BINARY,
        &[
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Scribe,
        ],
    )
    .await?;

    let wrong = WrongPrincipals::provision(&cluster).await?;
    let destination = cluster.nodes()[1].ready_report().advertise_addr.clone();

    refused_identities_never_reach_a_body(&mut cluster, &destination, &wrong)?;
    the_configured_principal_is_admitted_on_both_adapters(&mut cluster, &destination)?;
    first_frame_layout_does_not_change_admission(&mut cluster, &destination)?;

    cluster.shutdown();
    Ok(())
}

/// The deliberately wrong peer credentials one journey seeds.
///
/// Each is a real, exchangeable API key: the refusal under test is an
/// authorization verdict on a verified principal, not a malformed token.
struct WrongPrincipals {
    /// A different SYSTEM_OWNER service that also holds the peer permission.
    alternate: String,
    /// A SYSTEM_OWNER service holding no peer permission.
    unpermitted: String,
    /// A data-tenant service holding the peer permission.
    tenant_scoped: String,
}

impl WrongPrincipals {
    /// Seeds all three near-miss principals against the shared fixture.
    ///
    /// # Errors
    ///
    /// Returns the provisioning failure unchanged.
    async fn provision(cluster: &BifrostProcessCluster) -> Result<Self, PeerJourneyError> {
        let alternate = cluster
            .provision_peer_principal(PeerPrincipalShape::AlternateService)
            .await?;
        let unpermitted = cluster
            .provision_peer_principal(PeerPrincipalShape::WithoutPermission)
            .await?;
        let tenant_scoped = cluster
            .provision_peer_principal(PeerPrincipalShape::TenantScoped)
            .await?;
        Ok(Self {
            alternate: alternate.expose_secret().to_owned(),
            unpermitted: unpermitted.expose_secret().to_owned(),
            tenant_scoped: tenant_scoped.expose_secret().to_owned(),
        })
    }

    /// Returns each wrong identity with the refusal it must produce.
    fn cases(&self) -> Vec<(&'static str, PeerProbeCredential, &'static str)> {
        vec![
            (
                "no credential at all",
                PeerProbeCredential::Absent,
                "Unauthenticated",
            ),
            (
                "an unverifiable bearer",
                PeerProbeCredential::Invalid,
                "Unauthenticated",
            ),
            (
                "a different platform service",
                PeerProbeCredential::ApiKey(self.alternate.clone()),
                "PermissionDenied",
            ),
            (
                "a platform service without the peer permission",
                PeerProbeCredential::ApiKey(self.unpermitted.clone()),
                "PermissionDenied",
            ),
            (
                "a data-tenant service holding the peer permission",
                PeerProbeCredential::ApiKey(self.tenant_scoped.clone()),
                "PermissionDenied",
            ),
        ]
    }
}

/// Every private adapter this target serves.
const ADAPTERS: [PeerProbeService; 2] = [
    PeerProbeService::OraclePeer,
    PeerProbeService::AnalyticalWorker,
];

/// Reads the destination's body-poll counter through its own control channel.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
fn body_polls(cluster: &mut BifrostProcessCluster) -> Result<u64, PeerJourneyError> {
    Ok(cluster.nodes_mut()[1].peer_body_polls()?)
}

/// Sends one probe from the first pod and reports the destination's outcome.
///
/// # Errors
///
/// Returns the control-protocol failure unchanged.
fn probe(
    cluster: &mut BifrostProcessCluster,
    plan: &PeerProbePlan,
) -> Result<String, PeerJourneyError> {
    Ok(cluster.nodes_mut()[0].peer_probe(plan)?)
}

/// No unauthorized identity causes the destination to poll a request body.
///
/// This is the ordering claim itself: authentication runs to a verdict on
/// metadata alone, so a refused caller never reaches transport admission, the
/// frame parser, or a protobuf decode on either adapter.
///
/// # Errors
///
/// Returns a message naming the adapter and identity that broke the claim.
fn refused_identities_never_reach_a_body(
    cluster: &mut BifrostProcessCluster,
    destination: &str,
    wrong: &WrongPrincipals,
) -> Result<(), PeerJourneyError> {
    for adapter in ADAPTERS.iter() {
        for (description, credential, expected) in wrong.cases() {
            let before = body_polls(cluster)?;
            let plan = PeerProbePlan::own(destination)
                .against(adapter.clone())
                .presenting(credential);
            let outcome = probe(cluster, &plan)?;
            if outcome != expected {
                return Err(format!(
                    "{adapter:?} answered {description} with {outcome}, expected {expected}"
                )
                .into());
            }
            let after = body_polls(cluster)?;
            if after != before {
                return Err(format!(
                    "{adapter:?} polled {} request bodies for {description}",
                    after - before
                )
                .into());
            }
        }
    }
    Ok(())
}

/// The one configured peer Service principal is admitted on both adapters.
///
/// Admission is observed as a body poll the refused cases never produce: the
/// request reached the plane that decodes it. The verdict cannot carry the
/// claim on either adapter, because both apply a second, independent
/// operation-authority check to the decoded message — a stage ticket on the
/// worker adapter, a reservation purpose ticket on the Oracle one — and this
/// probe deliberately carries neither. Body polls are what separate an
/// identity refusal, which never reaches a body, from an operation refusal,
/// which necessarily has.
///
/// # Errors
///
/// Returns a message naming the adapter that refused the configured identity.
fn the_configured_principal_is_admitted_on_both_adapters(
    cluster: &mut BifrostProcessCluster,
    destination: &str,
) -> Result<(), PeerJourneyError> {
    for adapter in ADAPTERS.iter() {
        let before = body_polls(cluster)?;
        probe(
            cluster,
            &PeerProbePlan::own(destination).against(adapter.clone()),
        )?;
        if body_polls(cluster)? == before {
            return Err(format!(
                "{adapter:?} admitted the configured peer principal without reaching a body"
            )
            .into());
        }
    }
    Ok(())
}

/// A split or coalesced first frame is admitted exactly as a whole one is.
///
/// HTTP/2 never promises that a gRPC header arrives alone in its own DATA
/// frame, so a private listener that authenticates before decoding must not
/// acquire a different verdict from the same identity because of layout.
///
/// # Errors
///
/// Returns a message naming the layout whose verdict diverged.
fn first_frame_layout_does_not_change_admission(
    cluster: &mut BifrostProcessCluster,
    destination: &str,
) -> Result<(), PeerJourneyError> {
    let baseline = probe(cluster, &PeerProbePlan::own(destination))?;
    for framing in [PeerProbeFraming::SplitHeader, PeerProbeFraming::Coalesced] {
        let outcome = probe(cluster, &PeerProbePlan::own(destination).framed(framing))?;
        if outcome != baseline {
            return Err(format!(
                "a {framing:?} first frame answered {outcome}, but a whole frame answered {baseline}"
            )
            .into());
        }
    }
    Ok(())
}

/// Reservation authority is an independent, rotatable, exactly-bound,
/// single-use credential: a workload token cannot stand in for it, only
/// published keys verify, every bound identity must match, and no
/// state-changing operation may be replayed.
///
/// # Panics
///
/// Panics when any scenario in the table fails, naming the scenario.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn peer_tickets_are_independent_exact_and_replay_safe() {
    prove_peer_tickets_are_independent_exact_and_replay_safe()
        .await
        .expect("peer ticket journey");
}

/// Drives every reservation-authority scenario against one live topology.
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_peer_tickets_are_independent_exact_and_replay_safe() -> Result<(), PeerJourneyError>
{
    let mut cluster = BifrostProcessCluster::start(
        NODE_BINARY,
        &[
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Scribe,
        ],
    )
    .await?;

    let plane = ReservationPlane::observe(&mut cluster)?;
    let keyring = KeyringSigners::from(cluster.peer_keyring());

    only_published_unexpired_keys_authorize(&mut cluster, &plane, &keyring)?;
    every_bound_identity_must_match(&mut cluster, &plane, &keyring)?;
    a_state_changing_ticket_is_single_use(&mut cluster, &plane, &keyring)?;
    worker_discovery_is_always_refused(&mut cluster, &plane)?;

    cluster.shutdown();
    Ok(())
}

/// The two fenced Oracle incarnations one reservation travels between.
///
/// Read from live membership rather than assumed, because a reservation ticket
/// binds both incarnations and a stale fence on either side is one of the
/// refusals under test.
struct ReservationPlane {
    /// Address the follower advertises on the private plane.
    destination: String,
    /// Leader node identity minting the tickets.
    leader_node_id: uuid::Uuid,
    /// Leader Oracle fence at observation time.
    leader_fence: u64,
    /// Follower node identity the tickets are addressed to.
    follower_node_id: uuid::Uuid,
    /// Follower Oracle fence at observation time.
    follower_fence: u64,
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
    fn observe(cluster: &mut BifrostProcessCluster) -> Result<Self, PeerJourneyError> {
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
    fn reserve_binding(&self, query_id: uuid::Uuid) -> ReservationBinding {
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
    fn reserve_request(&self, query_id: uuid::Uuid) -> proto::ReserveNodeSlotsRequest {
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
struct KeyringSigners {
    /// Currently issuing key; every correct ticket is signed with it.
    active: (String, SigningKey),
    /// Retired key still inside its published verification window.
    retired_valid: (String, SigningKey),
    /// Retired key whose verification window has closed.
    retired_expired: (String, SigningKey),
    /// Well-formed key that appears in no manifest.
    unpublished: (String, SigningKey),
}

impl KeyringSigners {
    /// Copies every rotation-state key out of the cluster's shared keyring.
    fn from(keyring: &TestPeerKeyring) -> Self {
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
fn sign_ticket(
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
fn stamped(
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
fn reserve(
    cluster: &mut BifrostProcessCluster,
    destination: &str,
    payload: Vec<u8>,
) -> Result<String, PeerJourneyError> {
    probe(cluster, &PeerProbePlan::own(destination).carrying(payload))
}

/// Only a published, unexpired key authorizes a reservation.
///
/// This is the rotation claim and the independence claim in one table: the
/// active and the still-valid retired key are accepted, while an expired key,
/// a key no manifest publishes, and the deployment's own workload signing key
/// are all refused. The workload case is the one that would silently pass if
/// peer authority were ever folded back into the north-south key.
///
/// # Errors
///
/// Returns a message naming the key whose verdict broke the claim.
fn only_published_unexpired_keys_authorize(
    cluster: &mut BifrostProcessCluster,
    plane: &ReservationPlane,
    keyring: &KeyringSigners,
) -> Result<(), PeerJourneyError> {
    let cases: [(&str, &(String, SigningKey), bool); 4] = [
        ("the active key", &keyring.active, true),
        (
            "a retired key inside its window",
            &keyring.retired_valid,
            true,
        ),
        (
            "a retired key past its window",
            &keyring.retired_expired,
            false,
        ),
        ("a key no manifest publishes", &keyring.unpublished, false),
    ];
    for (description, key, accepted) in cases {
        let query_id = uuid::Uuid::new_v4();
        let binding = plane.reserve_binding(query_id);
        let payload = stamped(plane.reserve_request(query_id), |digest| {
            sign_ticket(key, &binding, digest)
        })?;
        let outcome = reserve(cluster, &plane.destination, payload)?;
        let refused = outcome == "PermissionDenied" || outcome == "Unauthenticated";
        if accepted && refused {
            return Err(format!("{description} was refused with {outcome}").into());
        }
        if !accepted && !refused {
            return Err(format!("{description} was accepted with {outcome}").into());
        }
    }
    Ok(())
}

/// One field-level change applied to an otherwise correct reservation binding.
///
/// Boxed so the deviation table can hold closures that capture different
/// values while staying one homogeneous list of attributable single-field
/// changes.
type BindingDeviation = Box<dyn Fn(&mut ReservationBinding)>;

/// Every identity a ticket binds must match the receiver's own derivation.
///
/// Each case changes exactly one bound value away from the correct ticket, so
/// each refusal is attributable: nothing here is refused because two things
/// were wrong at once.
///
/// # Errors
///
/// Returns a message naming the deviation the follower accepted.
fn every_bound_identity_must_match(
    cluster: &mut BifrostProcessCluster,
    plane: &ReservationPlane,
    keyring: &KeyringSigners,
) -> Result<(), PeerJourneyError> {
    let foreign = uuid::Uuid::new_v4();
    let deviations: Vec<(&str, BindingDeviation)> = vec![
        (
            "a ticket addressed to another follower",
            Box::new(move |binding: &mut ReservationBinding| {
                binding.destination_node_id = NodeId::new(foreign);
            }),
        ),
        (
            "a ticket carrying a stale follower fence",
            Box::new(|binding: &mut ReservationBinding| {
                binding.destination_fence = binding.destination_fence.wrapping_add(1);
            }),
        ),
        (
            "a ticket claiming another leader",
            Box::new(move |binding: &mut ReservationBinding| {
                binding.source_node_id = NodeId::new(foreign);
            }),
        ),
        (
            "a ticket carrying a stale leader fence",
            Box::new(|binding: &mut ReservationBinding| {
                binding.source_fence = binding.source_fence.wrapping_add(1);
            }),
        ),
        (
            "a ticket minted for another query",
            Box::new(move |binding: &mut ReservationBinding| {
                binding.query_id = foreign;
            }),
        ),
        (
            "a ticket minted for the release operation",
            Box::new(|binding: &mut ReservationBinding| {
                binding.operation = ReservationOperationV1::ReleaseSlots;
            }),
        ),
    ];
    for (description, deviate) in deviations {
        let query_id = uuid::Uuid::new_v4();
        let mut binding = plane.reserve_binding(query_id);
        deviate(&mut binding);
        let payload = stamped(plane.reserve_request(query_id), |digest| {
            sign_ticket(&keyring.active, &binding, digest)
        })?;
        let outcome = reserve(cluster, &plane.destination, payload)?;
        if outcome != "PermissionDenied" && outcome != "Unauthenticated" {
            return Err(format!("{description} was answered with {outcome}").into());
        }
    }

    // A correct ticket presented on a substituted request: the signature and
    // every identity still verify, and only the body digest does not.
    let query_id = uuid::Uuid::new_v4();
    let binding = plane.reserve_binding(query_id);
    let mut request = plane.reserve_request(query_id);
    let digest = reservation_body_digest(&request.encode_to_vec())
        .map_err(|error| format!("reserve body digest: {error}"))?;
    request.slot_units = 64;
    request.ticket = Some(sign_ticket(&keyring.active, &binding, digest));
    let outcome = reserve(cluster, &plane.destination, request.encode_to_vec())?;
    if outcome != "PermissionDenied" && outcome != "Unauthenticated" {
        return Err(format!("a substituted request body was answered with {outcome}").into());
    }
    Ok(())
}

/// A reservation ticket authorizes exactly one state change.
///
/// The same bytes are sent twice. The first send is the accepted case proved
/// above; the second must be refused on the nonce before any capacity moves,
/// which is what stops a captured reserve from charging a follower repeatedly.
///
/// # Errors
///
/// Returns a message naming the replay the follower accepted.
fn a_state_changing_ticket_is_single_use(
    cluster: &mut BifrostProcessCluster,
    plane: &ReservationPlane,
    keyring: &KeyringSigners,
) -> Result<(), PeerJourneyError> {
    let query_id = uuid::Uuid::new_v4();
    let binding = plane.reserve_binding(query_id);
    let payload = stamped(plane.reserve_request(query_id), |digest| {
        sign_ticket(&keyring.active, &binding, digest)
    })?;
    let first = reserve(cluster, &plane.destination, payload.clone())?;
    if first == "PermissionDenied" || first == "Unauthenticated" {
        return Err(format!("a correct reserve ticket was refused with {first}").into());
    }
    let replayed = reserve(cluster, &plane.destination, payload)?;
    if replayed != "PermissionDenied" && replayed != "Unauthenticated" {
        return Err(format!("a replayed reserve ticket was answered with {replayed}").into());
    }
    Ok(())
}

/// Worker discovery is refused on the private plane and has no internal caller.
///
/// The path carries no graph identity, so it can be bound to no ticket and
/// authorized by nothing. Wyrd pins one worker build across a topology, so
/// there is nothing to discover; the coordinator side refuses to originate the
/// call and the listener refuses to answer it.
///
/// # Errors
///
/// Returns a message when the private listener answers the call.
fn worker_discovery_is_always_refused(
    cluster: &mut BifrostProcessCluster,
    plane: &ReservationPlane,
) -> Result<(), PeerJourneyError> {
    let outcome = probe(
        cluster,
        &PeerProbePlan::own(&plane.destination).on_path("/worker.WorkerService/GetWorkerInfo"),
    )?;
    if outcome == "Ok" {
        return Err("worker discovery was answered on the private peer plane".into());
    }
    Ok(())
}
