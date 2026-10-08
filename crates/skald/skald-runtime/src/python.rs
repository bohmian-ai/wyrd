//! Python boundary for provider registries used by agent tests.

#![cfg(feature = "python")]

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_spec::{ProviderName, ProviderResponse};

use crate::{MockProvider, ProviderRegistry};

/// Python-visible provider registry handle.
#[pyclass(
    module = "wyrd._wyrd.providers",
    name = "_ProviderRegistryInner",
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyProviderRegistryInner {
    /// Shared provider registry.
    pub inner: Arc<ProviderRegistry>,
}

#[pymethods]
impl PyProviderRegistryInner {
    /// Build an empty provider registry.
    #[new]
    pub fn __new__() -> Self {
        Self {
            inner: Arc::new(ProviderRegistry::new()),
        }
    }

    /// Return registered provider names.
    pub fn names(&self) -> Vec<String> {
        self.inner.names().map(provider_name_label).collect()
    }
}

/// Offline, deterministic `mock` provider with caller-set canned responses.
///
/// Prompts that declare `provider="mock"` dispatch here when an Agent is built
/// with `mock_provider=`. Each call returns the next canned response text in
/// order as an assistant message; once the queue is exhausted the provider
/// echoes the last user message. No network, credentials, or randomness are
/// involved. Clones share one queue, so a provider passed to several Agents
/// is consumed across all of their runs.
#[pyclass(module = "wyrd.agent", name = "MockProvider", skip_from_py_object)]
#[derive(Clone)]
pub struct PyMockProvider {
    /// Shared native mock the runtime dispatches to.
    inner: MockProvider,
}

impl PyMockProvider {
    /// Build a registry whose only provider is this mock.
    ///
    /// Agents dispatch `provider="mock"` prompts through the returned registry
    /// instead of the process default, which keeps the run offline.
    #[must_use]
    pub fn registry(&self) -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(self.inner.clone()));
        registry
    }
}

#[pymethods]
impl PyMockProvider {
    /// Create a mock provider that returns `responses` in order, then echoes.
    #[new]
    #[pyo3(signature = (responses=None))]
    fn __new__(responses: Option<Vec<String>>) -> Self {
        let mock = Self {
            inner: MockProvider::echo(),
        };
        for text in responses.unwrap_or_default() {
            mock.push(&text);
        }
        mock
    }

    /// Queue one more canned assistant response.
    fn push(&self, text: &str) {
        self.inner.push_response(ProviderResponse::text(text));
    }

    /// Return how many canned responses are still queued.
    #[getter]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}

/// Refresh the process default provider registry from environment variables.
#[pyfunction]
pub fn _init_default_providers() {
    crate::refresh_default_registry_from_env();
}

/// Register provider Python classes.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyProviderRegistryInner>()?;
    module.add_function(wrap_pyfunction!(_init_default_providers, module)?)?;
    Ok(())
}

fn provider_name_label(name: &ProviderName) -> String {
    match name {
        ProviderName::OpenAi => "openai".to_owned(),
        ProviderName::Anthropic => "anthropic".to_owned(),
        ProviderName::Google => "google".to_owned(),
        ProviderName::Vertex => "vertex".to_owned(),
        ProviderName::Custom(value) => value.clone(),
    }
}
