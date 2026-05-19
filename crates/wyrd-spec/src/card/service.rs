//! Service Card spec and lockfile helper schemas.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::{CredentialRef, NonSecretValue};
use crate::reference::CardRef;

/// Composition of cards used by an application or deployment.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ServiceSpec {
    /// Service description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Service type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
    /// Alias-bound components.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ServiceComponent>,
    /// Entry point descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_point: Option<String>,
    /// Deployment metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deployment: BTreeMap<String, NonSecretValue>,
    /// Optional server-side runtime declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<ServiceRuntime>,
    /// Service config.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub service_config: BTreeMap<String, NonSecretValue>,
    /// Credential references required by this service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_refs: Vec<CredentialRef>,
    /// Content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Lock hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_hash: Option<String>,
    /// Free-form metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}

/// Server-side runtime declaration for a Service Card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ServiceRuntime {
    /// Runtime kind.
    pub kind: ServiceRuntimeKind,
    /// Framework or adapter hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Runtime placement mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ServiceRuntimeMode>,
    /// Whether the runtime should fail closed on unsupported features.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    /// Runtime policy settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<ServiceRuntimePolicy>,
    /// Non-secret runtime configuration.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub config: serde_json::Value,
}

/// Service runtime kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ServiceRuntimeKind {
    /// API service runtime.
    Api,
    /// MCP service runtime.
    Mcp,
    /// Agent service runtime.
    Agent,
    /// Workflow service runtime.
    Workflow,
}

/// Service runtime placement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ServiceRuntimeMode {
    /// Run inside Wyrd's server process.
    InProcess,
}

/// Service runtime policy settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ServiceRuntimePolicy {
    /// Whether to invoke Policy Cards around server-side runtime calls.
    #[serde(default)]
    pub runtime_hooks: bool,
}

/// Alias-bound service component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ServiceComponent {
    /// Local AppState handle.
    pub alias: String,
    /// Canonical Card reference.
    #[serde(rename = "ref")]
    pub card_ref: CardRef,
    /// Optional development-time source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ComponentSource>,
    /// Component config.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, NonSecretValue>,
    /// Component credential references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_refs: Vec<CredentialRef>,
}

/// Development-time local source resolved by `lock` or `install`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ComponentSource {
    /// Local path.
    pub path: String,
    /// Source kind hint.
    pub kind: String,
}

/// Installed service lock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ServiceLock {
    /// Locked service reference.
    #[serde(rename = "ref")]
    pub service_ref: CardRef,
    /// Locked components.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<LockedComponent>,
    /// Lock hash.
    pub lock_hash: String,
    /// Lock metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}

/// Locked resolved service component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct LockedComponent {
    /// Local AppState handle.
    pub alias: String,
    /// Exact locked Card reference.
    #[serde(rename = "ref")]
    pub card_ref: CardRef,
    /// Installed artifact path, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_path: Option<String>,
    /// Content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Component metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}
