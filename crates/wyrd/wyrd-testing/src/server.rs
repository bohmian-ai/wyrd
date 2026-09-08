//! Real-socket Wyrd server test harness.

use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderValue, Request, Response, StatusCode, header};
use chrono::{Duration as ChronoDuration, Utc};
use ed25519_dalek::VerifyingKey;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;
use vala_bifrost_redux::catalog::{
    BIFROST_CATALOG_NAME, BifrostCatalog, TableRef, TenantTableBinding,
};
use vala_bifrost_redux::cluster::{ClusterRegistry, RoleTiming};
use vala_bifrost_redux::forge::{
    ForgeClock, ForgeClockControl, ForgeConfig, ForgeSchedulerTrigger, ForgeWorker,
    ForgeWorkerCompletionObserver, ForgeWorkerConfig,
};
use vala_bifrost_redux::gate::limits::IngestLimits;
use vala_bifrost_redux::maintenance::{StagingFilePublisher, staging_file_channel};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::oracle::dispatcher::{DispatchError, OraclePeerCredentials};
use vala_bifrost_redux::resources::{
    BifrostResourcePolicy, BifrostRole, BifrostRuntimeResources, BifrostVolumeRoots,
    MIN_UNMANAGED_RESERVE_BYTES, ROLE_MEMORY_FLOOR_BYTES, ResourceSource, SystemResourceSnapshot,
};
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_sdk::BifrostGrpcTransport;
use vala_sql::queries::oracle_reader_authority::OracleTableProtections;
use vala_sql::row_types::oracle_reader_authority::{ProtectionRecord, TableAuthorityIdentity};
use wyrd_auth::exchange_api_key::{ExchangeApiKey, TokenExchangeSettings};
use wyrd_auth::issue_api_key::WyrdApiKey;
use wyrd_auth::permission_resolver::SqlPermissionResolver;
use wyrd_auth::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};
use wyrd_auth::revocation_resolver::SqlRevocationCheck;
use wyrd_auth::seed::seed_builtin_roles_for_tenant;
use wyrd_auth_check::{AuthzCheckRequest, AuthzCheckResponse, PolicyHook};
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_oidc::JwksCache;
use wyrd_auth_verify::{
    Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_crypt::SecretKey;
use wyrd_dev_fixtures::pg::PgFixture;
#[cfg(test)]
use wyrd_runtime::PermissionSet;
use wyrd_runtime::{Permission, PrincipalId, RbacCheck, RoleRef};
use wyrd_semver::VersionBlock;
use wyrd_server::boot::build_workload_bindings;
use wyrd_server::boot::issuer::{seed_trusted_issuers, seed_workload_bindings};
use wyrd_server::components::auth::audit_writer::{AuthzAuditWriter, NoopAuthzAuditWriter};
use wyrd_server::config::{
    BifrostRuntimeConfig, BifrostRuntimeRole, BifrostTarget, DeploymentProfile, ForgeRuntimeConfig,
    IssuerEntry, ServeMode, WorkloadBindingEntry,
};
use wyrd_server::postgres::ServerPostgres;
use wyrd_server::state::{
    BifrostBuildInputs, BifrostShutdownReport, BifrostTestControls, ComposedBifrost,
    QueryStreamFault, QueryStreamFaultController, ScribeCoordinationRuntime,
};
use wyrd_server::{AppState, WyrdServer, WyrdServerConfig, build_router};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{FencingToken, NodeId};
use wyrd_telemetry::TelemetryGuard;

use crate::bifrost::ForgeObjectStoreControl;

/// Harness-owned credential that exchanges one persisted Service API key.
struct TestOraclePeerCredentials {
    /// Shared real Postgres fixture containing the credential record.
    fixture: Arc<PgFixture>,
    /// Durable API key retained only for the cluster lifetime.
    api_key: SecretString,
    /// Production exchange service used for each access-token acquisition.
    exchange: ExchangeApiKey,
    /// Cached short-lived service bearer shared by every peer RPC in the
    /// cluster, retained with its own expiry so a journey that outlives one
    /// access TTL re-exchanges instead of presenting an expired token.
    bearer: tokio::sync::Mutex<Option<(String, chrono::DateTime<chrono::Utc>)>>,
}

impl std::fmt::Debug for TestOraclePeerCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestOraclePeerCredentials")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl OraclePeerCredentials for TestOraclePeerCredentials {
    /// Exchange the retained API key through the production auth service.
    async fn bearer(&self, force_refresh: bool) -> Result<String, DispatchError> {
        let mut cached = self.bearer.lock().await;
        let fresh_until = chrono::Utc::now() + chrono::Duration::seconds(60);
        if !force_refresh
            && let Some((bearer, expires_at)) = cached.as_ref()
            && *expires_at > fresh_until
        {
            return Ok(bearer.clone());
        }
        let mut conn = self
            .fixture
            .tenant_conn_for(DataTenantId::SYSTEM_OWNER)
            .await
            .map_err(|_| DispatchError::Terminal)?;
        let exchanged = self
            .exchange
            .execute(
                &mut conn,
                self.api_key.clone(),
                &RequestId::now_v7().to_string(),
            )
            .await
            .map_err(|_| DispatchError::Terminal)?;
        conn.commit().await.map_err(|_| DispatchError::Terminal)?;
        let bearer = exchanged.access_token.expose_secret().to_owned();
        *cached = Some((bearer.clone(), exchanged.expires_at));
        Ok(bearer)
    }
}

use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::auth::{
    RequestedSubject, SecretBearer, SubjectTokenType, TokenRequest, TokenResponse,
};
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    grant_role_to_service_account, grant_role_to_user, insert_api_key, insert_role,
    insert_service_account, insert_user, revoke_role_from_service_account, revoke_role_from_user,
    role_by_name, trusted_issuer_by_url, workload_binding_by_subject,
};
use wyrd_storage::{BackendConfig, StorageSettings};

use crate::time::ClockHandle;

/// Dedicated least-privilege role assigned to the test Oracle Service.
const ORACLE_PEER_ROLE: &str = "bifrost_oracle_peer";

/// Default Forge compaction budget a harness node carrying a Forge role names.
///
/// The production default is four fifths of the memory limit and deliberately
/// does not clamp, so a co-located harness whose Scribe and Oracle floors are
/// also protected must name a budget that fits the remainder — exactly as a
/// co-located deployment configures one. The constant is public because a test
/// that pins two Oracle replicas to one durable admission ceiling has to
/// subtract the same reservation the Forge-carrying replica takes.
pub const HARNESS_FORGE_COMPACTION_BUDGET_BYTES: usize = 256 * 1024 * 1024;

/// Memory limit a harness node observes when a test injects no snapshot.
///
/// Every node in a test cluster is carved from the same fixed observation, so
/// role placement is the only thing that moves a node's derived plan. The
/// constant is public because a test that pins two Oracle replicas to one
/// durable admission ceiling has to name the limit it sheds role floors from.
pub const HARNESS_NODE_MEMORY_LIMIT_BYTES: usize = 3 * 1024 * 1024 * 1024;

/// Separates a serve-task join failure from the server's own terminal outcome.
///
/// The load-bearing case is [`tokio::task::JoinError::is_panic`]: a panic on the
/// serve stack unwinds a `tokio-runtime-worker` thread that libtest never
/// attributes, so discarding the join result lets a panicking run finish green.
/// Returning it as [`WyrdTestServerError::Join`] makes every teardown seam that
/// joins the serve task fail loudly instead. The inner `Result` is the server's
/// own outcome and is left to the caller, which decides whether a terminal exit
/// is expected.
///
/// # Errors
/// Returns [`WyrdTestServerError::Join`] when the serve task panicked or was
/// cancelled before producing an outcome.
fn serve_task_outcome(
    join: Result<Result<BifrostShutdownReport, wyrd_server::BootExit>, tokio::task::JoinError>,
) -> Result<Result<BifrostShutdownReport, wyrd_server::BootExit>, WyrdTestServerError> {
    join.map_err(|error| WyrdTestServerError::Join(format!("bound serve task: {error}")))
}

/// Wyrd server test harness supporting in-process and real-socket modes.
pub struct WyrdTestServer {
    inner: WyrdTestServerInner,
    mode: Mode,
    shutdown_token: Option<CancellationToken>,
    /// Serve task for a bound server, yielding the production Bifrost drain
    /// outcome so teardown seams can assert what actually drained.
    serve_handle: Option<JoinHandle<Result<BifrostShutdownReport, wyrd_server::BootExit>>>,
    /// Optional fixed HTTP/gRPC addresses reserved by a multi-node harness.
    requested_bind: Option<(std::net::SocketAddr, std::net::SocketAddr)>,
    /// Optional TLS material applied when this in-process server binds.
    requested_oracle_peer_tls: Option<TestOraclePeerTls>,
    /// Test-only readiness failure requested by the builder.
    readiness_failure: bool,
    /// Optional test-only serve task that ignores cancellation until aborted.
    stalled_drain_for_test: Option<Arc<AtomicBool>>,
    /// Test-only request to panic the bound serve task after its drain returns.
    serve_task_panic_for_test: bool,
}

struct WyrdTestServerInner {
    fixture: Arc<PgFixture>,
    /// Lifetime guard retained only for local storage-backed servers.
    _storage_root: Option<Arc<tempfile::TempDir>>,
    _scribe_wal_root: Option<Arc<tempfile::TempDir>>,
    /// Lifetime guard for the Forge and Oracle DataFusion spill root.
    _bifrost_spill_root: Option<Arc<tempfile::TempDir>>,
    /// Lifetime guard for the cluster-retained Oracle audit WAL root.
    _oracle_audit_wal_root: Option<Arc<tempfile::TempDir>>,
    state: AppState,
    router: axum::Router,
    verifier: Arc<TokenVerifier<SqlPermissionResolver, PgIssuerResolver>>,
    issuing_key: Arc<IssuingKey>,
    api_key: SecretString,
    forge_publisher: StagingFilePublisher,
    /// Redux catalog retained for test-only built-in provisioning.
    bifrost_catalog: Arc<BifrostCatalog>,
    /// Manual wall-clock control shared by Forge fixtures derived from this server.
    forge_clock: ForgeClockControl,
    /// Trigger that wakes the real supervised Forge scheduler.
    forge_scheduler_trigger: ForgeSchedulerTrigger,
    /// Passive controls retained by the actual production-composed Forge object store.
    forge_object_store: Option<Arc<ForgeObjectStoreControl>>,
    /// Process composition selected for this test server.
    forge_process_role: BifrostTarget,
    /// Stable identity assigned to this server process.
    node_id: NodeId,
    /// Atomic one-shot query truncation controls for language journeys.
    query_stream_fault: QueryStreamFaultController,
    /// Atomic lifecycle-audit fault controls for causal query tests.
    query_control_audit_fault: wyrd_server::state::QueryControlAuditFaultController,
    /// Current notification-backed schema stall used by cancellation journeys.
    query_stream_stall: std::sync::Mutex<Option<Arc<wyrd_server::state::QueryStreamStall>>>,
    /// Sole owner of the dedicated Scribe coordination runtime this server composed.
    ///
    /// The harness calls `compose_bifrost` directly, so a bound test server
    /// builds the same dedicated executor production does and must retain the
    /// same single owner. Declared last so struct drop order releases it after
    /// `state`, which is the order production teardown also takes.
    _coordination_runtime: ScribeCoordinationRuntime,
    /// Sole owner of the dedicated Forge compaction runtime this server composed.
    ///
    /// Retained for the same reason as the coordination runtime above, and
    /// released in the same struct drop order: an admitted plan runner must not
    /// be abandoned between writing its outputs and Preparing its operation.
    _compaction_runtime: wyrd_server::state::ForgeCompactionRuntime,
}

/// Concrete lifecycle evidence returned after one test server stops.
#[derive(Debug)]
pub struct ServerShutdownInspection {
    /// Final Scribe ownership snapshot when this process hosted Scribe.
    pub scribe: Option<vala_bifrost_redux::scribe::telemetry::ScribeInspectionSnapshot>,
    /// Whether the bound listener supervisor joined successfully.
    pub listeners_stopped: bool,
    /// Supervisor join handles still retained after shutdown.
    pub supervised_tasks: u64,
}

/// Exact query-owned resources inspected by test-tier cancellation journeys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BifrostQueryResourceSnapshot {
    /// Durable Oracle admission slot units currently retained.
    pub admission_slots: u64,
    /// Parent-governed bytes currently retained by Oracle queries.
    pub memory_bytes: u64,
    /// Local leader or peer-worker slot units currently retained.
    pub peer_slots: u64,
    /// Scribe tail fences currently retained for Fused reads.
    pub tail_fences: u64,
}

/// Production-owner Oracle residual state captured without a test adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OracleRuntimeInspection {
    /// Queries holding local admission grants.
    pub active_queries: u64,
    /// Queries waiting for local admission.
    pub queued_queries: u64,
    /// Memory bytes reserved by active queries.
    pub reserved_memory_bytes: u64,
    /// Spill bytes reserved by active queries.
    pub reserved_spill_bytes: u64,
    /// Peer reservations waiting for worker execution.
    pub peer_pending: u64,
    /// Peer reservations executing worker streams.
    pub peer_running: u64,
    /// Accepted audit records retained in the local WAL.
    pub audit_wal_records: u64,
    /// Bytes retained in the local WAL.
    pub audit_wal_bytes: u64,
    /// Age of the oldest retained WAL record.
    pub audit_oldest_age: Option<Duration>,
    /// Active Oracle-owned process/query scratch directories.
    pub spill_directories: u64,
    /// Regular files beneath the Oracle-owned scratch prefix.
    pub spill_files: u64,
    /// Exact regular-file bytes beneath the Oracle-owned scratch prefix.
    pub spill_file_bytes: u64,
}

/// One exact current Iceberg data-file identity observed through Forge discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeDataFileInspection {
    /// Catalog path referenced by the current snapshot.
    pub path: String,
    /// Compressed Parquet bytes recorded in the manifest.
    pub bytes: u64,
}

/// One `vala.file_list` row exactly as the publication columns store it.
///
/// The decoding into [`PublishedHotFileInspection`] is deliberately separate:
/// this type is the SQL shape, and refusing a negative width or an undecodable
/// promotion record happens once, on the way out of it.
#[derive(sqlx::FromRow)]
struct PublishedHotFileRow {
    /// Durable row identity.
    id: uuid::Uuid,
    /// Deterministic object key.
    file_path: String,
    /// Recorded object bytes.
    file_size: i64,
    /// Recorded row count.
    row_count: i64,
    /// Zero-based artifact ordinal.
    file_ordinal: i16,
    /// Lowercase SHA-256 of the object.
    file_checksum: Option<String>,
    /// Stored promotion evidence.
    promotion_record: serde_json::Value,
    /// Inclusive lower WAL bound the publication set recorded.
    wal_lsn_min: i64,
    /// Inclusive upper WAL bound the publication set recorded.
    wal_lsn_max: i64,
}

/// One committed Scribe hot object as `vala.file_list` recorded it.
///
/// This is the fenced row plus the promotion evidence published with it, so a
/// production test can compare what Scribe committed against the object that
/// exists without reconstructing either from the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedHotFileInspection {
    /// Durable `file_list` row identity.
    pub id: uuid::Uuid,
    /// Deterministic object key the row names.
    pub object_key: String,
    /// Exact object bytes recorded at publication.
    pub file_size: u64,
    /// Rows the object contains.
    pub row_count: u64,
    /// Zero-based artifact ordinal within its publication set.
    pub file_ordinal: i16,
    /// Lowercase SHA-256 of the published object.
    pub file_checksum: String,
    /// Typed promotion evidence committed in the same transaction.
    pub promotion_record: vala_bifrost_redux::scribe::promotion::ScribePublishedHotFileV1,
    /// Inclusive lower WAL bound the publication set recorded.
    ///
    /// The bound spans every member the publishing claim merged, so it is an
    /// envelope over the node-global WAL counter rather than a statement about
    /// which members the object owns.
    pub wal_lsn_min: u64,
    /// Inclusive upper WAL bound the publication set recorded.
    pub wal_lsn_max: u64,
}

/// Durable pre-snapshot Forge workflow state for one tenant table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeWorkflowInspection {
    /// Whether a coalesced planning demand remains.
    pub has_demand: bool,
    /// Durable strategy/state pairs in creation order.
    pub tasks: Vec<(String, String)>,
    /// Tasks retaining active claims.
    pub active_claims: u64,
    /// Distinct non-terminal attempts.
    pub active_attempts: u64,
    /// Durable staging files not yet represented by an Iceberg fold.
    pub uncompacted_staging_files: u64,
}

/// Exact baseline-derived files consumed and produced by one Forge rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeRewriteComparison {
    /// Planned baseline files removed from the current snapshot.
    pub input_files: Vec<ForgeDataFileInspection>,
    /// Checked input byte total.
    pub input_bytes: u64,
    /// New replacement files referenced by the current snapshot.
    pub output_files: Vec<ForgeDataFileInspection>,
    /// Checked output byte total.
    pub output_bytes: u64,
}

/// Stable pointer identities for one server-owned runtime pool graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostgresPoolIdentity {
    /// Wyrd application pool identity.
    pub wyrd_app: usize,
    /// Vala/Bifrost application pool identity.
    pub vala_app: usize,
}

enum Mode {
    InProcess,
    Bound {
        addr: std::net::SocketAddr,
        base_url: String,
        grpc_addr: std::net::SocketAddr,
    },
}

/// Builder for [`WyrdTestServer`].
pub struct WyrdTestServerBuilder {
    policy_hook: Option<Arc<dyn PolicyHook>>,
    audit_writer: Option<Arc<dyn AuthzAuditWriter>>,
    /// Optional eval-run audit sink installed on the composed `AppState`.
    eval_audit: Option<Arc<dyn wyrd_server::components::eval::EvalAuditWriter>>,
    allow_preview_auth: bool,
    storage_settings: Option<StorageSettings>,
    storage_handle: Option<Arc<wyrd_storage::StorageHandle>>,
    access_ttl: Option<ChronoDuration>,
    auth_verify_settings: Option<WyrdAuthVerifySettings>,
    trusted_issuer_configs: Vec<IssuerEntry>,
    workload_binding_configs: Vec<WorkloadBindingEntry>,
    forge_interval: Duration,
    /// Executor slots composed into the production Forge worker.
    wal_sync_delay: Duration,
    scribe_admission: Option<AdmissionConfig>,
    /// One immutable lowerable limits snapshot shared by the test server's ingest owners.
    scribe_ingest_limits: IngestLimits,
    /// Optional projection of production Scribe rotation limits for real-server journeys.
    scribe_geometry_for_test: Option<vala_bifrost_redux::scribe::geometry::ScribeGeometry>,
    /// Optional faults installed in the real Scribe persistence graph for journeys.
    scribe_persistence_faults_for_test:
        Option<vala_bifrost_redux::scribe::persistence::PersistenceFaults>,
    /// Optional accelerated role cadence for heartbeat-specific journeys.
    role_timing: Option<RoleTiming>,
    /// Stable node identity retained when a cluster restarts this builder.
    node_id: Option<NodeId>,
    /// Closed role set constructed for this server instance.
    bifrost_roles: BTreeSet<BifrostRuntimeRole>,
    /// Cluster-retained Scribe WAL root reused across restarts.
    scribe_wal_root: Option<Arc<tempfile::TempDir>>,
    /// Cluster-retained Forge/Oracle spill root reused across restarts.
    oracle_spill_root: Option<Arc<tempfile::TempDir>>,
    /// Complete process observations injected into the production resource policy.
    system_resources: Option<SystemResourceSnapshot>,
    /// Forge compaction budget replacing the harness default on this node.
    forge_compaction_memory_limit_bytes: Option<usize>,
    /// Cluster-retained Oracle audit WAL root reused across restarts.
    oracle_audit_wal_root: Option<Arc<tempfile::TempDir>>,
    /// Process-installed production telemetry guard shared by every node.
    telemetry: Option<Arc<TelemetryGuard>>,
    /// Optional fixed HTTP/gRPC addresses used for truthful peer advertisement.
    bind_addrs: Option<(std::net::SocketAddr, std::net::SocketAddr)>,
    /// Optional cluster-scoped Oracle peer credential injected by the harness.
    oracle_peer_credentials: Option<Arc<dyn OraclePeerCredentials>>,
    /// Optional production-shaped Oracle server identity and peer trust paths.
    oracle_peer_tls: Option<TestOraclePeerTls>,
    /// Production Forge process role used by bound test servers.
    forge_process_role: BifrostTarget,
    /// Optional observer of successful supervised worker completions.
    forge_completion_observer: Option<ForgeWorkerCompletionObserver>,
    /// Optional test-only Forge catalog wrapper.
    forge_catalog: Option<Arc<dyn iceberg::Catalog>>,
    /// Optional complete Forge limit profile.
    forge_config: Option<ForgeConfig>,
    /// Force readiness failure after listeners are bound for rollback tests.
    readiness_failure: bool,
    /// Compose the server with no token verifier configured.
    omit_token_verifier: bool,
    /// Optional non-default edge limits applied to the composed `AppState`.
    limits: Option<wyrd_server::state::LimitsConfig>,
    /// Replace the serve task with a cancellation-resistant test task.
    stalled_drain_for_test: Option<Arc<AtomicBool>>,
    /// Test-only request to panic the bound serve task after its drain returns.
    serve_task_panic_for_test: bool,
}

/// Test-only file paths for a shared Oracle TLS identity and trust root.
#[derive(Clone)]
pub(crate) struct TestOraclePeerTls {
    /// Server certificate chain PEM path.
    pub(crate) certificate_path: std::path::PathBuf,
    /// Server private-key PEM path.
    pub(crate) private_key_path: std::path::PathBuf,
    /// Peer CA certificate PEM path.
    pub(crate) ca_path: std::path::PathBuf,
    /// Certificate DNS identity expected by clients.
    pub(crate) server_name: String,
}

impl Default for WyrdTestServerBuilder {
    fn default() -> Self {
        Self {
            policy_hook: None,
            audit_writer: None,
            allow_preview_auth: true,
            storage_settings: None,
            storage_handle: None,
            access_ttl: None,
            auth_verify_settings: None,
            trusted_issuer_configs: Vec::new(),
            workload_binding_configs: Vec::new(),
            forge_interval: Duration::from_secs(60),
            wal_sync_delay: Duration::ZERO,
            scribe_admission: None,
            scribe_ingest_limits: IngestLimits::default(),
            scribe_geometry_for_test: None,
            scribe_persistence_faults_for_test: None,
            role_timing: None,
            node_id: None,
            bifrost_roles: [
                BifrostRuntimeRole::Scribe,
                BifrostRuntimeRole::ForgeCoordinator,
                BifrostRuntimeRole::ForgeWorker,
                BifrostRuntimeRole::Oracle,
            ]
            .into_iter()
            .collect(),
            scribe_wal_root: None,
            oracle_spill_root: None,
            system_resources: None,
            forge_compaction_memory_limit_bytes: None,
            oracle_audit_wal_root: None,
            telemetry: None,
            bind_addrs: None,
            oracle_peer_credentials: None,
            oracle_peer_tls: None,
            forge_process_role: BifrostTarget::All,
            forge_completion_observer: None,
            forge_catalog: None,
            forge_config: None,
            readiness_failure: false,
            eval_audit: None,
            omit_token_verifier: false,
            limits: None,
            stalled_drain_for_test: None,
            serve_task_panic_for_test: false,
        }
    }
}

/// Result of a fixture-path principal bootstrap.
pub use crate::principal::Bootstrap;

/// Result of an authz-check request.
pub use crate::principal::CheckResult;

/// Test server errors.
#[derive(Debug, Error)]
pub enum WyrdTestServerError {
    /// Server startup failed.
    #[error("test server failed to start: {0}")]
    Start(String),
    /// TCP listener bind failed.
    #[error("test server failed to bind: {0}")]
    Bind(String),
    /// Serve task join failed.
    #[error("test server join failed: {0}")]
    Join(String),
    /// HTTP path returned an error status.
    #[error("HTTP error: status={status}, code={code}, body={body}")]
    Http {
        /// HTTP status.
        status: StatusCode,
        /// Stable Wyrd error code when present.
        code: String,
        /// Response body.
        body: String,
    },
    /// Auth flow failed.
    #[error("auth flow failed: {0}")]
    Auth(String),
    /// Audit query failed.
    #[error("audit query failed: {0}")]
    Audit(String),
    /// IO or request construction failed.
    #[error("io error: {0}")]
    Io(String),
    /// Surface is intentionally unsupported.
    #[error("not supported in this build: {0}")]
    Unsupported(String),
    /// SQL operation failed.
    #[error("sql error: {0}")]
    Sql(String),
}

impl From<WyrdTestServerError> for wyrd_spec::error::WyrdError {
    fn from(err: WyrdTestServerError) -> Self {
        let msg = err.to_string();
        match err {
            WyrdTestServerError::Start(_)
            | WyrdTestServerError::Sql(_)
            | WyrdTestServerError::Http { .. }
            | WyrdTestServerError::Io(_)
            | WyrdTestServerError::Unsupported(_) => wyrd_spec::error::WyrdError::HarnessStart {
                message: msg,
                details: serde_json::json!({}),
            },
            WyrdTestServerError::Bind(_) | WyrdTestServerError::Join(_) => {
                wyrd_spec::error::WyrdError::HarnessBound {
                    message: msg,
                    details: serde_json::json!({}),
                }
            }
            WyrdTestServerError::Auth(_) | WyrdTestServerError::Audit(_) => {
                wyrd_spec::error::WyrdError::HarnessBootstrap {
                    message: msg,
                    details: serde_json::json!({}),
                }
            }
        }
    }
}

impl WyrdTestServer {
    /// Create a builder for customised startup.
    #[must_use]
    pub fn builder() -> WyrdTestServerBuilder {
        WyrdTestServerBuilder::default()
    }

    /// Start an in-process Wyrd server harness with default settings.
    ///
    /// # Errors
    /// Returns an error when database, storage, auth, or router state cannot be created.
    pub async fn start_in_process() -> Result<Self, WyrdTestServerError> {
        Self::builder().start_in_process().await
    }

    /// Start a Wyrd server bound to a real OS-assigned TCP socket.
    ///
    /// # Errors
    /// Returns an error when startup or socket binding fails.
    pub async fn start_bound() -> Result<Self, WyrdTestServerError> {
        Self::builder().start_bound().await
    }

    /// Shut down the server, cancelling the serve task and dropping fixtures.
    ///
    /// A serve task that panicked is a real production defect, so its
    /// [`JoinError`] is propagated rather than discarded: swallowing it lets a
    /// panic on a `tokio-runtime-worker` thread finish the run green, which is
    /// precisely the failure mode this seam exists to catch. A join *timeout*
    /// remains tolerated — the bounded budget here is deliberately short and a
    /// slow drain is not the same signal as a panic.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Join`] when the serve task panicked or
    /// when the final blocking drop cannot be joined.
    pub async fn shutdown(mut self) -> Result<(), WyrdTestServerError> {
        if let Some(token) = self.shutdown_token.take() {
            token.cancel();
        }
        if let Some(handle) = self.serve_handle.take()
            && let Ok(join) = tokio::time::timeout(Duration::from_secs(2), handle).await
            && let Err(exit) = serve_task_outcome(join)?
        {
            tracing::warn!(?exit, "bound serve task exited terminally during shutdown");
        }
        tokio::task::spawn_blocking(move || drop(self))
            .await
            .map_err(|error| WyrdTestServerError::Join(error.to_string()))?;
        Ok(())
    }

    /// Cancel the serve task, join it in place, and return its drain outcome.
    ///
    /// Unlike [`shutdown`](Self::shutdown) and
    /// [`shutdown_and_inspect`](Self::shutdown_and_inspect) this seam borrows
    /// `self` instead of consuming it, so the caller keeps the harness — and
    /// therefore the last [`AppState`] — alive past the join. That is what makes
    /// production teardown reproducible from a test: once the serve task has
    /// dropped its own `AppState` clone, the caller's frame owns the final
    /// reference and drops it wherever the test chooses, rather than inside the
    /// `spawn_blocking` that `shutdown` uses.
    ///
    /// The returned [`BifrostShutdownReport`] is the report `BoundServer::run`
    /// produced from the real production drain, not a test-invoked second
    /// shutdown.
    ///
    /// # Errors
    /// Returns [`WyrdTestServerError::Start`] when no bound serve task is
    /// running or when the server exited terminally, and
    /// [`WyrdTestServerError::Join`] when the serve task panicked or the join
    /// exceeded its bounded budget.
    pub async fn cancel_and_join_for_test(
        &mut self,
    ) -> Result<BifrostShutdownReport, WyrdTestServerError> {
        if let Some(token) = self.shutdown_token.take() {
            token.cancel();
        }
        let handle = self.serve_handle.take().ok_or_else(|| {
            WyrdTestServerError::Start(
                "in-place teardown requires a running bound server".to_owned(),
            )
        })?;
        let join = tokio::time::timeout(Duration::from_secs(70), handle)
            .await
            .map_err(|_| {
                WyrdTestServerError::Join("serve task did not join before its deadline".to_owned())
            })?;
        serve_task_outcome(join)?.map_err(|exit| {
            WyrdTestServerError::Start(format!("server exited terminally: {exit:?}"))
        })
    }

    /// Abruptly terminate the test server without running graceful Scribe drain.
    ///
    /// This test-tier seam aborts the bound supervisor after cancellation and
    /// then drops the server, leaving configured WAL and storage roots owned by
    /// the caller's cluster fixture for replay assertions.
    ///
    /// # Errors
    ///
    /// Returns an error only when the supervisor join reports a panic. Normal
    /// task cancellation is treated as the expected abrupt termination path.
    pub async fn terminate_abruptly_for_test(mut self) -> Result<(), WyrdTestServerError> {
        if let Some(token) = self.shutdown_token.take() {
            token.cancel();
        }
        if let Some(handle) = self.serve_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        if let Some(query) = self.inner.state.bifrost_query() {
            query.abort_audit_tasks_for_test().await;
        }
        Ok(())
    }

    /// Await a bound production supervisor that must fail terminally.
    ///
    /// The method does not initiate shutdown. It consumes the server only after
    /// `WyrdServer::run` has observed a terminal subsystem failure, removed
    /// readiness, cancelled sibling work, and completed its bounded drain.
    ///
    /// # Errors
    ///
    /// Returns an error on timeout, task-join failure, or an unexpected clean
    /// server exit. The returned string is the production terminal exit.
    pub async fn await_terminal_failure_for_test(
        mut self,
        deadline: Duration,
    ) -> Result<String, WyrdTestServerError> {
        let handle = self.serve_handle.take().ok_or_else(|| {
            WyrdTestServerError::Start(
                "terminal-failure wait requires a running bound server".to_owned(),
            )
        })?;
        let result = tokio::time::timeout(deadline, handle)
            .await
            .map_err(|_| {
                WyrdTestServerError::Start(
                    "production supervisor did not fail within the bounded drain deadline"
                        .to_owned(),
                )
            })?
            .map_err(|error| WyrdTestServerError::Join(error.to_string()))?;
        let terminal = match result {
            Ok(_report) => Err(WyrdTestServerError::Start(
                "production supervisor exited cleanly while terminal failure was required"
                    .to_owned(),
            )),
            Err(exit) => Ok(format!("{exit:?}")),
        };
        tokio::task::spawn_blocking(move || drop(self))
            .await
            .map_err(|error| WyrdTestServerError::Join(error.to_string()))?;
        terminal
    }

    /// Shut down the bound workers and return concrete owner lifecycle evidence.
    ///
    /// Cancels the server shutdown token, then joins the serve task before any
    /// Scribe seal runs. The load-bearing invariant is that no Scribe seal or
    /// flush may run while the forge worker is still live: `token.cancel()` only
    /// signals the worker, which remains live inside its claim loop until it is
    /// joined. In `Mode::Bound`, joining `serve_handle` runs `WyrdServer::run`'s
    /// production drain-then-seal to completion, so the join is the point at
    /// which worker liveness ends. Sealing before that join would let the live
    /// worker claim a freshly sealed generation it cannot finish, stranding a
    /// non-terminal Forge claim. The join timeout therefore covers the real
    /// drain-and-seal budget, not a pre-drained join. The explicit
    /// [`ScribeImpl::shutdown`] afterward is the idempotent path for non-Bound
    /// harness modes that never ran `bound.run()`; after the join the worker is
    /// quiesced, so it drains a stable pipeline.
    ///
    /// # Errors
    /// Returns an error if the serve task join times out or the final Scribe
    /// inspection cannot be read.
    pub async fn shutdown_and_inspect(
        mut self,
    ) -> Result<ServerShutdownInspection, WyrdTestServerError> {
        if let Some(token) = self.shutdown_token.take() {
            token.cancel();
        }
        let listeners_stopped = if let Some(handle) = self.serve_handle.take() {
            tokio::time::timeout(Duration::from_secs(70), handle)
                .await
                .map_err(|_| WyrdTestServerError::Start("server shutdown timed out".to_owned()))?
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?
                .map_err(|error| WyrdTestServerError::Start(format!("server exited: {error:?}")))?;
            true
        } else {
            !matches!(self.mode, Mode::Bound { .. })
        };
        let scribe = self
            .inner
            .state
            .bifrost_scribe_for_test()
            .map(|_| self.scribe_inspection_snapshot())
            .transpose()?;
        let inspection = ServerShutdownInspection {
            scribe,
            listeners_stopped,
            supervised_tasks: self.supervised_task_count_for_test() as u64,
        };
        tokio::task::spawn_blocking(move || drop(self))
            .await
            .map_err(|error| WyrdTestServerError::Join(error.to_string()))?;
        Ok(inspection)
    }

    /// Cancel bound server workers without dropping the server-owned fixtures.
    ///
    /// Gated journeys use this to verify worker cleanup and then inspect or
    /// drain durable state before [`Self::shutdown`] releases the test database.
    pub fn cancel_bound_workers(&self) {
        if let Some(token) = &self.shutdown_token {
            token.cancel();
        }
    }

    /// Return the number of bound supervisor tasks still owned by this server.
    #[must_use]
    pub fn supervised_task_count_for_test(&self) -> usize {
        usize::from(self.serve_handle.is_some())
    }

    /// Flush the server-owned Scribe through its staged and claim lifecycle.
    ///
    /// This is intentionally test-tier only: production callers use the
    /// generation and shutdown coordinators rather than reaching into Scribe.
    /// The flush itself is the production path, so what a journey observes
    /// afterwards is what a real pod publishes.
    ///
    /// # Errors
    /// Returns an error when the server has no Scribe or a residue claim
    /// cannot publish.
    pub async fn flush_bifrost(&self) -> Result<(), WyrdTestServerError> {
        self.inner
            .state
            .flush_scribe_for_test()
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Freeze every writable generation into an immutable staged member.
    ///
    /// This is the first half of the durable lifecycle taken on its own.
    /// [`flush_bifrost`](Self::flush_bifrost) performs the freeze and the
    /// publication together, so a caller that only uses it can never observe
    /// the state between them. Sealing separately is what makes the
    /// active-to-staged boundary a real transition a workload can checkpoint
    /// rather than a name with nothing behind it.
    ///
    /// The seal is pod-wide, matching the shard owner it drives: there is no
    /// per-tenant freeze.
    ///
    /// # Errors
    ///
    /// Returns an error when the server owns no Scribe, or when a shard cannot
    /// freeze its writable generations. A shard that did not freeze leaves its
    /// rows appendable and WAL-authoritative, so the caller may retry.
    pub async fn seal_bifrost_writable_for_test(&self) -> Result<(), WyrdTestServerError> {
        self.bifrost_scribe()
            .ok_or_else(|| WyrdTestServerError::Start("server owns no Scribe".to_owned()))?
            .flush_writable_for_test()
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Publish only the staged residue belonging to one physical partition.
    ///
    /// [`flush_bifrost`](Self::flush_bifrost) settles every ready key at once,
    /// so a caller that only uses it can never observe a pod in which one
    /// partition is served by a published hot object while a neighbouring
    /// partition is still served by its live authority. That mixed state is an
    /// ordinary production state, and it is the state a pinned cut has to read
    /// correctly. This drives the same production residue and fenced
    /// publication owner for the already-staged keys of one partition.
    ///
    /// Returns how many claims published.
    ///
    /// # Errors
    ///
    /// Returns an error when the server owns no Scribe, or when the selected
    /// key's residue claim or its fenced publication is refused. The remaining
    /// keys stay staged and their WAL stays authoritative.
    pub async fn publish_bifrost_partition_for_test(
        &self,
        partition: vala_bifrost_redux::catalog::TimePartition,
    ) -> Result<usize, WyrdTestServerError> {
        self.bifrost_scribe()
            .ok_or_else(|| WyrdTestServerError::Start("server owns no Scribe".to_owned()))?
            .publish_partition_for_test(partition)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Report where every generation this pod still tracks is readable.
    ///
    /// The hot-source registry is the pod's single answer to that question, so
    /// a case proving one partition is hot-published while another is still
    /// live reads the authority itself rather than inferring it.
    ///
    /// # Errors
    ///
    /// Returns an error when the server owns no Scribe or the registry lock is
    /// poisoned.
    pub fn bifrost_live_authorities_for_test(
        &self,
    ) -> Result<Vec<vala_bifrost_redux::scribe::hot_source::LiveAuthority>, WyrdTestServerError>
    {
        self.bifrost_scribe()
            .ok_or_else(|| WyrdTestServerError::Start("server owns no Scribe".to_owned()))?
            .live_authorities_for_test()
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Seed deterministic rows through the public gRPC ingest and Scribe flush paths.
    ///
    /// # Errors
    ///
    /// Returns an error when the service credential, Arrow IPC encoding, gRPC
    /// ingest, or durable flush cannot complete.
    pub async fn seed_bifrost_rows(
        &self,
        table: &str,
        rows: &[i64],
    ) -> Result<Vec<i64>, WyrdTestServerError> {
        self.seed_bifrost_rows_for_tenant(self.data_tenant_id(), table, rows)
            .await
    }

    /// Seed deterministic rows through public ingest for one explicit tenant.
    ///
    /// This helper creates the writer after the server is already ready, then
    /// uses the normal API-key exchange, public gRPC ingest, and tenant-bound
    /// Scribe flush paths. It is the production-shaped late-tenant write seam.
    ///
    /// # Errors
    ///
    /// Returns an error when tenant-scoped credential creation, Arrow IPC
    /// encoding, public ingest, or the tenant-bound durable flush fails.
    pub async fn seed_bifrost_rows_for_tenant(
        &self,
        tenant_id: DataTenantId,
        table: &str,
        rows: &[i64],
    ) -> Result<Vec<i64>, WyrdTestServerError> {
        let bootstrap = self
            .bootstrap_service_in_tenant(
                tenant_id,
                &format!("bifrost-seed-{}", Uuid::now_v7().simple()),
                &["admin"],
            )
            .await?;
        let api_key = bootstrap
            .api_key()
            .ok_or_else(|| WyrdTestServerError::Auth("seed requires a service key".to_owned()))?
            .clone();
        let schema = std::sync::Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            std::sync::Arc::clone(&schema),
            vec![std::sync::Arc::new(Int64Array::from(rows.to_vec()))],
        )
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let mut ipc = Vec::new();
        let mut writer = StreamWriter::try_new(&mut ipc, schema.as_ref())
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        writer
            .write(&batch)
            .and_then(|()| writer.finish())
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let client = WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: self
                    .grpc_url()
                    .ok_or_else(|| WyrdTestServerError::Start("missing gRPC URL".to_owned()))?,
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: self
                    .base_url()
                    .ok_or_else(|| WyrdTestServerError::Start("missing HTTP URL".to_owned()))?
                    .to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(api_key),
            ..ClientConfig::default()
        })
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        BifrostGrpcTransport::connect(&client)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?
            .insert_batch(table, Uuid::now_v7().into_bytes(), ipc)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        self.flush_bifrost().await?;
        Ok(rows.to_vec())
    }

    /// Mint an authenticated token that intentionally lacks query-read permission.
    ///
    /// # Errors
    ///
    /// Returns an authentication error when the viewer service cannot be
    /// bootstrapped or its API key cannot be exchanged.
    pub async fn query_denied_token(&self) -> Result<String, WyrdTestServerError> {
        let bootstrap = self
            .bootstrap_service(
                &format!("bifrost-denied-{}", Uuid::now_v7().simple()),
                &["reader"],
            )
            .await?;
        let api_key = bootstrap.api_key().ok_or_else(|| {
            WyrdTestServerError::Auth("denied token requires a service key".to_owned())
        })?;
        self.exchange_api_key(api_key).await
    }

    /// Schedule EOF for the next query after its schema frame.
    pub fn fail_next_query_after_schema(&self) {
        self.inner
            .query_stream_fault
            .set_next(QueryStreamFault::EofAfterSchema);
    }

    /// Schedule EOF for the next query after its first batch frame.
    pub fn fail_next_query_after_batch(&self) {
        self.inner
            .query_stream_fault
            .set_next(QueryStreamFault::EofAfterBatch);
    }

    /// Fails cancellation audit gates before owner dispatch until restored.
    pub fn fail_query_cancel_attempt_audit(&self) {
        self.inner.query_control_audit_fault.fail_cancel_attempts();
    }

    /// Restores cancellation audit gates after a deterministic fault.
    pub fn restore_query_cancel_attempt_audit(&self) {
        self.inner
            .query_control_audit_fault
            .restore_cancel_attempts();
    }

    /// Stall the next query after its schema frame using test-tier notifications.
    pub fn stall_next_query_after_schema(&self) {
        let stall = self.inner.query_stream_fault.stall_next_after_schema();
        if let Ok(mut current) = self.inner.query_stream_stall.lock() {
            *current = Some(stall);
        }
    }

    /// Wait until the scheduled query reaches its schema stall.
    ///
    /// # Errors
    ///
    /// Returns a lifecycle error when no stall is scheduled or the server drain
    /// deadline expires before the response reaches its deterministic barrier.
    pub async fn wait_query_schema_stall(&self) -> Result<String, WyrdTestServerError> {
        let stall = self
            .inner
            .query_stream_stall
            .lock()
            .map_err(|_| WyrdTestServerError::Start("query stall lock poisoned".to_owned()))?
            .clone()
            .ok_or_else(|| WyrdTestServerError::Start("query stall is not scheduled".to_owned()))?;
        let deadline = Duration::from_millis(WyrdServerConfig::default().shutdown.drain_ms);
        tokio::time::timeout(deadline, stall.wait_entered())
            .await
            .map_err(|_| {
                WyrdTestServerError::Start("query schema stall deadline elapsed".to_owned())
            })?;
        let probe = stall.resource_probe().map_err(WyrdTestServerError::Start)?;
        Ok(probe.query_id().as_uuid().to_string())
    }

    /// Captures residual state directly from the production Oracle owners.
    ///
    /// # Errors
    /// Returns a start error when this server does not host an Oracle role.
    pub fn oracle_runtime_inspection(
        &self,
    ) -> Result<OracleRuntimeInspection, WyrdTestServerError> {
        let runtime =
            self.inner.state.bifrost_query().ok_or_else(|| {
                WyrdTestServerError::Start("Oracle runtime is not hosted".to_owned())
            })?;
        let (admission, (records, bytes, oldest)) = runtime.oracle_runtime_inspection();
        let (spill_directories, spill_files, spill_file_bytes) = self.inspect_oracle_spill()?;
        Ok(OracleRuntimeInspection {
            active_queries: admission.active_queries,
            queued_queries: admission.queued_queries,
            reserved_memory_bytes: admission.reserved_memory_bytes,
            reserved_spill_bytes: admission.reserved_spill_bytes,
            peer_pending: admission.peer_pending,
            peer_running: admission.peer_running,
            audit_wal_records: records,
            audit_wal_bytes: bytes,
            audit_oldest_age: oldest,
            spill_directories,
            spill_files,
            spill_file_bytes,
        })
    }

    /// Counts Oracle-owned scratch state without exposing local paths.
    ///
    /// # Errors
    ///
    /// Returns a start error when scratch metadata cannot be inspected.
    fn inspect_oracle_spill(&self) -> Result<(u64, u64, u64), WyrdTestServerError> {
        let Some(root) = self.inner._bifrost_spill_root.as_ref() else {
            return Ok((0, 0, 0));
        };
        let oracle_root = root.path().join("oracle-spill");
        if !oracle_root.exists() {
            return Ok((0, 0, 0));
        }
        let mut directories = 0_u64;
        let mut files = 0_u64;
        let mut bytes = 0_u64;
        let mut pending = vec![oracle_root];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory)
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?
            {
                let entry = entry.map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
                let metadata = entry
                    .metadata()
                    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
                if metadata.is_dir() {
                    directories = directories.saturating_add(1);
                    pending.push(entry.path());
                } else if metadata.is_file() {
                    files = files.saturating_add(1);
                    bytes = bytes.saturating_add(metadata.len());
                }
            }
        }
        Ok((directories, files, bytes))
    }

    /// Pauses the production audit relay before its next Postgres attempt.
    pub fn pause_audit_relay_for_test(
        &self,
    ) -> Result<wyrd_server::oracle::AuditRelayPauseGuard, WyrdTestServerError> {
        let runtime =
            self.inner.state.bifrost_query().ok_or_else(|| {
                WyrdTestServerError::Start("Oracle runtime is not hosted".to_owned())
            })?;
        Ok(runtime.pause_audit_relay_for_test())
    }

    /// Injects a commit-before-checkpoint relay crash window.
    pub fn fail_audit_after_commit_for_test(&self) -> Result<(), WyrdTestServerError> {
        let runtime =
            self.inner.state.bifrost_query().ok_or_else(|| {
                WyrdTestServerError::Start("Oracle runtime is not hosted".to_owned())
            })?;
        runtime.fail_audit_after_commit_for_test();
        Ok(())
    }

    /// Captures exact durable and in-memory query resource ownership.
    ///
    /// # Errors
    ///
    /// Returns a setup, SQL, conversion, or tail-registry error when the
    /// production-shaped owners cannot provide an exact snapshot.
    pub fn bifrost_query_resource_snapshot(
        &self,
        query_id: &str,
    ) -> Result<BifrostQueryResourceSnapshot, WyrdTestServerError> {
        let probe = self.query_resource_probe(query_id)?;
        let snapshot = probe.snapshot();
        Ok(BifrostQueryResourceSnapshot {
            admission_slots: snapshot.admission_slots,
            memory_bytes: snapshot.memory_bytes,
            peer_slots: snapshot.peer_slots,
            tail_fences: snapshot.tail_fences,
        })
    }

    /// Waits on lifecycle notifications until every query resource returns to baseline.
    ///
    /// # Errors
    ///
    /// Returns a lifecycle or inspection error when client cancellation does
    /// not drop the body and release all resources before `shutdown.drain_ms`.
    pub async fn wait_bifrost_query_resources_released(
        &self,
        query_id: &str,
        baseline: BifrostQueryResourceSnapshot,
    ) -> Result<BifrostQueryResourceSnapshot, WyrdTestServerError> {
        let probe = self.query_resource_probe(query_id)?;
        let mut releases = probe.subscribe();
        let deadline = tokio::time::Instant::now()
            + Duration::from_millis(WyrdServerConfig::default().shutdown.drain_ms);
        loop {
            let snapshot = self.bifrost_query_resource_snapshot(query_id)?;
            if snapshot == baseline {
                return Ok(snapshot);
            }
            tokio::time::timeout_at(deadline, releases.changed())
                .await
                .map_err(|_| {
                    WyrdTestServerError::Start(format!(
                        "query resource release deadline elapsed: {snapshot:?}"
                    ))
                })?
                .map_err(|_| {
                    WyrdTestServerError::Start("query release notifier closed".to_owned())
                })?;
        }
    }

    /// Resolves the lifecycle observation for one exact Oracle query identity.
    ///
    /// # Errors
    ///
    /// Returns an error when no stalled query is bound or the identity differs.
    fn query_resource_probe(
        &self,
        query_id: &str,
    ) -> Result<Arc<vala_bifrost_redux::oracle::QueryResourceProbe>, WyrdTestServerError> {
        let stall = self
            .inner
            .query_stream_stall
            .lock()
            .map_err(|_| WyrdTestServerError::Start("query stall lock poisoned".to_owned()))?
            .clone()
            .ok_or_else(|| WyrdTestServerError::Start("query stall is not scheduled".to_owned()))?;
        let probe = stall.resource_probe().map_err(WyrdTestServerError::Start)?;
        if probe.query_id().as_uuid().to_string() != query_id {
            return Err(WyrdTestServerError::Start(format!(
                "query resource identity mismatch: requested {query_id}"
            )));
        }
        Ok(probe)
    }

    /// Wake the already-running production Forge scheduler for a test pass.
    ///
    /// The trigger is observation-only plumbing: planning, claims, rewrites,
    /// and catalog commits remain owned by the configured production workers.
    pub fn request_forge_scheduler_pass_for_test(&self) {
        self.inner.forge_scheduler_trigger.request_pass();
    }

    /// Return completed production scheduler passes observed by the test trigger.
    #[must_use]
    pub fn completed_forge_scheduler_passes_for_test(&self) -> usize {
        self.inner.forge_scheduler_trigger.completed_passes()
    }

    /// Wait for the production scheduler to return at least `expected` passes.
    pub async fn wait_for_forge_scheduler_passes_for_test(&self, expected: usize) {
        self.inner
            .forge_scheduler_trigger
            .wait_for_passes_at_least(expected)
            .await;
    }

    /// Count non-terminal Forge tasks for one tenant after Scribe publication.
    ///
    /// This read-only probe tells a matrix caller whether a typed Forge
    /// completion wait is causally required; a Scribe flush alone does not
    /// imply that Forge compaction has been scheduled.
    ///
    /// # Errors
    /// Returns an error when the fixture pool cannot be acquired or the
    /// tenant-scoped Forge task query fails.
    pub async fn bifrost_pending_forge_tasks_for_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<i64, WyrdTestServerError> {
        let pool = self.inner.fixture.superuser_pool().await.map_err(sql)?;
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM vala.forge_tasks WHERE data_tenant_id = $1 AND state IN ('ready', 'claimed', 'running', 'prepared')",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .map_err(sql)
    }

    /// Read every committed hot object one tenant table published, in order.
    ///
    /// Production Scribe tests assert against what actually became durable:
    /// the fenced row identity, the object it names, and the typed promotion
    /// record committed with it. Reading them here — rather than through a
    /// Scribe-internal accessor — is what makes the assertion a statement about
    /// published state instead of writer intent.
    ///
    /// # Errors
    ///
    /// Returns an error when the fixture pool cannot be acquired, the query
    /// fails, a recorded size or row count is negative, or a stored promotion
    /// record does not decode as the one shipped version.
    pub async fn published_hot_files_for_test(
        &self,
        tenant: DataTenantId,
        namespace: &str,
        table_name: &str,
    ) -> Result<Vec<PublishedHotFileInspection>, WyrdTestServerError> {
        let pool = self.inner.fixture.superuser_pool().await.map_err(sql)?;
        let rows: Vec<PublishedHotFileRow> = sqlx::query_as(
            "SELECT id,file_path,file_size,row_count,file_ordinal,file_checksum,promotion_record,\
             wal_lsn_min,wal_lsn_max \
             FROM vala.file_list \
             WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
             ORDER BY wal_lsn_min, file_ordinal",
        )
        .bind(tenant.as_uuid())
        .bind(namespace)
        .bind(table_name)
        .fetch_all(&pool)
        .await
        .map_err(sql)?;
        rows.into_iter()
            .map(|row| {
                Ok(PublishedHotFileInspection {
                    id: row.id,
                    object_key: row.file_path,
                    file_size: u64::try_from(row.file_size).map_err(|_| {
                        WyrdTestServerError::Start(
                            "published hot file has a negative size".to_owned(),
                        )
                    })?,
                    row_count: u64::try_from(row.row_count).map_err(|_| {
                        WyrdTestServerError::Start(
                            "published hot file has a negative row count".to_owned(),
                        )
                    })?,
                    file_ordinal: row.file_ordinal,
                    file_checksum: row.file_checksum.unwrap_or_default(),
                    promotion_record:
                        vala_bifrost_redux::scribe::promotion::ScribePublishedHotFileV1::from_json(
                            &row.promotion_record,
                        )
                        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
                    wal_lsn_min: u64::try_from(row.wal_lsn_min).map_err(|_| {
                        WyrdTestServerError::Start(
                            "published hot file has a negative WAL lower bound".to_owned(),
                        )
                    })?,
                    wal_lsn_max: u64::try_from(row.wal_lsn_max).map_err(|_| {
                        WyrdTestServerError::Start(
                            "published hot file has a negative WAL upper bound".to_owned(),
                        )
                    })?,
                })
            })
            .collect()
    }

    /// Count Bifrost read-decision audit rows for the fixture tenant.
    ///
    /// Agent-surface denial journeys use this test-only probe to prove Gate
    /// rejection occurs before Oracle planning or durable read accounting.
    ///
    /// # Errors
    /// Returns an error when the fixture's superuser pool cannot be acquired
    /// or the tenant-scoped audit query fails.
    pub async fn bifrost_read_decision_count(&self) -> Result<i64, WyrdTestServerError> {
        self.bifrost_read_decision_count_for_tenant(self.data_tenant_id())
            .await
    }

    /// Count tenant-bound Bifrost read-decision audit rows.
    ///
    /// # Errors
    /// Returns an error when the fixture's superuser pool cannot be acquired
    /// or the tenant-scoped audit query fails.
    pub async fn bifrost_read_decision_count_for_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<i64, WyrdTestServerError> {
        let pool = self.inner.fixture.superuser_pool().await.map_err(sql)?;
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'bifrost.query.read_decision'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .map_err(sql)?;
        Ok(count)
    }

    /// Count the exact tenant-bound read-decision audit row for one request ID.
    ///
    /// # Errors
    /// Returns an error when the fixture's superuser pool cannot be acquired
    /// or the request-scoped audit query fails.
    pub async fn bifrost_read_decision_for_request(
        &self,
        tenant: DataTenantId,
        request_id: &str,
    ) -> Result<i64, WyrdTestServerError> {
        let pool = self.inner.fixture.superuser_pool().await.map_err(sql)?;
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND request_id = $2 AND operation = 'bifrost.query.read_decision'",
        )
        .bind(tenant.as_uuid())
        .bind(request_id)
        .fetch_one(&pool)
        .await
        .map_err(sql)
    }

    /// Return the newest tenant-bound Bifrost read-decision request ID.
    ///
    /// Callers bracket one serialized public query with the tenant count, then
    /// use this read-only probe to join that exact newly committed audit row.
    ///
    /// # Errors
    /// Returns an error when the fixture's superuser pool cannot be acquired
    /// or the tenant-scoped audit query fails.
    pub async fn latest_bifrost_read_decision_request_id(
        &self,
        tenant: DataTenantId,
    ) -> Result<Option<String>, WyrdTestServerError> {
        let pool = self.inner.fixture.superuser_pool().await.map_err(sql)?;
        sqlx::query_scalar::<_, String>(
            "SELECT request_id FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'bifrost.query.read_decision' ORDER BY seq DESC LIMIT 1",
        )
        .bind(tenant.as_uuid())
        .fetch_optional(&pool)
        .await
        .map_err(sql)
    }

    /// Return every claim this pod's Scribe has published, in commit order.
    ///
    /// Each entry names the objects one committed claim produced and the
    /// distinct shard lanes its contributing members were frozen on. Neither
    /// the `file_list` row nor its promotion record carries that binding, so
    /// this is how a harness proves a published object is the product of a real
    /// cross-shard merge rather than inferring it from object counts.
    ///
    /// # Errors
    ///
    /// Returns an error when this server owns no Scribe.
    pub fn published_scribe_claims_for_test(
        &self,
    ) -> Result<
        Vec<vala_bifrost_redux::scribe::staging_runtime::PublishedClaimObservation>,
        WyrdTestServerError,
    > {
        self.bifrost_scribe()
            .map(|scribe| scribe.published_claims_for_test())
            .ok_or_else(|| WyrdTestServerError::Start("server owns no Scribe".to_owned()))
    }

    /// Report the pod's closed contention registry totals.
    ///
    /// Fairness is a scheduling decision, not a row: the only truthful evidence
    /// that a vector was lent, released, or queued is the production registry
    /// that recorded the decision, so a contention case reads these totals
    /// instead of inferring capacity from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when this server owns no Scribe.
    pub fn scribe_contention_totals_for_test(
        &self,
    ) -> Result<vala_bifrost_redux::scribe::telemetry::ScribeTelemetrySnapshot, WyrdTestServerError>
    {
        self.bifrost_scribe()
            .map(|scribe| scribe.contention_totals_for_test())
            .ok_or_else(|| WyrdTestServerError::Start("server owns no Scribe".to_owned()))
    }

    /// Report the pod's closed staged-member and claim registry totals.
    ///
    /// Durability is a lifecycle transition, not a file: the only truthful
    /// evidence that a member became durable, that a claim opened, or that a
    /// published member's runs were retired is the production registry that
    /// recorded it. A reconciliation case reads these totals and checks them
    /// against the published objects and rows the same run can observe.
    ///
    /// # Errors
    ///
    /// Returns an error when this server owns no Scribe.
    pub fn scribe_staging_totals_for_test(
        &self,
    ) -> Result<vala_bifrost_redux::scribe::telemetry::ScribeStagingSnapshot, WyrdTestServerError>
    {
        self.bifrost_scribe()
            .map(|scribe| scribe.staging_totals_for_test())
            .ok_or_else(|| WyrdTestServerError::Start("server owns no Scribe".to_owned()))
    }

    /// Report how many complete lifecycle vectors this pod's capacity completes.
    ///
    /// # Errors
    ///
    /// Returns an error when this server owns no Scribe.
    pub fn scribe_ownership_ceiling_for_test(&self) -> Result<usize, WyrdTestServerError> {
        self.bifrost_scribe()
            .map(|scribe| scribe.ownership_ceiling_for_test())
            .ok_or_else(|| WyrdTestServerError::Start("server owns no Scribe".to_owned()))
    }

    /// Count the durable Scribe publication transitions one table has recorded.
    ///
    /// A file-list commit emits exactly one `bifrost.scribe.visibility.publish`
    /// row per published generation, and a reconciling retry that inserts no
    /// new artifact rows emits none. Counting them is therefore how a caller
    /// distinguishes "the retry reconciled the identical publication" from
    /// "the retry published a second time under a new identity".
    ///
    /// # Errors
    ///
    /// Returns an error when the fixture's superuser pool cannot be acquired or
    /// the tenant-scoped audit query fails.
    pub async fn scribe_publication_audit_count_for_test(
        &self,
        tenant: DataTenantId,
        resource: &str,
    ) -> Result<i64, WyrdTestServerError> {
        let pool = self.inner.fixture.superuser_pool().await.map_err(sql)?;
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 AND resource = $2 \
               AND operation = 'bifrost.scribe.visibility.publish'",
        )
        .bind(tenant.as_uuid())
        .bind(resource)
        .fetch_one(&pool)
        .await
        .map_err(sql)
    }

    /// Trip the server-owned WAL breaker for a deterministic benchmark probe.
    pub fn trip_bifrost_wal_disk_full_for_test(&self) -> Result<(), WyrdTestServerError> {
        self.inner
            .state
            .trip_scribe_wal_disk_full_for_test()
            .map_err(WyrdTestServerError::Start)
    }

    /// Return the server-owned Scribe snapshot used by benchmark inspection.
    pub fn scribe_inspection_snapshot(
        &self,
    ) -> Result<vala_bifrost_redux::scribe::telemetry::ScribeInspectionSnapshot, WyrdTestServerError>
    {
        self.inner
            .state
            .scribe_inspection_snapshot_for_test()
            .map_err(WyrdTestServerError::Start)
    }

    /// Return the exact live-tail fences retained by this server's Scribe.
    ///
    /// # Errors
    /// Returns an error when the server has no Scribe or the production fence
    /// registry cannot be inspected.
    pub fn active_bifrost_tail_fences(&self) -> Result<u64, WyrdTestServerError> {
        self.inner
            .state
            .active_scribe_tail_fences_for_test()
            .map_err(WyrdTestServerError::Start)
    }

    /// Return the base URL when bound to a real socket.
    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        match &self.mode {
            Mode::Bound { base_url, .. } => Some(base_url.as_str()),
            Mode::InProcess => None,
        }
    }

    /// Return the bound socket address when in bound mode.
    #[must_use]
    pub fn bound_addr(&self) -> Option<std::net::SocketAddr> {
        match &self.mode {
            Mode::Bound { addr, .. } => Some(*addr),
            Mode::InProcess => None,
        }
    }

    /// Return the gRPC endpoint URL when bound to a real socket.
    #[must_use]
    pub fn grpc_url(&self) -> Option<String> {
        match &self.mode {
            Mode::Bound { grpc_addr, .. } => Some(if self.requested_oracle_peer_tls.is_some() {
                format!("https://localhost:{}", grpc_addr.port())
            } else {
                format!("http://{grpc_addr}")
            }),
            Mode::InProcess => None,
        }
    }

    /// Return the placeholder API key (full bootstrap is complex).
    #[must_use]
    pub fn api_key(&self) -> &SecretString {
        &self.inner.api_key
    }

    /// Return the server state for focused assertions.
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.inner.state
    }

    /// Reclaim expired Forge attempts through a production worker instance.
    ///
    /// # Errors
    ///
    /// Returns an error when Forge is absent, the worker cannot be built, or
    /// production SQL/scratch reclamation fails.
    pub async fn reclaim_expired_forge_attempts_for_test(
        &self,
        cap: u32,
    ) -> Result<Vec<(Uuid, Uuid)>, WyrdTestServerError> {
        let forge = self
            .inner
            .state
            .forge_coordinator()
            .ok_or_else(|| WyrdTestServerError::Start("Forge is not composed".to_owned()))?;
        ForgeWorker::new(
            Arc::clone(forge),
            ForgeWorkerConfig::default(),
            Uuid::now_v7(),
        )
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?
        .reclaim_expired_attempts_for_test(cap)
        .await
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Injects a private Scribe discovery outage for one test-tier journey.
    pub fn set_tail_discovery_unavailable_for_test(&self, unavailable: bool) {
        if let Some(query) = self.inner.state.bifrost_query() {
            query
                .oracle()
                .set_tail_discovery_unavailable_for_test(unavailable);
        }
    }

    /// Cancels the one composed process token and observes both selected role owners.
    ///
    /// # Errors
    /// Returns a start error when the test pod does not select both Scribe and Oracle.
    pub fn cancel_and_observe_shared_bifrost_shutdown_for_test(
        &self,
    ) -> Result<(bool, bool), WyrdTestServerError> {
        let scribe = self
            .inner
            .state
            .bifrost
            .scribe()
            .ok_or_else(|| WyrdTestServerError::Start("Scribe is not composed".to_owned()))?;
        let oracle = self
            .inner
            .state
            .bifrost
            .oracle()
            .ok_or_else(|| WyrdTestServerError::Start("Oracle is not composed".to_owned()))?;
        self.inner.state.shutdown_token.cancel();
        Ok((
            scribe.process_shutdown_observed_for_test(),
            oracle.process_shutdown_observed_for_test(),
        ))
    }

    /// Return the production Forge process role selected for this server.
    #[must_use]
    pub const fn forge_process_role(&self) -> BifrostTarget {
        self.inner.forge_process_role
    }

    /// Wake the production Forge scheduler supervisor for one pass.
    pub fn trigger_forge_scheduler_for_test(&self) {
        self.inner.forge_scheduler_trigger.request_pass();
    }

    /// Return the scheduler trigger retained by this server.
    #[must_use]
    pub fn forge_scheduler_trigger_for_test(&self) -> ForgeSchedulerTrigger {
        self.inner.forge_scheduler_trigger.clone()
    }

    /// Return the manual Forge clock control shared by derived fixtures.
    #[must_use]
    pub fn forge_clock(&self) -> ForgeClockControl {
        self.inner.forge_clock.clone()
    }

    /// Return passive controls for the object store used by the supervised Forge owner.
    #[must_use]
    pub fn forge_object_store_control_for_test(&self) -> Option<Arc<ForgeObjectStoreControl>> {
        self.inner.forge_object_store.as_ref().map(Arc::clone)
    }

    /// Classifies one object through this node's production orphan predicate.
    ///
    /// The verdict is produced by the retained production protection loader and
    /// its single eligibility truth, not by a harness reimplementation, so a
    /// journey can observe what a collection pass on this node would decide
    /// about a named object without driving a destructive pass to find out.
    /// That matters while another process holds a catalog commit open: the
    /// node executing the rewrite cannot answer, and this one can.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdTestServerError::Start`] when this node composes no Forge
    /// coordinator, and the production catalog, SQL, object-metadata, or
    /// path-validation failure when the proof cannot be assembled.
    pub async fn forge_gc_eligibility_for_test(
        &self,
        binding: &TenantTableBinding,
        path: &str,
    ) -> Result<String, WyrdTestServerError> {
        self.inner
            .state
            .forge()
            .and_then(|forge| forge.coordinator())
            .ok_or_else(|| {
                WyrdTestServerError::Start("Forge coordinator is not composed".to_owned())
            })?
            .gc_eligibility_for_test(binding, path)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Return deterministic expiration gates from the supervised Forge owner.
    #[must_use]
    pub fn forge_expiry_controls_for_test(
        &self,
    ) -> Option<vala_bifrost_redux::forge::ExpiryTestControls> {
        self.inner
            .state
            .forge()
            .and_then(|forge| forge.coordinator())
            .map(|forge| forge.expiry_controls_for_test())
    }

    /// Arm the canonical one-shot Prepared audit failure on the composed Forge owner.
    ///
    /// # Errors
    ///
    /// Returns an error when this server has no Forge coordinator.
    pub fn fail_next_forge_prepared_audit_for_test(&self) -> Result<(), WyrdTestServerError> {
        let forge = self
            .inner
            .state
            .forge()
            .and_then(|forge| forge.coordinator())
            .ok_or_else(|| {
                WyrdTestServerError::Start("Forge coordinator is not composed".to_owned())
            })?;
        forge.fail_next_prepared_live_audit_for_test();
        Ok(())
    }

    /// Arm one failure after maintenance commits Prepared task evidence.
    ///
    /// # Errors
    ///
    /// Returns an error when this server has no Forge worker.
    pub fn fail_after_forge_maintenance_prepared_for_test(
        &self,
    ) -> Result<(), WyrdTestServerError> {
        let worker = self
            .inner
            .state
            .forge()
            .and_then(|forge| forge.worker())
            .ok_or_else(|| WyrdTestServerError::Start("Forge worker is not composed".to_owned()))?;
        worker.fail_after_maintenance_prepared_for_test();
        Ok(())
    }

    /// Return the stable node identity assigned by the cluster harness.
    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.inner.node_id
    }

    /// Create one tenant-qualified table through the retained production catalog.
    ///
    /// # Errors
    ///
    /// Returns the catalog error when validation, creation, or durable metadata
    /// publication fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops waiting for catalog completion. The production
    /// catalog remains authoritative for whether the idempotent table creation
    /// committed before cancellation.
    pub async fn create_bifrost_table_for_test(
        &self,
        request: vala_bifrost_redux::catalog::CreateTableRequest,
    ) -> Result<(), WyrdTestServerError> {
        self.inner
            .bifrost_catalog
            .create_table(request)
            .await
            .map(|_| ())
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Provision the canonical traces/spans table for one test tenant.
    ///
    /// # Errors
    /// Returns an error when the built-in definition or catalog operation is
    /// unavailable.
    pub async fn ensure_traces_spans_table_for_test(
        &self,
        tenant: DataTenantId,
    ) -> Result<(), WyrdTestServerError> {
        let definition =
            vala_bifrost_redux::tables::builtin_table("traces", "spans").ok_or_else(|| {
                WyrdTestServerError::Start("missing traces spans built-in".to_owned())
            })?;
        self.inner
            .bifrost_catalog
            .ensure_builtin(tenant, definition)
            .await
            .map(|_| ())
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Return the publisher paired with this server's Forge inbox.
    #[must_use]
    pub fn forge_publisher(&self) -> StagingFilePublisher {
        self.inner.forge_publisher.clone()
    }

    /// Inspect durable Forge demand and task state without requiring an Iceberg snapshot.
    ///
    /// # Errors
    ///
    /// Returns fixture-pool, SQL, or negative-count invariant errors.
    pub async fn inspect_forge_workflow_for_test(
        &self,
        tenant: DataTenantId,
        table: &str,
    ) -> Result<ForgeWorkflowInspection, WyrdTestServerError> {
        self.inspect_forge_workflow_ref_for_test(
            tenant,
            &TableRef::new(BifrostNamespace::Bifrost, table),
        )
        .await
    }

    /// Inspect durable Forge demand and task state for one namespace-qualified
    /// table without requiring an Iceberg snapshot.
    ///
    /// This is the authoritative body; the `&str` form above is the
    /// `vala.bifrost` shorthand every built-in journey uses.
    ///
    /// # Errors
    ///
    /// Returns fixture-pool, SQL, or negative-count invariant errors.
    pub async fn inspect_forge_workflow_ref_for_test(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
    ) -> Result<ForgeWorkflowInspection, WyrdTestServerError> {
        let namespace = table.namespace.as_str();
        let table = table.name.as_str();
        let pool = self.inner.fixture.superuser_pool().await.map_err(sql)?;
        let has_demand = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM vala.forge_planning_demands WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3)",
        )
        .bind(tenant.as_uuid())
        .bind(namespace)
        .bind(table)
        .fetch_one(&pool)
        .await
        .map_err(sql)?;
        let tasks = sqlx::query_as::<_, (String, String)>(
            "SELECT strategy,state FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3 ORDER BY created_at,task_id",
        )
        .bind(tenant.as_uuid())
        .bind(namespace)
        .bind(table)
        .fetch_all(&pool)
        .await
        .map_err(sql)?;
        let (active_claims, active_attempts) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT count(*) FILTER (WHERE claimed_by IS NOT NULL AND state IN ('claimed','running','prepared')),count(DISTINCT attempt_id) FILTER (WHERE attempt_id IS NOT NULL AND state IN ('claimed','running','prepared')) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND catalog_name='wyrd-redux' AND namespace_name=$2 AND table_name=$3",
        )
        .bind(tenant.as_uuid())
        .bind(namespace)
        .bind(table)
        .fetch_one(&pool)
        .await
        .map_err(sql)?;
        let uncompacted_staging_files = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND namespace=$2 AND table_name=$3 AND NOT compacted",
        )
        .bind(tenant.as_uuid())
        .bind(namespace)
        .bind(table)
        .fetch_one(&pool)
        .await
        .map_err(sql)?;
        Ok(ForgeWorkflowInspection {
            has_demand,
            tasks,
            active_claims: u64::try_from(active_claims).map_err(|_| {
                WyrdTestServerError::Start("negative Forge active claim count".to_owned())
            })?,
            active_attempts: u64::try_from(active_attempts).map_err(|_| {
                WyrdTestServerError::Start("negative Forge active attempt count".to_owned())
            })?,
            uncompacted_staging_files: u64::try_from(uncompacted_staging_files).map_err(|_| {
                WyrdTestServerError::Start("negative uncompacted staging file count".to_owned())
            })?,
        })
    }

    /// Return the Bifrost catalog this server's production surfaces read through.
    ///
    /// Exposing the owner rather than a projection is deliberate: a visibility
    /// proof has to call the same `pin_sealed_table` an Oracle read calls, so
    /// what it observes is the production cut and not a test reconstruction of
    /// one.
    #[must_use]
    pub fn bifrost_catalog(&self) -> Arc<BifrostCatalog> {
        Arc::clone(&self.inner.bifrost_catalog)
    }

    /// Return the Scribe retained by this server's production ingest runtime.
    #[must_use]
    pub fn bifrost_scribe(&self) -> Option<Arc<ScribeImpl>> {
        self.inner.state.bifrost_scribe_for_test().cloned()
    }

    /// Install a deterministic barrier at the public Bifrost write seam.
    ///
    /// # Errors
    /// Returns an error when this server was started without a Bifrost Scribe.
    pub fn stall_next_bifrost_write(
        &self,
    ) -> Result<Arc<vala_bifrost_redux::scribe::IngestStall>, WyrdTestServerError> {
        self.bifrost_scribe()
            .map(|scribe| scribe.stall_next_ingest_for_test())
            .ok_or_else(|| WyrdTestServerError::Start("server has no Bifrost Scribe".to_owned()))
    }

    /// Return the fixture tenant id.
    #[must_use]
    pub fn data_tenant_id(&self) -> DataTenantId {
        self.inner.fixture.data_tenant_id()
    }

    /// Return a clone of the app Postgres pool for direct SQL in tests.
    #[must_use]
    pub fn app_pool(&self) -> sqlx::PgPool {
        self.inner.fixture.app_pool().clone()
    }

    /// Return the deterministic public verifying key.
    #[must_use]
    pub fn verifying_key(&self) -> &VerifyingKey {
        crate::keys::verifying_key()
    }

    /// Return a virtual-clock handle.
    #[must_use]
    pub fn clock(&self) -> ClockHandle {
        ClockHandle::new()
    }

    /// Advance the virtual clock.
    pub async fn advance(&self, duration: Duration) {
        self.clock().advance(duration).await;
    }

    /// Route a request through the in-process router.
    ///
    /// # Errors
    /// Returns an error if the router fails.
    pub async fn oneshot(&self, req: Request<Body>) -> Result<Response<Body>, WyrdTestServerError> {
        self.raw_call(req).await
    }

    /// Route a request through the in-process router with an access token header.
    ///
    /// # Errors
    /// Returns an error if the request cannot be augmented or the router fails.
    pub async fn oneshot_authenticated(
        &self,
        jwt: &str,
        mut req: Request<Body>,
    ) -> Result<Response<Body>, WyrdTestServerError> {
        req.headers_mut().insert(
            "x-wyrd-access-token",
            HeaderValue::from_str(&format!("Bearer {jwt}"))
                .map_err(|error| WyrdTestServerError::Io(error.to_string()))?,
        );
        self.raw_call(req).await
    }

    /// Bootstrap a user principal through fixture SQL.
    ///
    /// # Errors
    /// Returns an error when SQL writes or test JWT minting fail.
    pub async fn bootstrap_user(
        &self,
        name: &str,
        roles: &[&str],
    ) -> Result<Bootstrap, WyrdTestServerError> {
        let user_id = Uuid::now_v7();
        let mut conn = self.tenant_conn().await?;
        insert_user(
            &mut conn,
            user_id,
            Some(&format!("{name}@test.wyrd")),
            "password",
            None,
        )
        .await
        .map_err(sql)?;
        for role in roles {
            grant_role(&mut conn, user_id, PrincipalTable::User, role).await?;
        }
        conn.commit().await.map_err(sql)?;

        let principal = TokenPrincipalRef {
            id: PrincipalId::new(user_id),
            kind: PrincipalKindTag::User,
            tenant_id: self.data_tenant_id(),
            card_ref: None,
            card_ref_scope: Default::default(),
        };
        let jwt = self
            .inner
            .issuing_key
            .issue_user_access_token(principal, role_refs(roles)?, chrono::Duration::minutes(15))
            .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?;
        Ok(Bootstrap::User {
            id: PrincipalId::new(user_id),
            jwt,
        })
    }

    /// Bootstrap a Service principal through fixture SQL.
    ///
    /// # Errors
    /// Returns an error when SQL writes or API-key hashing fail.
    pub async fn bootstrap_service(
        &self,
        name: &str,
        roles: &[&str],
    ) -> Result<Bootstrap, WyrdTestServerError> {
        self.bootstrap_machine(name, roles, CardKind::Service, "service")
            .await
    }

    /// Bootstrap a Service principal under an explicit tenant.
    ///
    /// The tenant-scoped analogue of [`Self::bootstrap_service`]: the service
    /// account and its API key are written through the supplied tenant's
    /// [`TenantConn`], and the generated key carries that tenant in its prefix
    /// so [`Self::exchange_api_key`] resolves the same tenant. Multi-tenant
    /// tests use this to mint a per-tenant admin that authors through the CLI.
    ///
    /// # Errors
    /// Returns an error when SQL writes or API-key hashing fail.
    pub async fn bootstrap_service_in_tenant(
        &self,
        tenant_id: DataTenantId,
        name: &str,
        roles: &[&str],
    ) -> Result<Bootstrap, WyrdTestServerError> {
        self.bootstrap_machine_in_tenant(tenant_id, name, roles, CardKind::Service, "service")
            .await
    }

    /// Provision a second active tenant: seed its row and built-in roles.
    ///
    /// The fixture seeds one tenant at boot; the same-issuer-two-tenant
    /// isolation test calls this to stand up tenant B so a subject bound only in
    /// tenant A fails closed in B. Returns the new tenant's isolation key.
    ///
    /// # Errors
    /// Returns an error when the tenant insert or role seed fails.
    pub async fn seed_tenant(&self, slug: &str) -> Result<DataTenantId, WyrdTestServerError> {
        let tenant_id = self
            .inner
            .fixture
            .seed_additional_tenant(slug)
            .await
            .map_err(sql)?;
        let mut conn = self.tenant_conn_for(tenant_id).await?;
        seed_builtin_roles_for_tenant(&mut conn, tenant_id)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        conn.commit().await.map_err(sql)?;
        Ok(tenant_id)
    }

    /// Return the raw `client_secret_enc` ciphertext for a trusted issuer.
    ///
    /// Opens a [`TenantConn`] on the supplied tenant and reads the stored
    /// column byte-for-byte (never text-decoded), so the journey can assert the
    /// sealing key wrote ciphertext at rest rather than plaintext. Returns
    /// `None` when no issuer with that URL exists for the tenant, and `None`
    /// when the issuer stores no secret (public client).
    ///
    /// # Errors
    /// Returns an error when the query fails.
    pub async fn trusted_issuer_secret_ciphertext(
        &self,
        tenant_id: DataTenantId,
        issuer_url: &str,
    ) -> Result<Option<Vec<u8>>, WyrdTestServerError> {
        let mut conn = self.tenant_conn_for(tenant_id).await?;
        let row = trusted_issuer_by_url(&mut conn, issuer_url)
            .await
            .map_err(sql)?;
        conn.commit().await.map_err(sql)?;
        Ok(row.and_then(|row| row.client_secret_enc))
    }

    /// Report whether a workload binding is visible under a tenant's RLS scope.
    ///
    /// Opens a [`TenantConn`] bound to `tenant_id` and runs the production
    /// `workload_binding_by_subject` lookup. The same-issuer-two-tenant
    /// isolation test calls this once per tenant to prove storage-level RLS: a
    /// binding authored only in tenant A is `true` under A's connection and
    /// `false` under B's, with no shared filter — two genuinely independent
    /// tenant-scoped reads.
    ///
    /// # Errors
    /// Returns an error when the query fails.
    pub async fn workload_binding_exists(
        &self,
        tenant_id: DataTenantId,
        issuer: &str,
        subject: &str,
        audience: Option<&str>,
    ) -> Result<bool, WyrdTestServerError> {
        let mut conn = self.tenant_conn_for(tenant_id).await?;
        let row = workload_binding_by_subject(&mut conn, issuer, subject, audience)
            .await
            .map_err(sql)?;
        conn.commit().await.map_err(sql)?;
        Ok(row.is_some())
    }

    /// Bootstrap an Agent principal through fixture SQL.
    ///
    /// # Errors
    /// Returns an error when SQL writes or API-key hashing fail.
    pub async fn bootstrap_agent(
        &self,
        name: &str,
        roles: &[&str],
    ) -> Result<Bootstrap, WyrdTestServerError> {
        self.bootstrap_machine(name, roles, CardKind::Agent, "agent")
            .await
    }

    /// Grant a role to a bootstrapped principal.
    ///
    /// # Errors
    /// Returns an error when the role is unknown or SQL fails.
    pub async fn grant_role(
        &self,
        principal: &Bootstrap,
        role: &str,
    ) -> Result<(), WyrdTestServerError> {
        let mut conn = self.tenant_conn().await?;
        match principal {
            Bootstrap::User { id, .. } => {
                grant_role(&mut conn, id.as_uuid(), PrincipalTable::User, role).await?;
            }
            Bootstrap::Machine { id, .. } => {
                grant_role(
                    &mut conn,
                    id.as_uuid(),
                    PrincipalTable::ServiceAccount,
                    role,
                )
                .await?;
            }
        }
        conn.commit().await.map_err(sql)
    }

    /// Revoke a role from a bootstrapped principal.
    ///
    /// # Errors
    /// Returns an error when the role is unknown or SQL fails.
    pub async fn revoke_role(
        &self,
        principal: &Bootstrap,
        role: &str,
    ) -> Result<(), WyrdTestServerError> {
        let mut conn = self.tenant_conn().await?;
        let role_id = lookup_role_id(&mut conn, role).await?;
        match principal {
            Bootstrap::User { id, .. } => {
                revoke_role_from_user(&mut conn, id.as_uuid(), role_id)
                    .await
                    .map_err(sql)?;
            }
            Bootstrap::Machine { id, .. } => {
                revoke_role_from_service_account(&mut conn, id.as_uuid(), role_id)
                    .await
                    .map_err(sql)?;
            }
        }
        conn.commit().await.map_err(sql)
    }

    /// Force the verifier to re-read permissions for this exact JWT.
    pub async fn force_recheck(&self, jwt: &str) {
        self.inner
            .verifier
            .invalidate(&SecretString::from(jwt.to_owned()))
            .await;
    }

    /// Force the verifier to re-read permissions for this principal.
    pub async fn force_recheck_principal(&self, principal: &Bootstrap) {
        self.inner
            .verifier
            .invalidate_principal(principal.id())
            .await;
    }

    /// Exchange an API key through the real `/auth/token` route.
    ///
    /// # Errors
    /// Returns an error when the HTTP route rejects the key or response parsing fails.
    pub async fn exchange_api_key(
        &self,
        key: &SecretString,
    ) -> Result<String, WyrdTestServerError> {
        let body = serde_json::to_vec(&TokenRequest::WyrdApiKey {
            api_key: SecretBearer::new(key.expose_secret().to_owned()),
        })
        .map_err(|error| WyrdTestServerError::Io(error.to_string()))?;
        let response = self
            .raw_call(
                Request::builder()
                    .method("POST")
                    .uri("/auth/token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .map_err(|error| WyrdTestServerError::Io(error.to_string()))?,
            )
            .await?;
        let token = parse_success::<TokenResponse>(response).await?;
        Ok(token.access_token.expose().to_owned())
    }

    /// Delegate a JWT to a target Service or Agent through `/auth/token`.
    ///
    /// # Errors
    /// Returns an error when delegation is unavailable, denied, or response parsing fails.
    pub async fn delegate(
        &self,
        from_jwt: &str,
        target: &CardRef,
    ) -> Result<String, WyrdTestServerError> {
        let body = serde_json::to_vec(&TokenRequest::TokenExchange {
            subject_token: SecretBearer::new(from_jwt.to_owned()),
            subject_token_type: SubjectTokenType::AccessToken,
            requested_subject: RequestedSubject::CardRef {
                card_ref: target.clone(),
            },
        })
        .map_err(|error| WyrdTestServerError::Io(error.to_string()))?;
        let response = self
            .raw_call(
                Request::builder()
                    .method("POST")
                    .uri("/auth/token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .map_err(|error| WyrdTestServerError::Io(error.to_string()))?,
            )
            .await?;
        let token = parse_success::<TokenResponse>(response).await?;
        Ok(token.access_token.expose().to_owned())
    }

    /// Call `/v1/authz/check` using a delegated token and projected request headers.
    ///
    /// # Errors
    /// Returns an error if request construction or routing fails.
    pub async fn authz_check(
        &self,
        jwt: &str,
        request: AuthzCheckRequest,
    ) -> Result<CheckResult, WyrdTestServerError> {
        let response = self
            .raw_call(
                Request::builder()
                    .method("POST")
                    .uri("/v1/authz/check")
                    .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&request).map_err(
                        |error| WyrdTestServerError::Io(error.to_string()),
                    )?))
                    .map_err(|error| WyrdTestServerError::Io(error.to_string()))?,
            )
            .await?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("wyrd-request-id")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| WyrdTestServerError::Io("missing wyrd-request-id header".to_owned()))
            .and_then(|value| {
                RequestId::parse(value).map_err(|error| WyrdTestServerError::Io(error.to_string()))
            })?;
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|error| WyrdTestServerError::Io(error.to_string()))?;
        let check_response: Option<AuthzCheckResponse> = serde_json::from_slice(&body).ok();
        Ok(CheckResult {
            status,
            wyrd_request_id: request_id,
            response: check_response,
        })
    }

    /// Seed a Service/Agent principal whose `card_ref` matches `card_ref` exactly.
    ///
    /// Workload bindings resolve to a server-owned `CardRef` with `uid = None`
    /// (boot's `build_workload_bindings`), and the jwt-bearer exchange looks the
    /// principal up by exact JSONB `card_ref` equality. The role-bearing
    /// `bootstrap_service` helper mints a random `uid`, so it can never match a
    /// binding. This seeds a principal under the binding's exact `card_ref` so a
    /// config-driven workload journey resolves to a real account.
    ///
    /// # Errors
    /// Returns an error when the card kind is not Service/Agent, or SQL fails.
    pub async fn seed_card_principal(
        &self,
        card_ref: &CardRef,
        roles: &[&str],
    ) -> Result<PrincipalId, WyrdTestServerError> {
        self.seed_card_principal_in_tenant(self.data_tenant_id(), card_ref, roles)
            .await
    }

    /// Seed a Service/Agent principal under an explicit tenant.
    ///
    /// The tenant-scoped analogue of [`Self::seed_card_principal`]: the same
    /// exact-`card_ref` seed, written through the supplied tenant's
    /// [`TenantConn`]. Multi-tenant isolation tests use this to seed a bound
    /// workload only in tenant A.
    ///
    /// # Errors
    /// Returns an error when the card kind is not Service/Agent, or SQL fails.
    pub async fn seed_card_principal_in_tenant(
        &self,
        tenant_id: DataTenantId,
        card_ref: &CardRef,
        roles: &[&str],
    ) -> Result<PrincipalId, WyrdTestServerError> {
        let principal_kind = match &card_ref.kind {
            CardKind::Service => "service",
            CardKind::Agent => "agent",
            other => {
                return Err(WyrdTestServerError::Unsupported(format!(
                    "seed_card_principal requires a Service or Agent card_ref, got {other:?}"
                )));
            }
        };
        let principal_id = Uuid::now_v7();
        let creator_id = self.ensure_fixture_admin_for(tenant_id).await?;
        let mut conn = self.tenant_conn_for(tenant_id).await?;
        insert_service_account(
            &mut conn,
            principal_id,
            principal_kind,
            card_ref,
            card_ref.name.as_str(),
            None,
            creator_id,
        )
        .await
        .map_err(sql)?;
        for role in roles {
            grant_role(
                &mut conn,
                principal_id,
                PrincipalTable::ServiceAccount,
                role,
            )
            .await?;
        }
        conn.commit().await.map_err(sql)?;
        Ok(PrincipalId::new(principal_id))
    }

    async fn bootstrap_machine(
        &self,
        name: &str,
        roles: &[&str],
        card_kind: CardKind,
        principal_kind: &'static str,
    ) -> Result<Bootstrap, WyrdTestServerError> {
        self.bootstrap_machine_in_tenant(
            self.data_tenant_id(),
            name,
            roles,
            card_kind,
            principal_kind,
        )
        .await
    }

    async fn bootstrap_machine_in_tenant(
        &self,
        tenant_id: DataTenantId,
        name: &str,
        roles: &[&str],
        card_kind: CardKind,
        principal_kind: &'static str,
    ) -> Result<Bootstrap, WyrdTestServerError> {
        let principal_id = Uuid::now_v7();
        let creator_id = self.ensure_fixture_admin_for(tenant_id).await?;
        let card_ref = card_ref(card_kind, name)?;
        let api_key = WyrdApiKey::generate(tenant_id);
        let raw = api_key.secret.clone();
        let key_hash = tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw))
            .await
            .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?
            .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?;

        let mut conn = self.tenant_conn_for(tenant_id).await?;
        seed_machine_card(&mut conn, &card_ref, creator_id).await?;
        insert_service_account(
            &mut conn,
            principal_id,
            principal_kind,
            &card_ref,
            name,
            None,
            creator_id,
        )
        .await
        .map_err(sql)?;
        insert_api_key(
            &mut conn,
            Uuid::now_v7(),
            principal_id,
            &api_key.prefix,
            &key_hash,
            creator_id,
            chrono::Utc::now() + chrono::Duration::days(365),
        )
        .await
        .map_err(sql)?;
        for role in roles {
            grant_role(
                &mut conn,
                principal_id,
                PrincipalTable::ServiceAccount,
                role,
            )
            .await?;
        }
        conn.commit().await.map_err(sql)?;

        Ok(Bootstrap::Machine {
            id: PrincipalId::new(principal_id),
            api_key: api_key.secret,
            card_ref,
        })
    }

    async fn ensure_fixture_admin_for(
        &self,
        tenant_id: DataTenantId,
    ) -> Result<Uuid, WyrdTestServerError> {
        let id = fixture_admin_id(tenant_id);
        let email = format!("fixture-admin-{}@test.wyrd", id.simple());
        let mut conn = self.tenant_conn_for(tenant_id).await?;
        insert_user(&mut conn, id, Some(&email), "password", None)
            .await
            .or_else(|error| {
                if is_unique_violation(&error) {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(sql)?;
        conn.commit().await.map_err(sql)?;
        Ok(id)
    }

    async fn tenant_conn(&self) -> Result<TenantConn<'_>, WyrdTestServerError> {
        self.inner.fixture.tenant_conn().await.map_err(sql)
    }

    /// Open a tenant-scoped transaction bound to an explicit tenant.
    ///
    /// The RLS handle the isolation test drives directly: two distinct calls
    /// yield two independent [`TenantConn`]s scoped to different
    /// `data_tenant_id`s on the shared `wyrd_app` pool.
    ///
    /// # Errors
    /// Returns an error when acquiring or binding the transaction fails.
    pub async fn tenant_conn_for(
        &self,
        tenant_id: DataTenantId,
    ) -> Result<TenantConn<'_>, WyrdTestServerError> {
        self.inner
            .fixture
            .tenant_conn_for(tenant_id)
            .await
            .map_err(sql)
    }

    /// Reads one accepted Oracle epoch's validated durable table protection.
    ///
    /// Resolves the registered table UID under tenant RLS and ends the read
    /// transaction before returning, so inspection holds no maintenance lock.
    ///
    /// # Errors
    /// Returns a harness SQL error for missing or invalid identity, a malformed
    /// frontier, an unrepresentable fence, or a failed read transaction.
    pub async fn oracle_table_protection_for_test(
        &self,
        binding: &TenantTableBinding,
        node: NodeId,
        fence: FencingToken,
    ) -> Result<Option<ProtectionRecord>, WyrdTestServerError> {
        let mut conn = self.tenant_conn_for(binding.tenant).await?;
        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT table_uid FROM vala.bifrost_tables WHERE fqn = $1")
                .bind(binding.table_ref.fqn())
                .fetch_one(&mut **conn.transaction())
                .await
                .map_err(sql)?;
        let identity = TableAuthorityIdentity {
            tenant: binding.tenant,
            table_uid: <[u8; 16]>::try_from(stored.as_slice()).map_err(sql)?,
            catalog_name: BIFROST_CATALOG_NAME.to_owned(),
            namespace_name: binding.table_ref.namespace.as_str().to_owned(),
            table_name: binding.table_ref.name.clone(),
        };
        let record = OracleTableProtections::new(&mut conn)
            .read(
                &identity,
                node.as_uuid(),
                i64::try_from(fence).map_err(sql)?,
            )
            .await
            .map_err(sql)?;
        conn.commit().await.map_err(sql)?;
        Ok(record)
    }
    /// Return the durable Scribe WAL root this fixture composed the server with.
    ///
    /// Durability assertions replay the WAL directly rather than trusting an
    /// in-memory acknowledgement, so they need the same root `compose_bifrost`
    /// was handed. Returns `None` for a target that composes no Scribe role.
    #[must_use]
    pub fn scribe_wal_root_for_test(&self) -> Option<&std::path::Path> {
        self.inner._scribe_wal_root.as_ref().map(|root| root.path())
    }

    async fn raw_call(
        &self,
        mut req: Request<Body>,
    ) -> Result<Response<Body>, WyrdTestServerError> {
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                0,
            )));
        self.inner
            .router
            .clone()
            .oneshot(req)
            .await
            .map_err(|error| WyrdTestServerError::Io(error.to_string()))
    }

    /// Return the Postgres fixture for direct SQL access in tests.
    #[must_use]
    pub fn pg_fixture(&self) -> &PgFixture {
        &self.inner.fixture
    }

    /// Return pointer identities proving this process owns fresh runtime pools.
    #[must_use]
    pub fn postgres_pool_identity(&self) -> PostgresPoolIdentity {
        PostgresPoolIdentity {
            wyrd_app: std::ptr::from_ref(self.inner.state.postgres.app_pool()) as usize,
            vala_app: std::ptr::from_ref(self.inner.state.postgres.vala().pool()) as usize,
        }
    }

    /// Cancel and drain a partially started bound server while preserving the
    /// startup error that caused rollback.
    async fn rollback_bound_startup(
        &mut self,
        primary: WyrdTestServerError,
    ) -> WyrdTestServerError {
        if let Some(token) = self.shutdown_token.take() {
            token.cancel();
        }
        let Some(mut handle) = self.serve_handle.take() else {
            return primary;
        };
        if tokio::time::timeout(Duration::from_secs(2), &mut handle)
            .await
            .is_ok()
        {
            return primary;
        }
        handle.abort();
        if tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .is_err()
        {
            tracing::warn!("bound test server did not terminate after startup rollback abort");
        } else {
            tracing::warn!(
                "bound test server did not drain before startup rollback deadline; aborted"
            );
        }
        primary
    }

    /// Verify one Oracle peer bearer and return its effective permission set.
    ///
    /// # Errors
    ///
    /// Returns an auth error when the bearer is invalid, expired, or not bound
    /// to the reserved system tenant.
    #[cfg(test)]
    pub(crate) async fn oracle_peer_permissions_for_test(
        &self,
        bearer: String,
    ) -> Result<PermissionSet, WyrdTestServerError> {
        self.inner
            .verifier
            .verify(&SecretString::from(bearer), &DataTenantId::SYSTEM_OWNER)
            .await
            .map(|verified| verified.principal.effective_permissions.clone())
            .map_err(|error| WyrdTestServerError::Auth(error.to_string()))
    }

    /// Bind an already-constructed server to OS-assigned HTTP and gRPC ports.
    pub(crate) async fn bind(mut self) -> Result<WyrdTestServer, WyrdTestServerError> {
        // Reuse the production composition's shutdown token so the harness
        // drives the exact same immutable Bifrost owner as mounted routes.
        let state = self.inner.state.clone();
        let shutdown_token = state.shutdown_token.clone();

        if self.forge_process_role() == BifrostTarget::ForgeWorker {
            let worker = wyrd_server::boot::spawn_forge_worker(&state, shutdown_token.clone())
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
            // A dedicated Forge worker never runs the bounded Bifrost drain, so
            // its serve task reports that nothing drained rather than claiming a
            // drain outcome it did not produce.
            let serve_handle = wyrd_runtime::runtime().spawn(async move {
                worker
                    .await
                    .map(|()| BifrostShutdownReport::none_drained())
                    .map_err(|error| wyrd_server::BootExit::Other(Box::new(error)))
            });
            self.shutdown_token = Some(shutdown_token);
            self.serve_handle = Some(serve_handle);
            self.mode = Mode::InProcess;
            return Ok(self);
        }

        // Ephemeral ports, metrics off (the recorder is a process-global
        // singleton that must not be installed per-test), reflection off.
        let loopback = "127.0.0.1:0"
            .parse()
            .expect("static loopback socket addr is valid");
        let (http_bind, grpc_bind) = self.requested_bind.unwrap_or((loopback, loopback));
        let mut config = WyrdServerConfig::default();
        config.http.bind = http_bind;
        config.grpc.bind = grpc_bind;
        config.role = self.forge_process_role();
        if let Some(tls) = &self.requested_oracle_peer_tls {
            config.grpc.certificate_chain_path = Some(tls.certificate_path.clone());
            config.grpc.private_key_path = Some(tls.private_key_path.clone());
        }
        config.metrics.enabled = false;
        config.serve.mode = ServeMode::Both;

        let bound = WyrdServer::new(config, state)
            .map_err(|e| WyrdTestServerError::Start(e.to_string()))?
            .bind(ServeMode::Both)
            .await
            .map_err(|e| WyrdTestServerError::Bind(format!("{e:?}")))?;

        let addr = bound
            .http_addr()
            .ok_or_else(|| WyrdTestServerError::Bind("no HTTP address bound".to_owned()))?;
        let grpc_addr = bound
            .grpc_addr()
            .ok_or_else(|| WyrdTestServerError::Bind("no gRPC address bound".to_owned()))?;
        let base_url = format!("http://{addr}");

        let serve_handle: JoinHandle<Result<BifrostShutdownReport, wyrd_server::BootExit>> =
            if let Some(aborted) = self.stalled_drain_for_test.take() {
                wyrd_runtime::runtime().spawn(async move {
                    struct AbortObservation(Arc<AtomicBool>);

                    impl Drop for AbortObservation {
                        fn drop(&mut self) {
                            self.0.store(true, Ordering::SeqCst);
                        }
                    }

                    let _observation = AbortObservation(aborted);
                    let _bound = bound;
                    std::future::pending::<()>().await;
                    // Invariant: `pending()` never resolves, so this arm only ever
                    // leaves the future by abort. Fabricating a `BifrostShutdownReport`
                    // here would compile silently and hand a future teardown assertion
                    // a drain outcome that no drain produced.
                    unreachable!("stalled-drain serve task never completes; it is only aborted")
                })
            } else if self.serve_task_panic_for_test {
                // Serves normally so bound startup reaches readiness, then panics on
                // the serve stack once the production drain returns.
                wyrd_runtime::runtime().spawn(async move {
                    let _report = bound.run().await;
                    panic!("test-injected serve-task panic after drain");
                })
            } else {
                wyrd_runtime::runtime().spawn(async move { bound.run().await })
            };

        self.shutdown_token = Some(shutdown_token);
        self.serve_handle = Some(serve_handle);
        self.mode = Mode::Bound {
            addr,
            base_url: base_url.clone(),
            grpc_addr,
        };

        if self.readiness_failure {
            return Err(self
                .rollback_bound_startup(WyrdTestServerError::Bind(
                    "test-injected readiness failure".to_owned(),
                ))
                .await);
        }
        if let Err(error) = wait_for_ready(&base_url).await {
            return Err(self.rollback_bound_startup(error).await);
        }

        Ok(self)
    }
}

impl Drop for WyrdTestServer {
    fn drop(&mut self) {
        let Some(token) = self.shutdown_token.take() else {
            return;
        };
        token.cancel();
        match tokio::runtime::Handle::try_current() {
            Ok(_) => {
                tracing::warn!(
                    "WyrdTestServer dropped on an active Tokio runtime without explicit \
                     shutdown(); cancelling shared token only. Prefer `srv.shutdown().await` \
                     in async tests."
                );
            }
            Err(_) => {
                let runtime = wyrd_runtime::runtime();
                if let Some(handle) = self.serve_handle.take() {
                    let _ = runtime.block_on(async {
                        tokio::time::timeout(Duration::from_secs(2), handle).await
                    });
                }
            }
        }
    }
}

impl WyrdTestServerBuilder {
    /// Selects an explicit role heartbeat cadence for this test server only.
    #[must_use]
    pub fn with_role_timing_for_test(mut self, timing: RoleTiming) -> Self {
        self.role_timing = Some(timing);
        self
    }

    /// Attach the process-installed production telemetry guard.
    #[must_use]
    pub fn with_telemetry_for_test(mut self, telemetry: Arc<TelemetryGuard>) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    /// Select the production Forge process role for this fixture.
    #[must_use]
    pub fn with_forge_process_role_for_test(mut self, role: BifrostTarget) -> Self {
        self.forge_process_role = role;
        self
    }

    /// Observe successful completions from the production Forge worker.
    #[must_use]
    pub fn with_forge_completion_observer_for_test(
        mut self,
        observer: ForgeWorkerCompletionObserver,
    ) -> Self {
        self.forge_completion_observer = Some(observer);
        self
    }

    /// Replace the Forge catalog with a deterministic test wrapper.
    #[must_use]
    pub fn with_forge_catalog_for_test(mut self, catalog: Arc<dyn iceberg::Catalog>) -> Self {
        self.forge_catalog = Some(catalog);
        self
    }

    /// Use one complete validated Forge configuration for this fixture.
    #[must_use]
    pub fn with_forge_config_for_test(mut self, config: ForgeConfig) -> Self {
        self.forge_config = Some(config);
        self
    }

    /// Install an eval-run audit sink on the composed `AppState`.
    ///
    /// The default sink discards events, so a test that must prove a run
    /// open/complete pair was audited supplies a recording writer here rather
    /// than reaching into composed state after the fact.
    #[must_use]
    pub fn with_eval_audit_for_test(
        mut self,
        writer: Arc<dyn wyrd_server::components::eval::EvalAuditWriter>,
    ) -> Self {
        self.eval_audit = Some(writer);
        self
    }

    /// Compose the server without a token verifier.
    ///
    /// Models a server whose auth backend is not yet configured — the state a
    /// deployment occupies between process start and verifier provisioning.
    /// Every `/v1` request must then answer `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE`
    /// rather than 401 or 500, so callers back off instead of treating the
    /// window as a credential failure.
    #[must_use]
    pub fn without_token_verifier_for_test(mut self) -> Self {
        self.omit_token_verifier = true;
        self
    }

    /// Apply a non-default edge limit profile to the composed `AppState`.
    ///
    /// Edge behaviour that only manifests at a boundary — a body-size refusal,
    /// a request timeout, a concurrency cap — is unreachable at the production
    /// defaults inside a test. This knob feeds the profile through the same
    /// `AppState::with_limits` the server uses, so the router under test is the
    /// composed production router with one configuration value changed.
    #[must_use]
    pub fn with_limits_for_test(mut self, limits: wyrd_server::state::LimitsConfig) -> Self {
        self.limits = Some(limits);
        self
    }

    /// Force the bound readiness phase to fail and exercise startup rollback.
    #[must_use]
    pub fn with_readiness_failure_for_test(mut self) -> Self {
        self.readiness_failure = true;
        self
    }

    /// Panic the bound serve task once its production drain has returned.
    ///
    /// The panic is deferred until after `BoundServer::run` completes so bound
    /// startup still reaches readiness; a task that panicked earlier would fail
    /// startup rollback instead of teardown. Aborting the task would instead
    /// surface [`tokio::task::JoinError::is_cancelled`], which is not the signal
    /// a teardown seam is required to refuse.
    #[must_use]
    pub fn with_serve_task_panic_for_test(mut self) -> Self {
        self.serve_task_panic_for_test = true;
        self
    }

    /// Make the bound serve task ignore shutdown until the rollback aborts it.
    ///
    /// The supplied flag is set when the stalled task is dropped, allowing a
    /// smoke test to prove that a timed-out drain was followed by an abort.
    #[must_use]
    pub fn with_stalled_drain_for_test(mut self, aborted: Arc<AtomicBool>) -> Self {
        self.stalled_drain_for_test = Some(aborted);
        self
    }

    /// Bind a test server to caller-reserved HTTP and gRPC addresses.
    #[must_use]
    pub fn with_bind_addrs_for_test(
        mut self,
        http: std::net::SocketAddr,
        grpc: std::net::SocketAddr,
    ) -> Self {
        self.bind_addrs = Some((http, grpc));
        self
    }
    /// Override the default allow policy hook.
    #[must_use]
    pub fn with_policy_hook(mut self, hook: Arc<dyn PolicyHook>) -> Self {
        self.policy_hook = Some(hook);
        self
    }

    /// Override the default no-op audit writer.
    #[must_use]
    pub fn with_audit_writer(mut self, writer: Arc<dyn AuthzAuditWriter>) -> Self {
        self.audit_writer = Some(writer);
        self
    }

    /// Override the preview-auth gate.
    #[must_use]
    pub fn with_preview_auth(mut self, allow: bool) -> Self {
        self.allow_preview_auth = allow;
        self
    }

    /// Override the storage backend used by the test server.
    ///
    /// By default the server uses a local filesystem backend backed by a
    /// `tempdir`. Call this to start the server against a real cloud backend
    /// (e.g. S3/RustFS) for multipart or end-to-end storage tests. Credentials
    /// must be present in the calling process's environment.
    #[must_use]
    pub fn with_storage_settings(mut self, settings: StorageSettings) -> Self {
        self.storage_settings = Some(settings);
        self
    }

    /// Inject a pre-built storage handle.
    ///
    /// Use this for emulator backends (GCS, Azure) whose backend config carries
    /// no emulator endpoint, so the handle must be built from an emulator signer
    /// via [`wyrd_storage::StorageHandle::from_signer`]. The in-process server
    /// skips the boot health probe, so the handle's operator is never exercised.
    /// Takes precedence over [`Self::with_storage_settings`].
    #[must_use]
    pub fn with_storage_handle(mut self, handle: Arc<wyrd_storage::StorageHandle>) -> Self {
        self.storage_handle = Some(handle);
        self
    }

    /// Override the access token TTL for all exchange paths.
    ///
    /// Use this in TTL-expiry journey tests to mint short-lived tokens without
    /// waiting for the 15-minute production default. Pair with
    /// [`Self::with_auth_verify_settings`] to reduce the clock-skew tolerance.
    #[must_use]
    pub fn with_access_ttl(mut self, ttl: ChronoDuration) -> Self {
        self.access_ttl = Some(ttl);
        self
    }

    /// Replace the token verifier settings.
    ///
    /// Use this to reduce `allowed_clock_skew` and `cache_ttl` to near-zero for
    /// TTL journey tests so a real `exp` can be observed without a multi-minute
    /// sleep.
    #[must_use]
    pub fn with_auth_verify_settings(mut self, settings: WyrdAuthVerifySettings) -> Self {
        self.auth_verify_settings = Some(settings);
        self
    }

    /// Boot trusted OIDC issuers from `[[trusted_issuers]]` config DTOs.
    ///
    /// At [`Self::start_in_process`] these are discovered and seeded into
    /// Postgres via the production [`seed_trusted_issuers`] path, and the
    /// verifier's external (foreign-OIDC) path is wired to the Postgres-backed
    /// [`PgIssuerResolver`] exactly the way a self-hosted deployment boots.
    /// Every entry binds to the fixture's implicit `DataTenantId`. Boot fails
    /// closed if discovery is unreachable.
    #[must_use]
    pub fn with_trusted_issuer_configs(mut self, configs: Vec<IssuerEntry>) -> Self {
        self.trusted_issuer_configs = configs;
        self
    }

    /// Boot workload bindings from `[[workload_bindings]]` config DTOs.
    ///
    /// At [`Self::start_in_process`] these run through the production
    /// [`build_workload_bindings`] boot path and are seeded into Postgres via
    /// [`seed_workload_bindings`], each bound to the fixture's implicit
    /// `DataTenantId`.
    #[must_use]
    pub fn with_workload_binding_configs(mut self, configs: Vec<WorkloadBindingEntry>) -> Self {
        self.workload_binding_configs = configs;
        self
    }

    /// Use a shorter Forge interval for scheduler-driven integration journeys.
    #[must_use]
    pub fn with_forge_interval(mut self, interval: Duration) -> Self {
        self.forge_interval = interval;
        self
    }

    /// Inject a deterministic WAL sync delay for benchmark and failure tests.
    /// Production server construction never uses this test-builder option.
    #[must_use]
    pub fn with_wal_sync_delay(mut self, delay: Duration) -> Self {
        self.wal_sync_delay = delay;
        self
    }

    /// Override Scribe admission limits for deterministic test-tier probes.
    #[must_use]
    pub fn with_scribe_admission_for_test(mut self, admission: AdmissionConfig) -> Self {
        self.scribe_admission = Some(admission);
        self
    }

    /// Replaces the shared Gate-and-Scribe ingest limits for deterministic journeys.
    ///
    /// The builder retains one copied immutable snapshot and supplies it to both
    /// owners during startup, preserving the production single-authority contract
    /// while allowing cap and cap-plus-one requests to remain small in tests.
    #[must_use]
    pub fn with_scribe_ingest_limits_for_test(mut self, limits: IngestLimits) -> Self {
        self.scribe_ingest_limits = limits;
        self
    }

    /// Select a scaled Scribe geometry for a real-server journey.
    ///
    /// This installs no second lifecycle policy: the value is the same
    /// validated geometry production boot derives from `ScribeRuntimeConfig`,
    /// so a journey can move the rotation limits and the assembled-object
    /// target down to test scale and still exercise the production controls.
    #[must_use]
    pub fn with_scribe_geometry_for_test(
        mut self,
        geometry: vala_bifrost_redux::scribe::geometry::ScribeGeometry,
    ) -> Self {
        self.scribe_geometry_for_test = Some(geometry);
        self
    }

    /// Installs deterministic controls in the real Scribe persistence graph.
    ///
    /// The controls are consumed only by the `test-support` build and preserve
    /// the production persistence, SQL publication, and retirement sequence.
    #[must_use]
    pub fn with_scribe_persistence_faults_for_test(
        mut self,
        faults: vala_bifrost_redux::scribe::persistence::PersistenceFaults,
    ) -> Self {
        self.scribe_persistence_faults_for_test = Some(faults);
        self
    }

    /// Run this server under one production Bifrost process target.
    ///
    /// The role set is derived by [`BifrostRoles::for_target`] — the same
    /// derivation `WYRD_TARGET` drives in production — so a Scribe-only test
    /// server activates exactly the subsystems a Scribe pod activates, without
    /// a test-specific topology.
    #[must_use]
    pub fn with_bifrost_target_for_test(mut self, target: BifrostTarget) -> Self {
        self.bifrost_roles = wyrd_server::config::BifrostRoles::for_target(target)
            .iter()
            .copied()
            .collect();
        self.forge_process_role = target;
        self
    }

    /// Retain one stable physical identity and role set across cluster restarts.
    #[must_use]
    pub(crate) fn with_bifrost_node(
        mut self,
        node_id: NodeId,
        roles: BTreeSet<BifrostRuntimeRole>,
    ) -> Self {
        self.node_id = Some(node_id);
        self.bifrost_roles = roles;
        self
    }

    /// Reuse cluster-owned local WAL and spill directories for this node.
    #[must_use]
    pub(crate) fn with_bifrost_roots(
        mut self,
        scribe_wal_root: Option<Arc<tempfile::TempDir>>,
        oracle_spill_root: Option<Arc<tempfile::TempDir>>,
        oracle_audit_wal_root: Option<Arc<tempfile::TempDir>>,
    ) -> Self {
        self.scribe_wal_root = scribe_wal_root;
        self.oracle_spill_root = oracle_spill_root;
        self.oracle_audit_wal_root = oracle_audit_wal_root;
        self
    }

    /// Injects complete process-visible resources into the production bootstrap.
    ///
    /// Tests vary raw observations through this seam; all reserve, floor,
    /// elastic, lease, and partition calculations remain production-owned.
    #[must_use]
    pub(crate) fn with_system_resources_for_test(
        mut self,
        snapshot: SystemResourceSnapshot,
    ) -> Self {
        self.system_resources = Some(snapshot);
        self
    }

    /// Names the Forge compaction budget this node admits plans against.
    ///
    /// The harness default is sized for the small tables most fixtures compact.
    /// A journey that compacts production-sized inputs states the budget its
    /// pod was sized for here, exactly as a deployment configures one.
    #[must_use]
    pub(crate) fn with_forge_compaction_memory_limit_for_test(mut self, bytes: usize) -> Self {
        self.forge_compaction_memory_limit_bytes = Some(bytes);
        self
    }

    /// Attach the process-installed production telemetry pipeline.
    #[must_use]
    pub(crate) fn with_telemetry(mut self, telemetry: Arc<TelemetryGuard>) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    /// Bind to cluster-reserved addresses and advertise the real private gRPC endpoint.
    #[must_use]
    pub(crate) fn with_bind_addrs(
        mut self,
        http: std::net::SocketAddr,
        grpc: std::net::SocketAddr,
    ) -> Self {
        self.bind_addrs = Some((http, grpc));
        self
    }

    /// Inject the cluster-owned Oracle peer credential without process globals.
    #[must_use]
    pub(crate) fn with_oracle_peer_credentials(
        mut self,
        credentials: Arc<dyn OraclePeerCredentials>,
    ) -> Self {
        self.oracle_peer_credentials = Some(credentials);
        self
    }

    /// Enable TLS on the bound gRPC listener and Oracle peer transport.
    #[must_use]
    pub(crate) fn with_oracle_peer_tls(mut self, tls: TestOraclePeerTls) -> Self {
        self.oracle_peer_tls = Some(tls);
        self
    }

    /// Build and start an in-process server.
    ///
    /// # Errors
    /// Returns an error when database, storage, auth, or router state cannot be created.
    pub async fn start_in_process(mut self) -> Result<WyrdTestServer, WyrdTestServerError> {
        let fixture = Arc::new(
            PgFixture::start()
                .await
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
        );
        let tenant_id = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.map_err(sql)?;
        seed_builtin_roles_for_tenant(&mut conn, tenant_id)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        conn.commit().await.map_err(sql)?;

        let (storage_root, storage) = if let Some(handle) = self.storage_handle.take() {
            (None, handle)
        } else if let Some(settings) = self.storage_settings.take() {
            let handle = wyrd_storage::StorageHandle::from_settings(settings)
                .await
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
            (None, handle)
        } else {
            let root = tempfile::tempdir()
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
            let settings = StorageSettings {
                backend: BackendConfig::Local {
                    root: root.path().to_path_buf(),
                },
                require_encryption: false,
                presign_ttl: Duration::from_secs(600),
                part_size_bytes: 16 * 1024 * 1024,
                multipart_threshold_bytes: 100 * 1024 * 1024,
                public_base_url: Some("https://wyrd.test".to_owned()),
            };
            // The filesystem service resumes a listing from `start_after`
            // correctly but does not advertise the capability, and Forge
            // workers refuse to start on a staging backend that cannot resume
            // a bounded orphan scan. The default fixture stands in for a
            // production object store, so it declares the support it actually
            // has; a caller wanting the incapable backend supplies plain
            // storage settings instead.
            let operator = wyrd_storage::factory::build_operator(&settings.backend)
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?
                .layer(opendal::layers::CapabilityOverrideLayer::new(
                    |mut capability| {
                        capability.list_with_start_after = true;
                        capability
                    },
                ));
            let handle =
                wyrd_storage::StorageHandle::from_settings_with_operator(settings, operator)
                    .await
                    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
            (Some(Arc::new(root)), handle)
        };
        if self.bifrost_roles.contains(&BifrostRuntimeRole::Oracle)
            && self.oracle_peer_credentials.is_none()
        {
            self.oracle_peer_credentials =
                Some(provision_oracle_peer_credentials(Arc::clone(&fixture)).await?);
        }

        self.start_with_resources(fixture, storage, storage_root)
            .await
    }

    /// Start a server over shared cluster resources.
    ///
    /// The caller owns the shared fixture, storage root, and catalog for the
    /// lifetime of every server created from them. Each server still binds
    /// its own HTTP and gRPC sockets and creates its own application state.
    ///
    /// # Errors
    ///
    /// Returns an error when another Rustls provider already owns the process,
    /// or fixture resources, authentication state, Forge, Scribe, or the
    /// application router cannot be constructed. Cancellation may leave
    /// fixture-owned database setup committed, but no server task is retained.
    pub(crate) async fn start_with_resources(
        self,
        fixture: Arc<PgFixture>,
        storage: Arc<wyrd_storage::StorageHandle>,
        storage_root: Option<Arc<tempfile::TempDir>>,
    ) -> Result<WyrdTestServer, WyrdTestServerError> {
        wyrd_tls::install_crypto_provider()
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        // Keep one governor and one DataFusion pool in this graph. Forge and
        // AppState must observe the same pool so query and rewrite admission
        // share accounting rather than silently creating independent budgets.
        let tenant_id = fixture.data_tenant_id();
        let (runtime_wyrd, runtime_vala) = fixture
            .fresh_runtime_handles()
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;

        let issuing_key = Arc::new(
            IssuingKey::from_ed_pem(
                crate::keys::private_key_pem(),
                Kid::new("test").expect("static kid is valid"),
                "wyrd",
            )
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
        );
        let mut decoding_keys = HashMap::new();
        decoding_keys.insert(
            Kid::new("test").expect("static kid is valid"),
            Arc::new(
                public_key_from_pem(crate::keys::public_key_pem().as_bytes())
                    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
            ),
        );
        let resolver = Arc::new(SqlPermissionResolver::new(Arc::new(
            runtime_wyrd.app_pool().clone(),
        )));
        let verify_settings = self.auth_verify_settings.unwrap_or_default();

        // Postgres-backed boot, mirroring a self-hosted deployment: discover and
        // seed `[[trusted_issuers]]` into Postgres, then seed `[[workload_bindings]]`
        // (which FK-reference them). A deterministic test sealing key encrypts any
        // client secret on write and decrypts it on read. The production Pg
        // resolvers then serve issuers/bindings per-request, including on the
        // verifier's external (foreign-OIDC) path.
        let sealing_key = Arc::new(SecretKey::from_bytes([9_u8; 32]));
        seed_trusted_issuers(
            runtime_wyrd.app_pool(),
            tenant_id,
            &self.trusted_issuer_configs,
            Some(sealing_key.as_ref()),
        )
        .await
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let bindings = build_workload_bindings(&self.workload_binding_configs, tenant_id)
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        seed_workload_bindings(runtime_wyrd.app_pool(), tenant_id, &bindings)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;

        let issuer_resolver = Arc::new(PgIssuerResolver::new(
            Arc::new(runtime_wyrd.app_pool().clone()),
            Some(Arc::clone(&sealing_key)),
        ));
        let binding_resolver = Arc::new(PgWorkloadBindingResolver::new(Arc::new(
            runtime_wyrd.app_pool().clone(),
        )));

        let verifier = Arc::new(
            TokenVerifier::new(decoding_keys, "wyrd", resolver, verify_settings)
                // Match production's five-second epoch cache. Immediate
                // revocation remains covered by the focused auth test seam;
                // benchmark requests must not force a SQL lookup per request.
                .with_revocation(Arc::new(SqlRevocationCheck::new(Arc::new(
                    runtime_wyrd.app_pool().clone(),
                ))))
                .with_external(
                    Arc::new(JwksCache::new(
                        reqwest::Client::new(),
                        Duration::from_secs(300),
                        Duration::from_secs(5),
                    )),
                    Arc::clone(&issuer_resolver),
                ),
        );

        let exchange_settings = if let Some(ttl) = self.access_ttl {
            TokenExchangeSettings {
                access_ttl: ttl,
                ..TokenExchangeSettings::default()
            }
        } else {
            TokenExchangeSettings::default()
        };

        let postgres = Arc::new(ServerPostgres::from_parts(runtime_wyrd, runtime_vala));
        let resource_roles = self
            .bifrost_roles
            .iter()
            .map(|role| match role {
                BifrostRuntimeRole::Scribe => BifrostRole::Scribe,
                BifrostRuntimeRole::ForgeCoordinator | BifrostRuntimeRole::ForgeWorker => {
                    BifrostRole::Forge
                }
                BifrostRuntimeRole::Oracle => BifrostRole::Oracle,
            })
            .collect();
        let spill_root = self.oracle_spill_root.clone().unwrap_or(Arc::new(
            tempfile::tempdir().map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
        ));
        let scratch_root = spill_root.path().to_owned();
        let wal_root = self.scribe_wal_root.clone().unwrap_or(Arc::new(
            tempfile::tempdir().map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
        ));
        let volume_roots = if self.bifrost_roles.is_empty() {
            None
        } else {
            let wal_volume_root = wal_root.path().to_owned();
            let scribe_stage = wal_volume_root.join("scribe-stage");
            let scribe_output = scratch_root.join("scribe-output");
            let oracle_scratch = scratch_root.join("oracle");
            for root in [
                &wal_volume_root,
                &scribe_stage,
                &scribe_output,
                &oracle_scratch,
            ] {
                std::fs::create_dir_all(root)
                    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
            }
            Some(BifrostVolumeRoots {
                wal: wal_volume_root,
                scribe_stage,
                scribe_output_scratch: scribe_output,
                oracle_scratch,
            })
        };
        let snapshot = self.system_resources.unwrap_or(SystemResourceSnapshot {
            memory_limit_bytes: HARNESS_NODE_MEMORY_LIMIT_BYTES,
            effective_cpu: 4,
            scratch_capacity_bytes: 4 * 1024 * 1024 * 1024,
            scratch_available_bytes: 4 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        });
        // The formula's default (four fifths of the memory limit) deliberately
        // does not clamp, so a co-located harness whose Scribe and Oracle floors
        // are also protected must name a budget that fits the remainder —
        // exactly as a co-located deployment configures one.
        let forge_budget_bytes = self
            .bifrost_roles
            .iter()
            .any(|role| {
                matches!(
                    role,
                    BifrostRuntimeRole::ForgeCoordinator | BifrostRuntimeRole::ForgeWorker
                )
            })
            .then(|| {
                self.forge_compaction_memory_limit_bytes.unwrap_or_else(|| {
                    // The default is sized for the tables most fixtures
                    // compact, but a small pod still has to leave elastic
                    // memory for the Scribe and Oracle work beside it, so the
                    // remainder left by the protected floors halves it.
                    let protected = self
                        .bifrost_roles
                        .iter()
                        .filter(|role| {
                            matches!(
                                role,
                                BifrostRuntimeRole::Scribe | BifrostRuntimeRole::Oracle
                            )
                        })
                        .count()
                        * ROLE_MEMORY_FLOOR_BYTES;
                    let safe = snapshot
                        .memory_limit_bytes
                        .saturating_sub(MIN_UNMANAGED_RESERVE_BYTES)
                        .saturating_sub(protected);
                    HARNESS_FORGE_COMPACTION_BUDGET_BYTES.min(safe / 2)
                })
            });
        let runtime_resources =
            BifrostRuntimeResources::from_snapshot_with_transport_message_limit(
                snapshot,
                BifrostResourcePolicy {
                    roles: resource_roles,
                    memory_limit_bytes: None,
                    unmanaged_reserve_bytes: None,
                    scratch_limit_bytes: None,
                    effective_cpu: None,
                    oracle_query_slot_limit: None,
                    // The formula's default (four fifths of the memory limit)
                    // deliberately does not clamp, so a co-located harness whose
                    // Scribe and Oracle floors are also protected must name a
                    // budget that fits the remainder — exactly as a co-located
                    // deployment configures one.
                    forge_compaction_memory_limit_bytes: forge_budget_bytes,
                    scratch_root,
                    volume_roots,
                },
                self.scribe_ingest_limits.max_frame_bytes,
            )
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let bifrost_resources = runtime_resources
            .compose_roles()
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        // Same boot order as production: resources, then the one storage owner,
        // then the catalog that runs its Iceberg I/O through it.
        let bifrost_storage = Arc::new(vala_bifrost_redux::storage::BifrostStorage::new(
            Arc::clone(&storage),
            vala_bifrost_redux::storage::BifrostStoragePolicy::resolve(
                vala_bifrost_redux::storage::BifrostStorageConfig::default(),
                u64::try_from(bifrost_resources.plan().managed_memory_bytes).unwrap_or(u64::MAX),
                bifrost_resources.oracle().is_some(),
            )
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
            bifrost_resources.oracle().map(|oracle| oracle.metadata()),
        ));
        let bifrost = test_catalog(&fixture, Arc::clone(&bifrost_storage)).await?;
        let target = self.forge_process_role;
        let node_id = self.node_id.unwrap_or_else(|| NodeId::new(Uuid::now_v7()));
        let cluster_registry = Arc::new(if let Some(timing) = self.role_timing {
            ClusterRegistry::new_with_role_timing(postgres.vala().clone(), node_id, timing)
        } else {
            ClusterRegistry::new(postgres.vala().clone(), node_id)
        });
        let forge_config = self.forge_config.unwrap_or_default();
        let (forge_clock, forge_clock_control) = ForgeClock::manual(Utc::now());
        let forge_scheduler_trigger = ForgeSchedulerTrigger::new();
        let forge_object_store = (self
            .bifrost_roles
            .contains(&BifrostRuntimeRole::ForgeCoordinator)
            || self
                .bifrost_roles
                .contains(&BifrostRuntimeRole::ForgeWorker))
        .then(|| ForgeObjectStoreControl::new(Arc::new(storage.operator().clone())));
        let composed_forge_object_store = forge_object_store.as_ref().map(|control| {
            Arc::clone(control) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>
        });
        let test_controls = BifrostTestControls {
            forge_clock,
            forge_scheduler_trigger: forge_scheduler_trigger.clone(),
            forge_completion_observer: self.forge_completion_observer.clone(),
            forge_config: Some(forge_config),
            forge_catalog: self.forge_catalog,
            forge_object_store: composed_forge_object_store,
            scribe_wal_sync_delay: self.wal_sync_delay,
            scribe_geometry: self.scribe_geometry_for_test,
            scribe_persistence_faults: self
                .scribe_persistence_faults_for_test
                .clone()
                .unwrap_or_default(),
            scribe_admission: self.scribe_admission,
            role_timing: self.role_timing,
        };
        let peer_credentials = match self.oracle_peer_credentials {
            Some(credentials) => credentials,
            None => provision_oracle_peer_credentials(Arc::clone(&fixture)).await?,
        };
        let peer_tls = match &self.oracle_peer_tls {
            Some(tls) => Some(vala_bifrost_redux::oracle::dispatcher::OraclePeerTls::new(
                std::fs::read(&tls.ca_path)
                    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
                tls.server_name.clone(),
            )),
            None => None,
        };
        let mut bifrost_config = BifrostRuntimeConfig::default();
        bifrost_config.scribe.ingest_request_bytes = self.scribe_ingest_limits.max_frame_bytes;
        let forge_runtime = ForgeRuntimeConfig {
            maintenance_interval_secs: Some(self.forge_interval.as_secs()),
            ..ForgeRuntimeConfig::default()
        };
        let shutdown = CancellationToken::new();
        let ComposedBifrost {
            bifrost: bifrost_runtime,
            coordination_runtime,
            compaction_runtime,
        } = wyrd_server::boot::compose_bifrost(BifrostBuildInputs {
            target,
            deployment_profile: DeploymentProfile::Development,
            postgres: postgres.as_ref().clone(),
            storage: Arc::clone(&storage),
            bifrost_storage: Arc::clone(&bifrost_storage),
            catalog: Arc::clone(&bifrost),
            resources: bifrost_resources,
            cluster: cluster_registry,
            token_verifier: Arc::clone(&verifier),
            peer_credentials,
            peer_tls,
            signing_key: SecretString::from(crate::keys::private_key_pem().to_owned()),
            config: bifrost_config,
            forge_config: forge_runtime,
            node_id,
            // Must agree with `WyrdTestServer::grpc_url`: the address published
            // into cluster membership is dialed by peer transports, and a TLS
            // transport refuses a peer advertising a plaintext scheme.
            advertise_addr: match (self.bind_addrs, self.oracle_peer_tls.is_some()) {
                (Some((_, grpc)), true) => format!("https://localhost:{}", grpc.port()),
                (Some((_, grpc)), false) => format!("http://{grpc}"),
                (None, true) => "https://localhost:0".to_owned(),
                (None, false) => "http://127.0.0.1:0".to_owned(),
            },
            wal_dir: wal_root.path().to_owned(),
            shutdown: shutdown.clone(),
            test_controls: Some(test_controls),
        })
        .await
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let query_stream_fault = QueryStreamFaultController::default();
        let query_control_audit_fault =
            wyrd_server::state::QueryControlAuditFaultController::default();
        let mut state = AppState::new(postgres, storage, bifrost_runtime, shutdown)
            .with_query_stream_fault(query_stream_fault.clone())
            .with_query_control_audit_fault(query_control_audit_fault.clone())
            .with_auth(wyrd_server::components::auth::ServerAuth {
                allow_preview: self.allow_preview_auth,
                issuing_key: Some(Arc::clone(&issuing_key)),
                token_verifier: (!self.omit_token_verifier).then(|| Arc::clone(&verifier)),
                token_exchange_settings: exchange_settings,
                trusted_issuer_resolver: Some(issuer_resolver),
                workload_binding_resolver: Some(binding_resolver),
                sealing_key: Some(sealing_key),
            });
        if let Some(writer) = self.eval_audit {
            state = state.with_eval_audit(writer);
        }
        if let Some(limits) = self.limits {
            state = state.with_limits(limits);
        }
        state.authz.permission_check = Arc::new(RbacCheck);
        state.authz.audit_writer = self
            .audit_writer
            .unwrap_or_else(|| Arc::new(NoopAuthzAuditWriter));
        if let Some(hook) = self.policy_hook {
            state.authz.policy_hook = hook;
        }
        let (forge_publisher, _forge_inbox) = staging_file_channel(16)
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let router = build_router(state.clone());

        Ok(WyrdTestServer {
            inner: WyrdTestServerInner {
                fixture,
                _storage_root: storage_root,
                _scribe_wal_root: Some(wal_root),
                _bifrost_spill_root: Some(spill_root),
                _oracle_audit_wal_root: self.oracle_audit_wal_root,
                state,
                router,
                verifier,
                issuing_key,
                api_key: SecretString::from(String::new()),
                forge_publisher,
                bifrost_catalog: Arc::clone(&bifrost),
                forge_clock: forge_clock_control,
                forge_scheduler_trigger,
                forge_object_store,
                forge_process_role: self.forge_process_role,
                node_id,
                query_stream_fault,
                query_control_audit_fault,
                query_stream_stall: std::sync::Mutex::new(None),
                _coordination_runtime: coordination_runtime,
                _compaction_runtime: compaction_runtime,
            },
            mode: Mode::InProcess,
            shutdown_token: None,
            serve_handle: None,
            requested_bind: self.bind_addrs,
            requested_oracle_peer_tls: self.oracle_peer_tls,
            readiness_failure: self.readiness_failure,
            stalled_drain_for_test: self.stalled_drain_for_test,
            serve_task_panic_for_test: self.serve_task_panic_for_test,
        })
    }

    /// Build, start, and bind the server to OS-assigned TCP sockets (HTTP + gRPC).
    ///
    /// Boots the **exact production server** — `WyrdServer::new(...).bind(...)`
    /// then `BoundServer::run()` — so bound-mode tests exercise the real
    /// composition (protected edge stack, gRPC ingest mount, readiness-driven
    /// health, supervisor) rather than a hand-rolled facsimile. Only the state
    /// origin differs from production: it is built from the per-test
    /// [`PgFixture`] instead of loaded config.
    ///
    /// Improves on the "find a free port, drop it, rebind" pattern: `bind`
    /// binds the OS-assigned `:0` listeners once and reports the concrete
    /// addresses, so there is no bind-then-rebind race.
    ///
    /// # Errors
    /// Returns an error when another Rustls provider already owns the process,
    /// startup fails, or socket binding fails. Cancellation may leave fixture
    /// database setup committed, while bound sockets close when owned state is
    /// dropped.
    pub async fn start_bound(mut self) -> Result<WyrdTestServer, WyrdTestServerError> {
        if self.bind_addrs.is_none() {
            self.bind_addrs = Some((reserve_loopback_addr()?, reserve_loopback_addr()?));
        }
        let srv = self.start_in_process().await?;
        srv.bind().await
    }
}

/// Ask the OS for one currently free loopback address used by a bound harness.
///
/// Reserving before server composition gives membership a concrete nonzero
/// private endpoint. The production binder later claims this exact address.
///
/// # Errors
///
/// Returns a bind error when loopback binding or address lookup fails.
pub(crate) fn reserve_loopback_addr() -> Result<std::net::SocketAddr, WyrdTestServerError> {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| WyrdTestServerError::Bind(error.to_string()))?;
    listener
        .local_addr()
        .map_err(|error| WyrdTestServerError::Bind(error.to_string()))
}

/// Poll a bound test server until its HTTP health endpoint reports readiness.
///
/// # Errors
///
/// Returns [`WyrdTestServerError::Start`] when another Rustls provider already
/// owns the process, or [`WyrdTestServerError::Bind`] when the server does not
/// become ready within the bounded retry window. Cancellation stops polling
/// without stopping the independently owned server task.
async fn wait_for_ready(base_url: &str) -> Result<(), WyrdTestServerError> {
    wyrd_tls::install_crypto_provider()
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
    let client = reqwest::Client::new();
    let url = format!("{base_url}/healthz");
    for attempt in 0..30u32 {
        if client
            .get(&url)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100 + (u64::from(attempt) * 10))).await;
    }
    Err(WyrdTestServerError::Bind(format!(
        "server at {base_url} never became ready"
    )))
}

#[derive(Clone, Copy)]
enum PrincipalTable {
    User,
    ServiceAccount,
}

async fn grant_role(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    table: PrincipalTable,
    role: &str,
) -> Result<(), WyrdTestServerError> {
    let role_id = lookup_role_id(conn, role).await?;
    match table {
        PrincipalTable::User => grant_role_to_user(conn, principal_id, role_id)
            .await
            .map(|_| ())
            .map_err(sql),
        PrincipalTable::ServiceAccount => {
            grant_role_to_service_account(conn, principal_id, role_id)
                .await
                .map(|_| ())
                .map_err(sql)
        }
    }
}

/// Provision the cluster-scoped SYSTEM_OWNER Service credential used by Oracle peers.
///
/// The returned owner retains the API key only in memory and exchanges it
/// through [`ExchangeApiKey`] whenever a node boots or refreshes.
///
/// # Errors
///
/// Returns an error when role seeding, principal/key persistence, hashing, or
/// issuing-key construction fails.
pub(crate) async fn provision_oracle_peer_credentials(
    fixture: Arc<PgFixture>,
) -> Result<Arc<dyn OraclePeerCredentials>, WyrdTestServerError> {
    let tenant_id = DataTenantId::SYSTEM_OWNER;
    let creator_id = fixture_admin_id(tenant_id);
    let principal_id = Uuid::now_v7();
    let service_ref = card_ref(CardKind::Service, "bifrost-oracle-peer")?;
    let api_key = WyrdApiKey::generate(tenant_id);
    let raw = api_key.secret.clone();
    let key_hash = tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw))
        .await
        .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?
        .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?;
    let mut conn = fixture.tenant_conn_for(tenant_id).await.map_err(sql)?;
    seed_builtin_roles_for_tenant(&mut conn, tenant_id)
        .await
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
    let permissions = vec![Permission::bifrost_oracle_peer_invoke()];
    let permissions_json = serde_json::to_value(&permissions)
        .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?;
    insert_role(
        &mut conn,
        Uuid::now_v7(),
        ORACLE_PEER_ROLE,
        &permissions_json,
        false,
    )
    .await
    .map_err(sql)?;
    let email = format!("fixture-admin-{}@test.wyrd", creator_id.simple());
    insert_user(&mut conn, creator_id, Some(&email), "password", None)
        .await
        .map_err(sql)?;
    seed_machine_card(&mut conn, &service_ref, creator_id).await?;
    insert_service_account(
        &mut conn,
        principal_id,
        "service",
        &service_ref,
        "bifrost-oracle-peer",
        None,
        creator_id,
    )
    .await
    .map_err(sql)?;
    insert_api_key(
        &mut conn,
        Uuid::now_v7(),
        principal_id,
        &api_key.prefix,
        &key_hash,
        creator_id,
        chrono::Utc::now() + chrono::Duration::days(1),
    )
    .await
    .map_err(sql)?;
    grant_role(
        &mut conn,
        principal_id,
        PrincipalTable::ServiceAccount,
        ORACLE_PEER_ROLE,
    )
    .await?;
    conn.commit().await.map_err(sql)?;
    let issuing_key = Arc::new(
        IssuingKey::from_ed_pem(
            crate::keys::private_key_pem(),
            Kid::new("test").expect("static kid is valid"),
            "wyrd",
        )
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
    );
    let credentials = Arc::new(TestOraclePeerCredentials {
        fixture,
        api_key: api_key.secret,
        exchange: ExchangeApiKey {
            issuing_key,
            settings: TokenExchangeSettings::default(),
        },
        bearer: tokio::sync::Mutex::new(None),
    });
    credentials
        .bearer(false)
        .await
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
    Ok(credentials)
}

async fn lookup_role_id(
    conn: &mut TenantConn<'_>,
    role: &str,
) -> Result<Uuid, WyrdTestServerError> {
    role_by_name(conn, role)
        .await
        .map_err(sql)?
        .map(|row| row.id)
        .ok_or_else(|| WyrdTestServerError::Sql(format!("role not found: {role}")))
}

/// Deterministic per-tenant fixture-admin user id.
///
/// XORing a fixed base with the tenant key yields a stable, distinct id per
/// tenant, so each tenant's `auth_users` creator row is independent under RLS
/// and repeated `ensure_fixture_admin_for` calls stay idempotent.
fn fixture_admin_id(tenant_id: DataTenantId) -> Uuid {
    const BASE: u128 = 0x018f_0000_0000_7000_8000_0000_0000_0001;
    Uuid::from_u128(BASE ^ tenant_id.as_uuid().as_u128())
}

fn role_refs(roles: &[&str]) -> Result<Vec<RoleRef>, WyrdTestServerError> {
    roles
        .iter()
        .map(|role| {
            RoleRef::new(role).map_err(|error| WyrdTestServerError::Auth(error.to_string()))
        })
        .collect()
}

fn card_ref(kind: CardKind, name: &str) -> Result<CardRef, WyrdTestServerError> {
    Ok(CardRef {
        kind,
        name: CardName::new(name).map_err(|error| WyrdTestServerError::Auth(error.to_string()))?,
        version: VersionBlock::parse("1.0.0").expect("static semantic version block is valid"),
        space: SpaceName::new("test").expect("static space name is valid"),
        uid: Some(
            CardUid::new(Uuid::now_v7().to_string()).expect("generated UUIDv7 is a valid CardUid"),
        ),
    })
}

/// Seed the backing Card row for a fixture-created Service or Agent principal.
///
/// # Errors
/// Returns an error when the fixture spec cannot be encoded or Postgres rejects
/// the insert.
async fn seed_machine_card(
    conn: &mut TenantConn<'_>,
    machine_ref: &CardRef,
    creator_id: Uuid,
) -> Result<(), WyrdTestServerError> {
    let spec = match &machine_ref.kind {
        CardKind::Service => machine_card_spec(&machine_ref.kind)?,
        CardKind::Agent => {
            let prompt_ref = card_ref(CardKind::Prompt, &format!("{}-prompt", machine_ref.name))?;
            seed_prompt_card(conn, &prompt_ref, creator_id).await?;
            Spec::from_kind_and_value(
                &CardKind::Agent,
                serde_json::json!({
                    "prompt": prompt_ref,
                }),
            )
            .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?
        }
        other => {
            return Err(WyrdTestServerError::Auth(format!(
                "machine fixture card must be Service or Agent, got {other:?}"
            )));
        }
    };
    insert_fixture_card(conn, machine_ref, &spec, creator_id).await
}

/// Seed a minimal Prompt Card for fixture-created Agent principals.
///
/// # Errors
/// Returns an error when the prompt spec cannot be decoded or Postgres rejects
/// the insert.
async fn seed_prompt_card(
    conn: &mut TenantConn<'_>,
    card_ref: &CardRef,
    creator_id: Uuid,
) -> Result<(), WyrdTestServerError> {
    let spec = Spec::from_kind_and_value(
        &CardKind::Prompt,
        serde_json::json!({
            "provider": "openai",
            "model": "gpt-4o-mini",
            "messages": "Fixture agent."
        }),
    )
    .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?;
    insert_fixture_card(conn, card_ref, &spec, creator_id).await
}

/// Insert a fixture Card row inside the caller's tenant transaction.
///
/// # Errors
/// Returns an error when canonical hashing, JSON encoding, or the SQL insert
/// fails.
async fn insert_fixture_card(
    conn: &mut TenantConn<'_>,
    card_ref: &CardRef,
    spec: &Spec,
    creator_id: Uuid,
) -> Result<(), WyrdTestServerError> {
    let (spec_hash, _) = spec
        .canonical_hash_with_bytes()
        .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?;
    let spec_json =
        serde_json::to_value(spec).map_err(|error| WyrdTestServerError::Auth(error.to_string()))?;
    let uid = card_ref
        .uid
        .as_ref()
        .ok_or_else(|| WyrdTestServerError::Auth("machine CardRef must carry a uid".to_owned()))?;

    sqlx::query(
        r#"
        INSERT INTO wyrd.cards (
            card_uid,
            data_tenant_id,
            kind,
            space,
            name,
            version,
            spec,
            spec_hash,
            artifact_hash,
            labels,
            annotations,
            status,
            created_by
        )
        VALUES (
            $1,
            wyrd.current_tenant(),
            $2,
            $3,
            $4,
            $5,
            $6,
            $7,
            NULL,
            '{}'::jsonb,
            '{}'::jsonb,
            'active',
            $8
        )
        "#,
    )
    .bind(uid.as_uuid())
    .bind(card_ref.kind.wire_name())
    .bind(card_ref.space.as_str())
    .bind(card_ref.name.as_str())
    .bind(card_ref.version.as_str())
    .bind(spec_json)
    .bind(spec_hash.as_str())
    .bind(creator_id)
    .execute(&mut **conn.transaction())
    .await
    .map_err(sql)?;

    Ok(())
}

/// Build the minimal fixture Card spec for a Service principal.
///
/// # Errors
/// Returns an error when the requested kind is not service-principal-backed.
fn machine_card_spec(kind: &CardKind) -> Result<Spec, WyrdTestServerError> {
    let value = match kind {
        CardKind::Service => serde_json::json!({}),
        other => {
            return Err(WyrdTestServerError::Auth(format!(
                "machine fixture card must be Service, got {other:?}"
            )));
        }
    };

    Spec::from_kind_and_value(kind, value)
        .map_err(|error| WyrdTestServerError::Auth(error.to_string()))
}

async fn parse_success<T>(response: Response<Body>) -> Result<T, WyrdTestServerError>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .map_err(|error| WyrdTestServerError::Io(error.to_string()))?;
    if !status.is_success() {
        let body = String::from_utf8_lossy(&body).into_owned();
        let code = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("code")
                    .and_then(|code| code.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "unknown".to_owned());
        return Err(WyrdTestServerError::Http { status, code, body });
    }
    serde_json::from_slice(&body).map_err(|error| WyrdTestServerError::Io(error.to_string()))
}

fn sql(error: impl std::fmt::Display) -> WyrdTestServerError {
    WyrdTestServerError::Sql(error.to_string())
}

/// Constructs the sole Redux catalog over the fixture's Vala tenant connection owner.
///
/// The returned catalog shares the fixture's migrated Postgres lifetime and the
/// caller-provided storage backend, matching the server's production ownership shape.
///
/// # Errors
///
/// Returns [`WyrdTestServerError::Start`] when the Redux catalog cannot connect
/// to its SQL catalog or construct its storage-backed Iceberg catalog.
pub(crate) async fn test_catalog(
    fixture: &PgFixture,
    storage: Arc<vala_bifrost_redux::storage::BifrostStorage>,
) -> Result<Arc<BifrostCatalog>, WyrdTestServerError> {
    let catalog = BifrostCatalog::new(
        fixture.catalog_dsn().expose_secret(),
        storage,
        fixture.vala_postgres().clone(),
    )
    .await
    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
    Ok(Arc::new(catalog))
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")
    )
}

/// Builds a control-plane storage owner over one already-built backend handle.
///
/// Used by cluster and Forge harnesses that need a catalog handle before any
/// node has composed its resources. It allocates no metadata cache, because a
/// control-plane catalog reads no hot Parquet footer; each simulated node still
/// constructs its own Oracle-funded owner when it starts.
///
/// # Panics
/// Panics when the default storage policy does not resolve, which would mean
/// the shipped defaults are themselves invalid.
#[must_use]
pub(crate) fn test_storage_owner(
    storage: &Arc<wyrd_storage::StorageHandle>,
) -> Arc<vala_bifrost_redux::storage::BifrostStorage> {
    Arc::new(vala_bifrost_redux::storage::BifrostStorage::new(
        Arc::clone(storage),
        vala_bifrost_redux::storage::BifrostStoragePolicy::resolve(
            vala_bifrost_redux::storage::BifrostStorageConfig::default(),
            u64::from(u32::MAX),
            false,
        )
        .expect("the default storage policy is valid"),
        None,
    ))
}

/// Build a [`ServerPostgres`] from an already-started [`PgFixture`].
///
/// Clones the fixture's real, migration-ready `WyrdPostgres` and `ValaPostgres`
/// handles. Use this wherever a test needs an [`AppState`] backed by a real
/// fixture database rather than a lazy no-op pool.
#[must_use]
pub fn server_postgres_from_fixture(fixture: &PgFixture) -> ServerPostgres {
    ServerPostgres::from_parts(
        fixture.wyrd_postgres().clone(),
        fixture.vala_postgres().clone(),
    )
}

/// Proves the test server boots on the one production resource composition.
#[cfg(test)]
mod production_composition_tests {
    use vala_bifrost_redux::resources::{
        BifrostResourcePolicy, BifrostRuntimeResources, ResourceSource, SystemResourceSnapshot,
    };

    use super::{BifrostRuntimeRole, BifrostTarget, WyrdTestServerBuilder};

    /// Injected raw observations reach production composition unmodified.
    ///
    /// The harness supplies only raw process-visible observations and the
    /// enabled roles. Every reserve, floor, elastic, scratch, and partition
    /// number the booted server holds must therefore equal the plan
    /// [`BifrostRuntimeResources`] derives from the same observation, proving
    /// the harness derives no allocator output of its own.
    #[test]
    fn test_server_uses_one_bifrost_composer_and_lifecycle() {
        let observation = SystemResourceSnapshot {
            memory_limit_bytes: 3 * 1024 * 1024 * 1024,
            effective_cpu: 6,
            scratch_capacity_bytes: 4 * 1024 * 1024 * 1024,
            scratch_available_bytes: 4 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        };
        let scratch = tempfile::tempdir().expect("Oracle scratch root");
        let composed = BifrostRuntimeResources::from_snapshot(
            observation,
            BifrostResourcePolicy {
                roles: [vala_bifrost_redux::resources::BifrostRole::Oracle]
                    .into_iter()
                    .collect(),
                memory_limit_bytes: None,
                unmanaged_reserve_bytes: None,
                scratch_limit_bytes: None,
                effective_cpu: None,
                oracle_query_slot_limit: None,
                forge_compaction_memory_limit_bytes: None,
                scratch_root: scratch.path().to_owned(),
                volume_roots: None,
            },
        )
        .expect("production resource composition accepts the injected observation")
        .compose_roles()
        .expect("the production composer issues the selected role capability");

        assert!(
            composed.oracle().is_some(),
            "an Oracle node must receive its narrow Oracle capability"
        );
        assert!(
            composed.scribe().is_none(),
            "composition must not issue capabilities for unselected roles"
        );
        let sources = composed.sources();
        assert_eq!(sources.memory, ResourceSource::Injected);
        assert_eq!(sources.cpu, ResourceSource::Injected);

        let builder = WyrdTestServerBuilder::default();
        assert_eq!(builder.forge_process_role, BifrostTarget::All);
        assert_eq!(
            builder.bifrost_roles,
            [
                BifrostRuntimeRole::Scribe,
                BifrostRuntimeRole::ForgeCoordinator,
                BifrostRuntimeRole::ForgeWorker,
                BifrostRuntimeRole::Oracle,
            ]
            .into_iter()
            .collect()
        );
        assert!(
            builder.bind_addrs.is_none(),
            "the harness must let the production server path bind and own its listeners"
        );
    }
}
