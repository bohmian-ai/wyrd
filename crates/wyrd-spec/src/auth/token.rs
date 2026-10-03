//! OAuth 2.0 authorization-server wire contracts.
//!
//! `POST /auth/token` takes `application/x-www-form-urlencoded` parameters
//! (RFC 6749 §4.1.3, §6; RFC 8628 §3.4; RFC 8693 §2.1; RFC 7523 §2.1) and
//! answers the RFC 6749 §5.1 [`TokenResponse`] or the §5.2
//! [`OAuthErrorResponse`]. Unrecognized parameters are ignored (RFC 6749
//! §3.2). The client is identified by HTTP Basic authentication or the
//! `client_id` parameter, never by a [`TokenRequest`] field.

use serde::{Deserialize, Serialize};

use crate::auth::{AbsoluteUrl, SecretBearer};
use crate::ids::TenantSlug;

/// Parameters of `POST /auth/token`, discriminated by `grant_type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "grant_type")]
pub enum TokenRequest {
    /// RFC 6749 §4.1.3: redeem a Wyrd authorization code with its PKCE
    /// verifier (RFC 7636 §4.5).
    #[serde(rename = "authorization_code")]
    AuthorizationCode {
        /// The code `GET /auth/authorize` returned to the redirect URI.
        code: SecretBearer,
        /// The exact redirect URI the authorization request carried.
        redirect_uri: String,
        /// The PKCE code verifier whose S256 challenge the request carried.
        code_verifier: SecretBearer,
    },
    /// RFC 6749 §6: renew a human session.
    #[serde(rename = "refresh_token")]
    RefreshToken {
        /// The Wyrd refresh token.
        refresh_token: SecretBearer,
    },
    /// RFC 8628 §3.4: poll with the device code from
    /// `POST /auth/device_authorization`.
    #[serde(rename = "urn:ietf:params:oauth:grant-type:device_code")]
    DeviceCode {
        /// The device code.
        device_code: SecretBearer,
    },
    /// RFC 8693 §2.1 token exchange.
    ///
    /// With an [`ExchangeTokenType::ApiKey`] subject and no actor, the Wyrd
    /// API key is exchanged for an access token of its own principal. With an
    /// [`ExchangeTokenType::AccessToken`] subject and actor, the actor acts on
    /// behalf of the subject: the issued token names the subject as its
    /// principal, the actor as its outermost `act`, and `audience` as its
    /// `aud`, with the intersection of both parties' permissions.
    #[serde(rename = "urn:ietf:params:oauth:grant-type:token-exchange")]
    TokenExchange {
        /// The API key, or the access token of the party acted for.
        subject_token: SecretBearer,
        /// Type of `subject_token`.
        subject_token_type: ExchangeTokenType,
        /// Access token of the party doing the work; delegation only.
        actor_token: Option<SecretBearer>,
        /// Type of `actor_token`.
        actor_token_type: Option<ExchangeTokenType>,
        /// Wyrd resource a delegated token is issued for; delegation only.
        audience: Option<TokenAudience>,
    },
    /// RFC 7523 §2.1: a platform-attested workload assertion.
    #[serde(rename = "urn:ietf:params:oauth:grant-type:jwt-bearer")]
    JwtBearer {
        /// Kubernetes service-account token, SPIFFE JWT-SVID, or cloud token.
        assertion: SecretBearer,
        /// Tenant selector fallback when the request host does not carry tenant.
        tenant: Option<TenantSlug>,
    },
}

/// RFC 8693 §3 token type of a subject, actor, or issued token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ExchangeTokenType {
    /// OAuth access token.
    #[serde(rename = "urn:ietf:params:oauth:token-type:access_token")]
    AccessToken,
    /// A Wyrd API key (`wyrd_sk_…`) or platform credential.
    #[serde(rename = "urn:wyrd:oauth:token-type:api_key")]
    ApiKey,
}

/// Audience (`aud`) of a Wyrd tenant access token.
///
/// Every directly issued token is [`Self::Wyrd`] and is accepted on every
/// tenant surface. A [`Self::Bifrost`] token is accepted only on the Bifrost
/// surfaces, so a token delegated for Bifrost cannot be replayed elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TokenAudience {
    /// Every Wyrd tenant surface.
    Wyrd,
    /// Only the Bifrost ingest and query surfaces.
    Bifrost,
}

impl TokenAudience {
    /// The canonical `aud` claim value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wyrd => "wyrd",
            Self::Bifrost => "bifrost",
        }
    }
}

/// RFC 6749 §5.1 successful token response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TokenResponse {
    /// Signed Wyrd access token.
    pub access_token: SecretBearer,
    /// Token type.
    pub token_type: TokenType,
    /// Seconds until the access token expires.
    pub expires_in: u64,
    /// Refresh token. Issued only for a human session: at authorization-code
    /// and device-code redemption, and when `wyrd-cli` rotates its refresh
    /// token. `wyrd-ui` keeps presenting its original refresh token, and no
    /// machine grant issues one: those clients re-present their durable
    /// credential instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<SecretBearer>,
    /// RFC 8693 §2.2.1 type of the issued token; present on token-exchange
    /// responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_token_type: Option<ExchangeTokenType>,
}

/// Bearer token marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "PascalCase")]
pub enum TokenType {
    /// Bearer token.
    Bearer,
}

/// The registered OAuth clients of a Wyrd deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum OAuthClientId {
    /// The UI's backend-for-frontend: a confidential client authenticating
    /// with `client_secret_basic` (RFC 6749 §2.3.1).
    #[serde(rename = "wyrd-ui")]
    WyrdUi,
    /// The CLI and SDK saved logins: a public client.
    #[serde(rename = "wyrd-cli")]
    WyrdCli,
}

impl OAuthClientId {
    /// The `client_id` value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WyrdUi => "wyrd-ui",
            Self::WyrdCli => "wyrd-cli",
        }
    }

    /// The client a `client_id` value names, if any.
    #[must_use]
    pub fn parse(client_id: &str) -> Option<Self> {
        match client_id {
            "wyrd-ui" => Some(Self::WyrdUi),
            "wyrd-cli" => Some(Self::WyrdCli),
            _ => None,
        }
    }
}

/// An OAuth error code: RFC 6749 §4.1.2.1 and §5.2, RFC 8628 §3.5, and
/// RFC 8693 §2.2.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum OAuthErrorCode {
    /// A required parameter is missing, invalid, or repeated.
    InvalidRequest,
    /// Client authentication failed.
    InvalidClient,
    /// The grant or refresh token is invalid, expired, revoked, or was
    /// issued to another client or redirect URI.
    InvalidGrant,
    /// The client may not use this grant or endpoint.
    UnauthorizedClient,
    /// The grant type is not supported.
    UnsupportedGrantType,
    /// The response type is not supported.
    UnsupportedResponseType,
    /// RFC 8693 §2.2.2: the server cannot issue a token for the requested
    /// `audience`.
    InvalidTarget,
    /// The resource owner or the server denied the request.
    AccessDenied,
    /// The device authorization is still pending.
    AuthorizationPending,
    /// The device code was polled too often.
    SlowDown,
    /// The device code expired.
    ExpiredToken,
    /// The server failed.
    ServerError,
    /// The server is temporarily unavailable.
    TemporarilyUnavailable,
}

/// RFC 6749 §5.2 error response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OAuthErrorResponse {
    /// The error code.
    pub error: OAuthErrorCode,
    /// Human-readable detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

/// RFC 8414 §2 authorization server metadata, served at
/// `/.well-known/oauth-authorization-server`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AuthorizationServerMetadata {
    /// The deployment's public origin.
    pub issuer: AbsoluteUrl,
    /// `GET /auth/authorize`.
    pub authorization_endpoint: AbsoluteUrl,
    /// `POST /auth/token`.
    pub token_endpoint: AbsoluteUrl,
    /// RFC 8628 §4: `POST /auth/device_authorization`.
    pub device_authorization_endpoint: AbsoluteUrl,
    /// RFC 7009: `POST /auth/revoke`.
    pub revocation_endpoint: AbsoluteUrl,
    /// `["code"]`.
    pub response_types_supported: Vec<String>,
    /// Every grant `POST /auth/token` accepts.
    pub grant_types_supported: Vec<String>,
    /// `client_secret_basic` for `wyrd-ui` and `none` for `wyrd-cli`.
    pub token_endpoint_auth_methods_supported: Vec<String>,
    /// The same methods at the revocation endpoint.
    pub revocation_endpoint_auth_methods_supported: Vec<String>,
    /// `["S256"]`.
    pub code_challenge_methods_supported: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        ExchangeTokenType, OAuthClientId, OAuthErrorCode, OAuthErrorResponse, TokenAudience,
        TokenRequest, TokenResponse, TokenType,
    };
    use crate::auth::SecretBearer;

    /// Decode form-shaped parameters, as the token endpoint does.
    ///
    /// # Errors
    /// Returns the decode error when the parameters name no supported grant
    /// or omit one of its required parameters.
    fn parse(pairs: &[(&str, &str)]) -> Result<TokenRequest, serde_json::Error> {
        let map = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), serde_json::json!(value)))
            .collect::<serde_json::Map<_, _>>();
        serde_json::from_value(serde_json::Value::Object(map))
    }

    /// Every grant decodes from its RFC parameters and ignores parameters it
    /// does not define (RFC 6749 §3.2); an unknown grant type does not decode.
    ///
    /// # Panics
    /// Panics when a grant decodes differently.
    #[test]
    fn grants_decode_from_rfc_parameters() {
        assert_eq!(
            parse(&[
                ("grant_type", "authorization_code"),
                ("code", "c"),
                ("redirect_uri", "https://wyrd.example.com/login/callback"),
                ("code_verifier", "v"),
                ("client_id", "wyrd-ui"),
            ])
            .expect("code grant decodes"),
            TokenRequest::AuthorizationCode {
                code: SecretBearer::new("c".to_owned()),
                redirect_uri: "https://wyrd.example.com/login/callback".to_owned(),
                code_verifier: SecretBearer::new("v".to_owned()),
            }
        );
        assert_eq!(
            parse(&[
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:token-exchange"
                ),
                ("subject_token", "wyrd_sk_key"),
                ("subject_token_type", "urn:wyrd:oauth:token-type:api_key"),
            ])
            .expect("api-key exchange decodes"),
            TokenRequest::TokenExchange {
                subject_token: SecretBearer::new("wyrd_sk_key".to_owned()),
                subject_token_type: ExchangeTokenType::ApiKey,
                actor_token: None,
                actor_token_type: None,
                audience: None,
            }
        );
        assert_eq!(
            parse(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", "r"),
                ("scope", "ignored"),
            ])
            .expect("refresh decodes"),
            TokenRequest::RefreshToken {
                refresh_token: SecretBearer::new("r".to_owned()),
            }
        );
        assert!(parse(&[("grant_type", "password"), ("username", "u")]).is_err());
        assert!(parse(&[("grant_type", "wyrd_api_key"), ("api_key", "k")]).is_err());
    }

    /// The success body carries `expires_in` and omits absent optional
    /// members; the error body carries the snake-case RFC code.
    ///
    /// # Panics
    /// Panics when a body serializes differently.
    #[test]
    fn response_bodies_follow_rfc_6749_section_5() {
        let response = TokenResponse {
            access_token: SecretBearer::new("access".to_owned()),
            token_type: TokenType::Bearer,
            expires_in: 300,
            refresh_token: None,
            issued_token_type: Some(ExchangeTokenType::AccessToken),
        };
        assert_eq!(
            serde_json::to_value(&response).expect("serializes"),
            serde_json::json!({
                "access_token": "access",
                "token_type": "Bearer",
                "expires_in": 300,
                "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
            })
        );
        let error = OAuthErrorResponse {
            error: OAuthErrorCode::AuthorizationPending,
            error_description: None,
        };
        assert_eq!(
            serde_json::to_value(&error).expect("serializes"),
            serde_json::json!({ "error": "authorization_pending" })
        );
        assert_eq!(
            serde_json::to_value(OAuthErrorCode::InvalidTarget).expect("serializes"),
            "invalid_target"
        );
        assert_eq!(
            OAuthClientId::parse("wyrd-cli"),
            Some(OAuthClientId::WyrdCli)
        );
        assert_eq!(OAuthClientId::WyrdUi.as_str(), "wyrd-ui");
        assert_eq!(OAuthClientId::parse("other"), None);
    }

    /// Secret-bearing parameters never reach `Debug`.
    ///
    /// # Panics
    /// Panics when the assertion is printed.
    #[test]
    fn secret_grants_redact_debug() {
        let request = TokenRequest::JwtBearer {
            assertion: SecretBearer::new("top-secret-assertion".to_owned()),
            tenant: None,
        };

        let debug = format!("{request:?}");

        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("top-secret-assertion"));
    }

    /// The audience wire values are the `aud` claim values.
    ///
    /// # Panics
    /// Panics when they differ.
    #[test]
    fn audience_wire_values_are_claim_values() {
        for audience in [TokenAudience::Wyrd, TokenAudience::Bifrost] {
            assert_eq!(
                serde_json::to_value(audience).expect("serializes"),
                audience.as_str()
            );
        }
    }
}
