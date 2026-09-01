//! Child-process side of the multi-process Bifrost peer network.
//!
//! Each simulated pod runs this code as its own `main`. It composes one real
//! `wyrd-server` from the shared resources its parent published, waits until
//! both listeners are bound and its selected role membership is ready at its
//! exact advertised address, and only then announces `Ready`. After that it
//! serves the private control protocol on stdin/stdout until it is told to
//! shut down.
//!
//! Everything a real replica owns privately is owned privately here: the
//! composition, the runtime, the listeners, the configuration, the WAL, the
//! spill root, and the certificate. Only the database, the object store, the
//! peer CA, and the peer Service principal come from the parent.

use std::io::{BufRead as _, Write as _};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use secrecy::SecretString;
use sha2::{Digest as _, Sha256};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_server::config::BifrostTarget;

use super::{
    ControlRequest, ControlResponse, MembershipEntry, NodeReport, ProcessClusterError,
    ProcessNodeTarget, env,
};
use crate::server::{TestBifrostPeerTls, WyrdTestServer};

/// How long a child waits for its own readiness before reporting failure.
const READY_DEADLINE: Duration = Duration::from_secs(60);

/// Interval between readiness observations.
const READY_POLL: Duration = Duration::from_millis(100);

/// Runs one simulated pod until its parent shuts it down.
///
/// Returns a failure exit code only when the child could not report anything
/// at all; every other failure is reported to the parent as a structured
/// [`ControlResponse::Failed`] so a journey sees a cause rather than a dead
/// pipe.
#[must_use]
pub fn run_peer_test_node() -> ExitCode {
    let runtime = wyrd_runtime::runtime();
    match runtime.block_on(serve()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The parent may already be gone; stderr is drained either way.
            let _ = emit(&ControlResponse::Failed {
                detail: error.to_string(),
            });
            eprintln!("bifrost peer test node failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Composes the server, announces readiness, and serves the control protocol.
///
/// # Errors
///
/// Returns [`ProcessClusterError`] when the environment is incomplete, shared
/// resources cannot be attached, the server cannot be composed or bound, or
/// readiness is not reached within [`READY_DEADLINE`].
async fn serve() -> Result<(), ProcessClusterError> {
    let config = ChildConfig::from_env()?;
    let fingerprint = config.certificate_fingerprint()?;
    let (server, credentials) = config.start().await?;
    let report = await_ready(&server, &config, fingerprint).await?;
    emit(&ControlResponse::Ready(report.clone()))?;

    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        let read = stdin
            .lock()
            .read_line(&mut line)
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
        if read == 0 {
            break;
        }
        let request = match serde_json::from_str::<ControlRequest>(line.trim()) {
            Ok(request) => request,
            Err(error) => {
                emit(&ControlResponse::Failed {
                    detail: format!("unparseable control request: {error}"),
                })?;
                continue;
            }
        };
        match request {
            ControlRequest::Inspect => {
                let report = describe(
                    &server,
                    &config,
                    report.peer_certificate_fingerprint.clone(),
                )
                .await;
                emit(&ControlResponse::Inspection(report))?;
            }
            ControlRequest::RegisterTable { table } => {
                match config.register_table(&server, &table).await {
                    Ok(()) => emit(&ControlResponse::Registered)?,
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
            }
            ControlRequest::DialPeer { address } => {
                match config.dial_peer(&address, credentials.as_ref()).await {
                    Ok(outcome) => emit(&ControlResponse::Dialed { outcome })?,
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
            }
            ControlRequest::ExecuteInactiveSql { .. } => {
                // The inactive Analytical execution seam is restored by the
                // Analytical slices of this remediation. Until then the control
                // verb exists and refuses explicitly, rather than silently
                // succeeding against a path that is not wired.
                emit(&ControlResponse::Failed {
                    detail: "inactive Analytical execution is not yet mounted on this node"
                        .to_owned(),
                })?;
            }
            ControlRequest::Shutdown => {
                emit(&ControlResponse::ShuttingDown)?;
                break;
            }
        }
    }
    server
        .shutdown()
        .await
        .map_err(|error| ProcessClusterError::Child(error.to_string()))
}

/// Writes one newline-delimited control response to stdout.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Protocol`] when the response cannot be
/// serialized and [`ProcessClusterError::Child`] when stdout cannot be written.
fn emit(response: &ControlResponse) -> Result<(), ProcessClusterError> {
    let mut line = serde_json::to_vec(response)
        .map_err(|error| ProcessClusterError::Protocol(error.to_string()))?;
    line.push(b'\n');
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    handle
        .write_all(&line)
        .and_then(|()| handle.flush())
        .map_err(|error| ProcessClusterError::Child(error.to_string()))
}

/// Everything one child reads from its environment.
///
/// Secrets arrive as filesystem paths under this child's own private root, so
/// no key material or API key is ever an argument, an environment value, or a
/// control message.
struct ChildConfig {
    /// Fixture database this child attaches to.
    database: String,
    /// Seeded data tenant identity.
    tenant_id: wyrd_spec::DataTenantId,
    /// Seeded data tenant slug.
    tenant_slug: String,
    /// Shared local object-store root.
    storage_root: PathBuf,
    /// Bifrost target this child serves.
    target: ProcessNodeTarget,
    /// Child-private durable Scribe root.
    wal_root: PathBuf,
    /// Child-private durable spill root.
    spill_root: PathBuf,
    /// Public HTTP socket.
    http_bind: SocketAddr,
    /// Public gRPC socket.
    grpc_bind: SocketAddr,
    /// Private peer socket.
    peer_bind: SocketAddr,
    /// This child's own peer identity material.
    peer_tls: TestBifrostPeerTls,
    /// Path to the shared peer Service API key.
    peer_api_key_path: PathBuf,
}

impl ChildConfig {
    /// Reads and validates the complete child environment.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when a required variable is
    /// missing or cannot be parsed.
    fn from_env() -> Result<Self, ProcessClusterError> {
        let read = |name: &str| -> Result<String, ProcessClusterError> {
            std::env::var(name).map_err(|_| {
                ProcessClusterError::Resource(format!("child environment is missing {name}"))
            })
        };
        let socket = |name: &str| -> Result<SocketAddr, ProcessClusterError> {
            read(name)?.parse().map_err(|error| {
                ProcessClusterError::Resource(format!("{name} is not a socket address: {error}"))
            })
        };
        let tenant_uuid: uuid::Uuid = read(env::TENANT_ID)?.parse().map_err(|error| {
            ProcessClusterError::Resource(format!("tenant id is not a UUID: {error}"))
        })?;
        Ok(Self {
            database: read(env::DATABASE)?,
            tenant_id: wyrd_spec::DataTenantId::new(tenant_uuid).map_err(|error| {
                ProcessClusterError::Resource(format!("tenant id is not a tenant key: {error}"))
            })?,
            tenant_slug: read(env::TENANT_SLUG)?,
            storage_root: PathBuf::from(read(env::STORAGE_ROOT)?),
            target: ProcessNodeTarget::parse(&read(env::TARGET)?)?,
            wal_root: PathBuf::from(read(env::WAL_ROOT)?),
            spill_root: PathBuf::from(read(env::SPILL_ROOT)?),
            http_bind: socket(env::HTTP_BIND)?,
            grpc_bind: socket(env::GRPC_BIND)?,
            peer_bind: socket(env::PEER_BIND)?,
            peer_tls: TestBifrostPeerTls {
                certificate_path: PathBuf::from(read(env::PEER_CERT_PATH)?),
                private_key_path: PathBuf::from(read(env::PEER_KEY_PATH)?),
                ca_path: PathBuf::from(read(env::PEER_CA_PATH)?),
                server_name: read(env::PEER_SERVER_NAME)?,
            },
            peer_api_key_path: PathBuf::from(read(env::PEER_API_KEY_PATH)?),
        })
    }

    /// Returns the SHA-256 digest of this child's own peer certificate file.
    ///
    /// A certificate is public material, so its digest is safe to report and is
    /// what a journey uses to prove which exact leaf a peer accepted.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the certificate cannot be
    /// read.
    fn certificate_fingerprint(&self) -> Result<String, ProcessClusterError> {
        let bytes = std::fs::read(&self.peer_tls.certificate_path)
            .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        Ok(hex::encode(Sha256::digest(&bytes)))
    }

    /// Maps this child's target onto the server's own target enum.
    fn server_target(&self) -> BifrostTarget {
        match self.target {
            ProcessNodeTarget::All => BifrostTarget::All,
            ProcessNodeTarget::Server => BifrostTarget::Server,
            ProcessNodeTarget::Oracle => BifrostTarget::Oracle,
            ProcessNodeTarget::Scribe => BifrostTarget::Scribe,
            ProcessNodeTarget::ForgeWorker => BifrostTarget::ForgeWorker,
        }
    }

    /// Composes and binds this child's server over the shared resources.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the fixture, storage, or
    /// peer credentials cannot be attached, and [`ProcessClusterError::Child`]
    /// when the server cannot be composed or bound.
    async fn start(
        &self,
    ) -> Result<
        (
            WyrdTestServer,
            Arc<dyn vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials>,
        ),
        ProcessClusterError,
    > {
        let fixture = Arc::new(
            PgFixture::attach(
                self.database.clone(),
                self.tenant_id,
                self.tenant_slug.clone(),
            )
            .await
            .map_err(|error| ProcessClusterError::Resource(error.to_string()))?,
        );
        let api_key = SecretString::from(
            std::fs::read_to_string(&self.peer_api_key_path)
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?
                .trim()
                .to_owned(),
        );
        let credentials =
            crate::server::oracle_peer_credentials_from_key(Arc::clone(&fixture), api_key)
                .await
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        let storage = wyrd_storage::StorageHandle::from_settings(wyrd_storage::StorageSettings {
            backend: wyrd_storage::BackendConfig::Local {
                root: self.storage_root.clone(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        let server = WyrdTestServer::builder()
            .with_bifrost_target_for_test(self.server_target())
            .with_forge_process_role_for_test(self.server_target())
            .with_peer_tls(self.peer_tls.clone())
            .with_peer_bind(self.peer_bind)
            .with_bind_addrs_for_test(self.http_bind, self.grpc_bind)
            .with_durable_bifrost_roots(self.wal_root.clone(), self.spill_root.clone())
            .with_oracle_peer_credentials(Arc::clone(&credentials))
            .with_storage_handle(Arc::clone(&storage))
            .start_with_resources(fixture, Arc::clone(&storage), None)
            .await
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?
            .bind()
            .await
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
        Ok((server, credentials))
    }

    /// Registers one Bifrost table through this child's own catalog.
    ///
    /// The table is created by a real pod against the shared catalog, so a
    /// journey can then query it through any pod's public listener without the
    /// parent holding a server of its own.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when this target composes no
    /// catalog or the catalog refuses the table.
    async fn register_table(
        &self,
        server: &WyrdTestServer,
        table: &str,
    ) -> Result<(), ProcessClusterError> {
        let catalog = server.state().bifrost_catalog().ok_or_else(|| {
            ProcessClusterError::Child("this target composes no Bifrost catalog".to_owned())
        })?;
        catalog
            .create_table(vala_bifrost_redux::catalog::CreateTableRequest {
                table: vala_bifrost_redux::catalog::TableRef::new(
                    vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
                    table,
                ),
                user_fields: vec![
                    arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
                    arrow::datatypes::Field::new(
                        "filter_key",
                        arrow::datatypes::DataType::Utf8,
                        false,
                    ),
                ],
                tenant: self.tenant_id,
                physical_layout: None,
                audit: None,
            })
            .await
            .map(|_| ())
            .map_err(|error| ProcessClusterError::Child(error.to_string()))
    }

    /// Dials another pod's peer socket as this pod, returning the gRPC outcome.
    ///
    /// Uses this child's own certificate and its peer Service bearer, so the
    /// result answers whether the destination admits this process at the
    /// transport and authorizes it at the application layer. A refused
    /// operation is still a successful dial and is reported as its status code.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when the endpoint cannot be
    /// built, the handshake fails, or the bearer cannot be obtained.
    async fn dial_peer(
        &self,
        address: &str,
        credentials: &dyn vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials,
    ) -> Result<String, ProcessClusterError> {
        let child = |error: String| ProcessClusterError::Child(error);
        let read = |path: &std::path::Path| -> Result<Vec<u8>, ProcessClusterError> {
            std::fs::read(path).map_err(|error| ProcessClusterError::Resource(error.to_string()))
        };
        let endpoint = wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
            address.to_owned(),
            &read(&self.peer_tls.ca_path)?,
            self.peer_tls.server_name.clone(),
            &read(&self.peer_tls.certificate_path)?,
            &read(&self.peer_tls.private_key_path)?,
        )
        .map_err(|error| child(error.to_string()))?;
        let channel = endpoint
            .connect()
            .await
            .map_err(|error| child(error.to_string()))?;
        let bearer = credentials
            .bearer(false)
            .await
            .map_err(|error| child(error.to_string()))?;
        let metadata =
            wyrd_tonic::tonic::metadata::MetadataValue::try_from(format!("Bearer {bearer}"))
                .map_err(|error| child(error.to_string()))?;
        let mut client =
            wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient::with_interceptor(
                channel,
                move |mut request: wyrd_tonic::tonic::Request<()>| {
                    request
                        .metadata_mut()
                        .insert("authorization", metadata.clone());
                    Ok(request)
                },
            );
        Ok(
            match client
                .reserve_slots(wyrd_tonic::wyrd::v1::ReserveNodeSlotsRequest::default())
                .await
            {
                Ok(_) => "ok".to_owned(),
                Err(status) => status.code().to_string(),
            },
        )
    }
}

/// Polls this child until every readiness probe passes.
///
/// Readiness includes the private peer listener whenever this target composes
/// one, so a peer-bearing child that bound only its public sockets never
/// announces `Ready`. A target that owns no peer plane is not held back by it.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Timeout`] when readiness is not reached
/// within [`READY_DEADLINE`].
async fn await_ready(
    server: &WyrdTestServer,
    config: &ChildConfig,
    fingerprint: String,
) -> Result<NodeReport, ProcessClusterError> {
    let deadline = Instant::now() + READY_DEADLINE;
    loop {
        let report = describe(server, config, fingerprint.clone()).await;
        if report.ready {
            return Ok(report);
        }
        if Instant::now() >= deadline {
            return Err(ProcessClusterError::Timeout(format!(
                "child pid {} readiness",
                std::process::id()
            )));
        }
        tokio::time::sleep(READY_POLL).await;
    }
}

/// Builds this child's current self-description.
///
/// The advertised address is read back from the composed runtime rather than
/// recomputed from configuration, so a journey observes what this node actually
/// published. Membership is refreshed from Postgres first, because the point of
/// the report is what this pod can currently see of its peers, not what its
/// background refresh happened to cache.
async fn describe(
    server: &WyrdTestServer,
    config: &ChildConfig,
    fingerprint: String,
) -> NodeReport {
    let state = server.state();
    let snapshot = state.readiness.load();
    let membership = match state.bifrost_cluster_for_test() {
        Some(cluster) => {
            let _ = cluster.refresh_snapshot().await;
            MembershipEntry::project(&cluster.snapshot())
        }
        None => Vec::new(),
    };
    let node_id = server.node_id().as_uuid();
    // The advertised address is whatever this node actually published into
    // membership, so a node that registered the wrong endpoint reports it.
    let advertise_addr = membership
        .iter()
        .find(|entry| entry.node_id == node_id)
        .map(|entry| entry.address.clone())
        .unwrap_or_default();
    NodeReport {
        pid: std::process::id(),
        target: config.target,
        node_id,
        http_addr: config.http_bind.to_string(),
        grpc_addr: config.grpc_bind.to_string(),
        peer_addr: config.peer_bind.to_string(),
        advertise_addr,
        peer_certificate_fingerprint: fingerprint,
        ready: snapshot.all_ok() && state.peer_plane.is_satisfied(),
        wal_root: config.wal_root.display().to_string(),
        membership,
    }
}
