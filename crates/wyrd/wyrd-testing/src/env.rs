//! In-process Wyrd server test environment.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{HeaderValue, Request, Response, StatusCode, header};
use ed25519_dalek::VerifyingKey;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tower::ServiceExt;
use uuid::Uuid;
use vala_bifrost_redux::catalog::BifrostCatalog;
use wyrd_auth::issue_api_key::WyrdApiKey;
use wyrd_auth::permission_resolver::SqlPermissionResolver;
use wyrd_auth::pg_resolvers::PgIssuerResolver;
use wyrd_auth::seed::seed_builtin_roles_for_tenant;
use wyrd_auth_check::{AuthzCheckRequest, AuthzCheckResponse, PolicyHook};
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::{
    Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::{PrincipalId, RbacCheck, RoleRef};
use wyrd_semver::VersionBlock;
use wyrd_server::components::auth::audit_writer::NoopAuthzAuditWriter;
use wyrd_server::postgres::ServerPostgres;
use wyrd_server::{AppState, build_router};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::auth::{
    RequestedSubject, SecretBearer, SubjectTokenType, TokenRequest, TokenResponse,
};
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::request_id::RequestId;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    grant_role_to_service_account, grant_role_to_user, insert_api_key, insert_service_account,
    insert_user, revoke_role_from_service_account, revoke_role_from_user, role_by_name,
};
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

use crate::time::ClockHandle;

/// In-process Wyrd server harness.
pub struct WyrdTestEnv {
    inner: WyrdTestEnvInner,
}

struct WyrdTestEnvInner {
    fixture: PgFixture,
    storage_root: tempfile::TempDir,
    state: AppState,
    router: axum::Router,
    verifier: Arc<TokenVerifier<SqlPermissionResolver, PgIssuerResolver>>,
    issuing_key: Arc<IssuingKey>,
}

/// Test environment errors.
#[derive(Debug, Error)]
pub enum WyrdTestError {
    /// Environment startup failed.
    #[error("test environment failed to start: {0}")]
    Start(String),
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

/// Result of a fixture-path principal bootstrap.
#[derive(Debug, Clone)]
pub enum Bootstrap {
    /// Human User bootstrap.
    User { id: PrincipalId, jwt: String },
    /// Service or Agent bootstrap.
    Machine {
        id: PrincipalId,
        api_key: SecretString,
        card_ref: CardRef,
    },
}

impl Bootstrap {
    /// Return the principal id.
    #[must_use]
    pub fn id(&self) -> PrincipalId {
        match self {
            Self::User { id, .. } | Self::Machine { id, .. } => *id,
        }
    }

    /// Return the card ref for machine principals.
    #[must_use]
    pub fn card_ref(&self) -> Option<&CardRef> {
        match self {
            Self::Machine { card_ref, .. } => Some(card_ref),
            Self::User { .. } => None,
        }
    }

    /// Return the API key for machine principals.
    #[must_use]
    pub fn api_key(&self) -> Option<&SecretString> {
        match self {
            Self::Machine { api_key, .. } => Some(api_key),
            Self::User { .. } => None,
        }
    }
}

/// Result of an authz-check request.
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// HTTP status returned by `/v1/authz/check`.
    pub status: StatusCode,
    /// Request id emitted by the request-id middleware.
    pub wyrd_request_id: RequestId,
    /// Parsed authz-check response body when the route returned a valid JSON payload.
    pub response: Option<AuthzCheckResponse>,
}

impl WyrdTestEnv {
    /// Start an in-process Wyrd server harness.
    ///
    /// # Errors
    /// Returns an error when database, storage, auth, or router state cannot be created.
    pub async fn start() -> Result<Self, WyrdTestError> {
        let fixture = PgFixture::start()
            .await
            .map_err(|error| WyrdTestError::Start(error.to_string()))?;
        let tenant_id = fixture.data_tenant_id();
        let mut conn = fixture.tenant_conn().await.map_err(sql)?;
        seed_builtin_roles_for_tenant(&mut conn, tenant_id)
            .await
            .map_err(|error| WyrdTestError::Start(error.to_string()))?;
        conn.commit().await.map_err(sql)?;

        let storage_root =
            tempfile::tempdir().map_err(|error| WyrdTestError::Start(error.to_string()))?;
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .map_err(|error| WyrdTestError::Start(error.to_string()))?;
        let bifrost = test_catalog(&fixture, &storage).await?;

        let issuing_key = Arc::new(
            IssuingKey::from_ed_pem(
                crate::keys::private_key_pem(),
                Kid::new("test").expect("static kid is valid"),
                "wyrd",
            )
            .map_err(|error| WyrdTestError::Start(error.to_string()))?,
        );
        let mut decoding_keys = HashMap::new();
        decoding_keys.insert(
            Kid::new("test").expect("static kid is valid"),
            Arc::new(
                public_key_from_pem(crate::keys::public_key_pem().as_bytes())
                    .map_err(|error| WyrdTestError::Start(error.to_string()))?,
            ),
        );
        let resolver = Arc::new(SqlPermissionResolver::new(Arc::new(
            fixture.app_pool().clone(),
        )));
        let verifier = Arc::new(TokenVerifier::new(
            decoding_keys,
            "wyrd",
            resolver,
            WyrdAuthVerifySettings::default(),
        ));

        let postgres = Arc::new(ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let mut state = AppState::new(postgres, storage, bifrost).with_auth(
            wyrd_server::components::auth::ServerAuth {
                allow_preview: true,
                issuing_key: Some(Arc::clone(&issuing_key)),
                token_verifier: Some(Arc::clone(&verifier)),
                ..wyrd_server::components::auth::ServerAuth::default()
            },
        );
        state.authz.permission_check = Arc::new(RbacCheck);
        state.authz.audit_writer = Arc::new(NoopAuthzAuditWriter);
        let router = build_router(state.clone());

        Ok(Self {
            inner: WyrdTestEnvInner {
                fixture,
                storage_root,
                state,
                router,
                verifier,
                issuing_key,
            },
        })
    }

    /// Binding a socket is not supported by the in-process test harness.
    ///
    /// # Errors
    /// Always returns [`WyrdTestError::Unsupported`] in this stage.
    pub async fn start_bound() -> Result<Self, WyrdTestError> {
        Err(WyrdTestError::Unsupported(
            "start_bound() is not supported by the in-process test harness".to_owned(),
        ))
    }

    /// Override the default allow policy hook.
    #[must_use]
    pub fn with_policy_hook(mut self, hook: Arc<dyn PolicyHook>) -> Self {
        self.inner.state.authz.policy_hook = hook;
        self.inner.router = build_router(self.inner.state.clone());
        self
    }

    /// Bootstrap a user principal through fixture SQL.
    ///
    /// # Errors
    /// Returns an error when SQL writes or test JWT minting fail.
    pub async fn bootstrap_user(
        &self,
        name: &str,
        roles: &[&str],
    ) -> Result<Bootstrap, WyrdTestError> {
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
            .map_err(|error| WyrdTestError::Auth(error.to_string()))?;
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
    ) -> Result<Bootstrap, WyrdTestError> {
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
    ) -> Result<Bootstrap, WyrdTestError> {
        self.bootstrap_machine(name, roles, CardKind::Agent, "agent")
            .await
    }

    /// Grant a role to a bootstrapped principal.
    ///
    /// # Errors
    /// Returns an error when the role is unknown or SQL fails.
    pub async fn grant_role(&self, principal: &Bootstrap, role: &str) -> Result<(), WyrdTestError> {
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
    ) -> Result<(), WyrdTestError> {
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
    pub async fn exchange_api_key(&self, key: &SecretString) -> Result<String, WyrdTestError> {
        let body = serde_json::to_vec(&TokenRequest::WyrdApiKey {
            api_key: SecretBearer::new(key.expose_secret().to_owned()),
        })
        .map_err(|error| WyrdTestError::Io(error.to_string()))?;
        let response = self
            .raw_call(
                Request::builder()
                    .method("POST")
                    .uri("/auth/token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .map_err(|error| WyrdTestError::Io(error.to_string()))?,
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
    ) -> Result<String, WyrdTestError> {
        let body = serde_json::to_vec(&TokenRequest::TokenExchange {
            subject_token: SecretBearer::new(from_jwt.to_owned()),
            subject_token_type: SubjectTokenType::AccessToken,
            requested_subject: RequestedSubject::CardRef {
                card_ref: target.clone(),
            },
        })
        .map_err(|error| WyrdTestError::Io(error.to_string()))?;
        let response = self
            .raw_call(
                Request::builder()
                    .method("POST")
                    .uri("/auth/token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .map_err(|error| WyrdTestError::Io(error.to_string()))?,
            )
            .await?;
        let token = parse_success::<TokenResponse>(response).await?;
        Ok(token.access_token.expose().to_owned())
    }

    /// Call the in-process router with an access token.
    ///
    /// # Errors
    /// Returns an error if the request cannot be augmented or the router fails.
    pub async fn call(
        &self,
        jwt: &str,
        mut req: Request<Body>,
    ) -> Result<Response<Body>, WyrdTestError> {
        req.headers_mut().insert(
            "x-wyrd-access-token",
            HeaderValue::from_str(&format!("Bearer {jwt}"))
                .map_err(|error| WyrdTestError::Io(error.to_string()))?,
        );
        self.raw_call(req).await
    }

    /// Call `/v1/authz/check` using a delegated token and projected request headers.
    ///
    /// # Errors
    /// Returns an error if request construction or routing fails.
    pub async fn authz_check(
        &self,
        jwt: &str,
        request: AuthzCheckRequest,
    ) -> Result<CheckResult, WyrdTestError> {
        let response = self
            .raw_call(
                Request::builder()
                    .method("POST")
                    .uri("/v1/authz/check")
                    .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&request)
                            .map_err(|error| WyrdTestError::Io(error.to_string()))?,
                    ))
                    .map_err(|error| WyrdTestError::Io(error.to_string()))?,
            )
            .await?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("wyrd-request-id")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| WyrdTestError::Io("missing wyrd-request-id header".to_owned()))
            .and_then(|value| {
                RequestId::parse(value).map_err(|error| WyrdTestError::Io(error.to_string()))
            })?;
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|error| WyrdTestError::Io(error.to_string()))?;
        let check_response: Option<AuthzCheckResponse> = serde_json::from_slice(&body).ok();
        Ok(CheckResult {
            status,
            wyrd_request_id: request_id,
            response: check_response,
        })
    }

    /// Advance the virtual clock.
    pub async fn advance(&self, duration: Duration) {
        self.clock().advance(duration).await;
    }

    /// Return a virtual-clock handle.
    #[must_use]
    pub fn clock(&self) -> ClockHandle {
        ClockHandle::new()
    }

    /// Return the deterministic public verifying key.
    #[must_use]
    pub fn verifying_key(&self) -> &VerifyingKey {
        crate::keys::verifying_key()
    }

    /// Return the fixture tenant id.
    #[must_use]
    pub fn data_tenant_id(&self) -> DataTenantId {
        self.inner.fixture.data_tenant_id()
    }

    /// Return the server state for focused assertions.
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.inner.state
    }

    /// Opens the production tenant transaction wrapper for an explicit test tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the fixture cannot acquire or bind the transaction.
    pub async fn tenant_conn_for(
        &self,
        tenant_id: DataTenantId,
    ) -> Result<TenantConn<'_>, WyrdTestError> {
        self.inner
            .fixture
            .tenant_conn_for(tenant_id)
            .await
            .map_err(sql)
    }

    /// Mints a signed Service token while retaining production verification and SQL resolution.
    ///
    /// # Errors
    ///
    /// Returns an error when a role or token cannot be encoded.
    pub fn issue_service_access_token_for_test(
        &self,
        principal_id: PrincipalId,
        tenant_id: DataTenantId,
        card_ref: CardRef,
        roles: &[&str],
    ) -> Result<String, WyrdTestError> {
        self.inner
            .issuing_key
            .issue_service_access_token(
                principal_id,
                tenant_id,
                card_ref.clone(),
                CardRefScope::own(&card_ref),
                role_refs(roles)?,
                chrono::Duration::minutes(15),
            )
            .map_err(|error| WyrdTestError::Auth(error.to_string()))
    }

    /// Mints a signed User token while retaining production verification and SQL resolution.
    ///
    /// # Errors
    ///
    /// Returns an error when a role or token cannot be encoded.
    pub fn issue_user_access_token_for_test(
        &self,
        principal_id: PrincipalId,
        tenant_id: DataTenantId,
        roles: &[&str],
    ) -> Result<String, WyrdTestError> {
        self.inner
            .issuing_key
            .issue_user_access_token(
                TokenPrincipalRef {
                    id: principal_id,
                    kind: PrincipalKindTag::User,
                    tenant_id,
                    card_ref: None,
                    card_ref_scope: Default::default(),
                },
                role_refs(roles)?,
                chrono::Duration::minutes(15),
            )
            .map_err(|error| WyrdTestError::Auth(error.to_string()))
    }

    /// Seeds one real Service principal and role membership under an explicit tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the card is not a Service or SQL persistence fails.
    pub async fn seed_service_principal_for_test(
        &self,
        tenant_id: DataTenantId,
        card_ref: &CardRef,
        roles: &[&str],
    ) -> Result<PrincipalId, WyrdTestError> {
        if card_ref.kind != CardKind::Service {
            return Err(WyrdTestError::Unsupported(
                "test principal card must be Service".to_owned(),
            ));
        }
        let principal_id = Uuid::now_v7();
        let creator_id = Uuid::now_v7();
        let mut conn = self.tenant_conn_for(tenant_id).await?;
        insert_user(
            &mut conn,
            creator_id,
            Some(&format!("{creator_id}@test.wyrd")),
            "password",
            None,
        )
        .await
        .map_err(sql)?;
        insert_service_account(
            &mut conn,
            principal_id,
            "service",
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
    ) -> Result<Bootstrap, WyrdTestError> {
        let principal_id = Uuid::now_v7();
        let creator_id = self.ensure_fixture_admin().await?;
        let card_ref = card_ref(card_kind, name)?;
        let api_key = WyrdApiKey::generate(self.data_tenant_id());
        let raw = api_key.secret.clone();
        let key_hash = tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw))
            .await
            .map_err(|error| WyrdTestError::Auth(error.to_string()))?
            .map_err(|error| WyrdTestError::Auth(error.to_string()))?;

        let mut conn = self.tenant_conn().await?;
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

    async fn ensure_fixture_admin(&self) -> Result<Uuid, WyrdTestError> {
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

    async fn tenant_conn(&self) -> Result<TenantConn<'_>, WyrdTestError> {
        self.inner.fixture.tenant_conn().await.map_err(sql)
    }

    async fn raw_call(&self, mut req: Request<Body>) -> Result<Response<Body>, WyrdTestError> {
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
            .map_err(|error| WyrdTestError::Io(error.to_string()))
    }
}

impl Drop for WyrdTestEnv {
    fn drop(&mut self) {
        let _ = self.inner.storage_root.path();
    }
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
) -> Result<(), WyrdTestError> {
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

async fn lookup_role_id(conn: &mut TenantConn<'_>, role: &str) -> Result<Uuid, WyrdTestError> {
    role_by_name(conn, role)
        .await
        .map_err(sql)?
        .map(|row| row.id)
        .ok_or_else(|| WyrdTestError::Sql(format!("role not found: {role}")))
}

fn role_refs(roles: &[&str]) -> Result<Vec<RoleRef>, WyrdTestError> {
    roles
        .iter()
        .map(|role| RoleRef::new(role).map_err(|error| WyrdTestError::Auth(error.to_string())))
        .collect()
}

fn card_ref(kind: CardKind, name: &str) -> Result<CardRef, WyrdTestError> {
    Ok(CardRef {
        kind,
        name: CardName::new(name).map_err(|error| WyrdTestError::Auth(error.to_string()))?,
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
) -> Result<(), WyrdTestError> {
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
            .map_err(|error| WyrdTestError::Auth(error.to_string()))?
        }
        other => {
            return Err(WyrdTestError::Auth(format!(
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
) -> Result<(), WyrdTestError> {
    let spec = Spec::from_kind_and_value(
        &CardKind::Prompt,
        serde_json::json!({
            "provider": "openai",
            "model": "gpt-4o-mini",
            "messages": "Fixture agent."
        }),
    )
    .map_err(|error| WyrdTestError::Auth(error.to_string()))?;
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
) -> Result<(), WyrdTestError> {
    let (spec_hash, _) = spec
        .canonical_hash_with_bytes()
        .map_err(|error| WyrdTestError::Auth(error.to_string()))?;
    let spec_json =
        serde_json::to_value(spec).map_err(|error| WyrdTestError::Auth(error.to_string()))?;
    let uid = card_ref
        .uid
        .as_ref()
        .ok_or_else(|| WyrdTestError::Auth("machine CardRef must carry a uid".to_owned()))?;

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

/// Build the minimal fixture Card spec for a Service or Agent principal.
///
/// # Errors
/// Returns an error when the requested kind is not machine-principal-backed or
/// the inline fixture agent prompt cannot be decoded.
fn machine_card_spec(kind: &CardKind) -> Result<Spec, WyrdTestError> {
    let value = match kind {
        CardKind::Service => serde_json::json!({}),
        other => {
            return Err(WyrdTestError::Auth(format!(
                "machine fixture card must be Service or Agent, got {other:?}"
            )));
        }
    };

    Spec::from_kind_and_value(kind, value).map_err(|error| WyrdTestError::Auth(error.to_string()))
}

async fn parse_success<T>(response: Response<Body>) -> Result<T, WyrdTestError>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .map_err(|error| WyrdTestError::Io(error.to_string()))?;
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
        return Err(WyrdTestError::Http { status, code, body });
    }
    serde_json::from_slice(&body).map_err(|error| WyrdTestError::Io(error.to_string()))
}

fn sql(error: impl std::fmt::Display) -> WyrdTestError {
    WyrdTestError::Sql(error.to_string())
}

async fn test_catalog(
    fixture: &PgFixture,
    storage: &Arc<StorageHandle>,
) -> Result<Arc<BifrostCatalog>, WyrdTestError> {
    let catalog = BifrostCatalog::new(
        fixture.catalog_dsn().expose_secret(),
        storage.backend_config(),
        fixture.vala_postgres().clone(),
    )
    .await
    .map_err(|error| WyrdTestError::Start(error.to_string()))?;
    Ok(Arc::new(catalog))
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")
    )
}

#[cfg(test)]
mod pg_tests {
    use secrecy::SecretString;
    use wyrd_auth_check::AuthzCheckRequest;
    use wyrd_auth_verify::{AccessTokenClaims, verify_eddsa};
    use wyrd_runtime::Permission;
    use wyrd_sql::queries::auth::{list_service_account_roles, list_user_roles};

    use super::{Bootstrap, WyrdTestEnv, WyrdTestError};

    #[tokio::test]
    async fn start_bound_returns_unsupported() {
        let Err(error) = WyrdTestEnv::start_bound().await else {
            panic!("bound server is not supported");
        };

        assert!(matches!(error, WyrdTestError::Unsupported(_)));
    }

    #[tokio::test]
    async fn start_seeds_builtin_roles() {
        let env = WyrdTestEnv::start().await.expect("env starts");
        let mut conn = env.tenant_conn().await.expect("tenant conn opens");
        let roles = wyrd_sql::queries::auth::list_roles(&mut conn)
            .await
            .expect("roles list");

        assert_eq!(
            roles.len(),
            wyrd_runtime::builtin_roles::BUILTIN_ROLES.len()
        );
    }

    #[tokio::test]
    async fn bootstrap_user_grants_listed_roles() {
        let env = WyrdTestEnv::start().await.expect("env starts");
        let user = env
            .bootstrap_user("u", &["writer", "reader"])
            .await
            .expect("user bootstraps");
        let mut conn = env.tenant_conn().await.expect("tenant conn opens");
        let roles = list_user_roles(&mut conn, user.id().as_uuid())
            .await
            .expect("roles list");

        assert_eq!(roles, ["reader", "writer"]);
    }

    #[tokio::test]
    async fn bootstrap_service_round_trip_through_real_token_route() {
        let env = WyrdTestEnv::start().await.expect("env starts");
        let service = env
            .bootstrap_service("svc", &["writer"])
            .await
            .expect("service bootstraps");
        let jwt = env
            .exchange_api_key(service.api_key().expect("machine has key"))
            .await
            .expect("key exchanges");
        let verified = env
            .state()
            .auth
            .token_verifier
            .as_ref()
            .expect("verifier exists")
            .verify(&SecretString::from(jwt), &env.data_tenant_id())
            .await
            .expect("jwt verifies");

        assert!(
            verified
                .principal
                .effective_permissions
                .contains(&Permission::card_write())
        );
    }

    #[tokio::test]
    async fn grant_then_revoke_force_recheck_flow() {
        let env = WyrdTestEnv::start().await.expect("env starts");
        let service = env
            .bootstrap_service("mutator", &[])
            .await
            .expect("service bootstraps");
        let initial_jwt = env
            .exchange_api_key(service.api_key().expect("machine has key"))
            .await
            .expect("key exchanges");

        env.grant_role(&service, "writer")
            .await
            .expect("role grants");
        env.force_recheck(&initial_jwt).await;
        let jwt = env
            .exchange_api_key(service.api_key().expect("machine has key"))
            .await
            .expect("key re-exchanges after grant");
        let verified = env
            .state()
            .auth
            .token_verifier
            .as_ref()
            .expect("verifier exists")
            .verify(&SecretString::from(jwt.clone()), &env.data_tenant_id())
            .await
            .expect("jwt verifies after grant");
        assert!(
            verified
                .principal
                .effective_permissions
                .contains(&Permission::card_write())
        );

        env.revoke_role(&service, "writer")
            .await
            .expect("role revokes");
        env.force_recheck_principal(&service).await;
        let jwt = env
            .exchange_api_key(service.api_key().expect("machine has key"))
            .await
            .expect("key re-exchanges after revoke");
        let verified = env
            .state()
            .auth
            .token_verifier
            .as_ref()
            .expect("verifier exists")
            .verify(&SecretString::from(jwt), &env.data_tenant_id())
            .await
            .expect("jwt verifies after revoke");
        assert!(
            !verified
                .principal
                .effective_permissions
                .contains(&Permission::card_write())
        );
    }

    #[tokio::test]
    async fn delegate_round_trip() {
        let env = WyrdTestEnv::start().await.expect("env starts");
        let caller = env
            .bootstrap_service("caller", &["runtime_admin"])
            .await
            .expect("caller bootstraps");
        let callee = env
            .bootstrap_agent("callee", &["writer"])
            .await
            .expect("callee bootstraps");
        let caller_jwt = env
            .exchange_api_key(caller.api_key().expect("machine has key"))
            .await
            .expect("caller key exchanges");
        let delegated = env
            .delegate(
                &caller_jwt,
                callee.card_ref().expect("machine has card ref"),
            )
            .await
            .expect("delegates");

        let claims = verify_eddsa::<AccessTokenClaims>(
            &delegated,
            &wyrd_auth_verify::public_key_from_pem(crate::keys::public_key_pem().as_bytes())
                .expect("public key loads"),
            Some("wyrd"),
        )
        .expect("delegated jwt verifies");
        let act = claims.act.expect("delegated token has actor chain");

        assert_eq!(act.sub, caller.id().to_string());
    }

    #[tokio::test]
    async fn authz_check_returns_request_id() {
        let env = WyrdTestEnv::start().await.expect("env starts");
        let caller = env
            .bootstrap_service("authz-caller", &["runtime_admin"])
            .await
            .expect("caller bootstraps");
        let callee = env
            .bootstrap_service("authz-callee", &["writer"])
            .await
            .expect("callee bootstraps");
        let caller_jwt = env
            .exchange_api_key(caller.api_key().expect("machine has key"))
            .await
            .expect("caller key exchanges");
        let delegated = env
            .delegate(
                &caller_jwt,
                callee.card_ref().expect("machine has card ref"),
            )
            .await
            .expect("delegates");

        let result = env
            .authz_check(
                &delegated,
                AuthzCheckRequest {
                    target: callee.card_ref().expect("machine has card ref").clone(),
                    action: "card_write".to_owned(),
                    context: serde_json::json!({}),
                },
            )
            .await
            .expect("authz check calls");

        assert_eq!(result.status, http::StatusCode::OK);
    }

    #[tokio::test]
    async fn machine_roles_are_granted() {
        let env = WyrdTestEnv::start().await.expect("env starts");
        let Bootstrap::Machine { id, .. } = env
            .bootstrap_agent("role-agent", &["writer"])
            .await
            .expect("agent bootstraps")
        else {
            panic!("agent bootstrap returns machine");
        };
        let mut conn = env.tenant_conn().await.expect("tenant conn opens");
        let roles = list_service_account_roles(&mut conn, id.as_uuid())
            .await
            .expect("roles list");

        assert_eq!(roles, ["writer"]);
    }
}
