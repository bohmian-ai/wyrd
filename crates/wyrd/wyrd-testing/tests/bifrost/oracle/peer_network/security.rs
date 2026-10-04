//! Peer receiver context checks on the mTLS-only private plane.

use std::sync::Arc;

use datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::oracle::assignment_schema_fingerprint;
use vala_bifrost_redux::oracle::codec::{
    OraclePhysicalExtensionCodec, RemoteSourcePlaceholderExec, physical_plan_fingerprint,
};
use vala_bifrost_redux::oracle::dispatcher::PEER_PROTOCOL_VERSION;
use vala_bifrost_redux::oracle::peer::{
    PeerTicketClaims, ReservationBinding, assignment_authority_digest_for, reservation_body_digest,
};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::vala::api::{
    ClusterRole, ExecuteFragmentRequest, FollowerScanAssignment, NodeId, OracleRoleFence,
    PeerContext, PersistedFileAssignment, ReservationId, ScribeProviderCut, TenantTableBinding,
};
use wyrd_tonic::prost::Message as _;
use wyrd_tonic::tonic;
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
    an_abandoned_grant_returns_the_follower_to_baseline(&cluster, &plane).await?;
    every_bound_identity_must_match(&cluster, &plane).await?;
    worker_discovery_is_always_refused(&cluster, &plane).await?;
    a_foreign_tenant_context_reads_no_scribe_rows(&cluster, &plane).await?;

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

/// A leader that admits a grant and abandons its plan leaves the follower clean.
///
/// The correct context is the baseline that makes every refusal below
/// attributable to its one deviation. It is also the exact shape of an
/// abandoned plan: the leader opens the grant stream, receives `Pending`, and
/// never sends a stage. While the stream is open the follower holds the grant
/// and its query envelope; the moment the stream closes the follower must
/// return to its pre-grant baseline — no held grant, no query runtime or
/// governed memory, no spill directory — without waiting for the grant's
/// query deadline, which is set far beyond the observation bound.
///
/// # Errors
///
/// Returns a message when the follower refuses the correct context, does not
/// hold the grant and its envelope on the open stream, or does not return to
/// its baseline promptly after the close.
async fn an_abandoned_grant_returns_the_follower_to_baseline(
    cluster: &PeerCluster,
    plane: &ReservationPlane,
) -> Result<(), PeerJourneyError> {
    let ownership_before = cluster.ownership_snapshot(1)?;
    let spill_before = cluster
        .server(1)?
        .oracle_runtime_inspection()?
        .spill_directories;
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
    let mut grant = client
        .reserve_slots(request)
        .await
        .map_err(|status| format!("a correct reserve context was refused with {status}"))?
        .into_inner();
    let accepted = grant
        .message()
        .await
        .map_err(|status| format!("the grant stream failed before its verdict: {status}"))?
        .ok_or("the grant stream closed before its verdict")?;
    let Some(proto::reserve_node_slots_response::Outcome::Pending(_)) = accepted.outcome else {
        return Err(format!("a correct reserve context was not held: {accepted:?}").into());
    };
    let held = cluster.ownership_snapshot(1)?;
    if held.held_grants != ownership_before.held_grants + 1
        || held.root_analytical_queries != ownership_before.root_analytical_queries + 1
    {
        return Err(format!(
            "the open stream does not hold exactly one grant and its envelope: {held:?}"
        )
        .into());
    }

    // The abandonment: the leader drops its stream and sends nothing else.
    drop(grant);
    let deadline = tokio::time::Instant::now() + GRANT_CLOSE_DEADLINE;
    loop {
        let ownership = cluster.ownership_snapshot(1)?;
        let spill = cluster
            .server(1)?
            .oracle_runtime_inspection()?
            .spill_directories;
        if ownership == ownership_before && spill == spill_before {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "after its leader abandoned the grant the follower holds {ownership:?} with \
                 {spill} spill directories, not its {ownership_before:?} with {spill_before}"
            )
            .into());
        }
        tokio::time::sleep(GRANT_CLOSE_POLL).await;
    }
}

/// Bound on the follower observing a closed grant stream.
///
/// A stream close reaches the follower as an HTTP/2 reset within one round
/// trip, so this is generous by orders of magnitude and far below the grant's
/// own query deadline, which would otherwise be what freed it.
const GRANT_CLOSE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// Interval between ownership reads while a stream close propagates.
const GRANT_CLOSE_POLL: std::time::Duration = std::time::Duration::from_millis(20);

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

    let claim_deviations: [(&str, ClaimsDeviation); 3] = [
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
        (
            "a context for an operation this entry point does not implement",
            |claims| {
                claims.operation = claims.operation.wrapping_add(1);
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

/// Scribe pod in [`prove_peer_context_refusals`]'s topology.
const SCRIBE_POD: usize = 2;

/// A Scribe fragment whose context names another tenant reads nothing.
///
/// The leader's context is unsigned, so the receiver's comparison of the
/// context tenant with the tenant of every assignment it received is the only
/// thing keeping one tenant's hot tail away from another. The same live
/// fragment is sent twice over the Scribe's real peer listener: once with the
/// correct context, which executes, and once with only the context tenant
/// changed, which must be refused before the fragment executes or a row is
/// streamed.
///
/// # Errors
///
/// Returns a message when the correct fragment does not execute, or the
/// foreign-tenant fragment is answered or executes.
async fn a_foreign_tenant_context_reads_no_scribe_rows(
    cluster: &PeerCluster,
    plane: &ReservationPlane,
) -> Result<(), PeerJourneyError> {
    let fragment = ScribeFragment::live(cluster, plane, "peer_tenant_probe").await?;
    let mut client = OraclePeerServiceClient::new(
        PeerDial::member(cluster.peer_ca(), cluster.peer_addr(SCRIBE_POD)?)
            .connect()
            .await?,
    );

    let before = cluster.scribe_fragments(SCRIBE_POD)?;
    let mut accepted = client
        .execute_fragment(fragment.request(cluster.tenant().as_uuid())?)
        .await
        .map_err(|status| format!("a correct Scribe fragment was refused with {status}"))?
        .into_inner();
    let mut frames = 0_usize;
    while let Some(frame) = accepted.message().await? {
        drop(frame);
        frames += 1;
    }
    if frames == 0 || cluster.scribe_fragments(SCRIBE_POD)? != before + 1 {
        return Err("a correct Scribe fragment did not execute".into());
    }

    let before = cluster.scribe_fragments(SCRIBE_POD)?;
    match client
        .execute_fragment(fragment.request(uuid::Uuid::now_v7())?)
        .await
    {
        Err(status) if status.code() == tonic::Code::PermissionDenied => {}
        Err(status) => {
            return Err(format!("a foreign-tenant fragment failed with {status}").into());
        }
        Ok(_) => return Err("a foreign-tenant fragment opened a row stream".into()),
    }
    if cluster.scribe_fragments(SCRIBE_POD)? != before {
        return Err("a foreign-tenant fragment executed on the Scribe".into());
    }
    Ok(())
}

/// One live Scribe fragment built exactly as an Oracle leader builds it.
struct ScribeFragment {
    /// The fragment's single hot-provider assignment, owned by the real tenant.
    assignment: FollowerScanAssignment,
    /// Encoded placeholder plan the Scribe decodes.
    plan: Vec<u8>,
    /// Fingerprint of `plan`, bound by the context and the footer.
    fingerprint: String,
    /// Leader Oracle incarnation the fragment claims to come from.
    leader: OracleRoleFence,
    /// Scribe incarnation serving the live stream.
    target: OracleRoleFence,
}

impl ScribeFragment {
    /// Registers `table`, leaves rows live on the Scribe, and builds the
    /// fragment for the one partition the Scribe reports.
    ///
    /// # Errors
    ///
    /// Returns a message when the table cannot be written, the Scribe reports
    /// no live partition, or the plan cannot be encoded.
    async fn live(
        cluster: &PeerCluster,
        plane: &ReservationPlane,
        table: &str,
    ) -> Result<Self, PeerJourneyError> {
        cluster.register_table(SCRIBE_POD, table).await?;
        cluster.ingest_live_rows(SCRIBE_POD, table, 0, 8, 2).await?;
        let state = cluster.server(SCRIBE_POD)?.state();
        let catalog = state
            .bifrost_catalog()
            .ok_or("the Scribe pod composes no catalog")?;
        let scribe = state
            .bifrost_ingest()
            .ok_or("the Scribe pod composes no Scribe")?;
        let tenant = cluster.tenant();
        let table_ref = TableRef::new(BifrostNamespace::Bifrost, table);
        let binding = TenantTableBinding {
            tenant_id: tenant,
            namespace: "bifrost".to_owned(),
            table: table.to_owned(),
        };
        let (partition, stream) = scribe
            .tail_service()
            .list_active_streams(&binding)?
            .into_iter()
            .next()
            .ok_or("the Scribe reports no live partition")?;
        let schema = catalog.assignment_schema(&table_ref, tenant).await?;
        let schema_fingerprint = assignment_schema_fingerprint(&schema);
        let projected = Arc::new(schema.project(&[schema.index_of("id")?])?);
        let scan_id = format!(
            "oracle:bifrost.{table}:scribe:{}:{}:live",
            stream.node_id.as_uuid(),
            stream.writer_epoch
        );
        let plan = physical_plan_to_bytes_with_extension_codec(
            Arc::new(RemoteSourcePlaceholderExec::new(
                scan_id.clone(),
                schema_fingerprint.clone(),
                projected,
            )),
            &OraclePhysicalExtensionCodec::encoder(),
        )?
        .to_vec();
        Ok(Self {
            fingerprint: physical_plan_fingerprint(&plan),
            plan,
            assignment: FollowerScanAssignment {
                scan_id,
                binding,
                persisted: PersistedFileAssignment { files: Vec::new() },
                scribe_provider_cut: Some(ScribeProviderCut {
                    writer_epoch: stream.writer_epoch,
                    start_partition: partition,
                    end_partition: partition,
                }),
                schema_fingerprint,
                required_columns: vec!["id".to_owned()],
                predicates: Vec::new(),
            },
            leader: OracleRoleFence {
                node_id: NodeId::new(plane.leader_node_id),
                role: ClusterRole::Oracle,
                fencing_token: plane.leader_fence,
            },
            target: OracleRoleFence {
                node_id: stream.node_id,
                role: ClusterRole::Scribe,
                fencing_token: stream.writer_epoch,
            },
        })
    }

    /// Encodes the fragment with a context naming `context_tenant`.
    ///
    /// Every other claim is the leader's own, so a refusal is attributable to
    /// the tenant alone.
    ///
    /// # Errors
    ///
    /// Returns the assignment-authority digest failure unchanged.
    fn request(
        &self,
        context_tenant: uuid::Uuid,
    ) -> Result<proto::ExecuteFragmentRequest, PeerJourneyError> {
        let assignments = vec![self.assignment.clone()];
        let deadline = (chrono::Utc::now() + chrono::Duration::seconds(30)).timestamp_millis();
        let claims = PeerTicketClaims {
            protocol_version: PEER_PROTOCOL_VERSION,
            audience: self.target.node_id.as_uuid().as_bytes().to_vec(),
            worker_fence: self.target.fencing_token,
            leader_node_id: self.leader.node_id.as_uuid().as_bytes().to_vec(),
            leader_fence: self.leader.fencing_token,
            query_id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            tenant_id: context_tenant.as_bytes().to_vec(),
            expires_at_ms: deadline,
            execution_deadline_unix_ms: deadline,
            binding: format!(
                "{}.{}",
                self.assignment.binding.namespace, self.assignment.binding.table
            ),
            fragment_digest: self.fingerprint.clone(),
            manifest_digest: self.fingerprint.clone(),
            projection_digest: self.fingerprint.clone(),
            permission_digest: "journey-permissions".to_owned(),
            assignment_authority_digest: assignment_authority_digest_for(&assignments)
                .map_err(|error| format!("assignment-authority digest: {error:?}"))?,
        };
        Ok(ExecuteFragmentRequest {
            context: PeerContext {
                claims_bytes: claims.encode_to_vec(),
            },
            physical_plan_bytes: self.plan.clone(),
            reservation_id: ReservationId::new(uuid::Uuid::nil()),
            leader_fence: self.leader.clone(),
            target_fence: self.target.clone(),
            assignments,
            plan_fingerprint: self.fingerprint.clone(),
        }
        .into())
    }
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
