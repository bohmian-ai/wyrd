//! Shared private registry engine state.

use std::sync::Arc;

use wyrd_client::WyrdClient;
use wyrd_storage_client::WyrdStorageClient;

/// Opaque authenticated context shared by registry client capabilities.
///
/// Cloning this value preserves the same transport, authentication cache, and
/// storage client rather than rebuilding them for each focused handle.
#[derive(Clone)]
pub struct RegistryContext {
    /// Shared private registry engine.
    pub(crate) engine: Arc<RegistryEngine>,
}

impl RegistryContext {
    /// Wrap one shared registry engine for transfer between focused handles.
    pub(crate) fn new(engine: Arc<RegistryEngine>) -> Self {
        Self { engine }
    }
}

/// Shared immutable state behind cloned [`crate::Cards`] handles.
pub(crate) struct RegistryEngine {
    /// Authenticated control-plane client.
    pub(crate) client: WyrdClient,
    /// Storage client sharing the control-plane transport and auth stack.
    pub(crate) storage: WyrdStorageClient,
}

impl RegistryEngine {
    /// Compose one storage client over one already-assembled Wyrd client.
    pub(crate) fn new(client: WyrdClient) -> Arc<Self> {
        let storage = WyrdStorageClient::new(&client);
        Arc::new(Self { client, storage })
    }
}
