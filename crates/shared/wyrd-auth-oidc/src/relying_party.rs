//! OpenID Connect relying party for human login, built on `openidconnect`.
//!
//! Discovery, the authorization request (state, nonce, and PKCE), the code
//! exchange, and ID-token verification are the library's. Every request it
//! makes goes through [`ScreenedHttp`], plugged in as its asynchronous HTTP
//! client, so each one is address-screened, pinned, proxy-free, bounded, and
//! never follows a redirect.
//!
//! What stays here is what the library leaves to its caller:
//!
//! - the per-issuer metadata and key-set cache, with exactly one forced
//!   re-discovery when a token names a key the cached set does not hold;
//! - the authorized-party rules of OpenID Connect Core 1.0 §3.1.3.7 steps 4
//!   and 5, which the library documents as the caller's;
//! - the issued-at acceptance window of step 10, through the library's hook;
//! - the Subject Identifier shape of OpenID Connect Core 1.0 §2; and
//! - the RFC 9207 `authorization_response_iss_parameter_supported` member,
//!   carried as additional provider metadata for the callback's `iss` check.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use moka::future::Cache;
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreAuthenticationFlow, CoreClaimName, CoreClaimType,
    CoreClientAuthMethod, CoreErrorResponseType, CoreGenderClaim, CoreGrantType, CoreJsonWebKey,
    CoreJweContentEncryptionAlgorithm, CoreJweKeyManagementAlgorithm, CoreJwsSigningAlgorithm,
    CoreResponseMode, CoreResponseType, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreSubjectIdentifierType, CoreTokenIntrospectionResponse, CoreTokenType,
};
use openidconnect::{
    AdditionalClaims, AdditionalProviderMetadata, AsyncHttpClient, AuthType, AuthorizationCode,
    ClaimsVerificationError, Client, ClientId, ClientSecret, CsrfToken, DiscoveryError,
    EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet, HttpRequest,
    HttpResponse, IdTokenClaims, IdTokenFields, IdTokenVerifier, JsonWebKeySet, Nonce,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RequestTokenError, Scope,
    SignatureVerificationError, StandardErrorResponse, StandardTokenResponse, TokenResponse,
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use url::Url;
use wyrd_spec::auth::IssuerUrl;

use crate::claims::{MappedClaims, map_claims};
use crate::registry::{ClaimMapping, ClientAuth};
use crate::screening::{ScreenError, ScreenedHttp, read_bounded_body};

/// How long a discovered provider is reused before it is discovered again.
///
/// The same five minutes a connection's key cache defaults to: long enough to
/// keep discovery off every login, short enough that a republished endpoint is
/// picked up without a restart.
const PROVIDER_TTL: Duration = Duration::from_mins(5);

/// Most distinct issuers whose discovery is cached at once.
const MAX_CACHED_PROVIDERS: u64 = 256;

/// Clock skew accepted on an ID token's `exp` and `iat`.
///
/// The same thirty seconds the deployment's token verifier allows by default.
const CLOCK_SKEW: chrono::Duration = chrono::Duration::seconds(30);

/// Random bytes in each generated `state` and `nonce` value.
const RANDOM_VALUE_BYTES: u32 = 32;

/// Longest Subject Identifier OpenID Connect Core 1.0 §2 permits, in bytes.
const MAX_SUBJECT_BYTES: usize = 255;

/// RFC 9207 provider metadata the relying party reads beyond the core set.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct IssuerParameterMetadata {
    /// RFC 9207 §3 `authorization_response_iss_parameter_supported`: whether
    /// the provider promises an `iss` parameter on every authorization
    /// response. An omitted member decodes as `false`, the RFC's meaning of
    /// absence; a non-boolean value fails discovery.
    #[serde(default)]
    pub authorization_response_iss_parameter_supported: bool,
}

impl AdditionalProviderMetadata for IssuerParameterMetadata {}

/// A provider's discovery document and the key set its `jwks_uri` served,
/// as `openidconnect` decodes and validates them.
pub type ProviderMetadata = openidconnect::ProviderMetadata<
    IssuerParameterMetadata,
    CoreAuthDisplay,
    CoreClientAuthMethod,
    CoreClaimName,
    CoreClaimType,
    CoreGrantType,
    CoreJweContentEncryptionAlgorithm,
    CoreJweKeyManagementAlgorithm,
    CoreJsonWebKey,
    CoreResponseMode,
    CoreResponseType,
    CoreSubjectIdentifierType,
>;

/// Every non-standard claim of a verified ID token, kept so a connection's
/// claim mapping can read provider-specific paths such as nested groups.
#[derive(Debug, Deserialize, Serialize)]
pub struct OtherClaims {
    /// The claims `openidconnect` does not model, by name.
    #[serde(flatten)]
    claims: serde_json::Map<String, serde_json::Value>,
}

impl AdditionalClaims for OtherClaims {}

/// The token endpoint response this relying party decodes.
type RelyingPartyTokenResponse = StandardTokenResponse<
    IdTokenFields<
        OtherClaims,
        EmptyExtraTokenFields,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    >,
    CoreTokenType,
>;

/// An `openidconnect` client built from discovered metadata.
type RelyingPartyClient = Client<
    OtherClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    RelyingPartyTokenResponse,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// Why a provider HTTP request produced no usable response.
#[derive(Debug, thiserror::Error)]
pub enum ProviderHttpError {
    /// Screening refused the URL before any connection was made.
    #[error(transparent)]
    Screened(#[from] ScreenError),
    /// The request could not be built, sent, or read within the body cap.
    #[error("provider request failed: {0}")]
    Transport(String),
    /// The provider answered with a server error.
    #[error("provider answered {0}")]
    Unavailable(openidconnect::http::StatusCode),
}

impl ScreenedHttp {
    /// Send one `openidconnect` request through a screened, pinned client.
    ///
    /// The request URL is screened and pinned by [`Self::client_for`], the
    /// request is sent with redirects disabled, and the body is read through
    /// [`read_bounded_body`]. A `3xx` comes back as the response it is, so the
    /// library refuses it and no second origin is ever contacted. A `5xx` is
    /// returned as [`ProviderHttpError::Unavailable`] so callers can tell a
    /// provider outage from a refusal.
    ///
    /// # Errors
    /// Returns [`ProviderHttpError::Screened`] when screening refuses the URL,
    /// [`ProviderHttpError::Transport`] when the URL does not parse or the
    /// request fails, times out, or exceeds the body cap, and
    /// [`ProviderHttpError::Unavailable`] for a server-error status.
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderHttpError> {
        let url = Url::parse(&request.uri().to_string())
            .map_err(|error| ProviderHttpError::Transport(error.to_string()))?;
        let client = self.client_for(&url).await?;
        let (parts, body) = request.into_parts();
        let response = client
            .request(parts.method, url)
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(|error| ProviderHttpError::Transport(error.to_string()))?;
        let status = response.status();
        if status.is_server_error() {
            return Err(ProviderHttpError::Unavailable(status));
        }
        let headers = response.headers().clone();
        let body = read_bounded_body(response)
            .await
            .map_err(|error| ProviderHttpError::Transport(error.to_string()))?;
        let mut response = HttpResponse::new(body);
        *response.status_mut() = status;
        *response.headers_mut() = headers;
        Ok(response)
    }
}

impl<'c> AsyncHttpClient<'c> for ScreenedHttp {
    type Error = ProviderHttpError;
    type Future = std::pin::Pin<
        Box<dyn Future<Output = Result<HttpResponse, ProviderHttpError>> + Send + 'c>,
    >;

    /// Hand `request` to [`ScreenedHttp::send`].
    fn call(&'c self, request: HttpRequest) -> Self::Future {
        Box::pin(self.send(request))
    }
}

/// Why a relying-party step refused or could not complete.
#[derive(Debug, thiserror::Error)]
pub enum RelyingPartyError {
    /// Screening refused a provider URL before any request was made to it.
    #[error("provider URL refused by address screening: {0}")]
    Screened(ScreenError),
    /// The discovery document names a different issuer than was requested
    /// (OpenID Connect Discovery 1.0 §4.3).
    #[error("the discovery document names a different issuer")]
    IssuerMismatch,
    /// Discovery or the key-set fetch failed or answered unusably.
    #[error("provider discovery failed: {0}")]
    DiscoveryUnavailable(String),
    /// The token endpoint could not be reached or answered with a server
    /// error.
    #[error("token endpoint unavailable: {0}")]
    TokenEndpointUnavailable(String),
    /// The token endpoint refused the code or answered with no usable ID
    /// token.
    #[error("authorization code exchange was rejected: {0}")]
    TokenRejected(String),
    /// The ID token does not carry the nonce the login recorded.
    #[error("ID token nonce is missing or mismatched")]
    InvalidNonce,
    /// No key in the provider's key set matches the ID token.
    #[error("no key in the provider key set matches the ID token")]
    UnknownKey,
    /// The ID token failed verification for any other reason.
    #[error("ID token rejected: {0}")]
    InvalidIdToken(String),
    /// The connection's client authentication or redirect cannot be used for
    /// a code exchange.
    #[error("relying-party configuration is unusable: {0}")]
    Configuration(String),
}

/// One login attempt's authorization request: the browser destination and the
/// library-generated values the login state must record.
#[derive(Debug)]
pub struct Authorization {
    /// Provider authorization URL carrying `state`, `nonce`, and the PKCE
    /// `S256` challenge.
    pub url: Url,
    /// The `state` value the callback returns; stored only as its hash.
    pub state: String,
    /// The `nonce` the ID token must echo.
    pub nonce: String,
    /// The PKCE code verifier the code exchange must present.
    pub code_verifier: SecretString,
}

/// What a code exchange needs from the login state and its connection.
#[derive(Debug)]
pub struct CodeRedemption<'a> {
    /// Wyrd's client identifier at the provider.
    pub client_id: &'a str,
    /// How Wyrd authenticates to the token endpoint.
    pub client_auth: &'a ClientAuth,
    /// The audience the ID token must name.
    pub audience: &'a str,
    /// How the verified claims map to an identity.
    pub claim_mapping: &'a ClaimMapping,
    /// The redirect URI the authorization request carried.
    pub redirect_uri: &'a str,
    /// The PKCE code verifier the login state recorded.
    pub code_verifier: &'a SecretString,
    /// The nonce the login state recorded.
    pub nonce: &'a str,
}

/// A verified ID token's identity and its complete claim set.
#[derive(Debug, Clone)]
pub struct VerifiedIdToken {
    /// The identity the connection's claim mapping extracted.
    pub identity: MappedClaims,
    /// Every claim of the verified token.
    pub claims: serde_json::Value,
}

/// The human-login relying party: provider discovery with its cache, the
/// authorization request, and the code exchange with ID-token verification.
///
/// Clones share one cache.
#[derive(Clone)]
pub struct RelyingParty {
    /// Screened transport every provider request goes through.
    http: ScreenedHttp,
    /// Discovered providers by issuer, each with the key set it served.
    providers: Cache<String, Arc<ProviderMetadata>>,
}

impl std::fmt::Debug for RelyingParty {
    /// Prints the transport policy; cached providers are not printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelyingParty")
            .field("http", &self.http)
            .finish_non_exhaustive()
    }
}

impl RelyingParty {
    /// A relying party whose provider requests all go through `http`.
    #[must_use]
    pub fn new(http: ScreenedHttp) -> Self {
        Self {
            http,
            providers: Cache::builder()
                .max_capacity(MAX_CACHED_PROVIDERS)
                .time_to_live(PROVIDER_TTL)
                .build(),
        }
    }

    /// The screened transport provider requests go through.
    #[must_use]
    pub const fn http(&self) -> ScreenedHttp {
        self.http
    }

    /// The provider at `issuer`, from the cache when it holds one.
    ///
    /// A miss discovers through [`Self::discover`]; concurrent misses for one
    /// issuer each discover and the last one cached wins.
    ///
    /// # Errors
    /// Returns the errors of [`Self::discover`].
    pub async fn cached(
        &self,
        issuer: &IssuerUrl,
    ) -> Result<Arc<ProviderMetadata>, RelyingPartyError> {
        match self.providers.get(issuer.as_str()).await {
            Some(provider) => Ok(provider),
            None => self.discover(issuer).await,
        }
    }

    /// Discover `issuer` now and replace its cached entry.
    ///
    /// `openidconnect` fetches `{issuer}/.well-known/openid-configuration`,
    /// requires the document's `issuer` to equal `issuer` exactly, and then
    /// fetches the key set its `jwks_uri` names. Both requests go through the
    /// screened transport.
    ///
    /// # Errors
    /// Returns [`RelyingPartyError::Screened`] when screening refuses the
    /// issuer or key-set URL, [`RelyingPartyError::IssuerMismatch`] when the
    /// document names another issuer, and
    /// [`RelyingPartyError::DiscoveryUnavailable`] when either request fails or
    /// its response does not decode. A failure leaves any cached entry as it
    /// was.
    pub async fn discover(
        &self,
        issuer: &IssuerUrl,
    ) -> Result<Arc<ProviderMetadata>, RelyingPartyError> {
        let issuer_url = openidconnect::IssuerUrl::new(issuer.as_str().to_owned())
            .map_err(|error| RelyingPartyError::DiscoveryUnavailable(error.to_string()))?;
        let provider = ProviderMetadata::discover_async(issuer_url, &self.http)
            .await
            .map_err(discovery_error)?;
        let provider = Arc::new(provider);
        self.providers
            .insert(issuer.as_str().to_owned(), Arc::clone(&provider))
            .await;
        Ok(provider)
    }

    /// Build the authorization request for one login attempt at `provider`.
    ///
    /// `openidconnect` generates the `state`, the `nonce`, and the PKCE `S256`
    /// challenge and verifier, and requests the `openid profile email`
    /// scopes. The discovered authorization endpoint must first pass this
    /// deployment's scheme screen, so production never sends those values to
    /// a browser over cleartext.
    ///
    /// # Errors
    /// Returns [`RelyingPartyError::Screened`] when the authorization
    /// endpoint's scheme is refused and [`RelyingPartyError::Configuration`]
    /// when `redirect_uri` is not a URL.
    pub fn authorize(
        &self,
        provider: &ProviderMetadata,
        client_id: &str,
        redirect_uri: &str,
    ) -> Result<Authorization, RelyingPartyError> {
        self.http
            .screen_scheme(provider.authorization_endpoint().url())
            .map_err(RelyingPartyError::Screened)?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state, nonce) = client(provider, client_id, None)
            .set_redirect_uri(redirect_url(redirect_uri)?)
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                || CsrfToken::new_random_len(RANDOM_VALUE_BYTES),
                || Nonce::new_random_len(RANDOM_VALUE_BYTES),
            )
            .add_scope(Scope::new("profile".to_owned()))
            .add_scope(Scope::new("email".to_owned()))
            .set_pkce_challenge(challenge)
            .url();
        Ok(Authorization {
            url,
            state: state.into_secret(),
            nonce: nonce.secret().to_owned(),
            code_verifier: SecretString::from(verifier.into_secret()),
        })
    }

    /// Exchange `code` at `provider`'s token endpoint and verify the ID token
    /// it returns.
    ///
    /// `openidconnect` sends the code with the recorded redirect URI and PKCE
    /// verifier, authenticating as the connection's client, and decodes the
    /// response. The ID token is then verified against `provider`'s key set
    /// by [`Self::verify`]. When that names a key the set does not hold, the
    /// issuer is discovered once more and the token is verified once against
    /// the fresh set; it is never retried again.
    ///
    /// # Errors
    /// Returns [`RelyingPartyError::DiscoveryUnavailable`] when the provider
    /// advertised no token endpoint, [`RelyingPartyError::Configuration`]
    /// when the client authentication is `private_key_jwt`,
    /// [`RelyingPartyError::Screened`] or
    /// [`RelyingPartyError::TokenEndpointUnavailable`] when the token
    /// endpoint is refused, unreachable, or answers with a server error,
    /// [`RelyingPartyError::TokenRejected`] when it refuses the code or its
    /// response carries no decodable ID token, the errors of
    /// [`Self::discover`] for the re-discovery, and the errors of
    /// [`Self::verify`]. Cancellation can leave the remote exchange outcome
    /// unknown; nothing local is persisted.
    pub async fn redeem(
        &self,
        issuer: &IssuerUrl,
        provider: Arc<ProviderMetadata>,
        code: SecretString,
        redemption: &CodeRedemption<'_>,
    ) -> Result<VerifiedIdToken, RelyingPartyError> {
        let secret = match redemption.client_auth {
            ClientAuth::SecretBasic(secret) | ClientAuth::SecretPost(secret) => {
                Some(ClientSecret::new(secret.expose_secret().to_owned()))
            }
            ClientAuth::Public => None,
            ClientAuth::PrivateKeyJwt => {
                return Err(RelyingPartyError::Configuration(
                    "private_key_jwt client authentication is not supported".to_owned(),
                ));
            }
        };
        let auth_type = match redemption.client_auth {
            ClientAuth::SecretBasic(_) => AuthType::BasicAuth,
            _ => AuthType::RequestBody,
        };
        let response = client(&provider, redemption.client_id, secret)
            .set_auth_type(auth_type)
            .set_redirect_uri(redirect_url(redemption.redirect_uri)?)
            .exchange_code(AuthorizationCode::new(code.expose_secret().to_owned()))
            .map_err(|error| RelyingPartyError::DiscoveryUnavailable(error.to_string()))?
            .set_pkce_verifier(PkceCodeVerifier::new(
                redemption.code_verifier.expose_secret().to_owned(),
            ))
            .request_async(&self.http)
            .await
            .map_err(token_error)?;
        let id_token = response.id_token().ok_or_else(|| {
            RelyingPartyError::TokenRejected("the token response carries no ID token".to_owned())
        })?;
        match Self::verify(&provider, id_token, redemption) {
            Err(RelyingPartyError::UnknownKey) => {
                tracing::info!(
                    issuer = issuer.as_str(),
                    "ID token names an unknown key; rediscovering once"
                );
                let fresh = self.discover(issuer).await?;
                Self::verify(&fresh, id_token, redemption)
            }
            outcome => outcome,
        }
    }

    /// Verify `id_token` from `provider` for `redemption` and map its claims.
    ///
    /// `openidconnect` checks the issuer exactly, the audience, a signing
    /// algorithm the provider advertised that is not symmetric, the signature
    /// against the provider's key set, the expiry, and the nonce. This adds
    /// what the library leaves to its caller: the authorized-party rules of
    /// OpenID Connect Core 1.0 §3.1.3.7 steps 4 and 5 (several audiences need
    /// an `azp` naming the client, and a present `azp` must name it), an
    /// issued-at no later than now plus [`CLOCK_SKEW`] (step 10), and a
    /// Subject Identifier of at most 255 ASCII bytes (§2). The verified claims
    /// are mapped through the connection's claim mapping.
    ///
    /// # Errors
    /// Returns [`RelyingPartyError::InvalidNonce`] for a missing or
    /// mismatched nonce, [`RelyingPartyError::UnknownKey`] when no key in the
    /// provider's set matches the token, and
    /// [`RelyingPartyError::InvalidIdToken`] for every other refusal.
    fn verify(
        provider: &ProviderMetadata,
        id_token: &openidconnect::IdToken<
            OtherClaims,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
        >,
        redemption: &CodeRedemption<'_>,
    ) -> Result<VerifiedIdToken, RelyingPartyError> {
        let verifier = id_token_verifier(provider, redemption.audience);
        let nonce = Nonce::new(redemption.nonce.to_owned());
        let claims = id_token.claims(&verifier, &nonce).map_err(claims_error)?;
        verify_authorized_party(claims, redemption.client_id)?;
        let subject = claims.subject().as_str();
        if subject.is_empty() || !subject.is_ascii() || subject.len() > MAX_SUBJECT_BYTES {
            return Err(RelyingPartyError::InvalidIdToken(
                "the subject is not an OpenID Connect Subject Identifier".to_owned(),
            ));
        }
        let claims = serde_json::to_value(claims)
            .map_err(|error| RelyingPartyError::InvalidIdToken(error.to_string()))?;
        let identity = map_claims(redemption.claim_mapping, &claims)
            .map_err(|error| RelyingPartyError::InvalidIdToken(error.to_string()))?;
        Ok(VerifiedIdToken { identity, claims })
    }
}

/// An `openidconnect` client for `client_id` at `provider`.
fn client(
    provider: &ProviderMetadata,
    client_id: &str,
    secret: Option<ClientSecret>,
) -> RelyingPartyClient {
    RelyingPartyClient::from_provider_metadata(
        provider.clone(),
        ClientId::new(client_id.to_owned()),
        secret,
    )
}

/// Parse a recorded or configured redirect URI.
///
/// # Errors
/// Returns [`RelyingPartyError::Configuration`] when it is not a URL.
fn redirect_url(redirect_uri: &str) -> Result<RedirectUrl, RelyingPartyError> {
    RedirectUrl::new(redirect_uri.to_owned())
        .map_err(|error| RelyingPartyError::Configuration(error.to_string()))
}

/// The ID-token verifier for `audience` at `provider`.
///
/// A public-client verifier, so symmetric algorithms are refused even when
/// advertised; only algorithms the provider advertised for ID tokens are
/// accepted. Additional audiences are left to [`verify_authorized_party`].
/// Expiry and issued-at are checked with [`CLOCK_SKEW`].
fn id_token_verifier<'a>(
    provider: &ProviderMetadata,
    audience: &str,
) -> IdTokenVerifier<'a, CoreJsonWebKey> {
    let keys = JsonWebKeySet::new(provider.jwks().keys().clone());
    IdTokenVerifier::new_public_client(
        ClientId::new(audience.to_owned()),
        provider.issuer().clone(),
        keys,
    )
    .set_allowed_algs(provider.id_token_signing_alg_values_supported().clone())
    .set_other_audience_verifier_fn(|_| true)
    .set_time_fn(|| Utc::now() - CLOCK_SKEW)
    .set_issue_time_verifier_fn(|issued_at| {
        if issued_at > Utc::now() + CLOCK_SKEW {
            Err("the ID token was issued in the future".to_owned())
        } else {
            Ok(())
        }
    })
}

/// Apply OpenID Connect Core 1.0 §3.1.3.7 steps 4 and 5 for `client_id`.
///
/// # Errors
/// Returns [`RelyingPartyError::InvalidIdToken`] when the token names several
/// audiences without an `azp`, or an `azp` other than `client_id`.
fn verify_authorized_party(
    claims: &IdTokenClaims<OtherClaims, CoreGenderClaim>,
    client_id: &str,
) -> Result<(), RelyingPartyError> {
    match claims.authorized_party() {
        Some(party) if party.as_str() == client_id => Ok(()),
        None if claims.audiences().len() <= 1 => Ok(()),
        _ => Err(RelyingPartyError::InvalidIdToken(
            "the authorized party does not name the client".to_owned(),
        )),
    }
}

/// Classify a discovery failure.
fn discovery_error(error: DiscoveryError<ProviderHttpError>) -> RelyingPartyError {
    match error {
        DiscoveryError::Request(ProviderHttpError::Screened(error)) => {
            RelyingPartyError::Screened(error)
        }
        DiscoveryError::Validation(_) => RelyingPartyError::IssuerMismatch,
        error => RelyingPartyError::DiscoveryUnavailable(error_chain(&error)),
    }
}

/// Classify a token-endpoint failure.
fn token_error(
    error: RequestTokenError<ProviderHttpError, StandardErrorResponse<CoreErrorResponseType>>,
) -> RelyingPartyError {
    match error {
        RequestTokenError::Request(ProviderHttpError::Screened(error)) => {
            RelyingPartyError::Screened(error)
        }
        RequestTokenError::Request(error) => {
            RelyingPartyError::TokenEndpointUnavailable(error.to_string())
        }
        error => RelyingPartyError::TokenRejected(error_chain(&error)),
    }
}

/// Classify an ID-token verification failure.
fn claims_error(error: ClaimsVerificationError) -> RelyingPartyError {
    match error {
        ClaimsVerificationError::InvalidNonce(_) => RelyingPartyError::InvalidNonce,
        ClaimsVerificationError::SignatureVerification(
            SignatureVerificationError::NoMatchingKey,
        ) => RelyingPartyError::UnknownKey,
        error => RelyingPartyError::InvalidIdToken(error.to_string()),
    }
}

/// `error` and its sources, joined for one log line.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// The relying party against a loopback mock provider: discovery and its
/// screening, the authorization request, the code exchange, ID-token
/// refusals, and the single re-discovery on an unknown key.
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use secrecy::SecretString;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_spec::auth::IssuerUrl;

    use super::{CodeRedemption, RelyingParty, RelyingPartyError, VerifiedIdToken};
    use crate::registry::{ClaimMapping, ClaimPath, ClientAuth};
    use crate::screening::{AddressPolicy, ScreenError, ScreenedHttp};

    /// Ed25519 key the mock provider signs with.
    const SIGNING_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    /// Public half of [`SIGNING_KEY`] as a JWK `x`.
    const SIGNING_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";
    /// An Ed25519 key the mock provider does not publish.
    const FOREIGN_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIPNZb/xl9U5jhHGkOTPVsxugPf3cN1/NZDDcvMG8Vczw\n-----END PRIVATE KEY-----\n";
    /// Client id the tests redeem as.
    const CLIENT: &str = "wyrd-client";
    /// Nonce the tests' logins recorded.
    const NONCE: &str = "login-nonce";
    /// Discovery path every mock provider serves.
    const DISCOVERY: &str = "/.well-known/openid-configuration";

    /// The discovery document of a provider at `issuer`.
    fn discovery(issuer: &str, jwks_uri: &str) -> Value {
        json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/authorize"),
            "token_endpoint": format!("{issuer}/token"),
            "jwks_uri": jwks_uri,
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["EdDSA"],
        })
    }

    /// A key set publishing [`SIGNING_KEY`] under `kid`.
    fn key_set(kid: &str) -> Value {
        json!({ "keys": [{ "kty": "OKP", "crv": "Ed25519", "kid": kid, "x": SIGNING_X }] })
    }

    /// Start a mock provider serving `document` and `keys`, returning it and
    /// its issuer.
    ///
    /// # Panics
    /// Panics when the mock URI is not a valid loopback issuer.
    async fn provider_with(
        document: impl Fn(&str) -> Value,
        keys: Value,
    ) -> (MockServer, IssuerUrl) {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path(DISCOVERY))
            .respond_with(ResponseTemplate::new(200).set_body_json(document(&issuer)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(keys))
            .mount(&server)
            .await;
        (
            server,
            IssuerUrl::new(issuer).expect("loopback issuer is valid"),
        )
    }

    /// A mock provider publishing [`SIGNING_KEY`] as `mock-1`.
    async fn provider() -> (MockServer, IssuerUrl) {
        provider_with(
            |issuer| discovery(issuer, &format!("{issuer}/jwks")),
            key_set("mock-1"),
        )
        .await
    }

    /// Answer the token endpoint of `server` with `response`.
    async fn token_endpoint(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    /// Valid ID-token claims from `issuer` for [`CLIENT`] and [`NONCE`].
    fn claims(issuer: &IssuerUrl) -> Value {
        let now = chrono::Utc::now().timestamp();
        json!({
            "iss": issuer.as_str(),
            "sub": "user-1",
            "aud": CLIENT,
            "exp": now + 3600,
            "iat": now,
            "nonce": NONCE,
            "email": "user@example.com",
            "groups": { "names": ["admins"] },
        })
    }

    /// Sign `claims` with `pem` under `kid` as EdDSA.
    ///
    /// # Panics
    /// Panics when `pem` is not an Ed25519 private key.
    fn signed(claims: &Value, pem: &str, kid: &str) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(kid.to_owned());
        let key = EncodingKey::from_ed_pem(pem.as_bytes()).expect("test key parses");
        jsonwebtoken::encode(&header, claims, &key).expect("token signs")
    }

    /// A token endpoint answer carrying `id_token`.
    fn token_reply(id_token: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "provider-access",
            "token_type": "Bearer",
            "id_token": id_token,
        }))
    }

    /// The claim mapping the tests verify through.
    fn mapping() -> ClaimMapping {
        ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: Some(ClaimPath::new("email")),
            groups: Some(ClaimPath::new("groups.names")),
        }
    }

    /// Discover `issuer` and redeem a code for [`CLIENT`] with [`NONCE`].
    ///
    /// # Errors
    /// Returns the refusal of discovery or of [`RelyingParty::redeem`].
    async fn redeem(
        party: &RelyingParty,
        issuer: &IssuerUrl,
    ) -> Result<VerifiedIdToken, RelyingPartyError> {
        let provider = party.discover(issuer).await?;
        let mapping = mapping();
        let verifier = SecretString::from("code-verifier".to_owned());
        party
            .redeem(
                issuer,
                provider,
                SecretString::from("code".to_owned()),
                &CodeRedemption {
                    client_id: CLIENT,
                    client_auth: &ClientAuth::Public,
                    audience: CLIENT,
                    claim_mapping: &mapping,
                    redirect_uri: "https://wyrd.example.com/auth/callback",
                    code_verifier: &verifier,
                    nonce: NONCE,
                },
            )
            .await
    }

    /// Requests `server` received at `route`.
    ///
    /// # Panics
    /// Panics when the mock server does not record requests.
    async fn hits(server: &MockServer, route: &str) -> usize {
        server
            .received_requests()
            .await
            .expect("requests are recorded")
            .iter()
            .filter(|request| request.url.path() == route)
            .count()
    }

    /// The authorization request carries library-generated state, nonce, and
    /// an `S256` PKCE challenge; production refuses a cleartext endpoint.
    #[tokio::test]
    async fn the_authorization_request_carries_state_nonce_and_pkce() {
        let (_server, issuer) = provider().await;
        let party = RelyingParty::new(ScreenedHttp::allowing_internal());
        let provider = party.cached(&issuer).await.expect("provider discovers");

        let authorization = party
            .authorize(&provider, CLIENT, "https://wyrd.example.com/auth/callback")
            .expect("authorization builds");

        let query: std::collections::HashMap<String, String> =
            authorization.url.query_pairs().into_owned().collect();
        assert_eq!(query["state"], authorization.state);
        assert_eq!(query["nonce"], authorization.nonce);
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["client_id"], CLIENT);
        assert_eq!(query["scope"], "openid profile email");
        assert_eq!(query["response_type"], "code");
        let refused = RelyingParty::new(ScreenedHttp::new(AddressPolicy::BlockInternal))
            .authorize(&provider, CLIENT, "https://wyrd.example.com/auth/callback")
            .expect_err("production refuses a cleartext authorization endpoint");
        assert!(
            matches!(refused, RelyingPartyError::Screened(ScreenError::Blocked)),
            "{refused:?}"
        );
    }

    /// A valid code exchange returns the mapped identity.
    #[tokio::test]
    async fn a_valid_id_token_maps_its_identity() {
        let (server, issuer) = provider().await;
        token_endpoint(
            &server,
            token_reply(&signed(&claims(&issuer), SIGNING_KEY, "mock-1")),
        )
        .await;

        let verified = redeem(
            &RelyingParty::new(ScreenedHttp::allowing_internal()),
            &issuer,
        )
        .await
        .expect("the token verifies");

        assert_eq!(verified.identity.subject, "user-1");
        assert_eq!(verified.identity.email.as_deref(), Some("user@example.com"));
        assert_eq!(verified.identity.groups, vec!["admins".to_owned()]);
        assert_eq!(verified.claims["nonce"], NONCE);
    }

    /// Reset `server` to a provider at `issuer` publishing `mock-1` whose
    /// token endpoint answers with `id_token`.
    async fn serve_token(server: &MockServer, issuer: &IssuerUrl, id_token: &str) {
        server.reset().await;
        let issuer = issuer.as_str();
        Mock::given(method("GET"))
            .and(path(DISCOVERY))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(discovery(issuer, &format!("{issuer}/jwks"))),
            )
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(key_set("mock-1")))
            .mount(server)
            .await;
        token_endpoint(server, token_reply(id_token)).await;
    }

    /// Nonce, issuer, audience, algorithm, signature, expiry, issued-at,
    /// authorized-party, and subject refusals all fail closed; several
    /// audiences with an `azp` naming the client verify.
    #[tokio::test]
    async fn id_token_refusals_fail_closed() {
        let (server, issuer) = provider().await;
        let party = RelyingParty::new(ScreenedHttp::allowing_internal());
        let base = claims(&issuer);
        let with = |field: &str, value: Value| {
            let mut claims = base.clone();
            claims[field] = value;
            claims
        };
        let now = chrono::Utc::now().timestamp();
        let mut hs256 = Header::new(Algorithm::HS256);
        hs256.kid = Some("mock-1".to_owned());
        let hmac = jsonwebtoken::encode(
            &hs256,
            &base,
            &EncodingKey::from_secret(SIGNING_X.as_bytes()),
        )
        .expect("token signs");
        let ed = |claims: &Value| signed(claims, SIGNING_KEY, "mock-1");
        let cases = [
            (
                "issuer",
                ed(&with("iss", json!("https://evil.example.com"))),
            ),
            ("audience", ed(&with("aud", json!("other-client")))),
            ("algorithm", hmac),
            ("signature", signed(&base, FOREIGN_KEY, "mock-1")),
            ("expiry", ed(&with("exp", json!(now - 3600)))),
            ("future iat", ed(&with("iat", json!(now + 3600)))),
            (
                "multi-audience without azp",
                ed(&with("aud", json!([CLIENT, "other"]))),
            ),
            (
                "azp naming another client",
                ed(&with("azp", json!("other"))),
            ),
            ("subject too long", ed(&with("sub", json!("s".repeat(256))))),
        ];
        for (label, id_token) in cases {
            serve_token(&server, &issuer, &id_token).await;
            let error = redeem(&party, &issuer).await.expect_err(label);
            assert!(
                matches!(error, RelyingPartyError::InvalidIdToken(_)),
                "{label}: {error:?}"
            );
        }
        serve_token(&server, &issuer, &ed(&with("nonce", json!("other")))).await;
        let error = redeem(&party, &issuer).await.expect_err("nonce");
        assert!(
            matches!(error, RelyingPartyError::InvalidNonce),
            "{error:?}"
        );
        let mut missing_iat = base.clone();
        missing_iat
            .as_object_mut()
            .expect("claims object")
            .remove("iat");
        serve_token(&server, &issuer, &ed(&missing_iat)).await;
        let error = redeem(&party, &issuer).await.expect_err("missing iat");
        assert!(
            matches!(error, RelyingPartyError::TokenRejected(_)),
            "{error:?}"
        );

        let mut with_azp = with("aud", json!([CLIENT, "other"]));
        with_azp["azp"] = json!(CLIENT);
        serve_token(&server, &issuer, &ed(&with_azp)).await;
        redeem(&party, &issuer)
            .await
            .expect("several audiences with azp naming the client verify");
    }

    /// A validly signed token whose asymmetric algorithm the provider did not
    /// advertise for ID tokens is refused.
    #[tokio::test]
    async fn an_unadvertised_signing_algorithm_is_refused() {
        let (server, issuer) = provider_with(
            |issuer| {
                let mut document = discovery(issuer, &format!("{issuer}/jwks"));
                document["id_token_signing_alg_values_supported"] = json!(["RS256"]);
                document
            },
            key_set("mock-1"),
        )
        .await;
        token_endpoint(
            &server,
            token_reply(&signed(&claims(&issuer), SIGNING_KEY, "mock-1")),
        )
        .await;

        let error = redeem(
            &RelyingParty::new(ScreenedHttp::allowing_internal()),
            &issuer,
        )
        .await
        .expect_err("an unadvertised algorithm is refused");

        assert!(
            matches!(error, RelyingPartyError::InvalidIdToken(_)),
            "{error:?}"
        );
    }

    /// A token naming a key the discovered set lacks re-discovers exactly
    /// once and verifies against the rotated set.
    #[tokio::test]
    async fn an_unknown_key_rediscovers_exactly_once() {
        let server = MockServer::start().await;
        let issuer = IssuerUrl::new(server.uri()).expect("loopback issuer is valid");
        Mock::given(method("GET"))
            .and(path(DISCOVERY))
            .respond_with(ResponseTemplate::new(200).set_body_json(discovery(
                issuer.as_str(),
                &format!("{}/jwks", issuer.as_str()),
            )))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(key_set("old")))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(key_set("rotated")))
            .mount(&server)
            .await;
        token_endpoint(
            &server,
            token_reply(&signed(&claims(&issuer), SIGNING_KEY, "rotated")),
        )
        .await;

        redeem(
            &RelyingParty::new(ScreenedHttp::allowing_internal()),
            &issuer,
        )
        .await
        .expect("the rotated key verifies after one re-discovery");

        assert_eq!(
            hits(&server, DISCOVERY).await,
            2,
            "one discovery plus one re-discovery"
        );
        assert_eq!(hits(&server, "/jwks").await, 2);
    }

    /// A key still unknown after the one re-discovery is refused, and no
    /// further discovery is attempted.
    #[tokio::test]
    async fn a_still_unknown_key_fails_after_one_rediscovery() {
        let (server, issuer) = provider().await;
        token_endpoint(
            &server,
            token_reply(&signed(&claims(&issuer), SIGNING_KEY, "never")),
        )
        .await;

        let error = redeem(
            &RelyingParty::new(ScreenedHttp::allowing_internal()),
            &issuer,
        )
        .await
        .expect_err("an unknown key is refused");

        assert!(matches!(error, RelyingPartyError::UnknownKey), "{error:?}");
        assert_eq!(hits(&server, DISCOVERY).await, 2);
        assert_eq!(hits(&server, "/jwks").await, 2);
    }

    /// Production screening refuses a loopback issuer before any request.
    #[tokio::test]
    async fn an_unsafe_issuer_is_refused_before_any_request() {
        let (server, issuer) = provider().await;

        let error = RelyingParty::new(ScreenedHttp::new(AddressPolicy::BlockInternal))
            .discover(&issuer)
            .await
            .expect_err("a loopback issuer is refused in production");

        assert!(
            matches!(error, RelyingPartyError::Screened(ScreenError::Blocked)),
            "{error:?}"
        );
        assert!(
            server
                .received_requests()
                .await
                .expect("recorded")
                .is_empty()
        );
    }

    /// A discovered key-set URL at the metadata address is refused before it
    /// is requested.
    #[tokio::test]
    async fn an_unsafe_key_set_url_is_refused_before_any_request() {
        let (server, issuer) = provider_with(
            |issuer| discovery(issuer, "http://169.254.169.254/jwks"),
            key_set("mock-1"),
        )
        .await;

        let error = RelyingParty::new(ScreenedHttp::allowing_internal())
            .discover(&issuer)
            .await
            .expect_err("the metadata address is refused");

        assert!(
            matches!(error, RelyingPartyError::Screened(ScreenError::Blocked)),
            "{error:?}"
        );
        assert_eq!(hits(&server, DISCOVERY).await, 1);
        assert_eq!(hits(&server, "/jwks").await, 0);
    }

    /// A token endpoint answering `307` or `308` toward a second origin is
    /// refused and the second origin receives nothing.
    #[tokio::test]
    async fn a_redirecting_token_endpoint_is_refused_without_following() {
        let elsewhere = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&elsewhere)
            .await;
        for status in [307, 308] {
            let (server, issuer) = provider().await;
            token_endpoint(
                &server,
                ResponseTemplate::new(status)
                    .insert_header("location", format!("{}/token", elsewhere.uri()).as_str()),
            )
            .await;

            let error = redeem(
                &RelyingParty::new(ScreenedHttp::allowing_internal()),
                &issuer,
            )
            .await
            .expect_err("a redirect is refused");

            assert!(
                matches!(error, RelyingPartyError::TokenRejected(_)),
                "{status}: {error:?}"
            );
        }
        assert!(
            elsewhere
                .received_requests()
                .await
                .expect("recorded")
                .is_empty()
        );
    }

    /// A provider outage fails closed: discovery and the token endpoint each
    /// answering `503` are reported unavailable.
    #[tokio::test]
    async fn a_provider_outage_fails_closed() {
        let down = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&down)
            .await;
        let issuer = IssuerUrl::new(down.uri()).expect("loopback issuer is valid");
        let party = RelyingParty::new(ScreenedHttp::allowing_internal());
        let error = party
            .discover(&issuer)
            .await
            .expect_err("discovery is down");
        assert!(
            matches!(error, RelyingPartyError::DiscoveryUnavailable(_)),
            "{error:?}"
        );

        let (server, issuer) = provider().await;
        token_endpoint(&server, ResponseTemplate::new(503)).await;
        let error = redeem(&party, &issuer)
            .await
            .expect_err("the token endpoint is down");
        assert!(
            matches!(error, RelyingPartyError::TokenEndpointUnavailable(_)),
            "{error:?}"
        );
    }

    /// A discovery document naming another issuer is an issuer mismatch.
    #[tokio::test]
    async fn a_mismatched_issuer_is_refused() {
        let (_server, issuer) = provider_with(
            |_| {
                discovery(
                    "https://other.example.com",
                    "https://other.example.com/jwks",
                )
            },
            key_set("mock-1"),
        )
        .await;

        let error = RelyingParty::new(ScreenedHttp::allowing_internal())
            .discover(&issuer)
            .await
            .expect_err("the mismatch is refused");

        assert!(
            matches!(error, RelyingPartyError::IssuerMismatch),
            "{error:?}"
        );
    }

    /// The cache serves a discovered provider without another request.
    #[tokio::test]
    async fn the_cache_serves_a_discovered_provider() {
        let (server, issuer) = provider().await;
        let party = RelyingParty::new(ScreenedHttp::allowing_internal());

        let first = party.cached(&issuer).await.expect("provider discovers");
        let second = party
            .clone()
            .cached(&issuer)
            .await
            .expect("provider is cached");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(hits(&server, DISCOVERY).await, 1);
    }
}
