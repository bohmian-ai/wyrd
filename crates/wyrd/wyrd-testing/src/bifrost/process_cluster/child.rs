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
    ControlRequest, ControlResponse, MembershipEntry, NodeReport, PeerProbeCredential,
    PeerProbeFraming, PeerProbePlan, PeerProbeTransport, ProcessClusterError, ProcessNodeTarget,
    env,
};
use crate::bifrost::peer_keyring::TestPeerKeyringPaths;
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
    install_child_tracing();
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
    let (server, credentials, fixture) = config.start().await?;
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
            ControlRequest::IngestRows {
                table,
                rows,
                groups,
            } => match config.ingest_rows(&server, &table, rows, groups).await {
                Ok(()) => emit(&ControlResponse::Ingested)?,
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::RefreshSnapshot => match refresh_snapshot(&server).await {
                Ok(()) => emit(&ControlResponse::Refreshed)?,
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::GraphLeases => {
                let (activated, live) = graph_lease_counts(&server);
                emit(&ControlResponse::GraphLeases { activated, live })?;
            }
            ControlRequest::PeerProbe(plan) => {
                match config
                    .peer_probe(&plan, credentials.as_ref(), &fixture)
                    .await
                {
                    Ok(outcome) => emit(&ControlResponse::Probed { outcome })?,
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
            }
            ControlRequest::PeerBodyPolls => {
                emit(&ControlResponse::BodyPolls {
                    count: wyrd_server::grpc::peer_body_polls(),
                })?;
            }
            ControlRequest::ExecuteInactiveSql { sql } => {
                match config.execute_inactive_sql(&server, &sql).await {
                    Ok(rows) => emit(&ControlResponse::Executed { rows })?,
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
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

/// Installs this child's log subscriber on stderr when `RUST_LOG` asks for one.
///
/// Stderr, never stdout: stdout carries the control protocol, and a log line
/// written there would be read by the parent as a malformed response. The
/// subscriber is installed only when `RUST_LOG` is set, so a lane run stays
/// silent and a diagnosing run gets the child's own view of a multi-process
/// failure, which the parent otherwise cannot see at all.
fn install_child_tracing() {
    let Ok(filter) = std::env::var("RUST_LOG") else {
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_writer(std::io::stderr)
        .finish();
    // A child installs exactly one subscriber; a failure here means something
    // already owns the global, which is not worth failing a journey over.
    let _ = tracing::subscriber::set_global_default(subscriber);
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
    /// Paths of the shared peer ticket keyring published to this child.
    peer_keyring: TestPeerKeyringPaths,
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
            peer_keyring: TestPeerKeyringPaths {
                active_key_id: read(env::PEER_TICKET_KEY_ID)?,
                signing_key_path: PathBuf::from(read(env::PEER_TICKET_KEY_PATH)?),
                verifying_keyring_path: PathBuf::from(read(env::PEER_TICKET_KEYRING_PATH)?),
            },
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

    /// Reports whether this child has finished coming up.
    ///
    /// A serving target is ready when every readiness probe passes and its peer
    /// plane obligation is met. A dedicated Forge worker composes no listener
    /// and runs no readiness loop at all, so its own composition returning is
    /// the whole of its startup; holding it to the serving probes would wait
    /// forever on a loop that was never spawned.
    fn is_ready(
        &self,
        snapshot: &wyrd_server::components::health::ReadinessSnapshot,
        state: &wyrd_server::state::AppState,
    ) -> bool {
        if self.target == ProcessNodeTarget::ForgeWorker {
            return true;
        }
        snapshot.all_ok() && state.peer_plane.is_satisfied()
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
            Arc<PgFixture>,
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
            .with_peer_keyring_paths(self.peer_keyring.clone())
            .with_peer_bind(self.peer_bind)
            .with_bind_addrs_for_test(self.http_bind, self.grpc_bind)
            .with_durable_bifrost_roots(self.wal_root.clone(), self.spill_root.clone())
            .with_oracle_peer_credentials(Arc::clone(&credentials))
            .with_storage_handle(Arc::clone(&storage))
            .start_with_resources(Arc::clone(&fixture), Arc::clone(&storage), None)
            .await
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?
            .bind()
            .await
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
        Ok((server, credentials, fixture))
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

    /// Writes and publishes deterministic fixture rows through this Scribe.
    ///
    /// The batch is admitted through the same logical ingress seam the public
    /// write surface uses and then frozen and published, so the rows a later
    /// query reads are files this pod's own Scribe encoded.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when this target composes no
    /// Scribe or catalog, the table is unregistered, or ingest, freeze, or
    /// publication fails.
    async fn ingest_rows(
        &self,
        server: &WyrdTestServer,
        table: &str,
        rows: i64,
        groups: i64,
    ) -> Result<(), ProcessClusterError> {
        let child = ProcessClusterError::Child;
        let scribe = server
            .bifrost_scribe()
            .ok_or_else(|| child("this target composes no Scribe".to_owned()))?;
        let catalog = server
            .state()
            .bifrost_catalog()
            .ok_or_else(|| child("this target composes no Bifrost catalog".to_owned()))?;
        let table_ref = vala_bifrost_redux::catalog::TableRef::new(
            vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
            table,
        );
        let (fingerprint, _) = catalog
            .table_registration(&table_ref, self.tenant_id)
            .await
            .map_err(|error| child(error.to_string()))?;
        let principal = wyrd_runtime::principal::Principal {
            id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            kind: wyrd_runtime::principal::PrincipalKind::User,
            tenant_id: self.tenant_id,
            roles: Vec::new(),
            effective_permissions: wyrd_runtime::PermissionSet::new(),
        };
        let audit_event = wyrd_spec::vala::api::AuditEvent::new(
            wyrd_spec::request_id::RequestId::now_v7(),
            None,
            "bifrost.write".to_owned(),
            format!("bifrost://vala.bifrost/{table}"),
            None,
            principal.id,
            wyrd_spec::auth::PrincipalKindTag::User,
            wyrd_spec::vala::api::AuthMethod::Internal,
            "bifrost_write:write".to_owned(),
            wyrd_spec::vala::api::AuditDecision::Allow,
            wyrd_spec::vala::api::AuditResult::Success,
            "peer network fixture ingest".to_owned(),
        );
        scribe
            .ingest_native_for_test(vala_bifrost_redux::scribe::NativeIngressTestFrame {
                principal,
                table: table_ref,
                expected_schema_fingerprint: fingerprint,
                request_id: wyrd_spec::request_id::RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                audit_event,
                payload: fixture_rows_ipc(rows, groups)?,
            })
            .await
            .map_err(|error| child(error.to_string()))?;
        server
            .flush_bifrost()
            .await
            .map_err(|error| child(error.to_string()))?;
        Ok(())
    }

    /// Runs one statement through this node's inactive Analytical path.
    ///
    /// The stream is drained to its terminal frame rather than dropped early,
    /// so the graph and attempt guards it carries settle before the parent
    /// inspects the node. Nothing in routing reaches this seam; the child
    /// authenticates the same way the public query service does and hands
    /// Oracle the identical context its own gRPC surface would have built.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when this target composes no
    /// Oracle, the context cannot be authorized, or the attempt fails at
    /// admission, planning, execution, or decode.
    async fn execute_inactive_sql(
        &self,
        server: &WyrdTestServer,
        sql: &str,
    ) -> Result<usize, ProcessClusterError> {
        let child = |detail: String| ProcessClusterError::Child(detail);
        let engine = Arc::clone(
            server
                .state()
                .bifrost_query()
                .ok_or_else(|| child("this target composes no Oracle".to_owned()))?
                .engine(),
        );
        let permission = wyrd_runtime::Permission::bifrost_query_read();
        let principal = wyrd_runtime::Principal::new(
            wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            wyrd_runtime::PrincipalKind::User,
            self.tenant_id,
            Vec::new(),
            wyrd_runtime::permission::PermissionSet::from_iter([permission.clone()]),
        );
        let context = vala_bifrost_redux::oracle::AuthorizedQueryContext::try_new(
            principal,
            self.tenant_id,
            wyrd_spec::request_id::RequestId::now_v7(),
            None,
            wyrd_spec::vala::api::AuthMethod::Internal,
            permission.to_string(),
        )
        .map_err(|error| child(error.to_string()))?;
        // Both query identities are allocated independently on purpose: a
        // leaked public identity into the distributed graph, or the reverse,
        // is exactly what the stage authority's isolation exists to refuse.
        let attempt = vala_bifrost_redux::oracle::analytical::AnalyticalAttemptContext {
            public_query_id: vala_bifrost_redux::oracle::analytical::PublicQueryId::from_uuid(
                uuid::Uuid::now_v7(),
            ),
            datafusion_query_id:
                vala_bifrost_redux::oracle::analytical::DataFusionQueryId::from_uuid(
                    uuid::Uuid::now_v7(),
                ),
            snapshot_digest: format!("snapshot-{}", uuid::Uuid::now_v7().simple()),
            permission_digest: format!("permission-{}", uuid::Uuid::now_v7().simple()),
        };
        let mut stream = engine
            .query_sql_inactive_analytical(
                context,
                wyrd_spec::vala::api::BifrostQueryRequest {
                    sql: sql.to_owned(),
                    visibility: wyrd_spec::vala::api::VisibilityMode::PublishedOnly,
                    freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                    deadline_ms: Some(30_000),
                },
                attempt,
            )
            .await
            .map_err(|error| child(error.to_string()))?;
        let mut decoder = vala_bifrost_redux::oracle::QueryIpcDecoder::new();
        let mut rows = 0;
        let mut terminal = None;
        while let Some(frame) = futures_util::StreamExt::next(&mut stream.frames).await {
            match frame.map_err(|error| child(error.to_string()))? {
                wyrd_spec::vala::api::QueryStreamFrame::Schema(schema) => {
                    decoder
                        .accept_schema(&schema.arrow_ipc_schema)
                        .map_err(|error| child(error.to_string()))?;
                }
                wyrd_spec::vala::api::QueryStreamFrame::Batch(batch) => {
                    rows += decoder
                        .accept_batch(&batch.arrow_ipc_batch)
                        .map_err(|error| child(error.to_string()))?
                        .num_rows();
                }
                wyrd_spec::vala::api::QueryStreamFrame::Terminal(frame) => terminal = Some(frame),
            }
        }
        let terminal = terminal
            .ok_or_else(|| child("the inactive attempt emitted no terminal frame".to_owned()))?;
        let _ = terminal;
        Ok(rows)
    }

    /// Performs one shaped private-plane probe and returns its gRPC outcome.
    ///
    /// The probe is issued as a raw HTTP/2 request over this child's own
    /// mutually authenticated channel rather than through a generated client,
    /// because the claim under test is about the bytes on the wire: which
    /// adapter is addressed, which workload credential accompanies it, and how
    /// the first gRPC frame is split or coalesced. A refusal is an outcome, not
    /// an error; only failing to reach the destination is an error.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when the endpoint cannot be
    /// built, the handshake fails, the credential cannot be obtained, or the
    /// destination never answered.
    async fn peer_probe(
        &self,
        plan: &PeerProbePlan,
        own: &dyn vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials,
        fixture: &Arc<PgFixture>,
    ) -> Result<String, ProcessClusterError> {
        let child = |error: String| ProcessClusterError::Child(error);
        let read = |path: &std::path::Path| -> Result<Vec<u8>, ProcessClusterError> {
            std::fs::read(path).map_err(|error| ProcessClusterError::Resource(error.to_string()))
        };
        let endpoint = match plan.transport {
            PeerProbeTransport::Mutual => {
                wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
                    plan.address.clone(),
                    &read(&self.peer_tls.ca_path)?,
                    self.peer_tls.server_name.clone(),
                    &read(&self.peer_tls.certificate_path)?,
                    &read(&self.peer_tls.private_key_path)?,
                )
                .map_err(|error| child(error.to_string()))?
            }
            // Deliberately built from the raw address with no trust material
            // at all: the destination must refuse the connection itself, so
            // the probe never gets far enough to present a credential.
            PeerProbeTransport::Plaintext => wyrd_tonic::tonic::transport::Endpoint::from_shared(
                plan.address.replace("https://", "http://"),
            )
            .map_err(|error| child(error.to_string()))?,
        };
        let mut channel = endpoint
            .connect()
            .await
            .map_err(|error| child(error.to_string()))?;
        let bearer = self.probe_bearer(&plan.credential, own, fixture).await?;
        let mut request = http::Request::builder()
            .method(http::Method::POST)
            .uri(plan.service.path())
            .header(http::header::CONTENT_TYPE, "application/grpc")
            .header("te", "trailers");
        if let Some(bearer) = bearer {
            request = request.header("x-wyrd-access-token", bearer);
        }
        let request = request
            .body(probe_body(plan.framing, plan.payload.as_deref()))
            .map_err(|error| child(error.to_string()))?;
        let response = tower::ServiceExt::oneshot(&mut channel, request)
            .await
            .map_err(|error| child(error.to_string()))?;
        Ok(probe_outcome(response).await)
    }

    /// Resolves the bearer value a probe presents, if it presents one.
    ///
    /// An API-key credential is exchanged through the same middleware a real
    /// peer uses, so a probe for a deliberately wrong principal is refused by
    /// authorization rather than by a malformed token.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when the exchange fails.
    async fn probe_bearer(
        &self,
        credential: &PeerProbeCredential,
        own: &dyn vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials,
        fixture: &Arc<PgFixture>,
    ) -> Result<Option<String>, ProcessClusterError> {
        let child = |error: String| ProcessClusterError::Child(error);
        match credential {
            PeerProbeCredential::Absent => Ok(None),
            PeerProbeCredential::Invalid => Ok(Some("Bearer not-a-real-token".to_owned())),
            PeerProbeCredential::Own => own
                .bearer(false)
                .await
                .map(|bearer| Some(format!("Bearer {bearer}")))
                .map_err(|error| child(error.to_string())),
            PeerProbeCredential::ApiKey(key) => {
                let credentials = crate::server::oracle_peer_credentials_from_key(
                    Arc::clone(fixture),
                    SecretString::from(key.clone()),
                )
                .await
                .map_err(|error| child(error.to_string()))?;
                credentials
                    .bearer(false)
                    .await
                    .map(|bearer| Some(format!("Bearer {bearer}")))
                    .map_err(|error| child(error.to_string()))
            }
        }
    }
}

/// Builds one probe request body laid out as `framing` describes.
///
/// Every variant carries a well-formed first message; only the HTTP/2 frame
/// boundaries differ, which is exactly the property a private listener must be
/// indifferent to.
fn probe_body(framing: PeerProbeFraming, payload: Option<&[u8]>) -> wyrd_tonic::tonic::body::Body {
    // An empty protobuf message is a valid `ReserveNodeSlotsRequest` and a
    // valid oversized-free first frame for the worker adapter, so the probe
    // never depends on a decodable domain payload to reach the boundary. A
    // parent-supplied payload replaces it verbatim, header included, because a
    // ticket binds the digest of exactly those bytes.
    let message = match payload {
        Some(payload) => {
            let mut framed = vec![0_u8];
            framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            framed.extend_from_slice(payload);
            framed
        }
        None => vec![0_u8, 0, 0, 0, 0],
    };
    let chunks = match framing {
        PeerProbeFraming::Whole => vec![message],
        PeerProbeFraming::SplitHeader => vec![message[..2].to_vec(), message[2..].to_vec()],
        PeerProbeFraming::Coalesced => {
            let mut coalesced = message.clone();
            coalesced.extend_from_slice(&message);
            vec![coalesced]
        }
    };
    let frames = futures_util::stream::iter(chunks.into_iter().map(|chunk| {
        Ok::<_, std::convert::Infallible>(http_body::Frame::data(
            wyrd_tonic::tonic::codegen::Bytes::from(chunk),
        ))
    }));
    wyrd_tonic::tonic::body::Body::new(http_body_util::StreamBody::new(frames))
}

/// Reduces one probe response to its non-secret gRPC status name.
///
/// gRPC reports its status in the response headers for a trailers-only
/// refusal and in the trailers otherwise, so both are read before the outcome
/// is decided.
async fn probe_outcome(response: http::Response<wyrd_tonic::tonic::body::Body>) -> String {
    let status = |headers: &http::HeaderMap| -> Option<String> {
        headers
            .get("grpc-status")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i32>().ok())
            .map(|code| format!("{:?}", wyrd_tonic::tonic::Code::from_i32(code)))
    };
    if let Some(outcome) = status(response.headers()) {
        return outcome;
    }
    let mut body = response.into_body();
    while let Some(frame) = http_body_util::BodyExt::frame(&mut body).await {
        match frame {
            Ok(frame) => {
                if let Some(trailers) = frame.trailers_ref()
                    && let Some(outcome) = status(trailers)
                {
                    return outcome;
                }
            }
            Err(error) => return format!("BodyError({error})"),
        }
    }
    "Ok".to_owned()
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
            let snapshot = server.state().readiness.load();
            // Naming the probes is what makes a readiness timeout actionable:
            // without them the parent only learns that some dependency of some
            // role never came up.
            return Err(ProcessClusterError::Timeout(format!(
                "child pid {} readiness: postgres={:?} storage={:?} scribe={:?} oracle={:?} \
                 peer={:?} peer_required={} peer_serving={}",
                std::process::id(),
                snapshot.postgres.reason,
                snapshot.storage.reason,
                snapshot.scribe.reason,
                snapshot.oracle.reason,
                snapshot.peer.reason,
                server.state().peer_plane.is_required(),
                server.state().peer_plane.is_serving(),
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
        ready: config.is_ready(&snapshot, state),
        wal_root: config.wal_root.display().to_string(),
        membership,
    }
}

/// Re-reads the shared membership snapshot into this pod's Oracle.
///
/// Each pod caches its own cluster view, so a journey that just changed
/// membership or published data refreshes the pods it is about to query
/// instead of waiting on their background cadence. A pod that composes no
/// Oracle has nothing to refresh and reports success.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when the snapshot cannot be read.
async fn refresh_snapshot(server: &WyrdTestServer) -> Result<(), ProcessClusterError> {
    let Some(cluster) = server.state().oracle_cluster() else {
        return Ok(());
    };
    cluster
        .refresh_snapshot()
        .await
        .map_err(|error| ProcessClusterError::Child(error.to_string()))
}

/// Reports this pod's cumulative graph-lease activations and live leases.
///
/// A pod that composes no Oracle owns no reservation registry and therefore
/// reports the baseline, which is the correct answer for a Scribe: it never
/// leases a graph.
fn graph_lease_counts(server: &WyrdTestServer) -> (u64, usize) {
    server
        .state()
        .bifrost_query()
        .map_or((0, 0), |oracle| oracle.engine().graph_lease_counts())
}

/// Encodes `rows` deterministic `(id, filter_key)` rows as one Arrow IPC stream.
///
/// The rows are spread over `groups` distinct keys so a grouped aggregate has
/// more than one non-trivial group, which is what makes a distributed plan
/// exchange partitions rather than collapse to a single stage.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when the batch or its IPC encoding
/// cannot be built.
fn fixture_rows_ipc(rows: i64, groups: i64) -> Result<bytes::Bytes, ProcessClusterError> {
    let child = ProcessClusterError::Child;
    let groups = groups.max(1);
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
        arrow::datatypes::Field::new("filter_key", arrow::datatypes::DataType::Utf8, false),
    ]));
    let keys: Vec<String> = (0..rows)
        .map(|id| format!("group_{}", id % groups))
        .collect();
    let batch = arrow::record_batch::RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(arrow::array::Int64Array::from(
                (0..rows).collect::<Vec<_>>(),
            )),
            Arc::new(arrow::array::StringArray::from(keys)),
        ],
    )
    .map_err(|error| child(error.to_string()))?;
    let mut ipc = Vec::new();
    {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, schema.as_ref())
            .map_err(|error| child(error.to_string()))?;
        writer
            .write(&batch)
            .map_err(|error| child(error.to_string()))?;
        writer.finish().map_err(|error| child(error.to_string()))?;
    }
    Ok(bytes::Bytes::from(ipc))
}
