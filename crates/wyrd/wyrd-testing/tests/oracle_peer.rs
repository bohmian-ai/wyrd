//! Focused real-tonic Oracle peer topology, parity, retry, and cleanup proof.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use futures_util::{Stream, StreamExt};
use parquet::arrow::ArrowWriter;
use secrecy::SecretString;
use tempfile::NamedTempFile;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::oracle::OracleSlotManager;
use vala_bifrost_redux::oracle::dispatcher::{
    DispatchCandidate, DispatchContext, DispatchError, FragmentDispatcher,
    LocalOraclePeerTransport, OraclePeerCredentials, OraclePeerTls, OraclePeerTransport,
    OraclePeerTransportDirectory, OraclePeerWorker, PEER_PROTOCOL_VERSION, ReservationRegistry,
    TonicOraclePeerTransport, WorkerExecution,
};

/// Deterministic credential seam recording normal and forced bearer requests.
struct RefreshProbeCredentials {
    /// Number of forced refreshes requested after `Unauthenticated`.
    forced: AtomicUsize,
}

/// Credential probe proving transport trust fails before bearer acquisition.
struct BearerCallProbe {
    /// Total bearer requests observed by the probe.
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl OraclePeerCredentials for BearerCallProbe {
    /// Records any forbidden bearer acquisition.
    ///
    /// # Errors
    /// This deterministic probe never fails.
    async fn bearer(&self, _force_refresh: bool) -> Result<String, DispatchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok("must-not-be-read".to_owned())
    }
}

#[async_trait::async_trait]
impl OraclePeerCredentials for RefreshProbeCredentials {
    async fn bearer(&self, force_refresh: bool) -> Result<String, DispatchError> {
        if force_refresh {
            self.forced.fetch_add(1, Ordering::SeqCst);
            Ok("fresh".to_owned())
        } else {
            Ok("stale".to_owned())
        }
    }
}

/// Real-tonic peer that rejects the stale bearer once and accepts the refreshed bearer.
struct RefreshProbeService {
    /// Total reservation calls across the initial attempt and retry.
    calls: Arc<AtomicUsize>,
}

#[wyrd_tonic::tonic::async_trait]
impl OraclePeerService for RefreshProbeService {
    type ExecuteFragmentStream =
        Pin<Box<dyn Stream<Item = Result<proto::WorkerAttemptFrame, Status>> + Send>>;

    async fn reserve_slots(
        &self,
        request: Request<ProtoReserveNodeSlotsRequest>,
    ) -> Result<Response<proto::ReserveNodeSlotsResponse>, Status> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let bearer = request
            .metadata()
            .get("x-wyrd-access-token")
            .and_then(|value| value.to_str().ok());
        if bearer == Some("Bearer stale") {
            return Err(Status::unauthenticated("stale"));
        }
        if bearer != Some("Bearer fresh") {
            return Err(Status::permission_denied("wrong bearer"));
        }
        Ok(Response::new(proto::ReserveNodeSlotsResponse {
            outcome: Some(proto::reserve_node_slots_response::Outcome::Pending(
                proto::PendingNodeReservation {
                    reservation_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
                    expires_at_unix_ms: u64::try_from(
                        chrono::Utc::now().timestamp_millis() + 1_000,
                    )
                    .expect("current timestamp is positive"),
                },
            )),
        }))
    }

    async fn release_slots(
        &self,
        _request: Request<ProtoReleaseNodeSlotsRequest>,
    ) -> Result<Response<proto::ReleaseNodeSlotsResponse>, Status> {
        Err(Status::unimplemented("unused"))
    }

    async fn execute_fragment(
        &self,
        _request: Request<ProtoExecuteFragmentRequest>,
    ) -> Result<Response<Self::ExecuteFragmentStream>, Status> {
        Err(Status::unimplemented("unused"))
    }
}
use vala_bifrost_redux::oracle::executor::SealedFragmentExecutor;
use vala_bifrost_redux::oracle::fragment::{SealedScanFile, SealedScanFragment, SealedSourceTier};
use vala_bifrost_redux::oracle::peer::{
    PeerSecurityAudit, PeerSecurityAuditError, PeerTicketClaims, PeerTicketMinter,
    PeerTicketVerifier, projection_digest,
};
use wyrd_semver::VersionBlock;
use wyrd_server::oracle::OraclePeerAuthority;
use wyrd_server::oracle::PostgresPeerSecurityAudit;
use wyrd_spec::DataTenantId;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{
    ExecuteFragmentRequest, NodeId, OracleCapabilitiesV1, PendingNodeReservation, QueryClass,
    QueryId, ReleaseNodeSlotsRequest, ReserveNodeSlotsRequest, ReserveNodeSlotsResponse,
};
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_testing::{Bootstrap, WyrdTestEnv};
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

/// Self-signed localhost certificate used only by the private peer TLS journey.
const PEER_TLS_CERTIFICATE_PEM: &str = "-----BEGIN CERTIFICATE-----\nMIIBejCCASygAwIBAgIUJeVIuSWVQLAAGvPJqorJ35AzIGowBQYDK2VwMBcxFTAT\nBgNVBAMMDFd5cmQgVGVzdCBDQTAeFw0yNjA4MDEyMjI3MzNaFw0zNjA3MjkyMjI3\nMzNaMBQxEjAQBgNVBAMMCWxvY2FsaG9zdDAqMAUGAytlcAMhAMQ7zYZh/326Ow2l\n47We9Ry2DX+dsle7/7KJ3xuOyV+go4GMMIGJMBQGA1UdEQQNMAuCCWxvY2FsaG9z\ndDAMBgNVHRMBAf8EAjAAMA4GA1UdDwEB/wQEAwIHgDATBgNVHSUEDDAKBggrBgEF\nBQcDATAdBgNVHQ4EFgQUpKZ9RXjCNCa7WdnZ/RbnQTjeeIswHwYDVR0jBBgwFoAU\nffiXuERzxz/wUzFHryjCu9kawNcwBQYDK2VwA0EAIAItrDZanEwE2rY8ZC8oXu2l\n3YuscufK/YpVoroZxjer7ANpKL2nf+Q7DdnVThHx7ZY0rHNr28P2+db0u/P6Aw==\n-----END CERTIFICATE-----\n";

/// Test certificate authority that signs [`PEER_TLS_CERTIFICATE_PEM`].
const PEER_TLS_CA_PEM: &str = "-----BEGIN CERTIFICATE-----\nMIIBUzCCAQWgAwIBAgIUIvBol5vUTE7Ep82M4nwyeiqWtfowBQYDK2VwMBcxFTAT\nBgNVBAMMDFd5cmQgVGVzdCBDQTAeFw0yNjA4MDEyMjI3MzNaFw0zNjA3MjkyMjI3\nMzNaMBcxFTATBgNVBAMMDFd5cmQgVGVzdCBDQTAqMAUGAytlcAMhAFLeEdcicBoj\nHvTVoGMRRX++lXDHJs/oSbiUsUFj3cJSo2MwYTAdBgNVHQ4EFgQUffiXuERzxz/w\nUzFHryjCu9kawNcwHwYDVR0jBBgwFoAUffiXuERzxz/wUzFHryjCu9kawNcwDwYD\nVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwBQYDK2VwA0EAZYpU+nqGryWC\n585mnbRAKnwY4b3MC9fjOfbc/ImO5Kt71Bazy1SKjAvdlGM01ayDZZM6nV7leZc6\nHYBI3uvWBA==\n-----END CERTIFICATE-----\n";

/// Private key paired with [`PEER_TLS_CERTIFICATE_PEM`] for test serving only.
const PEER_TLS_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEICZy1zJGs9jdufNpelf5EbzDgCAnMIslxtG6IvMvz3P1\n-----END PRIVATE KEY-----\n";

/// Independent test CA that does not authenticate the localhost peer certificate.
const UNTRUSTED_PEER_CA_PEM: &str = "-----BEGIN CERTIFICATE-----\nMIIBUzCCAQWgAwIBAgIUHH+ijx5UL3SHgI2fsI6qAXbExe0wBQYDK2VwMBQxEjAQ\nBgNVBAMMCW90aGVyaG9zdDAeFw0yNjA4MDEyMjIxMzdaFw0zNjA3MjkyMjIxMzda\nMBQxEjAQBgNVBAMMCW90aGVyaG9zdDAqMAUGAytlcAMhAPFN5MLyjXXFjohd5jvz\nbtiVpdxlVrdM/pJUlDG1D+1jo2kwZzAdBgNVHQ4EFgQUVM7sCG+JzrB0mqm0nEh2\nj94cTaMwHwYDVR0jBBgwFoAUVM7sCG+JzrB0mqm0nEh2j94cTaMwDwYDVR0TAQH/\nBAUwAwEB/zAUBgNVHREEDTALgglvdGhlcmhvc3QwBQYDK2VwA0EAXHVKQb5GNjQ8\nQ6n7fd8aojlerlNHClCipU/9+5W0UEbsOte5Zx4YdtX25dOmg9DqCqODoVmoMDnb\nOEYUyRxkCQ==\n-----END CERTIFICATE-----\n";

/// Attach one peer bearer to any generated tonic request.
fn attach_peer_bearer<T>(request: &mut Request<T>, token: &str) {
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {token}").parse().expect("bearer metadata"),
    );
}

/// Counts security-audit calls so valid completion can prove it emitted none.
#[derive(Default)]
struct RecordingPeerAudit {
    /// Number of attempted durable security rows.
    calls: Mutex<usize>,
    /// Scrubbed violation classes retained without principal or request detail.
    violations: Mutex<Vec<wyrd_spec::vala::api::BifrostSecurityViolationKind>>,
}

#[wyrd_tonic::tonic::async_trait]
impl PeerSecurityAudit for RecordingPeerAudit {
    /// Counts an unexpected unverified rejection.
    ///
    /// # Errors
    /// This recorder never fails.
    async fn append_unverified_ticket_rejection(
        &self,
        violation: wyrd_spec::vala::api::BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        *self.calls.lock().expect("audit mutex") += 1;
        self.violations
            .lock()
            .expect("violation mutex")
            .push(violation);
        Ok(())
    }

    /// Counts an unexpected verified rejection.
    ///
    /// # Errors
    /// This recorder never fails.
    async fn append_verified_ticket_violation(
        &self,
        _tenant_id: wyrd_spec::DataTenantId,
        violation: wyrd_spec::vala::api::BifrostSecurityViolationKind,
    ) -> Result<(), PeerSecurityAuditError> {
        *self.calls.lock().expect("audit mutex") += 1;
        self.violations
            .lock()
            .expect("violation mutex")
            .push(violation);
        Ok(())
    }
}

/// Generated-tonic test adapter around the production Redux peer worker.
struct PeerTestService {
    /// Worker owner under test.
    worker: Arc<OraclePeerWorker>,
    /// Deterministic fault injected before worker execution.
    fault: PeerFault,
}

/// Deterministic production-stream fault injected by the tonic test adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerFault {
    /// Preserve the production worker stream.
    None,
    /// Fail before the worker consumes the ticket.
    BeforeExecute,
    /// Change a footer count after production encoding.
    CorruptFooter,
    /// End the transport stream before the required footer.
    MissingFooter,
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
        Ok(Response::new(self.worker.reserve(&request).into()))
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
        if self.fault == PeerFault::BeforeExecute {
            return Err(Status::unavailable("injected peer loss"));
        }
        let request = ExecuteFragmentRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let WorkerExecution { mut stream } = self
            .worker
            .execute(request)
            .await
            .map_err(|error| Status::permission_denied(error.to_string()))?;
        let fault = self.fault;
        let output = async_stream::stream! {
            let mut index = 0_usize;
            while let Some(frame) = stream.next().await {
                if index > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                index += 1;
                let frame = frame.map_err(|error| Status::permission_denied(error.to_string()))?;
                match (fault, frame) {
                    (PeerFault::MissingFooter, wyrd_spec::vala::api::WorkerAttemptFrame::Footer(_)) => return,
                    (PeerFault::CorruptFooter, wyrd_spec::vala::api::WorkerAttemptFrame::Footer(mut footer)) => {
                        footer.row_count = footer.row_count.saturating_add(1);
                        yield Ok(wyrd_spec::vala::api::WorkerAttemptFrame::Footer(footer).into());
                    }
                    (_, frame) => yield Ok(frame.into()),
                }
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
    fault: PeerFault,
) -> RunningPeer {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("peer listener");
    let address = socket.local_addr().expect("peer address");
    drop(socket);
    let reservations = Arc::new(ReservationRegistry::new(
        Arc::new(OracleSlotManager::new(8, 1)),
        8,
    ));
    let verifier: Arc<dyn PeerTicketVerifier> = authority.clone();
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
                fault,
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

/// Binds one CA-authenticated generated tonic peer for production transport proofs.
async fn start_tls_peer(
    node: NodeId,
    fence: u64,
    authority: Arc<OraclePeerAuthority>,
    security_audit: Arc<dyn PeerSecurityAudit>,
) -> RunningPeer {
    wyrd_tls::install_crypto_provider().expect("test installs Wyrd TLS provider");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("peer listener");
    let address = listener.local_addr().expect("peer address");
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
        let router = wyrd_tonic::tonic::transport::Server::builder()
            .tls_config(
                wyrd_tonic::tonic::transport::ServerTlsConfig::new().identity(
                    wyrd_tonic::tonic::transport::Identity::from_pem(
                        PEER_TLS_CERTIFICATE_PEM,
                        PEER_TLS_PRIVATE_KEY_PEM,
                    ),
                ),
            )
            .expect("peer TLS identity")
            .add_service(OraclePeerServiceServer::new(PeerTestService {
                worker,
                fault: PeerFault::None,
            }));
        wyrd_tonic::server::serve_grpc_with_listener(router, listener, shutdown_task)
            .await
            .expect("peer server");
    });
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
    let verifier: Arc<dyn PeerTicketVerifier> = authority.clone();
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
        query_memory_pool: None,
        cancellation: CancellationToken::new(),
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
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

/// Reserves one pending slot through the generated tonic client.
///
/// # Panics
///
/// Panics when transport, conversion, or capacity prevents the isolated peer
/// from accepting the reservation.
async fn reserve_pending(
    client: &mut OraclePeerServiceClient<wyrd_tonic::tonic::transport::Channel>,
    context: &DispatchContext,
) -> PendingNodeReservation {
    let response: ReserveNodeSlotsResponse = client
        .reserve_slots(Request::new(
            ReserveNodeSlotsRequest {
                query_id: context.query_id,
                leader_node_id: context.leader_node_id,
                leader_fencing_token: context.leader_fence,
                query_class: context.query_class,
                slot_units: context.slot_units,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(2),
            }
            .into(),
        ))
        .await
        .expect("reserve transport")
        .into_inner()
        .try_into()
        .expect("reserve conversion");
    let ReserveNodeSlotsResponse::Pending(pending) = response else {
        panic!("isolated peer must accept one pending reservation");
    };
    pending
}

/// One dispatcher retries a failed remote peer on its in-process leader with parity and cleanup.
pub(crate) async fn prove_oracle_peer_remote_failure_falls_back_to_leader_local() {
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
        PeerFault::BeforeExecute,
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

/// Runs the remote-failure fallback proof as a focused peer test.
#[tokio::test]
async fn oracle_peer_remote_failure_falls_back_to_leader_local() {
    prove_oracle_peer_remote_failure_falls_back_to_leader_local().await;
}

/// Three accepted placements fail in order and each pending reservation releases immediately.
pub(crate) async fn prove_oracle_peer_three_failures_exhaust_and_release_partial_placement() {
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
            PeerFault::BeforeExecute,
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

/// Runs the partial-placement exhaustion proof as a focused peer test.
#[tokio::test]
async fn oracle_peer_three_failures_exhaust_and_release_partial_placement() {
    prove_oracle_peer_three_failures_exhaust_and_release_partial_placement().await;
}

/// A restarted tonic role rejects the old fence and accepts only a fresh fenced ticket.
pub(crate) async fn prove_oracle_peer_restart_rejects_old_fence_and_releases_reservation() {
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
        PeerFault::None,
    )
    .await;
    old.stop().await;
    let restarted = start_peer(
        worker,
        41,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        PeerFault::None,
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

/// Runs the restarted-fence replay proof as a focused peer test.
#[tokio::test]
async fn oracle_peer_restart_rejects_old_fence_and_releases_reservation() {
    prove_oracle_peer_restart_rejects_old_fence_and_releases_reservation().await;
}

/// Dropping a tonic attempt before its footer releases the running slot for fresh work.
pub(crate) async fn prove_oracle_peer_tonic_cancellation_releases_running_slot() {
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
        PeerFault::None,
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

/// Runs the tonic cancellation and replay proof as a focused peer test.
#[tokio::test]
async fn oracle_peer_tonic_cancellation_releases_running_slot() {
    prove_oracle_peer_tonic_cancellation_releases_running_slot().await;
}

/// Tonic worker rejects signed-permission, payload, and raw-ticket tamper before storage output.
pub(crate) async fn prove_oracle_peer_tonic_rejects_ticket_payload_and_permission_tamper() {
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
        61,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
        PeerFault::None,
    )
    .await;
    let mut client = OraclePeerServiceClient::connect(format!("http://{}", peer.address))
        .await
        .expect("client");
    let file = NamedTempFile::new().expect("parquet temp");
    let sealed = fragment(&file);

    let ticket_context = context(leader);
    let pending = reserve_pending(&mut client, &ticket_context).await;
    let mut ticket_tamper =
        execute_request(&authority, &ticket_context, worker, 61, &pending, &sealed);
    ticket_tamper.ticket.signature[0] ^= 1;
    let rejection = client
        .execute_fragment(Request::new(ticket_tamper.into()))
        .await
        .expect_err("signature tamper rejects");
    assert_eq!(rejection.code(), wyrd_tonic::tonic::Code::PermissionDenied);

    let payload_context = context(leader);
    let pending = reserve_pending(&mut client, &payload_context).await;
    let mut payload_tamper =
        execute_request(&authority, &payload_context, worker, 61, &pending, &sealed);
    payload_tamper.fragment_bytes[0] ^= 1;
    let rejection = client
        .execute_fragment(Request::new(payload_tamper.into()))
        .await
        .expect_err("payload tamper rejects");
    assert_eq!(rejection.code(), wyrd_tonic::tonic::Code::PermissionDenied);

    let mut permission_context = context(leader);
    permission_context.permission_digest.clear();
    let pending = reserve_pending(&mut client, &permission_context).await;
    let permission_tamper = execute_request(
        &authority,
        &permission_context,
        worker,
        61,
        &pending,
        &sealed,
    );
    let rejection = client
        .execute_fragment(Request::new(permission_tamper.into()))
        .await
        .expect_err("empty permission binding rejects");
    assert_eq!(rejection.code(), wyrd_tonic::tonic::Code::PermissionDenied);
    assert_eq!(*audit.calls.lock().expect("audit mutex"), 3);
    peer.stop().await;
}

/// Runs ticket, payload, and permission tamper proofs as a focused peer test.
#[tokio::test]
async fn oracle_peer_tonic_rejects_ticket_payload_and_permission_tamper() {
    prove_oracle_peer_tonic_rejects_ticket_payload_and_permission_tamper().await;
}

/// Leader refuses corrupted and missing footers without admitting partial peer batches.
pub(crate) async fn prove_oracle_peer_rejects_corrupted_and_missing_footer_attempts() {
    for (ordinal, fault) in [PeerFault::CorruptFooter, PeerFault::MissingFooter]
        .into_iter()
        .enumerate()
    {
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
        let fence = 70_u64 + u64::try_from(ordinal).expect("bounded ordinal");
        let peer = start_peer(
            worker,
            fence,
            Arc::clone(&authority),
            Arc::clone(&security_audit),
            fault,
        )
        .await;
        let dispatcher = FragmentDispatcher::new(
            authority.clone(),
            transport_directory(
                leader,
                Arc::clone(&authority),
                Arc::clone(&security_audit),
                TonicOraclePeerTransport::new(
                    HashMap::from([(worker, format!("http://{}", peer.address))]),
                    None,
                )
                .expect("transport"),
            ),
        );
        let file = NamedTempFile::new().expect("parquet temp");
        let error = dispatcher
            .execute(
                &context(leader),
                fragment(&file),
                &[DispatchCandidate {
                    node_id: worker,
                    worker_fence: fence,
                }],
            )
            .await
            .expect_err("invalid footer cannot produce an admitted attempt");
        assert!(
            matches!(error, DispatchError::Terminal | DispatchError::Exhausted),
            "footer failure must be terminal or exhaust the bounded candidate set: {error:?}"
        );
        assert_eq!(peer.reservations.cleanup_expired(chrono::Utc::now()), 0);
        peer.stop().await;
    }
}

/// Runs corrupt and missing footer proofs as a focused peer test.
#[tokio::test]
async fn oracle_peer_rejects_corrupted_and_missing_footer_attempts() {
    prove_oracle_peer_rejects_corrupted_and_missing_footer_attempts().await;
}

/// Production tonic transport force-refreshes once after an unauthenticated stale bearer.
#[tokio::test]
async fn oracle_peer_tonic_stale_bearer_refreshes_exactly_once() {
    wyrd_tls::install_crypto_provider().expect("test installs Wyrd TLS provider");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let calls = Arc::new(AtomicUsize::new(0));
    let shutdown = CancellationToken::new();
    let server_shutdown = shutdown.clone();
    let server_calls = Arc::clone(&calls);
    let server = tokio::spawn(async move {
        let router = wyrd_tonic::tonic::transport::Server::builder()
            .tls_config(
                wyrd_tonic::tonic::transport::ServerTlsConfig::new().identity(
                    wyrd_tonic::tonic::transport::Identity::from_pem(
                        PEER_TLS_CERTIFICATE_PEM,
                        PEER_TLS_PRIVATE_KEY_PEM,
                    ),
                ),
            )
            .expect("peer TLS identity")
            .add_service(OraclePeerServiceServer::new(RefreshProbeService {
                calls: server_calls,
            }));
        wyrd_tonic::server::serve_grpc_with_listener(router, listener, server_shutdown)
            .await
            .expect("server");
    });
    tokio::task::yield_now().await;
    let worker = NodeId::new(uuid::Uuid::now_v7());
    let credentials = Arc::new(RefreshProbeCredentials {
        forced: AtomicUsize::new(0),
    });
    let transport = TonicOraclePeerTransport::with_test_credentials_and_tls(
        HashMap::from([(worker, format!("https://{address}"))]),
        credentials.clone(),
        OraclePeerTls::new(PEER_TLS_CA_PEM.as_bytes().to_vec(), "localhost".to_owned()),
    );
    let response = transport
        .reserve(
            worker,
            ReserveNodeSlotsRequest {
                query_id: QueryId::new(uuid::Uuid::now_v7()),
                leader_node_id: NodeId::new(uuid::Uuid::now_v7()),
                leader_fencing_token: 1,
                query_class: QueryClass::Interactive,
                slot_units: 1,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(1),
            },
        )
        .await
        .expect("refresh retry succeeds");
    assert!(matches!(response, ReserveNodeSlotsResponse::Pending(_)));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(credentials.forced.load(Ordering::SeqCst), 1);
    shutdown.cancel();
    server.await.expect("server joins");
}

/// TLS-enabled peer transport rejects plaintext before requesting a bearer.
#[tokio::test]
async fn oracle_peer_tls_rejects_plaintext_before_bearer() {
    let worker = NodeId::new(uuid::Uuid::now_v7());
    let credentials = Arc::new(BearerCallProbe {
        calls: AtomicUsize::new(0),
    });
    let transport = TonicOraclePeerTransport::with_test_credentials_and_tls(
        HashMap::from([(worker, "http://127.0.0.1:9".to_owned())]),
        credentials.clone(),
        OraclePeerTls::new(
            b"-----BEGIN CERTIFICATE-----\ninvalid\n-----END CERTIFICATE-----\n".to_vec(),
            "oracle.test".to_owned(),
        ),
    );
    let error = transport
        .reserve(
            worker,
            ReserveNodeSlotsRequest {
                query_id: QueryId::new(uuid::Uuid::now_v7()),
                leader_node_id: NodeId::new(uuid::Uuid::now_v7()),
                leader_fencing_token: 1,
                query_class: QueryClass::Interactive,
                slot_units: 1,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(1),
            },
        )
        .await
        .expect_err("plaintext endpoint must fail closed");
    assert!(
        matches!(error, DispatchError::Terminal),
        "plaintext TLS configuration must be terminal, not stale topology: {error:?}"
    );
    assert_eq!(credentials.calls.load(Ordering::SeqCst), 0);
}

/// Untrusted authorities and DNS mismatches fail before bearer acquisition or handler dispatch.
#[tokio::test]
async fn oracle_peer_tls_rejects_untrusted_certificate_and_name_before_bearer() {
    wyrd_tls::install_crypto_provider().expect("test installs Wyrd TLS provider");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let handler_calls = Arc::new(AtomicUsize::new(0));
    let shutdown = CancellationToken::new();
    let server_shutdown = shutdown.clone();
    let server_calls = Arc::clone(&handler_calls);
    let server = tokio::spawn(async move {
        let router = wyrd_tonic::tonic::transport::Server::builder()
            .tls_config(
                wyrd_tonic::tonic::transport::ServerTlsConfig::new().identity(
                    wyrd_tonic::tonic::transport::Identity::from_pem(
                        PEER_TLS_CERTIFICATE_PEM,
                        PEER_TLS_PRIVATE_KEY_PEM,
                    ),
                ),
            )
            .expect("peer TLS identity")
            .add_service(OraclePeerServiceServer::new(RefreshProbeService {
                calls: server_calls,
            }));
        wyrd_tonic::server::serve_grpc_with_listener(router, listener, server_shutdown)
            .await
            .expect("server");
    });
    tokio::task::yield_now().await;

    for (ca, server_name) in [
        (UNTRUSTED_PEER_CA_PEM, "localhost"),
        (PEER_TLS_CA_PEM, "not-localhost.test"),
    ] {
        let worker = NodeId::new(uuid::Uuid::now_v7());
        let credentials = Arc::new(BearerCallProbe {
            calls: AtomicUsize::new(0),
        });
        let transport = TonicOraclePeerTransport::with_test_credentials_and_tls(
            HashMap::from([(worker, format!("https://{address}"))]),
            credentials.clone(),
            OraclePeerTls::new(ca.as_bytes().to_vec(), server_name.to_owned()),
        );
        transport
            .reserve(
                worker,
                ReserveNodeSlotsRequest {
                    query_id: QueryId::new(uuid::Uuid::now_v7()),
                    leader_node_id: NodeId::new(uuid::Uuid::now_v7()),
                    leader_fencing_token: 1,
                    query_class: QueryClass::Interactive,
                    slot_units: 1,
                    expires_at: chrono::Utc::now() + chrono::Duration::seconds(1),
                },
            )
            .await
            .expect_err("TLS authentication must fail closed");
        assert_eq!(credentials.calls.load(Ordering::SeqCst), 0);
        assert_eq!(handler_calls.load(Ordering::SeqCst), 0);
    }

    shutdown.cancel();
    server.await.expect("server joins");
}

/// A trusted TLS peer completes reserve, signed execution, stream validation, and release.
#[tokio::test]
async fn oracle_peer_tls_dispatches_and_releases_over_trusted_channel() {
    let audit = Arc::new(RecordingPeerAudit::default());
    let security_audit: Arc<dyn PeerSecurityAudit> = audit.clone();
    let authority = Arc::new(
        OraclePeerAuthority::from_pem(
            &SecretString::from(PRIVATE_KEY_PEM),
            Arc::clone(&security_audit),
        )
        .expect("peer authority"),
    );
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let worker = NodeId::new(uuid::Uuid::now_v7());
    let fence = 91;
    let peer = start_tls_peer(
        worker,
        fence,
        Arc::clone(&authority),
        Arc::clone(&security_audit),
    )
    .await;
    let transport = TonicOraclePeerTransport::with_test_credentials_and_tls(
        HashMap::from([(worker, format!("https://{}", peer.address))]),
        Arc::new(RefreshProbeCredentials {
            forced: AtomicUsize::new(0),
        }),
        OraclePeerTls::new(PEER_TLS_CA_PEM.as_bytes().to_vec(), "localhost".to_owned()),
    );
    let dispatcher = FragmentDispatcher::new(
        authority.clone(),
        transport_directory(leader, authority, security_audit, transport),
    );
    let file = NamedTempFile::new().expect("parquet temp");
    let result = dispatcher
        .execute(
            &context(leader),
            fragment(&file),
            &[DispatchCandidate {
                node_id: worker,
                worker_fence: fence,
            }],
        )
        .await
        .expect("trusted TLS dispatch succeeds");
    result
        .batches
        .collect::<Result<Vec<_>, _>>()
        .expect("trusted attempt stream validates");
    assert_eq!(peer.reservations.cleanup_expired(chrono::Utc::now()), 0);
    assert_eq!(*audit.calls.lock().expect("audit mutex"), 0);
    peer.stop().await;
}

/// Two real Wyrd servers preserve peer authorization, fencing, execution, and cleanup over TLS.
#[tokio::test]
async fn oracle_peer_two_wyrd_server_tls_journey() {
    let cluster = WyrdTestCluster::start_spec_with_oracle_peer_tls(BifrostClusterSpec::two_mixed())
        .await
        .expect("two-node TLS cluster starts");
    let target = cluster.server(0).expect("target Wyrd server");
    let peer = target
        .state()
        .oracle_peer
        .as_ref()
        .expect("target Oracle peer runtime")
        .clone();
    let lease = peer
        .cluster()
        .snapshot()
        .live_oracles()
        .into_iter()
        .find(|lease| lease.key.node_id == cluster.ready_query_nodes()[0])
        .expect("target live Oracle lease")
        .clone();
    let security_audit: Arc<dyn PeerSecurityAudit> = peer.security_audit();
    let authority = OraclePeerAuthority::from_pem(
        &wyrd_testing::keys::oracle_peer_signing_key_pem(),
        security_audit,
    )
    .expect("matching peer authority");
    let bearer = cluster
        .oracle_peer_bearer(false)
        .await
        .expect("least-privilege peer bearer");
    let mut client = OraclePeerServiceClient::new(
        cluster
            .oracle_peer_channel(0)
            .await
            .expect("CA/DNS-authenticated channel"),
    );
    let request = |fence| ReserveNodeSlotsRequest {
        query_id: QueryId::new(uuid::Uuid::now_v7()),
        leader_node_id: lease.key.node_id,
        leader_fencing_token: fence,
        query_class: QueryClass::Interactive,
        slot_units: 1,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(5),
    };
    let mut stale_bearer = Request::new(ProtoReserveNodeSlotsRequest::from(request(
        lease.fencing_token,
    )));
    attach_peer_bearer(&mut stale_bearer, "stale-peer-bearer");
    assert_eq!(
        client
            .reserve_slots(stale_bearer)
            .await
            .expect_err("stale bearer denied under TLS")
            .code(),
        wyrd_tonic::tonic::Code::Unauthenticated
    );
    let mut stale_fence = Request::new(ProtoReserveNodeSlotsRequest::from(request(
        lease.fencing_token.saturating_add(1),
    )));
    attach_peer_bearer(&mut stale_fence, &bearer);
    assert_eq!(
        client
            .reserve_slots(stale_fence)
            .await
            .expect_err("stale fence denied under TLS")
            .code(),
        wyrd_tonic::tonic::Code::PermissionDenied
    );

    let mut dispatch_context = context(lease.key.node_id);
    dispatch_context.leader_fence = lease.fencing_token;
    dispatch_context.tenant_id = target.data_tenant_id().as_uuid();
    let mut reserve = Request::new(ProtoReserveNodeSlotsRequest::from(
        ReserveNodeSlotsRequest {
            query_id: dispatch_context.query_id,
            leader_node_id: dispatch_context.leader_node_id,
            leader_fencing_token: lease.fencing_token,
            query_class: dispatch_context.query_class,
            slot_units: dispatch_context.slot_units,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(5),
        },
    ));
    attach_peer_bearer(&mut reserve, &bearer);
    let pending = match ReserveNodeSlotsResponse::try_from(
        client
            .reserve_slots(reserve)
            .await
            .expect("TLS reserve")
            .into_inner(),
    )
    .expect("reserve response")
    {
        ReserveNodeSlotsResponse::Pending(pending) => pending,
        other => panic!("expected pending reservation, got {other:?}"),
    };
    let file = NamedTempFile::new().expect("fragment file");
    let mut sealed = fragment(&file);
    let object_path = format!("oracle-peer/{}.parquet", uuid::Uuid::now_v7());
    target
        .state()
        .storage
        .operator()
        .write(
            &object_path,
            std::fs::read(file.path()).expect("fragment bytes"),
        )
        .await
        .expect("fragment uploaded to shared storage");
    let warehouse = match target.state().storage.backend_config() {
        wyrd_storage::BackendConfig::Local { root } => format!("file://{}", root.display()),
        other => panic!("TLS peer journey requires local storage, got {other:?}"),
    };
    sealed.files[0].location = format!("{warehouse}/{object_path}");
    sealed.binding = format!("{warehouse}/oracle-peer");
    sealed.fragment_id = sealed.digest();
    let mut execute = Request::new(ProtoExecuteFragmentRequest::from(execute_request(
        &authority,
        &dispatch_context,
        lease.key.node_id,
        lease.fencing_token,
        &pending,
        &sealed,
    )));
    attach_peer_bearer(&mut execute, &bearer);
    let mut frames = client
        .execute_fragment(execute)
        .await
        .expect("TLS execute")
        .into_inner();
    while frames.message().await.expect("worker frame").is_some() {}
    assert_eq!(peer.worker().pending_reservations(), 0);

    let release_query = QueryId::new(uuid::Uuid::now_v7());
    let mut reserve_for_release = Request::new(ProtoReserveNodeSlotsRequest::from(
        ReserveNodeSlotsRequest {
            query_id: release_query,
            leader_node_id: lease.key.node_id,
            leader_fencing_token: lease.fencing_token,
            query_class: QueryClass::Interactive,
            slot_units: 1,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(5),
        },
    ));
    attach_peer_bearer(&mut reserve_for_release, &bearer);
    let pending = match ReserveNodeSlotsResponse::try_from(
        client
            .reserve_slots(reserve_for_release)
            .await
            .expect("TLS reserve before explicit release")
            .into_inner(),
    )
    .expect("release reservation response")
    {
        ReserveNodeSlotsResponse::Pending(pending) => pending,
        other => panic!("expected releasable reservation, got {other:?}"),
    };
    let mut release = Request::new(ProtoReleaseNodeSlotsRequest::from(
        ReleaseNodeSlotsRequest {
            reservation_id: pending.reservation_id,
            query_id: release_query,
            leader_node_id: lease.key.node_id,
            leader_fencing_token: lease.fencing_token,
        },
    ));
    attach_peer_bearer(&mut release, &bearer);
    client.release_slots(release).await.expect("TLS release");
    assert_eq!(peer.worker().pending_reservations(), 0);
    cluster.shutdown().await.expect("TLS cluster shutdown");
}

/// A leader routes a late worker join and address/fence replacement from live snapshots.
#[tokio::test]
async fn oracle_peer_three_node_live_topology_routes_without_leader_restart() {
    let mut cluster = WyrdTestCluster::start_spec_with_oracle_peer_tls_delayed_last(
        BifrostClusterSpec::three_mixed(),
    )
    .await
    .expect("leader boots before delayed worker");
    let nodes = cluster.ready_query_nodes();
    let leader = nodes[0];
    let worker = *cluster
        .configured_node_ids()
        .last()
        .expect("delayed worker identity");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("delayed worker excluded");
    assert!(
        cluster
            .server(0)
            .expect("leader server")
            .state()
            .oracle_peer
            .as_ref()
            .expect("leader peer")
            .cluster()
            .snapshot()
            .live_oracle(worker)
            .is_none(),
        "unbooted worker must be excluded from the refreshed ready/live cut"
    );
    let transport = cluster
        .oracle_peer_transport(0)
        .expect("production registry-backed transport");
    cluster
        .restart_node_at_new_address(worker)
        .await
        .expect("late worker joins at a new address");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("late join published");
    let leader_peer = cluster
        .server(0)
        .expect("leader server")
        .state()
        .oracle_peer
        .as_ref()
        .expect("leader peer");
    let snapshot = leader_peer.cluster().snapshot();
    let leader_lease = snapshot.live_oracle(leader).expect("leader lease");
    let worker_lease = snapshot
        .live_oracle(worker)
        .expect("late worker lease")
        .clone();
    let query_id = QueryId::new(uuid::Uuid::now_v7());
    let pending = transport
        .reserve(
            worker,
            ReserveNodeSlotsRequest {
                query_id,
                leader_node_id: leader,
                leader_fencing_token: leader_lease.fencing_token,
                query_class: QueryClass::Interactive,
                slot_units: 1,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(5),
            },
        )
        .await
        .expect("late worker reserve reaches its live address");
    let pending = match pending {
        ReserveNodeSlotsResponse::Pending(pending) => pending,
        other => panic!("expected pending late-worker reservation, got {other:?}"),
    };
    transport
        .release(
            worker,
            ReleaseNodeSlotsRequest {
                reservation_id: pending.reservation_id,
                query_id,
                leader_node_id: leader,
                leader_fencing_token: leader_lease.fencing_token,
            },
        )
        .await
        .expect("late worker release uses the same live topology");

    cluster
        .stop_node(worker)
        .await
        .expect("joined worker stops");
    cluster
        .restart_node_at_new_address(worker)
        .await
        .expect("worker restarts on replacement address");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("replacement fence published");
    let replacement = cluster
        .server(2)
        .expect("replacement worker")
        .state()
        .oracle_peer
        .as_ref()
        .expect("replacement peer")
        .clone();
    let replacement_fence = replacement
        .cluster()
        .snapshot()
        .live_oracle(worker)
        .expect("replacement lease")
        .fencing_token;
    assert_ne!(
        replacement_fence, worker_lease.fencing_token,
        "restart must advance the role fence"
    );
    assert_ne!(
        replacement
            .cluster()
            .snapshot()
            .live_oracle(worker)
            .expect("replacement address")
            .address,
        worker_lease.address,
        "restart fixture must replace the advertised endpoint"
    );
    let replacement_query = QueryId::new(uuid::Uuid::now_v7());
    let replacement_pending = transport
        .reserve(
            worker,
            ReserveNodeSlotsRequest {
                query_id: replacement_query,
                leader_node_id: leader,
                leader_fencing_token: leader_lease.fencing_token,
                query_class: QueryClass::Interactive,
                slot_units: 1,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(5),
            },
        )
        .await
        .expect("replacement worker reserve reaches its new address");
    let replacement_pending = match replacement_pending {
        ReserveNodeSlotsResponse::Pending(pending) => pending,
        other => panic!("expected replacement reservation, got {other:?}"),
    };
    transport
        .release(
            worker,
            ReleaseNodeSlotsRequest {
                reservation_id: replacement_pending.reservation_id,
                query_id: replacement_query,
                leader_node_id: leader,
                leader_fencing_token: leader_lease.fencing_token,
            },
        )
        .await
        .expect("replacement worker release reaches its new address");
    assert_eq!(replacement.worker().pending_reservations(), 0);
    let security_audit: Arc<dyn PeerSecurityAudit> = replacement.security_audit();
    let authority = Arc::new(
        OraclePeerAuthority::from_pem(
            &wyrd_testing::keys::oracle_peer_signing_key_pem(),
            Arc::clone(&security_audit),
        )
        .expect("matching peer authority"),
    );
    let ticket_minter: Arc<dyn PeerTicketMinter> = authority.clone();
    let stale_dispatcher = FragmentDispatcher::new(
        ticket_minter,
        transport_directory(
            leader,
            authority,
            security_audit,
            cluster
                .oracle_peer_transport(0)
                .expect("replacement production transport"),
        ),
    );
    let stale_file = NamedTempFile::new().expect("stale fragment file");
    let mut routed_fragment = fragment(&stale_file);
    let object_path = format!("oracle-peer/{}.parquet", uuid::Uuid::now_v7());
    cluster
        .server(0)
        .expect("leader storage")
        .state()
        .storage
        .operator()
        .write(
            &object_path,
            std::fs::read(stale_file.path()).expect("routed fragment bytes"),
        )
        .await
        .expect("routed fragment uploaded");
    let warehouse = match cluster
        .server(0)
        .expect("leader storage")
        .state()
        .storage
        .backend_config()
    {
        wyrd_storage::BackendConfig::Local { root } => format!("file://{}", root.display()),
        other => panic!("topology journey requires local storage, got {other:?}"),
    };
    routed_fragment.files[0].location = format!("{warehouse}/{object_path}");
    routed_fragment.binding = format!("{warehouse}/oracle-peer");
    routed_fragment.fragment_id = routed_fragment.digest();
    let mut routed_context = context(leader);
    routed_context.leader_fence = leader_lease.fencing_token;
    routed_context.tenant_id = cluster.data_tenant_id().as_uuid();
    let stale = stale_dispatcher
        .execute(
            &routed_context,
            routed_fragment.clone(),
            &[DispatchCandidate {
                node_id: worker,
                worker_fence: worker_lease.fencing_token,
            }],
        )
        .await
        .expect_err("old selected fence is rejected by live topology");
    assert!(matches!(stale, DispatchError::StaleObject));
    assert_eq!(
        replacement.worker().pending_reservations(),
        0,
        "stale candidate must not mutate replacement capacity"
    );
    stale_dispatcher
        .execute(
            &routed_context,
            routed_fragment,
            &[DispatchCandidate {
                node_id: worker,
                worker_fence: replacement_fence,
            }],
        )
        .await
        .expect("replacement endpoint completes signed fragment execution");
    assert_eq!(replacement.worker().pending_reservations(), 0);
    cluster.shutdown().await.expect("topology cluster shutdown");
}

/// Production tonic authentication, SQL membership, audit, and capacity gate reserve mutation.
#[tokio::test]
async fn reserve_requires_oracle_peer_authority() {
    let server = WyrdTestEnv::start().await.expect("test environment");
    let mut system_conn = server
        .tenant_conn_for(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system tenant connection");
    wyrd_auth::seed::seed_builtin_roles_for_tenant(&mut system_conn, DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system roles seed");
    system_conn.commit().await.expect("system role commit");

    let audit = Arc::new(
        PostgresPeerSecurityAudit::try_new(&server.state().postgres)
            .await
            .expect("durable peer audit"),
    );
    let audit_started = chrono::Utc::now();
    let security_audit: Arc<dyn PeerSecurityAudit> = audit.clone();
    let authority = Arc::new(
        OraclePeerAuthority::from_pem(
            &SecretString::from(PRIVATE_KEY_PEM),
            Arc::clone(&security_audit),
        )
        .expect("peer authority"),
    );
    let node_id = NodeId::new(uuid::Uuid::now_v7());
    let cluster = Arc::new(ClusterRegistry::new(
        server.state().postgres.vala().clone(),
        node_id,
    ));
    let role = cluster
        .register_oracle(
            "127.0.0.1:0",
            OracleCapabilitiesV1 {
                peer_protocol_version: 1,
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 256 * 1024 * 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 64 * 1024 * 1024,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
                max_workers_per_query: 1,
            },
        )
        .await
        .expect("Oracle role");
    cluster
        .refresh_snapshot()
        .await
        .expect("membership snapshot");
    let reservations = Arc::new(ReservationRegistry::new(
        Arc::new(OracleSlotManager::new(1, 1)),
        1,
    ));
    let verifier: Arc<dyn PeerTicketVerifier> = authority.clone();
    let worker = Arc::new(OraclePeerWorker::new(
        node_id,
        role.fencing_token,
        verifier,
        Arc::clone(&security_audit),
        Arc::clone(&reservations),
        SealedFragmentExecutor::default(),
    ));
    let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("peer listener");
    let address = socket.local_addr().expect("peer address");
    drop(socket);
    let shutdown = CancellationToken::new();
    let server_shutdown = shutdown.clone();
    let peer_service = wyrd_server::oracle::OraclePeerGrpc::new(
        server.state().clone(),
        Arc::clone(&worker),
        Arc::clone(&cluster),
        security_audit,
    );
    let peer_server = tokio::spawn(async move {
        wyrd_tonic::tonic::transport::Server::builder()
            .add_service(peer_service.into_server())
            .serve_with_shutdown(address, server_shutdown.cancelled_owned())
            .await
            .expect("peer server");
    });
    tokio::task::yield_now().await;

    let lease = cluster
        .snapshot()
        .live_oracles()
        .into_iter()
        .next()
        .expect("live Oracle lease")
        .clone();
    let request = |leader_node_id: NodeId, leader_fencing_token: u64| {
        ProtoReserveNodeSlotsRequest::from(ReserveNodeSlotsRequest {
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id,
            leader_fencing_token,
            query_class: QueryClass::Interactive,
            slot_units: 1,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(2),
        })
    };
    let card_ref = |name: &str| CardRef {
        kind: CardKind::Service,
        name: CardName::new(name).expect("card name"),
        version: VersionBlock::parse("1.0.0").expect("version"),
        space: SpaceName::new("system").expect("space"),
        uid: Some(CardUid::from_uuid(uuid::Uuid::now_v7()).expect("card uid")),
    };
    let user = match server
        .bootstrap_user("oracle-peer-user", &["admin"])
        .await
        .expect("user bootstrap")
    {
        Bootstrap::User { jwt, .. } => jwt,
        Bootstrap::Machine { .. } => panic!("user bootstrap returned machine"),
    };
    let ordinary_ref = card_ref("oracle-peer-ordinary");
    let ordinary_id = server
        .seed_service_principal_for_test(server.data_tenant_id(), &ordinary_ref, &["writer"])
        .await
        .expect("ordinary service principal");
    let ordinary = server
        .issue_service_access_token_for_test(
            ordinary_id,
            server.data_tenant_id(),
            ordinary_ref,
            &["writer"],
        )
        .expect("ordinary service token");
    let tenant_admin_ref = card_ref("oracle-peer-tenant-admin");
    let tenant_admin_id = server
        .seed_service_principal_for_test(server.data_tenant_id(), &tenant_admin_ref, &["admin"])
        .await
        .expect("tenant admin principal");
    let tenant_admin = server
        .issue_service_access_token_for_test(
            tenant_admin_id,
            server.data_tenant_id(),
            tenant_admin_ref,
            &["admin"],
        )
        .expect("tenant admin token");
    let system_missing_ref = card_ref("oracle-peer-system-missing");
    let system_missing_id = server
        .seed_service_principal_for_test(
            DataTenantId::SYSTEM_OWNER,
            &system_missing_ref,
            &["writer"],
        )
        .await
        .expect("system missing-permission principal");
    let system_missing = server
        .issue_service_access_token_for_test(
            system_missing_id,
            DataTenantId::SYSTEM_OWNER,
            system_missing_ref,
            &["writer"],
        )
        .expect("system missing-permission token");
    let authorized_ref = card_ref("oracle-peer-authorized");
    let authorized_id = server
        .seed_service_principal_for_test(DataTenantId::SYSTEM_OWNER, &authorized_ref, &["admin"])
        .await
        .expect("authorized principal");
    let authorized = server
        .issue_service_access_token_for_test(
            authorized_id,
            DataTenantId::SYSTEM_OWNER,
            authorized_ref,
            &["admin"],
        )
        .expect("authorized token");

    let mut client = OraclePeerServiceClient::connect(format!("http://{address}"))
        .await
        .expect("peer client");
    for (case, token, expected_code) in [
        ("missing", None, wyrd_tonic::tonic::Code::Unauthenticated),
        (
            "user",
            Some(user.as_str()),
            wyrd_tonic::tonic::Code::PermissionDenied,
        ),
        (
            "ordinary",
            Some(ordinary.as_str()),
            wyrd_tonic::tonic::Code::PermissionDenied,
        ),
        (
            "tenant-admin",
            Some(tenant_admin.as_str()),
            wyrd_tonic::tonic::Code::PermissionDenied,
        ),
        (
            "system-missing",
            Some(system_missing.as_str()),
            wyrd_tonic::tonic::Code::PermissionDenied,
        ),
    ] {
        let mut rpc = Request::new(request(lease.key.node_id, lease.fencing_token));
        if let Some(token) = token {
            rpc.metadata_mut().insert(
                "x-wyrd-access-token",
                format!("Bearer {token}").parse().expect("bearer metadata"),
            );
        }
        let status = client
            .reserve_slots(rpc)
            .await
            .expect_err("authority denial");
        assert_eq!(status.code(), expected_code, "{case}: {status}");
        assert_eq!(worker.pending_reservations(), 0);
    }
    for (node, fence) in [
        (NodeId::new(uuid::Uuid::now_v7()), lease.fencing_token),
        (lease.key.node_id, lease.fencing_token.saturating_add(1)),
    ] {
        let mut rpc = Request::new(request(node, fence));
        rpc.metadata_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {authorized}")
                .parse()
                .expect("bearer metadata"),
        );
        let status = client
            .reserve_slots(rpc)
            .await
            .expect_err("membership denial");
        assert_eq!(status.code(), wyrd_tonic::tonic::Code::PermissionDenied);
        assert_eq!(worker.pending_reservations(), 0);
    }
    let mut dispatch_context = context(node_id);
    dispatch_context.leader_fence = role.fencing_token;
    let mut rpc = Request::new(ProtoReserveNodeSlotsRequest::from(
        ReserveNodeSlotsRequest {
            query_id: dispatch_context.query_id,
            leader_node_id: dispatch_context.leader_node_id,
            leader_fencing_token: dispatch_context.leader_fence,
            query_class: dispatch_context.query_class,
            slot_units: dispatch_context.slot_units,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(2),
        },
    ));
    rpc.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {authorized}")
            .parse()
            .expect("bearer metadata"),
    );
    let accepted = client
        .reserve_slots(rpc)
        .await
        .expect("authorized reserve")
        .into_inner();
    let pending = match ReserveNodeSlotsResponse::try_from(accepted)
        .expect("authorized response conversion")
    {
        ReserveNodeSlotsResponse::Pending(pending) => pending,
        other => panic!("authorized reserve returned {other:?}"),
    };
    assert_eq!(worker.pending_reservations(), 1);
    let mut wrong_release = Request::new(ProtoReleaseNodeSlotsRequest::from(
        ReleaseNodeSlotsRequest {
            reservation_id: pending.reservation_id,
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id: dispatch_context.leader_node_id,
            leader_fencing_token: dispatch_context.leader_fence,
        },
    ));
    wrong_release.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {authorized}")
            .parse()
            .expect("bearer metadata"),
    );
    client
        .release_slots(wrong_release)
        .await
        .expect("tuple-mismatched release is idempotent");
    assert_eq!(
        worker.pending_reservations(),
        1,
        "release with a different query tuple cannot mutate capacity"
    );
    let file = NamedTempFile::new().expect("fragment file");
    let sealed = fragment(&file);
    let execute = execute_request(
        &authority,
        &dispatch_context,
        node_id,
        role.fencing_token,
        &pending,
        &sealed,
    );
    let mut execute_rpc = Request::new(ProtoExecuteFragmentRequest::from(execute));
    execute_rpc.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {authorized}")
            .parse()
            .expect("bearer metadata"),
    );
    let mut stream = client
        .execute_fragment(execute_rpc)
        .await
        .expect("tuple-bound ticket executes")
        .into_inner();
    while stream.message().await.expect("attempt frame").is_some() {}
    assert_eq!(worker.pending_reservations(), 0);

    let replacement_query_id = QueryId::new(uuid::Uuid::now_v7());
    let mut replacement_request = Request::new(ProtoReserveNodeSlotsRequest::from(
        ReserveNodeSlotsRequest {
            query_id: replacement_query_id,
            leader_node_id: dispatch_context.leader_node_id,
            leader_fencing_token: dispatch_context.leader_fence,
            query_class: dispatch_context.query_class,
            slot_units: dispatch_context.slot_units,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(2),
        },
    ));
    replacement_request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {authorized}")
            .parse()
            .expect("bearer metadata"),
    );
    let replacement = client
        .reserve_slots(replacement_request)
        .await
        .expect("replacement reserve")
        .into_inner();
    let replacement = match ReserveNodeSlotsResponse::try_from(replacement)
        .expect("replacement response conversion")
    {
        ReserveNodeSlotsResponse::Pending(pending) => pending,
        other => panic!("replacement reserve returned {other:?}"),
    };
    let mut release = Request::new(ProtoReleaseNodeSlotsRequest::from(
        ReleaseNodeSlotsRequest {
            reservation_id: replacement.reservation_id,
            query_id: replacement_query_id,
            leader_node_id: dispatch_context.leader_node_id,
            leader_fencing_token: dispatch_context.leader_fence,
        },
    ));
    release.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {authorized}")
            .parse()
            .expect("bearer metadata"),
    );
    client
        .release_slots(release)
        .await
        .expect("tuple-matched release");
    assert_eq!(worker.pending_reservations(), 0);

    let mut stale_conn = server
        .state()
        .postgres
        .vala()
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system tenant connection");
    sqlx::query(
        "UPDATE vala.cluster_nodes SET heartbeat_at=$1 \
         WHERE data_tenant_id=$2 AND node_id=$3 AND role='oracle'",
    )
    .bind(chrono::Utc::now() - chrono::Duration::seconds(30))
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .bind(lease.key.node_id.as_uuid())
    .execute(&mut **stale_conn.transaction())
    .await
    .expect("stale heartbeat update");
    stale_conn.commit().await.expect("stale heartbeat commit");
    let mut stale_heartbeat = Request::new(request(lease.key.node_id, lease.fencing_token));
    stale_heartbeat.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {authorized}")
            .parse()
            .expect("bearer metadata"),
    );
    let stale = client
        .reserve_slots(stale_heartbeat)
        .await
        .expect_err("stale heartbeat membership denial");
    assert_eq!(stale.code(), wyrd_tonic::tonic::Code::PermissionDenied);
    assert_eq!(worker.pending_reservations(), 0);

    server.state().postgres.vala_pool().close().await;
    let mut outage_rpc = Request::new(request(lease.key.node_id, lease.fencing_token));
    outage_rpc.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {authorized}")
            .parse()
            .expect("bearer metadata"),
    );
    let outage = client
        .reserve_slots(outage_rpc)
        .await
        .expect_err("membership SQL outage fails closed");
    assert_eq!(outage.code(), wyrd_tonic::tonic::Code::PermissionDenied);
    assert_eq!(worker.pending_reservations(), 0);
    let mut audit_conn = server
        .state()
        .postgres
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system audit connection");
    let details: Vec<(String, String)> = sqlx::query_as(
        "SELECT payload_summary, detail::text FROM vala.audit_outbox \
         WHERE operation='bifrost.query.security_violation' AND created_at >= $1 \
         ORDER BY created_at",
    )
    .bind(audit_started)
    .fetch_all(&mut **audit_conn.transaction())
    .await
    .expect("durable peer audit rows");
    audit_conn.commit().await.expect("audit rows commit");
    assert_eq!(details.len(), 9);
    assert!(
        details.iter().all(|(summary, detail)| {
            summary.contains("scrubbed Oracle peer security rejection")
                && detail.contains("\"phase\":\"peer\"")
                && detail.contains("\"query_digest\":null")
                && !summary.contains("oracle-peer-authorized")
                && !detail.contains("oracle-peer-authorized")
        }),
        "unexpected peer audit details: {details:?}"
    );
    shutdown.cancel();
    peer_server.await.expect("peer server joins");
}

/// Execute the complete real-tonic peer security and cleanup journey.
pub async fn prove_oracle_peer_security_journey() {
    prove_oracle_peer_remote_failure_falls_back_to_leader_local().await;
    prove_oracle_peer_three_failures_exhaust_and_release_partial_placement().await;
    prove_oracle_peer_restart_rejects_old_fence_and_releases_reservation().await;
    prove_oracle_peer_tonic_cancellation_releases_running_slot().await;
    prove_oracle_peer_tonic_rejects_ticket_payload_and_permission_tamper().await;
    prove_oracle_peer_rejects_corrupted_and_missing_footer_attempts().await;
}
