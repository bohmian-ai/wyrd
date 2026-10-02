//! The OAuth 2.0 wire shared by the authorization-server endpoints.
//!
//! `POST /auth/token`, `POST /auth/platform/token`,
//! `POST /auth/device_authorization`, and `POST /auth/revoke` take
//! `application/x-www-form-urlencoded` parameters ([`OAuthForm`]), identify
//! their client through [`OAuthClients`], and answer the RFC 6749 §5.1
//! success or the §5.2 error ([`OAuthError`]) with `Cache-Control: no-store`.
//! They are the one exception to the `WyrdError` problem-json surface; a
//! refusal is still logged under its Wyrd error code.

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use base64::Engine;
use serde::de::DeserializeOwned;
use url::form_urlencoded;
use wyrd_spec::auth::{
    OAuthClientId, OAuthErrorCode, OAuthErrorResponse, Sha256Hex, TokenAudience, TokenRequest,
};
use wyrd_spec::error::WyrdError;

/// The RFC 8693 token-exchange `grant_type`.
pub const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

/// Every `grant_type` `POST /auth/token` accepts.
pub const GRANT_TYPES: [&str; 5] = [
    "authorization_code",
    "refresh_token",
    "urn:ietf:params:oauth:grant-type:device_code",
    TOKEN_EXCHANGE,
    "urn:ietf:params:oauth:grant-type:jwt-bearer",
];

/// The complete form an OAuth client endpoint documents: the endpoint's own
/// parameters plus the public client's `client_id` (RFC 6749 §2.3.1, §3.2.1).
///
/// It exists for the served contract only. Requests are read by
/// [`OAuthForm`] and the client is identified by [`OAuthClients`]; a
/// confidential client sends HTTP Basic (RFC 7617) instead of `client_id`,
/// which the operation declares as its alternative security requirement.
#[derive(utoipa::ToSchema)]
pub struct ClientForm<T> {
    /// The endpoint's own parameters.
    #[serde(flatten)]
    pub params: T,
    /// The public client's identifier. A confidential client omits it and
    /// authenticates with HTTP Basic instead.
    pub client_id: Option<OAuthClientId>,
}

/// The RFC 6749 §5.2 refusal of one OAuth endpoint request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OAuthError(
    /// The registered OAuth error code the response body carries; it also
    /// selects the HTTP status ([`Self::status`]).
    pub OAuthErrorCode,
);

impl OAuthError {
    /// The HTTP status RFC 6749 §5.2 gives `self`: `401` for
    /// `invalid_client`, `500` for `server_error`, `503` for
    /// `temporarily_unavailable`, and `400` otherwise.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self.0 {
            OAuthErrorCode::InvalidClient => StatusCode::UNAUTHORIZED,
            OAuthErrorCode::ServerError => StatusCode::INTERNAL_SERVER_ERROR,
            OAuthErrorCode::TemporarilyUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::BAD_REQUEST,
        }
    }

    /// The refusal of a request whose unusable parameter is
    /// `invalid_request` where a grant reports `invalid_grant`: a token
    /// exchange's subject or actor token (RFC 8693 §2.2.2) and a device
    /// authorization's tenant.
    #[must_use]
    pub fn request(error: WyrdError) -> Self {
        match Self::from(error) {
            Self(OAuthErrorCode::InvalidGrant) => Self(OAuthErrorCode::InvalidRequest),
            other => other,
        }
    }
}

impl OAuthError {
    /// The RFC 6749 §4.1.2.1 error a refused authorization request redirects
    /// back with: a server failure keeps its code and every other refusal of
    /// the sign-in is `access_denied`.
    #[must_use]
    pub fn authorization(error: WyrdError) -> OAuthErrorCode {
        match Self::from(error).0 {
            code @ (OAuthErrorCode::ServerError | OAuthErrorCode::TemporarilyUnavailable) => code,
            _ => OAuthErrorCode::AccessDenied,
        }
    }
}

impl From<WyrdError> for OAuthError {
    /// Map a catalog refusal onto its OAuth error code and log its Wyrd code.
    ///
    /// A device-code refusal carries its RFC 8628 §3.5 code in
    /// `details.error`; an unsupported grant is `unsupported_grant_type`; the
    /// rest map by status: an unusable credential (`401`, `403`, `404`) is
    /// `invalid_grant`, any other `4xx` `invalid_request`, `503`
    /// `temporarily_unavailable`, and anything else `server_error`.
    fn from(error: WyrdError) -> Self {
        let code = match &error {
            WyrdError::DeviceAuthorization { details, .. } => {
                serde_json::from_value(details["error"].clone())
                    .unwrap_or(OAuthErrorCode::InvalidGrant)
            }
            WyrdError::UnsupportedGrantType { .. } => OAuthErrorCode::UnsupportedGrantType,
            other => match other.status() {
                401 | 403 | 404 => OAuthErrorCode::InvalidGrant,
                400..=499 => OAuthErrorCode::InvalidRequest,
                503 => OAuthErrorCode::TemporarilyUnavailable,
                _ => OAuthErrorCode::ServerError,
            },
        };
        tracing::info!(wyrd_code = error.code(), oauth_error = ?code, %error, "oauth request refused");
        Self(code)
    }
}

impl IntoResponse for OAuthError {
    /// Render the RFC 6749 §5.2 JSON body with `no-store` caching, adding the
    /// `Basic` challenge to an `invalid_client` refusal.
    fn into_response(self) -> Response {
        let mut response = no_store(
            self.status(),
            Json(OAuthErrorResponse {
                error: self.0,
                error_description: None,
            }),
        );
        if self.0 == OAuthErrorCode::InvalidClient {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"wyrd\""),
            );
        }
        response
    }
}

/// `body` with `status` and the RFC 6749 §5.1 `Cache-Control: no-store` and
/// `Pragma: no-cache` headers every token-bearing or error response carries.
pub fn no_store(status: StatusCode, body: impl IntoResponse) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::PRAGMA, HeaderValue::from_static("no-cache")),
        ],
        body,
    )
        .into_response()
}

/// Form-encoded OAuth request parameters with their `client_id`.
///
/// Parameters without a value are treated as omitted (RFC 6749 §3.1) and
/// parameters a request type does not define are ignored (§3.2). A body that
/// is not `application/x-www-form-urlencoded`, repeats a parameter, or does
/// not decode is refused with `invalid_request`.
#[derive(Debug)]
pub struct OAuthForm {
    /// Every parameter by name.
    params: serde_json::Map<String, serde_json::Value>,
}

impl OAuthForm {
    /// Parse `raw` form-encoded parameters.
    ///
    /// # Errors
    /// Returns `invalid_request` when a parameter is repeated.
    pub fn parse(raw: &[u8]) -> Result<Self, OAuthError> {
        let mut params = serde_json::Map::new();
        for (name, value) in form_urlencoded::parse(raw) {
            if value.is_empty() {
                continue;
            }
            if params
                .insert(name.into_owned(), value.into_owned().into())
                .is_some()
            {
                return Err(OAuthError(OAuthErrorCode::InvalidRequest));
            }
        }
        Ok(Self { params })
    }

    /// The value of the parameter `name`, if present.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.params.get(name).and_then(serde_json::Value::as_str)
    }

    /// Decode the parameters as `T`.
    ///
    /// # Errors
    /// Returns `invalid_request` when a required parameter is missing or a
    /// value is invalid.
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, OAuthError> {
        serde_json::from_value(serde_json::Value::Object(self.params.clone()))
            .map_err(|_| OAuthError(OAuthErrorCode::InvalidRequest))
    }

    /// Decode the parameters as a token-endpoint [`TokenRequest`].
    ///
    /// A token exchange whose `audience` is not a [`TokenAudience`] names a
    /// target this server does not issue for, so it is classified before the
    /// generic decode turns it into a malformed request.
    ///
    /// # Errors
    /// Returns RFC 8693 §2.2.2 `invalid_target` for a token exchange naming
    /// an unsupported `audience`, and the errors of [`Self::decode`].
    pub fn token_request(&self) -> Result<TokenRequest, OAuthError> {
        let exchange = self.get("grant_type") == Some(TOKEN_EXCHANGE);
        let unsupported_audience = self.params.get("audience").is_some_and(|audience| {
            serde_json::from_value::<TokenAudience>(audience.clone()).is_err()
        });
        if exchange && unsupported_audience {
            return Err(OAuthError(OAuthErrorCode::InvalidTarget));
        }
        self.decode()
    }
}

impl<S: Send + Sync> FromRequest<S> for OAuthForm {
    /// A refused body answers the RFC 6749 §5.2 `invalid_request`.
    type Rejection = OAuthError;

    /// Read the body of an `application/x-www-form-urlencoded` request and
    /// parse it with [`OAuthForm::parse`].
    ///
    /// # Errors
    /// Returns `invalid_request` when the `Content-Type` is missing or names
    /// another media type, the body cannot be read, or a parameter repeats.
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let form = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .is_some_and(|media| {
                media
                    .trim()
                    .eq_ignore_ascii_case("application/x-www-form-urlencoded")
            });
        if !form {
            return Err(OAuthError(OAuthErrorCode::InvalidRequest));
        }
        let body = Bytes::from_request(request, state)
            .await
            .map_err(|_| OAuthError(OAuthErrorCode::InvalidRequest))?;
        Self::parse(&body)
    }
}

/// The deployment's OAuth client registrations.
///
/// `wyrd-cli` is public and identifies itself with `client_id` alone.
/// `wyrd-ui` is confidential and authenticates with `client_secret_basic`
/// (RFC 6749 §2.3.1) against the configured SHA-256 digests of its secret:
/// one, or two during a rotation overlap. With no digest configured
/// `wyrd-ui` cannot authenticate.
#[derive(Debug, Clone, Default)]
pub struct OAuthClients {
    /// SHA-256 of each accepted `wyrd-ui` client secret.
    ui_secret_hashes: Vec<Sha256Hex>,
}

impl OAuthClients {
    /// Register `wyrd-ui` with these secret digests.
    #[must_use]
    pub fn new(ui_secret_hashes: Vec<Sha256Hex>) -> Self {
        Self { ui_secret_hashes }
    }

    /// The client a request identifies, if any.
    ///
    /// An `Authorization: Basic` header must carry `wyrd-ui` and an accepted
    /// secret, both form-urlencoded (RFC 6749 §2.3.1); a `client_id`
    /// parameter beside it must name the same client. Without the header the
    /// `client_id` parameter must name the public `wyrd-cli`.
    ///
    /// # Errors
    /// Returns `invalid_client` for a malformed or refused header, an unknown
    /// client, or `wyrd-ui` without its secret, and `invalid_request` for a
    /// `client_id` parameter that contradicts the header.
    pub fn identify(
        &self,
        headers: &HeaderMap,
        form: &OAuthForm,
    ) -> Result<Option<OAuthClientId>, OAuthError> {
        let invalid_client = OAuthError(OAuthErrorCode::InvalidClient);
        let Some(authorization) = headers.get(header::AUTHORIZATION) else {
            return match form.get("client_id").map(OAuthClientId::parse) {
                None => Ok(None),
                Some(Some(OAuthClientId::WyrdCli)) => Ok(Some(OAuthClientId::WyrdCli)),
                Some(_) => Err(invalid_client),
            };
        };
        let (client_id, secret) = authorization
            .to_str()
            .ok()
            .and_then(|value| value.split_once(' '))
            .and_then(|(scheme, credentials)| {
                scheme.eq_ignore_ascii_case("basic").then_some(credentials)
            })
            .and_then(|encoded| {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded.trim())
                    .ok()
            })
            .and_then(|decoded| String::from_utf8(decoded).ok())
            .and_then(|decoded| {
                let (id, secret) = decoded.split_once(':')?;
                Some((form_decoded(id), form_decoded(secret)))
            })
            .ok_or(invalid_client)?;
        if OAuthClientId::parse(&client_id) != Some(OAuthClientId::WyrdUi)
            || !self
                .ui_secret_hashes
                .contains(&Sha256Hex::digest(secret.as_bytes()))
        {
            return Err(invalid_client);
        }
        if form
            .get("client_id")
            .is_some_and(|param| param != client_id)
        {
            return Err(OAuthError(OAuthErrorCode::InvalidRequest));
        }
        Ok(Some(OAuthClientId::WyrdUi))
    }

    /// The client a request that must identify one names.
    ///
    /// # Errors
    /// The errors of [`Self::identify`], and `invalid_client` when the
    /// request identifies no client.
    pub fn require(
        &self,
        headers: &HeaderMap,
        form: &OAuthForm,
    ) -> Result<OAuthClientId, OAuthError> {
        self.identify(headers, form)?
            .ok_or(OAuthError(OAuthErrorCode::InvalidClient))
    }
}

/// Decode one form-urlencoded component, as RFC 6749 §2.3.1 encodes the
/// Basic credentials.
fn form_decoded(component: &str) -> String {
    form_urlencoded::parse(component.as_bytes())
        .next()
        .map(|(decoded, _)| decoded.into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};
    use base64::Engine;
    use wyrd_spec::auth::{OAuthClientId, OAuthErrorCode, Sha256Hex, TokenAudience, TokenRequest};
    use wyrd_spec::error::WyrdError;

    use super::{OAuthClients, OAuthError, OAuthForm};

    /// Headers carrying `Authorization: Basic` for `id` and `secret`.
    fn basic(id: &str, secret: &str) -> HeaderMap {
        basic_with_scheme("Basic", id, secret)
    }

    /// Headers carrying `Authorization: <scheme>` with the Base64 of
    /// `id:secret`.
    ///
    /// # Panics
    /// Panics when the header value is not valid.
    fn basic_with_scheme(scheme: &str, id: &str, secret: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{id}:{secret}"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("{scheme} {encoded}")).expect("header value"),
        );
        headers
    }

    /// The `Basic` scheme token matches in any case (RFC 9110 §11.1); another
    /// scheme, a missing separator, or a wrong secret stays `invalid_client`.
    ///
    /// # Panics
    /// Panics when a header authenticates differently.
    #[test]
    fn basic_scheme_matches_case_insensitively() {
        let clients = OAuthClients::new(vec![Sha256Hex::digest(b"s3cret")]);
        let empty = OAuthForm::parse(b"").expect("form parses");
        for scheme in ["Basic", "basic", "BASIC", "bAsIc"] {
            assert_eq!(
                clients.identify(&basic_with_scheme(scheme, "wyrd-ui", "s3cret"), &empty),
                Ok(Some(OAuthClientId::WyrdUi)),
                "{scheme}"
            );
        }
        let invalid = Err(OAuthError(OAuthErrorCode::InvalidClient));
        assert_eq!(
            clients.identify(&basic_with_scheme("basic", "wyrd-ui", "wrong"), &empty),
            invalid
        );
        assert_eq!(
            clients.identify(&basic_with_scheme("Bearer", "wyrd-ui", "s3cret"), &empty),
            invalid
        );
        let mut joined = HeaderMap::new();
        joined.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basicd3lyZC11aTpzM2NyZXQ="),
        );
        assert_eq!(clients.identify(&joined, &empty), invalid);
    }

    /// A repeated parameter is refused, an empty one is omitted.
    ///
    /// # Panics
    /// Panics when the form parses differently.
    #[test]
    fn repeated_parameters_are_refused_and_empty_ones_omitted() {
        assert_eq!(
            OAuthForm::parse(b"grant_type=a&grant_type=b").expect_err("repeat refused"),
            OAuthError(OAuthErrorCode::InvalidRequest)
        );
        let form = OAuthForm::parse(b"grant_type=refresh_token&scope=").expect("form parses");
        assert_eq!(form.get("grant_type"), Some("refresh_token"));
        assert_eq!(form.get("scope"), None);
    }

    /// A token exchange naming an unsupported `audience` is `invalid_target`
    /// (RFC 8693 §2.2.2); a malformed exchange stays `invalid_request`, and
    /// both supported audiences decode unchanged.
    ///
    /// # Panics
    /// Panics when a request is classified differently.
    #[test]
    fn token_exchange_audience_is_classified_before_decoding() {
        let exchange = "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
            &subject_token=s&subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
            &actor_token=a&actor_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token";
        let with = |audience: &str| {
            OAuthForm::parse(format!("{exchange}&audience={audience}").as_bytes())
                .expect("form parses")
                .token_request()
        };
        assert_eq!(
            with("https%3A%2F%2Fother.example.com"),
            Err(OAuthError(OAuthErrorCode::InvalidTarget))
        );
        for audience in [TokenAudience::Wyrd, TokenAudience::Bifrost] {
            assert!(matches!(
                with(audience.as_str()),
                Ok(TokenRequest::TokenExchange { audience: Some(decoded), .. }) if decoded == audience
            ));
        }
        let malformed = OAuthForm::parse(
            b"grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange&audience=wyrd",
        )
        .expect("form parses");
        assert_eq!(
            malformed.token_request(),
            Err(OAuthError(OAuthErrorCode::InvalidRequest))
        );
        let refresh = OAuthForm::parse(b"grant_type=refresh_token&refresh_token=r&audience=x")
            .expect("form parses");
        assert!(matches!(
            refresh.token_request(),
            Ok(TokenRequest::RefreshToken { .. })
        ));
    }

    /// `wyrd-ui` authenticates only with an accepted, form-urlencoded Basic
    /// secret; `wyrd-cli` identifies itself by `client_id`.
    ///
    /// # Panics
    /// Panics when a client is identified differently.
    #[test]
    fn clients_authenticate_by_registration() {
        let clients = OAuthClients::new(vec![Sha256Hex::digest(b"s3cret/+")]);
        let empty = OAuthForm::parse(b"").expect("form parses");
        assert_eq!(
            clients.identify(&basic("wyrd-ui", "s3cret%2F%2B"), &empty),
            Ok(Some(OAuthClientId::WyrdUi))
        );
        assert_eq!(
            clients.identify(&basic("wyrd-ui", "wrong"), &empty),
            Err(OAuthError(OAuthErrorCode::InvalidClient))
        );
        assert_eq!(
            clients.identify(&basic("wyrd-cli", "s3cret%2F%2B"), &empty),
            Err(OAuthError(OAuthErrorCode::InvalidClient))
        );
        let cli = OAuthForm::parse(b"client_id=wyrd-cli").expect("form parses");
        assert_eq!(
            clients.identify(&HeaderMap::new(), &cli),
            Ok(Some(OAuthClientId::WyrdCli))
        );
        assert_eq!(
            clients.identify(&basic("wyrd-ui", "s3cret%2F%2B"), &cli),
            Err(OAuthError(OAuthErrorCode::InvalidRequest))
        );
        let ui = OAuthForm::parse(b"client_id=wyrd-ui").expect("form parses");
        assert_eq!(
            clients.identify(&HeaderMap::new(), &ui),
            Err(OAuthError(OAuthErrorCode::InvalidClient))
        );
        assert_eq!(clients.identify(&HeaderMap::new(), &empty), Ok(None));
    }

    /// Catalog refusals map to their RFC codes; a token exchange reports an
    /// unusable subject as `invalid_request`.
    ///
    /// # Panics
    /// Panics when a refusal maps differently.
    #[test]
    fn catalog_refusals_map_to_rfc_codes() {
        let details = serde_json::json!({ "error": "slow_down" });
        let message = String::new();
        assert_eq!(
            OAuthError::from(WyrdError::DeviceAuthorization {
                message: message.clone(),
                details,
            }),
            OAuthError(OAuthErrorCode::SlowDown)
        );
        let invalid = || WyrdError::InvalidToken {
            message: String::new(),
            details: serde_json::json!({}),
        };
        assert_eq!(
            OAuthError::from(invalid()),
            OAuthError(OAuthErrorCode::InvalidGrant)
        );
        assert_eq!(
            OAuthError::request(invalid()),
            OAuthError(OAuthErrorCode::InvalidRequest)
        );
        assert_eq!(
            OAuthError::from(WyrdError::AuthVerifyUnavailable {
                message,
                details: serde_json::json!({}),
            }),
            OAuthError(OAuthErrorCode::TemporarilyUnavailable)
        );
    }
}
