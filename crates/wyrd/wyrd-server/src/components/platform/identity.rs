//! Platform-scope human identity routes.
//!
//! Configuring the deployment's one OIDC connection, registering the humans who
//! may sign in through it, and the login pair itself. Like the rest of the
//! platform plane these sit outside the `/v1` nest, and the two login routes are
//! anonymous by necessity: someone signing in has no session yet.
//!
//! Federated login is additive. Nothing here can take the global credential
//! away, so removing the connection or losing the provider leaves the
//! deployment administrable.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use wyrd_auth::pg_resolvers::{client_auth_label, seal_platform_client_secret};
use wyrd_auth::platform_authz::{
    PLATFORM_ADMINS_RESOURCE, PLATFORM_CONNECTION_RESOURCE, PlatformAuthorization,
    PlatformAuthzError, platform_principal_resource,
};
use wyrd_auth::platform_login::{PlatformLogin, PlatformLoginError};
use wyrd_auth_oidc::ClientAuth;
use wyrd_runtime::Permission;
use wyrd_spec::auth::{
    ConfigurePlatformOidcRequest, IssuerUrl, LoginInitResponse, PlatformCallbackRequest,
    PlatformClientAuth, PlatformLoginRequest, PlatformOidcConnectionView,
    PlatformPrincipalListResponse, PlatformPrincipalSummary, PlatformTokenResponse, PrincipalId,
    PrincipalKindTag, RegisterPlatformAdminRequest, RegisterPlatformAdminResponse, SecretBearer,
    SetPlatformPrincipalStatusRequest,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::request_id::RequestId;
use wyrd_sql::queries::platform::identity::{
    delete_platform_oidc_connection, insert_platform_identity_tx, platform_oidc_connection,
    upsert_platform_oidc_connection,
};
use wyrd_sql::queries::platform::principals::{
    StatusChange, insert_platform_principal_tx, list_platform_principals,
    set_platform_principal_status,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn};

use crate::components::auth::PlatformCaller;
use wyrd_sql::queries::platform::principal_grants::set_platform_grant_tx;

use crate::boot::init::platform_administrator_grant;
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Claim mapping the platform connection uses.
///
/// Fixed rather than configurable: the platform plane matches a pre-registered
/// administrator on their email and pins the standard `sub`. There is no
/// group-to-role mapping here at all, because a platform principal's authority
/// comes from its grant and must never be assertable by a provider.
fn platform_claim_mapping() -> JsonValue {
    // Claim paths are stored as dotted strings, the shape the login resolver
    // decodes this row with. An array here parses as nothing at all.
    serde_json::json!({
        "subject": "sub",
        "email": "email",
        "groups": null,
    })
}

/// Build the authenticated platform identity routes.
pub fn platform_identity_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            read_connection,
            configure_connection,
            remove_connection
        ))
        .routes(routes!(register_admin, list_platform_admins))
        .routes(routes!(set_admin_status))
}

/// Build the anonymous platform login routes.
///
/// Separate from the authenticated router because a human signing in has no
/// platform session yet — requiring one would make federated login impossible
/// for exactly the people it exists for.
pub fn platform_login_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(begin_login))
        .routes(routes!(complete_login))
}

/// Resolve the platform boundary, or report the plane unconfigured.
///
/// # Errors
/// Returns an internal error when no operator connection is configured.
pub(super) fn operator(state: &AppState) -> Result<OperatorPool, WyrdErrorResponse> {
    state.postgres.operator_pool().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform control plane is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        })
    })
}

/// Authorize one platform identity operation and hand back its open decision.
///
/// The returned transaction already carries the allowance row, so the caller
/// performs its mutation on it and commits once through [`commit_decision`].
/// A read-only caller commits it immediately: there is no effect to pair the
/// record with.
///
/// The authorization handle owns the pool the transaction borrows from, so it
/// is returned alongside and must outlive the connection. `resource` is the
/// exact object the decision is about, recorded verbatim in the audit row.
///
/// # Errors
/// Returns a permission error when the grant does not cover `required`, and an
/// internal error when the decision cannot be recorded — in which case nothing
/// is performed.
pub(super) async fn authorize<'a>(
    authz: &'a PlatformAuthorization,
    caller: &PlatformCaller,
    required: &Permission,
    resource: &str,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    authz
        .authorize(
            &caller.context,
            required,
            caller.request_id.as_str(),
            resource,
        )
        .await
        .map_err(|error| platform_authz_error(&error, required))
}

/// Commit an allowance together with whatever the caller wrote on it.
///
/// # Errors
/// Returns an internal error when the commit fails, in which case neither the
/// effect nor the allowance is durable.
pub(super) async fn commit_decision(decision: TenantConn<'_>) -> Result<(), WyrdErrorResponse> {
    decision.commit().await.map_err(|error| {
        WyrdErrorResponse::from(internal_failure(
            "platform authorization could not be committed",
            &error,
        ))
    })
}

/// Authorize a read and release its record immediately.
///
/// A read has no effect to pair the allowance with, so holding the transaction
/// open across it would buy nothing.
///
/// # Errors
/// Returns a permission error when the grant does not cover `required`, and an
/// internal error when the decision cannot be recorded or committed.
pub(super) async fn authorize_read(
    pool: &OperatorPool,
    caller: &PlatformCaller,
    required: &Permission,
    resource: &str,
) -> Result<(), WyrdErrorResponse> {
    let authz = PlatformAuthorization::new(pool.clone());
    let decision = authorize(&authz, caller, required, resource).await?;
    commit_decision(decision).await
}

/// Install or replace the deployment's platform OIDC connection.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the client
/// secret cannot be sealed, or the write fails.
#[utoipa::path(
    put,
    path = "/platform/oidc/connection",
    request_body = ConfigurePlatformOidcRequest,
    responses(
        (status = 200, description = "Connection installed; the JWKS endpoint comes from discovery",
         body = PlatformOidcConnectionView),
        (status = 400, description = "Issuer invalid, unreachable, resolving to a blocked address, \
          or a client secret that cannot be sealed (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn configure_connection(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<ConfigurePlatformOidcRequest>,
) -> Result<Json<PlatformOidcConnectionView>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    // The handle must outlive the transaction it lends out.
    let authz = PlatformAuthorization::new(pool.clone());
    let mut decision = authorize(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        PLATFORM_CONNECTION_RESOURCE,
    )
    .await?;

    let client_auth = match request.client_auth {
        PlatformClientAuth::SecretBasic { secret } => {
            ClientAuth::SecretBasic(secret.into_secret_string())
        }
        PlatformClientAuth::SecretPost { secret } => {
            ClientAuth::SecretPost(secret.into_secret_string())
        }
        PlatformClientAuth::Public => ClientAuth::Public,
    };
    // Discovery is the only network call on this path, and it is screened
    // against the deployment's blocked address ranges and pinned against DNS
    // rebinding by the same owner the tenant issuer path uses. Taking a
    // caller-supplied JWKS URL instead would make this route an SSRF primitive:
    // the anonymous login route drives outbound fetches to whatever is stored.
    // The parser's message names the URL library, so the caller is told which
    // field to correct and nothing about what parsed it.
    let issuer = IssuerUrl::new(request.issuer_url).map_err(|error| {
        tracing::warn!(cause = %error, "platform issuer URL rejected");
        WyrdErrorResponse::from(WyrdError::Validation {
            message: "issuer is not a valid issuer URL".to_owned(),
            details: serde_json::json!({ "field": "issuer_url" }),
        })
    })?;
    let jwks_uri =
        crate::components::admin::routes::discover_jwks_uri(&issuer, state.deployment_profile)
            .await?;

    let sealed = seal_platform_client_secret(&client_auth, state.auth.sealing_key.as_deref())
        .map_err(|error| {
            // A secret-bearing connection with no sealing key fails closed
            // rather than being stored in the clear. The sealing failure names
            // the key store, so it stays server-side.
            tracing::error!(cause = %error, "platform client secret could not be sealed");
            WyrdErrorResponse::from(WyrdError::Validation {
                message: "platform client secret could not be sealed".to_owned(),
                details: serde_json::json!({}),
            })
        })?;

    // The parsed issuer, not the request text, is what gets stored: login
    // reparses the stored row into an `IssuerUrl` and pins the identity by that
    // normalized string, so persisting `https://idp.example/` verbatim while
    // searching for `https://idp.example` would make first login impossible.
    upsert_platform_oidc_connection(
        &mut decision,
        issuer.as_str(),
        jwks_uri.as_str(),
        &request.expected_audience,
        &request.client_id,
        client_auth_label(&client_auth),
        &platform_claim_mapping(),
        request.jwks_ttl_secs,
        sealed.as_deref(),
    )
    .await
    .map_err(store_error)?;
    commit_decision(decision).await?;

    Ok(Json(PlatformOidcConnectionView {
        issuer_url: issuer.as_str().to_owned(),
        jwks_uri: jwks_uri.to_string(),
        expected_audience: request.expected_audience,
        client_id: request.client_id,
        client_auth: client_auth_label(&client_auth).to_owned(),
        jwks_ttl_secs: request.jwks_ttl_secs,
    }))
}

/// Read the configured connection, with its secret omitted.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, no connection
/// is configured, or the read fails.
#[utoipa::path(
    get,
    path = "/platform/oidc/connection",
    responses(
        (status = 200, description = "The configured connection, never its provider secret",
         body = PlatformOidcConnectionView),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No connection configured (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn read_connection(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<Json<PlatformOidcConnectionView>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize_read(
        &pool,
        &caller,
        &Permission::platform_identity_read(),
        PLATFORM_CONNECTION_RESOURCE,
    )
    .await?;

    let row = platform_oidc_connection(&pool)
        .await
        .map_err(store_error)?
        .ok_or_else(|| {
            WyrdErrorResponse::from(WyrdError::NotFound {
                message: "no platform OIDC connection is configured".to_owned(),
                details: serde_json::json!({}),
            })
        })?;

    // The sealed secret is read from the row and deliberately not projected.
    Ok(Json(PlatformOidcConnectionView {
        issuer_url: row.issuer_url,
        jwks_uri: row.jwks_uri,
        expected_audience: row.expected_audience,
        client_id: row.client_id,
        client_auth: row.client_auth,
        jwks_ttl_secs: row.jwks_ttl_secs,
    }))
}

/// Remove the platform OIDC connection.
///
/// Platform principals and their grants are untouched. Federated login stops
/// working; the global credential does not, which is the whole reason removal
/// is safe to expose.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or the delete
/// fails.
#[utoipa::path(
    delete,
    path = "/platform/oidc/connection",
    responses(
        (status = 204, description = "Connection removed; the global credential still administers"),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No connection configured (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn remove_connection(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<StatusCode, WyrdErrorResponse> {
    let pool = operator(&state)?;
    // The handle must outlive the transaction it lends out.
    let authz = PlatformAuthorization::new(pool.clone());
    let mut decision = authorize(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        PLATFORM_CONNECTION_RESOURCE,
    )
    .await?;

    let removed = delete_platform_oidc_connection(&mut decision)
        .await
        .map_err(store_error)?;
    // Permission was evaluated and allowed either way. "There was nothing to
    // remove" is a stable answer to an authorized request, not a failure, so
    // the decision stays durable rather than being rolled back with it.
    commit_decision(decision).await?;
    if removed {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(WyrdErrorResponse::from(WyrdError::NotFound {
            message: "no platform OIDC connection is configured".to_owned(),
            details: serde_json::json!({}),
        }))
    }
}

/// Pre-register a human platform administrator.
///
/// The principal is created with no credential at all: this human will
/// authenticate by federated identity, and their subject is pinned on first
/// login. Only a caller who already holds platform authority reaches here,
/// which is what stops a login from manufacturing platform authority.
///
/// The registration carries no grant. Authority is granted separately and
/// deliberately, so registering someone is never the same act as empowering
/// them.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, no connection
/// is configured, the name or claim is already registered, or a write fails.
#[utoipa::path(
    post,
    path = "/platform/admins",
    request_body = RegisterPlatformAdminRequest,
    responses(
        (status = 200, description = "Administrator registered, awaiting first login",
         body = RegisterPlatformAdminResponse),
        (status = 400, description = "No platform OIDC connection is configured, so the \
          administrator could never sign in (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "Name or matching claim already registered \
          (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn register_admin(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<RegisterPlatformAdminRequest>,
) -> Result<Json<RegisterPlatformAdminResponse>, WyrdErrorResponse> {
    let pool = operator(&state)?;

    // The allowance, the principal, and its identity commit together. Splitting
    // them would let a failed identity write orphan a platform principal that
    // can never sign in and — because the name is unique — permanently consume
    // the name an operator would retry with.
    //
    // The handle must outlive the transaction it lends out.
    let authz = PlatformAuthorization::new(pool.clone());
    // The id is minted before the decision so the record names the principal
    // this request creates. Nothing has adopted an existing row here — unlike
    // tenant provisioning — so the minted id is the one that is written.
    let principal_id = Uuid::now_v7();
    let mut decision = authorize(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        &platform_principal_resource(principal_id),
    )
    .await?;

    // Registering against no connection would create a principal that could
    // never sign in, so the connection is required first. That refusal is a
    // stable outcome of an evaluated permission, so the decision commits alone
    // before it; a failed read commits nothing.
    let Some(connection) = platform_oidc_connection(&pool).await.map_err(store_error)? else {
        commit_decision(decision).await?;
        return Err(WyrdErrorResponse::from(WyrdError::Validation {
            message: "configure the platform OIDC connection before registering an \
                      administrator"
                .to_owned(),
            details: serde_json::json!({}),
        }));
    };

    insert_platform_principal_tx(
        &mut decision,
        principal_id,
        PrincipalKindTag::User,
        &request.name,
    )
    .await
    .map_err(taken_or_store)?;
    insert_platform_identity_tx(
        &mut decision,
        principal_id,
        &connection.issuer_url,
        &request.match_claim,
    )
    .await
    .map_err(taken_or_store)?;

    // Authority comes with registration, in the same transaction. A registered
    // human with no grant can complete federated login and then call nothing:
    // an absent grant denies every platform operation, so splitting this out
    // would ship an administrator who is not one.
    let grant = serde_json::to_value(platform_administrator_grant().iter().collect::<Vec<_>>())
        .expect("permission set serializes to JSON");
    set_platform_grant_tx(&mut decision, principal_id, &grant)
        .await
        .map_err(store_error)?;

    commit_decision(decision).await?;

    Ok(Json(RegisterPlatformAdminResponse {
        principal_id: PrincipalId::new(principal_id),
    }))
}

/// List every platform principal, including the ones no longer permitted to
/// act.
///
/// A suspended administrator is shown rather than hidden: an operator auditing
/// who can administer the deployment needs to see that a revoked one really is
/// revoked. Nothing secret appears — a principal has no credential material to
/// leak, and its pinned subject is not one.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or the read
/// fails.
#[utoipa::path(
    get,
    path = "/platform/admins",
    responses(
        (status = 200, description = "Every platform principal, including suspended ones",
         body = PlatformPrincipalListResponse),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn list_platform_admins(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<Json<PlatformPrincipalListResponse>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize_read(
        &pool,
        &caller,
        &Permission::platform_identity_read(),
        PLATFORM_ADMINS_RESOURCE,
    )
    .await?;

    let rows = list_platform_principals(&pool).await.map_err(store_error)?;
    Ok(Json(PlatformPrincipalListResponse {
        principals: rows
            .into_iter()
            .map(|row| PlatformPrincipalSummary {
                principal_id: PrincipalId::new(row.id),
                principal_kind: row.principal_kind,
                name: row.name,
                status: row.status,
                match_claim: row.match_claim,
                subject: row.subject,
            })
            .collect(),
    }))
}

/// Suspend or restore a platform principal.
///
/// This is what makes the active-status check every platform request already
/// performs a live guard: without it nothing in the deployment could stop a
/// platform principal from acting, and a federated administrator whose identity
/// was pinned in error would be unremovable without direct database access.
///
/// Suspending the last active principal is refused. A deployment with no
/// principal that can authenticate cannot be recovered through any served
/// surface, and locking an operator out of their own control plane is not an
/// outcome a single request should be able to reach.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the status is
/// not a lifecycle value, the principal is unknown, the change would leave no
/// active principal, or the write fails.
#[utoipa::path(
    put,
    path = "/platform/admins/{principal_id}/status",
    params(("principal_id" = String, Path, description = "Platform principal to change")),
    request_body = SetPlatformPrincipalStatusRequest,
    responses(
        (status = 204, description = "Status changed"),
        (status = 400, description = "Status is neither active nor suspended (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "Platform principal not found (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "Would leave the deployment with no active principal \
          (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn set_admin_status(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Path(principal_id): Path<Uuid>,
    Json(request): Json<SetPlatformPrincipalStatusRequest>,
) -> Result<StatusCode, WyrdErrorResponse> {
    // A malformed status is refused before any permission is evaluated, so the
    // request never opens a decision it would then have to discard.
    if !matches!(request.status.as_str(), "active" | "suspended") {
        return Err(WyrdErrorResponse::from(WyrdError::Validation {
            message: "status must be active or suspended".to_owned(),
            details: serde_json::json!({ "status": request.status }),
        }));
    }

    let pool = operator(&state)?;
    // The handle must outlive the transaction it lends out.
    let authz = PlatformAuthorization::new(pool.clone());
    let mut decision = authorize(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        &platform_principal_resource(principal_id),
    )
    .await?;

    // The guard and the write are one serialized operator transaction, so two
    // administrators suspending each other at once cannot both be told a
    // survivor remains.
    let grant = serde_json::to_value(platform_administrator_grant().iter().collect::<Vec<_>>())
        .expect("permission set serializes to JSON");
    let change =
        set_platform_principal_status(&mut decision, principal_id, &request.status, &grant)
            .await
            .map_err(store_error)?;
    // Each outcome here is a stable administrative answer reached after the
    // permission was evaluated and allowed, so the decision commits in every
    // one of them. Only a store failure above rolls back, taking its attempted
    // effect with it.
    commit_decision(decision).await?;
    match change {
        StatusChange::Changed | StatusChange::Unchanged => Ok(axum::http::StatusCode::NO_CONTENT),
        StatusChange::NotFound => Err(WyrdErrorResponse::from(WyrdError::NotFound {
            message: "platform principal not found".to_owned(),
            details: serde_json::json!({}),
        })),
        StatusChange::WouldStrandDeployment => Err(WyrdErrorResponse::from(WyrdError::Conflict {
            message:
                "suspending this administrator would leave the deployment with no way                           in"
                    .to_owned(),
            details: serde_json::json!({ "principal_id": principal_id.to_string() }),
        })),
    }
}

/// Build the platform login service from server state.
///
/// # Errors
/// Returns an internal error when the platform plane, the token verifier, or
/// the signing key is not configured.
fn login_service(state: &AppState) -> Result<PlatformLogin, WyrdErrorResponse> {
    let pool = operator(state)?;
    let issuing_key = state.auth.issuing_key.clone().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform session issuance is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        })
    })?;
    let verifier = state.auth.token_verifier.clone().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "token verification is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        })
    })?;
    let sessions = std::sync::Arc::new(wyrd_auth::platform_sessions::PlatformSessions::new(
        pool.clone(),
        issuing_key,
    ));
    Ok(PlatformLogin::new(
        pool,
        state.auth.sealing_key.clone(),
        verifier,
        sessions,
        state.deployment_profile.screened_http(),
    ))
}

/// Begin a platform federated login.
///
/// # Errors
/// Returns a stable Wyrd error when federated login is not configured, the
/// provider cannot be reached, or the login state cannot be persisted.
#[utoipa::path(
    post,
    path = "/auth/platform/login",
    request_body = PlatformLoginRequest,
    responses(
        (status = 200, description = "Provider authorization URL and opaque state",
         body = LoginInitResponse),
        (status = 404, description = "Federated login is not configured (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "Identity provider unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, request))]
async fn begin_login(
    State(state): State<AppState>,
    Json(request): Json<PlatformLoginRequest>,
) -> Result<Json<LoginInitResponse>, WyrdErrorResponse> {
    login_service(&state)?
        .begin(request.redirect_uri)
        .await
        .map(Json)
        .map_err(login_error)
}

/// Complete a platform federated login and return a platform session.
///
/// # Errors
/// Returns a stable Wyrd error when the state is missing or replayed, the
/// provider cannot be reached, or the identity is not accepted.
#[utoipa::path(
    post,
    path = "/auth/platform/callback",
    request_body = PlatformCallbackRequest,
    responses(
        (status = 200, description = "Platform session for the resolved administrator",
         body = PlatformTokenResponse),
        (status = 401, description = "Identity not accepted, indistinguishably for every cause \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 503, description = "Identity provider unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, request))]
async fn complete_login(
    State(state): State<AppState>,
    request_id: Option<Extension<RequestId>>,
    Json(request): Json<PlatformCallbackRequest>,
) -> Result<Json<PlatformTokenResponse>, WyrdErrorResponse> {
    let fallback_request_id: String;
    let req_id = match request_id.as_ref() {
        Some(axum::Extension(id)) => id.as_str(),
        None => {
            fallback_request_id = uuid::Uuid::new_v4().to_string();
            &fallback_request_id
        }
    };
    let token = login_service(&state)?
        .complete(SecretString::from(request.code), &request.state, req_id)
        .await
        .map_err(login_error)?;

    Ok(Json(PlatformTokenResponse {
        access_token: SecretBearer::new(token.expose_secret().to_owned()),
        token_type: "Bearer".to_owned(),
        expires_in: u64::try_from(
            wyrd_auth::platform_sessions::DEFAULT_PLATFORM_TOKEN_TTL_MINUTES * 60,
        )
        .unwrap_or(900),
    }))
}

/// Project a platform authorization failure onto the public catalog.
fn platform_authz_error(error: &PlatformAuthzError, required: &Permission) -> WyrdErrorResponse {
    match error {
        PlatformAuthzError::Denied { .. } => {
            WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
                message: "not authorized to perform this platform operation".to_owned(),
                details: serde_json::json!({ "permission": required.to_string() }),
            })
        }
        PlatformAuthzError::AuditUnavailable(reason) => WyrdErrorResponse::from(internal_failure(
            "platform authorization could not be audited",
            &reason,
        )),
        PlatformAuthzError::Transaction(error) => {
            WyrdErrorResponse::from(internal_failure("platform authorization failed", &error))
        }
    }
}

/// Project a login failure onto the public catalog.
///
/// Every identity rejection collapses to one unauthenticated error, so a caller
/// learns whether it got in and never whether a subject is registered.
fn login_error(error: PlatformLoginError) -> WyrdErrorResponse {
    match error {
        PlatformLoginError::NotConfigured => WyrdErrorResponse::from(WyrdError::NotFound {
            message: "platform federated login is not configured".to_owned(),
            details: serde_json::json!({}),
        }),
        PlatformLoginError::NotAccepted | PlatformLoginError::InvalidState => {
            WyrdErrorResponse::from(WyrdError::Unauthenticated {
                message: "federated identity was not accepted".to_owned(),
                details: serde_json::json!({ "plane": "platform" }),
            })
        }
        PlatformLoginError::ProviderUnavailable(_) | PlatformLoginError::ConnectionUnusable => {
            WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
                message: "identity provider unavailable".to_owned(),
                details: serde_json::json!({ "retry_after_seconds": 1 }),
            })
        }
        PlatformLoginError::Store(error) => {
            WyrdErrorResponse::from(internal_failure("platform login failed", &error))
        }
        PlatformLoginError::Session(reason) => WyrdErrorResponse::from(internal_failure(
            "platform session could not be issued",
            &reason,
        )),
    }
}

/// Distinguish an already-registered name or claim from a store failure.
fn taken_or_store(error: SqlError) -> WyrdErrorResponse {
    match error {
        SqlError::UniqueViolation { .. } => WyrdErrorResponse::from(WyrdError::Conflict {
            message: "that name or matching claim is already registered".to_owned(),
            details: serde_json::json!({}),
        }),
        other => store_error(other),
    }
}

/// Project a platform store failure onto the public catalog.
fn store_error(error: SqlError) -> WyrdErrorResponse {
    WyrdErrorResponse::from(internal_failure("platform identity store failed", &error))
}
