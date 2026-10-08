//! HTTP routes for the tenant auth surfaces: the OAuth authorization
//! server (authorization, token, device authorization, revocation, and
//! metadata endpoints), the OIDC provider callback, and Card-bound API key
//! issuance.

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};

use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_auth::callback::{AuthorizationCodeExchange, LoginCompletion};
use wyrd_auth_verify::{AccessTokenClaims, VerifiedToken};
use wyrd_spec::auth::{
    CallbackQuery, ExchangeTokenType, IssueKeyRequest, OAuthClientId, OAuthErrorCode,
    OAuthErrorResponse, SecretBearer, TokenAudience, TokenRequest, TokenResponse,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::request_id::RequestId;
use wyrd_sql::TenantConn;

use crate::auth::authorize::{client_redirect, error_name};
use crate::auth::callback::exchange_authorization_code;
use crate::auth::card_scope::{
    MINT_KIND_API_KEY_EXCHANGE, MINT_KIND_REFRESH, stage_scope_mint_failure_audit,
};
use crate::auth::exchange_api_key::{
    DelegateToken, ExchangeApiKey, api_key_invalid, map_exchange_error_to_wyrd,
};
use crate::auth::issue_api_key::{IssueApiKey, WyrdApiKey};
use crate::auth::jwt_bearer::exchange_jwt_bearer;
use crate::auth::oauth::{ClientForm, GRANT_TYPES, OAuthError, OAuthForm, no_store};
use crate::auth::refresh::{RefreshError, RefreshTokens, tenant_from_refresh_jwt};
use crate::components::auth::{AuthenticatedPrincipal, Caller};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build the tenant-plane auth router.
///
/// Mounts the auth surfaces — the authorization endpoint
/// (`GET /auth/authorize`), the CLI device authorization
/// (`POST /auth/device_authorization`) and its verification page
/// (`GET`/`POST /auth/device`), the common OIDC provider callback
/// (`GET /auth/callback`), the token endpoint (`POST /auth/token`), token
/// revocation (`POST /auth/revoke`), authorization server metadata, and API
/// key issuance (`POST /auth/issue-key`). Device user-code attempts are
/// limited per client address at the gateway (RFC 8628 §5.1), which sees the
/// real client and one budget across replicas; the token endpoint enforces
/// the device polling interval with `slow_down`.
pub fn auth_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(crate::auth::authorize::authorize))
        .routes(routes!(crate::auth::authorize::metadata))
        .routes(routes!(crate::auth::cli_login::device_authorization))
        .routes(routes!(
            crate::auth::cli_login::device_page,
            crate::auth::cli_login::device_decision
        ))
        .routes(routes!(crate::auth::cli_login::revoke))
        .routes(routes!(callback))
        .routes(routes!(token))
        .routes(routes!(issue_key))
}

/// `POST /auth/token` — the OAuth token endpoint.
///
/// Every tenant grant arrives here as form parameters and leaves with the
/// same RFC 6749 §5.1 [`TokenResponse`]: an authorization code (RFC 6749
/// §4.1.3, `wyrd-ui`), a refresh token (§6), an RFC 8628 device code
/// (`wyrd-cli`), an RFC 8693 token exchange — a Wyrd API key as subject, or
/// a delegation — and an RFC 7523 workload assertion. The human grants
/// require their client: `wyrd-ui` authenticates with its Basic secret and
/// `wyrd-cli` names itself with `client_id`. Tenant and principal are derived
/// from the verified credential, never from a client-supplied header.
///
/// # Errors
/// Answers the RFC 6749 §5.2 body: `invalid_request` for a malformed
/// request — and, per RFC 8693 §2.2.2, an unusable subject or actor token —
/// `invalid_target` for a token exchange naming an unsupported `audience`,
/// `invalid_client` (`401`) for a missing or refused client,
/// `unauthorized_client` for a grant another client must use,
/// `unsupported_grant_type`, `invalid_grant` for every unusable code,
/// refresh token, device code, or assertion, the RFC 8628 §3.5 device codes,
/// and `server_error` or `temporarily_unavailable` when issuance or the store
/// fails. Each grant's audit event is staged on the process audit outbox and
/// never refuses the grant.
#[utoipa::path(
    post,
    path = "/auth/token",
    request_body(content = ClientForm<TokenRequest>,
        content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Access token issued", body = TokenResponse),
        (status = 400, description = "RFC 6749 §5.2 or RFC 8628 §3.5 refusal: \
          `invalid_request`, `invalid_grant`, `unauthorized_client`, \
          `unsupported_grant_type`, `invalid_target`, `authorization_pending`, `slow_down`, `access_denied`, \
          or `expired_token`", body = OAuthErrorResponse),
        (status = 401, description = "`invalid_client`", body = OAuthErrorResponse),
        (status = 500, description = "`server_error`", body = OAuthErrorResponse),
        (status = 503, description = "`temporarily_unavailable`", body = OAuthErrorResponse)
    ),
    // No session exists yet at this operation: a public client names itself
    // with `client_id` and needs nothing, a confidential client uses Basic.
    security((), ("oauthClientBasic" = [])),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    request_id: Option<Extension<RequestId>>,
    form: OAuthForm,
) -> Result<Response, OAuthError> {
    let client = state.auth.oauth_clients.identify(&headers, &form)?;
    match form.get("grant_type") {
        None => return Err(OAuthError(OAuthErrorCode::InvalidRequest)),
        Some(grant) if !GRANT_TYPES.contains(&grant) => {
            return Err(OAuthError(OAuthErrorCode::UnsupportedGrantType));
        }
        Some(_) => {}
    }
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    let grants = TokenGrants {
        state: &state,
        request_id: request_id.as_str(),
    };
    let required = client.ok_or(OAuthError(OAuthErrorCode::InvalidClient));
    let response = match form.token_request()? {
        TokenRequest::AuthorizationCode {
            code,
            redirect_uri,
            code_verifier,
        } => grants
            .authorization_code(&code, required?, &redirect_uri, &code_verifier)
            .await
            .map_err(OAuthError::from)?,
        TokenRequest::RefreshToken { refresh_token } => grants
            .refresh(&refresh_token, required?)
            .await
            .map_err(OAuthError::from)?,
        TokenRequest::DeviceCode { device_code } => {
            if required? != OAuthClientId::WyrdCli {
                return Err(OAuthError(OAuthErrorCode::UnauthorizedClient));
            }
            crate::auth::cli_login::cli_logins(&state)?
                .redeem(&device_code, request_id.as_str())
                .await?
        }
        TokenRequest::TokenExchange {
            subject_token,
            subject_token_type: ExchangeTokenType::ApiKey,
            actor_token: None,
            actor_token_type: None,
            audience: None,
        } => grants
            .api_key(&subject_token)
            .await
            .map_err(OAuthError::request)?,
        TokenRequest::TokenExchange {
            subject_token,
            subject_token_type: ExchangeTokenType::AccessToken,
            actor_token: Some(actor_token),
            actor_token_type: Some(ExchangeTokenType::AccessToken),
            audience: Some(audience),
        } => grants
            .delegate(&subject_token, &actor_token, audience)
            .await
            .map_err(OAuthError::request)?,
        TokenRequest::TokenExchange { .. } => {
            return Err(OAuthError(OAuthErrorCode::InvalidRequest));
        }
        TokenRequest::JwtBearer { assertion, tenant } => exchange_jwt_bearer(
            &state,
            &headers,
            assertion.into_secret_string(),
            tenant,
            request_id.as_str(),
        )
        .await
        .map_err(|error| OAuthError::from(error.0))?
        .into_response(),
    };
    Ok(no_store(StatusCode::OK, Json(response)))
}

/// The tenant grants of one token-endpoint request, over the server's auth
/// configuration and the request's id.
///
/// OTLP ingest authenticates a stock exporter's API key through the same
/// API-key verification, so it is crate-visible.
pub(crate) struct TokenGrants<'a> {
    /// Server state holding the auth owners and the runtime store.
    pub(crate) state: &'a AppState,
    /// The request id every audit event of the grant carries.
    pub(crate) request_id: &'a str,
}

impl<'a> TokenGrants<'a> {
    /// Redeem an authorization code issued to `client` (RFC 6749 §4.1.3).
    ///
    /// # Errors
    /// The refusals of [`AuthorizationCodeExchange::redeem_code`], and
    /// [`WyrdError::Internal`] when auth is not configured.
    async fn authorization_code(
        &self,
        code: &SecretBearer,
        client: OAuthClientId,
        redirect_uri: &str,
        code_verifier: &SecretBearer,
    ) -> Result<TokenResponse, WyrdError> {
        AuthorizationCodeExchange {
            issuer: self.issuer()?,
            connections: self
                .state
                .auth
                .human_connections
                .clone()
                .ok_or_else(not_configured)?,
        }
        .redeem_code(code, client, redirect_uri, code_verifier, self.request_id)
        .await
    }

    /// Exchange a Wyrd API key for an access token of its own principal.
    ///
    /// A key that does not parse names no tenant, so it is refused with the
    /// same error every invalid key earns.
    ///
    /// # Errors
    /// Returns [`WyrdError::ApiKeyInvalid`] for every unusable key, its
    /// refusal staged on the audit outbox when it names a tenant, and a store
    /// or issuance error.
    async fn api_key(&self, api_key: &SecretBearer) -> Result<TokenResponse, WyrdError> {
        let presented = SecretString::from(api_key.expose().to_owned());
        let (parsed, mut conn, exchange) = self.api_key_exchange(api_key.expose()).await?;
        let exchanged = match exchange
            .execute(&mut conn, presented, self.request_id)
            .await
        {
            Ok(exchanged) => exchanged,
            Err(error) => {
                let wyrd = map_exchange_error_to_wyrd(&mut conn, &parsed.prefix, error).await;
                stage_scope_mint_failure_audit(
                    &self.state.audit_outbox,
                    parsed.tenant_id,
                    self.request_id,
                    MINT_KIND_API_KEY_EXCHANGE,
                    &wyrd,
                );
                return Err(wyrd);
            }
        };
        conn.commit().await.map_err(store_unavailable)?;
        Ok(exchanged.into_exchange_response())
    }

    /// Authenticate one OTLP request by the API key its exporter sent, without
    /// minting a token.
    ///
    /// The same key verification as [`Self::api_key`], committed with the
    /// key's use record, yielding the verified principal the exchanged token
    /// would carry. Nothing is staged on the audit outbox: like a bearer
    /// request, the request's own authorization decisions are the audit.
    ///
    /// # Errors
    /// Returns [`WyrdError::ApiKeyInvalid`] for every unusable key, and a
    /// store or issuance error.
    pub(crate) async fn api_key_principal(
        &self,
        api_key: &SecretString,
    ) -> Result<VerifiedToken, WyrdError> {
        let (parsed, mut conn, exchange) = self.api_key_exchange(api_key.expose_secret()).await?;
        let verified = match exchange.authenticate(&mut conn, api_key).await {
            Ok(verified) => verified,
            Err(error) => {
                return Err(map_exchange_error_to_wyrd(&mut conn, &parsed.prefix, error).await);
            }
        };
        conn.commit().await.map_err(store_unavailable)?;
        Ok(verified)
    }

    /// Parse a presented API key and open its tenant's transaction, with the
    /// API-key exchange bound to this server's issuer.
    ///
    /// A key that does not parse names no tenant, so it is refused before any
    /// store access with the error every invalid key earns.
    ///
    /// # Errors
    /// Returns [`WyrdError::ApiKeyInvalid`] for an unparseable key, the
    /// not-configured error when no issuer is configured, and the
    /// store-unavailable error when the transaction cannot open.
    async fn api_key_exchange(
        &self,
        api_key: &str,
    ) -> Result<(WyrdApiKey, TenantConn<'a>, ExchangeApiKey), WyrdError> {
        let parsed = WyrdApiKey::parse(api_key).map_err(|_| api_key_invalid())?;
        let issuer = self.issuer()?;
        let conn = self
            .state
            .postgres
            .tenant_conn(parsed.tenant_id)
            .await
            .map_err(store_unavailable)?;
        Ok((parsed, conn, ExchangeApiKey { issuer }))
    }

    /// Run an RFC 8693 delegation: the holder of `actor_token` acts for the
    /// holder of `subject_token`, bound to `audience`.
    ///
    /// Both tokens are verified against the subject's tenant, so an actor
    /// token from any other tenant is refused. The exchange stages its own
    /// authorization decision, so a refusal after the policy decision is
    /// still audited.
    ///
    /// # Errors
    /// Returns [`WyrdError::BadTokenFormat`] for a subject token that is not
    /// a compact JWT, and the refusals of [`DelegateToken::execute`].
    async fn delegate(
        &self,
        subject_token: &SecretBearer,
        actor_token: &SecretBearer,
        audience: TokenAudience,
    ) -> Result<TokenResponse, WyrdError> {
        let issuer = self.issuer()?;
        let verifier = self
            .state
            .auth
            .token_verifier
            .clone()
            .ok_or_else(not_configured)?;
        let tenant_id = tenant_from_unverified_access_token(subject_token.expose())?;
        let conn = self
            .state
            .postgres
            .tenant_conn(tenant_id)
            .await
            .map_err(store_unavailable)?;
        let exchanged = DelegateToken { issuer, verifier }
            .execute(
                conn,
                SecretString::from(subject_token.expose().to_owned()),
                SecretString::from(actor_token.expose().to_owned()),
                audience,
                self.request_id,
            )
            .await?;
        Ok(exchanged.into_exchange_response())
    }

    /// Renew a human session with its refresh token for `client` (RFC 6749
    /// §6).
    ///
    /// Replay is the one refusal that also writes: detection revoked the
    /// rotation chain on this transaction and staged the canonical
    /// revocation event, so it commits before the refusal returns — rolling it
    /// back would tell the legitimate holder the theft was contained while
    /// leaving the attacker's successor usable.
    ///
    /// # Errors
    /// Returns the refusals of [`RefreshTokens::execute`], each staged on
    /// the audit outbox, and a store error.
    async fn refresh(
        &self,
        refresh_token: &SecretBearer,
        client: OAuthClientId,
    ) -> Result<TokenResponse, WyrdError> {
        let tenant_id = tenant_from_refresh_jwt(refresh_token.expose())?;
        let mut conn = self
            .state
            .postgres
            .tenant_conn(tenant_id)
            .await
            .map_err(store_unavailable)?;
        let exchanged = match (RefreshTokens {
            issuer: self.issuer()?,
        })
        .execute(
            &mut conn,
            SecretString::from(refresh_token.expose().to_owned()),
            client,
            self.request_id,
        )
        .await
        {
            Ok(exchanged) => exchanged,
            Err(error) => {
                if matches!(error, RefreshError::Reused) {
                    conn.commit().await.map_err(store_unavailable)?;
                }
                let wyrd = WyrdError::from(error);
                stage_scope_mint_failure_audit(
                    &self.state.audit_outbox,
                    tenant_id,
                    self.request_id,
                    MINT_KIND_REFRESH,
                    &wyrd,
                );
                return Err(wyrd);
            }
        };
        conn.commit().await.map_err(store_unavailable)?;
        Ok(exchanged.into_response())
    }

    /// The tenant issuance owner.
    ///
    /// # Errors
    /// Returns [`WyrdError::Internal`] when no signing key is configured.
    fn issuer(&self) -> Result<wyrd_auth::issuance::TenantTokenIssuer, WyrdError> {
        self.state
            .auth
            .tenant_issuer(&self.state.audit_outbox)
            .ok_or_else(not_configured)
    }
}

/// Static page a device-code login's browser tab shows once the callback
/// has recorded the approval. It carries no token, code, or state.
const CLI_LOGIN_COMPLETE_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<title>Wyrd sign-in complete</title></head><body><p>Sign-in complete. You can return to your \
terminal.</p></body></html>";

/// Static page a candidate connection test's browser tab shows once the
/// callback has marked the candidate revision tested. The test issued no
/// session; the page carries no token, code, or state.
const CONNECTION_TEST_COMPLETE_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<title>Wyrd connection test complete</title></head><body><p>Connection test complete. Return to \
Wyrd settings to activate the connection.</p></body></html>";

/// `GET /auth/callback` — the common provider callback for tenant human login.
///
/// Anonymous by construction: the caller is mid-login and has no Wyrd session
/// yet. The opaque `state` generated at initiation is the only input that
/// selects the tenant and connection; no header is consulted. The RFC 9207
/// `iss` parameter, when sent, is forwarded so the exchange can bind the
/// response to the login's issuer before any token request. The response
/// carries no token and not the provider code: a login begun at
/// `GET /auth/authorize` is redirected (`303`) back to its client's redirect
/// URI with a single-use Wyrd authorization code and the client's `state`
/// (RFC 6749 §4.1.2), or with an RFC 6749 §4.1.2.1 error once its state was
/// consumed — including a provider's own `error`, which the client receives
/// as `access_denied`, or as `server_error` or `temporarily_unavailable`
/// when the provider reported one of those; a device-code login records its approval and receives a static
/// page; a candidate connection test marks its bound candidate revision
/// tested, issues nothing, and receives a static page.
///
/// # Errors
/// Returns problem JSON for every refusal of
/// [`exchange_authorization_code`] that cannot be returned to a client.
#[utoipa::path(
    get,
    path = "/auth/callback",
    params(
        ("code" = Option<String>, Query, description = "Authorization code from the identity \
          provider; exactly one of `code` or `error` is present"),
        ("error" = Option<String>, Query, description = "The identity provider's RFC 6749 \
          §4.1.2.1 error code"),
        ("state" = String, Query, description = "Opaque login state Wyrd generated at initiation"),
        ("iss" = Option<String>, Query, description = "RFC 9207 authorization-response issuer; \
          when present it must equal the login's issuer, and it is required when the \
          provider's discovery advertises `authorization_response_iss_parameter_supported`")
    ),
    responses(
        (status = 200, description = "A device-code login was approved and the CLI's token \
          poll issues the session, or a candidate connection test sign-in marked that \
          candidate revision tested and issued no session; the browser shows a static page",
          content_type = "text/html", body = String),
        (status = 303, description = "An authorization-request login completed: redirect to \
          the client's redirect URI with `code` and `state`, or with an RFC 6749 §4.1.2.1 \
          `error`", headers(("Location" = String, description = "The client's redirect URI"))),
        (status = 400, description = "The query carries both or neither of `code` and \
          `error` (WYRD_SPEC_400_VALIDATION), the login state is unknown, expired, or replayed \
          (WYRD_AUTH_400_INVALID_STATE), the ID token nonce does not match \
          (WYRD_AUTH_400_INVALID_NONCE), or the deployment has no public origin \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The code or ID token is not usable, the response `iss` \
          does not match the login's issuer or is missing while the provider advertises it, or \
          the login's connection changed (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 403, description = "The principal that began a connection test no longer \
          holds identity_connections:write (WYRD_PERMISSION_403_DENIED_RBAC)",
         body = WyrdProblem),
        (status = 409, description = "The tested candidate changed during its test sign-in \
          (WYRD_AUTH_409_CONNECTION_CONFLICT)", body = WyrdProblem),
        (status = 503, description = "The identity provider or auth backend is unavailable \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn callback(
    State(state): State<AppState>,
    request_id: Option<Extension<RequestId>>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, WyrdErrorResponse> {
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    let completed = exchange_authorization_code(
        &state,
        query.response()?,
        &query.state,
        query.iss.as_deref(),
        request_id.as_str(),
    )
    .await?;
    Ok(match completed {
        LoginCompletion::Authorized {
            authorization,
            code,
        } => client_redirect(&authorization, &[("code", code.expose_secret())]),
        LoginCompletion::Refused {
            authorization,
            error,
        } => client_redirect(
            &authorization,
            &[("error", error_name(OAuthError::authorization(error)))],
        ),
        LoginCompletion::DeviceApproved => Html(CLI_LOGIN_COMPLETE_PAGE).into_response(),
        LoginCompletion::ConnectionTested => Html(CONNECTION_TEST_COMPLETE_PAGE).into_response(),
    })
}

/// `POST /auth/issue-key` — mint a Card-bound API key for an existing principal.
///
/// Gated on `service_accounts:write`. The principal must already exist — a
/// credential is issued against an identity, never in place of one — and the
/// plaintext is returned exactly once.
///
/// # Errors
/// Returns a `401` without a usable token, a `403` without
/// `service_accounts:write`, a `404` when the named principal does not exist in
/// this tenant, a `500` when the write fails, and a `503` when the
/// store is unavailable or no token verifier is configured.
#[utoipa::path(
    post,
    path = "/auth/issue-key",
    request_body = IssueKeyRequest,
    responses(
        (status = 200, description = "Key issued, plaintext returned once",
         body = wyrd_spec::auth::IssueKeyResponse),
        (status = 401, description = "An access token is required \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 403, description = "Caller lacks service_accounts:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No principal is bound to the named Card \
          (WYRD_AUTH_404_ADMIN_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The server's auth configuration is missing \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Auth"
)]
async fn issue_key(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    request_id: Option<Extension<RequestId>>,
    Json(request): Json<IssueKeyRequest>,
) -> Result<Json<wyrd_spec::auth::IssueKeyResponse>, WyrdErrorResponse> {
    let request_id_str: String;
    let req_id = if let Some(Extension(id)) = request_id.as_ref() {
        id.as_str()
    } else {
        request_id_str = Uuid::new_v4().to_string();
        &request_id_str
    };
    let audited_caller = Caller::from_authenticated(
        &caller,
        RequestId::parse(req_id).unwrap_or_else(|_| RequestId::now_v7()),
    );
    let decision = crate::audit::authorize_service_accounts_write(
        &state,
        &audited_caller,
        "issue API keys",
        "auth.api_key.issue",
        &format!("card:{}", request.card_ref.name),
    )
    .map_err(WyrdErrorResponse::from)?;

    // The decision is staged as soon as it is made; the issued key is
    // recorded separately once its transaction commits.
    state
        .audit_outbox
        .stage(audited_caller.data_tenant_id, decision);
    let tenant = caller.principal().tenant_id;
    let mut conn = state
        .postgres
        .tenant_conn(tenant)
        .await
        .map_err(|error| WyrdErrorResponse::from(store_unavailable(error)))?;
    let service = IssueApiKey::default();
    let issued = service
        .execute(&mut conn, request, caller.principal())
        .await
        .map_err(|error| WyrdErrorResponse::from(WyrdError::from(error)))?;
    conn.commit()
        .await
        .map_err(|error| WyrdErrorResponse::from(store_unavailable(error)))?;
    service.audit(
        &state.audit_outbox,
        tenant,
        &issued,
        caller.principal(),
        req_id,
    );

    Ok(Json(issued.response))
}

/// The refusal of a grant the server lacks the auth configuration for.
fn not_configured() -> WyrdError {
    crate::auth::auth_not_configured().0
}

/// The refusal of a grant whose tenant store is unavailable; the cause is
/// logged, never returned.
fn store_unavailable(error: wyrd_sql::SqlError) -> WyrdError {
    tracing::warn!(error = %error, "auth db unavailable");
    WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({}),
    }
}

/// The tenant an access token's unverified claims name, which routes a
/// delegation to the tenant both tokens are then verified against.
///
/// # Errors
/// Returns [`WyrdError::BadTokenFormat`] when `token` is not a compact JWT
/// with decodable claims.
fn tenant_from_unverified_access_token(token: &str) -> Result<wyrd_spec::DataTenantId, WyrdError> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(bad_subject_token_format)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| bad_subject_token_format())?;
    let claims: AccessTokenClaims =
        serde_json::from_slice(&bytes).map_err(|_| bad_subject_token_format())?;
    Ok(claims.principal.tenant_id)
}

/// The refusal of a subject token that is not a compact JWT.
fn bad_subject_token_format() -> WyrdError {
    WyrdError::BadTokenFormat {
        message: "subject_token is not a compact JWT".to_owned(),
        details: serde_json::json!({}),
    }
}

#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;
    use wyrd_spec::reference::CardRefScope;

    use axum::Json;
    use axum::extract::State;
    use base64::Engine;
    use sqlx::types::Json as SqlxJson;
    use uuid::Uuid;
    use wyrd_auth_verify::{AccessTokenClaims, TokenPrincipalRef};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssueKeyRequest;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::components::auth::AuthenticatedPrincipal;
    use crate::state::AppState;
    use axum::Extension;
    use sqlx::Row;
    use wyrd_spec::request_id::RequestId;
    use wyrd_sql::TenantConn;

    use super::{auth_router, issue_key, tenant_from_unverified_access_token};

    #[test]
    fn mounts_token_and_issue_key_routes() {
        let _router = auth_router();
    }

    #[test]
    fn extracts_tenant_from_unverified_access_token() {
        let tenant_id: DataTenantId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id is valid");
        let principal_id: PrincipalId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
            .parse()
            .expect("static principal id is valid");
        let claims = AccessTokenClaims {
            sub: principal_id.to_string(),
            principal: TokenPrincipalRef {
                id: principal_id,
                kind: PrincipalKindTag::User,
                tenant_id,
                card_ref: None,
                card_ref_scope: CardRefScope::default(),
            },
            roles: vec![],
            permissions: PermissionSet::default(),
            act: None,
            aud: "wyrd".to_owned(),
            exp: 9_999_999_999,
            iat: 0,
            iss: "test".to_owned(),
            jti: "01K00000000000000000000000".to_owned(),
            cid: None,
        };
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            serde_json::to_string(&claims)
                .expect("claims serialize")
                .as_bytes(),
        );
        let fake_jwt = format!("header.{payload}.sig");

        let result = tenant_from_unverified_access_token(&fake_jwt).expect("extracts tenant");
        assert_eq!(result, tenant_id);
    }

    #[test]
    fn rejects_bad_base64_payload() {
        let result = tenant_from_unverified_access_token("not.a.jwt");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_wrong_segment_count() {
        let result = tenant_from_unverified_access_token("onlyone");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_non_json_payload() {
        let garbage = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"not json at all");
        let fake_jwt = format!("header.{garbage}.sig");

        let result = tenant_from_unverified_access_token(&fake_jwt);
        assert!(result.is_err());
    }

    fn service_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("issue-key-target").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn caller_with(
        id: Uuid,
        tenant: DataTenantId,
        permissions: PermissionSet,
    ) -> AuthenticatedPrincipal {
        AuthenticatedPrincipal::from_verified(Arc::new(wyrd_auth_verify::VerifiedToken {
            principal: Principal {
                id: PrincipalId::new(id),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: permissions,
                credential_id: None,
            },
            delegation_chain: Vec::new(),
            exp: chrono::Utc::now() + chrono::Duration::minutes(5),
        }))
    }

    async fn fixture_state(fixture: &PgFixture) -> AppState {
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let dir = tempfile::tempdir().expect("routes storage tempdir");
        let storage_root = dir.keep().join("issue-key-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
    }

    async fn insert_test_user(conn: &mut TenantConn<'_>, tenant: DataTenantId) -> Uuid {
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
             VALUES ($1, $2, $3, 'password', 'active')",
        )
        .bind(user_id)
        .bind(tenant.as_uuid())
        .bind(format!("test-{user_id}@example.com"))
        .execute(&mut **conn.transaction())
        .await
        .expect("test user inserts");
        user_id
    }

    async fn insert_test_service_account(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        created_by: Uuid,
        card_ref: &CardRef,
    ) -> Uuid {
        let sa_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, card_kind, card_uid, card_ref, space, name, version, status, created_by)
             VALUES ($1, $2, 'service', 'Service', $3, $4, $5, $6, $7, 'active', $8)",
        )
        .bind(sa_id)
        .bind(tenant.as_uuid())
        .bind(Uuid::new_v4())
        .bind(SqlxJson(card_ref.clone()))
        .bind(
            card_ref
                .space
                .as_ref()
                .expect("fixture service card ref has a resolved space")
                .as_str(),
        )
        .bind(format!("svc-{sa_id}"))
        .bind(card_ref.version.as_str())
        .bind(created_by)
        .execute(&mut **conn.transaction())
        .await
        .expect("service account inserts");
        sa_id
    }

    #[tokio::test]
    /// A caller without `service_accounts:write` is refused with the RBAC
    /// denial. The denial row commits to the real fixture database, so the
    /// refusal is not masked by an unreachable audit store.
    async fn issue_key_without_permission_is_rbac_denied() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = fixture_state(&fixture).await;
        let caller = caller_with(Uuid::new_v4(), tenant, PermissionSet::new());
        let request = IssueKeyRequest {
            card_ref: service_card_ref(),
            label: None,
            expires_in_seconds: None,
        };

        let error = issue_key(State(state), caller, None, Json(request))
            .await
            .expect_err("missing permission is denied");

        assert!(
            matches!(error.0, WyrdError::PermissionDeniedRbac { .. }),
            "expected an RBAC denial, got {:?}",
            error.0
        );
    }

    /// A caller holding `service_accounts:write` mints a key for a Card-bound
    /// Service principal: the response names the stored credential, and one
    /// issuance audit row records the issuing actor, target principal, tenant,
    /// and request id.
    ///
    /// # Panics
    /// Panics when seeding or issuance fails, the response names a different
    /// Card or credential than the stored row, or the staged issuance audit row
    /// is missing or records the wrong actor, target, tenant, or request id.
    #[tokio::test]
    async fn issue_key_with_permission_mints_response() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let card_ref = service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, tenant, creator, &card_ref).await;
        conn.commit().await.expect("seed commits");

        let state = fixture_state(&fixture).await;
        let caller = caller_with(
            creator,
            tenant,
            PermissionSet::from_iter([Permission::service_accounts_write()]),
        );
        let request = IssueKeyRequest {
            card_ref: card_ref.clone(),
            label: None,
            expires_in_seconds: None,
        };

        let request_id = RequestId::parse("01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff")
            .expect("static request id is valid");
        let expected_request_id = request_id.as_str().to_owned();

        let response = issue_key(
            State(state.clone()),
            caller,
            Some(Extension(request_id)),
            Json(request),
        )
        .await
        .expect("issue key succeeds");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        assert_eq!(
            state.audit_outbox.settle(deadline).await,
            0,
            "audit settles"
        );

        assert_eq!(response.0.card_ref, card_ref);
        assert!(!response.0.prefix.is_empty());

        let mut verify_conn = fixture.tenant_conn().await.expect("verify conn opens");
        let stored_key_id: Uuid =
            sqlx::query_scalar("SELECT id FROM wyrd.auth_api_keys WHERE prefix = $1")
                .bind(&response.0.prefix)
                .fetch_one(&mut **verify_conn.transaction())
                .await
                .expect("issued key row reads");
        assert_eq!(
            response.0.key_id, stored_key_id,
            "the response names the issued credential's UUID"
        );
        let row = sqlx::query(
            "SELECT principal_id, detail, request_id, data_tenant_id
             FROM vala.audit_staging
             WHERE data_tenant_id = $1
               AND operation = 'auth.api_key.issue'
               AND detail LIKE '%credential_issuance%'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&mut **verify_conn.transaction())
        .await
        .expect("credential issuance audit event was staged");

        let audit_actor: Uuid = row.get("principal_id");
        let detail: serde_json::Value =
            serde_json::from_str(row.get("detail")).expect("audit detail is json");
        let audit_target: Uuid = detail["target_principal_id"]
            .as_str()
            .expect("target principal id is present")
            .parse()
            .expect("target principal id is a uuid");
        let audit_request_id: String = row.get("request_id");
        let audit_tenant: Uuid = row.get("data_tenant_id");
        assert_eq!(audit_actor, creator, "audit records the issuing actor");
        assert_eq!(
            audit_target, sa_id,
            "audit records the target service account"
        );
        assert_eq!(audit_tenant, tenant.as_uuid(), "audit is tenant-scoped");
        assert_eq!(
            audit_request_id, expected_request_id,
            "audit records the request id"
        );
    }

    /// A failure after authorization records its allowance and no key.
    ///
    /// The caller holds `service_accounts:write`, so the decision is an
    /// allowance — but the named Card binds no principal, so issuance fails
    /// after it. The audit row records the authorization decision, not the
    /// operation's outcome: it is staged on the audit outbox before issuance
    /// runs, so the allowance survives while no key exists.
    ///
    /// # Panics
    /// Panics when the issue succeeds, the allowance is missing, or a key
    /// exists.
    #[tokio::test]
    async fn a_failed_issue_records_its_allowance_and_no_key() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = insert_test_user(&mut conn, tenant).await;
        conn.commit().await.expect("seed commits");

        let state = fixture_state(&fixture).await;
        let caller = caller_with(
            creator,
            tenant,
            PermissionSet::from_iter([Permission::service_accounts_write()]),
        );
        let request = IssueKeyRequest {
            card_ref: service_card_ref(),
            label: None,
            expires_in_seconds: None,
        };

        let error = issue_key(State(state.clone()), caller, None, Json(request))
            .await
            .expect_err("an unbound card cannot be issued a key");
        assert!(
            matches!(error.0, WyrdError::PrincipalNotFound { .. }),
            "expected the unbound-card refusal, got {:?}",
            error.0
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        assert_eq!(
            state.audit_outbox.settle(deadline).await,
            0,
            "audit settles"
        );
        let mut verify_conn = fixture.tenant_conn().await.expect("verify conn opens");
        let staged: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_staging
              WHERE data_tenant_id = $1
                AND operation = 'auth.api_key.issue'
                AND outcome = 'allowed'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&mut **verify_conn.transaction())
        .await
        .expect("allowance count reads");
        assert_eq!(
            staged, 1,
            "the failed issue records the decision it evaluated"
        );

        let keys: i64 =
            sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_api_keys WHERE data_tenant_id = $1")
                .bind(tenant.as_uuid())
                .fetch_one(&mut **verify_conn.transaction())
                .await
                .expect("key count reads");
        assert_eq!(keys, 0, "a failed issue leaves no key");
    }
}
