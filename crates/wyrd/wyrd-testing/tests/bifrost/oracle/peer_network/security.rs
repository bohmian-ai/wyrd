//! Peer receiver context checks on the mTLS-only private plane.

use vala_bifrost_redux::oracle::peer::{
    ReservationBinding, ReservationOperationV1, reservation_body_digest,
};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::vala::api::NodeId;
use wyrd_tonic::prost::Message as _;
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient;

use super::support::{
    PeerDial, PeerJourneyError, ReservationPlane, proto_with_context, reserve, stamped,
};
use crate::peer_cluster::{PeerCluster, PeerProbeFraming, PeerProbePlan, PeerProbeService};

/// A cluster member reaches both private adapters, and every operation it
/// sends is still checked against the receiver's own state before any
/// capacity moves: a forged, stale, expired, incompatible, or substituted
/// reservation context is refused while the correct one is accepted.
///
/// # Panics
///
/// Panics when any scenario in the table fails, naming the scenario.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn peer_context_refusals() {
    prove_peer_context_refusals()
        .await
        .expect("peer context journey");
}

/// Drives every receiver-context scenario against one live topology.
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_peer_context_refusals() -> Result<(), PeerJourneyError> {
    // Two Oracles so one pod calls another over the real peer listener, and
    // one Scribe so the topology owns a catalog and a tail source exactly as a
    // deployment that serves queries does.
    let cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;
    let destination = cluster.advertise_addr(1).await?;

    a_cluster_member_reaches_both_adapters(&cluster, &destination).await?;
    first_frame_layout_does_not_change_admission(&cluster, &destination).await?;
    let plane = ReservationPlane::observe(&cluster).await?;
    the_correct_context_is_accepted(&cluster, &plane).await?;
    every_bound_identity_must_match(&cluster, &plane).await?;
    worker_discovery_is_always_refused(&cluster, &plane).await?;

    cluster.shutdown().await?;
    Ok(())
}

/// Every private adapter this target serves.
const ADAPTERS: [PeerProbeService; 2] = [
    PeerProbeService::OraclePeer,
    PeerProbeService::AnalyticalWorker,
];

/// A certificate from the cluster authority reaches the body on both adapters.
///
/// mTLS is the only peer trust, so admission is observed as a body poll: the
/// request reached the plane that decodes and checks its typed context. The
/// verdict itself cannot carry the claim, because this probe deliberately
/// carries no operation context and each adapter refuses it after reading it.
/// The poll counter is process-wide; the idle topology sends no peer traffic
/// of its own, so the only request that can advance it is this probe.
///
/// # Errors
///
/// Returns a message naming the adapter that never reached a body.
async fn a_cluster_member_reaches_both_adapters(
    cluster: &PeerCluster,
    destination: &str,
) -> Result<(), PeerJourneyError> {
    for adapter in ADAPTERS.iter() {
        let before = cluster.peer_body_polls();
        cluster
            .probe(&PeerProbePlan::own(destination).against(adapter.clone()))
            .await?;
        if cluster.peer_body_polls() == before {
            return Err(
                format!("{adapter:?} admitted a cluster member without reaching a body").into(),
            );
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
async fn first_frame_layout_does_not_change_admission(
    cluster: &PeerCluster,
    destination: &str,
) -> Result<(), PeerJourneyError> {
    let baseline = cluster.probe(&PeerProbePlan::own(destination)).await?;
    for framing in [PeerProbeFraming::SplitHeader, PeerProbeFraming::Coalesced] {
        let outcome = cluster
            .probe(&PeerProbePlan::own(destination).framed(framing))
            .await?;
        if outcome != baseline {
            return Err(format!(
                "a {framing:?} first frame answered {outcome}, but a whole frame answered {baseline}"
            )
            .into());
        }
    }
    Ok(())
}

/// The reservation context the leader would send is accepted and released.
///
/// This is the baseline that makes every refusal below attributable to its
/// one deviation rather than to a context the follower never accepts. The
/// accepted reservation is then released exactly as its leader would release
/// it, so the follower returns every charged unit rather than holding a
/// reservation no leader will ever claim.
///
/// # Errors
///
/// Returns a message when the follower refuses the correct reserve or release
/// context, or still charges units after the release.
async fn the_correct_context_is_accepted(
    cluster: &PeerCluster,
    plane: &ReservationPlane,
) -> Result<(), PeerJourneyError> {
    let query_id = uuid::Uuid::new_v4();
    let request = proto::ReserveNodeSlotsRequest::decode(
        stamped(
            plane.reserve_request(query_id),
            &plane.reserve_binding(query_id),
            |_| {},
        )?
        .as_slice(),
    )?;
    let mut client = OraclePeerServiceClient::new(
        PeerDial::member(cluster.peer_ca(), cluster.peer_addr(1)?)
            .connect()
            .await?,
    );
    let accepted = client
        .reserve_slots(request)
        .await
        .map_err(|status| format!("a correct reserve context was refused with {status}"))?
        .into_inner();
    let Some(proto::reserve_node_slots_response::Outcome::Pending(pending)) = accepted.outcome
    else {
        return Err(format!("a correct reserve context was not held: {accepted:?}").into());
    };
    client
        .release_slots(plane.release_request(pending.reservation_id, query_id)?)
        .await
        .map_err(|status| format!("a correct release context was refused with {status}"))?;
    let held = cluster.ownership_snapshot(1)?.peer_running;
    if held != 0 {
        return Err(format!("the follower still charges {held} units after release").into());
    }
    Ok(())
}

/// Whether a probe outcome is a context refusal rather than an admission.
fn is_refusal(outcome: &str) -> bool {
    outcome == "PermissionDenied" || outcome == "Unauthenticated"
}

/// One field-level change applied to an otherwise correct reservation binding.
///
/// Boxed so the deviation table can hold closures that capture different
/// values while staying one homogeneous list of attributable single-field
/// changes.
type BindingDeviation = Box<dyn Fn(&mut ReservationBinding)>;

/// Every field a context binds must match the receiver's own derivation.
///
/// Each case changes exactly one value away from the correct context, so each
/// refusal is attributable: nothing here is refused because two things were
/// wrong at once. The expired and incompatible-version cases are claims-level
/// deviations; the rest change the binding the context was built from.
///
/// # Errors
///
/// Returns a message naming the deviation the follower accepted.
async fn every_bound_identity_must_match(
    cluster: &PeerCluster,
    plane: &ReservationPlane,
) -> Result<(), PeerJourneyError> {
    let foreign = uuid::Uuid::new_v4();
    let deviations: Vec<(&str, BindingDeviation)> = vec![
        (
            "a context addressed to another follower",
            Box::new(move |binding: &mut ReservationBinding| {
                binding.destination_node_id = NodeId::new(foreign);
            }),
        ),
        (
            "a context carrying a stale follower fence",
            Box::new(|binding: &mut ReservationBinding| {
                binding.destination_fence = binding.destination_fence.wrapping_add(1);
            }),
        ),
        (
            "a context claiming another leader",
            Box::new(move |binding: &mut ReservationBinding| {
                binding.source_node_id = NodeId::new(foreign);
            }),
        ),
        (
            "a context carrying a stale leader fence",
            Box::new(|binding: &mut ReservationBinding| {
                binding.source_fence = binding.source_fence.wrapping_add(1);
            }),
        ),
        (
            "a context built for another query",
            Box::new(move |binding: &mut ReservationBinding| {
                binding.query_id = foreign;
            }),
        ),
        (
            "a context built for the release operation",
            Box::new(|binding: &mut ReservationBinding| {
                binding.operation = ReservationOperationV1::ReleaseSlots;
            }),
        ),
    ];
    for (description, deviate) in deviations {
        let query_id = uuid::Uuid::new_v4();
        let mut binding = plane.reserve_binding(query_id);
        deviate(&mut binding);
        let payload = stamped(plane.reserve_request(query_id), &binding, |_| {})?;
        let outcome = reserve(cluster, &plane.destination, payload).await?;
        if !is_refusal(&outcome) {
            return Err(format!("{description} was answered with {outcome}").into());
        }
    }

    let claim_deviations: [(&str, ClaimsDeviation); 2] = [
        ("an expired context", |claims| {
            claims.expires_at_ms =
                (chrono::Utc::now() - chrono::Duration::seconds(1)).timestamp_millis();
        }),
        (
            "a context from an incompatible protocol version",
            |claims| {
                claims.protocol_version = claims.protocol_version.wrapping_add(1);
            },
        ),
    ];
    for (description, deviate) in claim_deviations {
        let query_id = uuid::Uuid::new_v4();
        let payload = stamped(
            plane.reserve_request(query_id),
            &plane.reserve_binding(query_id),
            deviate,
        )?;
        let outcome = reserve(cluster, &plane.destination, payload).await?;
        if !is_refusal(&outcome) {
            return Err(format!("{description} was answered with {outcome}").into());
        }
    }

    // A correct context presented on a substituted request: every identity
    // still matches, and only the body digest does not.
    let query_id = uuid::Uuid::new_v4();
    let request = plane.reserve_request(query_id);
    let digest = reservation_body_digest(&request.encode_to_vec())
        .map_err(|error| format!("reserve body digest: {error}"))?;
    let mut substituted = proto_with_context(request, &plane.reserve_binding(query_id), digest);
    substituted.expires_at_unix_ms -= 1;
    let outcome = reserve(cluster, &plane.destination, substituted.encode_to_vec()).await?;
    if !is_refusal(&outcome) {
        return Err(format!("a substituted request body was answered with {outcome}").into());
    }
    Ok(())
}

/// One claims-level change applied after an otherwise correct context is built.
type ClaimsDeviation = fn(&mut vala_bifrost_redux::oracle::peer::ReservationTicketClaims);

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
async fn worker_discovery_is_always_refused(
    cluster: &PeerCluster,
    plane: &ReservationPlane,
) -> Result<(), PeerJourneyError> {
    let outcome = cluster
        .probe(
            &PeerProbePlan::own(&plane.destination).on_path("/worker.WorkerService/GetWorkerInfo"),
        )
        .await?;
    if outcome == "Ok" {
        return Err("worker discovery was answered on the private peer plane".into());
    }
    Ok(())
}
