//! Domain logic for the human OIDC callback flow.

use std::sync::Arc;

use chrono::{Duration as ChronoDuration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_oidc::{ClientAuth, OidcProvider, ScreenedHttp, TrustedIssuer};
use wyrd_auth_verify::{TokenPrincipalRef, TokenVerifier};
use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::auth::{IssuerUrl, TokenResponse, TokenType};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::queries::auth::{
    advance_user_epoch_to_second, delete_user, insert_refresh_token, insert_refresh_token_rotated,
    insert_user, replace_user_roles, upsert_user_identity, user_id_by_identity,
};
use wyrd_sql::{SqlError, TenantConn, WyrdPostgres};

use crate::audit::{
    TOKEN_EXCHANGE_OPERATION, append_auth_audit, auth_event, auth_failure_code,
    record_auth_audit_best_effort,
};
use crate::error::{auth_error_to_wyrd, screen_error};
use crate::exchange_api_key::{ExchangedToken, role_refs, token_hash};
use crate::login::{LoginStateEntry, PgLoginStateStore};
use crate::permission_resolver::SqlPermissionResolver;
use crate::pg_resolvers::PgIssuerResolver;

const ACCESS_TTL: ChronoDuration = ChronoDuration::minutes(15);
const REFRESH_TTL: ChronoDuration = ChronoDuration::days(30);

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
    /// JWT issuing key.
    pub issuing_key: Arc<IssuingKey>,
    /// External/OIDC token verifier.
    pub verifier: Arc<TokenVerifier<SqlPermissionResolver, PgIssuerResolver>>,
    /// Tenant-scoped trusted issuer resolver.
    pub trusted_issuer_resolver: Arc<PgIssuerResolver>,
    /// Screened HTTP capability every provider request is made through.
    pub http: ScreenedHttp,
}

impl std::fmt::Debug for AuthorizationCodeExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationCodeExchange")
            .finish_non_exhaustive()
    }
}

impl AuthorizationCodeExchange {
    /// Execute the authorization-code grant.
    pub async fn execute(
        &self,
        postgres: &WyrdPostgres,
        tenant_id: DataTenantId,
        code: SecretString,
        state_key: &str,
        request_id: &str,
    ) -> Result<TokenResponse, WyrdError> {
        let store = PgLoginStateStore::new(postgres.app_pool().clone());
        let mut audit_principal_id = Uuid::nil();
        let result = async {
            let Some(login_state) = store.take(tenant_id, state_key).await.map_err(sql_error)?
            else {
                return Err(invalid_state(
                    "login state is missing, expired, or already consumed",
                ));
            };
            let issuer = IssuerUrl::new(login_state.issuer.clone())
                .map_err(|_| invalid_token("stored issuer URL is invalid"))?;
            let trusted = crate::issuer::trusted_issuer(
                Some(self.trusted_issuer_resolver.as_ref()),
                tenant_id,
                &issuer,
            )
            .await?;
            let provider = discover_provider(&trusted.issuer, self.http).await?;
            let id_token = exchange_code_for_id_token(
                &provider,
                &trusted.client_id,
                &trusted.client_auth,
                &login_state.redirect_uri,
                &login_state.code_verifier,
                code,
                self.http,
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
            .verify_external(&tenant_id, id_token)
            .await
            .map_err(auth_error_to_wyrd)?;
        verify_nonce(login_state, &verified.raw_claims)?;

        let mut conn = postgres.tenant_conn(tenant_id).await.map_err(sql_error)?;
        let principal_id = ensure_user_identity(
            &mut conn,
            trusted,
            &verified.subject,
            verified.email.as_deref(),
        )
        .await
        .map_err(sql_error)?;
        *audit_principal_id = principal_id;
        let roles = role_names_to_refs(trusted, &verified.groups)?;
        // The provider just asserted this human's authority, and nothing else
        // in Wyrd grants a user a role. Recording it here is what makes the
        // grant table the truth a later refresh rotation can re-read; without
        // it, renewal would mint an authority-free successor.
        let role_names = roles.iter().map(RoleRef::as_str).collect::<Vec<_>>();
        let roles_changed = replace_user_roles(&mut conn, principal_id, &role_names)
            .await
            .map_err(sql_error)?;
        // A role the provider withdrew is still spendable by every access token
        // this human already holds, because the names travel in signed claims.
        // Moving the authorization epoch in the same transaction is what makes
        // the withdrawal reach those live sessions; the successor issued below
        // is minted after the epoch and is admitted by it. An unchanged login
        // moves nothing, so re-authenticating never signs the human out of
        // their other sessions.
        if roles_changed {
            advance_user_epoch_to_second(&mut conn, principal_id)
                .await
                .map_err(sql_error)?;
        }
        let exchanged = issue_and_record_user_session(
            &mut conn,
            self.issuing_key.as_ref(),
            tenant_id,
            principal_id,
            roles,
            None,
            request_id,
        )
        .await?;
        conn.commit().await.map_err(sql_error)?;

        Ok(exchanged.into_response())
    }
}

/// Discover an issuer using Wyrd's process-owned TLS implementation.
///
/// # Errors
///
/// Returns [`WyrdError::DiscoveryUnavailable`] when the issuer URL is invalid,
/// `http` refuses the address behind it, or discovery fails. Cancellation
/// interrupts the request without persisting callback state.
pub(crate) async fn discover_provider(
    issuer: &IssuerUrl,
    http: ScreenedHttp,
) -> Result<OidcProvider, WyrdError> {
    let issuer_url =
        url::Url::parse(issuer.as_str()).map_err(|_| WyrdError::DiscoveryUnavailable {
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
            WyrdError::DiscoveryUnavailable {
                message: "OIDC discovery unavailable".to_owned(),
                details: serde_json::json!({}),
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
/// transport, response parsing, or token validation fails. Cancellation can
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
    let mut request = client.post(token_endpoint);
    let mut form = vec![
        ("grant_type", "authorization_code".to_owned()),
        ("code", code.expose_secret().to_owned()),
        ("client_id", client_id.to_owned()),
        ("redirect_uri", redirect_uri.to_owned()),
        ("code_verifier", code_verifier.expose_secret().to_owned()),
    ];

    match client_auth {
        ClientAuth::SecretBasic(secret) => {
            request = request.basic_auth(
                client_id.to_owned(),
                Some(secret.expose_secret().to_owned()),
            );
        }
        ClientAuth::SecretPost(secret) => {
            form.push(("client_secret", secret.expose_secret().to_owned()));
        }
        ClientAuth::PrivateKeyJwt => {
            return Err(WyrdError::Internal {
                message: "private_key_jwt client authentication is not implemented".to_owned(),
                details: serde_json::json!({ "client_auth": "private_key_jwt" }),
            });
        }
        ClientAuth::Public => {}
    }

    let response = request.form(&form).send().await.map_err(|error| {
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

    response
        .json::<TokenEndpointResponse>()
        .await
        .map(|body| body.id_token)
        .map_err(|error| {
            tracing::warn!(error = %error, "OIDC token response decode failed");
            WyrdError::AuthVerifyUnavailable {
                message: "OIDC token response decode failed".to_owned(),
                details: serde_json::json!({ "retry_after_seconds": 1 }),
            }
        })
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

/// Issue a human session's access and refresh pair and audit the grant.
///
/// The one owner for both ends of a human session: the OIDC callback that
/// establishes it and the refresh grant that renews it. Keeping them together
/// is what makes the family chain, the TTLs, and the audited grant identical
/// on renewal — a second issuance site would drift from this one silently.
///
/// `rotated_from` is absent at first login and carries the consumed refresh row
/// on renewal. It is both the family back-link that makes reuse detectable and
/// the credential id of the renewed session: a federated sign-in presents a
/// provider token and names no stored credential, but a request authenticated
/// by a stored refresh row does.
///
/// Only human sessions get a refresh token. A machine client re-exchanges its
/// durable credential instead, so no machine grant reaches this owner.
///
/// # Errors
/// Returns a [`WyrdError`] when the access or refresh token cannot be signed,
/// the refresh row cannot be written, or the audit append fails. Nothing is
/// committed here: the caller's transaction commits the row, the audit, and
/// the session together or not at all.
pub(crate) async fn issue_and_record_user_session(
    conn: &mut TenantConn<'_>,
    issuing_key: &IssuingKey,
    tenant_id: DataTenantId,
    principal_id: Uuid,
    roles: Vec<RoleRef>,
    rotated_from: Option<Uuid>,
    request_id: &str,
) -> Result<ExchangedToken, WyrdError> {
    let principal = Principal::new(
        PrincipalId::new(principal_id),
        PrincipalKind::User,
        tenant_id,
        roles.clone(),
        PermissionSet::default(),
    );
    let access_token = issuing_key
        .issue_user_access_token(
            TokenPrincipalRef::from(&principal),
            roles.clone(),
            rotated_from,
            ACCESS_TTL,
        )
        .map_err(|error| issue_error(&error))?;
    let refresh_token = issuing_key
        .issue_refresh_token(
            PrincipalKindTag::User,
            PrincipalId::new(principal_id),
            tenant_id,
            REFRESH_TTL,
        )
        .map_err(|error| issue_error(&error))?;
    let now = Utc::now();
    let expires_at = now + ACCESS_TTL;
    let hash = token_hash(&refresh_token);
    let successor = Uuid::new_v4();
    match rotated_from {
        Some(predecessor) => insert_refresh_token_rotated(
            conn,
            successor,
            "user",
            principal_id,
            &hash,
            now + REFRESH_TTL,
            predecessor,
        )
        .await
        .map_err(sql_error)?,
        None => insert_refresh_token(
            conn,
            successor,
            "user",
            principal_id,
            &hash,
            now + REFRESH_TTL,
        )
        .await
        .map_err(sql_error)?,
    }
    let user = PrincipalId::new(principal_id);
    let event = auth_event(
        request_id,
        TOKEN_EXCHANGE_OPERATION,
        user,
        PrincipalKindTag::User,
        None,
        AuditOutcome::Allowed,
        AuditDetail::TokenExchange {
            subject_principal_id: user,
            actor_principal_id: user,
            delegation_chain: Vec::new(),
            expires_at,
        },
    )
    .with_credential_id(rotated_from);
    append_auth_audit(conn, &event).await?;

    Ok(ExchangedToken {
        access_token: SecretString::from(access_token),
        refresh_token: Some(SecretString::from(refresh_token)),
        token_type: TokenType::Bearer,
        expires_at,
    })
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

fn sql_error(error: impl Into<SqlError>) -> WyrdError {
    let error = error.into();
    tracing::warn!(error = %error, "OIDC callback SQL unavailable");
    WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": 1 }),
    }
}

fn issue_error(error: &wyrd_auth_issue::IssueError) -> WyrdError {
    tracing::warn!(error = %error, "OIDC token issue failed");
    WyrdError::Internal {
        message: "token issue failed".to_owned(),
        details: serde_json::json!({}),
    }
}

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

#[cfg(test)]
mod pg_tests {
    //! Durable proof for the role half of a federated sign-in.
    //!
    //! The callback replaces the human's persisted roles and, when that set
    //! actually moved, advances their authorization epoch in the same
    //! transaction. Both halves are schema behavior, so they are proven against
    //! real Postgres rather than a stand-in.

    use chrono::Utc;
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_sql::queries::auth::{
        advance_user_epoch_to_second, insert_role, insert_user, replace_user_roles,
        user_revocation_epoch,
    };

    /// Seed a user and two assignable roles, returning the user's id.
    async fn seed_user_with_roles(fixture: &PgFixture) -> Uuid {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user_id = Uuid::new_v4();
        insert_user(&mut conn, user_id, Some("federated@test.com"), "oidc", None)
            .await
            .expect("user inserts");
        for name in ["runtime_admin", "writer"] {
            insert_role(
                &mut conn,
                Uuid::new_v4(),
                name,
                &serde_json::json!([]),
                false,
            )
            .await
            .expect("role inserts");
        }
        replace_user_roles(&mut conn, user_id, &["runtime_admin", "writer"])
            .await
            .expect("initial roles persist");
        conn.commit().await.expect("seed commits");
        user_id
    }

    /// A login that withdraws a role reports the change and moves the epoch.
    ///
    /// Role names travel in signed access-token claims, so the withdrawal only
    /// reaches a live session through the epoch. The epoch must also stay at or
    /// below the second the successor is issued in, or the login would revoke
    /// the session it just established.
    ///
    /// # Panics
    /// Panics when the role write fails, when the reduced set is not reported
    /// as a change, or when the epoch does not land in the current second.
    #[tokio::test]
    async fn a_withdrawn_role_reports_a_change_and_advances_the_epoch() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let user_id = seed_user_with_roles(&fixture).await;
        let before = Utc::now();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let changed = replace_user_roles(&mut conn, user_id, &["writer"])
            .await
            .expect("reduced role set persists");
        assert!(changed, "removing a role is a change the caller must see");
        advance_user_epoch_to_second(&mut conn, user_id)
            .await
            .expect("epoch advances");
        conn.commit().await.expect("role change commits");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let epoch = user_revocation_epoch(&mut conn, user_id)
            .await
            .expect("epoch lookup runs")
            .expect("a changed login leaves an epoch");
        assert!(
            epoch <= Utc::now(),
            "the epoch never lands in the future: {epoch}"
        );
        assert!(
            epoch >= before - chrono::Duration::seconds(1),
            "the epoch advanced to this login rather than staying at an older value: {epoch}"
        );
        // The successor's `iat` is whole seconds, so an epoch carrying
        // sub-second precision would refuse the token the login just minted.
        assert_eq!(
            epoch.timestamp_subsec_nanos(),
            0,
            "the epoch is truncated to the second the successor can carry"
        );
    }

    /// Re-asserting the same roles must not sign the human out elsewhere.
    ///
    /// # Panics
    /// Panics when an unchanged login reports a change or when it moves the
    /// authorization epoch.
    #[tokio::test]
    async fn an_unchanged_login_reports_no_change_and_leaves_the_epoch_alone() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let user_id = seed_user_with_roles(&fixture).await;

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let changed = replace_user_roles(&mut conn, user_id, &["runtime_admin", "writer"])
            .await
            .expect("identical role set persists");
        conn.commit().await.expect("unchanged login commits");
        assert!(
            !changed,
            "an identical role set is not a change and must not retire live sessions"
        );

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let epoch = user_revocation_epoch(&mut conn, user_id)
            .await
            .expect("epoch lookup runs");
        assert!(
            epoch.is_none(),
            "an unchanged login leaves the authorization epoch untouched, got: {epoch:?}"
        );
    }
}
