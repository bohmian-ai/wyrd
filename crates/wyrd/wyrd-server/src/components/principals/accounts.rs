//! Personal account operations for tenant Users and the operator-issued
//! local enrollment and reset codes.
//!
//! A User reads its own safe profile (`GET /principals/me`) and manages its
//! own named API keys through the existing credential routes, authorized by
//! the self capabilities `personal_credentials:read` and
//! `personal_credentials:write` against the verified caller — never a
//! submitted id. A User key is bounded by an explicit permission ceiling the
//! caller and the User both hold, and creating one for oneself needs a
//! password or provider browser session at most five minutes old. Tenant
//! administrators issue one-use invitations and password resets; their codes
//! are returned once, `no-store`, and only their SHA-256 is kept.

use std::collections::BTreeSet;

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use secrecy::ExposeSecret as _;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wyrd_auth::browser_sessions::permission_names;
use wyrd_auth::issue_api_key::WyrdApiKey;
use wyrd_auth::mail::InvitationMail;
use wyrd_auth::passwords::{
    canonical_username, create_local_user, hash_password, invitation_recipients, issue_invitations,
    issue_password_reset, new_password, reissue,
};
use wyrd_runtime::{Permission, PrincipalKind};
use wyrd_spec::auth::{
    AbsoluteUrl, AccountAuthentication, CREDENTIAL_CURSOR_MAX_BYTES, CREDENTIAL_PAGE_DEFAULT,
    CREDENTIAL_PAGE_MAX, CreateUserCredentialRequest, CreateUserInvitationRequest,
    CreateUserRequest, CreatedUser, CredentialListQuery, CredentialListResponse,
    CurrentUserProfile, InvitationDelivery, IssuedCredential, IssuedUserInvitation,
    IssuedUserInvitations, IssuedUserPasswordReset, PrincipalId, PrincipalKindTag, SecretBearer,
    Sha256Hex, USER_CREDENTIAL_MAX_LIFETIME_SECONDS, USER_CREDENTIAL_NAME_MAX_CHARS,
    UserInvitationList, UserInvitationView, UserPasswordResetView,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    insert_user_api_key, local_username, page_api_key_metadata, pending_invitations,
    record_invitation_delivery, revoke_invitation,
};

use crate::audit;
use crate::auth::RequestOrigin;
use crate::components::auth::Caller;
use crate::http::error::{WyrdErrorResponse, body_rejection, internal_failure, path_rejection};
use crate::state::AppState;

/// Whether `caller` is the real User `principal_id` names.
pub(crate) fn is_self(caller: &Caller, principal_id: Uuid) -> bool {
    caller.principal.kind == PrincipalKind::User && caller.principal.id.as_uuid() == principal_id
}

/// Whether `caller` holds tenant principal administration.
pub(crate) fn is_principal_admin(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission::service_accounts_write())
}

/// The position a credential page resumes after, bound to its tenant and
/// principal.
#[derive(Serialize, Deserialize)]
struct Cursor {
    /// Tenant the page was read in.
    t: Uuid,
    /// Principal whose credentials were listed.
    p: Uuid,
    /// Creation time of the last returned row.
    c: DateTime<Utc>,
    /// Id of the last returned row.
    i: Uuid,
}

/// The validation refusal for a malformed request on this surface.
fn invalid(message: &str, field: &str) -> WyrdError {
    WyrdError::Validation {
        message: message.to_owned(),
        details: serde_json::json!({ "field": field }),
    }
}

/// The internal failure for a store error on this surface.
fn internal(error: impl std::fmt::Display) -> WyrdError {
    internal_failure("personal account operation failed", &error)
}

/// Read one page of `principal_id`'s credential metadata on `conn` for
/// `query`: at most `limit` (default 50, at most 100) rows, newest first,
/// resuming strictly after the cursor, with `next_cursor` only when more rows
/// remain.
///
/// # Errors
/// Returns [`WyrdError::Validation`] for a limit outside 1–100 or a cursor
/// that is empty, oversized, malformed, or bound to another tenant or
/// principal, and an internal error when the read fails.
pub(crate) async fn credential_page(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    query: &CredentialListQuery,
) -> Result<CredentialListResponse, WyrdError> {
    let limit = query.limit.unwrap_or(CREDENTIAL_PAGE_DEFAULT);
    if !(1..=CREDENTIAL_PAGE_MAX).contains(&limit) {
        return Err(invalid("limit must be between 1 and 100", "limit"));
    }
    let tenant = conn.data_tenant_id().as_uuid();
    let after = query
        .cursor
        .as_deref()
        .map(|raw| {
            let decoded = (!raw.is_empty() && raw.len() <= CREDENTIAL_CURSOR_MAX_BYTES)
                .then(|| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(raw)
                        .ok()
                })
                .flatten()
                .and_then(|bytes| serde_json::from_slice::<Cursor>(&bytes).ok())
                .filter(|cursor| cursor.t == tenant && cursor.p == principal_id);
            decoded
                .map(|cursor| (cursor.c, cursor.i))
                .ok_or_else(|| invalid("cursor is not a continuation of this listing", "cursor"))
        })
        .transpose()?;
    let mut rows = page_api_key_metadata(conn, principal_id, after, limit + 1)
        .await
        .map_err(internal)?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next_cursor = rows.last().filter(|_| more).map(|last| {
        let cursor = Cursor {
            t: tenant,
            p: principal_id,
            c: last.created_at,
            i: last.id,
        };
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&cursor).unwrap_or_default())
    });
    Ok(CredentialListResponse {
        credentials: rows.into_iter().map(super::routes::metadata).collect(),
        next_cursor,
    })
}

/// Issue a named User key for `user_id` bounded by `request`, on `conn`.
///
/// The name is trimmed to 1–64 characters, the lifetime is 1 second to 365
/// days, and the ceiling is a nonempty list of distinct permission
/// expressions, each held both by the caller and by the User's current
/// grants, stored in canonical sorted order.
///
/// # Errors
/// Returns [`WyrdError::Validation`] for a request outside those bounds or a
/// ceiling permission either party lacks, and an internal error when the
/// issuer or a write fails.
pub(crate) async fn mint_user_credential(
    state: &AppState,
    conn: &mut TenantConn<'_>,
    caller: &Caller,
    user_id: Uuid,
    request: CreateUserCredentialRequest,
) -> Result<IssuedCredential, WyrdError> {
    let name = request.name.trim();
    if name.is_empty() || name.chars().count() > USER_CREDENTIAL_NAME_MAX_CHARS {
        return Err(invalid("name must be 1-64 characters", "name"));
    }
    if !(1..=USER_CREDENTIAL_MAX_LIFETIME_SECONDS).contains(&request.expires_in) {
        return Err(invalid(
            "expires_in must be between 1 and 31536000 seconds",
            "expires_in",
        ));
    }
    let mut ceiling = Vec::with_capacity(request.permission_ceiling.len());
    for expression in &request.permission_ceiling {
        let permission = expression.parse::<Permission>().map_err(|_| {
            invalid(
                "ceiling entries must be resource:action permissions",
                "permission_ceiling",
            )
        })?;
        ceiling.push(permission);
    }
    let canonical = ceiling
        .iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    if ceiling.is_empty() || canonical.len() != ceiling.len() {
        return Err(invalid(
            "permission_ceiling must be a nonempty list of distinct permissions",
            "permission_ceiling",
        ));
    }
    let issuer = state
        .auth
        .tenant_issuer(state.bifrost.scribe_outbox())
        .ok_or_else(|| internal("no tenant issuer is configured"))?;
    let held = issuer
        .current_permissions(conn, user_id)
        .await
        .map_err(WyrdError::from)?
        .unwrap_or_default();
    let caller_held = &caller.principal.effective_permissions;
    if let Some(excess) = ceiling
        .iter()
        .find(|permission| !held.contains(permission) || !caller_held.contains(permission))
    {
        return Err(WyrdError::Validation {
            message: "the ceiling names a permission the caller or the User does not hold"
                .to_owned(),
            details: serde_json::json!({ "field": "permission_ceiling", "permission": excess.to_string() }),
        });
    }
    let plaintext = WyrdApiKey::generate(conn.data_tenant_id());
    let key_hash = wyrd_auth_issue::hash_secret(plaintext.secret.expose_secret());
    let credential_id = Uuid::now_v7();
    insert_user_api_key(
        conn,
        credential_id,
        user_id,
        &plaintext.prefix,
        &key_hash,
        caller.attribution(),
        name,
        &canonical.into_iter().collect::<Vec<_>>(),
        std::time::Duration::from_secs(request.expires_in),
    )
    .await
    .map_err(internal)?;
    Ok(IssuedCredential {
        id: credential_id,
        credential: SecretBearer::new(plaintext.secret.expose_secret().to_owned()),
    })
}

/// The caller's tenant route key.
///
/// # Errors
/// Returns an internal error when the directory read fails or the tenant has
/// left it.
async fn tenant_slug(state: &AppState, caller: &Caller) -> Result<TenantSlug, WyrdError> {
    state
        .postgres
        .wyrd()
        .tenant_directory_entry(caller.data_tenant_id)
        .await
        .map_err(internal)?
        .map(|(slug, _)| slug)
        .ok_or_else(|| internal("tenant is not in the directory"))
}

/// Open a transaction scoped to the caller's tenant.
///
/// # Errors
/// Returns an internal error when the connection cannot be opened.
async fn tenant_conn<'a>(
    state: &'a AppState,
    caller: &Caller,
) -> Result<TenantConn<'a>, WyrdError> {
    state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(internal)
}

/// Authorize a local-account administration operation: User management plus
/// tenant administration, each decision staged.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when either is not held.
fn authorize_user_admin(
    state: &AppState,
    caller: &Caller,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdError> {
    audit::authorize(
        state,
        caller,
        &Permission::users_manage(),
        operation,
        resource,
    )?;
    audit::authorize(state, caller, &Permission::wildcard(), operation, resource)
}

/// The fixed tenant page at `path` under `origin`.
///
/// # Errors
/// Returns an internal error when the URL cannot be formed.
fn tenant_page(
    origin: &url::Url,
    tenant: &TenantSlug,
    page: &str,
) -> Result<AbsoluteUrl, WyrdError> {
    let url = origin
        .join(&format!("/t/{tenant}/{page}"))
        .map_err(internal)?;
    AbsoluteUrl::new(url.to_string()).map_err(internal)
}

/// Read the calling User's safe profile.
///
/// # Errors
/// Returns `WYRD_PERMISSION_403_DENIED_RBAC` for a caller that is not a User.
#[utoipa::path(
    get,
    path = "/principals/me",
    responses(
        (status = 200, description = "The calling User's safe profile", body = CurrentUserProfile),
        (status = 401, description = "The request carried no usable credential \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The caller is not a User \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn current_user(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<CurrentUserProfile>, WyrdErrorResponse> {
    Ok(Json(current_user_profile(&state, &caller).await?))
}

/// The calling User's safe profile, behind both the HTTP route and the MCP
/// tool.
///
/// # Errors
/// As [`current_user`].
pub(crate) async fn current_user_profile(
    state: &AppState,
    caller: &Caller,
) -> Result<CurrentUserProfile, WyrdError> {
    if caller.principal.kind != PrincipalKind::User {
        return Err(WyrdError::PermissionDeniedRbac {
            message: "only a User principal has a personal profile".to_owned(),
            details: serde_json::json!({ "required": "user" }),
        });
    }
    let tenant = tenant_slug(state, caller).await?;
    let mut conn = tenant_conn(state, caller).await?;
    let username = local_username(&mut conn, caller.principal.id.as_uuid())
        .await
        .map_err(internal)?;
    conn.commit().await.map_err(internal)?;
    Ok(CurrentUserProfile {
        tenant,
        tenant_id: caller.data_tenant_id,
        principal_id: PrincipalId::new(caller.principal.id.as_uuid()),
        principal_kind: PrincipalKindTag::User,
        authentication: if username.is_some() {
            AccountAuthentication::Password
        } else {
            AccountAuthentication::Sso
        },
        username,
        permissions: permission_names(
            &caller
                .principal
                .effective_permissions
                .flatten()
                .unwrap_or_default(),
        ),
    })
}

/// Create a local password User directly, holding the named Roles.
///
/// The password meets the same policy as signup and is stored only as its
/// verifier; derivation runs before the tenant transaction opens.
///
/// # Errors
/// Returns a stable Wyrd error when the caller lacks User management and
/// tenant administration, the username or password is outside policy, the
/// username is taken, the Role names are empty, repeated or unknown, or a
/// write fails.
#[utoipa::path(
    post,
    path = "/principals/users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created", body = CreatedUser),
        (status = 400, description = "The username or password is outside policy, the \
          username is taken, or the Role names are empty, repeated or unknown \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable credential \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "User management and tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
pub(crate) async fn create_user(
    State(state): State<AppState>,
    caller: Caller,
    request: Result<Json<CreateUserRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<CreatedUser>), WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    authorize_user_admin(&state, &caller, "auth.user.create", "users")?;
    let username = canonical_username(&request.username)?;
    let password = new_password(&request.password.into_secret_string(), "password")?;
    let verifier = hash_password(password).await?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let user_id = create_local_user(&mut conn, &username, &verifier, &request.assignments).await?;
    conn.commit().await.map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedUser {
            principal_id: PrincipalId::new(user_id),
            username,
        }),
    ))
}

/// The metadata of `row` in `tenant`.
fn invitation_view(
    row: wyrd_sql::queries::auth::InvitationRow,
    tenant: &TenantSlug,
) -> UserInvitationView {
    UserInvitationView {
        id: row.id,
        tenant: tenant.clone(),
        delivery: row.delivery(),
        recipient: row.recipient,
        assignments: row.assignments.0,
        created_at: row.created_at,
        expires_at: row.expires_at,
    }
}

/// `row` with its link: the tenant signup page carrying `code` in the fragment.
///
/// # Errors
/// Returns `WYRD_SPEC_500_INTERNAL` when the trusted origin cannot form a URL.
fn issued_invitation(
    origin: &url::Url,
    tenant: &TenantSlug,
    row: wyrd_sql::queries::auth::InvitationRow,
    code: &SecretBearer,
) -> Result<IssuedUserInvitation, WyrdError> {
    let mut link =
        url::Url::parse(tenant_page(origin, tenant, "signup")?.as_str()).map_err(internal)?;
    link.set_fragment(Some(&format!("invitation={}", code.expose())));
    Ok(IssuedUserInvitation {
        invitation: invitation_view(row, tenant),
        invitation_link: SecretBearer::new(link.to_string()),
    })
}

/// Email each freshly committed invitation through the deployment relay and
/// record its outcome beside it; `codes` pairs with `invitations`. Runs after
/// the issuing transaction commits, so no tenant transaction waits on SMTP.
/// Without a relay every outcome stays `not_emailed`. A failure to record an
/// outcome is logged; the response still reports it.
async fn email_invitations(
    state: &AppState,
    caller: &Caller,
    tenant: &TenantSlug,
    invitations: &mut [IssuedUserInvitation],
    codes: &[SecretBearer],
) {
    let Some(mail) = state.auth.application_mail.as_ref() else {
        return;
    };
    let outgoing: Vec<_> = invitations
        .iter()
        .map(|issued| InvitationMail {
            recipient: &issued.invitation.recipient,
            link: issued.invitation_link.expose(),
            expires_at: issued.invitation.expires_at,
        })
        .collect();
    let outcomes = mail.send_invitations(tenant.as_str(), &outgoing).await;
    let mut sent = Vec::new();
    for ((issued, code), outcome) in invitations.iter_mut().zip(codes).zip(outcomes) {
        issued.invitation.delivery = outcome;
        if outcome != InvitationDelivery::NotEmailed {
            sent.push((Sha256Hex::digest(code.expose().as_bytes()), outcome));
        }
    }
    if sent.is_empty() {
        return;
    }
    let recorded = async {
        let mut conn = tenant_conn(state, caller).await?;
        for (code_hash, outcome) in &sent {
            record_invitation_delivery(&mut conn, code_hash, *outcome)
                .await
                .map_err(internal)?;
        }
        conn.commit().await.map_err(internal)
    };
    if let Err(error) = recorded.await {
        tracing::error!(error = %error, "invitation send outcomes were not recorded");
    }
}

/// Issue one one-use, twenty-four-hour enrollment link per recipient.
///
/// # Errors
/// Returns a stable Wyrd error when the caller lacks User management and
/// tenant administration, a recipient or assignment is unusable, or a write
/// fails; then no invitation is issued.
#[utoipa::path(
    post,
    path = "/principals/users/invitations",
    request_body = CreateUserInvitationRequest,
    responses(
        (status = 201, description = "One invitation per recipient; each link is returned once, \
          beside what became of emailing it",
          body = IssuedUserInvitations),
        (status = 400, description = "A recipient is unusable or repeated, or an assignment is \
          empty, repeated or unknown (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable credential \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "User management and tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
pub(crate) async fn create_user_invitation(
    State(state): State<AppState>,
    caller: Caller,
    RequestOrigin(origin): RequestOrigin,
    request: Result<Json<CreateUserInvitationRequest>, JsonRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    authorize_user_admin(&state, &caller, "auth.user.invite", "users")?;
    let recipients = invitation_recipients(&request.recipients)?;
    let tenant = tenant_slug(&state, &caller).await?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let rows = issue_invitations(
        &mut conn,
        caller.principal.id.as_uuid(),
        &recipients,
        &request.assignments,
    )
    .await?;
    conn.commit().await.map_err(internal)?;
    let (mut invitations, codes) = rows
        .into_iter()
        .map(|(row, code)| Ok((issued_invitation(&origin, &tenant, row, &code)?, code)))
        .collect::<Result<(Vec<_>, Vec<_>), WyrdError>>()?;
    email_invitations(&state, &caller, &tenant, &mut invitations, &codes).await;
    Ok(crate::auth::oauth::no_store(
        StatusCode::CREATED,
        Json(IssuedUserInvitations { invitations }),
    ))
}

/// List the tenant's pending enrollment invitations, never their codes.
///
/// # Errors
/// Returns a stable Wyrd error when the caller lacks User management and
/// tenant administration or the read fails.
#[utoipa::path(
    get,
    path = "/principals/users/invitations",
    responses(
        (status = 200, description = "Unexpired, unrevoked and unconsumed invitations, oldest first",
          body = UserInvitationList),
        (status = 403, description = "User management and tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn list_user_invitations(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<UserInvitationList>, WyrdErrorResponse> {
    authorize_user_admin(&state, &caller, "auth.user.invite.list", "users")?;
    let tenant = tenant_slug(&state, &caller).await?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let rows = pending_invitations(&mut conn).await.map_err(internal)?;
    conn.commit().await.map_err(internal)?;
    Ok(Json(UserInvitationList {
        invitations: rows
            .into_iter()
            .map(|row| invitation_view(row, &tenant))
            .collect(),
    }))
}

/// Replace a pending invitation's link, voiding the previous one, and return
/// the new link once.
///
/// # Errors
/// Returns a stable Wyrd error when the caller lacks User management and
/// tenant administration, no unconsumed, unrevoked invitation has the id, or
/// a write fails.
#[utoipa::path(
    post,
    path = "/principals/users/invitations/{invitation_id}/resend",
    params(("invitation_id" = Uuid, Path, description = "Invitation to resend")),
    responses(
        (status = 200, description = "The invitation's new link, returned once, beside what \
          became of emailing it", body = IssuedUserInvitation),
        (status = 400, description = "The id is not a UUID (WYRD_SPEC_400_VALIDATION)",
          body = WyrdProblem),
        (status = 403, description = "User management and tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No unconsumed, unrevoked invitation has this id \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn resend_user_invitation(
    State(state): State<AppState>,
    caller: Caller,
    RequestOrigin(origin): RequestOrigin,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Path(id) = id.map_err(|rejection| path_rejection(&rejection))?;
    authorize_user_admin(
        &state,
        &caller,
        "auth.user.invite.resend",
        &format!("invitation:{id}"),
    )?;
    let tenant = tenant_slug(&state, &caller).await?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let reissued = reissue(&mut conn, id).await?;
    conn.commit().await.map_err(internal)?;
    let Some((row, code)) = reissued else {
        return Err(WyrdErrorResponse::from(WyrdError::NotFound {
            message: "no unconsumed, unrevoked invitation has this id".to_owned(),
            details: serde_json::json!({}),
        }));
    };
    let mut issued = [issued_invitation(&origin, &tenant, row, &code)?];
    email_invitations(&state, &caller, &tenant, &mut issued, &[code]).await;
    let [issued] = issued;
    Ok(crate::auth::oauth::no_store(StatusCode::OK, Json(issued)))
}

/// Revoke an unconsumed enrollment invitation.
///
/// # Errors
/// Returns a stable Wyrd error when the caller lacks User management and
/// tenant administration, no live invitation has the id, or a write fails.
#[utoipa::path(
    delete,
    path = "/principals/users/invitations/{invitation_id}",
    params(("invitation_id" = Uuid, Path, description = "Invitation to revoke")),
    responses(
        (status = 204, description = "Invitation revoked"),
        (status = 400, description = "The id is not a UUID (WYRD_SPEC_400_VALIDATION)",
          body = WyrdProblem),
        (status = 403, description = "User management and tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No unconsumed invitation has this id \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn revoke_user_invitation(
    State(state): State<AppState>,
    caller: Caller,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let Path(id) = id.map_err(|rejection| path_rejection(&rejection))?;
    authorize_user_admin(
        &state,
        &caller,
        "auth.user.invite.revoke",
        &format!("invitation:{id}"),
    )?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let revoked = revoke_invitation(&mut conn, id).await.map_err(internal)?;
    conn.commit().await.map_err(internal)?;
    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(WyrdErrorResponse::from(WyrdError::NotFound {
            message: "no unconsumed invitation has this id".to_owned(),
            details: serde_json::json!({}),
        }))
    }
}

/// Issue a one-use, fifteen-minute password reset for a local User.
///
/// # Errors
/// Returns a stable Wyrd error when the caller lacks User management and
/// tenant administration, the id names no local password User, or a write
/// fails.
#[utoipa::path(
    post,
    path = "/principals/users/{principal_id}/password-reset",
    params(("principal_id" = Uuid, Path, description = "Local User to reset")),
    responses(
        (status = 201, description = "Reset issued; its code is returned once",
          body = IssuedUserPasswordReset),
        (status = 400, description = "The id is not a UUID (WYRD_SPEC_400_VALIDATION)",
          body = WyrdProblem),
        (status = 403, description = "User management and tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No local password User has this id \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn create_password_reset(
    State(state): State<AppState>,
    caller: Caller,
    RequestOrigin(origin): RequestOrigin,
    principal_id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Path(principal_id) = principal_id.map_err(|rejection| path_rejection(&rejection))?;
    authorize_user_admin(
        &state,
        &caller,
        "auth.user.password_reset",
        &format!("principal:{principal_id}"),
    )?;
    let tenant = tenant_slug(&state, &caller).await?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let issued =
        issue_password_reset(&mut conn, caller.principal.id.as_uuid(), principal_id).await?;
    conn.commit().await.map_err(internal)?;
    let Some((row, code)) = issued else {
        return Err(WyrdErrorResponse::from(WyrdError::PrincipalNotFound {
            message: "no local password User has this id".to_owned(),
            details: serde_json::json!({ "id": principal_id.to_string() }),
        }));
    };
    let issued = IssuedUserPasswordReset {
        reset_url: tenant_page(&origin, &tenant, "reset")?,
        reset: UserPasswordResetView {
            id: row.id,
            tenant,
            principal_id: PrincipalId::new(row.user_id),
            created_at: row.created_at,
            expires_at: row.expires_at,
        },
        reset_code: code,
    };
    Ok(crate::auth::oauth::no_store(
        StatusCode::CREATED,
        Json(issued),
    ))
}
