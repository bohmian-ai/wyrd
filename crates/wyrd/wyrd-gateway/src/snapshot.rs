//! Immutable tenant configuration admitted for one gateway call.
//!
//! `wyrd-server` reads these values from tenant Postgres at admission and the
//! gateway executes against them unchanged, so a concurrent administrative
//! replacement affects only newly admitted calls.

use serde::Deserialize;
use url::Url;
use wyrd_spec::DataTenantId;
use wyrd_spec::gateway::{
    ExternalSecretReference, GatewayCapturePolicy, GatewayFallbackPolicy, GatewayGovernancePolicy,
    ProviderAdapter, ProviderCredentialState, ProviderDeployment,
};
use wyrd_spec::ids::{
    CredentialBindingName, ProviderCredentialName, ProviderId, SecretBackendName,
};
use wyrd_spec::security::SecretRef;

use crate::managed::ManagedSecretEnvelope;

/// Complete gateway configuration of one tenant at call admission.
#[derive(Debug, Clone)]
pub struct GatewayTenantSnapshot {
    /// Deployments ordered by name.
    pub deployments: Vec<ProviderDeployment>,
    /// Credential resolution inputs ordered by name.
    pub credentials: Vec<GatewayCredentialSnapshot>,
    /// Fallback policy.
    pub fallback: GatewayFallbackPolicy,
    /// Governance policy including retained pricing.
    pub governance: GatewayGovernancePolicy,
    /// Capture policy with its version.
    pub capture: GatewayCapturePolicy,
}

impl GatewayTenantSnapshot {
    /// Finds the credential named `name`, if the tenant has one.
    #[must_use]
    pub fn credential(&self, name: &ProviderCredentialName) -> Option<&GatewayCredentialSnapshot> {
        self.credentials
            .iter()
            .find(|credential| &credential.name == name)
    }
}

/// One credential as admitted: identity, lifecycle, and resolution inputs.
#[derive(Debug, Clone)]
pub struct GatewayCredentialSnapshot {
    /// Credential name.
    pub name: ProviderCredentialName,
    /// Provider identity.
    pub provider: ProviderId,
    /// Lifecycle state; the runtime fails closed on `Revoked`.
    pub state: ProviderCredentialState,
    /// Source-specific resolution inputs.
    pub source: GatewayCredentialSource,
    /// Operator assignment covering the source at admission; `None` when the
    /// binding or Vault path has no assignment, which fails closed.
    pub assignment: Option<CredentialAssignment>,
}

/// Operator-owned authority over one Environment binding or Vault path prefix.
///
/// Tenant input can neither create nor widen it. Administration checks it
/// before persistence and the resolver checks the admitted copy again before
/// dispatch, so both paths share [`Self::permits`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialAssignment {
    /// The only tenant that may use the source.
    pub tenant: DataTenantId,
    /// The only provider the source authenticates to.
    pub provider: ProviderId,
    /// Exact endpoint host an `OpenAiCompatible` deployment must target; an
    /// assignment without one is unusable by compatible deployments.
    #[serde(default)]
    pub host: Option<String>,
}

impl CredentialAssignment {
    /// Whether `tenant` may use this source for `provider`, and, when
    /// `adapter` is `OpenAiCompatible`, send it to that base URL's host.
    ///
    /// Built-in adapters are bound by their reserved provider identity, so the
    /// host is not consulted for them.
    #[must_use]
    pub fn permits(
        &self,
        tenant: DataTenantId,
        provider: &ProviderId,
        adapter: Option<&ProviderAdapter>,
    ) -> bool {
        self.tenant == tenant
            && &self.provider == provider
            && match adapter {
                Some(ProviderAdapter::OpenAiCompatible { base_url }) => {
                    let target = Url::parse(base_url.as_str()).ok();
                    let target = target.as_ref().and_then(Url::host_str);
                    self.host
                        .as_deref()
                        .zip(target)
                        .is_some_and(|(host, target)| host.eq_ignore_ascii_case(target))
                }
                _ => true,
            }
    }
}

/// Source-specific inputs the runtime uses to resolve credential plaintext.
#[derive(Debug, Clone)]
pub enum GatewayCredentialSource {
    /// Operator binding and its configured reference; `None` when the operator
    /// has since removed the binding, which fails closed at resolution.
    Environment {
        /// Tenant-selected binding name.
        binding: CredentialBindingName,
        /// Operator environment or file reference.
        secret: Option<SecretRef>,
    },
    /// External backend identity and opaque reference.
    ExternalSecret {
        /// Operator-declared backend identity.
        backend: SecretBackendName,
        /// Opaque backend reference.
        reference: ExternalSecretReference,
    },
    /// Sealed tenant-submitted secret. The snapshot carries the opaque
    /// envelope only; the key opening it is process configuration.
    ManagedSecret {
        /// Sealed envelope and the keyring version that must open it.
        envelope: ManagedSecretEnvelope,
    },
}
