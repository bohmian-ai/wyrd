//! Service Card spec and lockfile helper schemas.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::{CredentialRef, NonSecretValue};
use crate::reference::{CardRef, Ref};

/// Composition of cards used by an application or deployment.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
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
    /// Eval and Drift cards that receive observations from this service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes_to: Vec<Ref>,
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

/// Server-side runtime declaration; tenancy is inherited from `Card.metadata.space`.
// Status: Locked (2026-05-18)
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-4
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-5
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-7
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
    /// Non-secret runtime configuration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, NonSecretValue>,
    /// Runtime policy settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<ServiceRuntimePolicy>,
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
    pub card_ref: Ref,
    /// Eval and Drift cards that receive observations from this component in this Service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes_to: Vec<Ref>,
    /// Optional development-time source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ComponentSource>,
    /// Component config.
    /// TODO: does this earn its place?
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, NonSecretValue>,
    /// Component credential references.
    /// TODO: does this earn its place?
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

#[cfg(test)]
mod runtime_tests {
    use crate::card::common::NonSecretValue;
    use crate::card::service::{
        ServiceRuntime, ServiceRuntimeKind, ServiceRuntimeMode, ServiceRuntimePolicy, ServiceSpec,
    };
    use crate::envelope::{Card, Spec};
    use crate::format;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    #[test]
    fn legacy_service_spec_without_runtime_round_trips_without_runtime_key() {
        let fixture = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/legacy-service-spec-without-runtime.json"
        ));
        let decoded: ServiceSpec = serde_json::from_str(fixture).unwrap();
        assert_eq!(decoded.runtime, None);

        let encoded = serde_json::to_string(&decoded).unwrap();
        assert_eq!(encoded, fixture.trim());
    }

    #[test]
    fn service_runtime_fixture_round_trips() {
        let decoded: Card = format::yaml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/service-with-runtime-policy.yaml"
        )))
        .unwrap();
        let Spec::Service(spec) = &decoded.spec else {
            panic!("expected Service spec");
        };
        let runtime = spec.runtime.as_ref().expect("runtime");
        assert_eq!(runtime.kind, ServiceRuntimeKind::Agent);
        assert_eq!(runtime.mode, Some(ServiceRuntimeMode::InProcess));
        assert_eq!(runtime.strict, Some(true));
        assert!(runtime.policy.as_ref().unwrap().runtime_hooks);

        let encoded = format::yaml::to_string(&decoded).unwrap();
        let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
        assert_eq!(reparsed, decoded);
    }

    fn runtime_strategy() -> impl Strategy<Value = ServiceRuntime> {
        let kind = prop::sample::select(vec![
            ServiceRuntimeKind::Api,
            ServiceRuntimeKind::Mcp,
            ServiceRuntimeKind::Agent,
            ServiceRuntimeKind::Workflow,
        ]);
        let framework = prop::option::of("[a-z][a-z0-9_-]{0,12}".prop_map(String::from));
        let mode = prop::option::of(Just(ServiceRuntimeMode::InProcess));
        let strict = prop::option::of(any::<bool>());
        let policy = prop::option::of(
            any::<bool>().prop_map(|runtime_hooks| ServiceRuntimePolicy { runtime_hooks }),
        );
        let config = prop_oneof![
            Just(BTreeMap::new()),
            Just(BTreeMap::from([(
                "max_concurrent_invocations".to_string(),
                NonSecretValue::Number(4.0),
            )])),
            Just(BTreeMap::from([(
                "queue".to_string(),
                NonSecretValue::Str("default".to_string()),
            )])),
        ];

        (kind, framework, mode, strict, policy, config).prop_map(
            |(kind, framework, mode, strict, policy, config)| ServiceRuntime {
                kind,
                framework,
                mode,
                strict,
                config,
                policy,
            },
        )
    }

    proptest! {
        #[test]
        fn service_runtime_round_trips(runtime in runtime_strategy()) {
            let encoded = serde_json::to_string(&runtime).unwrap();
            let decoded: ServiceRuntime = serde_json::from_str(&encoded).unwrap();
            prop_assert_eq!(decoded, runtime);
        }
    }
}
