//! CLI human login: the one-use browser-to-CLI handoff and refresh-chain
//! revocation.
//!
//! `wyrd auth login` cannot receive the provider redirect itself, so the
//! server mediates. [`CliLogins::begin`] records a handoff that binds a
//! random handoff id to the tenant, its Active connection, and the SHA-256 of
//! a 256-bit verifier only the CLI holds, then begins an ordinary tenant
//! login bound to that handoff id
//! ([`HumanConnections::begin_login`]). The common callback issues the
//! session and stores it sealed against the handoff id, exactly as for a
//! browser login; the browser receives only a static page. The CLI polls
//! [`CliLogins::claim`] with its verifier: the claim that finds the sealed
//! completion redeems it and deletes the handoff in one tenant transaction,
//! so the credential is handed out once and only to the verifier holder.
//! [`CliLogins::end`] revokes a saved login's refresh chain at logout.

use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use serde_json::json;
use uuid::Uuid;
use wyrd_auth_verify::TokenVerifier;
use wyrd_crypt::SealingKeyring;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    AbsoluteUrl, BeginLogin, CliHandoff, CliHandoffClaim, CliLogin, LoginInitiation, PrincipalId,
    PrincipalKindTag, SecretBearer, Sha256Hex, TokenResponse,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::queries::auth::{
    delete_cli_handoff, insert_cli_handoff, lock_cli_handoff, lock_refresh_family,
    redeem_login_completion, refresh_by_hash, revoke_refresh_chain,
};

use crate::audit::{
    append_auth_audit, auth_event, auth_failure_code, principal_event, principal_kind_tag,
    record_auth_audit_best_effort,
};
use crate::connections::HumanConnections;
use crate::error::{auth_error_to_wyrd, store_error};
use crate::exchange_api_key::token_hash;
use crate::login::{login_unavailable, random_b64url};
use crate::refresh::tenant_from_refresh_jwt;

/// Operation for a CLI handoff claim that handed out, or was refused, a
/// login's credential.
pub const CLI_HANDOFF_CLAIM_OPERATION: &str = "auth.cli_handoff.claim";

/// Operation for a CLI logout that revoked one login's refresh chain.
pub const CLI_LOGOUT_OPERATION: &str = "auth.cli_login.logout";

/// Lifetime of a CLI handoff: the person must finish the browser sign-in and
/// the CLI must claim it within this window. The table refuses longer.
const CLI_HANDOFF_TTL: Duration = Duration::from_mins(5);

/// Seconds a pending claim tells the CLI to wait before polling again.
const CLAIM_RETRY_SECONDS: u32 = 2;

/// Owner of CLI logins: handoff begin, claim, cancel, and refresh-chain
/// revocation.
///
/// Composed per request from the server's human-connection owner, which
/// carries the runtime store, sealing keyring, and public origin, and the
/// verifier of the server's own access tokens, which names the principal a
/// claimed credential acts as.
#[derive(Clone)]
pub struct CliLogins {
    /// Tenant human-connection owner logins begin and redeem through.
    connections: HumanConnections,
    /// Verifier for the server's own access tokens.
    verifier: Arc<TokenVerifier>,
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
    /// Build the owner over the human-connection owner and token verifier.
    #[must_use]
    pub fn new(connections: HumanConnections, verifier: Arc<TokenVerifier>) -> Self {
        Self {
            connections,
            verifier,
        }
    }

    /// Begin a CLI login at `tenant_route_key` and return its handoff.
    ///
    /// Resolves the tenant and its Active connection (an unknown tenant and
    /// one without a connection get the same generic refusal as
    /// `POST /auth/login`), commits a handoff row bound to that connection and
    /// to the SHA-256 of a fresh 256-bit verifier with a five-minute
    /// `PostgreSQL`-derived expiry, then begins the tenant login bound to the
    /// handoff id. The raw verifier is returned once and never stored.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when the route key names no active
    /// tenant or the tenant has no Active connection, the refusals of
    /// [`HumanConnections::begin_login`], and
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails. A handoff
    /// whose login could not begin is left to expire unclaimable.
    pub async fn begin(&self, tenant_route_key: &TenantSlug) -> Result<CliHandoff, WyrdError> {
        self.connections.require_callback()?;
        self.connections.require_keyring()?;
        let tenant = self
            .tenant(tenant_route_key)
            .await?
            .ok_or_else(login_unavailable)?;
        let active = self
            .connections
            .active_connection(tenant)
            .await?
            .ok_or_else(login_unavailable)?;
        let handoff_id = Uuid::now_v7();
        let verifier = random_b64url(32);
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let expires_at = insert_cli_handoff(
            &mut conn,
            handoff_id,
            active.binding.connection_id,
            &Sha256Hex::digest(verifier.as_bytes()),
            CLI_HANDOFF_TTL,
        )
        .await
        .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let begun = self
            .connections
            .begin_login(&BeginLogin {
                tenant_route_key: tenant_route_key.clone(),
                browser_flow_hash: None,
                cli_handoff_id: Some(handoff_id),
            })
            .await?;
        Ok(CliHandoff {
            handoff_id,
            login_url: begun.authorization_url,
            poll_verifier: SecretBearer::new(verifier),
            expires_at,
        })
    }

    /// Claim the credential of handoff `handoff_id` with its verifier.
    ///
    /// One tenant transaction locks the unexpired handoff the id and verifier
    /// hash name. While the callback has not completed the login, the claim
    /// commits nothing and answers [`CliHandoffClaim::Pending`]. Once it has,
    /// the claim redeems the sealed completion, deletes the handoff, requires
    /// the login to have gone through the connection the handoff was begun
    /// at, appends one allowed `auth.cli_handoff.claim` audit event for the
    /// User, and commits, so a second claim finds nothing. A completion that
    /// cannot be used is still consumed.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidState`] with reason `cli_handoff_unavailable`
    /// for an unknown tenant, a missing, expired, cancelled, or already
    /// claimed handoff, or a wrong verifier, all indistinguishable;
    /// [`WyrdError::InvalidToken`] when the login went through another
    /// connection; [`WyrdError::Validation`] without a sealing keyring;
    /// [`WyrdError::Internal`] when the completion cannot be opened or carries
    /// no refresh token; [`WyrdError::AuthVerifyUnavailable`] when the store
    /// fails; and [`WyrdError::AuditUnavailable`] when the audit append fails.
    /// Every refusal once the tenant is known is audited best-effort.
    pub async fn claim(
        &self,
        tenant_route_key: &TenantSlug,
        handoff_id: Uuid,
        verifier: &SecretBearer,
        request_id: &str,
    ) -> Result<CliHandoffClaim, WyrdError> {
        let keyring = self.connections.require_keyring()?;
        let tenant = self
            .tenant(tenant_route_key)
            .await?
            .ok_or_else(handoff_unavailable)?;
        let result = self
            .claim_in(tenant, handoff_id, verifier, keyring, request_id)
            .await;
        if let Err(error) = &result {
            let event = auth_event(
                request_id,
                CLI_HANDOFF_CLAIM_OPERATION,
                PrincipalId::new(Uuid::nil()),
                PrincipalKindTag::User,
                None,
                AuditOutcome::Denied,
                AuditDetail::AuthFailure {
                    error_code: auth_failure_code(error),
                },
            );
            record_auth_audit_best_effort(self.connections.postgres(), tenant, &event).await;
        }
        result
    }

    /// The tenant transaction of [`Self::claim`] once the tenant is known.
    ///
    /// # Errors
    /// Returns the errors [`Self::claim`] documents after tenant resolution.
    async fn claim_in(
        &self,
        tenant: DataTenantId,
        handoff_id: Uuid,
        verifier: &SecretBearer,
        keyring: &SealingKeyring,
        request_id: &str,
    ) -> Result<CliHandoffClaim, WyrdError> {
        let verifier_hash = Sha256Hex::digest(verifier.expose().as_bytes());
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let connection_id = lock_cli_handoff(&mut conn, handoff_id, &verifier_hash)
            .await
            .map_err(store_error)?
            .ok_or_else(handoff_unavailable)?;
        let Some(redeemed) = redeem_login_completion(&mut conn, &LoginInitiation::Cli(handoff_id))
            .await
            .map_err(store_error)?
        else {
            conn.commit().await.map_err(store_error)?;
            return Ok(CliHandoffClaim::Pending {
                retry_after_seconds: CLAIM_RETRY_SECONDS,
            });
        };
        delete_cli_handoff(&mut conn, handoff_id, &verifier_hash)
            .await
            .map_err(store_error)?;
        let opened = if redeemed.connection_id == connection_id {
            open_login(keyring, &redeemed.sealed)
        } else {
            Err(WyrdError::InvalidToken {
                message: "the login connection changed while the login was in progress".to_owned(),
                details: json!({}),
            })
        };
        let (token, refresh_token) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                // The completion is single-use even when it is refused.
                conn.commit().await.map_err(store_error)?;
                return Err(error);
            }
        };
        let principal_id = self
            .verifier
            .verify(
                &SecretString::from(token.access_token.expose().to_owned()),
                &tenant,
            )
            .map_err(auth_error_to_wyrd)?
            .principal
            .id;
        let event = principal_event(
            request_id,
            CLI_HANDOFF_CLAIM_OPERATION,
            principal_id,
            PrincipalKindTag::User,
            None,
            AuditOutcome::Allowed,
        );
        append_auth_audit(&mut conn, &event).await?;
        conn.commit().await.map_err(store_error)?;
        Ok(CliHandoffClaim::Complete(CliLogin {
            server_origin: self.server_origin()?,
            tenant_id: tenant,
            principal_id,
            access_token: token.access_token,
            refresh_token,
            access_expires_at: token.expires_at,
        }))
    }

    /// Cancel handoff `handoff_id` with its verifier. Idempotent.
    ///
    /// Deletes the handoff and any login state still bound to it in one tenant
    /// transaction, so a later callback and every later claim find nothing.
    /// An unknown tenant, handoff, or wrong verifier deletes nothing.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails.
    pub async fn cancel(
        &self,
        tenant_route_key: &TenantSlug,
        handoff_id: Uuid,
        verifier: &SecretBearer,
    ) -> Result<(), WyrdError> {
        let Some(tenant) = self.tenant(tenant_route_key).await? else {
            return Ok(());
        };
        let mut conn = self
            .connections
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        delete_cli_handoff(
            &mut conn,
            handoff_id,
            &Sha256Hex::digest(verifier.expose().as_bytes()),
        )
        .await
        .map_err(store_error)?;
        conn.commit().await.map_err(store_error)
    }

    /// End the login `refresh_token` belongs to. Idempotent.
    ///
    /// The token's unverified tenant claim only routes the request; the hash
    /// lookup under tenant RLS is the authority. Under the User's
    /// refresh-family lock, the presented row and every row rotated from it
    /// are revoked, so the presented token and any successor stop renewing,
    /// while the User's other logins stay valid. One allowed
    /// `auth.cli_login.logout` audit event naming the row's principal, and no
    /// token, is appended on that same transaction under `request_id`, so the
    /// revocation commits exactly when its audit row does. A malformed or
    /// unknown token revokes and records nothing (RFC 7009); an already
    /// revoked one revokes nothing but is still recorded.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails and
    /// [`WyrdError::AuditUnavailable`] when the audit append fails; nothing is
    /// committed then.
    pub async fn end(
        &self,
        refresh_token: &SecretBearer,
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
        let Some(row) = refresh_by_hash(&mut conn, &token_hash(refresh_token.expose()))
            .await
            .map_err(store_error)?
        else {
            return Ok(());
        };
        lock_refresh_family(&mut conn, &row.principal_kind, row.principal_id)
            .await
            .map_err(store_error)?;
        revoke_refresh_chain(&mut conn, row.id, "cli_logout")
            .await
            .map_err(store_error)?;
        let event = principal_event(
            request_id,
            CLI_LOGOUT_OPERATION,
            PrincipalId::new(row.principal_id),
            principal_kind_tag(&row.principal_kind),
            None,
            AuditOutcome::Allowed,
        );
        append_auth_audit(&mut conn, &event).await?;
        conn.commit().await.map_err(store_error)
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

    /// The deployment's public origin, from its configured callback.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] without a public origin.
    fn server_origin(&self) -> Result<AbsoluteUrl, WyrdError> {
        let origin = self.connections.require_callback()?.origin();
        AbsoluteUrl::new(origin.ascii_serialization()).map_err(|_| completion_unusable())
    }
}

/// The one refusal for every claim that cannot name a live handoff.
fn handoff_unavailable() -> WyrdError {
    WyrdError::InvalidState {
        message: "the CLI login handoff is unknown, expired, or already claimed; start a new \
                  login"
            .to_owned(),
        details: json!({ "reason": "cli_handoff_unavailable" }),
    }
}

/// Open a sealed CLI login completion into its session and refresh token.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when the completion cannot be opened with
/// `keyring`, does not decode, or carries no refresh token.
fn open_login(
    keyring: &SealingKeyring,
    sealed: &[u8],
) -> Result<(TokenResponse, SecretBearer), WyrdError> {
    let opened = keyring.open(sealed).map_err(|_| completion_unusable())?;
    let mut token: TokenResponse =
        serde_json::from_slice(&opened).map_err(|_| completion_unusable())?;
    let refresh_token = token.refresh_token.take().ok_or_else(completion_unusable)?;
    Ok((token, refresh_token))
}

/// A sealed completion this process cannot open, decode, or use.
fn completion_unusable() -> WyrdError {
    WyrdError::Internal {
        message: "the login completion could not be processed; start a new login".to_owned(),
        details: json!({}),
    }
}

/// CLI handoff begin, claim, cancel, and logout revocation against a real
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
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_spec::auth::{
        BeginLogin, CliHandoffClaim, PrincipalId, PrincipalKindTag, SecretBearer,
    };
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::TenantSlug;
    use wyrd_sql::queries::auth::{insert_human_refresh_token, refresh_by_hash};

    use super::{CLI_LOGOUT_OPERATION, CliLogins};
    use crate::connections::HumanConnections;
    use crate::exchange_api_key::token_hash;

    /// A CLI login owner over `fixture` whose tenant's Active connection
    /// points at `provider`, a mock discovery document.
    ///
    /// # Panics
    /// Panics when the fixture cannot be seeded.
    async fn owner(fixture: &PgFixture, provider: &MockServer) -> CliLogins {
        let issuer = provider.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "id_token_signing_alg_values_supported": ["EdDSA"],
            })))
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
        .execute(&fixture.superuser_pool().await.expect("superuser pool"))
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
            ),
            Arc::new(verifier()),
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

    /// A begun handoff carries a provider URL without its verifier; before
    /// the callback completes, only the verifier holder at the handoff's
    /// tenant is told to keep polling. A wrong verifier, an unknown tenant,
    /// a cancelled handoff, and a handoff id `POST /auth/login` never issued
    /// all get the one refusal, and cancellation removes the bound login.
    ///
    /// # Panics
    /// Panics when any step is accepted or refused differently.
    #[tokio::test]
    async fn only_the_verifier_holder_polls_and_cancel_ends_the_login() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let logins = owner(&fixture, &provider).await;
        let slug = TenantSlug::new(fixture.tenant_slug()).expect("slug");

        let handoff = logins.begin(&slug).await.expect("handoff begins");
        assert!(
            !handoff
                .login_url
                .as_str()
                .contains(handoff.poll_verifier.expose())
        );
        let wrong = SecretBearer::new("wrong".to_owned());
        let other = TenantSlug::new("no-such-tenant").expect("slug");
        let pending = logins
            .claim(&slug, handoff.handoff_id, &handoff.poll_verifier, "req")
            .await
            .expect("the holder polls");
        assert!(matches!(pending, CliHandoffClaim::Pending { .. }));
        for (tenant, verifier) in [(&slug, &wrong), (&other, &handoff.poll_verifier)] {
            let error = logins
                .claim(tenant, handoff.handoff_id, verifier, "req")
                .await
                .expect_err("refused");
            assert_eq!(reason(&error), Some("cli_handoff_unavailable"));
        }

        logins
            .cancel(&slug, handoff.handoff_id, &handoff.poll_verifier)
            .await
            .expect("cancel succeeds");
        let error = logins
            .claim(&slug, handoff.handoff_id, &handoff.poll_verifier, "req")
            .await
            .expect_err("a cancelled handoff is gone");
        assert_eq!(reason(&error), Some("cli_handoff_unavailable"));
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let states: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.auth_login_state")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("count runs");
        assert_eq!(states, 0, "cancel removes the bound login state");
        drop(conn);

        let error = logins
            .connections
            .begin_login(&BeginLogin {
                tenant_route_key: slug,
                browser_flow_hash: None,
                cli_handoff_id: Some(Uuid::now_v7()),
            })
            .await
            .expect_err("an unissued handoff id is refused");
        assert_eq!(reason(&error), Some("unknown_cli_handoff"));
    }

    /// Logout from a stale refresh token revokes its live successor, while the
    /// same User's other login keeps renewing, and records exactly one
    /// logout audit event; an audit store that refuses the append rolls the
    /// revocation back.
    ///
    /// # Panics
    /// Panics when the wrong rows are revoked or the audit differs.
    #[tokio::test]
    async fn logout_revokes_only_its_own_chain() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let logins = owner(&fixture, &provider).await;
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
                &token_hash(&jwt),
                Utc::now() + Duration::days(1),
                rotated_from.map(|index: usize| ids[index]),
                binding,
            )
            .await
            .expect("refresh row inserts");
            tokens.push(jwt);
            ids.push(id);
        }
        conn.commit().await.expect("seed commits");

        let admin = fixture.superuser_pool().await.expect("superuser pool");
        let logouts = || async {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM vala.audit_staging
                  WHERE data_tenant_id = $1 AND operation = $2 AND principal_id = $3",
            )
            .bind(tenant.as_uuid())
            .bind(CLI_LOGOUT_OPERATION)
            .bind(user)
            .fetch_one(&admin)
            .await
            .expect("audit query runs")
        };
        sqlx::query("REVOKE INSERT ON vala.audit_staging FROM wyrd_app")
            .execute(&admin)
            .await
            .expect("append privilege revoked");
        let refused = logins
            .end(&SecretBearer::new(tokens[0].clone()), "req-refused")
            .await;
        sqlx::query("GRANT INSERT ON vala.audit_staging TO wyrd_app")
            .execute(&admin)
            .await
            .expect("append privilege restored");
        assert!(
            matches!(refused, Err(WyrdError::AuditUnavailable { .. })),
            "{refused:?}"
        );
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let unrevoked = refresh_by_hash(&mut conn, &token_hash(&tokens[0]))
            .await
            .expect("lookup")
            .expect("row exists");
        assert_eq!(unrevoked.revoked_reason, None, "the revocation rolled back");
        drop(conn);
        assert_eq!(logouts().await, 0);

        logins
            .end(&SecretBearer::new(tokens[0].clone()), "req-logout")
            .await
            .expect("logout revokes");
        logins
            .end(&SecretBearer::new("not-a-jwt".to_owned()), "req-noop")
            .await
            .expect("an unusable token is a no-op");
        assert_eq!(logouts().await, 1, "exactly one logout is recorded");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let mut revoked = Vec::new();
        for jwt in &tokens {
            let row = refresh_by_hash(&mut conn, &token_hash(jwt))
                .await
                .expect("lookup")
                .expect("row exists");
            revoked.push(row.revoked_reason);
        }
        assert_eq!(
            revoked,
            vec![
                Some("cli_logout".to_owned()),
                Some("cli_logout".to_owned()),
                None
            ]
        );
    }
}
