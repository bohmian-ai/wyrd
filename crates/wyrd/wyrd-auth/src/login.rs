//! Tenant human OIDC login initiation.
//!
//! A login begins at browser sign-in
//! ([`crate::browser_sessions::BrowserSessions::begin`]), at a device
//! verification page ([`crate::cli_logins::CliLogins::approve`]), or as a
//! candidate connection test ([`HumanConnections::begin_test`]). Each
//! resolves the tenant from its route key, requires the connection it signs
//! in through, and writes a login-state row keyed by the SHA-256 of a random
//! state value that exists only in the returned provider authorization URL,
//! bound to how the login was initiated. The common callback
//! ([`crate::callback::AuthorizationCodeExchange`]) consumes that row and
//! records the outcome: a browser session for the browser that began it, an
//! approval on the device authorization, or a tested candidate. No request
//! header ever selects a tenant, connection, or redirect.

use std::time::Duration;

use serde_json::json;
use url::Url;
use wyrd_spec::auth::{AbsoluteUrl, LoginInitiation, Sha256Hex};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::queries::auth::{LoginState, insert_login_state};

use crate::connections::{HumanConnections, provider_callback};
use crate::error::{relying_party_error, store_error};

/// Lifetime of a persisted login-state row: the browser must complete the `IdP`
/// round-trip and reach `/auth/callback` within this window or the state is
/// gone. A candidate test sign-in's state shares it.
pub(crate) const LOGIN_STATE_TTL: Duration = Duration::from_mins(5);

impl HumanConnections {
    /// Begin a tenant human SSO login bound to `initiation` and return the
    /// provider authorization URL.
    ///
    /// The route key resolves the tenant, and the tenant must have an Active connection; an unknown
    /// tenant and a tenant without one fail with the same generic refusal, so
    /// the endpoint cannot enumerate tenants or their login configuration.
    ///
    /// The provider comes from the relying party's per-issuer cache, which
    /// discovers through the screened HTTP capability on a miss, and the
    /// relying party screens the authorization endpoint by scheme and
    /// generates the state, nonce, and PKCE verifier. The redirect URI is the
    /// callback on `origin`, the address the request arrived at. The state row binds the
    /// exact connection revision, issuer, and client id, the PKCE verifier,
    /// the nonce, and the initiation binding; only the SHA-256 of the state is
    /// stored, and the raw state is returned only inside the URL. The
    /// unique binding index lets a device authorization name at most one
    /// login.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `origin` cannot form a callback;
    /// [`WyrdError::InvalidState`] for a binding
    /// already recorded for another login; [`WyrdError::InvalidToken`] when
    /// the route key names no active tenant or the tenant has no Active
    /// connection; [`WyrdError::DiscoveryUnavailable`] when the issuer is
    /// refused by screening, discovery fails, or the authorization endpoint's
    /// scheme is refused; and [`WyrdError::AuthVerifyUnavailable`] when a
    /// store read or the state write fails. No URL is returned unless its
    /// state row is durable; cancellation before the commit persists nothing.
    pub(crate) async fn begin_bound(
        &self,
        tenant_route_key: &TenantSlug,
        initiation: LoginInitiation,
        origin: &Url,
    ) -> Result<AbsoluteUrl, WyrdError> {
        let redirect_uri = provider_callback(origin)?;
        let tenant = self
            .postgres()
            .resolve_tenant_slug(tenant_route_key)
            .await
            .map_err(store_error)?
            .ok_or_else(login_unavailable)?;
        let active = self
            .active_connection(tenant)
            .await?
            .ok_or_else(login_unavailable)?;
        let trusted = &active.trusted;
        let provider = self
            .relying_party()
            .cached(&trusted.issuer)
            .await
            .map_err(relying_party_error)?;
        let authorization = self
            .relying_party()
            .authorize(&provider, &trusted.client_id, redirect_uri.as_str())
            .map_err(relying_party_error)?;
        let authorization_url = AbsoluteUrl::new(authorization.url.as_str().to_owned())
            .map_err(|_| invalid_token("authorization URL is invalid"))?;
        let row = LoginState {
            connection: active.binding,
            issuer: trusted.issuer.to_string(),
            client_id: trusted.client_id.clone(),
            redirect_uri: redirect_uri.to_string(),
            code_verifier: authorization.code_verifier,
            nonce: authorization.nonce,
            initiation,
        };
        let mut conn = self
            .postgres()
            .tenant_conn(tenant)
            .await
            .map_err(store_error)?;
        let state_hash = Sha256Hex::digest(authorization.state.as_bytes());
        if !insert_login_state(&mut conn, &state_hash, &row, LOGIN_STATE_TTL)
            .await
            .map_err(store_error)?
        {
            return Err(WyrdError::InvalidState {
                message: "this login binding was already used; start a new login".to_owned(),
                details: json!({ "reason": "login_binding_reused" }),
            });
        }
        conn.commit().await.map_err(store_error)?;
        Ok(authorization_url)
    }
}

/// The one refusal for a route key that names no active tenant or a tenant
/// with no Active connection, so neither can be told apart.
pub(crate) fn login_unavailable() -> WyrdError {
    WyrdError::InvalidToken {
        message: "SSO login is not available for this tenant".to_owned(),
        details: json!({}),
    }
}

fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

/// Login initiation refusals against a real tenant store.
#[cfg(test)]
mod pg_tests {
    use url::Url;
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::TenantSlug;
    use wyrd_spec::auth::{LoginInitiation, Sha256Hex};

    use crate::audit::test_audit::RecordedAudit;
    use crate::connections::HumanConnections;

    /// A connection owner over `fixture` with no keyring and an audit stage
    /// nothing in these tests stages on.
    fn connections(fixture: &PgFixture) -> HumanConnections {
        HumanConnections::new(
            fixture.wyrd_postgres().clone(),
            None,
            ScreenedHttp::allowing_internal(),
            RecordedAudit::new(),
        )
    }

    /// The origin these tests' sign-in requests arrive at.
    ///
    /// # Panics
    /// Panics when the fixed origin does not parse.
    fn origin() -> Url {
        Url::parse("https://wyrd.example.com").expect("origin parses")
    }

    /// A browser sign-in bound to a fixed login-transaction digest.
    fn browser() -> LoginInitiation {
        LoginInitiation::Browser(Sha256Hex::digest(b"login-transaction"))
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
            .expect("state count runs")
    }

    /// An unknown tenant and a tenant with no Active connection get the same
    /// generic refusal, so sign-in does not enumerate tenants, and neither
    /// writes login state.
    ///
    /// # Panics
    /// Panics when either sign-in is begun or the refusals differ.
    #[tokio::test]
    async fn unknown_tenant_and_no_connection_are_indistinguishable() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let owner = connections(&fixture);

        let unknown = owner
            .begin_bound(
                &TenantSlug::new("no-such-tenant").expect("slug is valid"),
                browser(),
                &origin(),
            )
            .await
            .expect_err("unknown tenant refuses");
        let unconfigured = owner
            .begin_bound(
                &TenantSlug::new(fixture.tenant_slug()).expect("slug is valid"),
                browser(),
                &origin(),
            )
            .await
            .expect_err("tenant without a connection refuses");

        assert_eq!(unknown.code(), unconfigured.code());
        assert_eq!(unknown.to_string(), unconfigured.to_string());
        assert_eq!(state_rows(&fixture).await, 0);
    }
}
