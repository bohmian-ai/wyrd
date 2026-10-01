//! Tenant human OIDC connection administration: `/v1/identity/oidc/*`.
//!
//! Thin HTTP adapters over [`HumanConnections`], the one owner of a tenant's
//! human login trust. The tenant is always the bearer's tenant; no route takes
//! a tenant identifier. Every route requires `identity_connections:write`, and
//! its verdict is audited before any provider IO or store read: a denial is
//! recorded standalone and refused, and the allowed decision is appended in the
//! same tenant transaction as the state change it authorizes. A refused caller
//! therefore sees neither connection metadata nor secrets, and no secret ever
//! appears in a response, error, log, or audit row.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wyrd_auth::connections::HumanConnections;
use wyrd_runtime::Permission;
use wyrd_spec::auth::{
    ConnectionActivate, ConnectionInput, ConnectionTestRequest, ConnectionTestResponse,
    ConnectionTester, HumanConnectionView, HumanConnectionsResponse,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::vala::api::AuditEvent;

use crate::audit;
use crate::auth::auth_not_configured;
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Audit resource naming the caller tenant's connection slot.
const RESOURCE: &str = "identity:oidc_connection";

/// Build the tenant connection administration routes for the `/v1` group.
pub fn identity_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_connections))
        .routes(routes!(put_candidate))
        .routes(routes!(test_candidate))
        .routes(routes!(activate_candidate))
        .routes(routes!(deactivate_active))
        .routes(routes!(remove_connection))
}

/// The deployment's connection owner, built once at boot.
///
/// # Errors
/// Returns `AUTH_NOT_CONFIGURED` when the server was assembled without auth.
fn connections(state: &AppState) -> Result<&HumanConnections, WyrdErrorResponse> {
    state
        .auth
        .human_connections
        .as_ref()
        .ok_or_else(auth_not_configured)
}

/// Evaluate `identity_connections:write` for `operation`, recording a denial.
///
/// # Errors
/// Returns the RBAC denial (already audited) and `AuditUnavailable` when the
/// denial cannot be recorded.
async fn decide(
    state: &AppState,
    caller: &Caller,
    operation: &str,
) -> Result<AuditEvent, WyrdErrorResponse> {
    audit::authorize_recording_denial(
        state,
        caller,
        &Permission::identity_connections_write(),
        operation,
        RESOURCE,
    )
    .await
    .map_err(WyrdErrorResponse::from)
}

/// Commit an allowed decision standalone and return `refusal`.
///
/// Used when a request is refused after authorization but before any tenant
/// transaction exists (an invalid body), so the allowance is still durable.
///
/// # Errors
/// Always returns an error: `refusal`, or `AuditUnavailable` when the decision
/// cannot be recorded.
async fn refuse_after_decision<T>(
    state: &AppState,
    caller: &Caller,
    decision: &AuditEvent,
    refusal: WyrdError,
) -> Result<T, WyrdErrorResponse> {
    audit::record_audit(state.postgres.vala(), caller.data_tenant_id, decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    Err(WyrdErrorResponse::from(refusal))
}

/// `GET /v1/identity/oidc/connections` — the caller tenant's Active and
/// Candidate connections, redacted, plus the callback URL to register.
///
/// # Errors
/// Returns `403` without `identity_connections:write`, and `503` when the store is
/// unavailable or the decision cannot be audited.
#[utoipa::path(
    get,
    path = "/identity/oidc/connections",
    responses(
        (status = 200, description = "Redacted Active and Candidate connections; secrets are \
          never returned", body = HumanConnectionsResponse),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A stored row did not decode (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.connections.list"))]
async fn list_connections(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<HumanConnectionsResponse>, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.connections.list").await?;
    connections(&state)?
        .list(caller.data_tenant_id, &decision)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `PUT /v1/identity/oidc/candidate` — create the tenant's candidate, or
/// replace it at the next revision.
///
/// This is also the rotation path for a secret or group-role map, including
/// when the issuer is unchanged. Replacing a candidate clears its test stamp.
/// The body is read as raw bytes, bounded by the router's default request
/// body limit, and is only decoded by [`ConnectionInput::from_slice`] after
/// the caller is authorized, so a refused caller never learns whether its
/// body was valid.
///
/// # Errors
/// Returns `400` for invalid input, a missing public origin, or an unsupported
/// client authentication method, `403` without `identity_connections:write`,
/// `409` for a stale `expected_revision`, and `503` when the store is
/// unavailable or the decision cannot be audited.
#[utoipa::path(
    put,
    path = "/identity/oidc/candidate",
    request_body = ConnectionInput,
    responses(
        (status = 200, description = "Candidate staged; the secret is never returned",
         body = HumanConnectionView),
        (status = 400, description = "Invalid input or no public origin configured \
          (WYRD_SPEC_400_VALIDATION), or an unsupported client authentication method \
          (WYRD_AUTH_400_UNSUPPORTED_CLIENT_AUTH)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "expected_revision does not match the current candidate \
          (WYRD_AUTH_409_CONNECTION_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.candidate.put"))]
async fn put_candidate(
    State(state): State<AppState>,
    caller: Caller,
    body: Bytes,
) -> Result<Json<HumanConnectionView>, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.candidate.put").await?;
    let owner = connections(&state)?;
    let staged = match ConnectionInput::from_slice(&body).and_then(|input| owner.stage(input)) {
        Ok(staged) => staged,
        Err(refusal) => return refuse_after_decision(&state, &caller, &decision, refusal).await,
    };
    owner
        .put_candidate(caller.data_tenant_id, staged, &decision)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `POST /v1/identity/oidc/candidate/test` — check the candidate at
/// `expected_revision` against its provider and begin a real test sign-in.
///
/// The allowed decision is committed before the screened provider IO starts.
/// Discovery must name the candidate's exact issuer and its JWKS must hold a
/// usable key; the response then carries the provider authorization URL of one
/// authorization-code login (PKCE, state, nonce) bound to this candidate
/// revision and this caller. Completing that sign-in at the provider returns
/// through the common `/auth/callback`, which redeems the code and verifies the
/// ID token exactly as a login, re-checks the caller's
/// `identity_connections:write` as `identity.oidc.candidate.tested`, and marks
/// only this revision tested for fifteen minutes. The test issues no session,
/// credential, or User.
///
/// # Errors
/// Returns `400` when no public origin or sealing key is configured, `403`
/// without `identity_connections:write`, `409` for a stale revision
/// (`CONNECTION_CONFLICT`) or a failed check (`CONNECTION_NOT_TESTED`), and
/// `503` when the provider is refused by address screening or unavailable, the
/// store is unavailable, or the decision cannot be audited.
#[utoipa::path(
    post,
    path = "/identity/oidc/candidate/test",
    request_body = ConnectionTestRequest,
    responses(
        (status = 200, description = "The provider checks passed; complete the sign-in at \
          authorization_url to mark this candidate revision tested", body = ConnectionTestResponse),
        (status = 400, description = "No public origin or sealing key is configured \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "No candidate at expected_revision \
          (WYRD_AUTH_409_CONNECTION_CONFLICT), or a check failed; details.reason is \
          issuer_mismatch or jwks_unusable (WYRD_AUTH_409_CONNECTION_NOT_TESTED)",
         body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The provider is refused by address screening or \
          unavailable, the store is unavailable, or the decision could not be audited \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.candidate.test"))]
async fn test_candidate(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<ConnectionTestRequest>,
) -> Result<Json<ConnectionTestResponse>, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.candidate.test").await?;
    audit::record_audit(state.postgres.vala(), caller.data_tenant_id, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let tester = ConnectionTester {
        principal_id: caller.principal.id,
        principal_kind: caller.principal.kind.tag(),
    };
    connections(&state)?
        .begin_test(caller.data_tenant_id, request.expected_revision, tester)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `POST /v1/identity/oidc/candidate/activate` — make the freshly tested
/// candidate the tenant's Active connection, retiring the previous one in the
/// same transaction.
///
/// Requires the candidate's current test stamp and a recovery API key of a
/// headless principal of this tenant holding `identity_connections:write`.
/// The key is verified and discarded; it is never stored, logged, or audited.
///
/// # Errors
/// Returns `403` without `identity_connections:write`, `409` for a stale
/// revision or invalid recovery key (`CONNECTION_CONFLICT`) or a missing or
/// expired test (`CONNECTION_NOT_TESTED`), and `503` when the store is
/// unavailable or the decision cannot be audited.
#[utoipa::path(
    post,
    path = "/identity/oidc/candidate/activate",
    request_body = ConnectionActivate,
    responses(
        (status = 200, description = "The candidate is now the Active connection",
         body = HumanConnectionView),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "Stale revision or invalid recovery key \
          (WYRD_AUTH_409_CONNECTION_CONFLICT), or no unexpired test for this revision \
          (WYRD_AUTH_409_CONNECTION_NOT_TESTED)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.candidate.activate"))]
async fn activate_candidate(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<ConnectionActivate>,
) -> Result<Json<HumanConnectionView>, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.candidate.activate").await?;
    connections(&state)?
        .activate(caller.data_tenant_id, request, &decision)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `POST /v1/identity/oidc/active/deactivate` — retire the Active connection;
/// human login through it stops immediately on every replica.
///
/// # Errors
/// Returns `403` without `identity_connections:write`, `404` when no Active
/// connection exists, and `503` when the store is unavailable
/// or the decision cannot be audited.
#[utoipa::path(
    post,
    path = "/identity/oidc/active/deactivate",
    responses(
        (status = 204, description = "The Active connection was retired"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No Active connection exists \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.active.deactivate"))]
async fn deactivate_active(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<StatusCode, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.active.deactivate").await?;
    connections(&state)?
        .deactivate(caller.data_tenant_id, &decision)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(WyrdErrorResponse::from)
}

/// `DELETE /v1/identity/oidc/connections/{id}` — tombstone one connection: it
/// stops trusting logins, its secret is wiped, and its id is kept for history.
///
/// # Errors
/// Returns `403` without `identity_connections:write`, `404` when this tenant
/// has no live connection with `id`, and `503` when the store is
/// unavailable or the decision cannot be audited.
#[utoipa::path(
    delete,
    path = "/identity/oidc/connections/{id}",
    params(("id" = Uuid, Path, description = "Connection id")),
    responses(
        (status = 204, description = "The connection was tombstoned"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No live connection with this id in the caller tenant \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.connection.remove"))]
async fn remove_connection(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.connection.remove").await?;
    connections(&state)?
        .remove(caller.data_tenant_id, id, &decision)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(WyrdErrorResponse::from)
}

#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{
        PermissionCheck, PermissionDenyReason, PermissionSet, PermissionVerdict, Principal,
        PrincipalId, PrincipalKind, RoleRef,
    };
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{LoginInitiation, PrincipalKindTag, Sha256Hex};
    use wyrd_spec::request_id::RequestId;
    use wyrd_sql::queries::auth::{consume_login_state, human_connection_in_state};
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use super::*;
    use crate::components::auth::{ServerAuth, ServerAuthz};
    use crate::postgres::ServerPostgres;
    use crate::test_support::{test_app_state, test_catalog};

    /// Public x coordinate of the Ed25519 key the mock provider publishes.
    const ED_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";

    /// Deployment origin whose `/auth/callback` a test sign-in returns to.
    const PUBLIC_ORIGIN: &str = "https://wyrd.test";

    /// A permission check that allows its first `allowed` evaluations and
    /// denies every later one.
    #[derive(Debug)]
    struct AllowFirst {
        /// Evaluations still allowed; each check consumes one.
        allowed: AtomicUsize,
        /// Evaluations performed so far.
        evaluations: AtomicUsize,
    }

    impl PermissionCheck for AllowFirst {
        /// Allow while the budget lasts, then deny as an RBAC refusal.
        fn check(&self, principal: &Principal, permission: &Permission) -> PermissionVerdict {
            self.evaluations.fetch_add(1, Ordering::SeqCst);
            let budget = self
                .allowed
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                });
            if budget.is_ok() {
                return PermissionVerdict::Allow;
            }
            PermissionVerdict::Deny {
                reason: PermissionDenyReason::Rbac {
                    required: Box::new(permission.clone()),
                    principal: principal.id,
                },
            }
        }
    }

    /// Start a mock provider that passes the checks a test begins with:
    /// discovery naming its own issuer and a usable JWKS.
    async fn passing_provider() -> MockServer {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "id_token_signing_alg_values_supported": ["EdDSA"],
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "keys": [{ "kty": "OKP", "crv": "Ed25519", "kid": "k1", "x": ED_X }]
            })))
            .mount(&server)
            .await;
        server
    }

    /// Build handler state whose connection owner holds a sealing keyring,
    /// may reach loopback providers, and whose permission check is `check`.
    async fn test_state(fixture: &PgFixture, check: Arc<AllowFirst>) -> AppState {
        let postgres = Arc::new(ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let root = tempfile::tempdir()
            .expect("identity storage tempdir")
            .keep()
            .join("identity-storage");
        std::fs::create_dir_all(&root).expect("storage root creates");
        let signer = LocalSigner::new(root).expect("local signer creates");
        let origin = Url::parse(PUBLIC_ORIGIN).expect("origin parses");
        test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            test_catalog().await,
        )
        .with_auth(ServerAuth {
            human_connections: Some(HumanConnections::new(
                fixture.wyrd_postgres().clone(),
                Some(Arc::new(SealingKeyring::new(SecretKey::from_bytes(
                    [7_u8; 32],
                )))),
                ScreenedHttp::allowing_internal(),
                Some(&origin),
            )),
            ..ServerAuth::default()
        })
        .with_authz(ServerAuthz {
            permission_check: check,
        })
    }

    /// A caller of `tenant`; the injected check, not its grants, decides.
    fn caller(tenant: DataTenantId) -> Caller {
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(Uuid::nil()),
                PrincipalKind::User,
                tenant,
                Vec::<RoleRef>::new(),
                PermissionSet::from_iter([Permission::identity_connections_write()]),
            ),
            request_id: RequestId::now_v7(),
            delegation_chain: Vec::new(),
        }
    }

    /// Raw candidate PUT bytes for `issuer` using client-auth `method`.
    fn candidate_body(issuer: &str, method: &str) -> Bytes {
        Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "issuer": issuer,
                "client_id": "wyrd-human",
                "client_auth": method,
                "claim_mapping": { "subject": "sub" },
            }))
            .expect("candidate body encodes"),
        )
    }

    /// Stage a public candidate for the mock provider at revision 1.
    ///
    /// # Panics
    /// Panics when the candidate cannot be staged.
    async fn stage_candidate(state: &AppState, tenant: DataTenantId, provider: &MockServer) {
        let input = ConnectionInput::from_slice(&candidate_body(&provider.uri(), "Public"))
            .expect("candidate input is valid");
        let owner = connections(state).expect("owner configured");
        let staged = owner.stage(input).expect("candidate stages");
        let decision = audit::audit_event(
            &caller(tenant),
            "identity.oidc.candidate.put",
            RESOURCE,
            &Permission::identity_connections_write().to_string(),
            wyrd_spec::vala::api::AuditOutcome::Allowed,
        );
        owner
            .put_candidate(tenant, staged, &decision)
            .await
            .expect("candidate is staged");
    }

    /// Count staged `(outcome)` decisions for `operation`.
    ///
    /// # Panics
    /// Panics when the staging read fails.
    async fn decisions(fixture: &PgFixture, operation: &str) -> Vec<(String, i64)> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let rows = sqlx::query_as(
            "SELECT outcome, count(*) FROM vala.audit_staging \
             WHERE operation = $1 GROUP BY outcome ORDER BY outcome",
        )
        .bind(operation)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("audit staging reads");
        conn.commit().await.expect("commit");
        rows
    }

    /// The candidate's `(tested_revision, tested_until is set)` stamp.
    ///
    /// # Panics
    /// Panics when the candidate cannot be read or is missing.
    async fn stamp(fixture: &PgFixture) -> (Option<i64>, bool) {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let row = human_connection_in_state(&mut conn, "Candidate")
            .await
            .expect("candidate reads")
            .expect("candidate exists");
        conn.commit().await.expect("commit");
        (row.tested_revision, row.tested_until.is_some())
    }

    /// Beginning a test records one allowed decision and returns an
    /// authorization URL for the candidate's client and the deployment
    /// callback carrying state, nonce, and an S256 PKCE challenge. Its state
    /// is stored bound to this caller and the exact candidate revision, while
    /// the candidate itself stays untested until the sign-in completes.
    ///
    /// # Panics
    /// Panics when the test is refused, the URL or stored state differs, or
    /// the candidate is stamped.
    #[tokio::test]
    async fn beginning_a_test_binds_one_sign_in_to_the_caller_and_revision() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let provider = passing_provider().await;
        let check = Arc::new(AllowFirst {
            allowed: AtomicUsize::new(usize::MAX),
            evaluations: AtomicUsize::new(0),
        });
        let state = test_state(&fixture, Arc::clone(&check)).await;
        stage_candidate(&state, tenant, &provider).await;

        let Json(begun) = test_candidate(
            State(state),
            caller(tenant),
            Json(ConnectionTestRequest {
                expected_revision: 1,
            }),
        )
        .await
        .expect("the candidate passes its provider checks");

        let url = Url::parse(begun.authorization_url.as_str()).expect("authorization url parses");
        assert_eq!(url.path(), "/authorize");
        let query: std::collections::HashMap<String, String> =
            url.query_pairs().into_owned().collect();
        assert_eq!(query["client_id"], "wyrd-human");
        assert_eq!(
            query["redirect_uri"],
            format!("{PUBLIC_ORIGIN}/auth/callback")
        );
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["code_challenge_method"], "S256");
        assert!(!query["nonce"].is_empty());
        let state_hash = Sha256Hex::digest(query["state"].as_bytes());
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let pending = consume_login_state(&mut conn, &state_hash)
            .await
            .expect("login state reads")
            .expect("the test state is pending");
        conn.commit().await.expect("commit");
        assert_eq!(pending.connection.connection_revision, 1);
        assert_eq!(pending.issuer, provider.uri());
        assert_eq!(
            pending.initiation,
            LoginInitiation::ConnectionTest(ConnectionTester {
                principal_id: PrincipalId::new(Uuid::nil()),
                principal_kind: PrincipalKindTag::User,
            })
        );
        assert_eq!(check.evaluations.load(Ordering::SeqCst), 1);
        assert_eq!(
            decisions(&fixture, "identity.oidc.candidate.test").await,
            vec![("allowed".to_owned(), 1)]
        );
        assert_eq!(stamp(&fixture).await, (None, false));
    }

    /// The candidate PUT decides authorization before it interprets the raw
    /// body: an unauthorized caller sending garbage gets an audited `403`, not
    /// a validation error; an authorized caller gets `VALIDATION` for bytes
    /// that are not JSON, `UNSUPPORTED_CLIENT_AUTH` for `PrivateKeyJwt`, and a
    /// staged candidate for a valid body.
    ///
    /// # Panics
    /// Panics when any response or recorded decision differs from the above.
    #[tokio::test]
    async fn put_candidate_authorizes_before_decoding_its_body() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let denied = test_state(
            &fixture,
            Arc::new(AllowFirst {
                allowed: AtomicUsize::new(0),
                evaluations: AtomicUsize::new(0),
            }),
        )
        .await;
        let refused = put_candidate(
            State(denied),
            caller(tenant),
            Bytes::from_static(b"{not json"),
        )
        .await
        .expect_err("an unauthorized caller is refused");
        assert_eq!(refused.0.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(
            decisions(&fixture, "identity.oidc.candidate.put").await,
            vec![("denied".to_owned(), 1)]
        );

        let state = test_state(
            &fixture,
            Arc::new(AllowFirst {
                allowed: AtomicUsize::new(usize::MAX),
                evaluations: AtomicUsize::new(0),
            }),
        )
        .await;
        let malformed = put_candidate(
            State(state.clone()),
            caller(tenant),
            Bytes::from_static(b"{not json"),
        )
        .await
        .expect_err("malformed input is refused");
        assert_eq!(malformed.0.code(), "WYRD_SPEC_400_VALIDATION");

        let unsupported = put_candidate(
            State(state.clone()),
            caller(tenant),
            candidate_body("https://idp.example.com", "PrivateKeyJwt"),
        )
        .await
        .expect_err("PrivateKeyJwt is refused");
        assert_eq!(
            unsupported.0.code(),
            "WYRD_AUTH_400_UNSUPPORTED_CLIENT_AUTH"
        );

        let Json(staged) = put_candidate(
            State(state),
            caller(tenant),
            candidate_body("https://idp.example.com", "Public"),
        )
        .await
        .expect("a valid body stages");
        assert_eq!(staged.revision, 1);
        assert_eq!(
            decisions(&fixture, "identity.oidc.candidate.put").await,
            vec![("allowed".to_owned(), 3), ("denied".to_owned(), 1)]
        );
    }
}
