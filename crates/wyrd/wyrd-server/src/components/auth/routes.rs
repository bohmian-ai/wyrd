//! HTTP routes for the tenant auth surfaces: the OAuth authorization
//! server (token, device authorization, revocation, and metadata
//! endpoints), browser sign-in sessions, and the OIDC provider callback.

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};

use base64::Engine;
use secrecy::{ExposeSecret, SecretString};
use wyrd_auth::callback::LoginCompletion;
use wyrd_auth_verify::{AccessTokenClaims, VerifiedToken};
use wyrd_spec::auth::{
    CallbackQuery, ExchangeTokenType, OAuthClientId, OAuthErrorCode, OAuthErrorResponse,
    SecretBearer, TokenAudience, TokenRequest, TokenResponse,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::CardUid;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::TenantConn;

use crate::auth::RequestOrigin;
use crate::auth::browser::{login_binding, login_refused, signed_in};
use crate::auth::callback::exchange_authorization_code;
use crate::auth::card_scope::{
    MINT_KIND_API_KEY_EXCHANGE, MINT_KIND_REFRESH, stage_scope_mint_failure_audit,
};
use crate::auth::exchange_api_key::{
    DelegateToken, ExchangeApiKey, api_key_invalid, map_exchange_error_to_wyrd,
};
use crate::auth::issue_api_key::WyrdApiKey;
use crate::auth::jwt_bearer::exchange_jwt_bearer;
use crate::auth::oauth::{ClientForm, GRANT_TYPES, OAuthError, OAuthForm, no_store};
use crate::auth::refresh::{RefreshError, RefreshTokens, tenant_from_refresh_jwt};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build the tenant-plane auth router.
///
/// Mounts the auth surfaces — browser sign-in
/// (`POST /auth/browser/login`), the CLI device authorization
/// (`POST /auth/device_authorization`) and its verification page
/// (`GET`/`POST /auth/device`), the common OIDC provider callback
/// (`GET /auth/callback`), the token endpoint (`POST /auth/token`), token
/// revocation (`POST /auth/revoke`), authorization server metadata, and the
/// browser-session operations under `/auth/browser`. Device user-code
/// attempts are limited per client address at the gateway (RFC 8628 §5.1),
/// which sees the real client and one budget across replicas; the token endpoint enforces
/// the device polling interval with `slow_down`.
pub fn auth_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(crate::auth::browser::login))
        .routes(routes!(crate::auth::metadata::metadata))
        .routes(routes!(crate::auth::cli_login::device_authorization))
        .routes(routes!(
            crate::auth::cli_login::device_page,
            crate::auth::cli_login::device_decision
        ))
        .routes(routes!(crate::auth::cli_login::revoke))
        .routes(routes!(crate::auth::browser::api_key))
        .routes(routes!(crate::auth::browser::session))
        .routes(routes!(crate::auth::browser::logout))
        .routes(routes!(callback))
        .routes(routes!(token))
}

/// `POST /auth/token` — the OAuth token endpoint.
///
/// Every tenant grant arrives here as form parameters and leaves with the
/// same RFC 6749 §5.1 [`TokenResponse`]: a refresh token (§6), an RFC 8628 device code
/// (`wyrd-cli`), an RFC 8693 token exchange — a Wyrd API key as subject, or
/// a delegation — and an RFC 7523 workload assertion. The human grants
/// require their client, which names itself with `client_id`. Tenant and principal are derived
/// from the verified credential, never from a client-supplied header.
///
/// # Errors
/// Answers the RFC 6749 §5.2 body: `invalid_request` for a malformed
/// request — and, per RFC 8693 §2.2.2, an unusable subject or actor token —
/// `invalid_target` for a token exchange naming an unsupported `audience`,
/// `invalid_client` (`401`) for a missing or refused client,
/// `unauthorized_client` for a grant another client must use,
/// `unsupported_grant_type`, `invalid_grant` for every unusable refresh
/// token, device code, or assertion, the RFC 8628 §3.5 device codes,
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
    // No session exists yet at this operation: a client names itself with
    // `client_id` and needs nothing.
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    request_id: Option<Extension<RequestId>>,
    form: OAuthForm,
) -> Result<Response, OAuthError> {
    let client = crate::auth::oauth::identify_client(&headers, &form)?;
    match form.get("grant_type") {
        None => return Err(OAuthError(OAuthErrorCode::InvalidRequest, None)),
        Some(grant) if !GRANT_TYPES.contains(&grant) => {
            return Err(OAuthError(OAuthErrorCode::UnsupportedGrantType, None));
        }
        Some(_) => {}
    }
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    let grants = TokenGrants {
        state: &state,
        request_id: request_id.as_str(),
    };
    let required = client.ok_or(OAuthError(OAuthErrorCode::InvalidClient, None));
    let response = match form.token_request()? {
        TokenRequest::RefreshToken { refresh_token } => grants
            .refresh(&refresh_token, required?)
            .await
            .map_err(OAuthError::from)?,
        TokenRequest::DeviceCode { device_code } => {
            if required? != OAuthClientId::WyrdCli {
                return Err(OAuthError(OAuthErrorCode::UnauthorizedClient, None));
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
            workload_uid,
        } => grants
            .api_key(&subject_token, workload_uid)
            .await
            .map_err(OAuthError::request)?,
        TokenRequest::TokenExchange {
            subject_token,
            subject_token_type: ExchangeTokenType::AccessToken,
            actor_token: Some(actor_token),
            actor_token_type: Some(ExchangeTokenType::AccessToken),
            audience: Some(audience),
            workload_uid: None,
        } => grants
            .delegate(&subject_token, &actor_token, audience)
            .await
            .map_err(OAuthError::request)?,
        TokenRequest::TokenExchange { .. } => {
            return Err(OAuthError(OAuthErrorCode::InvalidRequest, None));
        }
        TokenRequest::JwtBearer {
            assertion,
            tenant,
            workload_uid,
        } => exchange_jwt_bearer(
            &state,
            &headers,
            assertion.into_secret_string(),
            tenant,
            workload_uid,
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
/// API-key verification, and gateway ingress exchanges a presented API key
/// through the same API-key grant, so it is crate-visible.
pub(crate) struct TokenGrants<'a> {
    /// Server state holding the auth owners and the runtime store.
    pub(crate) state: &'a AppState,
    /// The request id every audit event of the grant carries.
    pub(crate) request_id: &'a str,
}

impl<'a> TokenGrants<'a> {
    /// Exchange a Wyrd API key for an access token of its own principal,
    /// under the workload root `workload_uid` selects, if any.
    ///
    /// A key that does not parse names no tenant, so it is refused with the
    /// same error every invalid key earns.
    ///
    /// # Errors
    /// Returns [`WyrdError::ApiKeyInvalid`] for every unusable key, its
    /// refusal staged on the audit outbox when it names a tenant, and a store
    /// or issuance error.
    pub(crate) async fn api_key(
        &self,
        api_key: &SecretBearer,
        workload_uid: Option<CardUid>,
    ) -> Result<TokenResponse, WyrdError> {
        let presented = SecretString::from(api_key.expose().to_owned());
        let (parsed, mut conn, exchange) = self.api_key_exchange(api_key.expose()).await?;
        let exchanged = match exchange
            .execute(&mut conn, presented, workload_uid, self.request_id)
            .await
        {
            Ok(exchanged) => exchanged,
            Err(error) => {
                let wyrd = map_exchange_error_to_wyrd(&mut conn, &parsed.prefix, error).await;
                stage_scope_mint_failure_audit(
                    &**self.state.bifrost.scribe_outbox(),
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
    /// would carry under the root `workload_uid` selects, if any. Nothing is staged on the audit outbox: like a bearer
    /// request, the request's own authorization decisions are the audit.
    ///
    /// # Errors
    /// Returns [`WyrdError::ApiKeyInvalid`] for every unusable key, and a
    /// store or issuance error.
    pub(crate) async fn api_key_principal(
        &self,
        api_key: &SecretString,
        workload_uid: Option<CardUid>,
    ) -> Result<VerifiedToken, WyrdError> {
        let (parsed, mut conn, exchange) = self.api_key_exchange(api_key.expose_secret()).await?;
        let verified = match exchange
            .authenticate(&mut conn, api_key, workload_uid)
            .await
        {
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
                    &**self.state.bifrost.scribe_outbox(),
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
            .tenant_issuer(self.state.bifrost.scribe_outbox())
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
/// selects the tenant and connection. The RFC 9207 `iss` parameter, when
/// sent, is forwarded so the exchange can bind the response to the login's
/// issuer before any token request. The response carries no token and not
/// the provider code. A browser sign-in completes only for the browser
/// presenting its `wyrd_login` login-transaction cookie: on success the
/// tenant's session cookie is set and the browser redirected (`303`) to
/// `/t/{tenant}`; once its state was consumed, any refusal — including a
/// provider's own `error` or a missing binding — redirects (`303`) to
/// `/t/{tenant}/login?login=failed`. Both clear the login cookie. A
/// device-code login records its approval and receives a static page; a
/// candidate connection test marks its bound candidate revision tested,
/// issues nothing, and receives a static page.
///
/// # Errors
/// Returns problem JSON for every refusal of
/// [`exchange_authorization_code`] that cannot be returned to a browser
/// sign-in page.
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
        (status = 303, description = "A browser sign-in completed: redirect to `/t/{tenant}` \
          with the tenant session cookie set, or, when refused, to \
          `/t/{tenant}/login?login=failed`; the `wyrd_login` cookie is cleared",
          headers(
            ("Location" = String, description = "The tenant home or sign-in page"),
            ("Set-Cookie" = String, description = "The tenant session cookie and the cleared \
              login-transaction cookie")
          )),
        (status = 400, description = "The query carries both or neither of `code` and \
          `error` (WYRD_SPEC_400_VALIDATION), the login state is unknown, expired, or replayed \
          (WYRD_AUTH_400_INVALID_STATE), the ID token nonce does not match \
          (WYRD_AUTH_400_INVALID_NONCE)", body = WyrdProblem),
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
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    request_id: Option<Extension<RequestId>>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, WyrdErrorResponse> {
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    let binding = login_binding(&headers);
    let completed = exchange_authorization_code(
        &state,
        query.response()?,
        &query.state,
        query.iss.as_deref(),
        binding.as_deref(),
        request_id.as_str(),
    )
    .await?;
    match completed {
        LoginCompletion::SignedIn(created) => signed_in(&state, &origin, &created).await,
        LoginCompletion::Refused { tenant_id, error } => {
            login_refused(&state, &origin, tenant_id, error).await
        }
        LoginCompletion::DeviceApproved => Ok(Html(CLI_LOGIN_COMPLETE_PAGE).into_response()),
        LoginCompletion::ConnectionTested => {
            Ok(Html(CONNECTION_TEST_COMPLETE_PAGE).into_response())
        }
    }
}

/// The refusal of a grant the server lacks the auth configuration for.
fn not_configured() -> WyrdError {
    crate::auth::auth_not_configured().0
}

/// The refusal of a grant whose tenant store is unavailable; the cause is
/// logged, never returned.
pub(crate) fn store_unavailable(error: wyrd_sql::SqlError) -> WyrdError {
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
mod tests {
    use base64::Engine;
    use wyrd_auth_verify::{AccessTokenClaims, TokenPrincipalRef};
    use wyrd_runtime::{PermissionSet, PrincipalId};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{CardAccess, PrincipalKindTag};

    use super::{auth_router, tenant_from_unverified_access_token};

    #[test]
    fn mounts_token_routes() {
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
            },
            roles: vec![],
            permissions: PermissionSet::default(),
            workload_uid: None,
            card_access: CardAccess::AnyRegistered,
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
}
