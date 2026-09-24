//! Tenant Operator connection contracts.
//!
//! A connection is the only place an Operator credential exists. Writes use a
//! closed provider-tagged union whose secret fields are write-only
//! [`SecretBearer`] values; reads return [`OperatorConnectionView`], whose
//! [`OperatorConnectionConfig`] is the nonsecret authority Postgres stores in
//! the clear. [`ConnectionSecret`] is the plaintext the server encrypts and is
//! never a wire response.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::SecretBearer;
use crate::card::operator::FORBIDDEN_HTTP_HEADERS;
use crate::error::WyrdError;
use crate::ids::{ConnectionName, OperatorConnectionId};

/// The closed set of Operator connection providers.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::Display,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum OperatorProvider {
    /// A Slack workspace bot token.
    Slack,
    /// A PagerDuty Global Integration key.
    PagerDuty,
    /// Credentials for one HTTPS origin.
    Http,
}

/// Whether a connection may be used by Operators.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum OperatorConnectionStatus {
    /// Registration and delivery may use it.
    Active,
    /// Retained for lineage; registration and delivery refuse it.
    Disabled,
}

/// A normalized HTTPS origin: `https://host[:port]`, default port elided.
///
/// No path, query, fragment, or userinfo. Plain `http` is accepted only for a
/// loopback host, matching the repository's local-fixture issuer rule; the
/// SSRF screen still decides reachability per deployment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct HttpsOrigin(String);

/// Why a value is not an [`HttpsOrigin`].
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct OriginError(String);

impl HttpsOrigin {
    /// Parse and normalize an origin.
    ///
    /// # Errors
    /// Returns [`OriginError`] for an unparsable URL, a scheme other than
    /// `https` (or loopback `http`), userinfo, or any path, query, or fragment.
    pub fn parse(value: &str) -> Result<Self, OriginError> {
        let url = url::Url::parse(value).map_err(|error| OriginError(error.to_string()))?;
        Self::of(&url, true)
    }

    /// The origin of an absolute URL, ignoring its path, query, and fragment.
    ///
    /// # Errors
    /// Returns [`OriginError`] for a disallowed scheme, no host, or userinfo.
    pub fn of_url(url: &url::Url) -> Result<Self, OriginError> {
        Self::of(url, false)
    }

    /// Build the origin of `url`, refusing any trailing component when `bare`.
    fn of(url: &url::Url, bare: bool) -> Result<Self, OriginError> {
        let loopback = match url.host() {
            Some(url::Host::Domain(host)) => host == "localhost",
            Some(url::Host::Ipv4(addr)) => addr.is_loopback(),
            Some(url::Host::Ipv6(addr)) => addr.is_loopback(),
            None => return Err(OriginError("origin must have a host".to_owned())),
        };
        match url.scheme() {
            "https" => {}
            "http" if loopback => {}
            scheme => return Err(OriginError(format!("scheme {scheme} is not https"))),
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(OriginError("origin must not carry userinfo".to_owned()));
        }
        if bare && (url.path() != "/" || url.query().is_some() || url.fragment().is_some()) {
            return Err(OriginError(
                "origin must not carry a path, query, or fragment".to_owned(),
            ));
        }
        Ok(Self(url.origin().ascii_serialization()))
    }

    /// Borrow the normalized origin.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HttpsOrigin {
    /// Write the normalized `https://host[:port]` origin exactly as parsed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for HttpsOrigin {
    /// Read a string and parse it through [`HttpsOrigin::parse`], normalizing
    /// it to its serialized origin.
    ///
    /// # Errors
    /// Returns the deserializer's error when the value is not a string, or a
    /// custom serde error carrying the [`OriginError`] when it is not an
    /// allowed `https` origin.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Write-only HTTP credential of a create or update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "scheme", rename_all = "snake_case", deny_unknown_fields)]
pub enum HttpConnectionAuth {
    /// `Authorization: Bearer <token>`.
    Bearer {
        /// The bearer token.
        token: SecretBearer,
    },
    /// `Authorization: Basic` from a username and password.
    Basic {
        /// The username; treated as secret.
        username: SecretBearer,
        /// The password.
        password: SecretBearer,
    },
    /// A custom header carrying `value`.
    Header {
        /// Header name; case-insensitive, never a server-owned header.
        name: String,
        /// The header value.
        value: SecretBearer,
    },
}

impl HttpConnectionAuth {
    /// Split into the stored nonsecret authority and the secret to encrypt.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorConnectionInvalid`] for an empty secret or
    /// an invalid or server-owned header name.
    fn split(self) -> Result<(HttpAuthScheme, ConnectionSecret), WyrdError> {
        Ok(match self {
            Self::Bearer { token } => (HttpAuthScheme::Bearer, ConnectionSecret::token(token)?),
            Self::Basic { username, password } => {
                if username.expose().is_empty() || username.expose().contains(':') {
                    return Err(invalid("auth.username", "must be non-empty without ':'"));
                }
                if password.expose().is_empty() {
                    return Err(invalid("auth.password", "must not be empty"));
                }
                (
                    HttpAuthScheme::Basic,
                    ConnectionSecret::Basic { username, password },
                )
            }
            Self::Header { name, value } => {
                let forbidden = FORBIDDEN_HTTP_HEADERS
                    .iter()
                    .any(|header| name.eq_ignore_ascii_case(header));
                let token = name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
                if name.is_empty() || !token || forbidden {
                    return Err(invalid(
                        "auth.name",
                        "must be a non-server-owned header name",
                    ));
                }
                (
                    HttpAuthScheme::Header { name },
                    ConnectionSecret::token(value)?,
                )
            }
        })
    }
}

/// Redacted HTTP authority: the scheme and custom header name only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "scheme", rename_all = "snake_case", deny_unknown_fields)]
pub enum HttpAuthScheme {
    /// Bearer token.
    Bearer,
    /// Username and password.
    Basic,
    /// Custom header.
    Header {
        /// Header name as created.
        name: String,
    },
}

/// `POST /v1/operator-connections` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreateOperatorConnectionRequest {
    /// A Slack workspace bot token.
    Slack {
        /// Immutable connection name.
        name: ConnectionName,
        /// Slack workspace (team) ID the token belongs to.
        workspace_id: String,
        /// Bot token with `chat:write`.
        bot_token: SecretBearer,
    },
    /// A PagerDuty Global Integration key.
    PagerDuty {
        /// Immutable connection name.
        name: ConnectionName,
        /// Events API v2 Global Integration key.
        integration_key: SecretBearer,
    },
    /// Credentials for one HTTPS origin.
    Http {
        /// Immutable connection name.
        name: ConnectionName,
        /// The only origin these credentials may be sent to.
        origin: HttpsOrigin,
        /// The credential and its scheme.
        auth: HttpConnectionAuth,
    },
}

impl CreateOperatorConnectionRequest {
    /// Validate and split into name, stored authority, and secret.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorConnectionInvalid`] for an empty workspace
    /// ID or secret, or an invalid header name.
    pub fn split(
        self,
    ) -> Result<(ConnectionName, OperatorConnectionConfig, ConnectionSecret), WyrdError> {
        Ok(match self {
            Self::Slack {
                name,
                workspace_id,
                bot_token,
            } => (
                name,
                OperatorConnectionConfig::slack(workspace_id)?,
                ConnectionSecret::token(bot_token)?,
            ),
            Self::PagerDuty {
                name,
                integration_key,
            } => (
                name,
                OperatorConnectionConfig::PagerDuty {},
                ConnectionSecret::token(integration_key)?,
            ),
            Self::Http { name, origin, auth } => {
                let (auth, secret) = auth.split()?;
                (
                    name,
                    OperatorConnectionConfig::Http { origin, auth },
                    secret,
                )
            }
        })
    }
}

/// `PATCH /v1/operator-connections/{connection_id}` body.
///
/// Omitted fields are preserved; a supplied secret replaces the encrypted
/// secret atomically. The provider tag must equal the stored provider, and the
/// name is not patchable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum UpdateOperatorConnectionRequest {
    /// Update a Slack connection.
    Slack {
        /// Replacement workspace ID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<String>,
        /// Replacement bot token.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bot_token: Option<SecretBearer>,
        /// New status.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<OperatorConnectionStatus>,
    },
    /// Update a PagerDuty connection.
    PagerDuty {
        /// Replacement Global Integration key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        integration_key: Option<SecretBearer>,
        /// New status.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<OperatorConnectionStatus>,
    },
    /// Update an HTTP connection.
    Http {
        /// Replacement origin.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<HttpsOrigin>,
        /// Replacement credential; may change its scheme and header name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<HttpConnectionAuth>,
        /// New status.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<OperatorConnectionStatus>,
    },
}

/// The effect of one validated update on a stored connection.
#[derive(Debug)]
pub struct ConnectionUpdate {
    /// The authority to store.
    pub config: OperatorConnectionConfig,
    /// A replacement secret, when one was supplied.
    pub secret: Option<ConnectionSecret>,
    /// A new status, when one was supplied.
    pub status: Option<OperatorConnectionStatus>,
}

impl UpdateOperatorConnectionRequest {
    /// Apply this update to the stored authority `current`.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorConnectionInvalid`] when the provider tag
    /// differs from `current`'s, or a supplied value is invalid.
    pub fn apply(self, current: OperatorConnectionConfig) -> Result<ConnectionUpdate, WyrdError> {
        match (self, current) {
            (
                Self::Slack {
                    workspace_id,
                    bot_token,
                    status,
                },
                OperatorConnectionConfig::Slack {
                    workspace_id: stored,
                },
            ) => Ok(ConnectionUpdate {
                config: OperatorConnectionConfig::slack(workspace_id.unwrap_or(stored))?,
                secret: bot_token.map(ConnectionSecret::token).transpose()?,
                status,
            }),
            (
                Self::PagerDuty {
                    integration_key,
                    status,
                },
                OperatorConnectionConfig::PagerDuty {},
            ) => Ok(ConnectionUpdate {
                config: OperatorConnectionConfig::PagerDuty {},
                secret: integration_key.map(ConnectionSecret::token).transpose()?,
                status,
            }),
            (
                Self::Http {
                    origin,
                    auth,
                    status,
                },
                OperatorConnectionConfig::Http {
                    origin: stored_origin,
                    auth: stored_auth,
                },
            ) => {
                let (auth, secret) = match auth {
                    Some(auth) => {
                        let (scheme, secret) = auth.split()?;
                        (scheme, Some(secret))
                    }
                    None => (stored_auth, None),
                };
                Ok(ConnectionUpdate {
                    config: OperatorConnectionConfig::Http {
                        origin: origin.unwrap_or(stored_origin),
                        auth,
                    },
                    secret,
                    status,
                })
            }
            (update, current) => Err(invalid(
                "provider",
                &format!(
                    "update for {} does not match the {} connection",
                    update.provider(),
                    current.provider()
                ),
            )),
        }
    }

    /// The provider this update is tagged with.
    #[must_use]
    pub const fn provider(&self) -> OperatorProvider {
        match self {
            Self::Slack { .. } => OperatorProvider::Slack,
            Self::PagerDuty { .. } => OperatorProvider::PagerDuty,
            Self::Http { .. } => OperatorProvider::Http,
        }
    }
}

/// Nonsecret connection authority, stored in the clear and returned on reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperatorConnectionConfig {
    /// Slack authority.
    Slack {
        /// Slack workspace ID.
        workspace_id: String,
    },
    /// PagerDuty has no nonsecret configuration.
    PagerDuty {},
    /// HTTP authority.
    Http {
        /// The only origin the credential may be sent to.
        origin: HttpsOrigin,
        /// Scheme and custom header name.
        auth: HttpAuthScheme,
    },
}

impl OperatorConnectionConfig {
    /// The provider of this authority.
    #[must_use]
    pub const fn provider(&self) -> OperatorProvider {
        match self {
            Self::Slack { .. } => OperatorProvider::Slack,
            Self::PagerDuty {} => OperatorProvider::PagerDuty,
            Self::Http { .. } => OperatorProvider::Http,
        }
    }

    /// Build a Slack authority.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorConnectionInvalid`] for an empty workspace ID.
    fn slack(workspace_id: String) -> Result<Self, WyrdError> {
        if workspace_id.trim().is_empty() {
            return Err(invalid("workspace_id", "must not be empty"));
        }
        Ok(Self::Slack { workspace_id })
    }
}

/// Redacted connection read returned by every management surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OperatorConnectionView {
    /// Connection identity.
    pub connection_id: OperatorConnectionId,
    /// Immutable name.
    pub name: ConnectionName,
    /// Provider and nonsecret authority.
    #[serde(flatten)]
    pub config: OperatorConnectionConfig,
    /// Whether Operators may use it.
    pub status: OperatorConnectionStatus,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}

/// Plaintext secret of one connection version; encrypted before storage.
///
/// Never a wire response. `Debug` stays redacted through [`SecretBearer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConnectionSecret {
    /// A Slack bot token, PagerDuty key, bearer token, or header value.
    Token {
        /// The secret value.
        value: SecretBearer,
    },
    /// HTTP basic credentials.
    Basic {
        /// The username.
        username: SecretBearer,
        /// The password.
        password: SecretBearer,
    },
}

impl ConnectionSecret {
    /// Wrap a non-empty single-value secret.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorConnectionInvalid`] for an empty value.
    fn token(value: SecretBearer) -> Result<Self, WyrdError> {
        if value.expose().is_empty() {
            return Err(invalid("secret", "must not be empty"));
        }
        Ok(Self::Token { value })
    }
}

/// Build the stable invalid-connection refusal for `field`.
fn invalid(field: &str, reason: &str) -> WyrdError {
    WyrdError::OperatorConnectionInvalid {
        message: format!("{field}: {reason}"),
        details: json!({ "field": field, "reason": reason }),
    }
}

#[cfg(test)]
mod tests {
    //! Wire shapes, normalization, and update semantics of connections.

    use super::*;

    /// Origins normalize scheme, host case, and default port; extras refuse.
    #[test]
    fn origins_normalize_and_refuse_extras() {
        let origin = HttpsOrigin::parse("HTTPS://Hooks.Example.COM:443").expect("valid origin");
        assert_eq!(origin.as_str(), "https://hooks.example.com");
        assert_eq!(
            HttpsOrigin::parse("https://hooks.example.com:8443/")
                .expect("explicit port")
                .as_str(),
            "https://hooks.example.com:8443"
        );
        assert!(HttpsOrigin::parse("http://127.0.0.1:9000").is_ok());
        for bad in [
            "http://hooks.example.com",
            "https://hooks.example.com/path",
            "https://hooks.example.com/?q=1",
            "https://user:pw@hooks.example.com",
            "ftp://hooks.example.com",
        ] {
            assert!(HttpsOrigin::parse(bad).is_err(), "{bad}");
        }
    }

    /// Each provider create splits into redacted config and secret; Debug hides it.
    #[test]
    fn create_splits_config_from_secret() {
        let request: CreateOperatorConnectionRequest = serde_json::from_value(json!({
            "provider": "http", "name": "hooks", "origin": "https://hooks.example.com",
            "auth": { "scheme": "header", "name": "X-Api-Key", "value": "s3cret" }
        }))
        .expect("documented create parses");
        assert!(!format!("{request:?}").contains("s3cret"));
        let (name, config, secret) = request.split().expect("valid create");
        assert_eq!(name.as_str(), "hooks");
        assert_eq!(
            serde_json::to_value(&config).expect("config serializes"),
            json!({ "provider": "http", "origin": "https://hooks.example.com", "auth": { "scheme": "header", "name": "X-Api-Key" } })
        );
        assert!(matches!(secret, ConnectionSecret::Token { value } if value.expose() == "s3cret"));

        let empty: CreateOperatorConnectionRequest = serde_json::from_value(json!({
            "provider": "pager_duty", "name": "pagerduty", "integration_key": ""
        }))
        .expect("parses");
        assert_eq!(
            empty.split().expect_err("empty key").code(),
            "WYRD_OPERATOR_400_INVALID_CONNECTION"
        );
        let host: CreateOperatorConnectionRequest = serde_json::from_value(json!({
            "provider": "http", "name": "hooks", "origin": "https://hooks.example.com",
            "auth": { "scheme": "header", "name": "Authorization", "value": "x" }
        }))
        .expect("parses");
        assert!(host.split().is_err());
    }

    /// Updates preserve omitted fields, replace supplied ones, and refuse a
    /// provider change or a name.
    #[test]
    fn update_preserves_replaces_and_refuses_provider_change() {
        let stored = OperatorConnectionConfig::Http {
            origin: HttpsOrigin::parse("https://hooks.example.com").expect("origin"),
            auth: HttpAuthScheme::Bearer,
        };
        let status_only: UpdateOperatorConnectionRequest =
            serde_json::from_value(json!({ "provider": "http", "status": "disabled" }))
                .expect("parses");
        let update = status_only.apply(stored.clone()).expect("status update");
        assert_eq!(update.config, stored);
        assert!(update.secret.is_none());
        assert_eq!(update.status, Some(OperatorConnectionStatus::Disabled));

        let rotate: UpdateOperatorConnectionRequest = serde_json::from_value(json!({
            "provider": "http", "auth": { "scheme": "basic", "username": "u", "password": "p" }
        }))
        .expect("parses");
        let update = rotate.apply(stored.clone()).expect("auth update");
        assert!(matches!(
            update.config,
            OperatorConnectionConfig::Http {
                auth: HttpAuthScheme::Basic,
                ..
            }
        ));
        assert!(update.secret.is_some());

        let wrong: UpdateOperatorConnectionRequest =
            serde_json::from_value(json!({ "provider": "slack", "bot_token": "x" }))
                .expect("parses");
        assert!(wrong.apply(stored).is_err());
        assert!(
            serde_json::from_value::<UpdateOperatorConnectionRequest>(
                json!({ "provider": "slack", "name": "renamed" })
            )
            .is_err()
        );
    }

    /// Views flatten the provider authority and never carry a secret field.
    #[test]
    fn view_shape_is_redacted() {
        let view = OperatorConnectionView {
            connection_id: OperatorConnectionId::new_v7(),
            name: ConnectionName::new("ops-slack").expect("name"),
            config: OperatorConnectionConfig::Slack {
                workspace_id: "T1".to_owned(),
            },
            status: OperatorConnectionStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let value = serde_json::to_value(&view).expect("serializes");
        assert_eq!(value["provider"], "slack");
        assert_eq!(value["workspace_id"], "T1");
        assert!(value.get("bot_token").is_none());
        let back: OperatorConnectionView = serde_json::from_value(value).expect("round trips");
        assert_eq!(back, view);
    }
}
