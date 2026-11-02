//! The private peer listener's trust boundary, target matrix, and node identity.

use std::collections::BTreeSet;

use sha2::{Digest as _, Sha256};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
use wyrd_tonic::tonic::Code;
use wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient;
use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_client::ScribeCapturePeerServiceClient;
use wyrd_tonic::wyrd::v1::{AnalyticalGraphRef, IngestCaptureRequest, ReserveNodeSlotsRequest};

use super::support::{
    DialIdentity, PeerDial, PeerJourneyError, probe_oracle_lifecycle, probe_oracle_peer,
    probe_scribe_tail, target_serves_peer_plane, target_serves_public_listener,
};
use crate::peer_cluster::{PeerCluster, PeerProbePlan};
use crate::support::public_client;

/// tonic's default per-message decode limit, which a peer capture frame must
/// be able to exceed when the configured ingest ceiling is larger.
const TONIC_DEFAULT_DECODE_BYTES: usize = 4 * 1024 * 1024;

/// File a Scribe's runtime node identity is persisted into beside its WAL.
///
/// Mirrored here rather than imported so the journey can damage the document
/// without the production store offering a way to write a broken one.
const IDENTITY_FILE_NAME: &str = "node-identity.json";

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
    Box::pin(prove_peer_listener_isolation())
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
    let mut cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
        BifrostTarget::ForgeWorker,
    ])
    .await?;

    one_lifecycle_owns_two_isolated_listeners(&cluster).await?;
    peer_services_are_absent_from_the_public_listener(&cluster).await?;
    incomplete_peer_material_refuses_to_start(cluster.peer_ca()).await?;
    only_a_member_certificate_completes_the_handshake(&cluster).await?;
    a_trusted_certificate_alone_authorizes_nothing(&cluster).await?;
    every_target_mounts_exactly_its_services(&cluster).await?;
    a_frame_above_the_tonic_default_is_acknowledged_and_retained(&cluster).await?;
    a_restarted_scribe_advances_its_fence(&mut cluster, 1).await?;
    cluster.shutdown().await?;

    scribe_identity_is_coupled_to_its_volume().await?;
    // Three Oracles is the width the Analytical journeys run at, and the
    // claim is that every replica is configured and addressed identically.
    // Replaying it at other widths re-proves the same uniformity for the cost
    // of booting more pods.
    oracle_topology_is_uniform_and_exactly_addressed(3).await?;
    Ok(())
}

/// One server lifecycle owns both listeners, and each pod is its own node.
///
/// # Errors
///
/// Returns a failure when two pods share a node identity, a data root, or a
/// socket, or when a peer-bearing pod is ready without a distinct private
/// socket published into membership.
async fn one_lifecycle_owns_two_isolated_listeners(
    cluster: &PeerCluster,
) -> Result<(), PeerJourneyError> {
    let mut node_ids = BTreeSet::new();
    let mut roots = BTreeSet::new();
    let mut sockets = BTreeSet::new();
    for index in 0..cluster.len() {
        let target = cluster.target(index);
        let server = cluster.server(index)?;
        if !node_ids.insert(server.node_id()) {
            return Err(format!("pod {index} shares a node identity with another pod").into());
        }
        let owned_roots = [
            server
                .scribe_wal_root_for_test()
                .map(std::path::Path::to_path_buf),
            server.state().bifrost_query().and_then(|oracle| {
                oracle
                    .engine()
                    .analytical_spill_root()
                    .map(std::path::Path::to_path_buf)
            }),
        ];
        for root in owned_roots.into_iter().flatten() {
            if !roots.insert(root) {
                return Err(format!("pod {index} shares a data root with another pod").into());
            }
        }
        let peer = cluster.peer_addr(index)?;
        if target_serves_public_listener(target) {
            let http = server
                .bound_addr()
                .ok_or("a serving pod bound no HTTP socket")?;
            let grpc = server
                .grpc_url()
                .ok_or("a serving pod bound no gRPC socket")?
                .trim_start_matches("http://")
                .parse::<std::net::SocketAddr>()?;
            for socket in [http, grpc, peer] {
                if !sockets.insert(socket) {
                    return Err(format!("socket {socket} is bound by two pods").into());
                }
            }
            if peer == grpc {
                return Err("the peer plane shares the public serving socket".into());
            }
        }
        if target_serves_peer_plane(target) && cluster.advertise_addr(index).await?.is_empty() {
            return Err(
                format!("pod {index} became ready without publishing a peer address").into(),
            );
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
    cluster: &PeerCluster,
) -> Result<(), PeerJourneyError> {
    for index in 0..cluster.len() {
        if !target_serves_public_listener(cluster.target(index)) {
            continue;
        }
        let address = cluster
            .server(index)?
            .grpc_url()
            .ok_or("a serving pod bound no gRPC socket")?;
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
                        "{name} answered {:?} on the public listener of pod {index}",
                        status.code(),
                    )
                    .into());
                }
                Ok(()) => {
                    return Err(format!(
                        "{name} served a request on the public listener of pod {index}"
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
/// Each case materializes a real bundle from the cluster authority, deletes
/// exactly one of its three files, and starts an Oracle over it. Composition
/// reads the bundle through the production peer configuration loader, so the
/// refusal is the one a deployed server gives.
///
/// # Errors
///
/// Returns a failure when a damaged server starts anyway, or the bundle
/// cannot be materialized.
async fn incomplete_peer_material_refuses_to_start(
    authority: &BifrostPeerCa,
) -> Result<(), PeerJourneyError> {
    let scratch = tempfile::tempdir()?;
    for (label, missing) in [
        ("probe-missing-ca", "ca.crt"),
        ("probe-missing-cert", "tls.crt"),
        ("probe-missing-key", "tls.key"),
    ] {
        let tls = authority.materialize(scratch.path(), label)?;
        std::fs::remove_file(tls.dir.join(missing))?;
        let started = WyrdTestServer::builder()
            .with_bifrost_target_for_test(BifrostTarget::Oracle)
            .with_peer_tls(tls)
            .start_bound()
            .await;
        if let Ok(server) = started {
            server.shutdown().await?;
            return Err(format!("an Oracle started without its {missing}").into());
        }
    }
    Ok(())
}

/// Only a leaf from the configured authority, under the configured name, joins.
///
/// TLS 1.3 reports a rejected client certificate after the client's own
/// handshake completes, so a refusal is proved by driving one real RPC rather
/// than by whether `connect` returned. A member is expected to reach the
/// application layer and be answered there; an anonymous or foreign identity is
/// expected to lose the connection instead. A valid leaf from the cluster's own
/// authority for another name completes the handshake, so it must instead be
/// refused as unauthenticated before the peer plane polls any request body.
///
/// # Errors
///
/// Returns a failure when an anonymous, foreign, expired, misnamed-server, or
/// misnamed-client identity reaches the application layer, or when a member
/// cannot.
async fn only_a_member_certificate_completes_the_handshake(
    cluster: &PeerCluster,
) -> Result<(), PeerJourneyError> {
    let index = (0..cluster.len())
        .find(|index| target_serves_peer_plane(cluster.target(*index)))
        .ok_or("no peer-bearing pod in the topology")?;
    let address = cluster.peer_addr(index)?;

    let before = cluster.peer_body_polls();
    let misnamed = PeerDial::member(cluster.peer_ca(), address)
        .with_identity(DialIdentity::Misnamed)
        .connect()
        .await;
    if let Ok(channel) = misnamed {
        match probe_oracle_peer(channel).await {
            Err(status) if status.code() == Code::Unauthenticated => {}
            Err(status) if !reached_the_application(&status) => {}
            outcome => {
                return Err(format!(
                    "a same-authority leaf for another name was admitted: {outcome:?}"
                )
                .into());
            }
        }
    }
    if cluster.peer_body_polls() != before {
        return Err("a same-authority leaf for another name reached a peer body".into());
    }

    let member = PeerDial::member(cluster.peer_ca(), address)
        .connect()
        .await
        .map_err(|error| format!("a member identity was refused: {error}"))?;
    match probe_oracle_peer(member).await {
        Ok(()) => {}
        Err(status) if reached_the_application(&status) => {}
        Err(status) => {
            return Err(format!(
                "a member identity was dropped by the transport as {:?}",
                status.code()
            )
            .into());
        }
    }

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
            "an expired member certificate",
            PeerDial::member(cluster.peer_ca(), address).with_identity(DialIdentity::Expired),
        ),
        (
            "a wrong served name",
            PeerDial::member(cluster.peer_ca(), address)
                .expecting_server_name("not-the-peer-plane.invalid"),
        ),
    ] {
        let Ok(channel) = dial.connect().await else {
            continue;
        };
        match probe_oracle_peer(channel).await {
            Err(status) if !reached_the_application(&status) => {}
            Ok(()) => return Err(format!("{name} executed a peer operation").into()),
            Err(status) => {
                return Err(format!(
                    "{name} reached the application layer and was answered {:?}",
                    status.code()
                )
                .into());
            }
        }
    }
    Ok(())
}

/// Reports whether a status was produced by the service rather than the transport.
///
/// A peer that was admitted is answered by Wyrd — authenticated, refused, or
/// told the operation is not mounted here. A peer whose certificate was
/// rejected never gets that far and sees the connection break instead.
fn reached_the_application(status: &wyrd_tonic::tonic::Status) -> bool {
    matches!(
        status.code(),
        Code::Unauthenticated
            | Code::PermissionDenied
            | Code::Unimplemented
            | Code::InvalidArgument
            | Code::NotFound
            | Code::FailedPrecondition
            | Code::ResourceExhausted
            | Code::Internal
    )
}

/// Transport admission is not node identity and is not operation authority.
///
/// Every peer-plane pod mounts `OraclePeerService`, so each receives a
/// well-formed reserve that carries no typed context. An Oracle pod is named
/// with its own live fence, so the request passes conversion and the fence
/// check and is refused only for its missing authority. A pod without the
/// Oracle role has no reservation to authorize and refuses the precondition.
///
/// # Errors
///
/// Returns a failure when a peer RPC succeeds for a caller that presented only
/// a trusted certificate, when it is refused for any other reason, or when the
/// certificate is treated as a `NodeId`.
async fn a_trusted_certificate_alone_authorizes_nothing(
    cluster: &PeerCluster,
) -> Result<(), PeerJourneyError> {
    let mut presented = BTreeSet::new();
    for index in 0..cluster.len() {
        let target = cluster.target(index);
        if !target_serves_peer_plane(target) {
            continue;
        }
        let node_id = cluster.node_id(index).as_uuid();
        let tls = cluster
            .server(index)?
            .peer_tls()
            .ok_or("a peer-bearing pod carries no peer identity")?;
        let certificate = std::fs::read(&tls.certificate_path)?;
        let text = String::from_utf8_lossy(&certificate);
        if text.contains(&node_id.simple().to_string()) || text.contains(&node_id.to_string()) {
            return Err("the peer certificate encodes the runtime node identity".into());
        }
        presented.insert(format!("{:x}", Sha256::digest(&certificate)));
        let channel = PeerDial::member(cluster.peer_ca(), cluster.peer_addr(index)?)
            .connect()
            .await?;
        let serves_oracle = matches!(target, BifrostTarget::Oracle | BifrostTarget::All);
        // The fence is read from a live membership cut, which is what a
        // leader stamps into a reservation.
        let fence = cluster
            .membership(index)
            .await?
            .iter()
            .find(|entry| entry.node_id == node_id && entry.role == "oracle")
            .map(|own| own.fencing_token);
        let leader_fencing_token = match (serves_oracle, fence) {
            (true, Some(fence)) => fence,
            (true, None) => return Err("an Oracle pod holds no live Oracle lease".into()),
            (false, _) => 1,
        };
        let request = ReserveNodeSlotsRequest {
            query_id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            leader_node_id: node_id.to_string(),
            leader_fencing_token,
            expires_at_unix_ms: u64::try_from(
                (chrono::Utc::now() + chrono::Duration::seconds(30)).timestamp_millis(),
            )?,
            context: None,
            graph: Some(AnalyticalGraphRef {
                public_query_id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                datafusion_query_id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            }),
        };
        match OraclePeerServiceClient::new(channel)
            .reserve_slots(request)
            .await
            .map(|_| ())
        {
            Err(status) if serves_oracle && status.code() == Code::Unauthenticated => {}
            Err(status) if !serves_oracle && status.code() == Code::FailedPrecondition => {}
            Err(status) => {
                return Err(format!(
                    "a certificate-only caller to {target:?} was refused as {:?}: {}",
                    status.code(),
                    status.message()
                )
                .into());
            }
            Ok(()) => {
                return Err("a certificate-only caller executed a peer operation".into());
            }
        }
    }
    // Every pod presents the one shared cluster identity, so no certificate
    // can distinguish, let alone name, a runtime node.
    if presented.len() > 1 {
        return Err("pods present distinct peer certificates instead of the cluster one".into());
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
    cluster: &PeerCluster,
) -> Result<(), PeerJourneyError> {
    for index in 0..cluster.len() {
        let target = cluster.target(index);
        let peer = cluster.peer_addr(index)?;
        if !target_serves_peer_plane(target) {
            // No Scribe and no Oracle means nothing to answer privately, so the
            // pod must not open a private listener at all.
            if PeerDial::member(cluster.peer_ca(), peer)
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
        let channel = PeerDial::member(cluster.peer_ca(), peer).connect().await?;
        let expects_tail = matches!(
            target,
            BifrostTarget::All | BifrostTarget::Server | BifrostTarget::Scribe
        );
        let expects_lifecycle = matches!(
            target,
            BifrostTarget::All | BifrostTarget::Server | BifrostTarget::Oracle
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

/// A valid capture frame above tonic's default decode limit, below the
/// configured ingest ceiling, is acknowledged by the Scribe peer plane and
/// lands whole in retained history.
///
/// One `bifrost.system.audit_log` row whose `resource` alone is six MiB is
/// projected through the production audit projection, encoded as Arrow IPC,
/// and submitted as a cluster member to the Scribe pod's capture peer RPC.
/// The ACK proves the listener decodes at the configured ceiling rather than
/// tonic's default; the retained read through an Oracle pod, matching the
/// exact resource length, proves the frame was neither truncated nor dropped.
///
/// # Errors
///
/// Returns a failure when the frame is not above the tonic default and below
/// the configured ceiling, the Scribe refuses it, or retained history does not
/// hold exactly that one row.
async fn a_frame_above_the_tonic_default_is_acknowledged_and_retained(
    cluster: &PeerCluster,
) -> Result<(), PeerJourneyError> {
    let index_of = |target: BifrostTarget| {
        (0..cluster.len())
            .find(|index| cluster.target(*index) == target)
            .ok_or_else(|| format!("the cluster runs no {target:?} pod"))
    };
    let scribe = cluster.server(index_of(BifrostTarget::Scribe)?)?;
    let oracle = cluster.server(index_of(BifrostTarget::Oracle)?)?;
    let operation = format!("journey.large_frame.{}", uuid::Uuid::now_v7().simple());
    let resource_bytes = 6 * 1024 * 1024;
    let event = wyrd_spec::vala::api::AuditEvent::new(
        wyrd_spec::request_id::RequestId::now_v7(),
        None,
        operation.clone(),
        "r".repeat(resource_bytes),
        None,
        wyrd_spec::auth::Attribution::direct(wyrd_spec::auth::PrincipalId::new(
            uuid::Uuid::now_v7(),
        )),
        wyrd_spec::auth::PrincipalKindTag::Service,
        operation.clone(),
        wyrd_spec::vala::api::AuditOutcome::Allowed,
    );
    let batch = vala_bifrost_redux::tables::staged::StagedRow::project(&[
        vala_bifrost_redux::tables::audit::projection::stage_audit_event(
            &event,
            chrono::Utc::now(),
        )?,
    ])?;
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(Vec::new(), &batch.schema())?;
    writer.write(&batch)?;
    writer.finish()?;
    let request = IngestCaptureRequest {
        tenant_id: cluster.tenant().to_string(),
        table: "bifrost.system.audit_log".to_owned(),
        batch_id: uuid::Uuid::now_v7().to_string(),
        request_id: event.request_id.as_str().to_owned(),
        arrow_ipc: writer.into_inner()?.into(),
    };
    let encoded = wyrd_tonic::prost::Message::encoded_len(&request);
    let ceiling = vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES;
    if encoded <= TONIC_DEFAULT_DECODE_BYTES || encoded >= ceiling {
        return Err(format!(
            "a {encoded}-byte frame does not sit above tonic's default and below the \
             {ceiling}-byte configured ceiling"
        )
        .into());
    }
    let authority = cluster.peer_ca();
    let member = authority.issue_leaf("large-frame-member")?;
    let channel = wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
        scribe
            .peer_url()
            .ok_or("the Scribe pod has no peer listener")?,
        authority.ca_certificate_pem().as_bytes(),
        authority.server_name().to_owned(),
        member.certificate_pem().as_bytes(),
        member.private_key_pem().as_bytes(),
    )?
    .connect()
    .await?;
    ScribeCapturePeerServiceClient::new(channel)
        .ingest_capture(request)
        .await
        .map_err(|status| {
            format!(
                "the Scribe refused a valid {encoded}-byte frame: {:?} {}",
                status.code(),
                status.message()
            )
        })?;
    oracle
        .await_retained_audit_count(
            cluster.tenant(),
            &format!("operation = '{operation}' AND length(resource) = {resource_bytes}"),
            1,
        )
        .await?;
    Ok(())
}

/// A Scribe restarted on its own volume comes back under an advanced fence.
///
/// The restarted pod keeps its node identity and address, so the only thing
/// that can distinguish the new incarnation from the old one is its fence.
///
/// # Errors
///
/// Returns a failure when the restart keeps or regresses the fence, or the
/// pod published no live role.
async fn a_restarted_scribe_advances_its_fence(
    cluster: &mut PeerCluster,
    index: usize,
) -> Result<(), PeerJourneyError> {
    let before = scribe_fence(cluster, index).await?;
    cluster.restart(index).await?;
    let advanced = scribe_fence(cluster, index).await?;
    if advanced <= before {
        return Err(format!(
            "the restarted Scribe reused fence {before} instead of advancing past it"
        )
        .into());
    }
    Ok(())
}

/// Returns the live Scribe fence pod `index` holds in its own membership cut.
///
/// # Errors
///
/// Returns the membership refresh failure, or a failure when the pod holds no
/// live Scribe lease.
async fn scribe_fence(cluster: &PeerCluster, index: usize) -> Result<u64, PeerJourneyError> {
    let node_id = cluster.node_id(index).as_uuid();
    cluster
        .membership(index)
        .await?
        .iter()
        .find(|entry| entry.node_id == node_id && entry.role == "scribe")
        .map(|entry| entry.fencing_token)
        .ok_or_else(|| "the Scribe published no live lease".into())
}

/// A Scribe's runtime node identity lives on its durable volume.
///
/// Each start is a peer-mode Scribe composed over one durable data root, the
/// way a pod is restarted on its own volume. The node identity is loaded from
/// that volume by the production identity store, so a retained volume keeps
/// it, a replaced volume mints a new one, and damaged identity state stops
/// startup instead of silently minting a new node over another node's WAL.
///
/// # Errors
///
/// Returns a failure when a retained volume changes the identity, a replaced
/// volume preserves it, or damaged identity state does not stop startup.
async fn scribe_identity_is_coupled_to_its_volume() -> Result<(), PeerJourneyError> {
    let scratch = tempfile::tempdir()?;
    let volume = scratch.path().join("volume");
    std::fs::create_dir_all(&volume)?;
    let authority = BifrostPeerCa::generate(wyrd_server::config::PEER_SERVER_NAME)?;
    let tls = authority.materialize(scratch.path(), "scribe")?;
    let start = || {
        WyrdTestServer::builder()
            .with_bifrost_target_for_test(BifrostTarget::Scribe)
            .with_durable_bifrost_data_root(volume.clone())
            .with_peer_tls(tls.clone())
            .start_bound()
    };

    let original = start().await?;
    let original_id = original.node_id();
    original.shutdown().await?;

    let restarted = start().await?;
    let restarted_id = restarted.node_id();
    restarted.shutdown().await?;
    if restarted_id != original_id {
        return Err("a Scribe restart on its own volume changed the node identity".into());
    }

    std::fs::remove_dir_all(&volume)?;
    std::fs::create_dir_all(&volume)?;
    let replaced = start().await?;
    let replaced_id = replaced.node_id();
    replaced.shutdown().await?;
    if replaced_id == original_id {
        return Err("a replaced volume kept the previous node identity".into());
    }

    for (damage, contents) in [
        ("malformed", b"{ this is not identity state".as_slice()),
        ("partial", br#"{"version":1}"#.as_slice()),
    ] {
        std::fs::write(volume.join(IDENTITY_FILE_NAME), contents)?;
        if let Ok(server) = start().await {
            server.shutdown().await?;
            return Err(format!("a Scribe started over {damage} identity state").into());
        }
    }
    Ok(())
}

/// Oracle pods are interchangeable across `oracles` replicas.
///
/// Interchangeability of the *Analytical coordinator* is proven where it is
/// observable, by
/// `peer_network::analytical::baseline_executes_join_group_sort_and_interchangeable_topology`,
/// which runs the same physical query on two different Oracles of one cluster
/// and compares their results. This scenario proves the configuration half:
/// membership, addressing, listener isolation, and peer reachability.
///
/// # Errors
///
/// Returns a failure when membership omits a pod, publishes an address that is
/// not that pod's own peer socket, when an Oracle cannot coordinate a public
/// query, or when one Oracle cannot reach another over the peer plane.
async fn oracle_topology_is_uniform_and_exactly_addressed(
    oracles: usize,
) -> Result<(), PeerJourneyError> {
    let mut targets = vec![BifrostTarget::Oracle; oracles];
    // One Scribe so the topology owns a catalog and a tail source, exactly as a
    // deployment that serves queries does.
    targets.push(BifrostTarget::Scribe);
    let cluster = PeerCluster::start(&targets).await?;

    let api_key = cluster
        .provision_public_api_key("peer-network-caller")
        .await?;
    let table = format!("peer_topology_{}", uuid::Uuid::now_v7().simple());
    let scribe = cluster.len() - 1;
    cluster.register_table(scribe, &table).await?;

    let mut addresses: Vec<(uuid::Uuid, String)> = Vec::new();
    for index in 0..cluster.len() {
        if target_serves_peer_plane(cluster.target(index)) {
            addresses.push((
                cluster.node_id(index).as_uuid(),
                format!("https://{}", cluster.peer_addr(index)?),
            ));
        }
    }

    for index in 0..cluster.len() {
        if !target_serves_peer_plane(cluster.target(index)) {
            continue;
        }
        let membership = cluster.membership(index).await?;
        for (node_id, address) in &addresses {
            let entry = membership
                .iter()
                .find(|entry| entry.node_id == *node_id)
                .ok_or_else(|| format!("pod {index} cannot see node {node_id} in membership"))?;
            if entry.address != *address {
                return Err(format!(
                    "membership publishes {} for node {node_id}, not its own peer socket {address}",
                    entry.address
                )
                .into());
            }
        }
    }

    for index in 0..cluster.len() {
        if cluster.target(index) != BifrostTarget::Oracle {
            continue;
        }
        coordinate_public_query(cluster.server(index)?, &api_key, &table).await?;
        let own = cluster.node_id(index).as_uuid();
        for (_, destination) in addresses.iter().filter(|(node_id, _)| *node_id != own) {
            let outcome = cluster.probe(&PeerProbePlan::own(destination)).await?;
            if outcome == format!("{:?}", Code::Unauthenticated)
                || outcome == format!("{:?}", Code::PermissionDenied)
            {
                return Err(format!(
                    "an authorized Oracle-to-peer dial to {destination} was refused as {outcome}"
                )
                .into());
            }
        }
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Runs one public Interactive query against a single pod's public listener.
///
/// # Errors
///
/// Returns a failure when the pod cannot serve the query as an ordinary
/// external caller.
async fn coordinate_public_query(
    node: &WyrdTestServer,
    api_key: &secrecy::SecretString,
    table: &str,
) -> Result<(), PeerJourneyError> {
    let client = public_client(node, api_key)?;
    let mut stream = wyrd_client::Bifrost::query_only(&client)
        .query(&BifrostQueryRequest {
            params: Vec::new(),
            sql: format!("SELECT id FROM bifrost.bifrost.{table} ORDER BY id"),
            deadline_ms: None,
        })
        .await?;
    while stream.next_batch().await?.is_some() {}
    stream
        .terminal()
        .ok_or("the coordinated query produced no terminal frame")?;
    Ok(())
}
