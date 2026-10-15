//! Provider deployment, adapter, and upstream authentication contracts.

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::{BUILTIN_PROVIDER_IDS, GatewayContractError, GatewayOperation};
use crate::auth::AbsoluteUrl;
use crate::ids::{ModelId, ProviderCredentialName, ProviderDeploymentName, ProviderId};

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
/// The built-in variants use Skald's typed provider protocols and authenticate
/// with a credential of their reserved provider identity. `OpenAiCompatible`
/// uses the standard OpenAI routes under `base_url` with a credential of any
/// non-reserved provider identity.
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
    /// TypeSafe Jev `systemone` question protocol.
    #[serde(rename = "jev")]
    Jev,
    /// Standard OpenAI-compatible routes under a tenant base URL.
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible {
        /// Absolute base URL whose standard OpenAI routes the deployment uses.
        base_url: AbsoluteUrl,
    },
}

impl ProviderAdapter {
    /// Reserved credential-provider identity a built-in adapter requires, or
    /// `None` for `OpenAiCompatible`.
    #[must_use]
    pub const fn builtin_provider(&self) -> Option<&'static str> {
        match self {
            Self::OpenAi => Some("openai"),
            Self::Anthropic => Some("anthropic"),
            Self::Gemini => Some("gemini"),
            Self::Vertex { .. } => Some("vertex"),
            Self::Jev => Some("jev"),
            Self::OpenAiCompatible { .. } => None,
        }
    }

    /// Whether a credential of `provider` may authenticate this adapter.
    ///
    /// A built-in adapter accepts only its reserved provider identity, and
    /// `OpenAiCompatible` only a non-reserved one. The deployment name never
    /// takes part, so naming a deployment cannot widen credential matching.
    #[must_use]
    pub fn accepts_credential_provider(&self, provider: &ProviderId) -> bool {
        match self.builtin_provider() {
            Some(reserved) => reserved == provider.as_str(),
            None => !BUILTIN_PROVIDER_IDS.contains(&provider.as_str()),
        }
    }

    /// Whether the gateway carries `operation` over this adapter.
    ///
    /// This is the typed capability source: deployment validation, request
    /// preparation, and the published operation-support table all agree with
    /// it. `OpenAI`-protocol adapters carry every `OpenAI` operation natively;
    /// Anthropic and Google adapters carry Chat Completions only. Jev alone
    /// carries [`GatewayOperation::SystemOne`], plus Chat Completions as a
    /// pass-through whose last user message is the typed Jev request; it
    /// never streams, embeds, or calls tools.
    #[must_use]
    pub const fn serves(&self, operation: GatewayOperation) -> bool {
        match self {
            Self::OpenAi | Self::OpenAiCompatible { .. } => {
                !matches!(operation, GatewayOperation::SystemOne)
            }
            Self::Anthropic | Self::Gemini | Self::Vertex { .. } => {
                matches!(operation, GatewayOperation::ChatCompletions)
            }
            Self::Jev => matches!(
                operation,
                GatewayOperation::SystemOne | GatewayOperation::ChatCompletions
            ),
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

/// Who may invoke a deployment beyond its administrators.
///
/// Neither value permits anonymous inference: every call still needs a
/// `gateway:invoke` grant of its effective subject. `Open` admits the
/// unscoped `all` grant; `Granted` admits only a grant naming the deployment
/// or one of its exact models.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DeploymentAccess {
    /// Any subject holding a covering `gateway:invoke` grant, including `all`.
    #[default]
    Open,
    /// Only subjects holding a deployment- or model-scoped grant covering it.
    Granted,
}

/// One named tenant inference target.
///
/// Callers address it as `<name>/<model>`; the name selects exactly this
/// adapter and credential binding. The same shape is the `PUT` body and the
/// redacted read view because it carries no secret material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ProviderDeployment {
    /// Tenant-unique deployment name, the first segment of every address.
    pub name: ProviderDeploymentName,
    /// Upstream protocol family.
    pub adapter: ProviderAdapter,
    /// Upstream authentication.
    pub auth: ProviderAuth,
    /// Models callers may address; absent applies no Wyrd allowlist, which
    /// promises neither upstream entitlement nor model existence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<ModelId>>,
    /// Operations narrowing what the adapter serves; absent permits every
    /// adapter-supported operation. Never inferred by probing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<BTreeSet<GatewayOperation>>,
    /// Who may invoke the deployment.
    #[serde(default)]
    pub access: DeploymentAccess,
}

impl ProviderDeployment {
    /// Rejects a deployment whose model allowlist or capabilities are invalid.
    ///
    /// A present `models` list must be nonempty and unique, and a present
    /// `capabilities` set nonempty with every operation one the adapter
    /// [serves](ProviderAdapter::serves). Credential existence, tenancy, and
    /// provider matching are enforced by the server against persisted
    /// credentials.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming `models` or `capabilities` for
    /// the violated rule.
    pub fn validate(&self) -> Result<(), GatewayContractError> {
        if let Some(models) = &self.models {
            if models.is_empty() {
                return Err(GatewayContractError::new(
                    "models",
                    "must be omitted or list at least one model",
                ));
            }
            if models.iter().collect::<BTreeSet<_>>().len() != models.len() {
                return Err(GatewayContractError::new(
                    "models",
                    "must not list a model twice",
                ));
            }
        }
        if let Some(capabilities) = &self.capabilities {
            if capabilities.is_empty() {
                return Err(GatewayContractError::new(
                    "capabilities",
                    "must be omitted or declare at least one operation",
                ));
            }
            if capabilities
                .iter()
                .any(|operation| !self.adapter.serves(*operation))
            {
                return Err(GatewayContractError::new(
                    "capabilities",
                    "declares an operation the adapter does not serve",
                ));
            }
        }
        Ok(())
    }

    /// Whether the deployment serves `operation`: its adapter carries it and
    /// any declared capabilities include it.
    #[must_use]
    pub fn serves(&self, operation: GatewayOperation) -> bool {
        self.adapter.serves(operation)
            && self
                .capabilities
                .as_ref()
                .is_none_or(|capabilities| capabilities.contains(&operation))
    }

    /// Whether callers may address `model` on this deployment: true unless
    /// a declared allowlist omits it.
    #[must_use]
    pub fn permits(&self, model: &ModelId) -> bool {
        self.models
            .as_ref()
            .is_none_or(|models| models.contains(model))
    }
}

#[cfg(test)]
mod tests {
    use super::{DeploymentAccess, ProviderAuthHeader, ProviderDeployment};
    use crate::gateway::GatewayOperation;
    use crate::ids::{ModelId, ProviderId};
    use serde_json::json;

    /// Decodes one deployment document.
    fn decode(value: serde_json::Value) -> ProviderDeployment {
        serde_json::from_value(value).expect("deployment decodes")
    }

    /// Proves the minimal shape defaults to open, unrestricted, and every
    /// adapter operation, and the full shape round-trips exactly.
    #[test]
    fn deployment_round_trips_minimal_and_full_shapes() {
        let minimal = decode(json!({
            "name": "openai-primary",
            "adapter": "openai",
            "auth": {"bearer": {"credential": "openai-key"}},
        }));
        minimal.validate().expect("valid");
        assert_eq!(minimal.access, DeploymentAccess::Open);
        assert!(minimal.permits(&ModelId::new("any/model").expect("model")));
        assert!(minimal.serves(GatewayOperation::Embeddings));
        assert_eq!(
            serde_json::to_value(&minimal).expect("encodes"),
            json!({
                "name": "openai-primary",
                "adapter": "openai",
                "auth": {"bearer": {"credential": "openai-key"}},
                "access": "open",
            })
        );
        let full = json!({
            "name": "support-gateway",
            "adapter": {"openai_compatible": {"base_url": "https://gateway.example/v1"}},
            "auth": {"api_key_header": {"header": "x-api-key", "credential": "external-key"}},
            "models": ["gpt-4o", "org/model-v3"],
            "capabilities": ["chat_completions"],
            "access": "granted",
        });
        let parsed = decode(full.clone());
        parsed.validate().expect("valid");
        assert_eq!(serde_json::to_value(&parsed).expect("encodes"), full);
        assert!(parsed.permits(&ModelId::new("org/model-v3").expect("model")));
        assert!(!parsed.permits(&ModelId::new("gpt-4.1").expect("model")));
        assert!(!parsed.serves(GatewayOperation::Embeddings));
    }

    /// Proves the removed single-model and weight fields, unknown adapters,
    /// unknown fields, and reserved headers fail at decode.
    #[test]
    fn invalid_shapes_fail_at_decode() {
        let base = json!({"name": "primary", "adapter": "openai", "auth": "none"});
        for (field, value) in [
            ("model", json!({"provider": "openai", "model": "gpt-4o"})),
            ("routing_weight", json!(1)),
            ("status", json!("active")),
            ("access", json!("public")),
        ] {
            let mut document = base.clone();
            document[field] = value;
            assert!(
                serde_json::from_value::<ProviderDeployment>(document).is_err(),
                "{field}"
            );
        }
        let mut bedrock = base;
        bedrock["adapter"] = json!("bedrock");
        assert!(serde_json::from_value::<ProviderDeployment>(bedrock).is_err());
        for reserved in [
            "Authorization",
            "X-Forwarded-For",
            "host",
            "Proxy-Foo",
            "bad header",
        ] {
            assert!(ProviderAuthHeader::new(reserved).is_err(), "{reserved}");
        }
    }

    /// Proves present allowlists and capability sets must be nonempty, unique,
    /// and served by the adapter.
    ///
    /// # Panics
    ///
    /// Panics when an invalid list validates or a valid one fails.
    #[test]
    fn allowlists_and_capabilities_are_validated() {
        let with = |field: &str, value: serde_json::Value, adapter: serde_json::Value| {
            let mut document = json!({"name": "primary", "adapter": adapter, "auth": "none"});
            document[field] = value;
            decode(document).validate().map_err(|error| error.field)
        };
        assert_eq!(
            with("models", json!([]), json!("openai")),
            Err("models".to_owned())
        );
        assert_eq!(
            with("models", json!(["gpt-4o", "gpt-4o"]), json!("openai")),
            Err("models".to_owned())
        );
        assert_eq!(
            with("capabilities", json!([]), json!("openai")),
            Err("capabilities".to_owned())
        );
        assert_eq!(
            with("capabilities", json!(["embeddings"]), json!("anthropic")),
            Err("capabilities".to_owned()),
            "anthropic serves no embeddings"
        );
        assert_eq!(
            with("capabilities", json!(["system_one"]), json!("openai")),
            Err("capabilities".to_owned()),
            "only Jev serves systemone questions"
        );
        assert_eq!(
            with(
                "capabilities",
                json!(["system_one", "chat_completions"]),
                json!("jev")
            ),
            Ok(())
        );
        let jev = decode(json!({"name": "jev", "adapter": "jev", "auth": "none"}));
        assert!(jev.serves(GatewayOperation::SystemOne));
        assert!(!jev.serves(GatewayOperation::Embeddings));
    }

    /// Proves credential-provider matching follows the adapter, never the
    /// deployment name: built-ins need their reserved identity and
    /// compatible adapters a non-reserved one.
    #[test]
    fn credential_provider_follows_the_adapter() {
        let provider = |name: &str| ProviderId::new(name).expect("provider");
        let adapter = |value: serde_json::Value| {
            decode(json!({"name": "anything", "adapter": value, "auth": "none"})).adapter
        };
        let openai = adapter(json!("openai"));
        assert!(openai.accepts_credential_provider(&provider("openai")));
        assert!(!openai.accepts_credential_provider(&provider("external")));
        let vertex = adapter(json!({"vertex": {"project": "proj-1", "location": "us-central1"}}));
        assert!(vertex.accepts_credential_provider(&provider("vertex")));
        let compatible =
            adapter(json!({"openai_compatible": {"base_url": "https://example.com/v1"}}));
        assert!(compatible.accepts_credential_provider(&provider("external")));
        for reserved in ["openai", "jev", "gemini"] {
            assert!(!compatible.accepts_credential_provider(&provider(reserved)));
        }
    }
}
