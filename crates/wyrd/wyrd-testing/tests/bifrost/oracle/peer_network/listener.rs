//! The private peer listener's trust boundary, target matrix, and node identity.

use std::collections::BTreeSet;

use vala_sdk::QueryClient;
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::bifrost::process_cluster::{
    BifrostProcessCluster, NodeReport, PeerTlsDefect, ProcessNodeTarget, VolumeAction,
};
use wyrd_tonic::tonic::Code;

use super::support::{
    DialIdentity, PeerDial, PeerJourneyError, probe_oracle_lifecycle, probe_oracle_peer,
    probe_scribe_tail, target_serves_peer_plane,
};

/// Path of the compiled child every simulated pod runs.
///
/// Resolved by Cargo for this integration target, which is the only place the
/// variable exists; the harness deliberately takes it as a parameter rather
/// than locating or building a binary itself.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

/// Oracle replica counts the topology scenarios cover.
///
/// One proves the complete contract with no remote follower; two and three
/// prove remote peers change no node's configuration; six proves the same at
/// the width the Analytical journeys use.
const ORACLE_TOPOLOGIES: [usize; 4] = [1, 2, 3, 6];

/// The private peer plane is one isolated, mutually authenticated listener
/// owned by the same `wyrd-server` lifecycle as the public one, and every
/// target composes exactly the roles it selects.
///
/// # Panics
///
/// Panics when any scenario in the table fails, naming the scenario.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn peer_listener_is_isolated_mtls_and_role_complete() {
    prove_peer_listener_isolation()
        .await
        .expect("peer listener isolation journey");
}

/// Drives every peer-listener scenario in order.
///
/// The scenarios that only observe a healthy topology share one cluster,
/// because booting a pod is the expensive part and none of them mutate it.
/// The identity and topology scenarios own their own clusters, because they
/// restart pods and vary replica counts.
///
/// # Errors
///
/// Returns the first scenario failure, which names the claim that broke.
async fn prove_peer_listener_isolation() -> Result<(), PeerJourneyError> {
    let mut cluster = BifrostProcessCluster::start(
        NODE_BINARY,
        &[
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Scribe,
            ProcessNodeTarget::ForgeWorker,
        ],
    )
    .await?;

    one_lifecycle_owns_two_isolated_listeners(&cluster)?;
    peer_services_are_absent_from_the_public_listener(&cluster).await?;
    incomplete_peer_material_refuses_to_start(&cluster)?;
    only_a_member_certificate_completes_the_handshake(&cluster).await?;
    a_trusted_certificate_alone_authorizes_nothing(&cluster).await?;
    every_target_mounts_exactly_its_services(&cluster).await?;
    cluster.shutdown();
    drop(cluster);

    scribe_identity_is_coupled_to_its_volume().await?;
    for oracles in ORACLE_TOPOLOGIES {
        oracle_topology_is_uniform_and_exactly_addressed(oracles).await?;
    }
    Ok(())
}

/// One server lifecycle owns both listeners, and each pod is a real process.
///
/// # Errors
///
/// Returns a failure when two pods share a PID, a root, or a socket, or when a
/// peer-bearing pod reports readiness without a distinct private socket.
fn one_lifecycle_owns_two_isolated_listeners(
    cluster: &BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    let mut pids = BTreeSet::new();
    let mut roots = BTreeSet::new();
    let mut sockets = BTreeSet::new();
    for node in cluster.nodes() {
        if !pids.insert(node.pid()) {
            return Err(format!("pod {} shares a PID with another pod", node.label()).into());
        }
        if !roots.insert(node.root().to_path_buf()) {
            return Err(format!("pod {} shares its root with another pod", node.label()).into());
        }
        for socket in [node.http_addr(), node.grpc_addr(), node.peer_addr()] {
            if !sockets.insert(socket) {
                return Err(format!("socket {socket} is bound by two pods").into());
            }
        }
        if node.peer_addr() == node.grpc_addr() {
            return Err("the peer plane shares the public serving socket".into());
        }
        let report = node.ready_report();
        if !report.ready {
            return Err(format!("pod {} announced an unready report", node.label()).into());
        }
        if target_serves_peer_plane(node.target()) && report.advertise_addr.is_empty() {
            return Err(format!(
                "pod {} became ready without publishing a peer address",
                node.label()
            )
            .into());
        }
    }
    Ok(())
}

/// No peer service answers on the public serving listener.
///
/// # Errors
///
/// Returns a failure when any peer service answers anything other than
/// `Unimplemented` on a public socket.
async fn peer_services_are_absent_from_the_public_listener(
    cluster: &BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    for node in cluster.nodes() {
        let address = format!("http://{}", node.grpc_addr());
        let channel = wyrd_tonic::transport::plaintext_endpoint(address)?
            .connect()
            .await?;
        for (name, status) in [
            (
                "OraclePeerService",
                probe_oracle_peer(channel.clone()).await,
            ),
            (
                "ScribeTailService",
                probe_scribe_tail(channel.clone()).await,
            ),
            (
                "OracleLifecycleService",
                probe_oracle_lifecycle(channel).await,
            ),
        ] {
            match status {
                Err(status) if status.code() == Code::Unimplemented => {}
                Err(status) => {
                    return Err(format!(
                        "{name} answered {:?} on the public listener of pod {}",
                        status.code(),
                        node.label()
                    )
                    .into());
                }
                Ok(()) => {
                    return Err(format!(
                        "{name} served a request on the public listener of pod {}",
                        node.label()
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}

/// A peer-bearing target refuses to start without complete mutual-TLS material.
///
/// # Errors
///
/// Returns a failure when a damaged child starts anyway.
fn incomplete_peer_material_refuses_to_start(
    cluster: &BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    for (label, defect) in [
        ("probe-missing-ca", PeerTlsDefect::MissingCa),
        ("probe-missing-cert", PeerTlsDefect::MissingCertificate),
        ("probe-missing-key", PeerTlsDefect::MissingPrivateKey),
    ] {
        cluster.probe_startup_failure(label, ProcessNodeTarget::Oracle, defect)?;
    }
    Ok(())
}

/// Only a leaf from the configured authority, under the configured name, joins.
///
/// # Errors
///
/// Returns a failure when an anonymous, foreign, or misnamed dial completes a
/// handshake, or when a member dial does not.
async fn only_a_member_certificate_completes_the_handshake(
    cluster: &BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    let node = cluster
        .nodes()
        .iter()
        .find(|node| target_serves_peer_plane(node.target()))
        .ok_or("no peer-bearing pod in the topology")?;
    let address = node.peer_addr();

    PeerDial::member(cluster.peer_ca(), address)
        .connect()
        .await
        .map_err(|error| format!("a member identity was refused: {error}"))?;

    for (name, dial) in [
        (
            "an anonymous client",
            PeerDial::member(cluster.peer_ca(), address).with_identity(DialIdentity::Anonymous),
        ),
        (
            "a foreign authority",
            PeerDial::member(cluster.peer_ca(), address).with_identity(DialIdentity::Foreign),
        ),
        (
            "a wrong served name",
            PeerDial::member(cluster.peer_ca(), address)
                .expecting_server_name("not-the-peer-plane.invalid"),
        ),
    ] {
        if dial.connect().await.is_ok() {
            return Err(format!("{name} completed the peer handshake").into());
        }
    }
    Ok(())
}

/// Transport admission is not node identity and is not operation authority.
///
/// # Errors
///
/// Returns a failure when a peer RPC succeeds for a caller that presented only
/// a trusted certificate, or when the certificate is treated as a `NodeId`.
async fn a_trusted_certificate_alone_authorizes_nothing(
    cluster: &BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    for node in cluster.nodes() {
        if !target_serves_peer_plane(node.target()) {
            continue;
        }
        let report = node.ready_report();
        if report
            .peer_certificate_fingerprint
            .contains(&report.node_id.simple().to_string())
        {
            return Err("the peer certificate encodes the runtime node identity".into());
        }
        let channel = PeerDial::member(cluster.peer_ca(), node.peer_addr())
            .connect()
            .await?;
        match probe_oracle_peer(channel).await {
            Err(status)
                if matches!(
                    status.code(),
                    Code::Unauthenticated | Code::PermissionDenied | Code::Unimplemented
                ) => {}
            Err(status) => {
                return Err(format!(
                    "a certificate-only caller was refused as {:?} rather than unauthorized",
                    status.code()
                )
                .into());
            }
            Ok(()) => {
                return Err("a certificate-only caller executed a peer operation".into());
            }
        }
    }
    Ok(())
}

/// Each target composes exactly the peer services its selected roles own.
///
/// # Errors
///
/// Returns a failure when a service is missing from a target that owns it or
/// present on a target that does not.
async fn every_target_mounts_exactly_its_services(
    cluster: &BifrostProcessCluster,
) -> Result<(), PeerJourneyError> {
    for node in cluster.nodes() {
        let target = node.target();
        if !target_serves_peer_plane(target) {
            // No Scribe and no Oracle means nothing to answer privately, so the
            // pod must not open a private listener at all.
            if PeerDial::member(cluster.peer_ca(), node.peer_addr())
                .connect()
                .await
                .is_ok()
            {
                return Err(
                    format!("{target:?} opened a peer listener it owns no service on").into(),
                );
            }
            continue;
        }
        let channel = PeerDial::member(cluster.peer_ca(), node.peer_addr())
            .connect()
            .await?;
        let expects_tail = matches!(
            target,
            ProcessNodeTarget::All | ProcessNodeTarget::Server | ProcessNodeTarget::Scribe
        );
        let expects_lifecycle = matches!(
            target,
            ProcessNodeTarget::All | ProcessNodeTarget::Server | ProcessNodeTarget::Oracle
        );
        for (name, mounted, status) in [
            (
                "OraclePeerService",
                true,
                probe_oracle_peer(channel.clone()).await,
            ),
            (
                "ScribeTailService",
                expects_tail,
                probe_scribe_tail(channel.clone()).await,
            ),
            (
                "OracleLifecycleService",
                expects_lifecycle,
                probe_oracle_lifecycle(channel.clone()).await,
            ),
        ] {
            let unimplemented =
                matches!(&status, Err(status) if status.code() == Code::Unimplemented);
            if mounted && unimplemented {
                return Err(format!("{target:?} does not mount {name}").into());
            }
            if !mounted && !unimplemented {
                return Err(format!("{target:?} mounts {name}, which it owns no role for").into());
            }
        }
    }
    Ok(())
}

/// A Scribe's runtime node identity lives on its durable volume.
///
/// # Errors
///
/// Returns a failure when a restart changes the identity, when a replaced
/// volume preserves it, or when damaged identity state does not stop startup.
async fn scribe_identity_is_coupled_to_its_volume() -> Result<(), PeerJourneyError> {
    let mut cluster =
        BifrostProcessCluster::start(NODE_BINARY, &[ProcessNodeTarget::Scribe]).await?;
    let original = cluster.nodes()[0].ready_report().clone();

    let restarted = cluster.restart(0, VolumeAction::Retain)?.clone();
    if restarted.node_id != original.node_id {
        return Err("a Scribe restart on its own volume changed the node identity".into());
    }
    if restarted.pid == original.pid {
        return Err("the restart reused the original process".into());
    }
    let advanced = fence_of(&restarted, original.node_id)?;
    let before = fence_of(&original, original.node_id)?;
    if advanced <= before {
        return Err(format!(
            "the restarted Scribe reused fence {before} instead of advancing past it"
        )
        .into());
    }

    let replaced = cluster.restart(0, VolumeAction::Reset)?.clone();
    if replaced.node_id == original.node_id {
        return Err("a replaced volume kept the previous node identity".into());
    }

    for damaged in [VolumeAction::Malformed, VolumeAction::Partial] {
        if cluster.restart(0, damaged).is_ok() {
            return Err(format!("a Scribe started over {damaged:?} identity state").into());
        }
        // A refused relaunch leaves the slot empty, so the next case needs a
        // pod to damage again.
        cluster = BifrostProcessCluster::start(NODE_BINARY, &[ProcessNodeTarget::Scribe]).await?;
    }
    cluster.shutdown();
    Ok(())
}

/// Returns the fence one node holds in a report's observed membership.
///
/// # Errors
///
/// Returns a failure when the node published no live role.
fn fence_of(report: &NodeReport, node_id: uuid::Uuid) -> Result<u64, PeerJourneyError> {
    report
        .membership
        .iter()
        .find(|entry| entry.node_id == node_id)
        .map(|entry| entry.fencing_token)
        .ok_or_else(|| "the node published no live role".into())
}

/// Oracle pods are interchangeable at every replica count.
///
/// # Errors
///
/// Returns a failure when membership omits a pod, publishes an address that is
/// not that pod's own peer socket, when an Oracle cannot coordinate a public
/// query, or when one Oracle cannot reach another over the peer plane.
async fn oracle_topology_is_uniform_and_exactly_addressed(
    oracles: usize,
) -> Result<(), PeerJourneyError> {
    let mut targets = vec![ProcessNodeTarget::Oracle; oracles];
    // One Scribe so the topology owns a catalog and a tail source, exactly as a
    // deployment that serves queries does.
    targets.push(ProcessNodeTarget::Scribe);
    let mut cluster = BifrostProcessCluster::start(NODE_BINARY, &targets).await?;

    let api_key = cluster
        .provision_public_api_key("peer-network-caller")
        .await?;
    let table = format!("peer_topology_{}", uuid::Uuid::now_v7().simple());
    let scribe = cluster.nodes().len() - 1;
    cluster.nodes_mut()[scribe].register_table(&table)?;

    let addresses: Vec<(uuid::Uuid, String)> = cluster
        .nodes()
        .iter()
        .filter(|node| target_serves_peer_plane(node.target()))
        .map(|node| {
            (
                node.ready_report().node_id,
                format!("https://{}", node.peer_addr()),
            )
        })
        .collect();

    for index in 0..cluster.nodes().len() {
        let report = cluster.nodes_mut()[index].inspect()?;
        if !target_serves_peer_plane(report.target) {
            continue;
        }
        for (node_id, address) in &addresses {
            let entry = report
                .membership
                .iter()
                .find(|entry| entry.node_id == *node_id)
                .ok_or_else(|| {
                    format!("pod {} cannot see node {node_id} in membership", report.pid)
                })?;
            if entry.address != *address {
                return Err(format!(
                    "membership publishes {} for node {node_id}, not its own peer socket {address}",
                    entry.address
                )
                .into());
            }
        }
    }

    for index in 0..cluster.nodes().len() {
        if cluster.nodes()[index].target() != ProcessNodeTarget::Oracle {
            continue;
        }
        coordinate_public_query(&cluster.nodes()[index], &api_key, &table).await?;
        let destinations: Vec<String> = addresses
            .iter()
            .filter(|(node_id, _)| *node_id != cluster.nodes()[index].ready_report().node_id)
            .map(|(_, address)| address.clone())
            .collect();
        for destination in destinations {
            let outcome = cluster.nodes_mut()[index].dial_peer(&destination)?;
            if outcome == Code::Unauthenticated.to_string()
                || outcome == Code::PermissionDenied.to_string()
            {
                return Err(format!(
                    "an authorized Oracle-to-peer dial to {destination} was refused as {outcome}"
                )
                .into());
            }
        }
    }
    cluster.shutdown();
    Ok(())
}

/// Runs one public Interactive query against a single pod's public listener.
///
/// # Errors
///
/// Returns a failure when the pod cannot serve the query as an ordinary
/// external caller.
async fn coordinate_public_query(
    node: &wyrd_testing::bifrost::process_cluster::ProcessNode,
    api_key: &secrecy::SecretString,
    table: &str,
) -> Result<(), PeerJourneyError> {
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: format!("http://{}", node.grpc_addr()),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: format!("http://{}", node.http_addr()),
            ..HttpConfig::default()
        },
        api_key: Some(api_key.clone()),
        ..ClientConfig::default()
    })?;
    let mut stream = QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id FROM vala.bifrost.{table} ORDER BY id"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await?;
    while stream.next_batch().await?.is_some() {}
    stream
        .terminal()
        .ok_or("the coordinated query produced no terminal frame")?;
    Ok(())
}
