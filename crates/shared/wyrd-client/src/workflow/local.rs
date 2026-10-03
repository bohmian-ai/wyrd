//! Local execution dependencies for the routes a Workflow selects.
//!
//! [`SelectedRoutes`] reads a Workflow's resolved step routes and prepares
//! only what they need from the shared client configuration: one
//! [`ExternalGatewayBinding`] per selected `ext_gateway` binding, with its
//! secret headers resolved at run time. Unselected bindings are never read.
//! A selected binding absent from configuration is left out, so Skald refuses
//! the run before any dispatch.

use std::collections::{BTreeSet, HashMap};

use reqwest::header::HeaderName;
use secrecy::SecretString;
use skald_runtime::ProviderRegistry;
use skald_workflow::{
    ExternalGatewayBinding, ExternalGatewayBindings, WorkflowExecutionDependencies,
};
use tokio::io::AsyncReadExt as _;
use wyrd_spec::card::workflow::{ExternalGatewayBindingConfig, LlmRoute, WorkflowSpec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CredentialBindingName;
use wyrd_spec::security::SecretRef;

use crate::global_config::LocalWorkflowConfig;

/// Largest secret file a binding header may read.
const MAX_SECRET_FILE_BYTES: u64 = 64 * 1024;

/// The non-native routes a Workflow's steps resolve to.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SelectedRoutes {
    /// External gateway bindings named by selected `ext_gateway` routes.
    external: BTreeSet<CredentialBindingName>,
}

impl SelectedRoutes {
    /// Collect the routes `spec`'s steps resolve to.
    pub(super) fn of(spec: &WorkflowSpec) -> Self {
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
        Self { external }
    }

    /// Whether any selected route needs shared configuration.
    pub(super) fn needs_config(&self) -> bool {
        !self.external.is_empty()
    }

    /// Build execution dependencies over `native` for the selected routes.
    ///
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
    ) -> Result<WorkflowExecutionDependencies, WyrdError> {
        let mut bindings = ExternalGatewayBindings::new();
        for name in &self.external {
            if let Some(binding) = config.external_gateway_bindings.get(name) {
                bindings.insert(resolve_binding(name, binding).await?)?;
            }
        }
        Ok(WorkflowExecutionDependencies::new(native).with_external_gateways(bindings))
    }
}

/// Resolve one configured binding's secret headers.
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
        let value = read_secret(secret)
            .await
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

/// Read one secret from an environment variable or an owner-only file.
///
/// A file is checked through metadata of the already-open handle: it must be
/// a regular file, on Unix with no group or other permission bits, and at
/// most [`MAX_SECRET_FILE_BYTES`]. Other reference kinds are unsupported
/// locally. Returns `None` for any refusal so no path or value is reported.
async fn read_secret(secret: &SecretRef) -> Option<SecretString> {
    let path = match secret {
        SecretRef::Env { name } => return std::env::var(name).ok().map(SecretString::from),
        SecretRef::File { path } => path,
        _ => return None,
    };
    let file = tokio::fs::File::open(path).await.ok()?;
    let metadata = file.metadata().await.ok()?;
    if !owner_only(&metadata) {
        return None;
    }
    let mut value = String::new();
    let read = file
        .take(MAX_SECRET_FILE_BYTES + 1)
        .read_to_string(&mut value)
        .await
        .ok()?;
    (read as u64 <= MAX_SECRET_FILE_BYTES).then(|| SecretString::from(value))
}

/// Whether an opened secret file is regular and owner-only.
#[cfg(unix)]
fn owner_only(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.is_file() && metadata.permissions().mode() & 0o077 == 0
}

/// Whether an opened secret file is regular; access restriction is a
/// deployment requirement on platforms without Unix mode bits.
#[cfg(not(unix))]
fn owner_only(metadata: &std::fs::Metadata) -> bool {
    metadata.is_file()
}

/// A binding-unavailable refusal naming the binding and a fixed reason.
fn binding_unavailable(name: &CredentialBindingName, reason: &str) -> WyrdError {
    WyrdError::WorkflowBindingUnavailable {
        message: format!("external gateway binding '{name}': {reason}"),
        details: serde_json::json!({ "binding": name.as_str(), "reason": reason }),
    }
}
