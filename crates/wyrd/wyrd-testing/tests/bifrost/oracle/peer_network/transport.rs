//! One outbound peer transport, and the immutable destinations it may address.

use wyrd_server::config::BifrostTarget;

use super::support::{PeerJourneyError, ReservationPlane, reserve, stamped};
use crate::peer_cluster::{PeerCluster, PeerProbePlan, PeerProbeService, PeerProbeTransport};

/// The private path a Scribe answers tail discovery on.
///
/// Named rather than modelled as an adapter variant because the point of using
/// it here is that it is a *different* role's adapter reached through the same
/// transport, not that it is one of the Oracle paths under test elsewhere.
const SCRIBE_TAIL_PATH: &str = "/wyrd.v1.ScribeTailService/ListActiveStreams";

/// Every private caller shares one mutually authenticated transport, and the
/// destinations one attempt may address are frozen at its fenced participants.
///
/// # Panics
///
/// Panics when any scenario in the table fails, naming the scenario.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn peer_transport_uses_immutable_fenced_destinations() {
    prove_peer_transport_uses_immutable_fenced_destinations()
        .await
        .expect("peer transport journey");
}

/// Drives every transport scenario against one live multi-pod topology.
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_peer_transport_uses_immutable_fenced_destinations() -> Result<(), PeerJourneyError> {
    let mut cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;

    one_transport_reaches_every_role(&cluster).await?;
    a_plaintext_dial_never_reaches_a_private_adapter(&cluster).await?;
    a_replaced_participant_does_not_inherit_the_frozen_fence(&mut cluster).await?;

    cluster.shutdown().await?;
    Ok(())
}

/// Oracle and Scribe destinations are admitted by the same transport
/// identity, each on its own pod.
///
/// The two destinations run different roles and answer different adapters, so
/// a role-specific trust path would show up here as one of them refusing the
/// shared cluster identity. Each destination is a distinct node on its own
/// private socket, and admission is observed as a body poll. The poll counter
/// is process-wide, so each probe is read against the counter immediately
/// around it on an otherwise idle topology.
///
/// # Errors
///
/// Returns a message naming the destination whose admission broke the claim.
async fn one_transport_reaches_every_role(cluster: &PeerCluster) -> Result<(), PeerJourneyError> {
    let destinations: [(&str, usize, PeerProbeService); 2] = [
        ("the Oracle peer adapter", 1, PeerProbeService::OraclePeer),
        (
            "the Scribe tail adapter",
            2,
            PeerProbeService::Path(SCRIBE_TAIL_PATH.to_owned()),
        ),
    ];
    for (description, index, service) in destinations {
        if cluster.node_id(index) == cluster.node_id(0)
            || cluster.peer_addr(index)? == cluster.peer_addr(0)?
        {
            return Err(format!("{description} shares the caller's pod").into());
        }
        let address = cluster.advertise_addr(index).await?;
        let before = cluster.peer_body_polls();
        cluster
            .probe(&PeerProbePlan::own(&address).against(service.clone()))
            .await?;
        if cluster.peer_body_polls() == before {
            return Err(format!("{description} admitted no body from the peer transport").into());
        }
    }
    Ok(())
}

/// A plaintext dial never reaches a private adapter.
///
/// The private plane exists to require a peer certificate, so a connection that
/// presents none must fail at the transport, before any credential above it is
/// read. Proved by the body-poll counter: whatever the dial reports, no body
/// was admitted anywhere in the topology.
///
/// # Errors
///
/// Returns a message when a body was admitted over a plaintext dial.
async fn a_plaintext_dial_never_reaches_a_private_adapter(
    cluster: &PeerCluster,
) -> Result<(), PeerJourneyError> {
    let address = cluster.advertise_addr(1).await?;
    let before = cluster.peer_body_polls();
    // A refused handshake surfaces as a dial failure rather than a gRPC
    // status, which is itself the expected outcome; the counter is what makes
    // either shape provable.
    let _ = cluster
        .probe(&PeerProbePlan::own(&address).over(PeerProbeTransport::Plaintext))
        .await;
    if cluster.peer_body_polls() != before {
        return Err("a plaintext dial reached a private adapter".into());
    }
    Ok(())
}

/// A replaced participant is a new incarnation, not the frozen one.
///
/// The destination is identified by fence as well as address. Restarting the
/// follower on its own volume advances its Oracle fence, so authority frozen
/// against the previous incarnation no longer names anything live: the request
/// is refused at the replacement rather than silently accepted by it, and it is
/// never redirected to the other live pod.
///
/// # Errors
///
/// Returns a message naming the property the replacement broke.
async fn a_replaced_participant_does_not_inherit_the_frozen_fence(
    cluster: &mut PeerCluster,
) -> Result<(), PeerJourneyError> {
    let frozen = ReservationPlane::observe(cluster).await?;

    cluster.restart(1).await?;
    let replaced = ReservationPlane::observe(cluster).await?;

    // An Oracle carries no volume-coupled identity, so a replacement either
    // advances the fence of the same node or comes back as a new one. Both are
    // the same claim: the frozen incarnation is gone, and neither form of
    // replacement may inherit its authority.
    let replaced_incarnation = (replaced.follower_node_id, replaced.follower_fence);
    if replaced_incarnation == (frozen.follower_node_id, frozen.follower_fence) {
        return Err("the replacement came back as the frozen incarnation".into());
    }
    if replaced.follower_node_id == frozen.follower_node_id
        && replaced.follower_fence <= frozen.follower_fence
    {
        return Err(format!(
            "the replacement kept fence {} instead of advancing past {}",
            replaced.follower_fence, frozen.follower_fence
        )
        .into());
    }
    if replaced.destination != frozen.destination {
        return Err("the replacement did not inherit the frozen pod's address".into());
    }

    // Authority frozen against the previous incarnation, presented to the
    // replacement at the same address it inherited.
    let query_id = uuid::Uuid::new_v4();
    let binding = frozen.reserve_binding(query_id);
    let payload = stamped(replaced.reserve_request(query_id), &binding, |_| {})?;
    let outcome = reserve(cluster, &replaced.destination, payload).await?;
    if outcome != "PermissionDenied" && outcome != "Unauthenticated" {
        return Err(format!("a frozen destination fence was answered with {outcome}").into());
    }

    // The same request against the live incarnation is accepted, which is what
    // makes the refusal above attributable to the stale fence alone.
    let query_id = uuid::Uuid::new_v4();
    let binding = replaced.reserve_binding(query_id);
    let payload = stamped(replaced.reserve_request(query_id), &binding, |_| {})?;
    let outcome = reserve(cluster, &replaced.destination, payload).await?;
    if outcome == "PermissionDenied" || outcome == "Unauthenticated" {
        return Err(format!("the live incarnation refused its own fence with {outcome}").into());
    }
    Ok(())
}
