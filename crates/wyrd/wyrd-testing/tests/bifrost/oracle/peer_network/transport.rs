//! One outbound peer transport, and the immutable destinations it may address.

use wyrd_testing::bifrost::process_cluster::{
    BifrostProcessCluster, PeerProbeCredential, PeerProbePlan, PeerProbeService, ProcessNodeTarget,
    VolumeAction,
};

use super::support::{
    KeyringSigners, PeerJourneyError, ReservationPlane, polls_at, probe_from, reserve, sign_ticket,
    stamped,
};

/// Path of the compiled child every simulated pod runs.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

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

/// Drives every transport scenario against one live multi-process topology.
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_peer_transport_uses_immutable_fenced_destinations() -> Result<(), PeerJourneyError> {
    let mut cluster = BifrostProcessCluster::start(
        NODE_BINARY,
        &[
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Scribe,
        ],
    )
    .await?;

    one_transport_reaches_every_role(&mut cluster)?;
    a_plaintext_dial_never_reaches_a_private_adapter(&mut cluster)?;
    a_replaced_participant_does_not_inherit_the_frozen_fence(&mut cluster)?;

    cluster.shutdown();
    Ok(())
}

/// Oracle-to-Oracle and Oracle-to-Scribe traffic is admitted by the same
/// transport, from a genuinely different process.
///
/// The two destinations run different roles and answer different adapters, so
/// a role-specific trust path would show up here as one of them admitting a
/// body the other refuses. Both are checked by the destination's own body-poll
/// counter, and the source PID is compared against each destination's so no
/// case is satisfied by a pod talking to itself.
///
/// # Errors
///
/// Returns a message naming the destination whose admission broke the claim.
fn one_transport_reaches_every_role(
    cluster: &mut BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    let source_pid = cluster.nodes()[0].pid();
    let destinations: [(&str, usize, PeerProbeService); 2] = [
        ("the Oracle peer adapter", 1, PeerProbeService::OraclePeer),
        (
            "the Scribe tail adapter",
            2,
            PeerProbeService::Path(SCRIBE_TAIL_PATH.to_owned()),
        ),
    ];
    for (description, index, service) in destinations {
        if cluster.nodes()[index].pid() == source_pid {
            return Err(format!("{description} shares the caller's process").into());
        }
        let address = cluster.nodes()[index].ready_report().advertise_addr.clone();
        let before = polls_at(cluster, index)?;
        probe_from(
            cluster,
            0,
            &PeerProbePlan::own(&address).against(service.clone()),
        )?;
        if polls_at(cluster, index)? == before {
            return Err(format!("{description} admitted no body from the peer transport").into());
        }

        // Same transport, same destination, an identity the plane must refuse:
        // a role-specific trust gap would show as one adapter admitting this.
        let before = polls_at(cluster, index)?;
        probe_from(
            cluster,
            0,
            &PeerProbePlan::own(&address)
                .against(service)
                .presenting(PeerProbeCredential::Invalid),
        )?;
        if polls_at(cluster, index)? != before {
            return Err(format!("{description} admitted an unverifiable bearer").into());
        }
    }
    Ok(())
}

/// A plaintext dial never reaches a private adapter.
///
/// The private plane exists to require a peer certificate, so a connection that
/// presents none must fail at the transport, before any credential above it is
/// read. Proved by the destination's own counter: whatever the dial reports, no
/// body was admitted.
///
/// # Errors
///
/// Returns a message when the destination admitted a body over a plaintext dial.
fn a_plaintext_dial_never_reaches_a_private_adapter(
    cluster: &mut BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    let address = cluster.nodes()[1].ready_report().advertise_addr.clone();
    let before = polls_at(cluster, 1)?;
    // A refused handshake surfaces as a child-side dial failure rather than a
    // gRPC status, which is itself the expected outcome; the counter is what
    // makes either shape provable.
    let _ = probe_from(
        cluster,
        0,
        &PeerProbePlan::own(&address)
            .over(wyrd_testing::bifrost::process_cluster::PeerProbeTransport::Plaintext),
    );
    if polls_at(cluster, 1)? != before {
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
/// never redirected to the other live child.
///
/// # Errors
///
/// Returns a message naming the property the replacement broke.
fn a_replaced_participant_does_not_inherit_the_frozen_fence(
    cluster: &mut BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    let frozen = ReservationPlane::observe(cluster)?;
    let keyring = KeyringSigners::from(cluster.peer_keyring());

    cluster.restart(1, VolumeAction::Retain)?;
    let replaced = ReservationPlane::observe(cluster)?;

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
    let payload = stamped(replaced.reserve_request(query_id), |digest| {
        sign_ticket(&keyring.active, &binding, digest)
    })?;
    let outcome = reserve(cluster, &replaced.destination, payload)?;
    if outcome != "PermissionDenied" && outcome != "Unauthenticated" {
        return Err(format!("a frozen destination fence was answered with {outcome}").into());
    }

    // The same request against the live incarnation is accepted, which is what
    // makes the refusal above attributable to the stale fence alone.
    let query_id = uuid::Uuid::new_v4();
    let binding = replaced.reserve_binding(query_id);
    let payload = stamped(replaced.reserve_request(query_id), |digest| {
        sign_ticket(&keyring.active, &binding, digest)
    })?;
    let outcome = reserve(cluster, &replaced.destination, payload)?;
    if outcome == "PermissionDenied" || outcome == "Unauthenticated" {
        return Err(format!("the live incarnation refused its own fence with {outcome}").into());
    }
    Ok(())
}
