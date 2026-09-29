//! Provider deployment, adapter, and upstream authentication contracts.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize};

use super::{BUILTIN_PROVIDER_IDS, GatewayContractError, GatewayOperation, ModelRef};
use crate::auth::AbsoluteUrl;
use crate::ids::{ProviderCredentialName, ProviderDeploymentName};

/// Header names an upstream API-key header may never use.
///
/// These carry caller identity, routing, framing, or connection semantics the
/// gateway owns; allowing a tenant to set them would forward or forge them.
const RESERVED_HEADERS: [&str; 20] = [
    "authorization",
    "proxy-authorization",
    "proxy-authenticate",
    "www-authenticate",
    "host",
    "forwarded",
    "via",
    "connection",
    "keep-alive",
    "transfer-encoding",
    "te",
    "trailer",
    "upgrade",
    "expect",
    "content-length",
    "content-type",
    "cookie",
    "set-cookie",
    "x-real-ip",
    "wyrd-request-id",
];

/// Header-name prefixes an upstream API-key header may never use.
const RESERVED_HEADER_PREFIXES: [&str; 4] = ["proxy-", "x-forwarded-", "sec-", "wyrd-"];

/// Upstream protocol family a deployment speaks.
///
/// The four built-in variants use Skald's typed provider protocols and require
/// their reserved provider identity. `OpenAiCompatible` uses the standard
/// OpenAI routes under `base_url` for any non-reserved provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ProviderAdapter {
    /// OpenAI native protocol.
    #[serde(rename = "openai")]
    OpenAi,
    /// Anthropic Messages protocol.
    #[serde(rename = "anthropic")]
    Anthropic,
    /// Google Gemini GenerateContent protocol.
    #[serde(rename = "gemini")]
    Gemini,
    /// Google Vertex GenerateContent protocol.
    #[serde(rename = "vertex")]
    Vertex {
        /// Google Cloud project identifier.
        project: VertexLocation,
        /// Google Cloud location, such as `us-central1`.
        location: VertexLocation,
    },
    /// Standard OpenAI-compatible routes under a tenant base URL.
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible {
        /// Absolute base URL whose standard OpenAI routes the deployment uses.
        base_url: AbsoluteUrl,
    },
}

impl ProviderAdapter {
    /// Reserved provider identity a built-in adapter requires, or `None` for
    /// `OpenAiCompatible`.
    #[must_use]
    pub const fn builtin_provider(&self) -> Option<&'static str> {
        match self {
            Self::OpenAi => Some("openai"),
            Self::Anthropic => Some("anthropic"),
            Self::Gemini => Some("gemini"),
            Self::Vertex { .. } => Some("vertex"),
            Self::OpenAiCompatible { .. } => None,
        }
    }

    /// Whether the gateway carries `operation` over this adapter.
    ///
    /// This is the typed capability source: deployment validation, request
    /// preparation, and the published operation-support table all agree with
    /// it. `OpenAI`-protocol adapters carry every operation natively; Anthropic
    /// and Google adapters carry Chat Completions only.
    #[must_use]
    pub const fn serves(&self, operation: GatewayOperation) -> bool {
        match self {
            Self::OpenAi | Self::OpenAiCompatible { .. } => true,
            Self::Anthropic | Self::Gemini | Self::Vertex { .. } => {
                matches!(operation, GatewayOperation::ChatCompletions)
            }
        }
    }
}

/// Google Cloud project or location segment used by the Vertex adapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct VertexLocation(String);

impl VertexLocation {
    /// Builds a validated Vertex project or location segment.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] unless `value` is 1..=128 ASCII
    /// alphanumerics, `-`, or `_`.
    pub fn new(value: &str) -> Result<Self, GatewayContractError> {
        let valid = !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(GatewayContractError::new(
                "adapter.vertex",
                "project and location must be 1..=128 ASCII alphanumerics, '-' or '_'",
            ))
        }
    }

    /// Borrows the segment.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for VertexLocation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(serde::de::Error::custom)
    }
}

/// Authentication the gateway presents to an upstream provider.
///
/// Unrelated to the caller's Wyrd access token, which is never forwarded. The
/// header value is built from the referenced tenant credential at call time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuth {
    /// No upstream authentication.
    None,
    /// `Authorization: Bearer <credential>`.
    Bearer {
        /// Tenant credential supplying the bearer value.
        credential: ProviderCredentialName,
    },
    /// A named API-key header carrying the credential.
    ApiKeyHeader {
        /// Validated non-reserved HTTP field name.
        header: ProviderAuthHeader,
        /// Tenant credential supplying the header value.
        credential: ProviderCredentialName,
    },
}

impl ProviderAuth {
    /// Credential this authentication references, if any.
    #[must_use]
    pub const fn credential(&self) -> Option<&ProviderCredentialName> {
        match self {
            Self::None => None,
            Self::Bearer { credential } | Self::ApiKeyHeader { credential, .. } => Some(credential),
        }
    }
}

/// HTTP field name for an upstream API-key header.
///
/// Stored lowercase. Authorization, proxy, host, forwarding, connection,
/// framing, cookie, and Wyrd-owned headers are rejected so a tenant cannot
/// forward or forge security-sensitive upstream headers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct ProviderAuthHeader(String);

impl ProviderAuthHeader {
    /// Builds a validated, lowercased header name.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] for field `auth.header` when `value` is
    /// not a 1..=128 byte RFC 9110 token or names a reserved header.
    pub fn new(value: &str) -> Result<Self, GatewayContractError> {
        let token = !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte));
        if !token {
            return Err(GatewayContractError::new(
                "auth.header",
                "must be a valid HTTP field name",
            ));
        }
        let lower = value.to_ascii_lowercase();
        if RESERVED_HEADERS.contains(&lower.as_str())
            || RESERVED_HEADER_PREFIXES
                .iter()
                .any(|prefix| lower.starts_with(prefix))
        {
            return Err(GatewayContractError::new(
                "auth.header",
                "names a reserved or security-sensitive header",
            ));
        }
        Ok(Self(lower))
    }

    /// Borrows the lowercase header name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ProviderAuthHeader {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(serde::de::Error::custom)
    }
}

/// One tenant-authorized upstream model target.
///
/// The name is an administration identifier only; inference callers select a
/// [`ModelRef`], never a deployment. The same shape is the `PUT` body and the
/// redacted read view because it carries no secret material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ProviderDeployment {
    /// Tenant-unique deployment name.
    pub name: ProviderDeploymentName,
    /// Exact model this deployment serves.
    pub model: ModelRef,
    /// Upstream protocol family.
    pub adapter: ProviderAdapter,
    /// Upstream authentication.
    pub auth: ProviderAuth,
    /// Explicitly declared operations; never inferred by probing.
    pub capabilities: BTreeSet<GatewayOperation>,
    /// Positive weight among deployments serving the same model.
    #[cfg_attr(feature = "server", schema(value_type = u32, minimum = 1))]
    pub routing_weight: NonZeroU32,
}

impl ProviderDeployment {
    /// Rejects a deployment whose adapter, provider, or capabilities disagree.
    ///
    /// Built-in adapters require their reserved provider identity and
    /// `OpenAiCompatible` requires a non-reserved one. At least one capability
    /// must be declared, and every declared capability must be one the adapter
    /// [serves](ProviderAdapter::serves). Credential tenancy and provider matching are enforced
    /// by the server against persisted credentials.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming `model.provider` or
    /// `capabilities` for the violated rule.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        let provider = self.model.provider.as_str();
        match self.adapter.builtin_provider() {
            Some(reserved) if reserved != provider => {
                return Err(GatewayContractError::new(
                    "model.provider",
                    "a built-in adapter requires its reserved provider identity",
                ));
            }
            None if BUILTIN_PROVIDER_IDS.contains(&provider) => {
                return Err(GatewayContractError::new(
                    "model.provider",
                    "an openai_compatible adapter cannot use a reserved provider identity",
                ));
            }
            _ => {}
        }
        if self.capabilities.is_empty() {
            return Err(GatewayContractError::new(
                "capabilities",
                "at least one operation must be declared",
            ));
        }
        if self
            .capabilities
            .iter()
            .any(|operation| !self.adapter.serves(*operation))
        {
            return Err(GatewayContractError::new(
                "capabilities",
                "declares an operation the adapter does not serve",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ProviderAuthHeader, ProviderDeployment};
    use serde_json::json;

    /// Builds one deployment JSON document for `provider` and `adapter`.
    fn deployment(provider: &str, adapter: serde_json::Value) -> serde_json::Value {
        json!({
            "name": "primary",
            "model": {"provider": provider, "model": "model/v1"},
            "adapter": adapter,
            "auth": {"api_key_header": {"header": "X-Api-Key", "credential": "deepseek-key"}},
            "capabilities": ["embeddings", "chat_completions"],
            "routing_weight": 3,
        })
    }

    /// Proves the exact wire shape round-trips and header names normalize.
    #[test]
    fn deployment_round_trips_exact_shape() {
        let value = deployment(
            "deepseek",
            json!({"openai_compatible": {"base_url": "https://api.deepseek.example/v1"}}),
        );
        let parsed: ProviderDeployment = serde_json::from_value(value).expect("decodes");
        parsed.validate().expect("valid");
        assert_eq!(
            serde_json::to_value(&parsed).expect("encodes"),
            json!({
                "name": "primary",
                "model": {"provider": "deepseek", "model": "model/v1"},
                "adapter": {"openai_compatible": {"base_url": "https://api.deepseek.example/v1"}},
                "auth": {"api_key_header": {"header": "x-api-key", "credential": "deepseek-key"}},
                "capabilities": ["chat_completions", "embeddings"],
                "routing_weight": 3,
            })
        );
    }

    /// Proves reserved providers bind to their adapters in both directions.
    #[test]
    fn adapter_provider_reservation_is_enforced() {
        let builtin_mismatch: ProviderDeployment =
            serde_json::from_value(deployment("deepseek", json!("openai"))).expect("decodes");
        assert!(builtin_mismatch.validate().is_err());
        let compatible_reserved: ProviderDeployment = serde_json::from_value(deployment(
            "openai",
            json!({"openai_compatible": {"base_url": "https://example.com"}}),
        ))
        .expect("decodes");
        assert!(compatible_reserved.validate().is_err());
        let mut vertex = deployment(
            "vertex",
            json!({"vertex": {"project": "proj-1", "location": "us-central1"}}),
        );
        vertex["capabilities"] = json!(["chat_completions"]);
        let vertex: ProviderDeployment = serde_json::from_value(vertex).expect("decodes");
        vertex
            .validate()
            .expect("vertex binds to its reserved provider");
    }

    /// Proves invalid shapes fail at decode: zero weight, unknown adapter,
    /// unknown fields, and reserved headers.
    #[test]
    fn invalid_shapes_fail_at_decode() {
        let mut zero = deployment("openai", json!("openai"));
        zero["routing_weight"] = json!(0);
        assert!(serde_json::from_value::<ProviderDeployment>(zero).is_err());
        assert!(
            serde_json::from_value::<ProviderDeployment>(deployment("openai", json!("bedrock")))
                .is_err()
        );
        let mut extra = deployment("openai", json!("openai"));
        extra["status"] = json!("active");
        assert!(serde_json::from_value::<ProviderDeployment>(extra).is_err());
        for reserved in [
            "Authorization",
            "X-Forwarded-For",
            "host",
            "Proxy-Foo",
            "bad header",
        ] {
            assert!(ProviderAuthHeader::new(reserved).is_err(), "{reserved}");
        }
        let mut empty = deployment("openai", json!("openai"));
        empty["capabilities"] = json!([]);
        let empty: ProviderDeployment = serde_json::from_value(empty).expect("decodes");
        assert!(empty.validate().is_err());
    }

    /// Proves a deployment cannot declare an operation its adapter does not
    /// serve, so tenant capability declarations stay truthful.
    ///
    /// # Panics
    ///
    /// Panics when an unserved declaration validates or a served one fails.
    #[test]
    fn capabilities_must_be_served_by_the_adapter() {
        let mut stale = deployment("anthropic", json!("anthropic"));
        stale["capabilities"] = json!(["chat_completions", "embeddings"]);
        let stale: ProviderDeployment = serde_json::from_value(stale).expect("decodes");
        assert_eq!(
            stale
                .validate()
                .expect_err("anthropic serves no embeddings")
                .field,
            "capabilities"
        );
        let mut served = deployment("openai", json!("openai"));
        served["capabilities"] = json!([
            "chat_completions",
            "responses",
            "embeddings",
            "images",
            "audio",
            "batches"
        ]);
        let served: ProviderDeployment = serde_json::from_value(served).expect("decodes");
        served.validate().expect("openai serves every operation");
    }
}
