//! Domain logic for the human OIDC callback flow.

use std::sync::Arc;

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
use wyrd_spec::auth::{IssuerUrl, TokenResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::queries::auth::{
    delete_user, insert_user, replace_user_roles, upsert_user_identity, user_id_by_identity,
};
use wyrd_sql::{SqlError, TenantConn, WyrdPostgres};

use crate::audit::{
    TOKEN_EXCHANGE_OPERATION, auth_event, auth_failure_code, record_auth_audit_best_effort,
};
use crate::connections::HumanConnections;
use crate::error::{auth_error_to_wyrd, screen_error, store_error};
use crate::exchange_api_key::role_refs;
use crate::issuance::TenantTokenIssuer;
use crate::login::{LoginStateEntry, PgLoginStateStore};
use crate::pg_resolvers::PgIssuerResolver;

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    id_token: String,
}

struct FinishAuthorizationCodeInput<'a> {
    postgres: &'a WyrdPostgres,
    tenant_id: DataTenantId,
    trusted: &'a TrustedIssuer,
    login_state: &'a LoginStateEntry,
    id_token: &'a str,
    request_id: &'a str,
    audit_principal_id: &'a mut Uuid,
}

/// Human OIDC authorization-code exchange service.
#[derive(Clone)]
pub struct AuthorizationCodeExchange {
    /// The shared tenant issuance workflow.
    pub issuer: TenantTokenIssuer,
    /// External OIDC id-token verifier.
    pub verifier: Arc<ExternalVerifier<PgIssuerResolver>>,
    /// The tenant human-connection owner; the only source of human trust and
    /// of the screened HTTP capability every provider request is made through.
    pub connections: HumanConnections,
}

impl std::fmt::Debug for AuthorizationCodeExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationCodeExchange")
            .finish_non_exhaustive()
    }
}

impl AuthorizationCodeExchange {
    /// Execute the authorization-code grant.
    ///
    /// The login must still be on the exact connection revision its state was
    /// bound to when it began, and issuance re-checks that revision under the
    /// connection slot lock, so a replacement, deactivation, or removal that
    /// commits while the provider round-trip is in flight refuses the session.
    ///
    /// # Errors
    /// Returns [`WyrdError`] when the login state is unknown, already consumed,
    /// or expired, when the login's connection is no longer the tenant's Active
    /// revision, when the issuer refuses the code or its id token fails
    /// verification, when role persistence or successor issuance fails, or when
    /// the decision cannot be audited. Every refusal is audited before it is
    /// returned, and the grant transaction commits or rolls back whole.
    pub async fn execute(
        &self,
        postgres: &WyrdPostgres,
        tenant_id: DataTenantId,
        code: SecretString,
        state_key: &str,
        request_id: &str,
    ) -> Result<TokenResponse, WyrdError> {
        let store = PgLoginStateStore::new(postgres.clone());
        let mut audit_principal_id = Uuid::nil();
        let result = async {
            let Some(login_state) = store
                .take(tenant_id, state_key)
                .await
                .map_err(store_error)?
            else {
                return Err(invalid_state(
                    "login state is missing, expired, or already consumed",
                ));
            };
            let issuer = IssuerUrl::new(login_state.issuer.clone())
                .map_err(|_| invalid_token("stored issuer URL is invalid"))?;
            let active = self
                .connections
                .active_connection_for(tenant_id, &issuer)
                .await?;
            if active.binding != login_state.connection {
                return Err(invalid_token(
                    "the login connection changed while the login was in progress",
                ));
            }
            let trusted = active.trusted;
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
            let token = self
                .finish_authorization_code_exchange(FinishAuthorizationCodeInput {
                    postgres,
                    tenant_id,
                    trusted: &trusted,
                    login_state: &login_state,
                    id_token: &id_token,
                    request_id,
                    audit_principal_id: &mut audit_principal_id,
                })
                .await?;
            Ok(token)
        }
        .await;

        match result {
            Ok(token) => Ok(token),
            Err(error) => {
                audit_authorization_code_failure(
                    postgres,
                    tenant_id,
                    audit_principal_id,
                    request_id,
                    &error,
                )
                .await;
                Err(error)
            }
        }
    }

    /// Complete a human OIDC login after the provider has returned an ID token.
    ///
    /// The session is bound to `login_state.connection` and issued only while
    /// that exact revision is still Active. Returns the token and the resolved
    /// user id.
    ///
    /// # Errors
    /// Returns the errors of the verification, persistence, and issuance steps
    /// of [`Self::execute`]; nothing commits unless every step succeeds.
    pub async fn finish_id_token_exchange(
        &self,
        postgres: &WyrdPostgres,
        tenant_id: DataTenantId,
        trusted: &TrustedIssuer,
        login_state: &LoginStateEntry,
        id_token: &str,
        request_id: &str,
    ) -> Result<(TokenResponse, Uuid), WyrdError> {
        let mut audit_principal_id = Uuid::nil();
        let token = self
            .finish_authorization_code_exchange(FinishAuthorizationCodeInput {
                postgres,
                tenant_id,
                trusted,
                login_state,
                id_token,
                request_id,
                audit_principal_id: &mut audit_principal_id,
            })
            .await?;
        Ok((token, audit_principal_id))
    }

    /// Complete the grant once the id token has been verified: persist the
    /// asserted roles, then issue the session through the shared issuance
    /// workflow inside the same transaction, so the access token carries the
    /// permissions of the roles just recorded. Issuance takes the connection
    /// slot lock and requires the login's bound revision to still be Active.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the bound connection revision
    /// is no longer Active, and [`WyrdError`] when identity or role
    /// persistence, issuance, the canonical audit append, or the commit fails;
    /// no session is returned unless all of them committed together.
    async fn finish_authorization_code_exchange(
        &self,
        input: FinishAuthorizationCodeInput<'_>,
    ) -> Result<TokenResponse, WyrdError> {
        let FinishAuthorizationCodeInput {
            postgres,
            tenant_id,
            trusted,
            login_state,
            id_token,
            request_id,
            audit_principal_id,
        } = input;
        let verified = self
            .verifier
            .verify_external_against(&trusted.verification(), id_token)
            .await
            .map_err(auth_error_to_wyrd)?;
        verify_nonce(login_state, &verified.raw_claims)?;

        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
        let principal_id = ensure_user_identity(
            &mut conn,
            trusted,
            &verified.subject,
            verified.email.as_deref(),
        )
        .await
        .map_err(store_error)?;
        *audit_principal_id = principal_id;
        let roles = role_names_to_refs(trusted, &verified.groups)?;
        // The provider just asserted this human's authority, and nothing else
        // in Wyrd grants a user a role. Recording it here is what makes the
        // grant table the truth a later refresh rotation can re-read; without
        // it, renewal would mint an authority-free successor.
        let role_names = roles.iter().map(RoleRef::as_str).collect::<Vec<_>>();
        replace_user_roles(&mut conn, principal_id, &role_names)
            .await
            .map_err(store_error)?;
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
        conn.commit().await.map_err(store_error)?;

        Ok(exchanged.into_response())
    }
}

/// Discover an issuer using Wyrd's process-owned TLS implementation.
///
/// Shared by login, the callback, and candidate testing, so every provider
/// discovery is screened and pinned to its issuer the same way.
///
/// # Errors
///
/// Returns [`WyrdError::DiscoveryUnavailable`] when the issuer URL is invalid,
/// `http` refuses the address behind it, or discovery fails; a discovery
/// document naming a different issuer carries `details.reason =
/// "issuer_mismatch"`. Cancellation interrupts the request without persisting
/// callback state.
pub(crate) async fn discover_provider(
    issuer: &IssuerUrl,
    http: ScreenedHttp,
) -> Result<OidcProvider, WyrdError> {
    let issuer_url = Url::parse(issuer.as_str()).map_err(|_| WyrdError::DiscoveryUnavailable {
        message: "trusted issuer URL could not be parsed".to_owned(),
        details: serde_json::json!({}),
    })?;
    let client = http
        .client_for(&issuer_url)
        .await
        .map_err(|error| screen_error(&error))?;
    OidcProvider::discover(issuer_url, client)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "OIDC discovery failed");
            match error {
                OidcError::IssuerMismatch { .. } => WyrdError::DiscoveryUnavailable {
                    message: "the provider discovery document names a different issuer".to_owned(),
                    details: serde_json::json!({ "reason": "issuer_mismatch" }),
                },
                _ => WyrdError::DiscoveryUnavailable {
                    message: "OIDC discovery unavailable".to_owned(),
                    details: serde_json::json!({}),
                },
            }
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
/// code in its own transaction. `principal_id` is nil when the refusal happened
/// before a user was resolved. Staging failures are logged, never returned, so
/// the caller's original error still reaches the client.
pub async fn audit_authorization_code_failure(
    postgres: &WyrdPostgres,
    tenant_id: DataTenantId,
    principal_id: Uuid,
    request_id: &str,
    error: &WyrdError,
) {
    let event = auth_event(
        request_id,
        TOKEN_EXCHANGE_OPERATION,
        PrincipalId::new(principal_id),
        PrincipalKindTag::User,
        None,
        AuditOutcome::Denied,
        AuditDetail::AuthFailure {
            error_code: auth_failure_code(error),
        },
    );
    record_auth_audit_best_effort(postgres.app_pool(), tenant_id, &event).await;
}

/// Verify the OIDC nonce bound to the login state.
pub fn verify_nonce(state: &LoginStateEntry, claims: &Value) -> Result<(), WyrdError> {
    let Some(nonce) = claims.get("nonce").and_then(Value::as_str) else {
        return Err(invalid_nonce("id token nonce is missing"));
    };
    if nonce != state.nonce {
        return Err(invalid_nonce("id token nonce mismatch"));
    }
    Ok(())
}

/// Map trusted external groups plus issuer defaults to local Wyrd role refs.
pub fn role_names_to_refs(
    trusted: &TrustedIssuer,
    groups: &[String],
) -> Result<Vec<RoleRef>, WyrdError> {
    let mut names = trusted.default_roles.clone();
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

        let error = crate::login::discover_authorization_endpoint(
            &issuer,
            ScreenedHttp::new(AddressPolicy::BlockInternal),
        )
        .await
        .expect_err("an internal issuer is refused");

        assert!(matches!(error, WyrdError::DiscoveryUnavailable { .. }));
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
