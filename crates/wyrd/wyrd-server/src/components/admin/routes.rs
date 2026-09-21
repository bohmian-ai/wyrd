//! Tenant-admin CRUD for trusted OIDC issuers and workload bindings.
//!
//! These routes are the authoring control plane for the cloud-issuer trust
//! store: `POST/GET/DELETE /v1/admin/trusted-issuers` and
//! `/v1/admin/workload-bindings`. Every mutation is permission-gated
//! (`service_accounts:write`) and writes through a [`TenantConn`] on the
//! `wyrd_app` RLS pool, so Postgres row-level security is the load-bearing
//! tenant boundary — no per-query tenant filtering.
//!
//! Create resolves OIDC discovery exactly once to fill `jwks_uri`, encrypts the
//! client secret with the process sealing key before insert, and never lets a
//! plaintext secret reach a column, log, or response (GET redacts it). Reads and
//! deletes never touch discovery.
//!
//! Every handler audits its `service_accounts:write` verdict exactly once. Every
//! mutation commits the Allowed row standalone before its work, so the decision
//! survives a conflict, not-found, or failed write, and issuer create runs no
//! network IO unaudited. The list handlers append it in their read transaction.

use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use sqlx::Error as SqlxError;
use std::fmt::Display;
use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, OidcProvider, ScreenError, TrustedIssuer, WorkloadBinding,
};
use wyrd_spec::auth::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, CreateWorkloadBindingRequest,
    IssuerTokenPolicy, IssuerUrl, TrustedIssuerView, WorkloadBindingView,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_sql::queries::auth::{
    TrustedIssuerWrite, WorkloadBindingWrite, delete_trusted_issuer, delete_workload_binding,
    delete_workload_bindings_for_issuer, insert_trusted_issuer, insert_workload_binding,
    trusted_issuers_for_tenant, workload_bindings_for_tenant,
};
use wyrd_sql::row_types::auth::{TrustedIssuerRow, WorkloadBindingRow};
use wyrd_sql::{SqlError, TenantConn};

use crate::audit;
use crate::auth::pg_resolvers::{binding_write_from_binding, issuer_write_from_trusted};
use crate::components::auth::Caller;
use crate::config::DeploymentProfile;
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Default JWKS key-cache TTL when a create request omits `jwks_ttl_secs`.
const DEFAULT_JWKS_TTL: Duration = Duration::from_secs(3600);

/// Build the tenant-admin CRUD routes for the `/v1` group.
pub fn admin_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            create_trusted_issuer,
            list_trusted_issuers,
            delete_trusted_issuer_route
        ))
        .routes(routes!(
            create_workload_binding,
            list_workload_bindings,
            delete_workload_binding_route
        ))
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

fn principal_kind_policy(payload: IssuerTokenPolicy) -> IssuerTokenPolicy {
    payload
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

/// Extract a non-empty `client_secret` from the create request, or fail with a
/// `400` [`WyrdError::MissingRequiredField`]. Called only for the secret-bearing
/// client-auth variants (`SecretBasic`/`SecretPost`).
///
/// # Errors
/// Returns [`WyrdError::MissingRequiredField`] as a `400` when `client_secret`
/// is absent or empty.
fn required_secret(
    request: &CreateTrustedIssuerRequest,
) -> Result<SecretString, WyrdErrorResponse> {
    match request.client_secret.as_ref() {
        Some(secret) if !secret.expose().is_empty() => Ok(secret.clone().into_secret_string()),
        _ => Err(WyrdErrorResponse::from(WyrdError::MissingRequiredField {
            message: "client_secret is required for SecretBasic and SecretPost client auth"
                .to_owned(),
            details: serde_json::json!({ "field": "client_secret" }),
        })),
    }
}

/// Redacted issuer projection from a write row. Never carries the client secret.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when a JSONB column this server just wrote
/// does not decode into its contract type — the response is typed, so a row
/// that cannot be projected is a server defect rather than a caller error.
fn trusted_issuer_view_from_write(
    write: &TrustedIssuerWrite,
) -> Result<TrustedIssuerView, WyrdErrorResponse> {
    Ok(TrustedIssuerView {
        issuer: write.issuer_url.clone(),
        jwks_uri: write.jwks_uri.clone(),
        expected_audience: write.expected_audience.clone(),
        client_id: write.client_id.clone(),
        client_auth: write.client_auth.clone(),
        principal_kind: write.principal_kind.clone(),
        jwks_ttl_secs: write.jwks_ttl_secs,
        claim_mapping: from_stored_json(&write.claim_mapping)?,
        group_role_map: from_stored_json(&write.group_role_map)?,
        default_roles: from_stored_json(&write.default_roles)?,
    })
}

/// Redacted issuer projection from a stored row.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when a stored JSONB column does not decode
/// into its contract type.
fn trusted_issuer_view_from_row(
    row: TrustedIssuerRow,
) -> Result<TrustedIssuerView, WyrdErrorResponse> {
    Ok(TrustedIssuerView {
        issuer: row.issuer_url,
        jwks_uri: row.jwks_uri,
        expected_audience: row.expected_audience,
        client_id: row.client_id,
        client_auth: row.client_auth,
        principal_kind: row.principal_kind,
        jwks_ttl_secs: row.jwks_ttl_secs,
        claim_mapping: from_stored_json(&row.claim_mapping)?,
        group_role_map: from_stored_json(&row.group_role_map)?,
        default_roles: from_stored_json(&row.default_roles)?,
    })
}

/// Decode one stored JSONB column into the concrete type the wire declares.
///
/// The admin views used to hand these columns back as raw `serde_json::Value`,
/// which made the published contract a lie: a caller could not tell a claim
/// mapping from a role list. Decoding here is what lets the response type name
/// the real shape.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the column does not match `T`. Nothing
/// about the stored value reaches the caller; a malformed row is this
/// deployment's problem, not the requester's.
fn from_stored_json<T: DeserializeOwned>(stored: &JsonValue) -> Result<T, WyrdErrorResponse> {
    serde_json::from_value(stored.clone()).map_err(internal_error)
}

/// Workload-binding projection from a write row.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the stored card reference does not
/// decode into a [`CardRef`].
fn workload_binding_view_from_write(
    write: &WorkloadBindingWrite,
) -> Result<WorkloadBindingView, WyrdErrorResponse> {
    Ok(WorkloadBindingView {
        issuer: write.issuer_url.clone(),
        subject: write.subject.clone(),
        audience: write.audience.clone(),
        card_ref: from_stored_json(&write.card_ref)?,
    })
}

/// Workload-binding projection from a stored row.
fn workload_binding_view_from_row(row: WorkloadBindingRow) -> WorkloadBindingView {
    WorkloadBindingView {
        issuer: row.issuer_url,
        subject: row.subject,
        audience: row.audience,
        card_ref: row.card_ref,
    }
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

/// Optional exact-match filters for the workload-binding list. Both default to
/// `None`, which lists every binding for the caller's tenant.
#[derive(Debug, Default, Deserialize)]
struct BindingFilter {
    #[serde(default)]
    issuer: Option<String>,
    #[serde(default)]
    subject: Option<String>,
}

// --------------------------------------------------------------------------
// Handlers — trusted issuers
// --------------------------------------------------------------------------

/// `POST /v1/admin/trusted-issuers` — register a trusted OIDC issuer for the
/// caller's tenant.
///
/// Gated on `service_accounts:write`. Resolves OIDC discovery once to fill
/// `jwks_uri`, encrypts any client secret with the process sealing key before
/// insert (fails closed if a secret is supplied without a key), and writes
/// through the RLS `TenantConn`. Returns the redacted [`TrustedIssuerView`]
/// (never the secret).
///
/// Local request validation and discovery run before the transaction opens; the
/// Allowed row is appended on that same transaction, so the recorded decision
/// and the issuer row commit together. A discovery or sealing failure happens
/// before either exists, and an insert failure discards both.
///
/// # Errors
///
/// Returns a `400` for a missing secret or blocked issuer, the RBAC denial,
/// `AuditUnavailable` when the decision cannot be recorded, a `503` when
/// discovery, the sealing key, or the store is unavailable, and a `409` for a
/// duplicate issuer.
#[utoipa::path(
    post,
    path = "/admin/trusted-issuers",
    request_body = CreateTrustedIssuerRequest,
    responses(
        (status = 200, description = "Issuer registered; the client secret is never returned",
         body = TrustedIssuerView),
        (status = 400, description = "The issuer URL is malformed, resolves to a blocked address, \
          or a required field is missing (WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD)",
         body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks service_accounts:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "The issuer is already registered for this tenant \
          (WYRD_AUTH_409_ADMIN_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "The sealing key or a tenant store write failed, or the \
          decision could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "OIDC discovery or the store is unavailable, or no \
          verifier is configured for the access token \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Admin"
)]
async fn create_trusted_issuer(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<CreateTrustedIssuerRequest>,
) -> Result<Json<TrustedIssuerView>, WyrdErrorResponse> {
    let client_auth = request_client_auth(&request)?;
    let decision = audit::authorize_service_accounts_write(
        &state,
        &caller,
        "manage trusted issuers",
        "admin.trusted_issuer.create",
        &format!(
            "trusted_issuer:{}",
            normalize_issuer(request.issuer.as_str())
        ),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    // The only network call on any admin path, and only at create: discovery
    // resolves jwks_uri. A runtime read must never re-discover.
    let jwks_uri = discover_jwks_uri(&request.issuer, state.deployment_profile).await?;

    let trusted = TrustedIssuer {
        tenant_id: caller.principal.tenant_id,
        issuer: request.issuer.clone(),
        jwks_uri,
        expected_audience: request.expected_audience.clone(),
        client_id: request.client_id.clone(),
        client_auth,
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
    let write = issuer_write_from_trusted(&trusted, state.auth.sealing_key.as_deref())
        .map_err(seal_error)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    audit::append_on(&mut conn, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    insert_trusted_issuer(&mut conn, &write)
        .await
        .map_err(|error| map_write_error(error, AdminWriteTarget::TrustedIssuer))?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(Json(trusted_issuer_view_from_write(&write)?))
}

/// `GET /v1/admin/trusted-issuers` — list the caller tenant's trusted issuers as
/// redacted [`TrustedIssuerView`]s. Gated on `service_accounts:write`; no
/// discovery or secret ever leaves the store.
///
/// # Errors
/// Returns a `401` when the request carries no usable token, a `403` when the
/// caller lacks `service_accounts:write`, a `500` when the store read or the
/// decision's audit fails, and a `503` when the store is unavailable or no
/// token verifier is configured.
#[utoipa::path(
    get,
    path = "/admin/trusted-issuers",
    responses(
        (status = 200, description = "The tenant's trusted issuers, client secrets redacted",
         body = Vec<TrustedIssuerView>),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks service_accounts:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the decision \
          could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable, or no verifier is configured \
          for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Admin"
)]
async fn list_trusted_issuers(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<TrustedIssuerView>>, WyrdErrorResponse> {
    let decision = audit::authorize_service_accounts_write(
        &state,
        &caller,
        "read trusted issuers",
        "admin.trusted_issuer.list",
        "trusted_issuers",
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    audit::append_on(&mut conn, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let rows = trusted_issuers_for_tenant(&mut conn)
        .await
        .map_err(sql_unavailable)?;
    conn.commit().await.map_err(sql_unavailable)?;

    let views = rows
        .into_iter()
        .map(trusted_issuer_view_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(views))
}

/// `DELETE /v1/admin/trusted-issuers?issuer=&cascade=` — remove one issuer.
///
/// Gated on `service_accounts:write`. With `cascade=true` the referencing
/// workload bindings are deleted first so the FK `ON DELETE RESTRICT` does not
/// block the issuer delete; without it, a live binding fails the delete closed
/// (`409`). A missing issuer is a `404`. Returns `204 No Content`.
///
/// # Errors
/// Returns a `401` without a usable token, a `403` without
/// `service_accounts:write`, a `404` for an unknown issuer, a `409` when a live
/// binding still references it, a `500` when the delete or its audit fails, and
/// a `503` when the store is unavailable or no token verifier is configured.
#[utoipa::path(
    delete,
    path = "/admin/trusted-issuers",
    params(
        ("issuer" = String, Query, description = "Issuer URL to remove"),
        ("cascade" = Option<bool>, Query,
         description = "Remove the issuer's workload bindings first instead of failing closed")
    ),
    responses(
        (status = 204, description = "Issuer removed"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks service_accounts:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such issuer in this tenant \
          (WYRD_AUTH_404_ADMIN_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "Live workload bindings still reference the issuer and \
          cascade was not requested (WYRD_AUTH_409_ADMIN_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the decision \
          could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable, or no verifier is configured \
          for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Admin"
)]
async fn delete_trusted_issuer_route(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<DeleteIssuerQuery>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let issuer = normalize_issuer(&query.issuer);
    let decision = audit::authorize_service_accounts_write(
        &state,
        &caller,
        "delete trusted issuers",
        "admin.trusted_issuer.delete",
        &format!("trusted_issuer:{issuer}"),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    audit::append_on(&mut conn, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;

    // --cascade removes the referencing bindings first so the issuer delete is
    // not blocked by the FK ON DELETE RESTRICT. Without it, a live binding makes
    // the delete fail closed (23503 -> 409).
    if query.cascade {
        delete_workload_bindings_for_issuer(&mut conn, &issuer)
            .await
            .map_err(|error| map_write_error(error, AdminWriteTarget::WorkloadBinding))?;
    }

    let removed = delete_trusted_issuer(&mut conn, &issuer)
        .await
        .map_err(|error| map_write_error(error, AdminWriteTarget::TrustedIssuer))?;
    // "No such issuer" is a stable answer to a request whose permission was
    // already evaluated and allowed, so the decision commits with it. Returning
    // before the commit would roll the decision back and leave an authorized
    // probe with no durable record; only the store failures above, which take
    // their attempted effect with them, roll back.
    conn.commit().await.map_err(sql_unavailable)?;
    if removed == 0 {
        return Err(issuer_not_found(&issuer));
    }

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// Handlers — workload bindings
// --------------------------------------------------------------------------

/// `POST /v1/admin/workload-bindings` — bind a workload identity
/// `(issuer, subject, audience)` to a server-owned `CardRef`.
///
/// Gated on `service_accounts:write`. The referenced issuer must already be
/// registered in this tenant; an FK violation maps to a `404` (create the issuer
/// first), a duplicate binding to a `409`. Writes through the RLS `TenantConn`.
///
/// # Errors
/// Returns a `401` without a usable token, a `403` without
/// `service_accounts:write`, a `404` when the issuer is not registered in this
/// tenant, a `409` for a duplicate binding, a `500` when the write or its audit
/// fails, and a `503` when the store is unavailable or no token verifier is configured.
#[utoipa::path(
    post,
    path = "/admin/workload-bindings",
    request_body = CreateWorkloadBindingRequest,
    responses(
        (status = 200, description = "Binding created", body = WorkloadBindingView),
        (status = 400, description = "A required field is missing or malformed \
          (WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks service_accounts:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "The named issuer is not trusted by this tenant \
          (WYRD_AUTH_404_ADMIN_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "The binding already exists \
          (WYRD_AUTH_409_ADMIN_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the decision \
          could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable, or no verifier is configured \
          for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Admin"
)]
async fn create_workload_binding(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<CreateWorkloadBindingRequest>,
) -> Result<Json<WorkloadBindingView>, WyrdErrorResponse> {
    let decision = audit::authorize_service_accounts_write(
        &state,
        &caller,
        "manage workload bindings",
        "admin.workload_binding.create",
        &format!(
            "workload_binding:{}:{}",
            normalize_issuer(request.issuer.as_str()),
            request.subject
        ),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    let binding = WorkloadBinding {
        tenant_id: caller.principal.tenant_id,
        issuer: request.issuer.clone(),
        subject: request.subject.clone(),
        audience: request.audience.clone(),
        card_ref: request.card_ref.clone(),
    };
    let write = binding_write_from_binding(&binding).map_err(internal_error)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    audit::append_on(&mut conn, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    insert_workload_binding(&mut conn, &write)
        .await
        .map_err(|error| map_binding_write_error(error, &binding.issuer))?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(Json(workload_binding_view_from_write(&write)?))
}

/// `GET /v1/admin/workload-bindings?issuer=&subject=` — list the caller tenant's
/// workload bindings, optionally filtered by exact issuer and/or subject. Gated
/// on `service_accounts:write`. The issuer filter is normalized to the stored
/// form so a trailing slash does not silently miss.
///
/// # Errors
/// Returns a `401` without a usable token, a `403` without
/// `service_accounts:write`, a `500` when the read or its audit fails, and a
/// `503` when the store is unavailable or no token verifier is configured.
#[utoipa::path(
    get,
    path = "/admin/workload-bindings",
    params(
        ("issuer" = Option<String>, Query, description = "Exact issuer to filter by"),
        ("subject" = Option<String>, Query, description = "Exact subject to filter by")
    ),
    responses(
        (status = 200, description = "The tenant's workload bindings",
         body = Vec<WorkloadBindingView>),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks service_accounts:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the decision \
          could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable, or no verifier is configured \
          for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Admin"
)]
async fn list_workload_bindings(
    State(state): State<AppState>,
    caller: Caller,
    Query(filter): Query<BindingFilter>,
) -> Result<Json<Vec<WorkloadBindingView>>, WyrdErrorResponse> {
    let decision = audit::authorize_service_accounts_write(
        &state,
        &caller,
        "read workload bindings",
        "admin.workload_binding.list",
        "workload_bindings",
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    // Normalize the issuer filter to the stored form so a trailing slash does
    // not silently miss; the subject is matched verbatim.
    let issuer = filter.issuer.as_deref().map(normalize_issuer);
    let mut conn = acquire_conn(&state, &caller).await?;
    audit::append_on(&mut conn, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let rows =
        workload_bindings_for_tenant(&mut conn, issuer.as_deref(), filter.subject.as_deref())
            .await
            .map_err(sql_unavailable)?;
    conn.commit().await.map_err(sql_unavailable)?;

    let views = rows
        .into_iter()
        .map(workload_binding_view_from_row)
        .collect::<Vec<_>>();
    Ok(Json(views))
}

/// `DELETE /v1/admin/workload-bindings?issuer=&subject=` — remove one binding
/// addressed by `(issuer, subject)`. Gated on `service_accounts:write`. A missing
/// binding is a `404`. Returns `204 No Content`.
///
/// # Errors
/// Returns a `401` without a usable token, a `403` without
/// `service_accounts:write`, a `404` for an unknown binding, a `500` when the
/// delete or its audit fails, and a `503` when the store or the revocation
/// store is unavailable.
#[utoipa::path(
    delete,
    path = "/admin/workload-bindings",
    params(
        ("issuer" = String, Query, description = "Issuer of the binding to remove"),
        ("subject" = String, Query, description = "Subject of the binding to remove")
    ),
    responses(
        (status = 204, description = "Binding removed"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks service_accounts:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such binding in this tenant \
          (WYRD_AUTH_404_ADMIN_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the decision \
          could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable, or no verifier is configured \
          for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Admin"
)]
async fn delete_workload_binding_route(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<BindingQuery>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let issuer = normalize_issuer(&query.issuer);
    let decision = audit::authorize_service_accounts_write(
        &state,
        &caller,
        "delete workload bindings",
        "admin.workload_binding.delete",
        &format!("workload_binding:{issuer}:{}", query.subject),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    audit::append_on(&mut conn, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let removed = delete_workload_binding(&mut conn, &issuer, &query.subject)
        .await
        .map_err(|error| map_write_error(error, AdminWriteTarget::WorkloadBinding))?;
    // An already-absent binding is a stable answer reached after the permission
    // was evaluated and allowed, so the decision commits with it rather than
    // being rolled back alongside it.
    conn.commit().await.map_err(sql_unavailable)?;
    if removed == 0 {
        return Err(binding_not_found(&issuer, &query.subject));
    }

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// Shared helpers
// --------------------------------------------------------------------------

/// Acquire an RLS [`TenantConn`] scoped to the caller's tenant, mapping an
/// acquisition failure to a `503`. Every admin handler mutates/reads through this
/// so Postgres row-level security is the load-bearing tenant boundary.
async fn acquire_conn<'a>(
    state: &'a AppState,
    caller: &Caller,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    state
        .postgres
        .tenant_conn(caller.principal.tenant_id)
        .await
        .map_err(sql_unavailable)
}

/// Resolve the issuer's `jwks_uri` via OIDC discovery.
///
/// The issuer URL is already validated by [`IssuerUrl`]; a discovery failure is
/// a `503`. The fetch itself goes through the deployment's
/// [`ScreenedHttp`](wyrd_auth_oidc::ScreenedHttp), which owns the address
/// policy, the bounded resolution, the pinning, and the redirect refusal — the
/// same capability the login, token-exchange, and JWKS-refresh paths use, so
/// none of them can be the one that forgot.
///
/// # Errors
///
/// Returns a `MissingRequiredField` rejection when the issuer resolves to a
/// blocked address, and `DiscoveryUnavailable` when the host cannot be
/// resolved, the client cannot be built, or discovery itself fails.
pub(crate) async fn discover_jwks_uri(
    issuer: &IssuerUrl,
    deployment_profile: DeploymentProfile,
) -> Result<url::Url, WyrdErrorResponse> {
    let url = url::Url::parse(issuer.as_str()).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::MissingRequiredField {
            message: format!("issuer is not a valid URL: {error}"),
            details: serde_json::json!({ "field": "issuer" }),
        })
    })?;

    let client = deployment_profile
        .screened_http()
        .client_for(&url)
        .await
        .map_err(screen_error)?;

    let provider = OidcProvider::discover(url, client).await.map_err(|error| {
        tracing::warn!(
            error = %error,
            issuer = issuer.as_str(),
            "OIDC discovery failed for admin create"
        );
        WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
            message: "OIDC discovery failed for issuer".to_owned(),
            details: serde_json::json!({ "issuer": issuer.as_str() }),
        })
    })?;
    Ok(provider.metadata.jwks_uri)
}

/// Project a screening refusal onto the admin error catalog.
///
/// A blocked address is the administrator's issuer being wrong and says so; a
/// resolution or client failure is transient infrastructure and is a `503`.
fn screen_error(error: ScreenError) -> WyrdErrorResponse {
    match error {
        ScreenError::Blocked => WyrdErrorResponse::from(WyrdError::MissingRequiredField {
            message: "issuer resolves to a blocked address range".to_owned(),
            details: serde_json::json!({ "field": "issuer" }),
        }),
        ScreenError::Unresolved | ScreenError::Client => {
            WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
                message: "issuer could not be reached".to_owned(),
                details: serde_json::json!({ "field": "issuer" }),
            })
        }
    }
}

/// Normalize a query-supplied issuer to match the stored form (trailing slash
/// trimmed, as [`IssuerUrl::new`] does on the create path).
fn normalize_issuer(value: &str) -> String {
    value.trim_end_matches('/').to_owned()
}

/// Map a write-path SQLx error to an admin response.
///
/// A unique violation (duplicate create) and an FK violation (deleting an issuer
/// that still has live bindings, `ON DELETE RESTRICT`) are both `409` conflicts.
/// Anything else is a backend-unavailable `503`.
///
/// `target` names the administrative object in the caller's own vocabulary.
/// The physical constraint that fired is traced server-side and never returned:
/// it is an internal schema identifier, and putting it on the wire would both
/// disclose the schema to any authenticated tenant and make a physical rename a
/// change to the public contract.
///
/// The binding-insert path does not use this mapper: there an FK violation means
/// the referenced issuer is missing, which is a `404`, not a conflict. See
/// [`map_binding_write_error`].
fn map_write_error(error: SqlxError, target: AdminWriteTarget) -> WyrdErrorResponse {
    match SqlError::from(error) {
        SqlError::UniqueViolation { constraint } => {
            trace_conflict(&constraint, target, "duplicate");
            target.duplicate()
        }
        SqlError::FkViolation { constraint } => {
            trace_conflict(&constraint, target, "referenced");
            target.still_referenced()
        }
        other => sql_unavailable(other),
    }
}

/// The administrative object an admin write was acting on.
///
/// Conflict text is derived from this rather than from PostgreSQL so that the
/// public contract is owned by the route and stays stable across physical
/// schema changes.
#[derive(Clone, Copy, Debug)]
enum AdminWriteTarget {
    /// A tenant's trusted issuer registration.
    TrustedIssuer,
    /// A workload identity bound to a server-owned card.
    WorkloadBinding,
}

impl AdminWriteTarget {
    /// The stable wire spelling used in conflict details and traces.
    fn as_str(self) -> &'static str {
        match self {
            Self::TrustedIssuer => "trusted_issuer",
            Self::WorkloadBinding => "workload_binding",
        }
    }

    /// `409` for a create that collides with an existing row.
    fn duplicate(self) -> WyrdErrorResponse {
        let message = match self {
            Self::TrustedIssuer => "a trusted issuer with this issuer URL already exists",
            Self::WorkloadBinding => {
                "a workload binding for this issuer, subject, and audience already exists"
            }
        };
        WyrdErrorResponse::from(WyrdError::AdminConflict {
            message: message.to_owned(),
            details: serde_json::json!({ "target": self.as_str(), "reason": "duplicate" }),
        })
    }

    /// `409` for a delete blocked by rows that still reference this one.
    fn still_referenced(self) -> WyrdErrorResponse {
        let message = match self {
            Self::TrustedIssuer => {
                "this trusted issuer still has workload bindings; remove them or retry with \
                 cascade"
            }
            Self::WorkloadBinding => "this workload binding is still referenced",
        };
        WyrdErrorResponse::from(WyrdError::AdminConflict {
            message: message.to_owned(),
            details: serde_json::json!({ "target": self.as_str(), "reason": "referenced" }),
        })
    }
}

/// Record the physical constraint that produced a public conflict.
///
/// The operator needs the constraint name to diagnose an unexpected conflict;
/// the caller must not have it. This is the only place it goes.
fn trace_conflict(constraint: &str, target: AdminWriteTarget, reason: &'static str) {
    tracing::warn!(
        constraint = %constraint,
        target = target.as_str(),
        reason,
        "admin write conflicted"
    );
}

/// Map a workload-binding insert error to an admin response.
///
/// The two shared composite-FK constraints (`auth_workload_bindings` →
/// `auth_trusted_issuers`) report the same constraint name whether they fire on
/// a binding insert or on a blocked issuer delete, so the constraint name cannot
/// distinguish them; the call site can. On insert the only reachable FK
/// violation is a reference to a trusted issuer that is not registered in this
/// tenant (the `platform.tenants` FK is unreachable under the RLS `WITH CHECK`),
/// so it maps to a `404` [`WyrdError::AdminNotFound`] rather than a conflict. A
/// unique violation is still a duplicate-binding `409`.
fn map_binding_write_error(error: sqlx::Error, issuer: &IssuerUrl) -> WyrdErrorResponse {
    match SqlError::from(error) {
        SqlError::FkViolation { .. } => WyrdErrorResponse::from(WyrdError::AdminNotFound {
            message: format!(
                "trusted issuer {} is not registered in this tenant; create it first",
                issuer.as_str()
            ),
            details: serde_json::json!({ "issuer": issuer.as_str() }),
        }),
        SqlError::UniqueViolation { constraint } => {
            trace_conflict(&constraint, AdminWriteTarget::WorkloadBinding, "duplicate");
            AdminWriteTarget::WorkloadBinding.duplicate()
        }
        other => sql_unavailable(other),
    }
}

/// Map a client-secret sealing failure to an admin response. A missing sealing
/// key is a fail-closed `503` (never persist plaintext); an encrypt/serialize
/// failure is a `500`.
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

/// `404` for a delete that matched no issuer in the caller's tenant.
fn issuer_not_found(issuer: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::AdminNotFound {
        message: format!("trusted issuer {issuer} not found in tenant"),
        details: serde_json::json!({ "issuer": issuer }),
    })
}

/// `404` for a delete that matched no binding for `(issuer, subject)` in the
/// caller's tenant.
fn binding_not_found(issuer: &str, subject: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::AdminNotFound {
        message: format!(
            "workload binding for issuer {issuer} subject {subject} not found in tenant"
        ),
        details: serde_json::json!({ "issuer": issuer, "subject": subject }),
    })
}

/// Map any backing-store failure to a fail-closed `503` and log the cause. Admin
/// paths never surface the raw SQL error to the client.
fn sql_unavailable(error: impl std::fmt::Display) -> WyrdErrorResponse {
    tracing::warn!(error = %error, "admin db unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({}),
    })
}

/// Map an unexpected server-side failure (serialization, encryption internals)
/// to a `500`.
fn internal_error(cause: impl Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(internal_failure("admin request failed", &cause))
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Json;
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, TrustedIssuer,
    };
    use wyrd_crypt::SecretKey;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{
        Permission, PermissionCheck, PermissionDenyReason, PermissionSet, PermissionVerdict,
        Principal, PrincipalId, PrincipalKind, RoleRef,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::{IssuerUrl, SecretBearer};
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use secrecy::ExposeSecret;
    use wyrd_sql::queries::auth::{trusted_issuer_by_url, upsert_trusted_issuer};

    use super::*;
    use crate::auth::pg_resolvers::{PgIssuerResolver, issuer_write_from_trusted};
    use crate::components::auth::{Caller, ServerAuthz};
    use wyrd_spec::request_id::RequestId;

    const SECRET: &str = "super-secret";
    const SEEDED_ISSUER: &str = "https://idp.example.com/realms/wyrd";

    fn sealing_key() -> SecretKey {
        SecretKey::from_bytes([7_u8; 32])
    }

    async fn test_state(fixture: &PgFixture) -> AppState {
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let dir = tempfile::tempdir().expect("admin storage tempdir");
        let storage_root = dir.keep().join("admin-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
        .with_auth(crate::components::auth::ServerAuth {
            sealing_key: Some(Arc::new(sealing_key())),
            ..crate::components::auth::ServerAuth::default()
        })
    }

    fn principal_with(tenant: DataTenantId, perms: PermissionSet) -> Caller {
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::nil()),
                PrincipalKind::User,
                tenant,
                Vec::<RoleRef>::new(),
                perms,
            ),
            request_id: RequestId::now_v7(),
            // Nondelegated fixture caller: no verified `act` chain exists.
            delegation_chain: Vec::new(),
        }
    }

    fn writer(tenant: DataTenantId) -> Caller {
        principal_with(
            tenant,
            PermissionSet::from_iter([Permission::service_accounts_write()]),
        )
    }

    fn reader(tenant: DataTenantId) -> Caller {
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
            client_secret: secret.map(|raw| SecretBearer::new(raw.to_owned())),
            claim_mapping: ClaimMappingPayload {
                subject: "sub".to_owned(),
                email: Some("email".to_owned()),
                groups: Some("realm_access.roles".to_owned()),
            },
            group_role_map: HashMap::new(),
            default_roles: vec!["viewer".to_owned()],
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl_secs: None,
        }
    }

    fn sample_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("my-model").expect("card name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: Some(SpaceName::new("prod").expect("space")),
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
            principal_kind: IssuerTokenPolicy::Workload,
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
    async fn create_blocks_private_issuer_in_production() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture)
            .await
            .with_deployment_profile(DeploymentProfile::Production);
        // The SSRF guard must reject a loopback issuer under the Production
        // profile before any network call — no discovery server is stood up.
        let issuer = IssuerUrl::new("http://127.0.0.1:9/").expect("loopback issuer parses");

        let error = create_trusted_issuer(
            State(state),
            writer(tenant),
            Json(create_issuer_request(issuer, Some(SECRET))),
        )
        .await
        .expect_err("Production must block a private/loopback issuer");

        assert!(matches!(
            &error.0,
            WyrdError::MissingRequiredField { message, .. }
                if message.contains("blocked address range")
        ));
    }

    #[tokio::test]
    async fn create_blocks_metadata_endpoint_in_development() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        // Default (non-production) profile: the private/loopback block does not
        // apply, but the cloud metadata endpoint is blocked in every profile and
        // must be rejected before any network call.
        let state = test_state(&fixture).await;
        // `https` because `IssuerUrl` allows `http` only for loopback; the guard
        // rejects the metadata address before any connection is attempted.
        let issuer = IssuerUrl::new("https://169.254.169.254/").expect("metadata issuer parses");

        let error = create_trusted_issuer(
            State(state),
            writer(tenant),
            Json(create_issuer_request(issuer, Some(SECRET))),
        )
        .await
        .expect_err("the metadata endpoint must be blocked in every profile");

        assert!(matches!(
            &error.0,
            WyrdError::MissingRequiredField { message, .. }
                if message.contains("blocked address range")
        ));
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

        // LIST returns the redacted view.
        let listed = list_trusted_issuers(State(state), writer(tenant))
            .await
            .expect("list succeeds")
            .0;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].issuer, issuer.as_str());
        let listed_body = serde_json::to_value(&listed).expect("views serialize");
        assert!(
            !listed_body.to_string().contains(SECRET),
            "list response must not echo the plaintext secret"
        );
    }

    /// Every physical identifier an administrative conflict must never
    /// disclose.
    ///
    /// PostgreSQL derives constraint names from table names, so the two table
    /// names cover `*_pkey` and the composite `*_fkey` alike. A response that
    /// carried one would both hand an authenticated tenant the schema and make
    /// a physical rename a change to the public contract.
    const PHYSICAL_NAMES: [&str; 3] = [
        "auth_trusted_issuers",
        "auth_workload_bindings",
        "constraint",
    ];

    /// Assert a conflict keeps its stable code and leaks no physical name.
    ///
    /// # Panics
    ///
    /// Panics when the error is not an [`WyrdError::AdminConflict`], when it
    /// does not carry `WYRD_AUTH_409_ADMIN_CONFLICT`, or when its rendered
    /// message or details contain a physical identifier.
    fn assert_safe_conflict(error: &WyrdErrorResponse) {
        let WyrdError::AdminConflict { message, details } = &error.0 else {
            panic!("expected a 409 admin conflict, got {:?}", error.0);
        };
        assert_eq!(
            error.0.code(),
            "WYRD_AUTH_409_ADMIN_CONFLICT",
            "a conflict keeps its stable code"
        );
        let rendered = format!("{message} {details}").to_lowercase();
        for name in PHYSICAL_NAMES {
            assert!(
                !rendered.contains(name),
                "a public conflict disclosed the physical name {name}: {rendered}"
            );
        }
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

        assert_safe_conflict(&error);
        assert_eq!(
            decision_rows(&fixture, "admin.trusted_issuer.create").await,
            vec![("allowed".to_owned(), 1)],
            "only the create that actually wrote an issuer is recorded as allowed"
        );
    }

    #[tokio::test]
    async fn list_is_empty_and_delete_missing_issuer_is_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;

        // An empty tenant lists no issuers — absence is an empty Vec, not a 404.
        let listed = list_trusted_issuers(State(state.clone()), writer(tenant))
            .await
            .expect("list succeeds on an empty tenant")
            .0;
        assert!(listed.is_empty());

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
        assert_eq!(
            decision_rows(&fixture, "admin.trusted_issuer.delete").await,
            vec![("allowed".to_owned(), 1)],
            "a delete that removed nothing still records the decision it evaluated"
        );
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
        assert_safe_conflict(&conflict);

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
        assert_safe_conflict(&conflict);

        // LIST with the issuer filter resolves the binding.
        let listed = list_workload_bindings(
            State(state.clone()),
            writer(tenant),
            Query(BindingFilter {
                issuer: Some(SEEDED_ISSUER.to_owned()),
                subject: None,
            }),
        )
        .await
        .expect("binding list succeeds")
        .0;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].subject, subject);

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

    #[tokio::test]
    async fn create_binding_for_unknown_issuer_is_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        // Deliberately do NOT seed the issuer: the binding's composite FK to
        // auth_trusted_issuers has no target row.
        let issuer = IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid");

        let error = create_workload_binding(
            State(state),
            writer(tenant),
            Json(CreateWorkloadBindingRequest {
                issuer,
                subject: "system:serviceaccount:default/sa".to_owned(),
                audience: None,
                card_ref: sample_card_ref(),
            }),
        )
        .await
        .expect_err("binding create against a missing issuer must be not-found");
        // The FK violation is a missing referenced issuer (404), not a conflict.
        assert!(matches!(error.0, WyrdError::AdminNotFound { .. }));
        assert!(
            decision_rows(&fixture, "admin.workload_binding.create")
                .await
                .is_empty(),
            "a binding that never landed records no allowance"
        );
    }

    #[tokio::test]
    async fn list_returns_every_issuer_for_the_tenant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;

        // Seed two distinct issuers directly; the list must return both, proving
        // GET is a tenant-scoped collection read, not a single-resource get.
        for url in ["https://idp-a.example.com", "https://idp-b.example.com"] {
            let trusted = TrustedIssuer {
                tenant_id: tenant,
                issuer: IssuerUrl::new(url).expect("issuer is valid"),
                jwks_uri: format!("{url}/jwks").parse().expect("jwks uri"),
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
                principal_kind: IssuerTokenPolicy::Workload,
                jwks_ttl: Duration::from_secs(3600),
            };
            let write = issuer_write_from_trusted(&trusted, None).expect("issuer encodes");
            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            upsert_trusted_issuer(&mut conn, &write)
                .await
                .expect("issuer upsert");
            conn.commit().await.expect("issuer seed commits");
        }

        let listed = list_trusted_issuers(State(state), writer(tenant))
            .await
            .expect("list succeeds")
            .0;
        assert_eq!(listed.len(), 2);
    }

    /// Checker that denies every permission, standing in for a configured
    /// `PermissionCheck` that disagrees with the principal's effective set.
    #[derive(Debug)]
    struct DenyAllCheck;

    impl PermissionCheck for DenyAllCheck {
        /// Deny `permission` for `principal` regardless of its grants.
        fn check(&self, principal: &Principal, permission: &Permission) -> PermissionVerdict {
            PermissionVerdict::Deny {
                reason: PermissionDenyReason::Rbac {
                    required: Box::new(permission.clone()),
                    principal: principal.id,
                },
            }
        }
    }

    /// Return the staged `(outcome, count)` decision rows for `operation`.
    ///
    /// # Panics
    ///
    /// Panics when the tenant connection or the staging read fails.
    async fn decision_rows(fixture: &PgFixture, operation: &str) -> Vec<(String, i64)> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let rows = sqlx::query_as(
            "SELECT outcome, count(*) FROM vala.audit_staging \
             WHERE operation = $1 GROUP BY outcome",
        )
        .bind(operation)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("decision rows read");
        conn.commit().await.expect("assertion transaction commits");
        rows
    }

    /// Make every append to the canonical audit staging table fail.
    ///
    /// The one lever that separates "the effect committed and the record did
    /// not" from "neither did": if the two share a transaction, refusing the
    /// append must also cost the effect.
    ///
    /// `vala.audit_staging` is shared by every fixture in this Postgres, so the
    /// caller must [`allow_audit_appends`] again as soon as the call under test
    /// returns — a trigger left installed would fail unrelated tests.
    async fn fail_audit_appends(fixture: &PgFixture) {
        let superuser = fixture
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        sqlx::raw_sql(
            "CREATE OR REPLACE FUNCTION vala.test_refuse_audit()
               RETURNS trigger LANGUAGE plpgsql AS $$
               BEGIN
                 RAISE EXCEPTION 'injected audit failure';
               END;
               $$;
             DROP TRIGGER IF EXISTS test_refuse_audit ON vala.audit_staging;
             CREATE TRIGGER test_refuse_audit
               BEFORE INSERT ON vala.audit_staging
               FOR EACH ROW EXECUTE FUNCTION vala.test_refuse_audit();",
        )
        .execute(&superuser)
        .await
        .expect("audit failure installs");
    }

    /// Let audit appends succeed again.
    async fn allow_audit_appends(fixture: &PgFixture) {
        let superuser = fixture
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        sqlx::raw_sql("DROP TRIGGER IF EXISTS test_refuse_audit ON vala.audit_staging")
            .execute(&superuser)
            .await
            .expect("audit failure clears");
    }

    /// An unrecordable issuer create refuses and writes no issuer.
    ///
    /// The decision now rides the write transaction, so the fail-closed rule and
    /// atomicity are the same property: nothing can be registered that the audit
    /// log has no record of permitting.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn an_unrecordable_issuer_create_writes_no_issuer() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        let (_server, issuer) = discovery_server().await;
        fail_audit_appends(&fixture).await;

        let error = create_trusted_issuer(
            State(state),
            writer(tenant),
            Json(create_issuer_request(issuer.clone(), Some(SECRET))),
        )
        .await
        .expect_err("an unrecordable create is refused");
        allow_audit_appends(&fixture).await;
        assert_eq!(
            error.0.code(),
            "WYRD_VALA_500_AUDIT_UNAVAILABLE",
            "{:?}",
            error.0
        );

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let row = trusted_issuer_by_url(&mut conn, issuer.as_str())
            .await
            .expect("query succeeds");
        conn.commit().await.expect("commit");
        assert!(row.is_none(), "a refused create registers no issuer");
    }

    /// An unrecordable binding create refuses and writes no binding.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn an_unrecordable_binding_create_writes_no_binding() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        seed_issuer(&fixture, tenant).await;
        fail_audit_appends(&fixture).await;

        let error = create_workload_binding(
            State(state),
            writer(tenant),
            Json(CreateWorkloadBindingRequest {
                issuer: IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid"),
                subject: "system:serviceaccount:default/sa".to_owned(),
                audience: None,
                card_ref: sample_card_ref(),
            }),
        )
        .await
        .expect_err("an unrecordable binding create is refused");
        allow_audit_appends(&fixture).await;
        assert_eq!(
            error.0.code(),
            "WYRD_VALA_500_AUDIT_UNAVAILABLE",
            "{:?}",
            error.0
        );

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let bindings = workload_bindings_for_tenant(&mut conn, None, None)
            .await
            .expect("query succeeds");
        conn.commit().await.expect("commit");
        assert!(
            bindings.is_empty(),
            "a refused binding create binds nothing"
        );
    }

    /// A revocation that finds no principal commits its one allowed decision.
    ///
    /// Principal revocation shares this module's `service_accounts:write` gate.
    /// The caller was permitted to ask, so the miss commits exactly that one
    /// allowed decision with no effect, and the caller still sees a `404`.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn a_revocation_that_finds_nothing_records_one_allowance() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;

        let error = crate::auth::revoke::revoke_principal(
            State(state),
            writer(tenant),
            axum::extract::Path(PrincipalId::new(uuid::Uuid::new_v4())),
            Json(wyrd_spec::auth::RevokePrincipalRequest {
                principal_kind: wyrd_spec::auth::PrincipalKindTag::Service,
                reason: "key rotation".to_owned(),
            }),
        )
        .await
        .expect_err("revoking an absent principal is not found");
        assert!(matches!(error.0, WyrdError::PrincipalNotFound { .. }));
        assert_eq!(
            decision_rows(&fixture, "auth.principal.revoke").await,
            vec![("allowed".to_owned(), 1)],
            "a revocation that changed nothing commits exactly its allowed decision"
        );
    }

    /// A discovery failure refuses the create and records no allowance.
    ///
    /// The allowance belongs to the write transaction, and discovery runs before
    /// that transaction exists. So there is no issuer and no row claiming one was
    /// permitted — an allowance without its effect would read, afterwards, as an
    /// issuer someone registered and then hid.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn a_failed_discovery_leaves_no_allowance_and_no_issuer() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        // No discovery document is mounted, so discovery fails after the verdict.
        let server = MockServer::start().await;
        let issuer = IssuerUrl::new(server.uri()).expect("loopback http issuer is valid");

        let error = create_trusted_issuer(
            State(state),
            writer(tenant),
            Json(create_issuer_request(issuer.clone(), Some(SECRET))),
        )
        .await
        .expect_err("discovery failure refuses the create");
        assert!(matches!(error.0, WyrdError::DiscoveryUnavailable { .. }));

        assert!(
            decision_rows(&fixture, "admin.trusted_issuer.create")
                .await
                .is_empty(),
            "a create that never opened its transaction recorded nothing"
        );
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let row = trusted_issuer_by_url(&mut conn, issuer.as_str())
            .await
            .expect("query succeeds");
        conn.commit().await.expect("commit");
        assert!(row.is_none(), "a failed discovery creates no issuer");
    }

    /// The configured checker's denial governs the response, the effect, and the
    /// single audit row even when the principal holds `service_accounts:write`.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or any assertion fails.
    #[tokio::test]
    async fn configured_checker_denial_governs_workload_binding_create() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await.with_authz(ServerAuthz {
            permission_check: Arc::new(DenyAllCheck),
            ..ServerAuthz::default()
        });
        seed_issuer(&fixture, tenant).await;

        let error = create_workload_binding(
            State(state.clone()),
            writer(tenant),
            Json(CreateWorkloadBindingRequest {
                issuer: IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid"),
                subject: "system:serviceaccount:default/sa".to_owned(),
                audience: None,
                card_ref: sample_card_ref(),
            }),
        )
        .await
        .expect_err("the configured checker denies the write");
        assert!(matches!(
            &error.0,
            WyrdError::PermissionDeniedRbac { message, .. }
                if message == "service_accounts:write permission required to manage workload bindings"
        ));

        assert_eq!(
            decision_rows(&fixture, "admin.workload_binding.create").await,
            vec![("denied".to_owned(), 1)]
        );
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let bindings = workload_bindings_for_tenant(&mut conn, None, None)
            .await
            .expect("binding list reads");
        conn.commit().await.expect("commit");
        assert!(bindings.is_empty(), "a denied write creates no binding");
    }
}
