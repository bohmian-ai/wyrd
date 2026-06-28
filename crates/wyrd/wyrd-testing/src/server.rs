//! Real-socket Wyrd server test harness.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{HeaderValue, Request, Response, StatusCode, header};
use chrono::Duration as ChronoDuration;
use ed25519_dalek::VerifyingKey;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;
use wyrd_auth_check::{AuthzCheckRequest, AuthzCheckResponse, PolicyHook};
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_oidc::{JwksCache, TrustedIssuer, TrustedIssuerRegistry};
use wyrd_auth_verify::{
    Kid, PrincipalKindWire, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings,
    public_key_from_pem,
};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::{PrincipalId, RbacCheck, RoleRef};
use wyrd_semver::VersionBlock;
use wyrd_server::auth::audit_writer::{AuthzAuditWriter, NoopAuthzAuditWriter};
use wyrd_server::auth::exchange_api_key::TokenExchangeSettings;
use wyrd_server::auth::issue_api_key::WyrdApiKey;
use wyrd_server::auth::jwt_bearer::WorkloadBindingRegistry;
use wyrd_server::auth::permission_resolver::SqlPermissionResolver;
use wyrd_server::auth::revocation_resolver::SqlRevocationCheck;
use wyrd_server::auth::seed::seed_builtin_roles_for_tenant;
use wyrd_server::boot::build_workload_bindings;
use wyrd_server::config::{IssuerEntry, WorkloadBindingEntry};
use wyrd_server::issuer_boot::ConfigFileIssuerResolver;
use wyrd_server::{AppState, build_router};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    RequestedSubject, SecretBearer, SubjectTokenType, TokenRequest, TokenResponse,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    grant_role_to_service_account, grant_role_to_user, insert_api_key, insert_service_account,
    insert_user, revoke_role_from_service_account, revoke_role_from_user, role_by_name,
};
use wyrd_storage::{BackendConfig, StorageSettings};

use crate::time::ClockHandle;

/// Wyrd server test harness supporting in-process and real-socket modes.
pub struct WyrdTestServer {
    inner: WyrdTestServerInner,
    mode: Mode,
    shutdown_token: Option<CancellationToken>,
    serve_handle: Option<JoinHandle<()>>,
}

struct WyrdTestServerInner {
    fixture: PgFixture,
    // Lifetime guard: only set for local-backend servers; cloud backends need no tempdir.
    #[allow(dead_code)]
    storage_root: Option<tempfile::TempDir>,
    state: AppState,
    router: axum::Router,
    verifier: Arc<TokenVerifier<SqlPermissionResolver>>,
    issuing_key: Arc<IssuingKey>,
    api_key: SecretString,
}

enum Mode {
    InProcess,
    Bound {
        addr: std::net::SocketAddr,
        base_url: String,
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
    trusted_issuer_registry: Option<Arc<TrustedIssuerRegistry>>,
    workload_binding_registry: Option<Arc<WorkloadBindingRegistry>>,
    trusted_issuer_configs: Vec<IssuerEntry>,
    workload_binding_configs: Vec<WorkloadBindingEntry>,
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
            trusted_issuer_registry: None,
            workload_binding_registry: None,
            trusted_issuer_configs: Vec::new(),
            workload_binding_configs: Vec::new(),
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
    ///
    /// The harness binds a single TCP socket today; this derives the gRPC URL
    /// from that same address so `WYRD_GRPC_URL` and `WYRD_SERVER_URL` point
    /// to the same host:port until the harness grows a dedicated gRPC listener.
    #[must_use]
    pub fn grpc_url(&self) -> Option<String> {
        match &self.mode {
            Mode::Bound { addr, .. } => Some(format!("http://{addr}")),
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

    /// Wire trusted OIDC issuers after server startup.
    ///
    /// Use this when the tenant ID is not known until after [`start_in_process`]
    /// returns (the common case): call `data_tenant_id()`, build your
    /// [`TrustedIssuer`] entries, then call this method to rebuild the router
    /// with the live registry.
    pub fn wire_trusted_issuers(&mut self, issuers: Vec<TrustedIssuer>) {
        let registry = Arc::new(TrustedIssuerRegistry::from_issuers(issuers));
        self.inner.state = self
            .inner
            .state
            .clone()
            .with_trusted_issuer_registry(registry);
        self.inner.router = build_router(self.inner.state.clone());
    }

    /// Wire a workload binding registry after server startup.
    pub fn wire_workload_bindings(&mut self, registry: Arc<WorkloadBindingRegistry>) {
        self.inner.state = self
            .inner
            .state
            .clone()
            .with_workload_binding_registry(registry);
        self.inner.router = build_router(self.inner.state.clone());
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
            kind: PrincipalKindWire::User,
            tenant_id: self.data_tenant_id(),
            card_ref: None,
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
        let creator_id = self.ensure_fixture_admin().await?;
        let mut conn = self.tenant_conn().await?;
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
        let principal_id = Uuid::now_v7();
        let creator_id = self.ensure_fixture_admin().await?;
        let card_ref = card_ref(card_kind, name)?;
        let api_key = WyrdApiKey::generate(self.data_tenant_id());
        let raw = api_key.secret.clone();
        let key_hash = tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw))
            .await
            .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?
            .map_err(|error| WyrdTestServerError::Auth(error.to_string()))?;

        let mut conn = self.tenant_conn().await?;
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

    async fn ensure_fixture_admin(&self) -> Result<Uuid, WyrdTestServerError> {
        let id = Uuid::from_u128(0x018f0000000070008000000000000001);
        let mut conn = self.tenant_conn().await?;
        insert_user(
            &mut conn,
            id,
            Some("fixture-admin@test.wyrd"),
            "password",
            None,
        )
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

    /// Pre-wire a trusted OIDC issuer registry into the server state.
    ///
    /// Use when the registry contents can be determined before startup (e.g.
    /// the tenant IDs are known in advance). For tests that need the live
    /// tenant ID, use [`WyrdTestServer::wire_trusted_issuers`] after startup.
    #[must_use]
    pub fn with_trusted_issuer_registry(mut self, registry: Arc<TrustedIssuerRegistry>) -> Self {
        self.trusted_issuer_registry = Some(registry);
        self
    }

    /// Pre-wire a workload binding registry into the server state.
    #[must_use]
    pub fn with_workload_binding_registry(
        mut self,
        registry: Arc<WorkloadBindingRegistry>,
    ) -> Self {
        self.workload_binding_registry = Some(registry);
        self
    }

    /// Boot trusted OIDC issuers from `[[trusted_issuers]]` config DTOs.
    ///
    /// At [`Self::start_in_process`] these run through the production
    /// [`ConfigFileIssuerResolver`] + per-issuer OIDC discovery, building the
    /// [`TrustedIssuerRegistry`] (and the verifier's external path) exactly the
    /// way a self-hosted deployment boots. Every entry binds to the fixture's
    /// implicit `DataTenantId`. Boot fails closed if discovery is unreachable.
    #[must_use]
    pub fn with_trusted_issuer_configs(mut self, configs: Vec<IssuerEntry>) -> Self {
        self.trusted_issuer_configs = configs;
        self
    }

    /// Boot workload bindings from `[[workload_bindings]]` config DTOs.
    ///
    /// At [`Self::start_in_process`] these run through the production
    /// [`build_workload_bindings`] boot path into the [`WorkloadBindingRegistry`],
    /// each bound to the fixture's implicit `DataTenantId`.
    #[must_use]
    pub fn with_workload_binding_configs(mut self, configs: Vec<WorkloadBindingEntry>) -> Self {
        self.workload_binding_configs = configs;
        self
    }

    /// Build and start an in-process server.
    ///
    /// # Errors
    /// Returns an error when database, storage, auth, or router state cannot be created.
    pub async fn start_in_process(self) -> Result<WyrdTestServer, WyrdTestServerError> {
        let fixture = PgFixture::start()
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        let tenant_id = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.map_err(sql)?;
        seed_builtin_roles_for_tenant(&mut conn, tenant_id)
            .await
            .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
        conn.commit().await.map_err(sql)?;

        let (storage_root, storage) = if let Some(handle) = self.storage_handle {
            (None, handle)
        } else if let Some(settings) = self.storage_settings {
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
            (Some(root), handle)
        };

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

        // Config-driven boot: resolve `[[trusted_issuers]]` through the real
        // ConfigFileIssuerResolver + per-issuer OIDC discovery, bound to the
        // fixture's implicit tenant. Done before the verifier is built so the
        // verifier's external (foreign-OIDC) path can be wired to the resolved
        // registry — the same shape the callback and jwt-bearer routes require.
        let config_issuer_registry = if self.trusted_issuer_configs.is_empty() {
            None
        } else {
            let issuers = ConfigFileIssuerResolver::new(self.trusted_issuer_configs, tenant_id)
                .resolve()
                .await
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
            Some(Arc::new(TrustedIssuerRegistry::from_issuers(issuers)))
        };

        let verifier_base = TokenVerifier::new(decoding_keys, "wyrd", resolver, verify_settings)
            .with_revocation(Arc::new(SqlRevocationCheck::new_with_ttl(
                Arc::new(fixture.app_pool().clone()),
                Duration::ZERO,
            )));
        let verifier = Arc::new(match &config_issuer_registry {
            Some(registry) => verifier_base.with_external(
                Arc::new(JwksCache::new(
                    reqwest::Client::new(),
                    Duration::from_secs(300),
                    Duration::from_secs(5),
                )),
                Arc::clone(registry),
            ),
            None => verifier_base,
        });

        let exchange_settings = if let Some(ttl) = self.access_ttl {
            TokenExchangeSettings {
                access_ttl: ttl,
                ..TokenExchangeSettings::default()
            }
        } else {
            TokenExchangeSettings::default()
        };

        let mut state = AppState::new(
            fixture.app_pool().clone(),
            Some(fixture.platform_admin_pool().clone()),
            storage,
        )
        .with_preview_auth(self.allow_preview_auth)
        .with_auth_handles(Arc::clone(&issuing_key), Arc::clone(&verifier))
        .with_token_exchange_settings(exchange_settings);
        state.permission_check = Arc::new(RbacCheck);
        state.audit_writer = self
            .audit_writer
            .unwrap_or_else(|| Arc::new(NoopAuthzAuditWriter));
        if let Some(hook) = self.policy_hook {
            state.policy_hook = hook;
        }
        if let Some(registry) = self.trusted_issuer_registry {
            state = state.with_trusted_issuer_registry(registry);
        }
        if let Some(registry) = self.workload_binding_registry {
            state = state.with_workload_binding_registry(registry);
        }
        if let Some(registry) = config_issuer_registry {
            state = state.with_trusted_issuer_registry(registry);
        }
        if !self.workload_binding_configs.is_empty() {
            let bindings = build_workload_bindings(&self.workload_binding_configs, tenant_id)
                .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
            state = state.with_workload_binding_registry(Arc::new(
                WorkloadBindingRegistry::from_bindings(bindings),
            ));
        }
        let router = build_router(state.clone());

        Ok(WyrdTestServer {
            inner: WyrdTestServerInner {
                fixture,
                storage_root,
                state,
                router,
                verifier,
                issuing_key,
                api_key: SecretString::from(String::new()),
            },
            mode: Mode::InProcess,
            shutdown_token: None,
            serve_handle: None,
        })
    }

    /// Build, start, and bind the server to an OS-assigned TCP socket.
    ///
    /// # Errors
    /// Returns an error when startup or socket binding fails.
    pub async fn start_bound(self) -> Result<WyrdTestServer, WyrdTestServerError> {
        let mut srv = self.start_in_process().await?;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| WyrdTestServerError::Bind(e.to_string()))?;
        let addr = listener
            .local_addr()
            .map_err(|e| WyrdTestServerError::Bind(e.to_string()))?;
        let base_url = format!("http://{addr}");

        let shutdown_token = CancellationToken::new();
        let token_clone = shutdown_token.clone();
        let router = srv.inner.router.clone();

        let handle = wyrd_runtime::runtime().spawn(async move {
            let _ = wyrd_server::serve(router, listener, token_clone).await;
        });

        srv.shutdown_token = Some(shutdown_token);
        srv.serve_handle = Some(handle);
        srv.mode = Mode::Bound {
            addr,
            base_url: base_url.clone(),
        };

        wait_for_ready(&base_url).await?;

        Ok(srv)
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

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")
    )
}
