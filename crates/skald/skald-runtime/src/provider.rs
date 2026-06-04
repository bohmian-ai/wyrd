//! Provider registry and runtime provider trait.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use async_trait::async_trait;
use skald_providers::{
    AnthropicClient, GoogleClient, OpenAiClient, ProviderClient, ProviderError, ProviderStream,
    VertexClient,
};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};

use crate::mock::MockProvider;

static DEFAULT_REGISTRY: OnceLock<RwLock<Arc<ProviderRegistry>>> = OnceLock::new();

/// Runtime provider seam over the concrete Skald provider clients.
///
/// This trait is intentionally separate from [`ProviderClient`] (in
/// `skald-providers`) to keep `skald-runtime` decoupled from the concrete
/// client crate. The `provider_client_impl!` macro bridges them by delegation.
/// Any new method added to `ProviderClient` must be mirrored here manually.
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    /// Sends one native provider request.
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError>;

    /// Sends one streaming native provider request.
    async fn stream(&self, request: ProviderRequest) -> Result<ProviderStream, ProviderError>;

    /// Returns the provider this client handles.
    fn name(&self) -> ProviderName;
}

macro_rules! provider_client_impl {
    ($client:ty, $name:expr) => {
        #[async_trait]
        impl Provider for $client {
            async fn send(
                &self,
                request: ProviderRequest,
            ) -> Result<ProviderResponse, ProviderError> {
                ProviderClient::send(self, request).await
            }

            async fn stream(
                &self,
                request: ProviderRequest,
            ) -> Result<ProviderStream, ProviderError> {
                ProviderClient::stream(self, request).await
            }

            fn name(&self) -> ProviderName {
                $name
            }
        }
    };
}

provider_client_impl!(OpenAiClient, ProviderName::OpenAi);
provider_client_impl!(AnthropicClient, ProviderName::Anthropic);
provider_client_impl!(GoogleClient, ProviderName::Google);
provider_client_impl!(VertexClient, ProviderName::Vertex);

/// Immutable provider registry keyed by native provider name.
#[derive(Clone, Default)]
pub struct ProviderRegistry {
    inner: HashMap<ProviderName, Arc<dyn Provider>>,
}

impl ProviderRegistry {
    /// Creates an empty provider registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a registry from environment-backed built-in clients.
    ///
    /// Missing provider environment variables skip that provider; callers can
    /// still register explicit clients or mocks afterwards.
    pub fn from_env(default_google_model: impl AsRef<str>) -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(MockProvider::echo()));
        if let Ok(client) = OpenAiClient::from_env() {
            registry.register(Arc::new(client));
        }
        if let Ok(client) = AnthropicClient::from_env() {
            registry.register(Arc::new(client));
        }
        if let Ok(client) = GoogleClient::from_env(default_google_model.as_ref()) {
            registry.register(Arc::new(client));
        }
        if let Ok(client) = VertexClient::from_env(default_google_model.as_ref()) {
            registry.register(Arc::new(client));
        }
        registry
    }

    /// Registers or replaces one provider client.
    pub fn register(&mut self, client: Arc<dyn Provider>) {
        self.inner.insert(client.name(), client);
    }

    /// Returns the registered provider client for `name`.
    pub fn get(&self, name: &ProviderName) -> Option<Arc<dyn Provider>> {
        self.inner.get(name).map(Arc::clone)
    }

    /// Returns the registered provider names.
    pub fn names(&self) -> impl Iterator<Item = &ProviderName> {
        self.inner.keys()
    }

    /// Returns the number of registered providers.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true when no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Return the process default provider registry.
pub fn default_registry() -> Arc<ProviderRegistry> {
    let lock = DEFAULT_REGISTRY
        .get_or_init(|| RwLock::new(Arc::new(ProviderRegistry::from_env("gemini-2.5-flash"))));
    match lock.read() {
        Ok(registry) => registry.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// Refresh the process default provider registry from environment-backed clients.
pub fn refresh_default_registry_from_env() -> Arc<ProviderRegistry> {
    let registry = Arc::new(ProviderRegistry::from_env("gemini-2.5-flash"));
    let lock = DEFAULT_REGISTRY.get_or_init(|| RwLock::new(registry.clone()));
    match lock.write() {
        Ok(mut current) => {
            *current = registry.clone();
        }
        Err(poisoned) => {
            *poisoned.into_inner() = registry.clone();
        }
    }
    registry
}
