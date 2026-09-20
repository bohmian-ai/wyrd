//! Tenant-scoped principal and credential administration.
//!
//! A tenant administrator manages its own tenant here: creating machine
//! principals for automation, granting them a narrower set of roles, and
//! issuing, listing, and revoking their credentials. Nothing on this surface
//! reaches the tenant directory or another tenant — it runs entirely inside the
//! authenticated caller's tenant, under row-level security.
//!
//! Credentials are separate from identity throughout. Issuing a second
//! credential and revoking the first is a rotation with no gap, and it never
//! touches the principal or its roles.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Duration;
use secrecy::ExposeSecret;
use uuid::Uuid;
use wyrd_auth::exchange_api_key::principal_kind_wire;
use wyrd_auth::issue_api_key::WyrdApiKey;
use wyrd_auth::revocation_listener::notify_principal_revoked;
use wyrd_runtime::RoleRef;
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, CreateServicePrincipalResponse, CredentialListResponse,
    CredentialMetadata, IssuedCredential, SecretBearer,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::vala::api::AuditOutcome;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    ApiKeyMetadataRow, credential_belongs_to, grant_role_to_service_account, insert_api_key,
    insert_service_account, list_api_key_metadata, revoke_api_key,
    revoke_service_account_principal, role_by_name, service_account_by_id, user_by_id,
};

use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;

/// Lifetime of a credential issued to tenant automation.
const AUTOMATION_CREDENTIAL_DAYS: i64 = 90;

/// Permission every operation on this surface requires.
const REQUIRED_PERMISSION: &str = "service_accounts:write";

/// Build the tenant principal-administration routes for the `/v1` group.
pub fn principals_router() -> Router<AppState> {
    Router::new()
        .route("/principals", post(create_service_principal))
        .route(
            "/principals/{principal_id}/credentials",
            get(list_credentials).post(issue_credential),
        )
        .route(
            "/principals/{principal_id}/credentials/{credential_id}",
            axum::routing::delete(revoke_credential),
        )
}

/// Refuse a caller that lacks tenant principal-management authority.
///
/// Delegates to the repository's existing gate rather than re-checking the
/// permission here, so this surface cannot drift from the one the principal
/// revoke route already enforces.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the permission is not held.
fn require_principal_admin(caller: &Caller, action: &str) -> Result<(), WyrdErrorResponse> {
    wyrd_auth::service_accounts::require_service_accounts_write(&caller.principal, action)
        .map_err(WyrdErrorResponse::from)
}

/// Authorize one operation on this surface and record the decision.
///
/// Opens the tenant transaction first so the decision and the work it permits
/// share it. A denial is committed on its own — a refused attempt is exactly
/// the thing an operator needs in the log — and the connection is returned only
/// when the caller may proceed.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the caller lacks the
/// permission, and [`WyrdError::AuditUnavailable`] when the decision cannot be
/// recorded, which refuses the operation either way.
async fn authorize<'a>(
    state: &'a AppState,
    caller: &Caller,
    action: &str,
    operation: &str,
    resource: &str,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    let mut conn = tenant_conn(state, caller).await?;
    match require_principal_admin(caller, action) {
        Ok(()) => {
            record_decision(
                &mut conn,
                caller,
                operation,
                resource,
                AuditOutcome::Allowed,
            )
            .await?;
            Ok(conn)
        }
        Err(denied) => {
            record_decision(&mut conn, caller, operation, resource, AuditOutcome::Denied).await?;
            conn.commit().await.map_err(internal)?;
            Err(denied)
        }
    }
}

/// Record an authorization decision on this surface.
///
/// Every operation here decides whether a principal may administer its tenant's
/// identities, and agent-rules requires such a decision to be audited in the
/// transaction that made it. Appending on the caller's own connection is what
/// makes the record and the write it permits commit or fail together — an audit
/// row for an operation that was rolled back would be worse than none.
///
/// # Errors
/// Returns [`WyrdError::AuditUnavailable`] when the append fails, which fails
/// the operation closed: a decision that cannot be recorded did not happen.
async fn record_decision(
    conn: &mut TenantConn<'_>,
    caller: &Caller,
    operation: &str,
    resource: &str,
    outcome: AuditOutcome,
) -> Result<(), WyrdErrorResponse> {
    audit::append_on(
        conn,
        &audit::audit_event(caller, operation, resource, REQUIRED_PERMISSION, outcome),
    )
    .await
    .map_err(WyrdErrorResponse::from)
}

/// Open a transaction scoped to the caller's tenant.
///
/// Tenant identity comes from the verified principal, never the request, so no
/// caller can steer this at another tenant. Goes through `WyrdPostgres`, the
/// owner of pooled tenant connections, so these routes carry the same acquire
/// telemetry as every other tenant-plane handler.
///
/// # Errors
/// Returns an internal error when the connection cannot be opened.
async fn tenant_conn<'a>(
    state: &'a AppState,
    caller: &Caller,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(|error| {
            WyrdErrorResponse::from(internal_failure("tenant connection unavailable", &error))
        })
}

/// Mint a credential for `principal_id` inside the caller's tenant.
///
/// # Errors
/// Returns an internal error when hashing or the insert fails.
/// Refuse a principal id that names nothing in this caller's tenant.
///
/// Row-level security already keeps a foreign principal's rows unreachable, but
/// unreachable is not the same as refused: without this, listing a foreign
/// principal answers with an empty page and issuing for one writes a credential
/// against an id the tenant does not own. Both surfaces route through here so
/// they give one answer, and it is the same non-enumerating `404` a genuinely
/// unknown id gets — a caller cannot tell the two apart, which is the point.
///
/// # Errors
/// Returns [`WyrdError::PrincipalNotFound`] when neither a service-kind nor a
/// user principal with this id exists in the tenant, or an internal error when
/// a lookup fails.
async fn require_principal(conn: &mut TenantConn<'_>, principal_id: Uuid) -> Result<(), WyrdError> {
    let exists = service_account_by_id(conn, principal_id)
        .await
        .map_err(|error| WyrdError::from(internal(error)))?
        .is_some()
        || user_by_id(conn, principal_id)
            .await
            .map_err(|error| WyrdError::from(internal(error)))?
            .is_some();
    if exists {
        return Ok(());
    }
    Err(WyrdError::PrincipalNotFound {
        message: "principal not found in this tenant".to_owned(),
        details: serde_json::json!({ "id": principal_id.to_string() }),
    })
}

async fn mint_credential(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    created_by: Uuid,
) -> Result<IssuedCredential, WyrdErrorResponse> {
    require_principal(conn, principal_id)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let plaintext = WyrdApiKey::generate(conn.data_tenant_id());
    let raw = plaintext.secret.clone();
    let key_hash = tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    let credential_id = Uuid::now_v7();
    insert_api_key(
        conn,
        credential_id,
        principal_id,
        &plaintext.prefix,
        &key_hash,
        created_by,
        Some(chrono::Utc::now() + Duration::days(AUTOMATION_CREDENTIAL_DAYS)),
    )
    .await
    .map_err(internal)?;

    Ok(IssuedCredential {
        id: credential_id.to_string(),
        credential: SecretBearer::new(plaintext.secret.expose_secret().to_owned()),
    })
}

/// Create a Card-free machine principal and issue its first credential.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, a requested
/// role does not exist in the tenant, or a write fails.
#[utoipa::path(
    post,
    path = "/v1/principals",
    request_body = CreateServicePrincipalRequest,
    responses(
        (status = 200, description = "Principal created with its first credential, returned once",
         body = CreateServicePrincipalResponse),
        (status = 400, description = "A requested role name is invalid or does not exist in \
          this tenant (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the authorization \
          decision could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn create_service_principal(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<CreateServicePrincipalRequest>,
) -> Result<Json<CreateServicePrincipalResponse>, WyrdErrorResponse> {
    for role in &request.roles {
        RoleRef::new(role).map_err(|_| {
            WyrdErrorResponse::from(WyrdError::Validation {
                message: "role name is not valid".to_owned(),
                details: serde_json::json!({ "role": role }),
            })
        })?;
    }

    let mut conn = authorize(
        &state,
        &caller,
        "create tenant principals",
        "auth.principal.create",
        &format!("principal:{}", request.name),
    )
    .await?;
    let principal_id = Uuid::now_v7();
    insert_service_account(
        &mut conn,
        principal_id,
        "service",
        None,
        &request.name,
        request.description.as_deref(),
        caller.principal.id.as_uuid(),
    )
    .await
    .map_err(internal)?;

    for role in &request.roles {
        let row = role_by_name(&mut conn, role)
            .await
            .map_err(internal)?
            .ok_or_else(|| {
                WyrdErrorResponse::from(WyrdError::Validation {
                    message: "role does not exist in this tenant".to_owned(),
                    details: serde_json::json!({ "role": role }),
                })
            })?;
        grant_role_to_service_account(&mut conn, principal_id, row.id)
            .await
            .map_err(internal)?;
    }

    let issued = mint_credential(&mut conn, principal_id, caller.principal.id.as_uuid()).await?;
    conn.commit().await.map_err(internal)?;

    Ok(Json(CreateServicePrincipalResponse {
        principal_id: wyrd_spec::auth::PrincipalId::new(principal_id),
        credential: issued.credential,
    }))
}

/// Issue an additional credential for an existing principal.
///
/// This is the first half of a rotation: the principal now holds two live
/// credentials, so the old one can be retired once the new one is verified.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or a write
/// fails.
#[utoipa::path(
    post,
    path = "/v1/principals/{principal_id}/credentials",
    params(("principal_id" = String, Path, description = "Principal to issue for")),
    responses(
        (status = 200, description = "Credential issued, plaintext returned once",
         body = IssuedCredential),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such principal in the caller's tenant \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the authorization \
          decision could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn issue_credential(
    State(state): State<AppState>,
    caller: Caller,
    Path(principal_id): Path<Uuid>,
) -> Result<Json<IssuedCredential>, WyrdErrorResponse> {
    let mut conn = authorize(
        &state,
        &caller,
        "issue tenant credentials",
        "auth.credential.issue",
        &format!("principal:{principal_id}"),
    )
    .await?;
    let issued = mint_credential(&mut conn, principal_id, caller.principal.id.as_uuid()).await?;
    conn.commit().await.map_err(internal)?;
    Ok(Json(issued))
}

/// List a principal's credential metadata.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or the read
/// fails.
#[utoipa::path(
    get,
    path = "/v1/principals/{principal_id}/credentials",
    params(("principal_id" = String, Path, description = "Principal whose credentials to list")),
    responses(
        (status = 200, description = "Non-secret credential metadata, newest first",
         body = CredentialListResponse),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such principal in the caller's tenant \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the authorization \
          decision could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn list_credentials(
    State(state): State<AppState>,
    caller: Caller,
    Path(principal_id): Path<Uuid>,
) -> Result<Json<CredentialListResponse>, WyrdErrorResponse> {
    list_credentials_for(&state, &caller, principal_id)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Read one principal's credential metadata.
///
/// The operation behind both the HTTP route and the MCP tool, so the two
/// surfaces cannot disagree about authorization, tenancy, or what a listing
/// contains.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the decision
/// cannot be audited, or the read fails.
pub(crate) async fn list_credentials_for(
    state: &AppState,
    caller: &Caller,
    principal_id: Uuid,
) -> Result<CredentialListResponse, WyrdError> {
    let mut conn = authorize(
        state,
        caller,
        "list tenant credentials",
        "auth.credential.list",
        &format!("principal:{principal_id}"),
    )
    .await
    .map_err(WyrdError::from)?;
    require_principal(&mut conn, principal_id).await?;
    let rows = list_api_key_metadata(&mut conn, principal_id)
        .await
        .map_err(|error| WyrdError::from(internal(error)))?;
    conn.commit()
        .await
        .map_err(|error| WyrdError::from(internal(error)))?;

    Ok(CredentialListResponse {
        credentials: rows.into_iter().map(metadata).collect(),
    })
}

/// Revoke one credential, leaving the principal and its roles untouched.
///
/// Revocation has to mean two things, and marking the credential row only
/// achieves the first: the credential can mint no new token, *and* the tokens
/// it already minted stop working. The second is the one that matters when a
/// credential leaks, and a bearer token is useful to whoever holds it for its
/// whole lifetime. So this also advances the principal's revocation epoch,
/// which the verifier checks on every request.
///
/// That epoch is per-principal, which is the granularity the schema offers, so
/// it also invalidates live tokens minted by the principal's *other*
/// credentials. That does not reintroduce a rotation gap — a surviving
/// credential re-exchanges immediately — and the alternative, letting a leaked
/// credential's tokens outlive their revocation, is not a trade worth making.
///
/// Both writes are in one transaction: a credential marked revoked whose tokens
/// still authorize is precisely the state this exists to prevent.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the credential
/// is unknown in this tenant, or a write fails.
#[utoipa::path(
    delete,
    path = "/v1/principals/{principal_id}/credentials/{credential_id}",
    params(
        ("principal_id" = String, Path, description = "Principal that owns the credential"),
        ("credential_id" = String, Path, description = "Credential to retire")
    ),
    responses(
        (status = 204, description = "Credential retired"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such principal in the caller's tenant, or it holds \
          no live credential of that id (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND, \
          WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the authorization \
          decision could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn revoke_credential(
    State(state): State<AppState>,
    caller: Caller,
    Path((principal_id, credential_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, WyrdErrorResponse> {
    revoke_credential_for(&state, &caller, principal_id, credential_id)
        .await
        .map(|()| axum::http::StatusCode::NO_CONTENT)
        .map_err(WyrdErrorResponse::from)
}

/// Retire one credential.
///
/// The operation behind both the HTTP route and the MCP tool. See the route
/// documentation above for why revocation advances the principal's epoch and
/// what that costs.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the credential
/// is not found for that principal, or a write fails.
pub(crate) async fn revoke_credential_for(
    state: &AppState,
    caller: &Caller,
    principal_id: Uuid,
    credential_id: Uuid,
) -> Result<(), WyrdError> {
    let mut conn = authorize(
        state,
        caller,
        "revoke tenant credentials",
        "auth.credential.revoke",
        &format!("credential:{credential_id}"),
    )
    .await
    .map_err(WyrdError::from)?;

    // Scoped to the principal the path names, so a credential id alone cannot
    // revoke a credential belonging to some other principal. The refusal
    // commits, because a probe for someone else's credential is exactly the
    // attempt an operator needs to find in the audit log.
    let owned = credential_belongs_to(&mut conn, credential_id, principal_id)
        .await
        .map_err(|error| WyrdError::from(internal(error)))?;
    if !owned {
        conn.commit()
            .await
            .map_err(|error| WyrdError::from(internal(error)))?;
        return Err(WyrdError::from(not_found()));
    }

    // Nothing after this point runs unless *this* call is the one that retires
    // the credential. Advancing the epoch is not idempotent in effect — it
    // kills every live token the principal holds — so a replayed revoke must
    // not do it a second time.
    if !revoke_api_key(&mut conn, credential_id)
        .await
        .map_err(|error| WyrdError::from(internal(error)))?
    {
        conn.commit()
            .await
            .map_err(|error| WyrdError::from(internal(error)))?;
        return Err(WyrdError::from(not_found()));
    }

    // The kind is read rather than assumed: the epoch cache is keyed by it, so
    // naming the wrong one invalidates nothing and leaves the window this
    // exists to close wide open. A tenant's administrative principal is
    // `tenant_admin`, not `service`.
    let kind = service_account_by_id(&mut conn, principal_id)
        .await
        .map_err(|error| WyrdError::from(internal(error)))?
        .and_then(|row| principal_kind_wire(&row.principal_kind))
        .ok_or_else(|| WyrdError::Internal {
            message: "credential owner has no recognizable principal kind".to_owned(),
            details: serde_json::json!({}),
        })?;

    revoke_service_account_principal(&mut conn, principal_id)
        .await
        .map_err(|error| WyrdError::from(internal(error)))?;
    conn.commit()
        .await
        .map_err(|error| WyrdError::from(internal(error)))?;

    // Each replica caches a principal's revocation epoch for a few seconds, so
    // without this the revoked credential's tokens keep working elsewhere until
    // that cache expires. Best-effort: the epoch write is durable and the TTL
    // enforces it regardless, this only shortens the window to the NOTIFY.
    if let Err(error) = notify_principal_revoked(
        state.postgres.app_pool(),
        caller.data_tenant_id,
        kind,
        wyrd_runtime::PrincipalId::new(principal_id),
    )
    .await
    {
        tracing::warn!(
            error = %error,
            "revocation NOTIFY failed; the epoch write is durable, TTL will enforce it"
        );
    }

    Ok(())
}

/// The one refusal for a credential this principal does not have.
///
/// An unknown credential and an already-retired one render identically, so a
/// replay learns nothing a first call would not have told it.
fn not_found() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::NotFound {
        message: "credential not found for this principal".to_owned(),
        details: serde_json::json!({}),
    })
}

/// Project a stored credential row onto its non-secret wire metadata.
fn metadata(row: ApiKeyMetadataRow) -> CredentialMetadata {
    CredentialMetadata {
        id: row.id.to_string(),
        prefix: row.prefix,
        created_at: row.created_at.to_rfc3339(),
        expires_at: row.expires_at.map(|at| at.to_rfc3339()),
        revoked_at: row.revoked_at.map(|at| at.to_rfc3339()),
        last_used_at: row.last_used_at.map(|at| at.to_rfc3339()),
    }
}

/// Map any internal failure onto the stable catalog without leaking detail.
fn internal(error: impl std::fmt::Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(internal_failure(
        "tenant principal administration failed",
        &error,
    ))
}
