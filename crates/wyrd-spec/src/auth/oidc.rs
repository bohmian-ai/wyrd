//! OIDC login contracts and URL newtypes.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Metadata, Schema, SchemaObject, StringValidation};
use serde::{Deserialize, Deserializer, Serialize};

use uuid::Uuid;

use crate::auth::SecretBearer;
use crate::error::WyrdError;
use crate::ids::TenantSlug;

/// Absolute URL used by auth contracts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct AbsoluteUrl(String);

impl AbsoluteUrl {
    /// Build a validated absolute URL.
    ///
    /// # Errors
    /// Returns [`UrlParseError`] when the value is not an absolute `https` or `http` URL with an authority.
    pub fn new(value: impl Into<String>) -> Result<Self, UrlParseError> {
        let value = value.into();
        validate_absolute_url(&value)?;
        Ok(Self(value))
    }

    /// Borrow the URL string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AbsoluteUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for AbsoluteUrl {
    type Err = UrlParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for AbsoluteUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for AbsoluteUrl {
    fn schema_name() -> String {
        "AbsoluteUrl".to_owned()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        url_schema(
            "Absolute URL with a scheme and authority.",
            Some("^https?://"),
        )
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for AbsoluteUrl {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        openapi_url_schema("Absolute URL with a scheme and authority.")
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for AbsoluteUrl {}

/// Trusted OIDC issuer URL.
///
/// The constructor validates the URL and removes trailing slashes so issuer
/// comparisons use one canonical string form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct IssuerUrl(String);

impl IssuerUrl {
    /// Build a normalized issuer URL.
    ///
    /// # Errors
    /// Returns [`UrlParseError`] when the value is not an absolute `https` URL with an authority.
    pub fn new(value: impl Into<String>) -> Result<Self, UrlParseError> {
        let value = value.into();
        validate_issuer_url(&value)?;
        Ok(Self(normalize_issuer(&value)))
    }

    /// Borrow the normalized issuer string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Build a normalized issuer URL accepting `http://` scheme.
    ///
    /// Only for use in test harnesses against local containers (Keycloak, Dex).
    /// Production code must use [`IssuerUrl::new`] which enforces `https://`.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_for_tests(value: impl Into<String>) -> Self {
        let value = value.into();
        Self(normalize_issuer(&value))
    }
}

impl fmt::Display for IssuerUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for IssuerUrl {
    type Err = UrlParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for IssuerUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for IssuerUrl {
    fn schema_name() -> String {
        "IssuerUrl".to_owned()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        url_schema(
            "Absolute OIDC issuer URL. Trailing slashes are normalized away.",
            Some("^https://"),
        )
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for IssuerUrl {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        openapi_url_schema("Absolute OIDC issuer URL. Trailing slashes are normalized away.")
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for IssuerUrl {}

/// Platform-administrator login initiation response.
///
/// The platform plane posts the provider's `code` and `state` back itself, so
/// it receives the state here. Tenant human login never does: its
/// [`BeginLoginResponse`] carries only the authorization URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct LoginInitResponse {
    /// Authorization URL used as the redirect target.
    pub authorization_url: AbsoluteUrl,
    /// Opaque login state echoed for SDK correlation.
    pub state: String,
}

/// A SHA-256 digest in its canonical wire form: exactly 64 lowercase
/// hexadecimal characters.
///
/// Used where a caller proves possession of a secret it keeps to itself, such
/// as the BFF's random login flow id: the server records only the digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Hex([u8; 32]);

impl Sha256Hex {
    /// Parse a canonical digest.
    ///
    /// # Errors
    /// Returns [`Sha256HexError`] when the value is not exactly 64 lowercase
    /// hexadecimal characters. Uppercase is refused rather than normalized so
    /// one digest has one wire form.
    pub fn new(value: &str) -> Result<Self, Sha256HexError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Sha256HexError);
        }
        let mut digest = [0_u8; 32];
        hex::decode_to_slice(value, &mut digest).map_err(|_| Sha256HexError)?;
        Ok(Self(digest))
    }

    /// Digest `bytes` with SHA-256.
    #[must_use]
    pub fn digest(bytes: &[u8]) -> Self {
        use sha2::Digest as _;
        Self(sha2::Sha256::digest(bytes).into())
    }

    /// The raw 32-byte digest.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for Sha256Hex {
    /// Wrap an already-computed raw digest, such as one read back from storage.
    fn from(digest: [u8; 32]) -> Self {
        Self(digest)
    }
}

impl fmt::Display for Sha256Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

impl Serialize for Sha256Hex {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256Hex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Sha256Hex {
    fn schema_name() -> String {
        "Sha256Hex".to_owned()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            metadata: Some(Box::new(Metadata {
                description: Some(SHA256_HEX_DESCRIPTION.to_owned()),
                ..Default::default()
            })),
            instance_type: Some(InstanceType::String.into()),
            string: Some(Box::new(StringValidation {
                min_length: Some(64),
                max_length: Some(64),
                pattern: Some(SHA256_HEX_PATTERN.to_owned()),
            })),
            ..Default::default()
        })
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for Sha256Hex {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};

        utoipa::openapi::RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(64))
                .max_length(Some(64))
                .pattern(Some(SHA256_HEX_PATTERN))
                .description(Some(SHA256_HEX_DESCRIPTION))
                .build(),
        ))
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for Sha256Hex {}

/// Description shared by the JSON Schema and `OpenAPI` projections of
/// [`Sha256Hex`].
const SHA256_HEX_DESCRIPTION: &str =
    "SHA-256 digest as exactly 64 lowercase hexadecimal characters.";

/// Pattern shared by the JSON Schema and `OpenAPI` projections of
/// [`Sha256Hex`].
const SHA256_HEX_PATTERN: &str = "^[0-9a-f]{64}$";

/// A value that is not a canonical [`Sha256Hex`] digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected a SHA-256 digest as 64 lowercase hexadecimal characters")]
pub struct Sha256HexError;

/// `POST /auth/login` request: begin a tenant human SSO login.
///
/// The route key is pre-login routing context only; it never becomes tenant
/// authority. Exactly one initiation binding is required: a browser login
/// through the BFF sends the SHA-256 of its random flow id, and a CLI login
/// sends its server-issued handoff id. The completed session is later redeemed
/// only by that binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BeginLogin {
    /// The tenant's route key (its slug), as in `/t/{tenantKey}/login`.
    pub tenant_route_key: TenantSlug,
    /// SHA-256 of the BFF's random browser flow id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_flow_hash: Option<Sha256Hex>,
    /// Server-issued CLI login handoff id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_handoff_id: Option<Uuid>,
}

/// How a tenant human login was initiated: the single binding its completed
/// session is redeemed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginInitiation {
    /// A browser login bound to the BFF's flow id hash.
    Browser(Sha256Hex),
    /// A CLI login bound to its handoff id.
    Cli(Uuid),
}

impl BeginLogin {
    /// The request's one initiation binding.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when both or neither of
    /// `browser_flow_hash` and `cli_handoff_id` are present.
    pub fn initiation(&self) -> Result<LoginInitiation, WyrdError> {
        match (self.browser_flow_hash, self.cli_handoff_id) {
            (Some(hash), None) => Ok(LoginInitiation::Browser(hash)),
            (None, Some(handoff)) => Ok(LoginInitiation::Cli(handoff)),
            _ => Err(WyrdError::Validation {
                message: "exactly one of browser_flow_hash or cli_handoff_id is required"
                    .to_owned(),
                details: serde_json::json!({ "reason": "login_binding_required" }),
            }),
        }
    }
}

/// `POST /auth/login` response.
///
/// Carries only the provider authorization URL. The login state is in that
/// URL and nowhere else; the caller never needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BeginLoginResponse {
    /// Provider authorization URL to send the person's browser to.
    pub authorization_url: AbsoluteUrl,
}

/// `GET /auth/callback` query: the provider's redirect back to the common
/// callback. It carries no tenant selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CallbackQuery {
    /// Authorization code from the identity provider callback.
    pub code: SecretBearer,
    /// Opaque login state generated by Wyrd.
    #[schemars(schema_with = "state_key_schema")]
    pub state: String,
}

/// URL validation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UrlParseError {
    /// The string is not a valid URL.
    #[error("URL failed to parse: {0}")]
    ParseFailed(String),
    /// The URL has no authority (host) component.
    #[error("URL must have an authority (host)")]
    NoHost,
    /// The URL scheme is not permitted by this validator.
    #[error("URL scheme '{0}' is not allowed")]
    DisallowedScheme(String),
}

fn validate_absolute_url(value: &str) -> Result<(), UrlParseError> {
    let parsed = url::Url::parse(value).map_err(|e| UrlParseError::ParseFailed(e.to_string()))?;
    if !parsed.has_host() {
        return Err(UrlParseError::NoHost);
    }
    match parsed.scheme() {
        "https" | "http" => Ok(()),
        scheme => Err(UrlParseError::DisallowedScheme(scheme.to_owned())),
    }
}

fn validate_issuer_url(value: &str) -> Result<(), UrlParseError> {
    let parsed = url::Url::parse(value).map_err(|e| UrlParseError::ParseFailed(e.to_string()))?;
    if !parsed.has_host() {
        return Err(UrlParseError::NoHost);
    }
    match parsed.scheme() {
        "https" => Ok(()),
        // `http` is permitted only for loopback issuers (local Keycloak/Dex
        // e2e fixtures). Every non-loopback host stays https-only.
        "http" if is_loopback_host(&parsed) => Ok(()),
        scheme => Err(UrlParseError::DisallowedScheme(scheme.to_owned())),
    }
}

fn is_loopback_host(parsed: &url::Url) -> bool {
    match parsed.host() {
        Some(url::Host::Domain(host)) => host == "localhost",
        Some(url::Host::Ipv4(addr)) => addr.is_loopback(),
        Some(url::Host::Ipv6(addr)) => addr.is_loopback(),
        None => false,
    }
}

fn normalize_issuer(value: &str) -> String {
    value.trim_end_matches('/').to_owned()
}

fn url_schema(description: &'static str, pattern: Option<&'static str>) -> Schema {
    Schema::Object(SchemaObject {
        metadata: Some(Box::new(Metadata {
            description: Some(description.to_owned()),
            ..Default::default()
        })),
        instance_type: Some(InstanceType::String.into()),
        format: Some("uri".to_owned()),
        string: pattern.map(|p| {
            Box::new(StringValidation {
                pattern: Some(p.to_owned()),
                ..Default::default()
            })
        }),
        ..Default::default()
    })
}

fn state_key_schema(_gen: &mut SchemaGenerator) -> Schema {
    Schema::Object(SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        string: Some(Box::new(StringValidation {
            min_length: Some(1),
            max_length: Some(512),
            pattern: None,
        })),
        ..Default::default()
    })
}

#[cfg(feature = "server")]
fn openapi_url_schema(
    description: &'static str,
) -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{ObjectBuilder, Schema, SchemaFormat, Type};

    utoipa::openapi::RefOr::T(Schema::Object(
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::Custom("uri".to_owned())))
            .description(Some(description))
            .build(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        AbsoluteUrl, BeginLogin, CallbackQuery, IssuerUrl, LoginInitResponse, LoginInitiation,
        Sha256Hex, UrlParseError,
    };

    #[test]
    fn issuer_url_normalizes_trailing_slash() {
        let issuer =
            IssuerUrl::new("https://idp.example.com/realms/acme/").expect("issuer URL is valid");

        assert_eq!(issuer.as_str(), "https://idp.example.com/realms/acme");
    }

    #[test]
    fn issuer_url_rejects_non_url_input() {
        assert!(IssuerUrl::new("not a url").is_err());
        assert!(IssuerUrl::new("/relative/path").is_err());
    }

    #[test]
    fn issuer_url_accepts_https_any_host() {
        assert!(IssuerUrl::new("https://idp.example.com/realms/acme").is_ok());
        assert!(IssuerUrl::new("https://localhost:8443/realms/wyrd-test").is_ok());
    }

    #[test]
    fn issuer_url_accepts_http_loopback() {
        assert!(IssuerUrl::new("http://localhost:8080/realms/wyrd-test").is_ok());
        assert!(IssuerUrl::new("http://127.0.0.1:8080/realms/wyrd-test").is_ok());
        assert!(IssuerUrl::new("http://[::1]:8080/realms/wyrd-test").is_ok());
    }

    #[test]
    fn issuer_url_rejects_http_non_loopback() {
        assert!(matches!(
            IssuerUrl::new("http://idp.example.com"),
            Err(UrlParseError::DisallowedScheme(scheme)) if scheme == "http"
        ));
        assert!(matches!(
            IssuerUrl::new("http://10.0.0.1:8080"),
            Err(UrlParseError::DisallowedScheme(scheme)) if scheme == "http"
        ));
    }

    #[test]
    fn issuer_url_rejects_non_https() {
        assert!(IssuerUrl::new("http://insecure.example.com").is_err());
        assert!(IssuerUrl::new("ftp://files.example.com").is_err());
        assert!(matches!(
            IssuerUrl::new("ftp://localhost"),
            Err(UrlParseError::DisallowedScheme(scheme)) if scheme == "ftp"
        ));
    }

    #[test]
    fn issuer_url_rejects_bad_url_and_no_host() {
        assert!(matches!(
            IssuerUrl::new("not a url"),
            Err(UrlParseError::ParseFailed(_))
        ));
        assert!(matches!(
            IssuerUrl::new("mailto:user@example.com"),
            Err(UrlParseError::NoHost)
        ));
    }

    #[test]
    fn url_rejects_non_url_input() {
        assert!(AbsoluteUrl::new("not a url").is_err());
        assert!(AbsoluteUrl::new("urn:example:login").is_err());
    }

    #[test]
    fn url_rejects_disallowed_schemes() {
        assert!(AbsoluteUrl::new("ftp://files.example.com").is_err());
        assert!(AbsoluteUrl::new("file://localhost/etc/passwd").is_err());
    }

    #[test]
    fn url_accepts_valid_absolute_url() {
        let url = AbsoluteUrl::new("https://idp.example.com/authorize").expect("valid");
        assert_eq!(url.as_str(), "https://idp.example.com/authorize");
        let from_str: AbsoluteUrl = "https://idp.example.com/authorize"
            .parse()
            .expect("FromStr");
        assert_eq!(from_str, url);
    }

    #[test]
    fn url_deserialize_rejects_invalid() {
        assert!(serde_json::from_str::<AbsoluteUrl>("\"not a url\"").is_err());
        assert!(serde_json::from_str::<AbsoluteUrl>("\"/relative/path\"").is_err());
        assert!(serde_json::from_str::<AbsoluteUrl>("\"ftp://files.example.com\"").is_err());
    }

    #[test]
    fn issuer_url_deserialize_rejects_invalid() {
        assert!(serde_json::from_str::<IssuerUrl>("\"not a url\"").is_err());
        assert!(serde_json::from_str::<IssuerUrl>("\"/relative/path\"").is_err());
        assert!(serde_json::from_str::<IssuerUrl>("\"http://insecure.example.com\"").is_err());
    }

    #[test]
    fn login_init_response_roundtrips() {
        let resp = LoginInitResponse {
            authorization_url: AbsoluteUrl::new("https://idp.example.com/authorize").unwrap(),
            state: "opaque-state".to_owned(),
        };
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(
            value["authorization_url"],
            "https://idp.example.com/authorize"
        );
        assert_eq!(
            serde_json::from_value::<LoginInitResponse>(value).unwrap(),
            resp
        );
    }

    #[test]
    fn callback_query_rejects_unknown_fields() {
        let json = serde_json::json!({
            "code": "auth-code",
            "state": "state-123",
            "extra": true
        });
        assert!(serde_json::from_value::<CallbackQuery>(json).is_err());
    }

    /// A digest parses only in its canonical lowercase 64-hex form and
    /// serializes back to it.
    #[test]
    fn sha256_hex_accepts_only_the_canonical_form() {
        let digest = Sha256Hex::digest(b"flow");
        let wire = digest.to_string();
        assert_eq!(wire.len(), 64);
        assert_eq!(Sha256Hex::new(&wire), Ok(digest));
        assert!(Sha256Hex::new(&wire.to_uppercase()).is_err());
        assert!(Sha256Hex::new(&wire[..63]).is_err());
        assert!(Sha256Hex::new(&format!("{}g", &wire[..63])).is_err());
        assert_eq!(
            serde_json::to_value(digest).expect("serializes"),
            serde_json::json!(wire)
        );
    }

    /// Exactly one initiation binding is required; both or neither is a
    /// validation error, and unknown fields are refused.
    #[test]
    fn begin_login_requires_exactly_one_binding() {
        let hash = Sha256Hex::digest(b"flow").to_string();
        let handoff = uuid::Uuid::now_v7();
        let parse = |value: serde_json::Value| {
            serde_json::from_value::<BeginLogin>(value).expect("begin login parses")
        };

        let browser = parse(serde_json::json!({
            "tenant_route_key": "acme", "browser_flow_hash": hash
        }));
        assert!(matches!(
            browser.initiation(),
            Ok(LoginInitiation::Browser(_))
        ));
        let cli = parse(serde_json::json!({
            "tenant_route_key": "acme", "cli_handoff_id": handoff
        }));
        assert!(matches!(cli.initiation(), Ok(LoginInitiation::Cli(id)) if id == handoff));
        for ambiguous in [
            serde_json::json!({ "tenant_route_key": "acme" }),
            serde_json::json!({
                "tenant_route_key": "acme", "browser_flow_hash": hash, "cli_handoff_id": handoff
            }),
        ] {
            let error = parse(ambiguous).initiation().expect_err("refused");
            assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION");
        }
        assert!(
            serde_json::from_value::<BeginLogin>(serde_json::json!({
                "tenant_route_key": "acme", "browser_flow_hash": hash, "issuer": "x"
            }))
            .is_err()
        );
    }
}
