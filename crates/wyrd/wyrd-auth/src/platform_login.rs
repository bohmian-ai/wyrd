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

use chrono::{Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use wyrd_auth_oidc::{ClientAuth, IssuerVerification, ScreenedHttp};
use wyrd_auth_verify::{ExternalClaims, ExternalVerifier};
use wyrd_crypt::SecretKey;
use wyrd_spec::auth::LoginInitResponse;
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::platform::identity::{
    PlatformOidcConnectionRow, insert_platform_login_state, platform_oidc_connection,
    take_platform_login_state,
};
use wyrd_sql::{OperatorPool, SqlError};

use crate::error::auth_error_to_wyrd;
use crate::login::{auth_nonce, auth_state_key, build_authorization_url, pkce_verifier};
use crate::pg_resolvers::{PgIssuerResolver, platform_connection_from_row};
use crate::platform_sessions::{PlatformSessionError, PlatformSessions};
use std::fmt::{Debug, Formatter, Result as FmtResult};

/// How long a started platform login may take to complete.
///
/// The same five minutes the tenant flow allows: long enough for a real
/// provider round-trip including a password and a second factor, short enough
/// that an abandoned login is not a standing replay target.
const LOGIN_STATE_TTL: Duration = Duration::minutes(5);

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
/// is sealed with, the verifier that checks the provider's tokens, and the
/// session minter — a login is only meaningful as the composition of all four.
pub struct PlatformLogin {
    /// Cross-tenant boundary the connection and identity stores live behind.
    pool: OperatorPool,
    /// Process sealing key the stored client secret is opened with.
    sealing_key: Option<Arc<SecretKey>>,
    /// The deployment's single external-token verification implementation.
    verifier: Arc<ExternalVerifier<PgIssuerResolver>>,
    /// Mints the platform session an accepted identity receives.
    sessions: Arc<PlatformSessions>,
    /// Screened HTTP capability every provider request is made through.
    http: ScreenedHttp,
}

impl Debug for PlatformLogin {
    /// Prints the handle without its key, pool, or verifier, none of which may
    /// reach a log or trace.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("PlatformLogin").finish_non_exhaustive()
    }
}

impl PlatformLogin {
    /// Bind platform login to the boundary, key, verifier, session minter, and
    /// the screened HTTP capability every provider request is made through.
    #[must_use]
    pub fn new(
        pool: OperatorPool,
        sealing_key: Option<Arc<SecretKey>>,
        verifier: Arc<ExternalVerifier<PgIssuerResolver>>,
        sessions: Arc<PlatformSessions>,
        http: ScreenedHttp,
    ) -> Self {
        Self {
            pool,
            sealing_key,
            verifier,
            sessions,
            http,
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
    /// in the request chooses which issuer to trust. PKCE verifier, nonce, and
    /// state are generated here and stored server-side; the browser carries only
    /// the opaque state key.
    ///
    /// # Errors
    /// Returns [`PlatformLoginError::NotConfigured`] when federated login is not
    /// configured, [`PlatformLoginError::ProviderUnavailable`] when discovery
    /// fails, and [`PlatformLoginError::Store`] when the state cannot be
    /// persisted.
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn begin(
        &self,
        redirect_uri: String,
    ) -> Result<LoginInitResponse, PlatformLoginError> {
        let connection = self.connection().await?;
        let authorization_endpoint = crate::login::discover_authorization_endpoint(
            &connection.verification.issuer,
            self.http,
        )
        .await
        .map_err(|error| PlatformLoginError::ProviderUnavailable(Box::new(error)))?;

        let state = auth_state_key();
        let code_verifier = pkce_verifier();
        let nonce = auth_nonce();
        let authorization_url = build_authorization_url(
            &authorization_endpoint,
            &connection.client_id,
            &redirect_uri,
            &state,
            code_verifier.expose_secret(),
            &nonce,
        );

        insert_platform_login_state(
            &self.pool,
            &state,
            code_verifier.expose_secret(),
            &nonce,
            connection.verification.issuer.as_str(),
            &redirect_uri,
            Utc::now() + LOGIN_STATE_TTL,
        )
        .await?;

        Ok(LoginInitResponse {
            authorization_url: wyrd_spec::auth::AbsoluteUrl::new(
                authorization_url.as_str().to_owned(),
            )
            .map_err(|_| {
                PlatformLoginError::ProviderUnavailable(Box::new(WyrdError::DiscoveryUnavailable {
                    message: "authorization URL is invalid".to_owned(),
                    details: serde_json::json!({}),
                }))
            })?,
            state,
        })
    }

    /// Complete a platform login and mint a platform session.
    ///
    /// Consumes the login state exactly once, exchanges the code, verifies the
    /// ID token through the deployment's single external verification path, and
    /// resolves the subject to a pre-registered principal — pinning it if this
    /// is that principal's first login.
    ///
    /// # Errors
    /// Returns [`PlatformLoginError::InvalidState`] for a missing, expired, or
    /// replayed state, [`PlatformLoginError::ProviderUnavailable`] when the
    /// provider cannot be reached, [`PlatformLoginError::NotAccepted`] when the
    /// token is rejected or its subject is unregistered,
    /// [`PlatformLoginError::Store`] when the platform store fails, and
    /// [`PlatformLoginError::Session`] when the session cannot be signed.
    #[tracing::instrument(level = "info", skip(self, code), err)]
    pub async fn complete(
        &self,
        code: SecretString,
        state: &str,
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

        let provider =
            crate::callback::discover_provider(&connection.verification.issuer, self.http)
                .await
                .map_err(|error| PlatformLoginError::ProviderUnavailable(Box::new(error)))?;
        let id_token = crate::callback::exchange_code_for_id_token(
            &provider,
            &connection.client_id,
            &connection.client_auth,
            &login_state.redirect_uri,
            &SecretString::from(login_state.code_verifier),
            code,
            self.http,
        )
        .await
        .map_err(|error| PlatformLoginError::ProviderUnavailable(Box::new(error)))?;

        let claims = self
            .verifier
            .verify_external_against(&connection.verification, &id_token)
            .await
            .map_err(|error| {
                tracing::warn!(error = %auth_error_to_wyrd(error), "platform ID token rejected");
                PlatformLoginError::NotAccepted
            })?;
        verify_nonce(&login_state.nonce, &claims)?;

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
        let match_claim = verified_email(&claims);
        self.sessions
            .issue_federated(
                connection.verification.issuer.as_str(),
                &claims.subject,
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
    sealing_key: Option<&SecretKey>,
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
fn verified_email(claims: &ExternalClaims) -> Option<String> {
    let verified = claims
        .raw_claims
        .get("email_verified")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    verified.then(|| claims.email.clone()).flatten()
}

/// Confirm the ID token echoes the nonce this login generated.
///
/// Without it a token minted for a different login attempt at the same provider
/// would be accepted here, so this is what binds the token to this exchange.
///
/// # Errors
/// Returns [`PlatformLoginError::NotAccepted`] when the nonce is absent or
/// does not match.
fn verify_nonce(expected: &str, claims: &ExternalClaims) -> Result<(), PlatformLoginError> {
    let presented = claims
        .raw_claims
        .get("nonce")
        .and_then(serde_json::Value::as_str);
    if presented == Some(expected) {
        Ok(())
    } else {
        Err(PlatformLoginError::NotAccepted)
    }
}
