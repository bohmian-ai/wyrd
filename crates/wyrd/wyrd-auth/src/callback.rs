//! Domain logic for the common human OIDC callback.
//!
//! The callback carries the provider's `code` or `error`, its `state`, and
//! optional RFC 9207 `iss`. The state's SHA-256 names one login-state row across tenants; that
//! row alone decides the tenant, connection revision, issuer, client,
//! redirect, PKCE verifier, nonce, and initiation binding. No request header
//! takes part. A present `iss` must name that recorded issuer.
//!
//! A verified login mints no token. A login that answers an OAuth
//! authorization request gets a short-lived, single-use authorization code
//! bound to its client, redirect URI, PKCE challenge, tenant, and principal
//! (RFC 6749 §4.1.2), which [`AuthorizationCodeExchange::redeem_code`]
//! exchanges for the session at the token endpoint (RFC 6749 §4.1.3; RFC
//! 7636 §4.6). A device login records only the approval on its device
//! authorization; the CLI's token poll mints the session.
//!
//! A candidate connection test's state takes the same path against its bound
//! candidate instead of the Active connection: the code is redeemed and the ID
//! token verified exactly as a login, and then only that candidate revision is
//! marked tested. A test issues no session, credential, or User.

use std::time::Duration;

use base64::Engine as _;
use rand::RngCore as _;
use secrecy::{ExposeSecret as _, SecretString};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;
use wyrd_auth_oidc::{CodeRedemption, MappedClaims, TrustedIssuer};
use wyrd_runtime::audit::AuditStage;
use wyrd_runtime::{PrincipalId, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::auth::{
    ClientAuthorization, IssuerUrl, LoginInitiation, OAuthClientId, ProviderResponse, SecretBearer,
    Sha256Hex, TokenResponse,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};
use wyrd_sql::queries::auth::{
    LoginState, approve_device_authorization, consume_login_state, delete_user,
    human_connection_is_active, insert_user, issue_authorization_code, lock_human_connection_slot,
    lock_refresh_family, redeem_authorization_code, replace_idp_user_roles, upsert_user_identity,
    user_id_by_identity,
};
use wyrd_sql::row_types::auth::{HumanConnectionBinding, HumanSessionBinding};
use wyrd_sql::{SqlError, TenantConn};

use crate::audit::{
    LOGIN_OPERATION, TOKEN_EXCHANGE_OPERATION, USER_ROLES_SYNC_OPERATION, auth_event,
    auth_failure_code, principal_event,
};
use crate::connections::HumanConnections;
use crate::error::{relying_party_error, store_error};
use crate::exchange_api_key::role_refs;
use crate::issuance::TenantTokenIssuer;

/// Lifetime of an authorization code: the client must redeem it within this
/// window (RFC 6749 §4.1.2 recommends at most ten minutes).
const AUTHORIZATION_CODE_TTL: Duration = Duration::from_mins(1);

/// What a completed provider callback recorded, which decides the callback's
/// response.
#[derive(Debug)]
pub enum LoginCompletion {
    /// An authorization-request login succeeded: redirect the browser to the
    /// client's redirect URI with `code` and the client's `state` (RFC 6749
    /// §4.1.2).
    Authorized {
        /// The client request the login answers.
        authorization: ClientAuthorization,
        /// The single-use authorization code; only its hash is stored.
        code: SecretString,
    },
    /// An authorization-request login was refused after its state was
    /// consumed: redirect the browser to the client's redirect URI with an
    /// RFC 6749 §4.1.2.1 error. The refusal has already been audited.
    Refused {
        /// The client request the login answers.
        authorization: ClientAuthorization,
        /// Why the login was refused; it names the RFC error and the log line.
        error: WyrdError,
    },
    /// A device login was approved; the CLI's next token poll mints the
    /// session.
    DeviceApproved,
    /// A candidate connection test sign-in succeeded and marked the
    /// candidate tested.
    ConnectionTested,
}

/// Human OIDC authorization-code exchange service behind the common callback.
#[derive(Clone)]
pub struct AuthorizationCodeExchange {
    /// The shared tenant issuance workflow.
    pub issuer: TenantTokenIssuer,
    /// The tenant human-connection owner; the only source of human trust, of
    /// the completion keyring, and of the relying party every provider
    /// request is made through.
    pub connections: HumanConnections,
}

impl std::fmt::Debug for AuthorizationCodeExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationCodeExchange")
            .finish_non_exhaustive()
    }
}

impl AuthorizationCodeExchange {
    /// Complete a login from the provider callback's `response` (its code or
    /// error), `state`, and optional RFC 9207 `iss`.
    ///
    /// The SHA-256 of `state_key` resolves the login's tenant through the
    /// narrow definer lookup; an unknown, expired, or already consumed state
    /// names no tenant and is refused without any tenant audit. Within the
    /// tenant the state row is consumed and committed before any provider IO,
    /// so a replayed state never reaches the provider. The bound connection
    /// revision must still be the tenant's Active connection with the recorded
    /// issuer and client; the provider's cached discovery (refreshed once if
    /// the ID token names an unknown key) decides whether the response must
    /// carry `iss`, and [`verify_response_issuer`] binds it to
    /// the recorded issuer before any token-endpoint request; the relying
    /// party exchanges the code with the recorded redirect URI and PKCE
    /// verifier and verifies the ID token, its nonce, and its authorized
    /// party; and [`Self::finish_id_token_exchange`] records the outcome. A
    /// provider error consumes the state the same way and, once a present
    /// `iss` names the recorded issuer, refuses the login without any
    /// provider request ([`provider_refusal`]).
    /// Returns what the callback recorded, which decides its response: an
    /// authorization-request login is redirected back to its client with a
    /// code, or with an error when it is refused after its state was
    /// consumed; a device login and a candidate connection test get a static
    /// page.
    ///
    /// A connection test's state is bound to a candidate revision, not the
    /// Active connection: the provider is discovered from the recorded issuer
    /// first, and that exact candidate — with the freshly discovered JWKS URI
    /// it has not stored yet — is the trust the code and ID token are checked
    /// against ([`HumanConnections::tested_candidate`]).
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] when the state is unknown, expired,
    /// or replayed; [`WyrdError::InvalidToken`] when the bound connection is no
    /// longer Active (or, for a test, the bound candidate changed), the
    /// response issuer is mismatched or required and missing, the provider
    /// refuses the code, or the ID token fails verification;
    /// [`WyrdError::InvalidNonce`] on a missing or mismatched nonce;
    /// [`WyrdError::DiscoveryUnavailable`] or
    /// [`WyrdError::AuthVerifyUnavailable`] when the provider or store is
    /// unavailable; and the errors of [`Self::finish_id_token_exchange`].
    /// Every refusal after the tenant is known is staged on the audit stage before
    /// it is returned, and an authorization-request login's refusal after
    /// its state was consumed is returned as [`LoginCompletion::Refused`]
    /// instead. A failure after the consume commit leaves the state spent:
    /// the person starts a new login.
    pub async fn execute(
        &self,
        response: ProviderResponse,
        state_key: &str,
        response_issuer: Option<&str>,
        request_id: &str,
    ) -> Result<LoginCompletion, WyrdError> {
        let postgres = self.connections.postgres();
        let state_hash = Sha256Hex::digest(state_key.as_bytes());
        let Some(tenant_id) = postgres
            .login_state_tenant(&state_hash)
            .await
            .map_err(store_error)?
        else {
            tracing::info!("login callback state names no pending login");
            return Err(invalid_state(
                "login state is missing, expired, or already consumed",
            ));
        };
        let result = self
            .complete(
                tenant_id,
                &state_hash,
                response,
                response_issuer,
                request_id,
            )
            .await;
        // A refusal rolls back any user it resolved, so the denied event
        // names no principal.
        match &result {
            Err(error) | Ok(LoginCompletion::Refused { error, .. }) => {
                audit_authorization_code_failure(self.issuer.audit(), tenant_id, request_id, error);
            }
            Ok(_) => {}
        }
        result
    }

    /// Consume the state, then verify and finish the login for
    /// [`Self::execute`] once the tenant is known.
    ///
    /// # Errors
    /// Returns the errors [`Self::execute`] documents after tenant
    /// resolution, except an authorization-request login's refusal after its
    /// state was consumed, which becomes [`LoginCompletion::Refused`].
    async fn complete(
        &self,
        tenant_id: DataTenantId,
        state_hash: &Sha256Hex,
        response: ProviderResponse,
        response_issuer: Option<&str>,
        request_id: &str,
    ) -> Result<LoginCompletion, WyrdError> {
        let postgres = self.connections.postgres();
        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let login_state = consume_login_state(&mut conn, state_hash)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let login_state = login_state
            .ok_or_else(|| invalid_state("login state is missing, expired, or already consumed"))?;
        let result = match response {
            ProviderResponse::Code(code) => {
                self.verify_and_finish(
                    tenant_id,
                    state_hash,
                    &login_state,
                    code.into_secret_string(),
                    response_issuer,
                    request_id,
                )
                .await
            }
            ProviderResponse::Error(error) => {
                verify_response_issuer(response_issuer, &login_state.issuer, false)
                    .and_then(|()| Err(provider_refusal(&error)))
            }
        };
        match (result, login_state.initiation) {
            (Err(error), LoginInitiation::Authorize(authorization)) => {
                Ok(LoginCompletion::Refused {
                    authorization,
                    error,
                })
            }
            (result, _) => result,
        }
    }

    /// Exchange the provider code and verify the ID token for a consumed
    /// login, then finish it.
    ///
    /// # Errors
    /// Returns the errors [`Self::execute`] documents after the consume.
    async fn verify_and_finish(
        &self,
        tenant_id: DataTenantId,
        state_hash: &Sha256Hex,
        login_state: &LoginState,
        code: SecretString,
        response_issuer: Option<&str>,
        request_id: &str,
    ) -> Result<LoginCompletion, WyrdError> {
        let relying_party = self.connections.relying_party();
        let (trusted, provider) = if let LoginInitiation::ConnectionTest(_) = login_state.initiation
        {
            let issuer = IssuerUrl::new(login_state.issuer.as_str()).map_err(|error| {
                WyrdError::Internal {
                    message: format!("recorded login issuer does not decode: {error}"),
                    details: serde_json::json!({}),
                }
            })?;
            let provider = relying_party
                .discover(&issuer)
                .await
                .map_err(relying_party_error)?;
            let trusted = self
                .connections
                .tested_candidate(tenant_id, login_state, provider.jwks_uri().url())
                .await?
                .ok_or_else(|| {
                    invalid_token("the login connection changed while the login was in progress")
                })?;
            (trusted, provider)
        } else {
            let trusted = self.bound_connection(tenant_id, login_state).await?;
            let provider = relying_party
                .cached(&trusted.issuer)
                .await
                .map_err(relying_party_error)?;
            (trusted, provider)
        };
        verify_response_issuer(
            response_issuer,
            &login_state.issuer,
            provider
                .additional_metadata()
                .authorization_response_iss_parameter_supported,
        )?;
        let redemption = CodeRedemption {
            client_id: &trusted.client_id,
            client_auth: &trusted.client_auth,
            claim_mapping: &trusted.claim_mapping,
            redirect_uri: &login_state.redirect_uri,
            code_verifier: &login_state.code_verifier,
            nonce: &login_state.nonce,
        };
        let verified = relying_party
            .redeem(&trusted.issuer, provider, code, &redemption)
            .await
            .map_err(relying_party_error)?;
        self.finish_id_token_exchange(
            state_hash,
            &trusted,
            login_state,
            &verified.identity,
            request_id,
        )
        .await
    }

    /// Finish a consumed login in `trusted.tenant_id` once the relying party
    /// has verified the provider's ID token and mapped it to `identity`.
    ///
    /// The tenant's Active connection is re-read and must still be the exact
    /// revision, issuer, and client the login bound. One tenant transaction
    /// then resolves the user by (issuer, `sub`) only and takes the User's
    /// tenant-qualified refresh-family lock, held through commit, so
    /// concurrent callbacks for one User serialize and the final durable roles
    /// equal one callback's mapping. It then takes the tenant's connection
    /// slot lock — after the family lock, as every refresh path orders them —
    /// and requires the bound revision to still be Active, so a lifecycle
    /// mutation that won the lock leaves no User, role, code, or approval
    /// behind, and one that waits commits only after this login. It then
    /// replaces the user's roles with
    /// those the verified groups map to (unmapped groups and unknown role
    /// names grant nothing; connection default roles are never applied to
    /// human login) and, when that changed the user's durable roles, stages
    /// one allowed `auth.user.roles.sync` audit event for the User after the
    /// commit. Finally it
    /// records the outcome and commits:
    ///
    /// - an authorization-request login attaches a fresh authorization code
    ///   `{tenant}.{256-bit secret}` — stored only as its SHA-256, with a
    ///   one-minute `PostgreSQL`-derived expiry — and the principal to its
    ///   consumed state row, and returns the code once;
    /// - a device login records the principal and connection as the approval
    ///   of its device authorization, which must still be pending.
    ///
    /// Either outcome stages one allowed `auth.login` event for the User on
    /// the process audit stage once the login, its roles, and its code or
    /// approval have committed together; the audit never fails the login.
    ///
    /// No token is minted here; the token endpoint mints the session when the
    /// code or device code is redeemed.
    ///
    /// A connection test stops before any of that: instead of the Active
    /// connection re-check and any user work,
    /// [`HumanConnections::stamp_test_sign_in`] re-checks the tester's
    /// authority and marks only the bound candidate revision tested with
    /// `trusted`'s JWKS URI. The consumed test state is left for expiry
    /// purge.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the bound connection is no
    /// longer Active, [`WyrdError::InvalidState`] when the state row no longer
    /// awaits a code or the device authorization was denied, expired, or
    /// deleted meanwhile, the errors of
    /// [`HumanConnections::stamp_test_sign_in`] for a connection test, and the
    /// store errors; nothing commits unless every step succeeds.
    pub async fn finish_id_token_exchange(
        &self,
        state_hash: &Sha256Hex,
        trusted: &TrustedIssuer,
        login_state: &LoginState,
        identity: &MappedClaims,
        request_id: &str,
    ) -> Result<LoginCompletion, WyrdError> {
        let tenant_id = trusted.tenant_id;
        if let LoginInitiation::ConnectionTest(tester) = login_state.initiation {
            self.connections
                .stamp_test_sign_in(
                    tenant_id,
                    login_state.connection,
                    tester,
                    &trusted.jwks_uri,
                    request_id,
                )
                .await?;
            return Ok(LoginCompletion::ConnectionTested);
        }
        self.bound_connection(tenant_id, login_state).await?;

        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant_id)
            .await
            .map_err(store_error)?;
        let principal_id = ensure_user_identity(
            &mut conn,
            trusted,
            &identity.subject,
            identity.email.as_deref(),
        )
        .await
        .map_err(store_error)?;
        // Concurrent callbacks for this User would otherwise each replace
        // roles unseen by the other and the later one mint their union; the
        // family lock held to commit makes one callback's set the whole truth.
        lock_refresh_family(&mut conn, "user", principal_id)
            .await
            .map_err(store_error)?;
        fence_bound_connection(&mut conn, login_state.connection).await?;
        let roles = role_names_to_refs(trusted, &identity.groups)?;
        // The provider just asserted this human's `idp` authority. Recording
        // it beside any `direct` grants is what makes the grant table the
        // truth every later token is minted from.
        let role_names = roles.iter().map(RoleRef::as_str).collect::<Vec<_>>();
        let roles_changed = replace_idp_user_roles(&mut conn, principal_id, &role_names)
            .await
            .map_err(store_error)?;
        let completion = match &login_state.initiation {
            LoginInitiation::Authorize(authorization) => {
                let code = SecretString::from(format!("{tenant_id}.{}", new_code_secret()));
                if !issue_authorization_code(
                    &mut conn,
                    state_hash,
                    &Sha256Hex::digest(code.expose_secret().as_bytes()),
                    principal_id,
                    AUTHORIZATION_CODE_TTL,
                )
                .await
                .map_err(store_error)?
                {
                    return Err(invalid_state(
                        "login state is missing, expired, or already consumed",
                    ));
                }
                LoginCompletion::Authorized {
                    authorization: authorization.clone(),
                    code,
                }
            }
            LoginInitiation::Device(device_id) => {
                if !approve_device_authorization(
                    &mut conn,
                    *device_id,
                    principal_id,
                    login_state.connection,
                )
                .await
                .map_err(store_error)?
                {
                    return Err(invalid_state(
                        "the device code was denied or expired; start a new login",
                    ));
                }
                LoginCompletion::DeviceApproved
            }
            // Returned above; a test state never carries a code or approval.
            LoginInitiation::ConnectionTest(_) => {
                return Err(invalid_state(
                    "login state is missing, expired, or already consumed",
                ));
            }
        };
        conn.commit().await.map_err(store_error)?;
        let audit = self.issuer.audit();
        if roles_changed {
            audit.stage(tenant_id, roles_sync_event(request_id, principal_id));
        }
        audit.stage(tenant_id, login_event(request_id, principal_id));
        Ok(completion)
    }

    /// Redeem an authorization code at the token endpoint for the
    /// authenticated `client` (RFC 6749 §4.1.3–4.1.4; RFC 7636 §4.6).
    ///
    /// The code's tenant prefix only routes the request; the code hash under
    /// tenant RLS is the authority. One tenant transaction deletes the row
    /// holding the code — the delete is its single use — and requires it to
    /// be unexpired, issued to `client`, issued for exactly `redirect_uri`,
    /// and that `BASE64URL(SHA-256(code_verifier))` equals its S256
    /// challenge. A refused code stays deleted. A live code then mints the
    /// session through [`TenantTokenIssuer::issue_human_session`], bound to
    /// the login's connection and `client`, with its canonical token-exchange
    /// audit, and commits.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] for a malformed, unknown, expired,
    /// already redeemed, or mismatched code or verifier;
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails; and the
    /// issuance errors, including a connection that is no longer Active or a
    /// suspended User. Every refusal for a routed tenant is staged on
    /// the audit stage.
    pub async fn redeem_code(
        &self,
        code: &SecretBearer,
        client: OAuthClientId,
        redirect_uri: &str,
        code_verifier: &SecretBearer,
        request_id: &str,
    ) -> Result<TokenResponse, WyrdError> {
        let tenant_id = code
            .expose()
            .split_once('.')
            .and_then(|(tenant, _)| tenant.parse::<DataTenantId>().ok())
            .ok_or_else(|| invalid_token("the authorization code is not valid"))?;
        let result = self
            .redeem_code_in(
                tenant_id,
                code,
                client,
                redirect_uri,
                code_verifier,
                request_id,
            )
            .await;
        if let Err(error) = &result {
            audit_authorization_code_failure(self.issuer.audit(), tenant_id, request_id, error);
        }
        result
    }

    /// The tenant transaction of [`Self::redeem_code`] once the tenant is
    /// routed.
    ///
    /// # Errors
    /// Returns the errors [`Self::redeem_code`] documents after routing.
    async fn redeem_code_in(
        &self,
        tenant_id: DataTenantId,
        code: &SecretBearer,
        client: OAuthClientId,
        redirect_uri: &str,
        code_verifier: &SecretBearer,
        request_id: &str,
    ) -> Result<TokenResponse, WyrdError> {
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant_id)
            .await
            .map_err(store_error)?;
        let redeemed =
            redeem_authorization_code(&mut conn, &Sha256Hex::digest(code.expose().as_bytes()))
                .await
                .map_err(store_error)?
                .ok_or_else(|| invalid_token("the authorization code is not valid"))?;
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(code_verifier.expose().as_bytes()));
        if !redeemed.live
            || redeemed.client != client
            || redeemed.redirect_uri != redirect_uri
            || redeemed.code_challenge != challenge
        {
            // The code is spent even when refused.
            conn.commit().await.map_err(store_error)?;
            return Err(invalid_token(
                "the authorization code is expired or does not match this client, redirect \
                 URI, or code verifier",
            ));
        }
        let session = self
            .issuer
            .issue_human_session(
                &mut conn,
                redeemed.principal_id,
                None,
                HumanSessionBinding {
                    connection: redeemed.connection,
                    client,
                },
                request_id,
            )
            .await?;
        conn.commit().await.map_err(store_error)?;
        Ok(session.into_response())
    }

    /// Require the tenant's Active connection to be exactly the one the login
    /// bound — same connection id and revision, issuer, and client id — and
    /// return its trust.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the tenant has no Active
    /// connection or it differs from the login's binding, and the store
    /// errors of [`HumanConnections::active_connection`].
    async fn bound_connection(
        &self,
        tenant_id: DataTenantId,
        login_state: &LoginState,
    ) -> Result<TrustedIssuer, WyrdError> {
        let active = self.connections.active_connection(tenant_id).await?;
        match active {
            Some(active)
                if active.binding == login_state.connection
                    && active.trusted.issuer.as_str() == login_state.issuer
                    && active.trusted.client_id == login_state.client_id =>
            {
                Ok(active.trusted)
            }
            _ => Err(invalid_token(
                "the login connection changed while the login was in progress",
            )),
        }
    }
}

/// Take the tenant's connection slot lock on `conn` and require the login's
/// bound connection revision to still be Active.
///
/// Every connection lifecycle mutation takes the slot lock, so with it held
/// until `conn` ends a replacement, deactivation, or removal either committed
/// before this check — and the login is refused — or waits until the login
/// commits or rolls back. Callers take the User's refresh-family lock first.
///
/// # Errors
/// Returns [`WyrdError::InvalidToken`] when the bound revision is no longer
/// Active, and [`WyrdError::AuthVerifyUnavailable`] when a statement fails.
async fn fence_bound_connection(
    conn: &mut TenantConn<'_>,
    binding: HumanConnectionBinding,
) -> Result<(), WyrdError> {
    lock_human_connection_slot(conn)
        .await
        .map_err(store_error)?;
    let active =
        human_connection_is_active(conn, binding.connection_id, binding.connection_revision)
            .await
            .map_err(store_error)?;
    if active {
        Ok(())
    } else {
        Err(invalid_token(
            "the login connection changed while the login was in progress",
        ))
    }
}

/// Resolve the local user for a trusted external identity or create it once.
///
/// Keyed by (issuer, subject) only: the same email under another issuer is a
/// different user with no inherited grants.
///
/// # Errors
/// Returns a [`SqlError`] when a lookup, insert, or identity upsert fails.
pub async fn ensure_user_identity(
    conn: &mut TenantConn<'_>,
    trusted: &TrustedIssuer,
    subject: &str,
    email: Option<&str>,
) -> Result<Uuid, SqlError> {
    let issuer = trusted.issuer.as_str();
    if let Some(user_id) = user_id_by_identity(conn, issuer, subject).await? {
        return Ok(user_id);
    }

    let user_id = Uuid::new_v4();
    insert_user(conn, user_id, email, "oidc", None).await?;
    let canonical = upsert_user_identity(conn, issuer, subject, user_id).await?;
    if canonical != user_id {
        let _ = delete_user(conn, user_id).await?;
    }
    Ok(canonical)
}

/// Stage the audit of a refused human authorization-code exchange.
///
/// Stages one denied `auth.token.exchange` event carrying the closed failure
/// code on the process audit stage. A refusal rolls back any user it resolved, so
/// the event names the nil principal. Staging never waits and never fails, so
/// the caller's original error still reaches the client.
pub fn audit_authorization_code_failure(
    audit: &dyn AuditStage,
    tenant_id: DataTenantId,
    request_id: &str,
    error: &WyrdError,
) {
    let event = auth_event(
        request_id,
        TOKEN_EXCHANGE_OPERATION,
        PrincipalId::new(Uuid::nil()),
        PrincipalKindTag::User,
        None,
        AuditOutcome::Denied,
        AuditDetail::AuthFailure {
            error_code: auth_failure_code(error),
        },
    );
    audit.stage(tenant_id, event);
}

/// Bind an authorization response to the issuer its login state recorded
/// (RFC 9207), before the code is sent to any token endpoint.
///
/// A present `response_issuer` must equal `expected` by exact string
/// comparison. An absent one is refused only when the provider's discovery
/// advertised `authorization_response_iss_parameter_supported`; otherwise the
/// login proceeds on server-bound state, PKCE, and ID-token issuer
/// validation, which leaves the residual mix-up exposure documented in the
/// security posture.
///
/// # Errors
/// Returns [`WyrdError::InvalidToken`] when the response issuer differs from
/// `expected`, or is missing while `advertised` is `true`.
pub fn verify_response_issuer(
    response_issuer: Option<&str>,
    expected: &str,
    advertised: bool,
) -> Result<(), WyrdError> {
    match response_issuer {
        Some(issuer) if issuer == expected => Ok(()),
        Some(_) => Err(invalid_token(
            "authorization response issuer does not match the login's issuer",
        )),
        None if advertised => Err(invalid_token(
            "authorization response is missing the issuer the provider advertises",
        )),
        None => Ok(()),
    }
}

/// The refusal a provider's RFC 6749 §4.1.2.1 `error` resolves the login to.
///
/// `temporarily_unavailable` and `server_error` keep their meaning, so the
/// client is redirected back with the same code; every other provider error
/// is a refused sign-in, which the client sees as `access_denied`. Neither
/// the provider's code nor its `error_description` is copied into the
/// refusal, so none of its text is reflected or logged.
fn provider_refusal(error: &str) -> WyrdError {
    let message = "the identity provider refused the sign-in".to_owned();
    let details = serde_json::json!({});
    match error {
        "temporarily_unavailable" => WyrdError::DiscoveryUnavailable { message, details },
        "server_error" => WyrdError::Internal { message, details },
        _ => WyrdError::InvalidToken { message, details },
    }
}

/// The allowed `auth.login` event recording that a provider callback signed
/// the User in and committed its authorization code or device approval.
///
/// The resource is the User principal and the event carries no detail: the
/// provider's claims stay out of audit history.
fn login_event(request_id: &str, principal_id: Uuid) -> AuditEvent {
    principal_event(
        request_id,
        LOGIN_OPERATION,
        PrincipalId::new(principal_id),
        PrincipalKindTag::User,
        None,
        AuditOutcome::Allowed,
    )
}

/// The allowed `auth.user.roles.sync` event recording that a login's
/// provider-asserted groups changed a User's durable roles.
///
/// The resource is the User principal. The event carries no detail: role
/// names are tenant configuration, and the fact of the mutation is what the
/// audit needs to distinguish it from an unchanged login.
fn roles_sync_event(request_id: &str, principal_id: Uuid) -> AuditEvent {
    principal_event(
        request_id,
        USER_ROLES_SYNC_OPERATION,
        PrincipalId::new(principal_id),
        PrincipalKindTag::User,
        None,
        AuditOutcome::Allowed,
    )
}

/// Map the verified groups of a human login to local Wyrd role refs.
///
/// Only groups present in the connection's group-to-role map grant anything;
/// connection `default_roles` are never applied to a human login, so a person
/// in no mapped group has no authority. A mapped role name that names no
/// tenant role is dropped when roles are recorded.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when a mapped role name is not a valid
/// role reference.
pub fn role_names_to_refs(
    trusted: &TrustedIssuer,
    groups: &[String],
) -> Result<Vec<RoleRef>, WyrdError> {
    let mut names = Vec::new();
    for group in groups {
        if let Some(mapped) = trusted.group_role_map.get(group) {
            names.extend(mapped.iter().cloned());
        }
    }
    names.sort_unstable();
    names.dedup();
    role_refs(names).map_err(|_| WyrdError::Internal {
        message: "trusted issuer role mapping is invalid".to_owned(),
        details: serde_json::json!({}),
    })
}

/// A fresh unguessable authorization-code secret: 32 random bytes,
/// base64url without padding (RFC 6749 §10.10).
fn new_code_secret() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn invalid_state(message: &str) -> WyrdError {
    WyrdError::InvalidState {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

/// The RFC 9207 response-issuer decision in isolation.
#[cfg(test)]
mod response_issuer_tests {
    use super::verify_response_issuer;

    /// The issuer a login recorded.
    const ISSUER: &str = "https://idp.example.com/realms/acme";

    /// A matching `iss` passes whether or not support is advertised; a
    /// mismatched one (including a trailing-slash variant) is refused either
    /// way; a missing one is refused only when the provider advertises support.
    #[test]
    fn response_issuer_is_bound_exactly_and_required_only_when_advertised() {
        for advertised in [false, true] {
            verify_response_issuer(Some(ISSUER), ISSUER, advertised).expect("matching iss passes");
            for wrong in [
                "https://evil.example.com",
                "https://idp.example.com/realms/acme/",
            ] {
                let error = verify_response_issuer(Some(wrong), ISSUER, advertised)
                    .expect_err("mismatched iss is refused");
                assert_eq!(error.code(), "WYRD_AUTH_401_INVALID_TOKEN");
            }
        }
        let error =
            verify_response_issuer(None, ISSUER, true).expect_err("required iss is refused");
        assert_eq!(error.code(), "WYRD_AUTH_401_INVALID_TOKEN");
        verify_response_issuer(None, ISSUER, false).expect("unadvertised iss may be absent");
    }
}
