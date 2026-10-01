//! Domain logic for the common human OIDC callback.
//!
//! The callback carries only the provider's `code` and `state`. The state's
//! SHA-256 names one login-state row across tenants; that row alone decides
//! the tenant, connection revision, issuer, client, redirect, PKCE verifier,
//! nonce, and initiation binding. No request header takes part.

use std::str::FromStr;
use std::sync::Arc;

use jsonwebtoken::Algorithm;
use reqwest::RequestBuilder;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::Value;
use url::Url;
use uuid::Uuid;
use wyrd_auth_oidc::{
    ClientAuth, OidcError, OidcProvider, ScreenedHttp, TrustedIssuer, read_bounded_body,
};
use wyrd_auth_verify::ExternalVerifier;
use wyrd_runtime::{PrincipalId, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::auth::{IssuerUrl, LoginInitiation, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};
use wyrd_sql::queries::auth::{
    LoginState, complete_login_state, consume_login_state, delete_user, insert_user,
    lock_refresh_family, replace_user_roles, upsert_user_identity, user_id_by_identity,
};
use wyrd_sql::{SqlError, TenantConn, WyrdPostgres};

use crate::audit::{
    TOKEN_EXCHANGE_OPERATION, USER_ROLES_SYNC_OPERATION, append_auth_audit, auth_event,
    auth_failure_code, principal_event, record_auth_audit_best_effort,
};
use crate::connections::HumanConnections;
use crate::error::{auth_error_to_wyrd, provider_unreachable, screen_error, store_error};
use crate::exchange_api_key::role_refs;
use crate::issuance::TenantTokenIssuer;
use crate::login::{LOGIN_COMPLETION_TTL, seal_completion};
use crate::pg_resolvers::PgIssuerResolver;

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    id_token: String,
}

/// Human OIDC authorization-code exchange service behind the common callback.
#[derive(Clone)]
pub struct AuthorizationCodeExchange {
    /// The shared tenant issuance workflow.
    pub issuer: TenantTokenIssuer,
    /// External OIDC id-token verifier.
    pub verifier: Arc<ExternalVerifier<PgIssuerResolver>>,
    /// The tenant human-connection owner; the only source of human trust, of
    /// the completion keyring, and of the screened HTTP capability every
    /// provider request is made through.
    pub connections: HumanConnections,
}

impl std::fmt::Debug for AuthorizationCodeExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationCodeExchange")
            .finish_non_exhaustive()
    }
}

impl AuthorizationCodeExchange {
    /// Complete a login from the provider callback's `code` and `state`.
    ///
    /// The SHA-256 of `state_key` resolves the login's tenant through the
    /// narrow definer lookup; an unknown, expired, or already consumed state
    /// names no tenant and is refused without any tenant audit. Within the
    /// tenant the state row is consumed and committed before any provider IO,
    /// so a replayed state never reaches the provider. The bound connection
    /// revision must still be the tenant's Active connection with the recorded
    /// issuer and client; the code is exchanged with the recorded redirect URI
    /// and PKCE verifier; and [`Self::finish_id_token_exchange`] verifies the
    /// token and issues and seals the session. Returns how the login was
    /// initiated, which decides the callback's response: a browser login is
    /// redirected to the BFF completion route, a CLI login gets a static page.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] when the state is unknown, expired,
    /// or replayed; [`WyrdError::Validation`] when no sealing keyring is
    /// configured; [`WyrdError::InvalidToken`] when the bound connection is no
    /// longer Active or the provider refuses the code;
    /// [`WyrdError::DiscoveryUnavailable`] or
    /// [`WyrdError::AuthVerifyUnavailable`] when the provider or store is
    /// unavailable; and the errors of [`Self::finish_id_token_exchange`].
    /// Every refusal after the tenant is known is audited best-effort before
    /// it is returned. A failure after the consume commit leaves the state
    /// spent: the person starts a new login.
    pub async fn execute(
        &self,
        code: SecretString,
        state_key: &str,
        request_id: &str,
    ) -> Result<LoginInitiation, WyrdError> {
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
            .complete(tenant_id, &state_hash, code, request_id)
            .await;
        if let Err(error) = &result {
            // A refusal rolls back any user it resolved, so the denied event
            // names no principal.
            audit_authorization_code_failure(postgres, tenant_id, request_id, error).await;
        }
        result
    }

    /// Consume the state, exchange the code, and finish the login for
    /// [`Self::execute`] once the tenant is known.
    ///
    /// # Errors
    /// Returns the errors [`Self::execute`] documents after tenant resolution.
    async fn complete(
        &self,
        tenant_id: DataTenantId,
        state_hash: &Sha256Hex,
        code: SecretString,
        request_id: &str,
    ) -> Result<LoginInitiation, WyrdError> {
        self.connections.require_keyring()?;
        let postgres = self.connections.postgres();
        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let login_state = consume_login_state(&mut conn, state_hash)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let login_state = login_state
            .ok_or_else(|| invalid_state("login state is missing, expired, or already consumed"))?;
        let trusted = self.bound_connection(tenant_id, &login_state).await?;
        let http = self.connections.http();
        let provider = discover_provider(&trusted.issuer, http).await?;
        let id_token = exchange_code_for_id_token(
            &provider,
            &trusted.client_id,
            &trusted.client_auth,
            &login_state.redirect_uri,
            &login_state.code_verifier,
            code,
            http,
        )
        .await?;
        self.finish_id_token_exchange(
            state_hash,
            &trusted,
            &login_state,
            &provider.metadata.id_token_signing_alg_values_supported,
            &id_token,
            request_id,
        )
        .await
    }

    /// Finish a consumed login in `trusted.tenant_id` once the provider has
    /// returned an ID token.
    ///
    /// The token's header algorithm must be one the provider's fresh
    /// discovery advertised in `supported_algorithms`; only then is it
    /// verified against `trusted` by the shared external verifier, which
    /// refuses symmetric algorithms even when the provider advertises them. The verified claims must carry
    /// the nonce recorded in `login_state` and satisfy OIDC authorized-party
    /// rules for the bound client ([`verify_authorized_party`]). The tenant's
    /// Active connection is then re-read and must still be the exact revision,
    /// issuer, and client the login bound. One tenant transaction then
    /// resolves the user by (issuer, `sub`) only and takes the User's
    /// tenant-qualified refresh-family lock, held through commit and taken
    /// before the connection slot lock as every refresh path orders them, so
    /// concurrent callbacks for one User serialize and each issued session and
    /// the final durable roles equal one callback's mapping. It then replaces
    /// the user's roles with those the verified groups map to (unmapped groups and unknown role
    /// names grant nothing; connection default roles are never applied to
    /// human login) and, when that changed the user's durable roles, stages
    /// one allowed `auth.user.roles.sync` audit event for the User, issues the
    /// session under the connection slot lock, and stores it sealed on the
    /// consumed state row with a fresh redemption expiry. The session never
    /// leaves this method except sealed. Returns how the login was initiated.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the token's algorithm was not
    /// advertised, the token fails verification or authorized-party checks,
    /// or the bound connection is no longer Active,
    /// [`WyrdError::InvalidNonce`] on a missing or mismatched nonce,
    /// [`WyrdError::InvalidState`] when the state row is no longer consumed
    /// and awaiting completion, [`WyrdError::Validation`] when no sealing
    /// keyring is configured, and the store, issuance, audit, and sealing
    /// errors; nothing commits unless every step succeeds, so a failed audit
    /// append — role sync or token exchange — leaves no role change, session,
    /// refresh row, or completion.
    pub async fn finish_id_token_exchange(
        &self,
        state_hash: &Sha256Hex,
        trusted: &TrustedIssuer,
        login_state: &LoginState,
        supported_algorithms: &[String],
        id_token: &str,
        request_id: &str,
    ) -> Result<LoginInitiation, WyrdError> {
        let tenant_id = trusted.tenant_id;
        let keyring = self.connections.require_keyring()?;
        verify_id_token_algorithm(supported_algorithms, id_token)?;
        let verified = self
            .verifier
            .verify_id_token_against(&trusted.verification(), id_token)
            .await
            .map_err(auth_error_to_wyrd)?;
        verify_nonce(&login_state.nonce, &verified.raw_claims)?;
        verify_authorized_party(&trusted.client_id, &verified.raw_claims)?;
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
            &verified.subject,
            verified.email.as_deref(),
        )
        .await
        .map_err(store_error)?;
        // Concurrent callbacks for this User would otherwise each replace
        // roles unseen by the other and the later one mint their union; the
        // family lock held to commit makes one callback's set the whole truth.
        lock_refresh_family(&mut conn, "user", principal_id)
            .await
            .map_err(store_error)?;
        let roles = role_names_to_refs(trusted, &verified.groups)?;
        // The provider just asserted this human's authority, and nothing else
        // in Wyrd grants a user a role. Recording it here is what makes the
        // grant table the truth a later refresh rotation can re-read.
        let role_names = roles.iter().map(RoleRef::as_str).collect::<Vec<_>>();
        if replace_user_roles(&mut conn, principal_id, &role_names)
            .await
            .map_err(store_error)?
        {
            append_auth_audit(&mut conn, &roles_sync_event(request_id, principal_id)).await?;
        }
        let exchanged = self
            .issuer
            .issue_human_session(
                &mut conn,
                principal_id,
                None,
                login_state.connection,
                request_id,
            )
            .await?;
        let sealed = seal_completion(keyring, &exchanged.into_response())?;
        if !complete_login_state(&mut conn, state_hash, &sealed, LOGIN_COMPLETION_TTL)
            .await
            .map_err(store_error)?
        {
            return Err(invalid_state(
                "login state is missing, expired, or already consumed",
            ));
        }
        conn.commit().await.map_err(store_error)?;
        Ok(login_state.initiation)
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

/// Why screened discovery produced no provider.
///
/// Keeps an issuer mismatch typed so candidate testing can report it as a
/// failed check while login and the callback convert it into the public
/// [`WyrdError::DiscoveryUnavailable`] through [`From`].
#[derive(Debug)]
pub(crate) enum DiscoveryFailure {
    /// The discovery document names a different issuer than requested.
    IssuerMismatch,
    /// Any other failure, already mapped to its public error.
    Unavailable(WyrdError),
}

impl From<DiscoveryFailure> for WyrdError {
    /// Project a discovery failure onto the public catalog; a mismatch keeps
    /// its stable `details.reason = "issuer_mismatch"`.
    fn from(failure: DiscoveryFailure) -> Self {
        match failure {
            DiscoveryFailure::IssuerMismatch => WyrdError::DiscoveryUnavailable {
                message: "the provider discovery document names a different issuer".to_owned(),
                details: serde_json::json!({ "reason": "issuer_mismatch" }),
            },
            DiscoveryFailure::Unavailable(error) => error,
        }
    }
}

/// Discover an issuer using Wyrd's process-owned TLS implementation.
///
/// Shared by login, the callback, platform login, and candidate testing, so
/// every provider discovery is screened and pinned to its issuer the same way.
///
/// # Errors
///
/// Returns [`DiscoveryFailure::IssuerMismatch`] when the discovery document
/// names a different issuer, and [`DiscoveryFailure::Unavailable`] carrying
/// [`WyrdError::DiscoveryUnavailable`] when the issuer URL is invalid, `http`
/// refuses the address behind it, or discovery otherwise fails. Cancellation
/// interrupts the request without persisting callback state.
pub(crate) async fn discover_provider(
    issuer: &IssuerUrl,
    http: ScreenedHttp,
) -> Result<OidcProvider, DiscoveryFailure> {
    let issuer_url = Url::parse(issuer.as_str()).map_err(|error| {
        DiscoveryFailure::Unavailable(provider_unreachable(format_args!(
            "trusted issuer URL could not be parsed: {error}"
        )))
    })?;
    let client = http
        .client_for(&issuer_url)
        .await
        .map_err(|error| DiscoveryFailure::Unavailable(screen_error(&error)))?;
    OidcProvider::discover(issuer_url, client)
        .await
        .map_err(|error| match error {
            OidcError::IssuerMismatch { .. } => {
                tracing::warn!(error = %error, "OIDC discovery failed");
                DiscoveryFailure::IssuerMismatch
            }
            error => DiscoveryFailure::Unavailable(provider_unreachable(format_args!(
                "OIDC discovery failed: {error}"
            ))),
        })
}

/// Exchange one validated authorization code for an issuer ID token.
///
/// The request includes the configured client authentication material and the
/// callback's PKCE verifier; no token is persisted by this helper.
///
/// # Errors
///
/// Returns [`WyrdError::DiscoveryUnavailable`] when another Rustls provider
/// already owns the process or discovery omitted the token endpoint. Returns
/// the callback's structured authentication errors when request construction,
/// transport, response parsing (including a body over
/// [`wyrd_auth_oidc::MAX_RESPONSE_BYTES`]), or token validation fails. Cancellation can
/// leave the remote exchange outcome unknown, but this helper makes no local
/// durable progress.
pub(crate) async fn exchange_code_for_id_token(
    provider: &OidcProvider,
    client_id: &str,
    client_auth: &ClientAuth,
    redirect_uri: &str,
    code_verifier: &SecretString,
    code: SecretString,
    http: ScreenedHttp,
) -> Result<String, WyrdError> {
    let Some(token_endpoint) = provider.metadata.token_endpoint.clone() else {
        return Err(WyrdError::DiscoveryUnavailable {
            message: "OIDC discovery document did not advertise a token endpoint".to_owned(),
            details: serde_json::json!({}),
        });
    };

    let client = http
        .client_for(&token_endpoint)
        .await
        .map_err(|error| screen_error(&error))?;
    let request = authorization_code_request(
        &client,
        token_endpoint,
        client_id,
        client_auth,
        redirect_uri,
        code.expose_secret(),
        code_verifier.expose_secret(),
    )?;
    let response = request.send().await.map_err(|error| {
        tracing::warn!(error = %error, "OIDC token endpoint unavailable");
        WyrdError::AuthVerifyUnavailable {
            message: "OIDC token endpoint unavailable".to_owned(),
            details: serde_json::json!({ "retry_after_seconds": 1 }),
        }
    })?;
    if !response.status().is_success() {
        return if response.status().is_server_error() {
            Err(WyrdError::AuthVerifyUnavailable {
                message: "OIDC token endpoint unavailable".to_owned(),
                details: serde_json::json!({ "retry_after_seconds": 1 }),
            })
        } else {
            Err(invalid_token("authorization code exchange was rejected"))
        };
    }

    let decode_failed = |error: &dyn std::fmt::Display| {
        tracing::warn!(error = %error, "OIDC token response decode failed");
        WyrdError::AuthVerifyUnavailable {
            message: "OIDC token response decode failed".to_owned(),
            details: serde_json::json!({ "retry_after_seconds": 1 }),
        }
    };
    let body = read_bounded_body(response)
        .await
        .map_err(|error| decode_failed(&error))?;
    serde_json::from_slice::<TokenEndpointResponse>(&body)
        .map(|body| body.id_token)
        .map_err(|error| decode_failed(&error))
}

/// Build one authorization-code token request with the client's configured
/// authentication.
///
/// Shared by the login callback's real exchange and candidate testing's
/// invalid-code probe, so the probe proves exactly the request a login sends:
/// the same grant form, PKCE verifier, redirect URI, and `client_secret_basic`
/// or `client_secret_post` placement. Nothing is sent here.
///
/// # Errors
/// Returns [`WyrdError::Internal`] for `private_key_jwt`, which human
/// connections cannot store and this request cannot sign.
pub(crate) fn authorization_code_request(
    client: &reqwest::Client,
    token_endpoint: Url,
    client_id: &str,
    client_auth: &ClientAuth,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
) -> Result<RequestBuilder, WyrdError> {
    let mut request = client.post(token_endpoint);
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];
    match client_auth {
        ClientAuth::SecretBasic(secret) => {
            request = request.basic_auth(client_id, Some(secret.expose_secret()));
        }
        ClientAuth::SecretPost(secret) => form.push(("client_secret", secret.expose_secret())),
        ClientAuth::PrivateKeyJwt => {
            return Err(WyrdError::Internal {
                message: "private_key_jwt client authentication is not implemented".to_owned(),
                details: serde_json::json!({ "client_auth": "private_key_jwt" }),
            });
        }
        ClientAuth::Public => {}
    }
    Ok(request.form(&form))
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

/// Best-effort audit of a refused human authorization-code exchange.
///
/// Stages one denied `auth.token.exchange` event carrying the closed failure
/// code in its own transaction. A refusal rolls back any user it resolved, so
/// the event names the nil principal. Staging failures are logged, never
/// returned, so the caller's original error still reaches the client.
pub async fn audit_authorization_code_failure(
    postgres: &WyrdPostgres,
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
    record_auth_audit_best_effort(postgres, tenant_id, &event).await;
}

/// Require the verified ID token's `nonce` claim to equal the nonce the login
/// state recorded.
///
/// # Errors
/// Returns [`WyrdError::InvalidNonce`] when the claim is missing or differs.
pub fn verify_nonce(expected: &str, claims: &Value) -> Result<(), WyrdError> {
    let Some(nonce) = claims.get("nonce").and_then(Value::as_str) else {
        return Err(invalid_nonce("id token nonce is missing"));
    };
    if nonce != expected {
        return Err(invalid_nonce("id token nonce mismatch"));
    }
    Ok(())
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

/// Require the ID token's header algorithm to be one the provider advertised
/// for ID tokens in its fresh discovery document.
///
/// This checks advertised-set membership only; it does not itself refuse
/// symmetric algorithms. Its sole caller,
/// [`AuthorizationCodeExchange::finish_id_token_exchange`], immediately hands
/// the token to [`ExternalVerifier::verify_id_token_against`], which owns
/// symmetric-algorithm (`HS256`/`HS384`/`HS512`) rejection and the signature
/// and JWKS verification. This check narrows a tenant login to the
/// provider's own policy before that key lookup. Advertised names that do not
/// parse as a known algorithm are ignored rather than rejected, so they can
/// never admit a token.
///
/// # Errors
/// Returns [`WyrdError::InvalidToken`] when the token header is malformed or
/// does not decode, or when its algorithm is absent from the advertised set.
/// An unparseable advertised value is not itself an error: it is skipped, so
/// it can only contribute to the absent-from-advertised-set failure.
pub fn verify_id_token_algorithm(supported: &[String], id_token: &str) -> Result<(), WyrdError> {
    let header = jsonwebtoken::decode_header(id_token)
        .map_err(|_| invalid_token("id token header is malformed"))?;
    let advertised = supported
        .iter()
        .filter_map(|name| Algorithm::from_str(name).ok())
        .any(|alg| alg == header.alg);
    if advertised {
        Ok(())
    } else {
        Err(invalid_token(
            "id token algorithm is not advertised by the provider",
        ))
    }
}

/// Require OIDC authorized-party semantics for the connection's client.
///
/// A token for several audiences must name this client in a string `azp`;
/// a present `azp` must equal this client for any audience. The generic
/// verifier has already proven `client_id` is among the audiences.
///
/// # Errors
/// Returns [`WyrdError::InvalidToken`] when `azp` is required and missing, or
/// is present and not exactly `client_id`.
pub fn verify_authorized_party(client_id: &str, claims: &Value) -> Result<(), WyrdError> {
    let multiple_audiences = claims
        .get("aud")
        .and_then(Value::as_array)
        .is_some_and(|audiences| audiences.len() > 1);
    match claims.get("azp") {
        None if !multiple_audiences => Ok(()),
        Some(Value::String(azp)) if azp == client_id => Ok(()),
        _ => Err(invalid_token(
            "id token authorized party does not match the client",
        )),
    }
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

fn invalid_nonce(message: &str) -> WyrdError {
    WyrdError::InvalidNonce {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
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

/// Outbound address screening on the federated callback's own HTTP calls.
///
/// A trusted issuer URL is screened when it is registered, but DNS can answer
/// differently later; these cases drive the callback against a provider that
/// really does resolve to a blocked range.
#[cfg(test)]
mod screening_tests {
    use secrecy::SecretString;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::{AddressPolicy, ClientAuth, OidcProvider, ScreenedHttp};
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_spec::error::WyrdError;

    /// Stand up a provider that advertises itself on loopback.
    ///
    /// Loopback is exactly the address a rebinding answer aims at, so a
    /// deployment that blocks internal ranges must refuse this provider at the
    /// moment of every request — even though the URL was accepted when the
    /// issuer was configured under a different answer.
    async fn loopback_provider() -> (MockServer, IssuerUrl) {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "id_token_signing_alg_values_supported": ["RS256"],
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id_token": "never.reached.here"
            })))
            .mount(&server)
            .await;
        let url = IssuerUrl::new(issuer).expect("mock issuer is a valid URL");
        (server, url)
    }

    /// Beginning a login screens the issuer again and makes no request.
    #[tokio::test]
    async fn begin_login_refuses_an_internal_issuer_without_reaching_it() {
        let (server, issuer) = loopback_provider().await;

        let error =
            super::discover_provider(&issuer, ScreenedHttp::new(AddressPolicy::BlockInternal))
                .await
                .expect_err("an internal issuer is refused");

        assert!(matches!(
            WyrdError::from(error),
            WyrdError::DiscoveryUnavailable { .. }
        ));
        assert!(
            server
                .received_requests()
                .await
                .is_some_and(|r| r.is_empty()),
            "the refusal must happen before any request leaves the process"
        );
    }

    /// The token exchange screens the endpoint discovery handed it, not the
    /// address that was acceptable when discovery ran.
    #[tokio::test]
    async fn the_token_exchange_refuses_an_internal_endpoint_without_reaching_it() {
        let (server, issuer) = loopback_provider().await;

        let provider = OidcProvider::discover(
            url::Url::parse(issuer.as_str()).expect("issuer parses"),
            ScreenedHttp::allowing_internal()
                .client_for(&url::Url::parse(issuer.as_str()).expect("issuer parses"))
                .await
                .expect("a permissive deployment reaches its loopback provider"),
        )
        .await
        .expect("discovery succeeds under the permissive policy");
        let discovery_requests = server
            .received_requests()
            .await
            .expect("the mock records requests")
            .len();

        let error = super::exchange_code_for_id_token(
            &provider,
            "wyrd",
            &ClientAuth::Public,
            "https://tenant.example/auth/callback",
            &SecretString::from("verifier"),
            SecretString::from("code"),
            ScreenedHttp::new(AddressPolicy::BlockInternal),
        )
        .await
        .expect_err("an internal token endpoint is refused");

        assert!(matches!(error, WyrdError::DiscoveryUnavailable { .. }));
        assert_eq!(
            server
                .received_requests()
                .await
                .expect("the mock records requests")
                .len(),
            discovery_requests,
            "no token request may leave the process after the screen refuses"
        );
    }
}
