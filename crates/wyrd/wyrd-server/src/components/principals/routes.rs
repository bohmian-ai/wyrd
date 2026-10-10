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
//!
//! Role assignments are subresources of the principal. Discovery and reads
//! keep the principal-administration permission; granting and revoking a
//! direct assignment requires tenant administration (`*`), so a credential
//! administrator cannot raise anyone's authority.

use axum::Json;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use secrecy::ExposeSecret;
use std::fmt::Display;
use uuid::Uuid;
use wyrd_auth::issue_api_key::WyrdApiKey;
use wyrd_runtime::{Permission, RoleRef};
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, CreateServicePrincipalResponse, CredentialListResponse,
    CredentialMetadata, IssuedCredential, PrincipalId, PrincipalKindTag, PrincipalPage,
    PrincipalQuery, PrincipalRoles, PrincipalStatus, PrincipalSummary, RoleAssignment,
    RoleAssignmentChange, RoleSource, SecretBearer,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::vala::api::AuditOutcome;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    ApiKeyMetadataRow, AssignablePrincipalRow, PrincipalFilter, assignable_principal,
    credential_belongs_to, grant_role_to_service_account, grant_role_to_user, insert_api_key,
    insert_service_account, list_api_key_metadata, list_assignable_principals,
    list_service_account_roles, list_user_role_assignments, revoke_api_key,
    revoke_role_from_service_account, revoke_role_from_user, role_by_name, service_account_by_id,
    user_by_id,
};

use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::{WyrdErrorResponse, internal_failure, path_rejection};
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Lifetime of a credential issued to tenant automation.
/// Lifetime of an automation principal's first credential.
const AUTOMATION_CREDENTIAL_LIFETIME: std::time::Duration = std::time::Duration::from_hours(2160);

/// Permission every operation on this surface requires; Role writes
/// additionally require tenant administration.
const REQUIRED_PERMISSION: &str = "service_accounts:write";

/// Page size of `GET /principals` when the caller names none.
const DEFAULT_PAGE_SIZE: u32 = 100;

/// Largest page `GET /principals` returns.
const MAX_PAGE_SIZE: u32 = 200;

/// Build the tenant principal-administration routes for the `/v1` group.
///
/// Mounts principal discovery and creation, credential issue/list/revoke,
/// whole-principal revocation (`POST /principals/{id}/revoke`), and Role
/// assignment reads and writes.
pub fn principals_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(crate::auth::revoke::revoke_principal))
        .routes(routes!(list_principals, create_service_principal))
        .routes(routes!(list_principal_roles))
        .routes(routes!(grant_principal_role, revoke_principal_role))
        .routes(routes!(list_credentials, issue_credential))
        .routes(routes!(revoke_credential))
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

/// Authorize one operation on this surface and stage the decision.
///
/// The decision is staged on the process audit outbox as soon as it is made,
/// allowed and denied alike, and the tenant transaction the work runs in is
/// opened only when the caller may proceed.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the caller lacks the
/// permission, and an internal error when the connection cannot be opened.
async fn authorize<'a>(
    state: &'a AppState,
    caller: &Caller,
    action: &str,
    operation: &str,
    resource: &str,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    let verdict = require_principal_admin(caller, action);
    let outcome = if verdict.is_ok() {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Denied
    };
    state.scribe_outbox.stage(
        caller.data_tenant_id,
        audit::audit_event(caller, operation, resource, REQUIRED_PERMISSION, outcome),
    );
    verdict?;
    tenant_conn(state, caller).await
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

/// Refuse a principal id that names nothing in this caller's tenant.
///
/// Row-level security already keeps a foreign principal's rows unreachable, but
/// unreachable is not the same as refused: without this, listing a foreign
/// principal answers with an empty page and issuing for one writes a credential
/// against an id the tenant does not own. Both surfaces route through here so
/// they give one answer, and it is the same non-enumerating `404` a genuinely
/// unknown id gets — a caller cannot tell the two apart, which is the point.
///
/// Takes the caller's decision transaction by value. When the principal is
/// absent the allowance already appended to it still records an evaluated
/// permission, so the transaction commits that decision alone before the
/// refusal; when it exists the transaction is handed back for the operation.
///
/// # Errors
/// Returns [`WyrdError::PrincipalNotFound`] when neither a service-kind nor a
/// user principal with this id exists in the tenant, or an internal error when
/// a lookup or the decision commit fails. A failed lookup commits nothing.
async fn require_principal(
    mut conn: TenantConn<'_>,
    principal_id: Uuid,
) -> Result<TenantConn<'_>, WyrdError> {
    let exists = service_account_by_id(&mut conn, principal_id)
        .await
        .map_err(|error| WyrdError::from(internal(error)))?
        .is_some()
        || user_by_id(&mut conn, principal_id)
            .await
            .map_err(|error| WyrdError::from(internal(error)))?
            .is_some();
    if exists {
        return Ok(conn);
    }
    conn.commit()
        .await
        .map_err(|error| WyrdError::from(internal(error)))?;
    Err(WyrdError::PrincipalNotFound {
        message: "principal not found in this tenant".to_owned(),
        details: serde_json::json!({ "id": principal_id.to_string() }),
    })
}

/// Mint one API key for an existing principal inside the caller's transaction.
///
/// The shared tail of principal creation and credential issuance. The caller
/// has already established, in this same transaction, that the principal
/// exists in the tenant — creation by inserting it, issuance through
/// [`require_principal`] — so a credential is never written against an absent
/// or foreign id. Only the SHA-256 verifier is inserted; the plaintext is returned to the caller once and never stored.
///
/// The insert joins the caller's transaction rather than committing on its
/// own, so a later failure in the same request rolls the credential back with
/// everything else and leaves no orphan key.
///
/// # Errors
/// Returns an internal failure when hashing or the insert fails.
async fn mint_credential(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    created_by: Uuid,
) -> Result<IssuedCredential, WyrdErrorResponse> {
    let plaintext = WyrdApiKey::generate(conn.data_tenant_id());
    let key_hash = wyrd_auth_issue::hash_secret(plaintext.secret.expose_secret());
    let credential_id = Uuid::now_v7();
    insert_api_key(
        conn,
        credential_id,
        principal_id,
        &plaintext.prefix,
        &key_hash,
        created_by,
        Some(AUTOMATION_CREDENTIAL_LIFETIME),
    )
    .await
    .map_err(internal)?;

    Ok(IssuedCredential {
        id: credential_id,
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
    path = "/principals",
    request_body = CreateServicePrincipalRequest,
    responses(
        (status = 200, description = "Principal created with its first credential, returned once",
         body = CreateServicePrincipalResponse),
        (status = 400, description = "A requested role name is invalid or does not exist in \
          this tenant (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "No verifier is configured for the access token (\
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

    // Every role is resolved before anything is written. An absent role is a
    // stable refusal of an evaluated permission, so the decision commits alone
    // and no tentative principal ever exists to roll back with it.
    let mut role_ids = Vec::with_capacity(request.roles.len());
    for role in &request.roles {
        let Some(row) = role_by_name(&mut conn, role).await.map_err(internal)? else {
            conn.commit().await.map_err(internal)?;
            return Err(WyrdErrorResponse::from(WyrdError::Validation {
                message: "role does not exist in this tenant".to_owned(),
                details: serde_json::json!({ "role": role }),
            }));
        };
        role_ids.push(row.id);
    }

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

    for role_id in role_ids {
        grant_role_to_service_account(&mut conn, principal_id, role_id)
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
    path = "/principals/{principal_id}/credentials",
    params(("principal_id" = Uuid, Path, description = "Principal to issue for")),
    responses(
        (status = 200, description = "Credential issued, plaintext returned once",
         body = IssuedCredential),
        (status = 400, description = "A path identifier is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such principal in the caller's tenant \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "No verifier is configured for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn issue_credential(
    State(state): State<AppState>,
    caller: Caller,
    principal_id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<IssuedCredential>, WyrdErrorResponse> {
    let Path(principal_id) = principal_id.map_err(|rejection| path_rejection(&rejection))?;
    let conn = authorize(
        &state,
        &caller,
        "issue tenant credentials",
        "auth.credential.issue",
        &format!("principal:{principal_id}"),
    )
    .await?;
    let mut conn = require_principal(conn, principal_id).await?;
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
    path = "/principals/{principal_id}/credentials",
    params(("principal_id" = Uuid, Path, description = "Principal whose credentials to list")),
    responses(
        (status = 200, description = "Non-secret credential metadata, newest first",
         body = CredentialListResponse),
        (status = 400, description = "A path identifier is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such principal in the caller's tenant \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "No verifier is configured for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn list_credentials(
    State(state): State<AppState>,
    caller: Caller,
    principal_id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<CredentialListResponse>, WyrdErrorResponse> {
    let Path(principal_id) = principal_id.map_err(|rejection| path_rejection(&rejection))?;
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
/// Returns a stable Wyrd error when the caller is unauthorized or the read
/// fails.
pub(crate) async fn list_credentials_for(
    state: &AppState,
    caller: &Caller,
    principal_id: Uuid,
) -> Result<CredentialListResponse, WyrdError> {
    let conn = authorize(
        state,
        caller,
        "list tenant credentials",
        "auth.credential.list",
        &format!("principal:{principal_id}"),
    )
    .await
    .map_err(WyrdError::from)?;
    let mut conn = require_principal(conn, principal_id).await?;
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
/// Revocation retires the credential so it can mint no new token. Tenant
/// tokens are self-contained and short-lived, so a token the credential already
/// minted keeps authorizing until it expires — at most one access-token TTL —
/// while the principal's other credentials are unaffected and keep working.
///
/// The retirement and its audited authorization decision commit together.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the credential
/// is unknown in this tenant, or a write fails.
#[utoipa::path(
    delete,
    path = "/principals/{principal_id}/credentials/{credential_id}",
    params(
        ("principal_id" = Uuid, Path, description = "Principal that owns the credential"),
        ("credential_id" = Uuid, Path, description = "Credential to retire")
    ),
    responses(
        (status = 204, description = "Credential retired"),
        (status = 400, description = "A path identifier is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such principal in the caller's tenant, or it holds \
          no live credential of that id (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND, \
          WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "No verifier is configured for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn revoke_credential(
    State(state): State<AppState>,
    caller: Caller,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let Path((principal_id, credential_id)) =
        ids.map_err(|rejection| path_rejection(&rejection))?;
    revoke_credential_for(&state, &caller, principal_id, credential_id)
        .await
        .map(|()| axum::http::StatusCode::NO_CONTENT)
        .map_err(WyrdErrorResponse::from)
}

/// Retire one credential.
///
/// The operation behind both the HTTP route and the MCP tool. See the route
/// documentation above for what retirement does and does not stop.
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

    // An already-retired credential renders as not found, so a replayed revoke
    // learns nothing a first call would not have told it.
    if !revoke_api_key(&mut conn, credential_id)
        .await
        .map_err(|error| WyrdError::from(internal(error)))?
    {
        conn.commit()
            .await
            .map_err(|error| WyrdError::from(internal(error)))?;
        return Err(WyrdError::from(not_found()));
    }

    conn.commit()
        .await
        .map_err(|error| WyrdError::from(internal(error)))?;

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

/// Discover the tenant's assignable principals.
///
/// Lists users and Service and Agent principals that are not deleted, ordered
/// by id, with exact-match filters and keyset paging. The tenant
/// administrator and the internal `system` writer are never listed.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for a malformed query, a page size
/// outside 1–200, or a non-assignable `kind`; the RBAC denial without
/// principal administration; and an internal error when the read fails.
#[utoipa::path(
    get,
    path = "/principals",
    params(PrincipalQuery),
    responses(
        (status = 200, description = "One page of assignable principals", body = PrincipalPage),
        (status = 400, description = "The query is malformed, its limit is outside 1-200, or \
          its kind is not user, service, or agent (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "No verifier is configured for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller, query))]
async fn list_principals(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<PrincipalQuery>, QueryRejection>,
) -> Result<Json<PrincipalPage>, WyrdErrorResponse> {
    let Query(query) = query.map_err(|rejection| {
        invalid(
            "principal query is malformed",
            serde_json::json!({ "query": rejection.body_text() }),
        )
    })?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(invalid(
            "limit must be between 1 and 200",
            serde_json::json!({ "limit": limit }),
        ));
    }
    if let Some(kind) = query.kind
        && !matches!(
            kind,
            PrincipalKindTag::User | PrincipalKindTag::Service | PrincipalKindTag::Agent
        )
    {
        return Err(invalid(
            "kind must be user, service, or agent",
            serde_json::json!({ "kind": kind.as_str() }),
        ));
    }
    let mut conn = authorize(
        &state,
        &caller,
        "list tenant principals",
        "auth.principal.list",
        "principals",
    )
    .await?;
    let filter = PrincipalFilter {
        kind: query.kind.as_ref().map(PrincipalKindTag::as_str),
        email: query.email.as_deref(),
        name: query.name.as_deref(),
        after: query.after.map(|after| after.as_uuid()),
    };
    // One row past the page says whether another page exists without a count.
    let mut rows = list_assignable_principals(&mut conn, filter, i64::from(limit) + 1)
        .await
        .map_err(internal)?;
    conn.commit().await.map_err(internal)?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let principals = rows
        .into_iter()
        .map(summary)
        .collect::<Result<Vec<_>, _>>()?;
    let next = if more {
        principals.last().map(|last| last.principal_id)
    } else {
        None
    };
    Ok(Json(PrincipalPage { principals, next }))
}

/// List one principal's Role assignments and their sources.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for a malformed id, the RBAC denial
/// without principal administration, the non-enumerating not-found error for
/// a principal that is not assignable in this tenant, and an internal error
/// when the read fails.
#[utoipa::path(
    get,
    path = "/principals/{principal_id}/roles",
    params(("principal_id" = Uuid, Path, description = "Principal whose Roles to list")),
    responses(
        (status = 200, description = "Assignments ordered by Role, then source",
         body = PrincipalRoles),
        (status = 400, description = "A path identifier is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No assignable principal with this id in the caller's \
          tenant (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "No verifier is configured for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn list_principal_roles(
    State(state): State<AppState>,
    caller: Caller,
    principal_id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<PrincipalRoles>, WyrdErrorResponse> {
    let Path(principal_id) = principal_id.map_err(|rejection| path_rejection(&rejection))?;
    let mut conn = authorize(
        &state,
        &caller,
        "list principal roles",
        "auth.principal.role.list",
        &format!("principal:{principal_id}"),
    )
    .await?;
    let (principal, kind) = require_assignable(&mut conn, principal_id).await?;
    let roles = role_assignments(&mut conn, principal.id, kind).await?;
    conn.commit().await.map_err(internal)?;
    Ok(Json(PrincipalRoles {
        principal_id: PrincipalId::new(principal.id),
        kind,
        roles,
    }))
}

/// Grant a principal a direct Role assignment.
///
/// Idempotent: a principal already holding the Role directly is answered with
/// `changed: false`. A user's `idp` assignment of the same Role is independent
/// and unaffected. The Role reaches the principal's next token.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for a malformed id or a Role that does
/// not exist in the tenant, the RBAC denial without tenant administration,
/// the non-enumerating not-found error for a principal that is not assignable
/// in this tenant, and an internal error when the store fails.
#[utoipa::path(
    put,
    path = "/principals/{principal_id}/roles/{role}",
    params(
        ("principal_id" = Uuid, Path, description = "Principal to grant the Role to"),
        ("role" = String, Path, description = "Role to grant directly")
    ),
    responses(
        (status = 200, description = "The principal holds the Role directly",
         body = RoleAssignmentChange),
        (status = 400, description = "A path identifier is not a valid UUID, or the Role is \
          malformed or does not exist in this tenant (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No assignable principal with this id in the caller's \
          tenant (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "No verifier is configured for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn grant_principal_role(
    State(state): State<AppState>,
    caller: Caller,
    target: Result<Path<(Uuid, String)>, PathRejection>,
) -> Result<Json<RoleAssignmentChange>, WyrdErrorResponse> {
    let Path((principal_id, role)) = target.map_err(|rejection| path_rejection(&rejection))?;
    change_direct_role(&state, &caller, principal_id, &role, RoleChange::Grant)
        .await
        .map(Json)
}

/// Revoke a principal's direct Role assignment.
///
/// Idempotent: a principal not holding the Role directly is answered with
/// `changed: false`. A user's `idp` assignment of the same Role remains, and
/// only the identity provider removes it at the next login. The removal
/// reaches the principal's next token; tokens already issued keep their
/// bounded lifetime.
///
/// # Errors
/// Returns the errors documented on [`grant_principal_role`].
#[utoipa::path(
    delete,
    path = "/principals/{principal_id}/roles/{role}",
    params(
        ("principal_id" = Uuid, Path, description = "Principal to revoke the Role from"),
        ("role" = String, Path, description = "Role whose direct assignment to remove")
    ),
    responses(
        (status = 200, description = "The principal no longer holds the Role directly",
         body = RoleAssignmentChange),
        (status = 400, description = "A path identifier is not a valid UUID, or the Role is \
          malformed or does not exist in this tenant (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No assignable principal with this id in the caller's \
          tenant (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "No verifier is configured for the access token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn revoke_principal_role(
    State(state): State<AppState>,
    caller: Caller,
    target: Result<Path<(Uuid, String)>, PathRejection>,
) -> Result<Json<RoleAssignmentChange>, WyrdErrorResponse> {
    let Path((principal_id, role)) = target.map_err(|rejection| path_rejection(&rejection))?;
    change_direct_role(&state, &caller, principal_id, &role, RoleChange::Revoke)
        .await
        .map(Json)
}

/// Which direct-assignment write a Role route performs.
#[derive(Debug, Clone, Copy)]
enum RoleChange {
    /// Add the direct assignment.
    Grant,
    /// Remove the direct assignment.
    Revoke,
}

impl RoleChange {
    /// Audit operation recorded for this write's authorization decision.
    const fn operation(self) -> &'static str {
        match self {
            Self::Grant => "auth.principal.role.grant",
            Self::Revoke => "auth.principal.role.revoke",
        }
    }
}

/// Grant or revoke one direct Role assignment and report the resulting set.
///
/// Tenant administration is checked and its decision staged before anything
/// else, then the Role and principal are resolved in the caller's tenant. The
/// write and the read of the resulting assignments share one transaction, so
/// the response is exactly what was committed.
///
/// # Errors
/// Returns the errors documented on [`grant_principal_role`].
async fn change_direct_role(
    state: &AppState,
    caller: &Caller,
    principal_id: Uuid,
    role: &str,
    change: RoleChange,
) -> Result<RoleAssignmentChange, WyrdErrorResponse> {
    audit::authorize(
        state,
        caller,
        &Permission::wildcard(),
        change.operation(),
        &format!("principal:{principal_id}/role:{role}"),
    )?;
    let unknown_role = || {
        invalid(
            "role does not exist in this tenant",
            serde_json::json!({ "role": role }),
        )
    };
    RoleRef::new(role).map_err(|_| unknown_role())?;
    let mut conn = tenant_conn(state, caller).await?;
    let (principal, kind) = require_assignable(&mut conn, principal_id).await?;
    let role_id = role_by_name(&mut conn, role)
        .await
        .map_err(internal)?
        .ok_or_else(unknown_role)?
        .id;
    let changed = match (change, kind) {
        (RoleChange::Grant, PrincipalKindTag::User) => {
            grant_role_to_user(&mut conn, principal.id, role_id, RoleSource::Direct).await
        }
        (RoleChange::Revoke, PrincipalKindTag::User) => {
            revoke_role_from_user(&mut conn, principal.id, role_id, RoleSource::Direct).await
        }
        (RoleChange::Grant, _) => {
            grant_role_to_service_account(&mut conn, principal.id, role_id).await
        }
        (RoleChange::Revoke, _) => {
            revoke_role_from_service_account(&mut conn, principal.id, role_id).await
        }
    }
    .map_err(internal)?;
    let roles = role_assignments(&mut conn, principal.id, kind).await?;
    conn.commit().await.map_err(internal)?;
    Ok(RoleAssignmentChange {
        principal_id: PrincipalId::new(principal.id),
        kind,
        role: role.to_owned(),
        changed,
        roles,
    })
}

/// Resolve an assignable principal in the caller's tenant and its kind.
///
/// Unknown, deleted, foreign-tenant, tenant-administrator, and `system` ids
/// all answer with the same non-enumerating not-found error.
///
/// # Errors
/// Returns [`WyrdError::PrincipalNotFound`] when no assignable principal has
/// this id, and an internal error when the lookup fails.
async fn require_assignable(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
) -> Result<(AssignablePrincipalRow, PrincipalKindTag), WyrdErrorResponse> {
    let principal = assignable_principal(conn, principal_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            WyrdErrorResponse::from(WyrdError::PrincipalNotFound {
                message: "principal not found in this tenant".to_owned(),
                details: serde_json::json!({ "id": principal_id.to_string() }),
            })
        })?;
    let kind = assignable_kind(&principal.kind)?;
    Ok((principal, kind))
}

/// Read a principal's assignments ordered by Role, then source.
///
/// Service and Agent assignments have one source, `direct`.
///
/// # Errors
/// Returns an internal error when the read fails.
async fn role_assignments(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    kind: PrincipalKindTag,
) -> Result<Vec<RoleAssignment>, WyrdErrorResponse> {
    let assignments = if kind == PrincipalKindTag::User {
        list_user_role_assignments(conn, principal_id)
            .await
            .map_err(internal)?
    } else {
        list_service_account_roles(conn, principal_id)
            .await
            .map_err(internal)?
            .into_iter()
            .map(|role| (role, RoleSource::Direct))
            .collect()
    };
    Ok(assignments
        .into_iter()
        .map(|(role, source)| RoleAssignment { role, source })
        .collect())
}

/// Project a directory row onto its wire summary.
///
/// # Errors
/// Returns an internal error for a stored kind or status the directory query
/// cannot produce.
fn summary(row: AssignablePrincipalRow) -> Result<PrincipalSummary, WyrdErrorResponse> {
    let kind = assignable_kind(&row.kind)?;
    let status = match row.status.as_str() {
        "active" => PrincipalStatus::Active,
        "suspended" => PrincipalStatus::Suspended,
        other => return Err(internal(format!("unexpected principal status {other:?}"))),
    };
    Ok(PrincipalSummary {
        principal_id: PrincipalId::new(row.id),
        kind,
        status,
        email: row.email,
        name: row.name,
        card_ref: row.card_ref.map(|card_ref| card_ref.0),
    })
}

/// Decode an assignable principal's stored kind label.
///
/// # Errors
/// Returns an internal error for a label the directory query cannot produce.
fn assignable_kind(kind: &str) -> Result<PrincipalKindTag, WyrdErrorResponse> {
    match kind {
        "user" => Ok(PrincipalKindTag::User),
        "service" => Ok(PrincipalKindTag::Service),
        "agent" => Ok(PrincipalKindTag::Agent),
        other => Err(internal(format!("unexpected principal kind {other:?}"))),
    }
}

/// The validation refusal for a malformed request on this surface.
fn invalid(message: &str, details: serde_json::Value) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Validation {
        message: message.to_owned(),
        details,
    })
}

/// Project a stored credential row onto its non-secret wire metadata.
fn metadata(row: ApiKeyMetadataRow) -> CredentialMetadata {
    CredentialMetadata {
        id: row.id,
        prefix: row.prefix,
        created_at: row.created_at.to_rfc3339(),
        expires_at: row.expires_at.map(|at| at.to_rfc3339()),
        revoked_at: row.revoked_at.map(|at| at.to_rfc3339()),
        last_used_at: row.last_used_at.map(|at| at.to_rfc3339()),
    }
}

/// Map any internal failure onto the stable catalog without leaking detail.
fn internal(error: impl Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(internal_failure(
        "tenant principal administration failed",
        &error,
    ))
}
