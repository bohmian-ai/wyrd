//! Federated human login at the platform control plane.
//!
//! An optional, additional way in. A deployment may configure one platform-scope
//! OIDC connection so its administrators sign in individually and are
//! individually audited, instead of sharing the one global credential. The
//! global credential never stops working: if the connection is absent, removed,
//! or its provider is down, platform administration continues through it.
//!
//! Identity here is pinned, never provisioned. Someone who already holds
//! platform authority pre-registers a principal against a claim; the first
//! successful login records `(issuer, subject)` durably and every later login
//! matches on the subject alone. An authenticated but unregistered subject is
//! denied and creates nothing, so no login can manufacture platform authority.
//!
//! The entry point selects the connection, the connection selects the
//! principal, and the principal carries the scope. Nothing infers scope from a
//! token, header, or hostname, and the session this mints is platform-scope
//! only — it confers no access inside any tenant.

use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use wyrd_auth_oidc::{
    ClientAuth, CodeRedemption, IssuerVerification, ProviderMetadata, RelyingParty,
    RelyingPartyError, ScreenedHttp, VerifiedIdToken,
};
use wyrd_crypt::SealingKeyring;
use wyrd_spec::auth::LoginInitResponse;
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::platform::identity::{
    PlatformOidcConnectionRow, insert_platform_login_state, platform_oidc_connection,
    take_platform_login_state,
};
use wyrd_sql::{OperatorPool, SqlError};

use crate::callback::verify_response_issuer;
use crate::error::relying_party_error;
use crate::pg_resolvers::platform_connection_from_row;
use crate::platform_sessions::{PlatformSessionError, PlatformSessions};
use std::fmt::{Debug, Formatter, Result as FmtResult};

/// How long a started platform login may take to complete.
///
/// The same five minutes the tenant flow allows: long enough for a real
/// provider round-trip including a password and a second factor, short enough
/// that an abandoned login is not a standing replay target.
const LOGIN_STATE_TTL: std::time::Duration = std::time::Duration::from_mins(5);

/// Failure during platform federated login.
#[derive(Debug, thiserror::Error)]
pub enum PlatformLoginError {
    /// This deployment has no platform OIDC connection configured.
    #[error("platform federated login is not configured")]
    NotConfigured,
    /// The stored connection could not be decoded, typically a missing sealing
    /// key for its client secret.
    #[error("platform OIDC connection could not be loaded")]
    ConnectionUnusable,
    /// The login state is missing, expired, or already consumed.
    #[error("login state is missing, expired, or already consumed")]
    InvalidState,
    /// The provider could not be reached or its response was unusable.
    #[error("identity provider unavailable")]
    ProviderUnavailable(Box<WyrdError>),
    /// The discovered authorization endpoint uses a scheme this deployment
    /// refuses to send a browser to. Carries the same redacted discovery
    /// refusal tenant login returns, so both planes answer it identically.
    #[error("authorization endpoint refused")]
    EndpointRefused(Box<WyrdError>),
    /// The token was rejected, or it verified to a subject this deployment does
    /// not recognize. Deliberately one variant: a caller learns whether it got
    /// in, never whether a given subject is registered.
    #[error("federated identity was not accepted")]
    NotAccepted,
    /// The platform store could not be reached.
    #[error("platform store unavailable: {0}")]
    Store(#[from] SqlError),
    /// A session could not be minted for an accepted identity.
    #[error("platform session could not be issued: {0}")]
    Session(String),
}

/// Federated login for the platform control plane.
///
/// Owns the platform boundary, the sealing key its connection's client secret
/// is sealed with, the relying party that runs the provider exchange and
/// verifies its ID token, and the session minter — a login is only meaningful
/// as the composition of all four.
///
/// Built once per process and shared by the begin and callback routes; clones
/// share one relying-party cache, so a callback reuses the provider state its
/// begin discovered.
#[derive(Clone)]
pub struct PlatformLogin {
    /// Cross-tenant boundary the connection and identity stores live behind.
    pool: OperatorPool,
    /// Process sealing key the stored client secret is opened with.
    sealing_key: Option<Arc<SealingKeyring>>,
    /// Mints the platform session an accepted identity receives.
    sessions: Arc<PlatformSessions>,
    /// The relying party every provider request is made through.
    relying_party: RelyingParty,
}

impl Debug for PlatformLogin {
    /// Prints the handle without its key, pool, or provider cache, none of
    /// which may reach a log or trace.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("PlatformLogin").finish_non_exhaustive()
    }
}

impl PlatformLogin {
    /// Bind platform login to the boundary, key, session minter, and a
    /// relying party whose provider requests all go through `http`.
    #[must_use]
    pub fn new(
        pool: OperatorPool,
        sealing_key: Option<Arc<SealingKeyring>>,
        sessions: Arc<PlatformSessions>,
        http: ScreenedHttp,
    ) -> Self {
        Self {
            pool,
            sealing_key,
            sessions,
            relying_party: RelyingParty::new(http),
        }
    }

    /// Load the configured connection, or report that there is none.
    ///
    /// # Errors
    /// Returns [`PlatformLoginError::NotConfigured`] when no connection exists,
    /// [`PlatformLoginError::ConnectionUnusable`] when the stored row cannot be
    /// decoded, and [`PlatformLoginError::Store`] when the read fails.
    async fn connection(&self) -> Result<PlatformConnection, PlatformLoginError> {
        let row = platform_oidc_connection(&self.pool)
            .await?
            .ok_or(PlatformLoginError::NotConfigured)?;
        decode_connection(row, self.sealing_key.as_deref())
    }

    /// Begin a platform login and return the provider authorization URL.
    ///
    /// The entry point selects the connection: there is exactly one, so nothing
    /// in the request chooses which issuer to trust. The provider comes from
    /// the relying party's per-issuer cache. PKCE verifier, nonce, and state
    /// are generated by the relying party and stored server-side; the browser
    /// carries only the opaque state key.
    ///
    /// # Errors
    /// Returns [`PlatformLoginError::NotConfigured`] when federated login is not
    /// configured, [`PlatformLoginError::ProviderUnavailable`] when discovery
    /// fails, [`PlatformLoginError::EndpointRefused`] when the discovered
    /// authorization endpoint's scheme is refused, and
    /// [`PlatformLoginError::Store`] when the state cannot be persisted.
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn begin(
        &self,
        redirect_uri: String,
    ) -> Result<LoginInitResponse, PlatformLoginError> {
        let connection = self.connection().await?;
        let provider = self
            .relying_party
            .cached(&connection.verification.issuer)
            .await
            .map_err(provider_unavailable)?;
        self.authorize(&connection, &provider, redirect_uri).await
    }

    /// Screen the discovered authorization endpoint, persist the login state,
    /// and return the browser destination.
    ///
    /// The endpoint passes the same scheme rule tenant login applies before any
    /// state exists, so production never sends state, nonce, or PKCE challenge
    /// over cleartext while a permissive deployment keeps its local `http`
    /// provider. Nothing is persisted when the endpoint is refused.
    ///
    /// # Errors
    /// Returns [`PlatformLoginError::EndpointRefused`] when the policy refuses
    /// the endpoint's scheme, [`PlatformLoginError::Store`] when the state
    /// cannot be persisted, and [`PlatformLoginError::ProviderUnavailable`]
    /// when the redirect URI or the authorization URL is invalid.
    async fn authorize(
        &self,
        connection: &PlatformConnection,
        provider: &ProviderMetadata,
        redirect_uri: String,
    ) -> Result<LoginInitResponse, PlatformLoginError> {
        let authorization = self
            .relying_party
            .authorize(provider, &connection.client_id, &redirect_uri)
            .map_err(|error| match error {
                RelyingPartyError::Screened(_) => {
                    PlatformLoginError::EndpointRefused(Box::new(relying_party_error(error)))
                }
                error => provider_unavailable(error),
            })?;

        insert_platform_login_state(
            &self.pool,
            &authorization.state,
            authorization.code_verifier.expose_secret(),
            &authorization.nonce,
            connection.verification.issuer.as_str(),
            &redirect_uri,
            LOGIN_STATE_TTL,
        )
        .await?;

        Ok(LoginInitResponse {
            authorization_url: wyrd_spec::auth::AbsoluteUrl::new(
                authorization.url.as_str().to_owned(),
            )
            .map_err(|_| {
                PlatformLoginError::ProviderUnavailable(Box::new(WyrdError::DiscoveryUnavailable {
                    message: "authorization URL is invalid".to_owned(),
                    details: serde_json::json!({}),
                }))
            })?,
            state: authorization.state,
        })
    }

    /// Complete a platform login and mint a platform session.
    ///
    /// Consumes the login state exactly once and reads the provider from the
    /// relying party's per-issuer cache (re-discovered once when the ID token
    /// names an unknown key). The authorization response's `iss`
    /// (`response_issuer`) is bound to the login's issuer by
    /// [`verify_response_issuer`] (RFC 9207) before the code reaches any token
    /// endpoint. The relying party then exchanges the code and verifies the
    /// ID token — its issuer, the connection's client ID as its audience,
    /// signature, expiry, issued-at, nonce, and authorized party. The subject
    /// then resolves to a pre-registered principal, pinned if this is that
    /// principal's first login.
    ///
    /// # Errors
    /// Returns [`PlatformLoginError::InvalidState`] for a missing, expired, or
    /// replayed state, [`PlatformLoginError::ProviderUnavailable`] when the
    /// provider cannot be reached or refuses the code,
    /// [`PlatformLoginError::NotAccepted`] when the response issuer is
    /// mismatched or advertised and missing, the ID token is rejected, or its
    /// subject is unregistered,
    /// [`PlatformLoginError::Store`] when the platform store fails, and
    /// [`PlatformLoginError::Session`] when the session cannot be signed.
    #[tracing::instrument(level = "info", skip(self, code), err)]
    pub async fn complete(
        &self,
        code: SecretString,
        state: &str,
        response_issuer: Option<&str>,
        request_id: &str,
    ) -> Result<SecretString, PlatformLoginError> {
        let connection = self.connection().await?;
        let login_state = take_platform_login_state(&self.pool, state)
            .await?
            .ok_or(PlatformLoginError::InvalidState)?;

        // The state row names the issuer the login started against. If the
        // connection was replaced mid-flight, the in-flight login belongs to an
        // issuer this deployment no longer trusts.
        if login_state.issuer != connection.verification.issuer.as_str() {
            return Err(PlatformLoginError::InvalidState);
        }

        let issuer = &connection.verification.issuer;
        let provider = self
            .relying_party
            .cached(issuer)
            .await
            .map_err(provider_unavailable)?;
        verify_response_issuer(
            response_issuer,
            &login_state.issuer,
            provider
                .additional_metadata()
                .authorization_response_iss_parameter_supported,
        )
        .map_err(|error| {
            tracing::warn!(error = %error, "platform authorization response issuer refused");
            PlatformLoginError::NotAccepted
        })?;
        let code_verifier = SecretString::from(login_state.code_verifier);
        let redemption = CodeRedemption {
            client_id: &connection.client_id,
            client_auth: &connection.client_auth,
            claim_mapping: &connection.verification.claim_mapping,
            redirect_uri: &login_state.redirect_uri,
            code_verifier: &code_verifier,
            nonce: &login_state.nonce,
        };
        let verified = self
            .relying_party
            .redeem(issuer, provider, code, &redemption)
            .await
            .map_err(|error| match error {
                RelyingPartyError::InvalidNonce
                | RelyingPartyError::UnknownKey
                | RelyingPartyError::InvalidIdToken(_) => {
                    tracing::warn!(error = %error, "platform ID token rejected");
                    PlatformLoginError::NotAccepted
                }
                error => provider_unavailable(error),
            })?;

        // Everything after this point is the grant, and the grant is one
        // transaction the sessions owner holds: resolving or pinning the
        // identity, re-reading the principal, minting, and auditing commit
        // together or not at all. This owner hands over only what it verified.
        //
        // The claim must be a *verified* email. An unverified one is a string
        // the subject chose, and pinning on it would let anyone at this issuer
        // who can set their email to the pre-registered address capture that
        // principal — permanently, since pinning is one-way. A provider that
        // does not assert `email_verified` cannot be used to establish a
        // platform administrator at all, which is the right outcome rather than
        // a weaker match.
        let match_claim = verified_email(&verified);
        self.sessions
            .issue_federated(
                connection.verification.issuer.as_str(),
                &verified.identity.subject,
                match_claim.as_deref(),
                request_id,
            )
            .await
            .map_err(|error| match error {
                PlatformSessionError::Invalid => PlatformLoginError::NotAccepted,
                PlatformSessionError::Store(error) => PlatformLoginError::Store(error),
                PlatformSessionError::Key(message) => PlatformLoginError::Session(message),
            })
    }
}

/// The platform OIDC connection in usable form.
///
/// Splits what verification needs from what the token endpoint needs, which is
/// why the client secret lives beside [`IssuerVerification`] rather than inside
/// it: verification never authenticates to the provider.
#[derive(Debug, Clone)]
pub struct PlatformConnection {
    /// Facts the token verifier checks against.
    pub verification: IssuerVerification,
    /// Wyrd's client identifier at the provider.
    pub client_id: String,
    /// How Wyrd authenticates to the provider's token endpoint.
    pub client_auth: ClientAuth,
}

/// Decode a stored connection row, opening its sealed client secret.
///
/// # Errors
/// Returns [`PlatformLoginError::ConnectionUnusable`] when the row's URLs,
/// claim mapping, or client authentication cannot be decoded — including when
/// it carries a secret and no sealing key is configured. That fails closed
/// rather than silently degrading to an unauthenticated client.
fn decode_connection(
    row: PlatformOidcConnectionRow,
    sealing_key: Option<&SealingKeyring>,
) -> Result<PlatformConnection, PlatformLoginError> {
    platform_connection_from_row(row, sealing_key).map_err(|error| {
        tracing::warn!(error = %error, "platform OIDC connection could not be decoded");
        PlatformLoginError::ConnectionUnusable
    })
}

/// Extract the subject's email only when the issuer asserts it is verified.
///
/// Returns `None` when the token carries no email, or carries one the provider
/// has not verified. Pinning is one-way, so an unverified address is not a
/// weaker match to fall back on — it is a takeover of the pre-registered
/// principal by whoever claims that address first.
fn verified_email(token: &VerifiedIdToken) -> Option<String> {
    let verified = token
        .claims
        .get("email_verified")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    verified.then(|| token.identity.email.clone()).flatten()
}

/// The provider could not be reached, or refused or garbled the exchange.
///
/// Carries the same public refusal tenant login returns for `error`.
fn provider_unavailable(error: RelyingPartyError) -> PlatformLoginError {
    PlatformLoginError::ProviderUnavailable(Box::new(relying_party_error(error)))
}

#[cfg(test)]
mod pg_tests {
    //! Durable proof that platform login screens the discovered authorization
    //! endpoint with the tenant-login scheme rule before persisting any state.

    use std::sync::Arc;

    use secrecy::SecretString;
    use serde_json::json;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_oidc::{
        AddressPolicy, ClaimMapping, ClaimPath, ClientAuth, IssuerVerification, ProviderMetadata,
        RelyingParty, ScreenedHttp,
    };
    use wyrd_auth_verify::Kid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::auth::{IssuerTokenPolicy, IssuerUrl};
    use wyrd_spec::error::WyrdError;

    use super::{PlatformConnection, PlatformLogin, PlatformLoginError};
    use crate::platform_sessions::PlatformSessions;

    /// A throwaway Ed25519 key the session minter is built with; these tests
    /// never mint a session.
    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

    /// Build platform login over `fixture` with the deployment's `policy`.
    ///
    /// # Panics
    /// Panics when the test signing key does not load.
    fn platform_login(fixture: &PgFixture, policy: AddressPolicy) -> PlatformLogin {
        let key = IssuingKey::from_ed_pem(
            SecretString::from(PRIVATE_KEY_PEM),
            Kid::new("k1").expect("kid is valid"),
            "wyrd",
        )
        .expect("test private key loads");
        PlatformLogin::new(
            fixture.operator_pool().clone(),
            None,
            Arc::new(PlatformSessions::new(
                fixture.operator_pool().clone(),
                Arc::new(key),
            )),
            ScreenedHttp::new(policy),
        )
    }

    /// Discover a loopback provider that publishes `authorization_endpoint`,
    /// returning the mock and the platform connection that trusts it.
    ///
    /// # Panics
    /// Panics when the mock provider cannot be discovered.
    async fn provider_publishing(
        authorization_endpoint: &str,
    ) -> (MockServer, Arc<ProviderMetadata>, PlatformConnection) {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issuer": issuer,
                "authorization_endpoint": authorization_endpoint,
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "response_types_supported": ["code"],
                "subject_types_supported": ["public"],
                "id_token_signing_alg_values_supported": ["RS256"],
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "keys": [] })))
            .mount(&server)
            .await;
        let provider = RelyingParty::new(ScreenedHttp::allowing_internal())
            .discover(&IssuerUrl::new(issuer.clone()).expect("mock issuer is valid"))
            .await
            .expect("mock provider is discovered");
        let connection = PlatformConnection {
            verification: IssuerVerification {
                issuer: IssuerUrl::new(issuer.clone()).expect("mock issuer is valid"),
                jwks_uri: Url::parse(&format!("{issuer}/jwks")).expect("jwks uri parses"),
                expected_audience: "wyrd-platform".to_owned(),
                claim_mapping: ClaimMapping {
                    subject: ClaimPath::new("sub"),
                    email: None,
                    groups: None,
                },
                principal_kind: IssuerTokenPolicy::Human,
            },
            client_id: "wyrd-platform".to_owned(),
            client_auth: ClientAuth::Public,
        };
        (server, provider, connection)
    }

    /// Count the platform login states persisted in `fixture`.
    ///
    /// # Panics
    /// Panics when the superuser read fails.
    async fn login_states(fixture: &PgFixture) -> i64 {
        let admin = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query_scalar("SELECT count(*) FROM platform.login_state")
            .fetch_one(&admin)
            .await
            .expect("login states read")
    }

    /// Production refuses a discovered cleartext authorization endpoint with
    /// tenant login's redacted discovery refusal and persists no state.
    ///
    /// # Panics
    /// Panics when the endpoint is accepted, refused with another error, or
    /// any login state is persisted.
    #[tokio::test]
    async fn production_platform_login_refuses_a_cleartext_authorization_endpoint() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let (_server, provider, connection) =
            provider_publishing("http://idp.example/authorize").await;

        let error = platform_login(&fixture, AddressPolicy::BlockInternal)
            .authorize(
                &connection,
                &provider,
                "https://wyrd.example/callback".to_owned(),
            )
            .await
            .expect_err("a cleartext browser destination is refused");

        assert!(
            matches!(
                &error,
                PlatformLoginError::EndpointRefused(inner)
                    if matches!(**inner, WyrdError::DiscoveryUnavailable { .. })
            ),
            "{error:?}"
        );
        assert_eq!(login_states(&fixture).await, 0, "no login state persists");
    }

    /// Production accepts an `https` authorization endpoint and persists the
    /// one login state the callback will consume.
    ///
    /// # Panics
    /// Panics when the endpoint is refused or the state is not persisted.
    #[tokio::test]
    async fn production_platform_login_accepts_an_https_authorization_endpoint() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let (_server, provider, connection) =
            provider_publishing("https://idp.example/authorize").await;

        let init = platform_login(&fixture, AddressPolicy::BlockInternal)
            .authorize(
                &connection,
                &provider,
                "https://wyrd.example/callback".to_owned(),
            )
            .await
            .expect("an https browser destination is accepted");

        assert!(
            init.authorization_url
                .as_str()
                .starts_with("https://idp.example/authorize?"),
            "{}",
            init.authorization_url.as_str()
        );
        assert_eq!(login_states(&fixture).await, 1, "one login state persists");
    }
}
