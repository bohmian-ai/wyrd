//! Human OIDC login initiation and login-state storage.

use std::time::Duration;

use base64::Engine;
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use url::Url;
use wyrd_auth_oidc::ScreenedHttp;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{AbsoluteUrl, IssuerUrl, LoginInitResponse};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::auth::{LoginStateRow, insert_login_state, take_login_state};
use wyrd_sql::row_types::auth::HumanConnectionBinding;
use wyrd_sql::{SqlError, WyrdPostgres};

use crate::callback::discover_provider;
use crate::connections::ActiveHumanConnection;
use crate::error::store_error;

/// Lifetime of a persisted login-state row: the browser must complete the `IdP`
/// round-trip and hit `/auth/callback` within this window or the state is gone.
const LOGIN_STATE_TTL: Duration = Duration::from_mins(5);

/// Short-lived state row stored server-side during the OIDC login flow.
#[derive(Debug, Clone)]
pub struct LoginStateEntry {
    /// Server-generated PKCE verifier.
    pub code_verifier: SecretString,
    /// Server-generated nonce.
    pub nonce: String,
    /// Trusted issuer URL.
    pub issuer: String,
    /// Callback redirect URI.
    pub redirect_uri: String,
    /// The exact human-connection id and revision the login began through;
    /// the callback issues a session only while that revision is still Active.
    pub connection: HumanConnectionBinding,
}

/// Postgres-backed login-state store.
///
/// Every row is written and consumed on an RLS tenant transaction acquired
/// through [`WyrdPostgres`], so a state key only ever resolves inside the
/// tenant that created it.
#[derive(Clone)]
pub struct PgLoginStateStore {
    /// The role-separated runtime store the tenant transactions come from.
    postgres: WyrdPostgres,
}

impl PgLoginStateStore {
    /// Build a store over the runtime Postgres handle.
    #[must_use]
    pub fn new(postgres: WyrdPostgres) -> Self {
        Self { postgres }
    }

    /// Store one login-state row whose expiry `PostgreSQL` derives from `ttl`.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the tenant transaction cannot be opened, the
    /// insert fails, or the commit fails; nothing is durable unless it commits.
    pub async fn put(
        &self,
        tenant: DataTenantId,
        state: &str,
        entry: LoginStateEntry,
        ttl: Duration,
    ) -> Result<(), SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let row = LoginStateRow {
            code_verifier: entry.code_verifier.expose_secret().to_owned(),
            nonce: entry.nonce,
            issuer: entry.issuer,
            redirect_uri: entry.redirect_uri,
            connection: entry.connection,
        };
        insert_login_state(&mut conn, state, &row, ttl).await?;
        conn.commit().await
    }

    /// Consume a login-state row exactly once.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the tenant transaction cannot be opened, the
    /// consume fails, or the commit fails. A row consumed by an uncommitted
    /// transaction stays available.
    pub async fn take(
        &self,
        tenant: DataTenantId,
        state: &str,
    ) -> Result<Option<LoginStateEntry>, SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let row = take_login_state(&mut conn, state).await?;
        conn.commit().await?;
        Ok(row.map(|row| LoginStateEntry {
            code_verifier: SecretString::from(row.code_verifier),
            nonce: row.nonce,
            issuer: row.issuer,
            redirect_uri: row.redirect_uri,
            connection: row.connection,
        }))
    }
}

/// Resolve the `IdP` authorization URL and persist login state for the callback.
///
/// `redirect_uri` is the deployment's configured callback — the one URL
/// tenants register at their provider and candidate testing proved — never a
/// value derived from request headers. The state row binds the exact
/// connection revision in `active`, so the callback can refuse a login whose
/// connection was replaced, deactivated, or removed in the meantime.
///
/// # Errors
/// Returns [`WyrdError`] when the issuer's authorization endpoint cannot be
/// discovered, when the authorization URL cannot be built from the trusted
/// issuer's configuration, or when the login state cannot be persisted. No
/// redirect is returned unless its state row is durable.
pub async fn prepare_login(
    postgres: &WyrdPostgres,
    active: &ActiveHumanConnection,
    redirect_uri: &Url,
    http: ScreenedHttp,
) -> Result<LoginInitResponse, WyrdError> {
    let trusted = &active.trusted;
    let authorization_endpoint = discover_authorization_endpoint(&trusted.issuer, http).await?;
    let state_key = auth_state_key();
    let code_verifier = pkce_verifier();
    let nonce = auth_nonce();
    let authz_url = build_authorization_url(
        &authorization_endpoint,
        &trusted.client_id,
        redirect_uri.as_str(),
        &state_key,
        code_verifier.expose_secret(),
        &nonce,
    );
    let init = LoginInitResponse {
        authorization_url: AbsoluteUrl::new(authz_url.as_str().to_owned())
            .map_err(|_| invalid_token("authorization URL is invalid"))?,
        state: state_key.clone(),
    };
    PgLoginStateStore::new(postgres.clone())
        .put(
            trusted.tenant_id,
            &state_key,
            LoginStateEntry {
                code_verifier,
                nonce,
                issuer: trusted.issuer.to_string(),
                redirect_uri: redirect_uri.to_string(),
                connection: active.binding,
            },
            LOGIN_STATE_TTL,
        )
        .await
        .map_err(store_error)?;
    Ok(init)
}

/// Resolve the trusted issuer's OIDC `authorization_endpoint` via discovery.
///
/// # Errors
///
/// Returns [`WyrdError::DiscoveryUnavailable`] when the issuer URL is invalid,
/// the address behind it is refused by `http`, or the metadata request or
/// response is invalid. Cancellation can interrupt discovery without
/// persisting state.
///
/// The address is screened by `http` at the moment of the request, so an
/// issuer that resolved publicly when it was configured cannot resolve inward
/// now.
pub async fn discover_authorization_endpoint(
    issuer: &IssuerUrl,
    http: ScreenedHttp,
) -> Result<Url, WyrdError> {
    Ok(discover_provider(issuer, http)
        .await?
        .metadata
        .authorization_endpoint)
}

/// Generate an unguessable login-state key.
pub(crate) fn auth_state_key() -> String {
    random_b64url(32)
}

/// Generate the nonce the returned ID token must echo.
pub(crate) fn auth_nonce() -> String {
    random_b64url(32)
}

/// Generate a PKCE code verifier.
pub(crate) fn pkce_verifier() -> SecretString {
    SecretString::from(random_b64url(48))
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn random_b64url(bytes: usize) -> String {
    let mut buf = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Build the provider authorization URL for one login attempt.
///
/// Takes the client identifier rather than a whole trusted issuer because the
/// platform control plane's connection is not a tenant's issuer, and the URL
/// depends on nothing else about the issuer's trust configuration.
pub(crate) fn build_authorization_url(
    authorization_endpoint: &Url,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_verifier: &str,
    nonce: &str,
) -> Url {
    let mut url = authorization_endpoint.clone();
    let challenge = pkce_challenge(code_verifier);
    let mut query = url.query_pairs_mut();
    query.append_pair("response_type", "code");
    query.append_pair("client_id", client_id);
    query.append_pair("redirect_uri", redirect_uri);
    query.append_pair("scope", "openid profile email");
    query.append_pair("code_challenge", &challenge);
    query.append_pair("code_challenge_method", "S256");
    query.append_pair("state", state);
    query.append_pair("nonce", nonce);
    drop(query);
    url
}

fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

#[cfg(test)]
mod pg_tests {
    use super::{LoginStateEntry, PgLoginStateStore};
    use secrecy::{ExposeSecret, SecretString};
    use std::time::Duration;
    use uuid::Uuid;
    use wyrd_sql::row_types::auth::HumanConnectionBinding;

    /// A login-state binding; the store records it verbatim and nothing here
    /// requires the named connection to exist.
    fn binding() -> HumanConnectionBinding {
        HumanConnectionBinding {
            connection_id: Uuid::now_v7(),
            connection_revision: 3,
        }
    }

    /// A login-state row is consumed once from any replica's store and carries
    /// its connection binding back to the callback.
    #[tokio::test]
    async fn login_state_store_consumes_single_use_rows_once() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let store_a = PgLoginStateStore::new(fixture.wyrd_postgres().clone());
        let bound = binding();
        let store_b = PgLoginStateStore::new(fixture.wyrd_postgres().clone());

        store_a
            .put(
                tenant,
                "state-1",
                LoginStateEntry {
                    code_verifier: SecretString::from("verifier-1".to_owned()),
                    nonce: "nonce-1".to_owned(),
                    issuer: "https://idp.example.com/realms/acme".to_owned(),
                    redirect_uri: "https://app.example.com/auth/callback".to_owned(),
                    connection: bound,
                },
                Duration::from_mins(5),
            )
            .await
            .expect("state inserts");

        let consumed = store_b.take(tenant, "state-1").await.expect("state takes");
        let consumed = consumed.expect("state exists");
        assert_eq!(consumed.nonce, "nonce-1");
        assert_eq!(consumed.issuer, "https://idp.example.com/realms/acme");
        assert_eq!(
            consumed.redirect_uri,
            "https://app.example.com/auth/callback"
        );
        assert_eq!(consumed.code_verifier.expose_secret(), "verifier-1");
        assert_eq!(
            consumed.connection, bound,
            "the connection binding round-trips"
        );

        let replay = store_a
            .take(tenant, "state-1")
            .await
            .expect("state replay reads");
        assert!(replay.is_none());
    }

    /// A row past its database-derived expiry is never returned.
    #[tokio::test]
    async fn expired_login_state_returns_none() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let store = PgLoginStateStore::new(fixture.wyrd_postgres().clone());

        store
            .put(
                tenant,
                "state-expired",
                LoginStateEntry {
                    code_verifier: SecretString::from("verifier-expired".to_owned()),
                    nonce: "nonce-expired".to_owned(),
                    issuer: "https://idp.example.com/realms/acme".to_owned(),
                    redirect_uri: "https://app.example.com/auth/callback".to_owned(),
                    connection: binding(),
                },
                Duration::from_secs(0),
            )
            .await
            .expect("state inserts");

        tokio::time::sleep(Duration::from_millis(50)).await;

        let consumed = store
            .take(tenant, "state-expired")
            .await
            .expect("expired state read succeeds");
        assert!(consumed.is_none());
    }
}
