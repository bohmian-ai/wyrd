//! Human logins for OAuth clients that are not the browser: the RFC 8628
//! device authorization grant with Wyrd as the authorization server, and
//! RFC 7009 refresh-token revocation at logout.
//!
//! `wyrd auth login` cannot receive the provider redirect itself.
//! [`CliLogins::authorize`] records a device authorization: a random device
//! code the CLI keeps and polls with, stored only as its SHA-256, and a short
//! user code the person confirms. On the verification page the person
//! approves that code ([`CliLogins::approve`]), which begins an ordinary
//! tenant login bound to the device id, or denies it ([`CliLogins::deny`]).
//! The common callback records only the approval — the signed-in principal
//! and the connection revision — on the device authorization; the browser
//! receives only a static page. The CLI's token poll ([`CliLogins::redeem`])
//! mints the session from that approval and deletes the device authorization
//! in one tenant transaction, so the credential exists once and only for the
//! device code holder. [`CliLogins::revoke`] revokes a login's refresh chain
//! at logout for either client.

use std::time::Duration;

use base64::Engine as _;
use rand::{Rng as _, RngCore as _};
use serde_json::json;
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    AbsoluteUrl, DeviceAuthorization, LoginInitiation, OAuthClientId, PrincipalId,
    PrincipalKindTag, SecretBearer, Sha256Hex, TokenResponse,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::queries::auth::{
    delete_device_authorization, deny_device_authorization, insert_device_authorization,
    lock_refresh_family, pending_device_authorization, poll_device_authorization, refresh_by_hash,
    revoke_refresh_chain,
};
use wyrd_sql::row_types::auth::HumanSessionBinding;

use crate::audit::{auth_event, auth_failure_code, principal_event, principal_kind_tag};
use crate::connections::HumanConnections;
use crate::error::store_error;
use crate::issuance::TenantTokenIssuer;
use crate::login::login_unavailable;
use crate::refresh::tenant_from_refresh_jwt;
use wyrd_auth_issue::hash_secret;

/// Operation for a device-code token request that issued, or was refused, a
/// login's credential.
pub const DEVICE_CODE_GRANT_OPERATION: &str = "auth.device_code.grant";

/// Operation for a logout that revoked one login's refresh chain (RFC 7009).
pub const TOKEN_REVOCATION_OPERATION: &str = "auth.token.revoke";

/// Path of the verification page the person approves a user code on.
pub const DEVICE_VERIFICATION_PATH: &str = "/auth/device";

/// Lifetime of a device code: the person must approve it and sign in, and the
/// CLI must redeem the login, within this window. The table refuses longer.
const DEVICE_CODE_TTL: Duration = Duration::from_mins(10);

/// Minimum time between two token polls of one device code (RFC 8628 §3.5).
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// User-code alphabet: 20 consonants without look-alikes, so a code never
/// spells a word or confuses `0`/`O` (RFC 8628 §6.1).
const USER_CODE_ALPHABET: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";

/// Letters in a user code, shown as two groups of four.
const USER_CODE_LEN: usize = 8;

/// Owner of device logins and logout revocation: device authorization,
/// approval, denial, redemption, and refresh-chain revocation.
///
/// Composed per request from the server's human-connection owner, which
/// carries the runtime store and public origin, and the tenant issuance owner
/// that mints a redeemed device login's session.
#[derive(Clone)]
pub struct CliLogins {
    /// Tenant human-connection owner logins begin through.
    connections: HumanConnections,
    /// Tenant issuance owner that mints the session at redemption.
    issuer: TenantTokenIssuer,
}

impl std::fmt::Debug for CliLogins {
    /// Render only the type name; the owner holds the sealing keyring.
    ///
    /// # Errors
    /// Returns the formatter's error when writing fails.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliLogins").finish_non_exhaustive()
    }
}

impl CliLogins {
    /// Build the owner over the human-connection owner and issuance owner.
    #[must_use]
    pub fn new(connections: HumanConnections, issuer: TenantTokenIssuer) -> Self {
        Self {
            connections,
            issuer,
        }
    }

    /// Begin a device login at `tenant_route_key` (RFC 8628 §3.1).
    ///
    /// Resolves the tenant and requires its Active connection (an unknown
    /// tenant and one without a connection get the same generic refusal as
    /// the authorization endpoint), then commits a device authorization holding the
    /// SHA-256 of a fresh device code and a random user code, with a
    /// ten-minute `PostgreSQL`-derived expiry. The device code is
    /// `{tenant_id}.{256-bit secret}`, so a token poll routes itself to its
    /// tenant the way a refresh token does; it is returned once and never
    /// stored.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] without a public origin,
    /// [`WyrdError::InvalidToken`] when the route key names no active
    /// tenant or the tenant has no Active connection, and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails, including
    /// the negligible chance of a user code already live in the tenant.
    pub async fn authorize(
        &self,
        tenant_route_key: &TenantSlug,
    ) -> Result<DeviceAuthorization, WyrdError> {
        let origin = self
            .connections
            .require_callback()?
            .origin()
            .ascii_serialization();
        let tenant = self
            .tenant(tenant_route_key)
            .await?
            .ok_or_else(login_unavailable)?;
        self.connections
            .active_connection(tenant)
            .await?
            .ok_or_else(login_unavailable)?;
        let device_code = format!("{tenant}.{}", new_device_secret());
        let user_code = new_user_code();
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        insert_device_authorization(
            &mut conn,
            Uuid::now_v7(),
            &Sha256Hex::digest(device_code.as_bytes()),
            &user_code,
            DEVICE_CODE_TTL,
        )
        .await
        .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let verification_uri = format!(
            "{origin}{DEVICE_VERIFICATION_PATH}?tenant={}",
            tenant_route_key.as_str()
        );
        let verification_uri_complete = format!("{verification_uri}&user_code={user_code}");
        Ok(DeviceAuthorization {
            device_code: SecretBearer::new(device_code),
            user_code,
            verification_uri: absolute(verification_uri)?,
            verification_uri_complete: absolute(verification_uri_complete)?,
            expires_in: DEVICE_CODE_TTL.as_secs(),
            interval: POLL_INTERVAL.as_secs(),
        })
    }

    /// Approve `user_code` at `tenant_route_key` and return the provider URL
    /// the person signs in at.
    ///
    /// Finds the tenant's unexpired, undenied device authorization with that
    /// user code, then begins the tenant login bound to its device id
    /// ([`HumanConnections::begin_bound`]). The sign-in completes the
    /// approval: until the callback has recorded it, a token poll stays
    /// pending. The unique binding index lets one device authorization
    /// begin at most one login.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] with reason `user_code_unavailable`
    /// for an unknown tenant or a malformed, unknown, expired, or denied user
    /// code, all indistinguishable; the refusals of
    /// [`HumanConnections::begin_bound`]; and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn approve(
        &self,
        tenant_route_key: &TenantSlug,
        user_code: &str,
    ) -> Result<AbsoluteUrl, WyrdError> {
        let user_code = normalize_user_code(user_code).ok_or_else(user_code_unavailable)?;
        let tenant = self
            .tenant(tenant_route_key)
            .await?
            .ok_or_else(user_code_unavailable)?;
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let device_id = pending_device_authorization(&mut conn, &user_code)
            .await
            .map_err(store_error)?
            .ok_or_else(user_code_unavailable)?;
        conn.commit().await.map_err(store_error)?;
        self.connections
            .begin_bound(tenant_route_key, LoginInitiation::Device(device_id))
            .await
    }

    /// Deny `user_code` at `tenant_route_key`, so the CLI's next poll ends
    /// the login with `access_denied`. An approved code can no longer be
    /// denied.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] with reason `user_code_unavailable`
    /// for an unknown tenant or a malformed, unknown, or expired user code,
    /// and [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn deny(
        &self,
        tenant_route_key: &TenantSlug,
        user_code: &str,
    ) -> Result<(), WyrdError> {
        let user_code = normalize_user_code(user_code).ok_or_else(user_code_unavailable)?;
        let tenant = self
            .tenant(tenant_route_key)
            .await?
            .ok_or_else(user_code_unavailable)?;
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        if !deny_device_authorization(&mut conn, &user_code)
            .await
            .map_err(store_error)?
        {
            return Err(user_code_unavailable());
        }
        conn.commit().await.map_err(store_error)
    }

    /// Redeem `device_code` for the Wyrd user session (RFC 8628 §3.4–3.5).
    ///
    /// The device code's tenant prefix only routes the request; the code hash
    /// under tenant RLS is the authority. One tenant transaction locks the
    /// device authorization and records the poll. An expired or denied one is
    /// deleted and refused; a poll within the interval of the previous one is
    /// told to slow down; until the callback has recorded an approval the
    /// poll is pending. Once it has, the poll deletes the device
    /// authorization, mints the `wyrd-cli` session for the approving
    /// principal through the connection revision it signed in with
    /// ([`TenantTokenIssuer::issue_human_session`], with its canonical
    /// token-exchange audit), appends one allowed `auth.device_code.grant`
    /// audit event for the User, and commits, so a second poll finds nothing
    /// and the session is issued exactly once.
    ///
    /// # Errors
    /// Returns [`WyrdError::DeviceAuthorization`] with `details.error`
    /// `invalid_grant` for a malformed, unknown, or already redeemed code,
    /// `expired_token`, `access_denied`, `slow_down`, or
    /// `authorization_pending`; the issuance errors, including a connection
    /// that is no longer Active or a suspended User, which leave the approval
    /// in place until it expires; [`WyrdError::AuthVerifyUnavailable`] when
    /// the store fails. Every refusal that ends a known device code is staged
    /// on the audit stage.
    pub async fn redeem(
        &self,
        device_code: &SecretBearer,
        request_id: &str,
    ) -> Result<TokenResponse, WyrdError> {
        let tenant = device_code
            .expose()
            .split_once('.')
            .and_then(|(tenant, _)| tenant.parse::<DataTenantId>().ok())
            .ok_or_else(|| device_error("invalid_grant", "the device code is not valid"))?;
        let result = self.redeem_in(tenant, device_code, request_id).await;
        if let Err(error) = &result
            && audits_refusal(error)
        {
            let event = auth_event(
                request_id,
                DEVICE_CODE_GRANT_OPERATION,
                PrincipalId::new(Uuid::nil()),
                PrincipalKindTag::User,
                None,
                AuditOutcome::Denied,
                AuditDetail::AuthFailure {
                    error_code: auth_failure_code(error),
                },
            );
            self.issuer.audit().stage(tenant, event);
        }
        result
    }

    /// The tenant transaction of [`Self::redeem`] once the tenant is routed.
    ///
    /// # Errors
    /// Returns the errors [`Self::redeem`] documents after routing.
    async fn redeem_in(
        &self,
        tenant: DataTenantId,
        device_code: &SecretBearer,
        request_id: &str,
    ) -> Result<TokenResponse, WyrdError> {
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let poll = poll_device_authorization(
            &mut conn,
            &Sha256Hex::digest(device_code.expose().as_bytes()),
            POLL_INTERVAL,
        )
        .await
        .map_err(store_error)?
        .ok_or_else(|| device_error("invalid_grant", "the device code is not valid"))?;
        let refusal = if poll.expired {
            Some(device_error(
                "expired_token",
                "the device code expired; start a new login",
            ))
        } else if poll.denied {
            Some(device_error("access_denied", "the login was denied"))
        } else {
            None
        };
        if let Some(refusal) = refusal {
            delete_device_authorization(&mut conn, poll.device_id)
                .await
                .map_err(store_error)?;
            conn.commit().await.map_err(store_error)?;
            return Err(refusal);
        }
        if poll.too_fast {
            conn.commit().await.map_err(store_error)?;
            return Err(device_error("slow_down", "poll less often"));
        }
        let Some((principal_id, connection)) = poll.approval() else {
            conn.commit().await.map_err(store_error)?;
            return Err(device_error(
                "authorization_pending",
                "the person has not approved this device code yet",
            ));
        };
        delete_device_authorization(&mut conn, poll.device_id)
            .await
            .map_err(store_error)?;
        let session = self
            .issuer
            .issue_human_session(
                &mut conn,
                principal_id,
                None,
                HumanSessionBinding {
                    connection,
                    client: OAuthClientId::WyrdCli,
                },
                request_id,
            )
            .await?;
        let event = principal_event(
            request_id,
            DEVICE_CODE_GRANT_OPERATION,
            PrincipalId::new(principal_id),
            PrincipalKindTag::User,
            None,
            AuditOutcome::Allowed,
        );
        conn.commit().await.map_err(store_error)?;
        self.issuer.audit().stage(tenant, event);
        Ok(session.into_response())
    }

    /// Revoke the login `refresh_token` belongs to for the authenticated
    /// `client` (RFC 7009 §2.1). Idempotent.
    ///
    /// The token's unverified tenant claim only routes the request; the hash
    /// lookup under tenant RLS is the authority. A token issued to another
    /// client is refused. Under the User's refresh-family lock, the presented
    /// row and every row rotated from it are revoked, so the presented token
    /// and any successor stop renewing, while the User's other logins stay
    /// valid. One allowed `auth.token.revoke` audit event naming the row's
    /// principal, and no token, is staged on the audit stage under
    /// `request_id` once the revocation commits. A malformed or unknown token revokes and records nothing (RFC
    /// 7009 §2.2); an already revoked one revokes nothing but is still
    /// recorded.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with reason `token_client_mismatch`
    /// for a token issued to another client,
    /// and [`WyrdError::AuthVerifyUnavailable`] when the store fails; nothing
    /// is committed then.
    pub async fn revoke(
        &self,
        refresh_token: &SecretBearer,
        client: OAuthClientId,
        request_id: &str,
    ) -> Result<(), WyrdError> {
        let Ok(tenant) = tenant_from_refresh_jwt(refresh_token.expose()) else {
            return Ok(());
        };
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let Some(row) = refresh_by_hash(&mut conn, &hash_secret(refresh_token.expose()))
            .await
            .map_err(store_error)?
        else {
            return Ok(());
        };
        if row
            .human_session()
            .is_none_or(|session| session.client != client)
        {
            return Err(WyrdError::Validation {
                message: "the token was not issued to this client".to_owned(),
                details: json!({ "reason": "token_client_mismatch" }),
            });
        }
        lock_refresh_family(&mut conn, &row.principal_kind, row.principal_id)
            .await
            .map_err(store_error)?;
        revoke_refresh_chain(&mut conn, row.id, "logout")
            .await
            .map_err(store_error)?;
        let event = principal_event(
            request_id,
            TOKEN_REVOCATION_OPERATION,
            PrincipalId::new(row.principal_id),
            principal_kind_tag(&row.principal_kind),
            None,
            AuditOutcome::Allowed,
        );
        conn.commit().await.map_err(store_error)?;
        self.issuer.audit().stage(tenant, event);
        Ok(())
    }

    /// The tenant `route_key` names, if it is an active tenant.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    async fn tenant(&self, route_key: &TenantSlug) -> Result<Option<DataTenantId>, WyrdError> {
        self.connections
            .postgres()
            .resolve_tenant_slug(route_key)
            .await
            .map_err(store_error)
    }
}

/// A fresh unguessable device-code secret: 32 random bytes, base64url
/// without padding (RFC 8628 §5.2).
fn new_device_secret() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A fresh random user code, `XXXX-XXXX` over [`USER_CODE_ALPHABET`].
fn new_user_code() -> String {
    let mut rng = rand::rng();
    let letters: String = (0..USER_CODE_LEN)
        .map(|_| char::from(USER_CODE_ALPHABET[rng.random_range(0..USER_CODE_ALPHABET.len())]))
        .collect();
    format!("{}-{}", &letters[..4], &letters[4..])
}

/// The stored form of a user code a person typed: case and separators are
/// ignored. `None` when it cannot be a user code.
fn normalize_user_code(input: &str) -> Option<String> {
    let letters: String = input
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    (letters.len() == USER_CODE_LEN && letters.bytes().all(|b| USER_CODE_ALPHABET.contains(&b)))
        .then(|| format!("{}-{}", &letters[..4], &letters[4..]))
}

/// Whether a refused token poll is recorded: everything except an unknown
/// code and the two keep-polling answers, which decide nothing.
fn audits_refusal(error: &WyrdError) -> bool {
    let WyrdError::DeviceAuthorization { details, .. } = error else {
        return true;
    };
    !matches!(
        details.get("error").and_then(serde_json::Value::as_str),
        Some("invalid_grant" | "authorization_pending" | "slow_down")
    )
}

/// A device-code token refusal carrying the RFC 8628 `error`.
fn device_error(error: &'static str, message: &str) -> WyrdError {
    WyrdError::DeviceAuthorization {
        message: message.to_owned(),
        details: json!({ "error": error }),
    }
}

/// The one refusal for a verification-page request that names no live user
/// code.
fn user_code_unavailable() -> WyrdError {
    WyrdError::InvalidState {
        message: "the code is unknown or expired; check the code your terminal shows, or start a \
                  new login"
            .to_owned(),
        details: json!({ "reason": "user_code_unavailable" }),
    }
}

/// A verification URL built from the configured public origin.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when it is not an absolute URL.
fn absolute(url: String) -> Result<AbsoluteUrl, WyrdError> {
    AbsoluteUrl::new(url).map_err(|_| WyrdError::Internal {
        message: "the verification URL is invalid".to_owned(),
        details: json!({}),
    })
}

/// Device authorization, approval, denial, polling, and logout revocation
/// against a real
/// tenant store and a mock provider.
#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::{Duration, Utc};
    use secrecy::SecretString;
    use url::Url;
    use uuid::Uuid;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_oidc::MappedClaims;
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_spec::auth::{
        DeviceAuthorization, LoginInitiation, OAuthClientId, PrincipalId, PrincipalKindTag,
        SecretBearer, Sha256Hex,
    };
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::TenantSlug;
    use wyrd_sql::queries::auth::{
        LoginState, approve_device_authorization, consume_login_state, deny_device_authorization,
        insert_human_refresh_token, insert_login_state, pending_device_authorization,
        refresh_by_hash,
    };
    use wyrd_sql::row_types::auth::HumanSessionBinding;

    use super::{CliLogins, TOKEN_REVOCATION_OPERATION};
    use crate::audit::test_audit::RecordedAudit;
    use crate::callback::{AuthorizationCodeExchange, LoginCompletion};
    use crate::connections::HumanConnections;
    use crate::issuance::{TenantTokenIssuer, TokenExchangeSettings};
    use wyrd_auth_issue::hash_secret;

    /// A CLI login owner over `fixture` whose tenant's Active connection
    /// points at `provider`, a mock discovery document, staging its audit on
    /// `audit`.
    ///
    /// # Panics
    /// Panics when the fixture cannot be seeded.
    async fn owner(
        fixture: &PgFixture,
        provider: &MockServer,
        audit: &Arc<RecordedAudit>,
    ) -> CliLogins {
        let issuer = provider.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "response_types_supported": ["code"],
                "subject_types_supported": ["public"],
                "id_token_signing_alg_values_supported": ["EdDSA"],
            })))
            .mount(provider)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "keys": [] })),
            )
            .mount(provider)
            .await;
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        conn.commit().await.expect("seed commits");
        sqlx::query(
            "UPDATE wyrd.auth_human_connections SET issuer_url = $1 WHERE connection_id = $2",
        )
        .bind(&issuer)
        .bind(binding.connection_id)
        .execute(&fixture.superuser_pool().expect("superuser pool"))
        .await
        .expect("connection points at the mock provider");
        let origin = Url::parse("https://wyrd.example.com").expect("origin parses");
        CliLogins::new(
            HumanConnections::new(
                fixture.wyrd_postgres().clone(),
                Some(Arc::new(SealingKeyring::new(SecretKey::from_bytes(
                    [5_u8; 32],
                )))),
                ScreenedHttp::allowing_internal(),
                Some(&origin),
                Arc::clone(audit) as _,
            ),
            TenantTokenIssuer::new(
                Arc::new(issuing_key()),
                TokenExchangeSettings::default(),
                Arc::clone(audit) as _,
            ),
        )
    }

    /// The test signing key.
    ///
    /// # Panics
    /// Panics when the fixed key does not load.
    fn issuing_key() -> IssuingKey {
        IssuingKey::from_ed_pem(
            SecretString::from(
                "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n",
            ),
            Kid::new("k1").expect("kid"),
            "wyrd",
        )
        .expect("key loads")
    }

    /// A verifier for tokens [`issuing_key`] signs.
    ///
    /// # Panics
    /// Panics when the verifying key does not encode.
    fn verifier() -> TokenVerifier {
        let pem = issuing_key().verifying_key_pem().expect("key encodes");
        let key = public_key_from_pem(pem.as_bytes()).expect("key decodes");
        let keys = HashMap::from([(Kid::new("k1").expect("kid"), Arc::new(key))]);
        TokenVerifier::new(keys, "wyrd", WyrdAuthVerifySettings::default())
    }

    /// The stable `details.reason` of a state refusal.
    fn reason(error: &WyrdError) -> Option<&str> {
        let WyrdError::InvalidState { details, .. } = error else {
            return None;
        };
        details.get("reason").and_then(serde_json::Value::as_str)
    }

    /// The RFC 8628 `error` of a device-code refusal.
    fn device_error(result: Result<impl std::fmt::Debug, WyrdError>) -> String {
        match result {
            Err(WyrdError::DeviceAuthorization { details, .. }) => details["error"]
                .as_str()
                .expect("error is a string")
                .to_owned(),
            other => panic!("expected a device-code refusal, got {other:?}"),
        }
    }

    /// Number of login-state rows in the fixture tenant.
    ///
    /// # Panics
    /// Panics when the count query fails.
    async fn state_rows(fixture: &PgFixture) -> i64 {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.auth_login_state")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("count runs")
    }

    /// A device authorization keeps its device code out of the verification
    /// URLs. Its token poll is pending until the login completes and told to
    /// slow down when it polls within the interval; a wrong code is an
    /// invalid grant. The user code is approved case- and separator-
    /// insensitively and begins exactly one login, while an unknown user code
    /// or tenant gets the one refusal. A denied code ends the login and its
    /// bound login state at the next poll, and an expired code is refused
    /// once; afterwards both are invalid grants.
    ///
    /// # Panics
    /// Panics when any step is accepted or refused differently.
    #[tokio::test]
    async fn device_codes_poll_approve_deny_and_expire() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let audit = RecordedAudit::new();
        let logins = owner(&fixture, &provider, &audit).await;
        let slug = TenantSlug::new(fixture.tenant_slug()).expect("slug");

        let device = logins.authorize(&slug).await.expect("authorizes");
        let code = device.device_code.expose();
        assert!(!device.verification_uri_complete.as_str().contains(code));
        assert!(
            device
                .verification_uri_complete
                .as_str()
                .ends_with(&format!("user_code={}", device.user_code))
        );
        assert_eq!((device.expires_in, device.interval), (600, 5));
        assert_eq!(
            device_error(logins.redeem(&device.device_code, "req").await),
            "authorization_pending"
        );
        assert_eq!(
            device_error(logins.redeem(&device.device_code, "req").await),
            "slow_down"
        );
        let wrong = SecretBearer::new(format!("{}.wrong", fixture.data_tenant_id()));
        for code in [wrong, SecretBearer::new("not-a-code".to_owned())] {
            assert_eq!(
                device_error(logins.redeem(&code, "req").await),
                "invalid_grant"
            );
        }

        let other = TenantSlug::new("no-such-tenant").expect("slug");
        for (tenant, user_code) in [
            (&slug, "BCDF-GHJK"),
            (&slug, "nope"),
            (&other, &*device.user_code),
        ] {
            let error = logins
                .approve(tenant, user_code)
                .await
                .expect_err("refused");
            assert_eq!(reason(&error), Some("user_code_unavailable"));
        }
        let typed = device.user_code.replace('-', " ").to_lowercase();
        let url = logins.approve(&slug, &typed).await.expect("approves");
        assert!(url.as_str().starts_with(&provider.uri()));
        let error = logins
            .approve(&slug, &device.user_code)
            .await
            .expect_err("one login per device code");
        assert_eq!(reason(&error), Some("login_binding_reused"));
        assert_eq!(state_rows(&fixture).await, 1);

        logins.deny(&slug, &device.user_code).await.expect("denies");
        assert_eq!(
            device_error(logins.redeem(&device.device_code, "req").await),
            "access_denied"
        );
        assert_eq!(state_rows(&fixture).await, 0, "denial ends the bound login");
        assert_eq!(
            device_error(logins.redeem(&device.device_code, "req").await),
            "invalid_grant"
        );

        let expiring = logins.authorize(&slug).await.expect("authorizes");
        sqlx::query(
            "UPDATE wyrd.auth_device_authorizations
                SET created_at = statement_timestamp() - interval '11 minutes',
                    expires_at = statement_timestamp() - interval '1 minute'",
        )
        .execute(&fixture.superuser_pool().expect("superuser pool"))
        .await
        .expect("expires the code");
        assert_eq!(
            device_error(logins.redeem(&expiring.device_code, "req").await),
            "expired_token"
        );
        assert_eq!(
            device_error(logins.redeem(&expiring.device_code, "req").await),
            "invalid_grant"
        );
        let error = logins
            .deny(&slug, &expiring.user_code)
            .await
            .expect_err("a removed code cannot be denied");
        assert_eq!(reason(&error), Some("user_code_unavailable"));
    }

    /// The device code of an approved device authorization is redeemed for
    /// a `wyrd-cli` session exactly once: the approval stores no token, the
    /// first poll after it mints the session for the approving User with one
    /// refresh row, and every later poll is an invalid grant. An approved
    /// code can no longer be denied, and a denied code can no longer be
    /// approved, so a sign-in that finishes after a denial records nothing.
    ///
    /// # Panics
    /// Panics when any step is accepted or refused differently.
    #[tokio::test]
    async fn an_approved_device_code_issues_exactly_once() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let audit = RecordedAudit::new();
        let logins = owner(&fixture, &provider, &audit).await;
        let slug = TenantSlug::new(fixture.tenant_slug()).expect("slug");
        let tenant = fixture.data_tenant_id();
        let approved = logins.authorize(&slug).await.expect("authorizes");
        let denied = logins.authorize(&slug).await.expect("authorizes");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
             VALUES ($1, $2, 'device@example.com', 'oidc', 'active')",
        )
        .bind(user)
        .bind(tenant.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("user inserts");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection reads");
        let approved_id = pending_device_authorization(&mut conn, &approved.user_code)
            .await
            .expect("lookup runs")
            .expect("code is pending");
        let denied_id = pending_device_authorization(&mut conn, &denied.user_code)
            .await
            .expect("lookup runs")
            .expect("code is pending");
        assert!(
            approve_device_authorization(&mut conn, approved_id, user, binding)
                .await
                .expect("approval runs")
        );
        assert!(
            !deny_device_authorization(&mut conn, &approved.user_code)
                .await
                .expect("denial runs"),
            "an approved code cannot be denied"
        );
        assert!(
            deny_device_authorization(&mut conn, &denied.user_code)
                .await
                .expect("denial runs")
        );
        assert!(
            !approve_device_authorization(&mut conn, denied_id, user, binding)
                .await
                .expect("approval runs"),
            "a denied code cannot be approved"
        );
        conn.commit().await.expect("approval commits");
        let refresh_rows = || async {
            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM wyrd.auth_refresh_tokens WHERE principal_id = $1",
            )
            .bind(user)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("count runs")
        };
        assert_eq!(refresh_rows().await, 0, "the approval minted nothing");

        let session = logins
            .redeem(&approved.device_code, "req-redeem")
            .await
            .expect("the approved code redeems");
        assert!(session.refresh_token.is_some());
        let claims = verifier()
            .verify(
                &SecretString::from(session.access_token.expose().to_owned()),
                &tenant,
            )
            .expect("the access token verifies");
        assert_eq!(claims.principal.id, PrincipalId::new(user));
        assert_eq!(refresh_rows().await, 1);
        assert_eq!(
            device_error(logins.redeem(&approved.device_code, "req-again").await),
            "invalid_grant"
        );
        assert_eq!(
            device_error(logins.redeem(&denied.device_code, "req-denied").await),
            "access_denied"
        );
        assert_eq!(refresh_rows().await, 1, "the session was issued once");
    }

    /// How another actor ends a device grant while its approval is in flight.
    #[derive(Debug, Clone, Copy)]
    enum Termination {
        /// The person denies the user code.
        Deny,
        /// The code expires and the CLI's next poll deletes it.
        ExpireAndPoll,
    }

    /// Approve a fresh device code with [`CliLogins::approve`] parked after
    /// its live-row lookup and before its bound login-state insert, end the
    /// grant by `termination` meanwhile, then complete the provider sign-in
    /// for the login the approval began.
    ///
    /// A gate transaction holds an uncommitted login state bound to the same
    /// device id, so the approval's insert waits on the device binding's
    /// unique index; the wait is observed through `pg_blocking_pids`, not
    /// timing, and rolling the gate back releases the approval. Returns the
    /// device authorization and the provider completion's result.
    ///
    /// # Panics
    /// Panics when a step outside the race fails or the approval never parks.
    async fn approval_racing(
        fixture: &PgFixture,
        logins: &CliLogins,
        termination: Termination,
    ) -> (DeviceAuthorization, Result<LoginCompletion, WyrdError>) {
        let slug = TenantSlug::new(fixture.tenant_slug()).expect("slug");
        let device = logins.authorize(&slug).await.expect("authorizes");
        let active = logins
            .connections
            .active_connection(fixture.data_tenant_id())
            .await
            .expect("connection reads")
            .expect("connection is active");
        let mut gate = fixture.tenant_conn().await.expect("gate conn opens");
        let device_id = pending_device_authorization(&mut gate, &device.user_code)
            .await
            .expect("lookup runs")
            .expect("code is pending");
        let gate_login = LoginState {
            connection: active.binding,
            issuer: "https://gate.example.com".to_owned(),
            client_id: "gate".to_owned(),
            redirect_uri: "https://gate.example.com/callback".to_owned(),
            code_verifier: SecretString::from("gate"),
            nonce: "gate".to_owned(),
            initiation: LoginInitiation::Device(device_id),
        };
        let gated = insert_login_state(
            &mut gate,
            &Sha256Hex::digest(b"gate"),
            &gate_login,
            std::time::Duration::from_mins(5),
        )
        .await
        .expect("gate state inserts");
        assert!(gated, "the gate holds the device binding");
        let gate_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut **gate.transaction())
            .await
            .expect("gate pid reads");

        let (approval, ()) = tokio::join!(logins.approve(&slug, &device.user_code), async {
            wait_for_blocked_by(fixture, gate_pid).await;
            match termination {
                Termination::Deny => logins.deny(&slug, &device.user_code).await.expect("denies"),
                Termination::ExpireAndPoll => {
                    sqlx::query(
                        "UPDATE wyrd.auth_device_authorizations
                            SET created_at = statement_timestamp() - interval '11 minutes',
                                expires_at = statement_timestamp() - interval '1 minute'
                          WHERE device_id = $1",
                    )
                    .bind(device_id)
                    .execute(&fixture.superuser_pool().expect("superuser pool"))
                    .await
                    .expect("expires the code");
                    assert_eq!(
                        device_error(logins.redeem(&device.device_code, "req-expire").await),
                        "expired_token"
                    );
                }
            }
            gate.rollback().await.expect("the gate releases");
        });
        let url = approval.expect("the parked approval begins its login");
        let state = Url::parse(url.as_str())
            .expect("provider URL parses")
            .query_pairs()
            .find(|(name, _)| name == "state")
            .map(|(_, value)| value.into_owned())
            .expect("provider URL carries state");
        let state_hash = Sha256Hex::digest(state.as_bytes());
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let login = consume_login_state(&mut conn, &state_hash)
            .await
            .expect("state consumes")
            .expect("the approval's login is pending");
        conn.commit().await.expect("consumption commits");
        let completion = AuthorizationCodeExchange {
            issuer: logins.issuer.clone(),
            connections: logins.connections.clone(),
        }
        .finish_id_token_exchange(
            &state_hash,
            &active.trusted,
            &login,
            &MappedClaims {
                subject: "device-racer".to_owned(),
                email: None,
                groups: Vec::new(),
            },
            "req-race",
        )
        .await;
        (device, completion)
    }

    /// Wait until some backend is blocked by the backend `pid`.
    ///
    /// # Panics
    /// Panics when lock state cannot be read or no waiter appears within 30s.
    async fn wait_for_blocked_by(fixture: &PgFixture, pid: i32) {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let blocked: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                                     WHERE $1 = ANY(pg_blocking_pids(pid)))",
                )
                .bind(pid)
                .fetch_one(fixture.app_pool())
                .await
                .expect("lock state reads");
                if blocked {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the approval parks behind the gate");
    }

    /// Assert the raced sign-in left no User, session, refresh row,
    /// authorization code, or device approval behind, and staged no login
    /// audit on `audit`.
    ///
    /// # Panics
    /// Panics when anything persisted or a login was audited.
    async fn assert_no_device_authority(fixture: &PgFixture, audit: &RecordedAudit) {
        assert!(
            audit.operation("auth.login").is_empty(),
            "the raced sign-in staged no login audit"
        );
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM wyrd.auth_users),
                    (SELECT count(*) FROM wyrd.auth_refresh_tokens),
                    (SELECT count(*) FROM wyrd.auth_login_state WHERE code_hash IS NOT NULL),
                    (SELECT count(*) FROM wyrd.auth_device_authorizations
                      WHERE principal_id IS NOT NULL)",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("authority counts run");
        assert_eq!(counts, (0, 0, 0, 0), "the raced sign-in left authority");
    }

    /// A denial that lands while an approval is between its live-row lookup
    /// and its login-state insert wins: the approval's sign-in is refused,
    /// records no approval or credential, and the CLI's poll ends the login
    /// with `access_denied`, then finds nothing.
    ///
    /// # Panics
    /// Panics when the sign-in records anything or a poll issues a token.
    #[tokio::test]
    async fn a_denial_during_approval_wins() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let audit = RecordedAudit::new();
        let logins = owner(&fixture, &provider, &audit).await;
        let (device, completion) = approval_racing(&fixture, &logins, Termination::Deny).await;

        assert!(
            matches!(completion, Err(WyrdError::InvalidState { .. })),
            "{completion:?}"
        );
        assert_no_device_authority(&fixture, &audit).await;
        assert_eq!(
            device_error(logins.redeem(&device.device_code, "req-denied").await),
            "access_denied"
        );
        assert_eq!(
            device_error(logins.redeem(&device.device_code, "req-again").await),
            "invalid_grant"
        );
        assert_no_device_authority(&fixture, &audit).await;
    }

    /// An expiry whose poll deletes the device authorization while an
    /// approval is between its live-row lookup and its login-state insert
    /// wins: the approval's sign-in is refused, records no approval or
    /// credential, and every later poll is an invalid grant.
    ///
    /// # Panics
    /// Panics when the sign-in records anything or a poll issues a token.
    #[tokio::test]
    async fn an_expiry_deleted_during_approval_wins() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let audit = RecordedAudit::new();
        let logins = owner(&fixture, &provider, &audit).await;
        let (device, completion) =
            approval_racing(&fixture, &logins, Termination::ExpireAndPoll).await;

        assert!(
            matches!(completion, Err(WyrdError::InvalidState { .. })),
            "{completion:?}"
        );
        assert_no_device_authority(&fixture, &audit).await;
        assert_eq!(
            device_error(logins.redeem(&device.device_code, "req-after").await),
            "invalid_grant"
        );
        assert_no_device_authority(&fixture, &audit).await;
    }

    /// Seed one User with two CLI logins: a stale refresh token and its live
    /// successor (one chain), plus an unrelated second chain.
    ///
    /// Returns the User id and the three refresh JWTs in that order.
    ///
    /// # Panics
    /// Panics when a seed row cannot be written.
    async fn seed_two_cli_logins(fixture: &PgFixture) -> (Uuid, Vec<String>) {
        let tenant = fixture.data_tenant_id();
        let key = issuing_key();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let user = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
             VALUES ($1, $2, 'cli@example.com', 'oidc', 'active')",
        )
        .bind(user)
        .bind(tenant.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("user inserts");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection reads");
        let mut tokens = Vec::new();
        let mut ids = Vec::new();
        for rotated_from in [None, Some(0), None] {
            let jwt = key
                .issue_refresh_token(
                    PrincipalKindTag::User,
                    PrincipalId::new(user),
                    tenant,
                    Utc::now(),
                    Duration::days(1),
                )
                .expect("refresh issues");
            let id = Uuid::now_v7();
            insert_human_refresh_token(
                &mut conn,
                id,
                user,
                &hash_secret(&jwt),
                Utc::now() + Duration::days(1),
                rotated_from.map(|index: usize| ids[index]),
                HumanSessionBinding {
                    connection: binding,
                    client: OAuthClientId::WyrdCli,
                },
            )
            .await
            .expect("refresh row inserts");
            tokens.push(jwt);
            ids.push(id);
        }
        conn.commit().await.expect("seed commits");
        (user, tokens)
    }

    /// Logout from a stale refresh token revokes its live successor, while the
    /// same User's other login keeps renewing, and stages exactly one logout
    /// audit event for the User.
    ///
    /// # Panics
    /// Panics when the wrong rows are revoked or the audit differs.
    #[tokio::test]
    async fn logout_revokes_only_its_own_chain() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let audit = RecordedAudit::new();
        let logins = owner(&fixture, &provider, &audit).await;
        let tenant = fixture.data_tenant_id();
        let (user, tokens) = seed_two_cli_logins(&fixture).await;

        let logouts = || {
            audit
                .operation(TOKEN_REVOCATION_OPERATION)
                .iter()
                .filter(|(staged, event)| {
                    *staged == tenant && event.principal_id == PrincipalId::new(user)
                })
                .count()
        };
        logins
            .revoke(
                &SecretBearer::new(tokens[0].clone()),
                OAuthClientId::WyrdCli,
                "req-logout",
            )
            .await
            .expect("the logout revokes");

        logins
            .revoke(
                &SecretBearer::new("not-a-jwt".to_owned()),
                OAuthClientId::WyrdCli,
                "req-noop",
            )
            .await
            .expect("an unusable token is a no-op");
        assert_eq!(logouts(), 1, "exactly one logout is recorded");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let mut revoked = Vec::new();
        for jwt in &tokens {
            let row = refresh_by_hash(&mut conn, &hash_secret(jwt))
                .await
                .expect("lookup")
                .expect("row exists");
            revoked.push(row.revoked_reason);
        }
        assert_eq!(
            revoked,
            vec![Some("logout".to_owned()), Some("logout".to_owned()), None]
        );
    }
}
