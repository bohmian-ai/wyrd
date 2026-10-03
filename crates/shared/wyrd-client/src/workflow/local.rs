//! Local execution dependencies for the routes a Workflow selects.
//!
//! [`SelectedRoutes`] reads a Workflow's resolved step routes and prepares
//! only what they need: a [`PublicWyrdGatewayCaller`] over the Workflow's
//! client when a step uses `wyrd_gateway`, and, from the shared client
//! configuration, one [`ExternalGatewayBinding`] per selected `ext_gateway`
//! binding, with its secret headers resolved at run time. Unselected bindings
//! are never read.
//! A selected binding absent from configuration is left out, so Skald refuses
//! the run before any dispatch.

use std::collections::{BTreeSet, HashMap};

use reqwest::header::HeaderName;
use skald_runtime::ProviderRegistry;
use skald_workflow::{
    ExternalGatewayBinding, ExternalGatewayBindings, WorkflowExecutionDependencies,
    WyrdGatewayCaller,
};
use wyrd_spec::card::workflow::{ExternalGatewayBindingConfig, LlmRoute, WorkflowSpec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CredentialBindingName;
use wyrd_utils::secret::read_secret_ref;

use super::PublicWyrdGatewayCaller;
use crate::WyrdClient;
use crate::global_config::LocalWorkflowConfig;

/// The non-native routes a Workflow's steps resolve to.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SelectedRoutes {
    /// Whether any step resolves to the `wyrd_gateway` route.
    wyrd_gateway: bool,
    /// External gateway bindings named by selected `ext_gateway` routes.
    external: BTreeSet<CredentialBindingName>,
}

impl SelectedRoutes {
    /// Collect the routes `spec`'s steps resolve to.
    pub(super) fn of(spec: &WorkflowSpec) -> Self {
        let wyrd_gateway = spec
            .steps
            .iter()
            .any(|step| *spec.resolved_route(step) == LlmRoute::WyrdGateway);
        let external = spec
            .steps
            .iter()
            .filter_map(|step| match spec.resolved_route(step) {
                LlmRoute::ExtGateway {
                    credential_binding, ..
                } => Some(credential_binding.clone()),
                LlmRoute::Native | LlmRoute::WyrdGateway => None,
            })
            .collect();
        Self {
            wyrd_gateway,
            external,
        }
    }

    /// Whether any selected route needs a Wyrd gateway client.
    pub(super) fn needs_gateway(&self) -> bool {
        self.wyrd_gateway
    }

    /// Whether any selected route needs shared configuration.
    pub(super) fn needs_config(&self) -> bool {
        !self.external.is_empty()
    }

    /// Build execution dependencies over `native` for the selected routes.
    ///
    /// When a step uses `wyrd_gateway`, `gateway` serves it through the
    /// public ingress; without one Skald refuses the run before dispatch.
    /// Each selected binding present in `config` has its secret headers
    /// resolved now; bindings no route selects are never read.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` when a selected
    /// binding's secret header name is invalid, its secret cannot be read, or
    /// Skald refuses the binding. No error includes a secret value.
    pub(super) async fn dependencies(
        &self,
        native: ProviderRegistry,
        config: &LocalWorkflowConfig,
        gateway: Option<WyrdClient>,
    ) -> Result<WorkflowExecutionDependencies, WyrdError> {
        let mut bindings = ExternalGatewayBindings::new();
        for name in &self.external {
            if let Some(binding) = config.external_gateway_bindings.get(name) {
                bindings.insert(resolve_binding(name, binding).await?)?;
            }
        }
        let dependencies =
            WorkflowExecutionDependencies::new(native).with_external_gateways(bindings);
        Ok(match gateway.filter(|_| self.wyrd_gateway) {
            Some(client) => dependencies
                .with_wyrd_gateway(std::sync::Arc::new(PublicWyrdGatewayCaller::new(client))
                    as std::sync::Arc<dyn WyrdGatewayCaller>),
            None => dependencies,
        })
    }
}

/// Resolve one configured binding's secret headers.
///
/// Each secret is read by the shared [`read_secret_ref`] on the blocking
/// pool, under its owner-only, bounded file rule.
///
/// # Errors
/// Returns `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` for an invalid header name
/// or an unreadable secret.
async fn resolve_binding(
    name: &CredentialBindingName,
    config: &ExternalGatewayBindingConfig,
) -> Result<ExternalGatewayBinding, WyrdError> {
    let mut secret_headers = HashMap::with_capacity(config.secret_headers.len());
    for (header, secret) in &config.secret_headers {
        let header = HeaderName::from_bytes(header.as_bytes()).map_err(|_| {
            binding_unavailable(name, "secret header names must be valid HTTP field names")
        })?;
        let secret = secret.clone();
        let value = tokio::task::spawn_blocking(move || read_secret_ref(&secret))
            .await
            .ok()
            .and_then(Result::ok)
            .ok_or_else(|| binding_unavailable(name, "a secret header value is unavailable"))?;
        secret_headers.insert(header, value);
    }
    Ok(ExternalGatewayBinding {
        name: name.clone(),
        protocol: config.protocol,
        origin: config.origin.clone(),
        secret_headers,
    })
}

/// A binding-unavailable refusal naming the binding and a fixed reason.
fn binding_unavailable(name: &CredentialBindingName, reason: &str) -> WyrdError {
    WyrdError::WorkflowBindingUnavailable {
        message: format!("external gateway binding '{name}': {reason}"),
        details: serde_json::json!({ "binding": name.as_str(), "reason": reason }),
    }
}
