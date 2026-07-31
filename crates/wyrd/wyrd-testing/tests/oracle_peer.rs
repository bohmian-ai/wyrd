//! Focused real-tonic Oracle peer topology, parity, retry, and cleanup proof.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use futures_util::{Stream, StreamExt};
use parquet::arrow::ArrowWriter;
use secrecy::SecretString;
use tempfile::NamedTempFile;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::oracle::OracleSlotManager;
use vala_bifrost_redux::oracle::dispatcher::{
    DispatchCandidate, DispatchContext, DispatchError, FragmentDispatcher,
    LocalOraclePeerTransport, OraclePeerTransportDirectory, OraclePeerWorker,
    PEER_PROTOCOL_VERSION, ReservationRegistry, TonicOraclePeerTransport, WorkerExecution,
};
use vala_bifrost_redux::oracle::executor::SealedFragmentExecutor;
use vala_bifrost_redux::oracle::fragment::{SealedScanFile, SealedScanFragment, SealedSourceTier};
use vala_bifrost_redux::oracle::peer::{
    PeerSecurityAudit, PeerSecurityAuditError, PeerTicketClaims, PeerTicketVerifier,
    projection_digest,
};
use wyrd_server::oracle::OraclePeerAuthority;
use wyrd_spec::vala::api::{
    ExecuteFragmentRequest, NodeId, PendingNodeReservation, QueryClass, QueryId,
    ReserveNodeSlotsRequest, ReserveNodeSlotsResponse,
};
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient;
use wyrd_tonic::wyrd::v1::oracle_peer_service_server::{
    OraclePeerService, OraclePeerServiceServer,
};
use wyrd_tonic::wyrd::v1::{
    self as proto, ExecuteFragmentRequest as ProtoExecuteFragmentRequest,
    ReleaseNodeSlotsRequest as ProtoReleaseNodeSlotsRequest,
    ReserveNodeSlotsRequest as ProtoReserveNodeSlotsRequest,
};

/// Fixed PKCS#8 test authority shared by all three independent roles.
const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

/// Counts security-audit calls so valid completion can prove it emitted none.
#[derive(Default)]
struct RecordingPeerAudit {
    /// Number of attempted durable security rows.
    calls: Mutex<usize>,
}

#[wyrd_tonic::tonic::async_trait]
impl PeerSecurityAudit for RecordingPeerAudit {
    /// Counts an unexpected unverified rejection.
    ///
    /// # Errors
    /// This recorder never fails.
    async fn append_unverified_ticket_rejection(
        &self,
        _violation: wyrd_spec::vala::api::BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        *self.calls.lock().expect("audit mutex") += 1;
        Ok(())
    }

    /// Counts an unexpected verified rejection.
    ///
    /// # Errors
    /// This recorder never fails.
    async fn append_verified_ticket_violation(
        &self,
        _tenant_id: wyrd_spec::DataTenantId,
        _violation: wyrd_spec::vala::api::BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        *self.calls.lock().expect("audit mutex") += 1;
        Ok(())
    }
}

/// Generated-tonic test adapter around the production Redux peer worker.
struct PeerTestService {
    /// Worker owner under test.
    worker: Arc<OraclePeerWorker>,
    /// Deterministic fault injected before worker execution.
    fail_execute: bool,
}

#[wyrd_tonic::tonic::async_trait]
impl OraclePeerService for PeerTestService {
    /// Footer-terminated attempt stream retaining the running guard.
    type ExecuteFragmentStream =
        Pin<Box<dyn Stream<Item = Result<proto::WorkerAttemptFrame, Status>> + Send>>;

    /// Reserves through the production registry and generated conversion.
    ///
    /// # Errors
    /// Returns invalid argument for malformed generated fields.
    async fn reserve_slots(
        &self,
        request: Request<ProtoReserveNodeSlotsRequest>,
    ) -> Result<Response<proto::ReserveNodeSlotsResponse>, Status> {
        let request = wyrd_spec::vala::api::ReserveNodeSlotsRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        Ok(Response::new(self.worker.reserve(request).into()))
    }

    /// Releases through the production tuple-bound registry.
    ///
    /// # Errors
    /// Returns invalid argument for malformed generated fields.
    async fn release_slots(
        &self,
        request: Request<ProtoReleaseNodeSlotsRequest>,
    ) -> Result<Response<proto::ReleaseNodeSlotsResponse>, Status> {
        let request = wyrd_spec::vala::api::ReleaseNodeSlotsRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        self.worker.release(&request);
        Ok(Response::new(proto::ReleaseNodeSlotsResponse {}))
    }

    /// Executes or injects one retryable peer loss before ticket consumption.
    ///
    /// # Errors
    /// Returns the injected loss, conversion failure, or worker rejection.
    async fn execute_fragment(
        &self,
        request: Request<ProtoExecuteFragmentRequest>,
    ) -> Result<Response<Self::ExecuteFragmentStream>, Status> {
        if self.fail_execute {
            return Err(Status::unavailable("injected peer loss"));
        }
        let request = ExecuteFragmentRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let WorkerExecution { mut stream } = self
            .worker
            .execute(request)
            .await
            .map_err(|error| Status::permission_denied(error.to_string()))?;
        let output = async_stream::stream! {
            let mut index = 0_usize;
            while let Some(frame) = stream.next().await {
                if index > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                index += 1;
                yield frame
                    .map(Into::into)
                    .map_err(|error| Status::permission_denied(error.to_string()));
            }
        };
        Ok(Response::new(Box::pin(output)))
    }
}

/// Running real tonic peer plus its observable reservation registry.
struct RunningPeer {
    /// Bound peer address.
    address: SocketAddr,
    /// Pending reservations retained by this worker.
    reservations: Arc<ReservationRegistry>,
    /// Server shutdown signal.
    shutdown: CancellationToken,
    /// Serving task.
    task: tokio::task::JoinHandle<()>,
}

impl RunningPeer {
    /// Stops the independently bound tonic peer.
    async fn stop(self) {
        self.shutdown.cancel();
        let _ = self.task.await;
    }
}

/// Binds and starts one generated tonic peer service.
async fn start_peer(
    node: NodeId,
    fence: u64,
    authority: Arc<OraclePeerAuthority>,
    security_audit: Arc<dyn PeerSecurityAudit>,
    fail_execute: bool,
) -> RunningPeer {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("peer listener");
    let address = socket.local_addr().expect("peer address");
    drop(socket);
    let reservations = Arc::new(ReservationRegistry::new(
        Arc::new(OracleSlotManager::new(8, 1)),
        8,
    ));
    let verifier: Arc<dyn PeerTicketVerifier> = authority;
    let worker = Arc::new(OraclePeerWorker::new(
        node,
        fence,
        verifier,
        security_audit,
        Arc::clone(&reservations),
        SealedFragmentExecutor::default(),
    ));
    let shutdown = CancellationToken::new();
    let shutdown_task = shutdown.clone();
    let task = tokio::spawn(async move {
        wyrd_tonic::tonic::transport::Server::builder()
            .add_service(OraclePeerServiceServer::new(PeerTestService {
                worker,
                fail_execute,
            }))
            .serve_with_shutdown(address, shutdown_task.cancelled_owned())
            .await
            .expect("peer server");
    });
    tokio::task::yield_now().await;
    RunningPeer {
        address,
        reservations,
        shutdown,
        task,
    }
}

/// Builds a production-shaped directory for tests whose candidates are remote-only.
fn transport_directory(
    leader: NodeId,
    authority: Arc<OraclePeerAuthority>,
    security_audit: Arc<dyn PeerSecurityAudit>,
    remote: TonicOraclePeerTransport,
) -> OraclePeerTransportDirectory {
    let verifier: Arc<dyn PeerTicketVerifier> = authority;
    let local_worker = Arc::new(OraclePeerWorker::new(
        leader,
        11,
        verifier,
        security_audit,
        Arc::new(ReservationRegistry::new(
            Arc::new(OracleSlotManager::new(8, 8)),
            8,
        )),
        SealedFragmentExecutor::default(),
    ));
    OraclePeerTransportDirectory::new(
        leader,
        Arc::new(LocalOraclePeerTransport::new(local_worker)),
        Arc::new(remote),
    )
}

/// Writes one immutable Parquet file and returns its closed fragment.
fn fragment(file: &NamedTempFile) -> SealedScanFragment {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Int64Array::from(vec![1_i64, 2, 3]))],
    )
    .expect("record batch");
    let output = std::fs::File::create(file.path()).expect("parquet output");
    let mut writer = ArrowWriter::try_new(output, Arc::clone(&schema), None).expect("writer");
    writer.write(&batch).expect("parquet batch");
    writer.close().expect("parquet close");
    let location = file.path().to_string_lossy().into_owned();
    let binding = file
        .path()
        .parent()
        .expect("temp parent")
        .to_string_lossy()
        .into_owned();
    let schema_fingerprint = hex::encode(
        vala_bifrost_redux::schema::SchemaFingerprint::from_arrow_schema(&schema).as_ref(),
    );
    let size_bytes = std::fs::metadata(file.path())
        .expect("parquet metadata")
        .len();
    let mut fragment = SealedScanFragment {
        fragment_id: String::new(),
        binding,
        tier: SealedSourceTier::HotSealed,
        pinned_digest: "manifest".to_owned(),
        files: vec![SealedScanFile {
            location,
            row_groups: Vec::new(),
            size_bytes,
            estimated_rows: 3,
        }],
        projection: Vec::new(),
        predicates: Vec::new(),
        schema_fingerprint,
        estimated_rows: 3,
        estimated_bytes: size_bytes,
        deadline_unix_ms: (chrono::Utc::now() + chrono::Duration::seconds(10)).timestamp_millis(),
    };
    fragment.fragment_id = fragment.digest();
    fragment
}

/// Builds one immutable dispatch context.
fn context(leader: NodeId) -> DispatchContext {
    DispatchContext {
        query_id: QueryId::new(uuid::Uuid::now_v7()),
        leader_node_id: leader,
        leader_fence: 11,
        tenant_id: uuid::Uuid::now_v7(),
        query_class: QueryClass::Interactive,
        slot_units: 1,
        permission_digest: "permission".to_owned(),
        attempt_bytes: 8 * 1024 * 1024,
        attempt_memory_bytes: 1,
    }
}

/// Mints one exact request for a previously accepted pending reservation.
fn execute_request(
    authority: &OraclePeerAuthority,
    context: &DispatchContext,
    worker: NodeId,
    worker_fence: u64,
    pending: &PendingNodeReservation,
    fragment: &SealedScanFragment,
) -> ExecuteFragmentRequest {
    let claims = PeerTicketClaims {
        protocol_version: PEER_PROTOCOL_VERSION,
        audience: worker.as_uuid().as_bytes().to_vec(),
        worker_fence,
        leader_node_id: context.leader_node_id.as_uuid().as_bytes().to_vec(),
        leader_fence: context.leader_fence,
        query_id: context.query_id.as_uuid().as_bytes().to_vec(),
        tenant_id: context.tenant_id.as_bytes().to_vec(),
        nonce: uuid::Uuid::now_v7().as_bytes().to_vec(),
        expires_at_ms: pending
            .expires_at
            .timestamp_millis()
            .min(fragment.deadline_unix_ms),
        binding: fragment.binding.clone(),
        fragment_digest: fragment.fragment_id.clone(),
        manifest_digest: fragment.pinned_digest.clone(),
        projection_digest: projection_digest(&fragment.projection),
        permission_digest: context.permission_digest.clone(),
    };
    ExecuteFragmentRequest {
        ticket: authority.mint(&claims).expect("ticket"),
        fragment_bytes: fragment.encode().expect("fragment"),
        reservation_id: pending.reservation_id,
    }
}

/// One dispatcher retries a failed remote peer on its in-process leader with parity and cleanup.
#[tokio::test]
async fn oracle_peer_remote_failure_falls_back_to_leader_local() {
    let audit = Arc::new(RecordingPeerAudit::default());
    let security_audit: Arc<dyn PeerSecurityAudit> = audit.clone();
    let authority = Arc::new(
        OraclePeerAuthority::from_pem(
            &SecretString::from(PRIVATE_KEY_PEM),
            Arc::clone(&security_audit),
        )
        .expect("key"),
    );
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let failed_node = NodeId::new(uuid::Uuid::now_v7());
    let failed = start_peer(
        failed_node,
        21,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        true,
    )
    .await;
    let file = NamedTempFile::new().expect("parquet temp");
    let fragment = fragment(&file);

    let local_registry = Arc::new(ReservationRegistry::new(
        Arc::new(OracleSlotManager::new(8, 8)),
        8,
    ));
    let local_verifier: Arc<dyn PeerTicketVerifier> = authority.clone();
    let local_worker = Arc::new(OraclePeerWorker::new(
        leader,
        11,
        local_verifier,
        Arc::clone(&security_audit),
        Arc::clone(&local_registry),
        SealedFragmentExecutor::default(),
    ));
    let dispatcher = FragmentDispatcher::new(
        authority,
        OraclePeerTransportDirectory::new(
            leader,
            Arc::new(LocalOraclePeerTransport::new(local_worker)),
            Arc::new(
                TonicOraclePeerTransport::new(
                    HashMap::from([(failed_node, format!("http://{}", failed.address))]),
                    None,
                )
                .expect("transport"),
            ),
        ),
    );
    let expected = dispatcher
        .execute(
            &context(leader),
            fragment.clone(),
            &[DispatchCandidate {
                node_id: leader,
                worker_fence: 11,
            }],
        )
        .await
        .expect("local attempt");

    let actual = dispatcher
        .execute(
            &context(leader),
            fragment,
            &[
                DispatchCandidate {
                    node_id: failed_node,
                    worker_fence: 21,
                },
                DispatchCandidate {
                    node_id: leader,
                    worker_fence: 11,
                },
            ],
        )
        .await
        .expect("leader-local retry succeeds");
    assert_eq!(actual.schema, expected.schema);
    let actual_batches = actual
        .batches
        .collect::<Result<Vec<_>, _>>()
        .expect("actual batches read");
    let expected_batches = expected
        .batches
        .collect::<Result<Vec<_>, _>>()
        .expect("expected batches read");
    assert_eq!(actual_batches, expected_batches);
    assert_eq!(failed.reservations.cleanup_expired(chrono::Utc::now()), 0);
    assert_eq!(local_registry.cleanup_expired(chrono::Utc::now()), 0);
    assert_eq!(*audit.calls.lock().expect("audit mutex"), 0);

    failed.stop().await;
}

/// Three accepted placements fail in order and each pending reservation releases immediately.
#[tokio::test]
async fn oracle_peer_three_failures_exhaust_and_release_partial_placement() {
    let audit = Arc::new(RecordingPeerAudit::default());
    let security_audit: Arc<dyn PeerSecurityAudit> = audit.clone();
    let authority = Arc::new(
        OraclePeerAuthority::from_pem(
            &SecretString::from(PRIVATE_KEY_PEM),
            Arc::clone(&security_audit),
        )
        .expect("key"),
    );
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let specs = [(31_u64, 1_u128), (32, 2), (33, 3)];
    let mut peers = Vec::new();
    let mut addresses = HashMap::new();
    let mut candidates = Vec::new();
    for (fence, identity) in specs {
        let node = NodeId::new(uuid::Uuid::from_u128(identity));
        let peer = start_peer(
            node,
            fence,
            Arc::clone(&authority),
            Arc::clone(&security_audit),
            true,
        )
        .await;
        addresses.insert(node, format!("http://{}", peer.address));
        candidates.push(DispatchCandidate {
            node_id: node,
            worker_fence: fence,
        });
        peers.push(peer);
    }
    let file = NamedTempFile::new().expect("parquet temp");
    let transports = transport_directory(
        leader,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        TonicOraclePeerTransport::new(addresses, None).expect("transport"),
    );
    let dispatcher = FragmentDispatcher::new(authority, transports);
    let error = dispatcher
        .execute(&context(leader), fragment(&file), &candidates)
        .await
        .expect_err("three distinct failures exhaust");
    assert!(matches!(error, DispatchError::Exhausted));
    for peer in &peers {
        assert_eq!(
            peer.reservations.cleanup_expired(chrono::Utc::now()),
            0,
            "accepted pending placement must release before retry",
        );
    }
    assert_eq!(*audit.calls.lock().expect("audit mutex"), 0);
    for peer in peers {
        peer.stop().await;
    }
}

/// A restarted tonic role rejects the old fence and accepts only a fresh fenced ticket.
#[tokio::test]
async fn oracle_peer_restart_rejects_old_fence_and_releases_reservation() {
    let audit = Arc::new(RecordingPeerAudit::default());
    let security_audit: Arc<dyn PeerSecurityAudit> = audit.clone();
    let authority = Arc::new(
        OraclePeerAuthority::from_pem(
            &SecretString::from(PRIVATE_KEY_PEM),
            Arc::clone(&security_audit),
        )
        .expect("key"),
    );
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let worker = NodeId::new(uuid::Uuid::now_v7());
    let old = start_peer(
        worker,
        40,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        false,
    )
    .await;
    old.stop().await;
    let restarted = start_peer(
        worker,
        41,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        false,
    )
    .await;
    let transports = transport_directory(
        leader,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        TonicOraclePeerTransport::new(
            HashMap::from([(worker, format!("http://{}", restarted.address))]),
            None,
        )
        .expect("transport"),
    );
    let dispatcher = FragmentDispatcher::new(authority, transports);
    let file = NamedTempFile::new().expect("parquet temp");
    let stale = dispatcher
        .execute(
            &context(leader),
            fragment(&file),
            &[DispatchCandidate {
                node_id: worker,
                worker_fence: 40,
            }],
        )
        .await
        .expect_err("old role fence rejects");
    assert!(matches!(stale, DispatchError::Terminal));
    assert_eq!(
        restarted.reservations.cleanup_expired(chrono::Utc::now()),
        0,
    );
    dispatcher
        .execute(
            &context(leader),
            fragment(&file),
            &[DispatchCandidate {
                node_id: worker,
                worker_fence: 41,
            }],
        )
        .await
        .expect("fresh role fence succeeds");
    assert_eq!(
        restarted.reservations.cleanup_expired(chrono::Utc::now()),
        0,
    );
    assert_eq!(*audit.calls.lock().expect("audit mutex"), 1);
    restarted.stop().await;
}

/// Dropping a tonic attempt before its footer releases the running slot for fresh work.
#[tokio::test]
async fn oracle_peer_tonic_cancellation_releases_running_slot() {
    let audit = Arc::new(RecordingPeerAudit::default());
    let security_audit: Arc<dyn PeerSecurityAudit> = audit.clone();
    let authority = Arc::new(
        OraclePeerAuthority::from_pem(
            &SecretString::from(PRIVATE_KEY_PEM),
            Arc::clone(&security_audit),
        )
        .expect("key"),
    );
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let worker = NodeId::new(uuid::Uuid::now_v7());
    let peer = start_peer(
        worker,
        51,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        false,
    )
    .await;
    let address = format!("http://{}", peer.address);
    let mut client = OraclePeerServiceClient::connect(address.clone())
        .await
        .expect("client");
    let dispatch_context = context(leader);
    let reserve = ReserveNodeSlotsRequest {
        query_id: dispatch_context.query_id,
        leader_node_id: leader,
        leader_fencing_token: dispatch_context.leader_fence,
        query_class: dispatch_context.query_class,
        slot_units: 1,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(2),
    };
    let response: ReserveNodeSlotsResponse = client
        .reserve_slots(Request::new(reserve.into()))
        .await
        .expect("reserve")
        .into_inner()
        .try_into()
        .expect("reserve conversion");
    let ReserveNodeSlotsResponse::Pending(pending) = response else {
        panic!("pending reservation expected");
    };
    let file = NamedTempFile::new().expect("parquet temp");
    let sealed = fragment(&file);
    let request = execute_request(&authority, &dispatch_context, worker, 51, &pending, &sealed);
    let mut stream = client
        .execute_fragment(Request::new(request.clone().into()))
        .await
        .expect("execute")
        .into_inner();
    assert!(stream.message().await.expect("schema frame").is_some());
    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let replay = client
        .execute_fragment(Request::new(request.into()))
        .await
        .expect_err("consumed ticket replay rejects before fragment access");
    assert_eq!(replay.code(), wyrd_tonic::tonic::Code::PermissionDenied);

    let transports = transport_directory(
        leader,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        TonicOraclePeerTransport::new(HashMap::from([(worker, address)]), None).expect("transport"),
    );
    let dispatcher = FragmentDispatcher::new(authority, transports);
    dispatcher
        .execute(
            &context(leader),
            sealed,
            &[DispatchCandidate {
                node_id: worker,
                worker_fence: 51,
            }],
        )
        .await
        .expect("fresh work reuses released running slot");
    assert_eq!(peer.reservations.cleanup_expired(chrono::Utc::now()), 0);
    assert_eq!(*audit.calls.lock().expect("audit mutex"), 1);
    peer.stop().await;
}
