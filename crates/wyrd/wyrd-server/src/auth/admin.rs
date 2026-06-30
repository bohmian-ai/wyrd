//! Tenant-admin CRUD for trusted OIDC issuers and workload bindings.
//!
//! These routes are the authoring control plane for the cloud-issuer trust
//! store: `POST/GET/DELETE /admin/trusted-issuers` and
//! `/admin/workload-bindings`. Every mutation is permission-gated
//! (`service_accounts:write`) and writes through a [`TenantConn`] on the
//! `wyrd_app` RLS pool, so Postgres row-level security is the load-bearing
//! tenant boundary — no per-query tenant filtering.
//!
//! Create resolves OIDC discovery exactly once to fill `jwks_uri`, encrypts the
//! client secret with the process sealing key before insert, and never lets a
//! plaintext secret reach a column, log, or response (GET redacts it). Reads and
//! deletes never touch discovery.
//!
//! No audit row is written here, and that is deliberate: admin-mutation audit is
//! deferred to the audit→Vala/Iceberg consolidation rather than a per-feature
//! Postgres table (the `revoke_principal` precedent writes no audit row either).

use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::post;
use secrecy::SecretString;
use serde::Deserialize;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, OidcProvider, PrincipalKindPolicy, TrustedIssuer,
    WorkloadBinding,
};
use wyrd_spec::auth::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, CreateWorkloadBindingRequest,
    IssuerUrl, PrincipalKindPayload, TrustedIssuerView, WorkloadBindingView,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::auth::{
    TrustedIssuerWrite, WorkloadBindingWrite, delete_trusted_issuer, delete_workload_binding,
    delete_workload_bindings_for_issuer, insert_trusted_issuer, insert_workload_binding,
    trusted_issuer_by_url, workload_binding_by_key,
};
use wyrd_sql::row_types::auth::{TrustedIssuerRow, WorkloadBindingRow};
use wyrd_sql::{SqlError, TenantConn};

use crate::auth::AuthenticatedPrincipal;
use crate::auth::pg_resolvers::{binding_write_from_binding, issuer_write_from_trusted};
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

/// Default JWKS key-cache TTL when a create request omits `jwks_ttl_secs`.
const DEFAULT_JWKS_TTL: Duration = Duration::from_secs(3600);

/// Mount the tenant-admin CRUD routes into an existing `/v1` router.
pub fn mount(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/admin/trusted-issuers",
            post(create_trusted_issuer)
                .get(get_trusted_issuer)
                .delete(delete_trusted_issuer_route),
        )
        .route(
            "/admin/workload-bindings",
            post(create_workload_binding)
                .get(get_workload_binding)
                .delete(delete_workload_binding_route),
        )
}

// --------------------------------------------------------------------------
// Wire-DTO conversions
//
// The request/response DTOs themselves live in `wyrd_spec::auth::admin`. The
// orphan rule forbids `impl From<wyrd_auth_oidc::_> for wyrd_spec::_` here
// (neither type is local to this crate), so every mapping to or from the
// domain and row types is a free function below.
// --------------------------------------------------------------------------

/// Build the domain [`ClaimMapping`] from its serde mirror; [`ClaimPath`] is
/// not directly (de)serializable.
fn claim_mapping_into_domain(payload: ClaimMappingPayload) -> ClaimMapping {
    ClaimMapping {
        subject: ClaimPath::new(payload.subject),
        email: payload.email.map(ClaimPath::new),
        groups: payload.groups.map(ClaimPath::new),
    }
}

/// Map the authored principal kind to the domain policy.
fn principal_kind_policy(payload: PrincipalKindPayload) -> PrincipalKindPolicy {
    match payload {
        PrincipalKindPayload::Human => PrincipalKindPolicy::Human,
        PrincipalKindPayload::Workload => PrincipalKindPolicy::Workload,
    }
}

/// Build the domain [`ClientAuth`], requiring a secret for the secret-bearing
/// variants. A missing secret on `SecretBasic`/`SecretPost` is a client error.
fn request_client_auth(
    request: &CreateTrustedIssuerRequest,
) -> Result<ClientAuth, WyrdErrorResponse> {
    match request.client_auth {
        ClientAuthKind::SecretBasic => Ok(ClientAuth::SecretBasic(required_secret(request)?)),
        ClientAuthKind::SecretPost => Ok(ClientAuth::SecretPost(required_secret(request)?)),
        ClientAuthKind::PrivateKeyJwt => Ok(ClientAuth::PrivateKeyJwt),
        ClientAuthKind::Public => Ok(ClientAuth::Public),
    }
}

fn required_secret(
    request: &CreateTrustedIssuerRequest,
) -> Result<SecretString, WyrdErrorResponse> {
    match request.client_secret.as_deref() {
        Some(secret) if !secret.is_empty() => Ok(SecretString::from(secret.to_owned())),
        _ => Err(WyrdErrorResponse::from(WyrdError::MissingRequiredField {
            message: "client_secret is required for SecretBasic and SecretPost client auth"
                .to_owned(),
            details: serde_json::json!({ "field": "client_secret" }),
        })),
    }
}

/// Redacted issuer projection from a write row. Never carries the client secret.
fn trusted_issuer_view_from_write(write: &TrustedIssuerWrite) -> TrustedIssuerView {
    TrustedIssuerView {
        issuer: write.issuer_url.clone(),
        jwks_uri: write.jwks_uri.clone(),
        expected_audience: write.expected_audience.clone(),
        client_id: write.client_id.clone(),
        client_auth: write.client_auth.clone(),
        principal_kind: write.principal_kind.clone(),
        jwks_ttl_secs: write.jwks_ttl_secs,
        claim_mapping: write.claim_mapping.clone(),
        group_role_map: write.group_role_map.clone(),
        default_roles: write.default_roles.clone(),
    }
}

/// Redacted issuer projection from a stored row.
fn trusted_issuer_view_from_row(row: TrustedIssuerRow) -> TrustedIssuerView {
    TrustedIssuerView {
        issuer: row.issuer_url,
        jwks_uri: row.jwks_uri,
        expected_audience: row.expected_audience,
        client_id: row.client_id,
        client_auth: row.client_auth,
        principal_kind: row.principal_kind,
        jwks_ttl_secs: row.jwks_ttl_secs,
        claim_mapping: row.claim_mapping,
        group_role_map: row.group_role_map,
        default_roles: row.default_roles,
    }
}

/// Workload-binding projection from a write row.
fn workload_binding_view_from_write(write: &WorkloadBindingWrite) -> WorkloadBindingView {
    WorkloadBindingView {
        issuer: write.issuer_url.clone(),
        subject: write.subject.clone(),
        audience: write.audience.clone(),
        card_ref: write.card_ref.clone(),
    }
}

/// Workload-binding projection from a stored row.
fn workload_binding_view_from_row(
    row: WorkloadBindingRow,
) -> Result<WorkloadBindingView, WyrdErrorResponse> {
    Ok(WorkloadBindingView {
        issuer: row.issuer_url,
        subject: row.subject,
        audience: row.audience,
        card_ref: serde_json::to_value(row.card_ref).map_err(internal_error)?,
    })
}

/// Query for addressing one issuer.
#[derive(Debug, Deserialize)]
struct IssuerQuery {
    issuer: String,
}

/// Query for deleting one issuer, with the cascade flag.
#[derive(Debug, Deserialize)]
struct DeleteIssuerQuery {
    issuer: String,
    #[serde(default)]
    cascade: bool,
}

/// Query for addressing one workload binding by `(issuer, subject)`.
#[derive(Debug, Deserialize)]
struct BindingQuery {
    issuer: String,
    subject: String,
}

// --------------------------------------------------------------------------
// Handlers — trusted issuers
// --------------------------------------------------------------------------

async fn create_trusted_issuer(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    Json(request): Json<CreateTrustedIssuerRequest>,
) -> Result<Json<TrustedIssuerView>, WyrdErrorResponse> {
    crate::auth::require_service_accounts_write(&caller.principal, "manage trusted issuers")?;

    // The only network call on any admin path, and only at create: discovery
    // resolves jwks_uri. A runtime read must never re-discover.
    let jwks_uri = discover_jwks_uri(&request.issuer).await?;

    let trusted = TrustedIssuer {
        tenant_id: caller.principal.tenant_id,
        issuer: request.issuer.clone(),
        jwks_uri,
        expected_audience: request.expected_audience.clone(),
        client_id: request.client_id.clone(),
        client_auth: request_client_auth(&request)?,
        claim_mapping: claim_mapping_into_domain(request.claim_mapping.clone()),
        group_role_map: request.group_role_map.clone(),
        default_roles: request.default_roles.clone(),
        principal_kind: principal_kind_policy(request.principal_kind),
        jwks_ttl: request
            .jwks_ttl_secs
            .map_or(DEFAULT_JWKS_TTL, Duration::from_secs),
    };

    // Encrypt before insert. A secret-bearing issuer with no sealing key fails
    // closed here — plaintext never lands in a column.
    let write =
        issuer_write_from_trusted(&trusted, state.sealing_key.as_deref()).map_err(seal_error)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    insert_trusted_issuer(&mut conn, &write)
        .await
        .map_err(map_write_error)?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(Json(trusted_issuer_view_from_write(&write)))
}

async fn get_trusted_issuer(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    Query(query): Query<IssuerQuery>,
) -> Result<Json<TrustedIssuerView>, WyrdErrorResponse> {
    crate::auth::require_service_accounts_write(&caller.principal, "read trusted issuers")?;

    let issuer = normalize_issuer(&query.issuer);
    let mut conn = acquire_conn(&state, &caller).await?;
    let row = trusted_issuer_by_url(&mut conn, &issuer)
        .await
        .map_err(sql_unavailable)?
        .ok_or_else(|| issuer_not_found(&issuer))?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(Json(trusted_issuer_view_from_row(row)))
}

async fn delete_trusted_issuer_route(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    Query(query): Query<DeleteIssuerQuery>,
) -> Result<StatusCode, WyrdErrorResponse> {
    crate::auth::require_service_accounts_write(&caller.principal, "delete trusted issuers")?;

    let issuer = normalize_issuer(&query.issuer);
    let mut conn = acquire_conn(&state, &caller).await?;

    // --cascade removes the referencing bindings first so the issuer delete is
    // not blocked by the FK ON DELETE RESTRICT. Without it, a live binding makes
    // the delete fail closed (23503 -> 409).
    if query.cascade {
        delete_workload_bindings_for_issuer(&mut conn, &issuer)
            .await
            .map_err(map_write_error)?;
    }

    let removed = delete_trusted_issuer(&mut conn, &issuer)
        .await
        .map_err(map_write_error)?;
    if removed == 0 {
        return Err(issuer_not_found(&issuer));
    }
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// Handlers — workload bindings
// --------------------------------------------------------------------------

async fn create_workload_binding(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    Json(request): Json<CreateWorkloadBindingRequest>,
) -> Result<Json<WorkloadBindingView>, WyrdErrorResponse> {
    crate::auth::require_service_accounts_write(&caller.principal, "manage workload bindings")?;

    let binding = WorkloadBinding {
        tenant_id: caller.principal.tenant_id,
        issuer: request.issuer.clone(),
        subject: request.subject.clone(),
        audience: request.audience.clone(),
        card_ref: request.card_ref.clone(),
    };
    let write = binding_write_from_binding(&binding).map_err(internal_error)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    insert_workload_binding(&mut conn, &write)
        .await
        .map_err(map_write_error)?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(Json(workload_binding_view_from_write(&write)))
}

async fn get_workload_binding(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    Query(query): Query<BindingQuery>,
) -> Result<Json<WorkloadBindingView>, WyrdErrorResponse> {
    crate::auth::require_service_accounts_write(&caller.principal, "read workload bindings")?;

    let issuer = normalize_issuer(&query.issuer);
    let mut conn = acquire_conn(&state, &caller).await?;
    let row = workload_binding_by_key(&mut conn, &issuer, &query.subject)
        .await
        .map_err(sql_unavailable)?
        .ok_or_else(|| binding_not_found(&issuer, &query.subject))?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(Json(workload_binding_view_from_row(row)?))
}

async fn delete_workload_binding_route(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    Query(query): Query<BindingQuery>,
) -> Result<StatusCode, WyrdErrorResponse> {
    crate::auth::require_service_accounts_write(&caller.principal, "delete workload bindings")?;

    let issuer = normalize_issuer(&query.issuer);
    let mut conn = acquire_conn(&state, &caller).await?;
    let removed = delete_workload_binding(&mut conn, &issuer, &query.subject)
        .await
        .map_err(map_write_error)?;
    if removed == 0 {
        return Err(binding_not_found(&issuer, &query.subject));
    }
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// Shared helpers
// --------------------------------------------------------------------------

async fn acquire_conn<'a>(
    state: &'a AppState,
    caller: &AuthenticatedPrincipal,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    TenantConn::acquire(&state.pool, caller.principal.tenant_id)
        .await
        .map_err(sql_unavailable)
}

/// Resolve the issuer's `jwks_uri` via OIDC discovery. The issuer URL is already
/// validated by [`IssuerUrl`]; a discovery failure is a `503`.
async fn discover_jwks_uri(issuer: &IssuerUrl) -> Result<url::Url, WyrdErrorResponse> {
    let url = url::Url::parse(issuer.as_str()).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::MissingRequiredField {
            message: format!("issuer is not a valid URL: {error}"),
            details: serde_json::json!({ "field": "issuer" }),
        })
    })?;
    let provider = OidcProvider::discover(url, reqwest::Client::new())
        .await
        .map_err(|error| {
            WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
                message: format!(
                    "OIDC discovery failed for issuer {}: {error}",
                    issuer.as_str()
                ),
                details: serde_json::json!({ "issuer": issuer.as_str() }),
            })
        })?;
    Ok(provider.metadata.jwks_uri)
}

/// Normalize a query-supplied issuer to match the stored form (trailing slash
/// trimmed, as [`IssuerUrl::new`] does on the create path).
fn normalize_issuer(value: &str) -> String {
    value.trim_end_matches('/').to_owned()
}

/// Map a write-path SQLx error to an admin response.
///
/// A unique violation (duplicate create) and an FK violation (delete blocked by
/// a live binding, or a binding referencing a missing issuer) are both `409`
/// conflicts. Anything else is a backend-unavailable `503`.
fn map_write_error(error: sqlx::Error) -> WyrdErrorResponse {
    match SqlError::from(error) {
        SqlError::UniqueViolation { constraint } | SqlError::FkViolation { constraint } => {
            WyrdErrorResponse::from(WyrdError::AdminConflict {
                message: format!("admin mutation conflicted on constraint {constraint}"),
                details: serde_json::json!({ "constraint": constraint }),
            })
        }
        other => sql_unavailable(other),
    }
}

fn seal_error(error: crate::auth::pg_resolvers::IssuerSealError) -> WyrdErrorResponse {
    use crate::auth::pg_resolvers::IssuerSealError;
    match error {
        IssuerSealError::SealingKeyMissing => {
            WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
                message: "a client secret was supplied but no sealing key is configured; \
                          refusing to store a plaintext secret"
                    .to_owned(),
                details: serde_json::json!({ "reason": "sealing_key_missing" }),
            })
        }
        IssuerSealError::Encrypt | IssuerSealError::Serialize(_) => internal_error(error),
    }
}

fn issuer_not_found(issuer: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::AdminNotFound {
        message: format!("trusted issuer {issuer} not found in tenant"),
        details: serde_json::json!({ "issuer": issuer }),
    })
}

fn binding_not_found(issuer: &str, subject: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::AdminNotFound {
        message: format!(
            "workload binding for issuer {issuer} subject {subject} not found in tenant"
        ),
        details: serde_json::json!({ "issuer": issuer, "subject": subject }),
    })
}

fn sql_unavailable(error: impl std::fmt::Display) -> WyrdErrorResponse {
    tracing::warn!(error = %error, "admin db unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({}),
    })
}

fn internal_error(error: impl std::fmt::Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: error.to_string(),
        details: serde_json::Value::Null,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Json;
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, PrincipalKindPolicy,
        TrustedIssuer,
    };
    use wyrd_crypt::SecretKey;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use secrecy::ExposeSecret;
    use wyrd_sql::queries::auth::{trusted_issuer_by_url, upsert_trusted_issuer};

    use super::*;
    use crate::auth::pg_resolvers::{PgIssuerResolver, issuer_write_from_trusted};

    const SECRET: &str = "super-secret";
    const SEEDED_ISSUER: &str = "https://idp.example.com/realms/wyrd";

    fn sealing_key() -> SecretKey {
        SecretKey::from_bytes([7_u8; 32])
    }

    async fn test_state(fixture: &PgFixture) -> AppState {
        let storage_root = fixture.tempdir_path().join("admin-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        AppState::new(
            fixture.app_pool().clone(),
            None,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
        )
        .with_sealing_key(Arc::new(sealing_key()))
    }

    fn principal_with(tenant: DataTenantId, perms: PermissionSet) -> AuthenticatedPrincipal {
        AuthenticatedPrincipal {
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::nil()),
                PrincipalKind::User,
                tenant,
                Vec::<RoleRef>::new(),
                perms,
            ),
        }
    }

    fn writer(tenant: DataTenantId) -> AuthenticatedPrincipal {
        principal_with(
            tenant,
            PermissionSet::from_iter([Permission::service_accounts_write()]),
        )
    }

    fn reader(tenant: DataTenantId) -> AuthenticatedPrincipal {
        principal_with(tenant, PermissionSet::new())
    }

    /// Start a wiremock IdP that answers OIDC discovery, returning the issuer URL.
    async fn discovery_server() -> (MockServer, IssuerUrl) {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "id_token_signing_alg_values_supported": ["RS256", "EdDSA"],
            })))
            .mount(&server)
            .await;
        let issuer_url = IssuerUrl::new(&issuer).expect("loopback http issuer is valid");
        (server, issuer_url)
    }

    fn create_issuer_request(
        issuer: IssuerUrl,
        secret: Option<&str>,
    ) -> CreateTrustedIssuerRequest {
        CreateTrustedIssuerRequest {
            issuer,
            expected_audience: "wyrd-api".to_owned(),
            client_id: "wyrd-client".to_owned(),
            client_auth: ClientAuthKind::SecretPost,
            client_secret: secret.map(ToOwned::to_owned),
            claim_mapping: ClaimMappingPayload {
                subject: "sub".to_owned(),
                email: Some("email".to_owned()),
                groups: Some("realm_access.roles".to_owned()),
            },
            group_role_map: HashMap::new(),
            default_roles: vec!["viewer".to_owned()],
            principal_kind: PrincipalKindPayload::Human,
            jwks_ttl_secs: None,
        }
    }

    fn sample_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("my-model").expect("card name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: SpaceName::new("prod").expect("space"),
            uid: None,
        }
    }

    /// Seed a no-secret issuer directly so binding tests can FK-reference it
    /// without standing up discovery.
    async fn seed_issuer(fixture: &PgFixture, tenant: DataTenantId) {
        let trusted = TrustedIssuer {
            tenant_id: tenant,
            issuer: IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid"),
            jwks_uri: format!("{SEEDED_ISSUER}/jwks").parse().expect("jwks uri"),
            expected_audience: "wyrd-api".to_owned(),
            client_id: "wyrd-client".to_owned(),
            client_auth: ClientAuth::PrivateKeyJwt,
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: None,
                groups: None,
            },
            group_role_map: HashMap::new(),
            default_roles: Vec::new(),
            principal_kind: PrincipalKindPolicy::Workload,
            jwks_ttl: Duration::from_secs(3600),
        };
        let write = issuer_write_from_trusted(&trusted, None).expect("issuer encodes");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        upsert_trusted_issuer(&mut conn, &write)
            .await
            .expect("issuer upsert");
        conn.commit().await.expect("issuer seed commits");
    }

    #[tokio::test]
    async fn create_denied_without_service_accounts_write() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        // No discovery server: the RBAC gate must short-circuit before any
        // network call or DB write.
        let issuer = IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid");

        let error = create_trusted_issuer(
            State(state),
            reader(tenant),
            Json(create_issuer_request(issuer, Some(SECRET))),
        )
        .await
        .expect_err("a principal without service_accounts:write must be denied");

        assert!(matches!(error.0, WyrdError::PermissionDeniedRbac { .. }));
    }

    #[tokio::test]
    async fn create_persists_sealed_secret_and_get_redacts() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        let (_server, issuer) = discovery_server().await;

        let view = create_trusted_issuer(
            State(state.clone()),
            writer(tenant),
            Json(create_issuer_request(issuer.clone(), Some(SECRET))),
        )
        .await
        .expect("create succeeds")
        .0;
        assert_eq!(view.issuer, issuer.as_str());
        assert!(view.jwks_uri.ends_with("/jwks"));

        // The create response is structurally redacted: no secret field anywhere.
        let body = serde_json::to_value(&view).expect("view serializes");
        assert!(
            !body.to_string().contains(SECRET),
            "create response must not echo the plaintext secret"
        );

        // The stored column holds ciphertext, never the plaintext bytes.
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let row = trusted_issuer_by_url(&mut conn, issuer.as_str())
            .await
            .expect("query succeeds")
            .expect("issuer row exists");
        conn.commit().await.expect("commit");
        let enc = row.client_secret_enc.expect("secret is stored");
        assert!(!enc.is_empty(), "ciphertext must be present");
        assert_ne!(
            enc,
            SECRET.as_bytes(),
            "plaintext must never land in the column"
        );

        // The sealing key round-trips: the resolver decrypts back to the original.
        let resolver = PgIssuerResolver::new(
            Arc::new(fixture.app_pool().clone()),
            Some(Arc::new(sealing_key())),
        );
        let resolved = resolver
            .trusted_issuers(&tenant)
            .await
            .expect("resolve succeeds");
        assert_eq!(resolved.len(), 1);
        match &resolved[0].client_auth {
            ClientAuth::SecretPost(secret) => assert_eq!(secret.expose_secret(), SECRET),
            other => panic!("expected SecretPost, got {other:?}"),
        }

        // GET returns the redacted view.
        let got = get_trusted_issuer(
            State(state),
            writer(tenant),
            Query(IssuerQuery {
                issuer: issuer.as_str().to_owned(),
            }),
        )
        .await
        .expect("get succeeds")
        .0;
        let got_body = serde_json::to_value(&got).expect("view serializes");
        assert!(
            !got_body.to_string().contains(SECRET),
            "GET response must not echo the plaintext secret"
        );
    }

    #[tokio::test]
    async fn duplicate_create_conflicts() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        let (_server, issuer) = discovery_server().await;

        let _ = create_trusted_issuer(
            State(state.clone()),
            writer(tenant),
            Json(create_issuer_request(issuer.clone(), Some(SECRET))),
        )
        .await
        .expect("first create succeeds");

        let error = create_trusted_issuer(
            State(state),
            writer(tenant),
            Json(create_issuer_request(issuer, Some(SECRET))),
        )
        .await
        .expect_err("a duplicate issuer must conflict");

        assert!(matches!(error.0, WyrdError::AdminConflict { .. }));
    }

    #[tokio::test]
    async fn get_and_delete_missing_issuer_is_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;

        let get_err = get_trusted_issuer(
            State(state.clone()),
            writer(tenant),
            Query(IssuerQuery {
                issuer: SEEDED_ISSUER.to_owned(),
            }),
        )
        .await
        .expect_err("missing issuer get is not found");
        assert!(matches!(get_err.0, WyrdError::AdminNotFound { .. }));

        let delete_err = delete_trusted_issuer_route(
            State(state),
            writer(tenant),
            Query(DeleteIssuerQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                cascade: false,
            }),
        )
        .await
        .expect_err("missing issuer delete is not found");
        assert!(matches!(delete_err.0, WyrdError::AdminNotFound { .. }));
    }

    #[tokio::test]
    async fn delete_issuer_with_live_binding_conflicts_then_cascades() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        seed_issuer(&fixture, tenant).await;
        let issuer = IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid");

        let _ = create_workload_binding(
            State(state.clone()),
            writer(tenant),
            Json(CreateWorkloadBindingRequest {
                issuer: issuer.clone(),
                subject: "system:serviceaccount:default/sa".to_owned(),
                audience: None,
                card_ref: sample_card_ref(),
            }),
        )
        .await
        .expect("binding create succeeds");

        // A live binding makes the issuer delete fail closed (FK RESTRICT -> 409).
        let conflict = delete_trusted_issuer_route(
            State(state.clone()),
            writer(tenant),
            Query(DeleteIssuerQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                cascade: false,
            }),
        )
        .await
        .expect_err("delete blocked by a live binding must conflict");
        assert!(matches!(conflict.0, WyrdError::AdminConflict { .. }));

        // --cascade removes the binding first, then the issuer.
        let status = delete_trusted_issuer_route(
            State(state),
            writer(tenant),
            Query(DeleteIssuerQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                cascade: true,
            }),
        )
        .await
        .expect("cascade delete succeeds");
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn workload_binding_crud_round_trip() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        seed_issuer(&fixture, tenant).await;
        let issuer = IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid");
        let subject = "system:serviceaccount:default/sa";

        let make_request = || CreateWorkloadBindingRequest {
            issuer: issuer.clone(),
            subject: subject.to_owned(),
            audience: Some("wyrd-workload".to_owned()),
            card_ref: sample_card_ref(),
        };

        let created =
            create_workload_binding(State(state.clone()), writer(tenant), Json(make_request()))
                .await
                .expect("binding create succeeds")
                .0;
        assert_eq!(created.subject, subject);

        // Duplicate (issuer, subject) conflicts.
        let conflict =
            create_workload_binding(State(state.clone()), writer(tenant), Json(make_request()))
                .await
                .expect_err("duplicate binding must conflict");
        assert!(matches!(conflict.0, WyrdError::AdminConflict { .. }));

        // GET resolves the binding.
        let got = get_workload_binding(
            State(state.clone()),
            writer(tenant),
            Query(BindingQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                subject: subject.to_owned(),
            }),
        )
        .await
        .expect("binding get succeeds")
        .0;
        assert_eq!(got.subject, subject);

        // DELETE removes it; a second delete is not found.
        let status = delete_workload_binding_route(
            State(state.clone()),
            writer(tenant),
            Query(BindingQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                subject: subject.to_owned(),
            }),
        )
        .await
        .expect("binding delete succeeds");
        assert_eq!(status, StatusCode::NO_CONTENT);

        let missing = delete_workload_binding_route(
            State(state),
            writer(tenant),
            Query(BindingQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                subject: subject.to_owned(),
            }),
        )
        .await
        .expect_err("deleting an absent binding is not found");
        assert!(matches!(missing.0, WyrdError::AdminNotFound { .. }));
    }
}
