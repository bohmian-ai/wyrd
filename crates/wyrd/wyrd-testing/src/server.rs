//! Real-socket Wyrd server test harness.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderValue, Request, Response, StatusCode, header};
use chrono::Duration as ChronoDuration;
use datafusion::execution::memory_pool::MemoryPool;
use ed25519_dalek::VerifyingKey;
use opendal::{Buffer, Entry, Metadata, Operator};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::forge::{
    Forge, ForgeBuildConfig, ForgeConfig, ForgeObjectStore, ForgeRewriteRuntime,
};
use vala_bifrost_redux::maintenance::{StagingFilePublisher, staging_file_channel};
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor;
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use wyrd_auth::exchange_api_key::TokenExchangeSettings;
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
use wyrd_crypt::SecretKey;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::{PrincipalId, RbacCheck, RoleRef};
use wyrd_semver::VersionBlock;
use wyrd_server::boot::build_workload_bindings;
use wyrd_server::boot::issuer::{seed_trusted_issuers, seed_workload_bindings};
use wyrd_server::components::auth::audit_writer::{AuthzAuditWriter, NoopAuthzAuditWriter};
use wyrd_server::config::ServeMode;
use wyrd_server::config::{IssuerEntry, WorkloadBindingEntry};
use wyrd_server::postgres::ServerPostgres;
use wyrd_server::state::BifrostIngestRuntime;
use wyrd_server::{AppState, WyrdServer, WyrdServerConfig, build_router};
use wyrd_spec::DataTenantId;

/// Production-shaped object-store seam for the embedded Forge fixture.
#[derive(Debug)]
struct TestForgeObjectStore {
    operator: Arc<Operator>,
}

impl TestForgeObjectStore {
    /// Retain the server-owned staging operator without changing its behavior.
    fn new(operator: Arc<Operator>) -> Self {
        Self { operator }
    }
}

#[async_trait]
impl ForgeObjectStore for TestForgeObjectStore {
    async fn read(&self, path: &str) -> opendal::Result<Buffer> {
        self.operator.read(path).await
    }

    /// Read exactly the requested byte range through OpenDAL's native reader.
    ///
    /// Forge uses bounded reads for Parquet metadata and row-group admission;
    /// fetching the entire object here would bypass that memory guardrail.
    ///
    /// # Errors
    ///
    /// Returns the OpenDAL error when the reader cannot open or fetch the
    /// requested range.
    async fn read_range(&self, path: &str, range: std::ops::Range<u64>) -> opendal::Result<Buffer> {
        self.operator.reader(path).await?.read(range).await
    }

    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>> {
        self.operator.list_with(prefix).recursive(true).await
    }

    async fn stat(&self, path: &str) -> opendal::Result<Metadata> {
        self.operator.stat(path).await
    }

    async fn delete(&self, path: &str) -> opendal::Result<()> {
        self.operator.delete(path).await
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
    grant_role_to_service_account, grant_role_to_user, insert_api_key, insert_service_account,
    insert_user, revoke_role_from_service_account, revoke_role_from_user, role_by_name,
    trusted_issuer_by_url, workload_binding_by_subject,
};
use wyrd_storage::{BackendConfig, StorageSettings};

use crate::time::ClockHandle;

/// Wyrd server test harness supporting in-process and real-socket modes.
pub struct WyrdTestServer {
    inner: WyrdTestServerInner,
    mode: Mode,
    shutdown_token: Option<CancellationToken>,
    serve_handle: Option<JoinHandle<Result<(), wyrd_server::BootExit>>>,
}

struct WyrdTestServerInner {
    fixture: Arc<PgFixture>,
    // Lifetime guard: only set for local-backend servers; cloud backends need no tempdir.
    #[allow(dead_code)]
    storage_root: Option<Arc<tempfile::TempDir>>,
    _scribe_wal_root: Arc<tempfile::TempDir>,
    /// Lifetime guard for the Forge DataFusion spill directory.
    _forge_spill_root: Arc<tempfile::TempDir>,
    state: AppState,
    router: axum::Router,
    verifier: Arc<TokenVerifier<SqlPermissionResolver, PgIssuerResolver>>,
    issuing_key: Arc<IssuingKey>,
    api_key: SecretString,
    forge_publisher: StagingFilePublisher,
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
    allow_preview_auth: bool,
    storage_settings: Option<StorageSettings>,
    storage_handle: Option<Arc<wyrd_storage::StorageHandle>>,
    access_ttl: Option<ChronoDuration>,
    auth_verify_settings: Option<WyrdAuthVerifySettings>,
    trusted_issuer_configs: Vec<IssuerEntry>,
    workload_binding_configs: Vec<WorkloadBindingEntry>,
    forge_interval: Duration,
    /// Test-tier bin width makes three-file current-day journeys deterministic.
    forge_max_files_per_bin: usize,
    wal_sync_delay: Duration,
    scribe_admission: Option<AdmissionConfig>,
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
            forge_max_files_per_bin: 3,
            wal_sync_delay: Duration::ZERO,
            scribe_admission: None,
        }
    }
}

/// Result of a fixture-path principal bootstrap.
pub use crate::env::Bootstrap;

/// Result of an authz-check request.
pub use crate::env::CheckResult;

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
    /// # Errors
    /// Returns an error if the serve task join times out.
    pub async fn shutdown(mut self) -> Result<(), WyrdTestServerError> {
        if let Some(token) = self.shutdown_token.take() {
            token.cancel();
        }
        if let Some(handle) = self.serve_handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }
        Ok(())
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

    /// Flush the server-owned Scribe through its normal post-commit seal path.
    ///
    /// This is intentionally test-tier only: production callers use the
    /// generation and shutdown coordinators rather than reaching into Scribe.
    ///
    /// # Errors
    /// Returns an error when the server has no Scribe or the seal transaction
    /// cannot be committed.
    pub async fn flush_bifrost(&self) -> Result<(), WyrdTestServerError> {
        let conn = self.tenant_conn().await?;
        self.inner
            .state
            .flush_scribe_for_test(conn)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
    }

    /// Flush the server-owned Scribe through a specific tenant's seal path.
    ///
    /// # Errors
    /// Returns an error when the tenant connection or seal transaction fails.
    pub async fn flush_bifrost_for_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<(), WyrdTestServerError> {
        let conn = self.tenant_conn_for(tenant).await?;
        self.inner
            .state
            .flush_scribe_for_test(conn)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))
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
            Mode::Bound { grpc_addr, .. } => Some(format!("http://{grpc_addr}")),
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

    /// Return the publisher paired with this server's Forge inbox.
    #[must_use]
    pub fn forge_publisher(&self) -> StagingFilePublisher {
        self.inner.forge_publisher.clone()
    }

    /// Return the Scribe retained by this server's production ingest runtime.
    #[must_use]
    pub fn bifrost_scribe(&self) -> Option<Arc<ScribeImpl>> {
        self.inner.state.bifrost_scribe_for_test().cloned()
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

    /// Bind an already-constructed server to OS-assigned HTTP and gRPC ports.
    pub(crate) async fn bind(mut self) -> Result<WyrdTestServer, WyrdTestServerError> {
        // Inject a known shutdown token so the harness can stop the real server;
        // `BoundServer::run` observes `state.shutdown_token`.
        let shutdown_token = CancellationToken::new();
        let state = self
            .inner
            .state
            .clone()
            .with_shutdown_token(shutdown_token.clone());

        // Ephemeral ports, metrics off (the recorder is a process-global
        // singleton that must not be installed per-test), reflection off.
        let loopback = "127.0.0.1:0"
            .parse()
            .expect("static loopback socket addr is valid");
        let mut config = WyrdServerConfig::default();
        config.http.bind = loopback;
        config.grpc.bind = loopback;
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

        let serve_handle = wyrd_runtime::runtime().spawn(async move { bound.run().await });

        self.shutdown_token = Some(shutdown_token);
        self.serve_handle = Some(serve_handle);
        self.mode = Mode::Bound {
            addr,
            base_url: base_url.clone(),
            grpc_addr,
        };

        if let Err(error) = wait_for_ready(&base_url).await {
            if let Some(handle) = self.serve_handle.take()
                && handle.is_finished()
                && let Ok(Err(exit)) = handle.await
            {
                return Err(WyrdTestServerError::Start(format!(
                    "bound Wyrd test server exited before readiness: {exit:?}"
                )));
            }
            return Err(error);
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
            let handle = wyrd_storage::StorageHandle::from_settings(StorageSettings {
                backend: BackendConfig::Local {
                    root: root.path().to_path_buf(),
                },
                require_encryption: false,
                presign_ttl: Duration::from_secs(600),
                part_size_bytes: 16 * 1024 * 1024,
                multipart_threshold_bytes: 100 * 1024 * 1024,
                public_base_url: Some("https://wyrd.test".to_owned()),
            })
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
            (Some(Arc::new(root)), handle)
        };
        let bifrost = test_catalog(&fixture, &storage).await?;
        let bifrost_redux = test_redux_catalog(&fixture, &storage).await?;

        self.start_with_resources(fixture, storage, bifrost, bifrost_redux, storage_root)
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
    /// Returns an error when fixture resources, authentication state, Forge,
    /// Scribe, or the application router cannot be constructed.
    pub(crate) async fn start_with_resources(
        self,
        fixture: Arc<PgFixture>,
        storage: Arc<wyrd_storage::StorageHandle>,
        bifrost: Arc<WyrdCatalog>,
        bifrost_redux: Arc<BifrostCatalog>,
        storage_root: Option<Arc<tempfile::TempDir>>,
    ) -> Result<WyrdTestServer, WyrdTestServerError> {
        // Keep one governor and one DataFusion pool in this graph. Forge and
        // AppState must observe the same pool so query and rewrite admission
        // share accounting rather than silently creating independent budgets.
        let tenant_id = fixture.data_tenant_id();

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
            fixture.app_pool().clone(),
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
            fixture.app_pool(),
            tenant_id,
            &self.trusted_issuer_configs,
            Some(sealing_key.as_ref()),
        )
        .await
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let bindings = build_workload_bindings(&self.workload_binding_configs, tenant_id)
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        seed_workload_bindings(fixture.app_pool(), tenant_id, &bindings)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;

        let issuer_resolver = Arc::new(PgIssuerResolver::new(
            Arc::new(fixture.app_pool().clone()),
            Some(Arc::clone(&sealing_key)),
        ));
        let binding_resolver = Arc::new(PgWorkloadBindingResolver::new(Arc::new(
            fixture.app_pool().clone(),
        )));

        let verifier = Arc::new(
            TokenVerifier::new(decoding_keys, "wyrd", resolver, verify_settings)
                .with_revocation(Arc::new(SqlRevocationCheck::new_with_ttl(
                    Arc::new(fixture.app_pool().clone()),
                    Duration::ZERO,
                )))
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

        let postgres = Arc::new(ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let operator_pool = postgres.operator_pool().ok_or_else(|| {
            WyrdTestServerError::Start(
                "test server requires a platform-admin operator pool for Forge".to_owned(),
            )
        })?;
        let scribe_admission = self.scribe_admission.unwrap_or_default();
        let bifrost_memory = BifrostMemoryGovernor::new_with_scribe_limit(
            scribe_admission.memory_limit_bytes,
            scribe_admission.scribe_memory_limit_bytes,
        )
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let forge_config = ForgeConfig {
            max_files_per_bin: self.forge_max_files_per_bin,
            ..ForgeConfig::default()
        };
        let (forge_publisher, forge_inbox) = staging_file_channel(forge_config.max_hints_per_wake)
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let query_memory: Arc<dyn MemoryPool> = Arc::new(
            vala_bifrost_redux::scribe::memory::BifrostDataFusionMemoryPool::new(
                bifrost_memory.clone(),
            ),
        );
        let spill_root = Arc::new(
            tempfile::tempdir().map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
        );
        let forge_runtime = ForgeRewriteRuntime::new(
            Arc::clone(&query_memory),
            spill_root.path(),
            forge_config.spill_limit_bytes,
        )
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let staging = Arc::new(storage.operator().clone());
        let object_store: Arc<dyn ForgeObjectStore> =
            Arc::new(TestForgeObjectStore::new(Arc::clone(&staging)));
        let forge = Arc::new(
            Forge::new(ForgeBuildConfig {
                vala: postgres.vala().clone(),
                operator_pool,
                catalog: bifrost_redux.iceberg_catalog(),
                staging,
                object_store,
                rewrite_runtime: forge_runtime,
                hints: forge_inbox,
                config: forge_config,
                maintenance_interval: self.forge_interval,
            })
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
        );
        let scribe_wal_root = Arc::new(
            tempfile::tempdir().map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
        );
        let node_id = Uuid::now_v7();
        let wal = Arc::new(
            WalWriter::new(
                scribe_wal_root.path(),
                *node_id.as_bytes(),
                1,
                WalConfig::default(),
            )
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?,
        );
        let scribe = if self.wal_sync_delay.is_zero() {
            ScribeImpl::new_for_embedded_with_runtime_config_and_admission_and_memory(
                Arc::new(storage.operator().clone()),
                wal,
                &node_id.to_string(),
                1,
                vala_bifrost_redux::scribe::ScribeEmbeddedConfig {
                    lane_config: vala_bifrost_redux::scribe::ScribeLaneConfig::default(),
                    admission: scribe_admission,
                    coordination_runtime: tokio::runtime::Handle::current(),
                    memory_budget: Some(bifrost_memory.scribe_budget()),
                    staging_file_publisher: Some(forge_publisher.clone()),
                },
            )
        } else {
            ScribeImpl::try_new_for_embedded_with_wal_sync_delay_and_admission_and_memory(
                Arc::new(storage.operator().clone()),
                wal,
                &node_id.to_string(),
                1,
                self.wal_sync_delay,
                vala_bifrost_redux::scribe::ScribeEmbeddedConfig {
                    lane_config: vala_bifrost_redux::scribe::ScribeLaneConfig::resolved(),
                    admission: scribe_admission,
                    coordination_runtime: tokio::runtime::Handle::current(),
                    memory_budget: Some(bifrost_memory.scribe_budget()),
                    staging_file_publisher: Some(forge_publisher.clone()),
                },
            )
            .map_err(WyrdTestServerError::Start)?
        };
        let scribe = Arc::new(scribe);
        let ingest = Arc::new(BifrostIngestRuntime::new(
            scribe,
            Arc::clone(&bifrost_redux),
            Arc::clone(&verifier),
            vala_bifrost_redux::gate::limits::IngestLimits::default(),
            None,
        ));
        let mut state = AppState::new(postgres, storage, bifrost)
            .with_bifrost_redux(bifrost_redux)
            .with_bifrost_memory_pool(bifrost_memory, query_memory)
            .with_forge(forge)
            .with_bifrost_ingest(ingest)
            .with_auth(wyrd_server::components::auth::ServerAuth {
                allow_preview: self.allow_preview_auth,
                issuing_key: Some(Arc::clone(&issuing_key)),
                token_verifier: Some(Arc::clone(&verifier)),
                token_exchange_settings: exchange_settings,
                trusted_issuer_resolver: Some(issuer_resolver),
                workload_binding_resolver: Some(binding_resolver),
                sealing_key: Some(sealing_key),
            });
        state.authz.permission_check = Arc::new(RbacCheck);
        state.authz.audit_writer = self
            .audit_writer
            .unwrap_or_else(|| Arc::new(NoopAuthzAuditWriter));
        if let Some(hook) = self.policy_hook {
            state.authz.policy_hook = hook;
        }
        let router = build_router(state.clone());

        Ok(WyrdTestServer {
            inner: WyrdTestServerInner {
                fixture,
                storage_root,
                _scribe_wal_root: scribe_wal_root,
                _forge_spill_root: spill_root,
                state,
                router,
                verifier,
                issuing_key,
                api_key: SecretString::from(String::new()),
                forge_publisher,
            },
            mode: Mode::InProcess,
            shutdown_token: None,
            serve_handle: None,
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
    /// Returns an error when startup or socket binding fails.
    pub async fn start_bound(self) -> Result<WyrdTestServer, WyrdTestServerError> {
        let srv = self.start_in_process().await?;
        srv.bind().await
    }
}

async fn wait_for_ready(base_url: &str) -> Result<(), WyrdTestServerError> {
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

pub(crate) async fn test_catalog(
    fixture: &PgFixture,
    storage: &Arc<wyrd_storage::StorageHandle>,
) -> Result<Arc<WyrdCatalog>, WyrdTestServerError> {
    let catalog = WyrdCatalog::new(
        fixture.catalog_dsn().expose_secret(),
        storage.backend_config(),
        Arc::new(fixture.app_pool().clone()),
    )
    .await
    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
    Ok(Arc::new(catalog))
}

pub(crate) async fn test_redux_catalog(
    fixture: &PgFixture,
    storage: &Arc<wyrd_storage::StorageHandle>,
) -> Result<Arc<BifrostCatalog>, WyrdTestServerError> {
    BifrostCatalog::new(
        fixture.catalog_dsn().expose_secret(),
        storage.backend_config(),
        fixture.vala_postgres().clone(),
    )
    .await
    .map(Arc::new)
    .map_err(|error| WyrdTestServerError::Start(error.to_string()))
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")
    )
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
