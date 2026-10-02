//! Domain logic for the common human OIDC callback.
//!
//! The callback carries the provider's `code`, `state`, and optional RFC 9207
//! `iss`. The state's SHA-256 names one login-state row across tenants; that
//! row alone decides the tenant, connection revision, issuer, client,
//! redirect, PKCE verifier, nonce, and initiation binding. No request header
//! takes part. A present `iss` must name that recorded issuer.
//!
//! A candidate connection test's state takes the same path against its bound
//! candidate instead of the Active connection: the code is redeemed and the ID
//! token verified exactly as a login, and then only that candidate revision is
//! marked tested. A test issues no session, credential, or User.

use secrecy::SecretString;
use uuid::Uuid;
use wyrd_auth_oidc::{CodeRedemption, MappedClaims, TrustedIssuer};
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
use crate::error::{relying_party_error, store_error};
use crate::exchange_api_key::role_refs;
use crate::issuance::TenantTokenIssuer;
use crate::login::{LOGIN_COMPLETION_TTL, seal_completion};

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
    /// Complete a login from the provider callback's `code`, `state`, and
    /// optional RFC 9207 `iss`.
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
    /// party; and [`Self::finish_id_token_exchange`] issues and seals the
    /// session. Returns how the login was
    /// initiated, which decides the callback's response: a browser login is
    /// redirected to the BFF completion route, a CLI login and a candidate
    /// connection test get a static page.
    ///
    /// A connection test's state is bound to a candidate revision, not the
    /// Active connection: the provider is discovered from the recorded issuer
    /// first, and that exact candidate — with the freshly discovered JWKS URI
    /// it has not stored yet — is the trust the code and ID token are checked
    /// against ([`HumanConnections::tested_candidate`]).
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] when the state is unknown, expired,
    /// or replayed; [`WyrdError::Validation`] when no sealing keyring is
    /// configured; [`WyrdError::InvalidToken`] when the bound connection is no
    /// longer Active (or, for a test, the bound candidate changed), the
    /// response issuer is mismatched or required and missing, the provider
    /// refuses the code, or the ID token fails verification;
    /// [`WyrdError::InvalidNonce`] on a missing or mismatched nonce;
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
        response_issuer: Option<&str>,
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
            .complete(tenant_id, &state_hash, code, response_issuer, request_id)
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
        response_issuer: Option<&str>,
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
                .tested_candidate(tenant_id, &login_state, provider.jwks_uri().url())
                .await?
                .ok_or_else(|| {
                    invalid_token("the login connection changed while the login was in progress")
                })?;
            (trusted, provider)
        } else {
            let trusted = self.bound_connection(tenant_id, &login_state).await?;
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
            &login_state,
            &verified.identity,
            request_id,
        )
        .await
    }

    /// Finish a consumed login in `trusted.tenant_id` once the relying party
    /// has verified the provider's ID token and mapped it to `identity`.
    ///
    /// The tenant's Active connection is re-read and must still be the exact revision,
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
    /// A connection test stops here: instead of the Active
    /// connection re-check and any user or session work,
    /// [`HumanConnections::stamp_test_sign_in`] re-checks the tester's
    /// authority and marks only the bound candidate revision tested with
    /// `trusted`'s JWKS URI. The consumed test state is left for expiry
    /// purge; it never carries a completion.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the bound connection is no
    /// longer Active, [`WyrdError::InvalidState`] when the state row is no longer consumed
    /// and awaiting completion, [`WyrdError::Validation`] when no sealing
    /// keyring is configured, the errors of
    /// [`HumanConnections::stamp_test_sign_in`] for a connection test, and the
    /// store, issuance, audit, and sealing errors; nothing commits unless every step succeeds, so a failed audit
    /// append — role sync or token exchange — leaves no role change, session,
    /// refresh row, or completion.
    pub async fn finish_id_token_exchange(
        &self,
        state_hash: &Sha256Hex,
        trusted: &TrustedIssuer,
        login_state: &LoginState,
        identity: &MappedClaims,
        request_id: &str,
    ) -> Result<LoginInitiation, WyrdError> {
        let tenant_id = trusted.tenant_id;
        let keyring = self.connections.require_keyring()?;
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
            return Ok(login_state.initiation);
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
        let roles = role_names_to_refs(trusted, &identity.groups)?;
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
