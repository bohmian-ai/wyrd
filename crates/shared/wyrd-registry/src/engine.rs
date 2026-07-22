//! Shared private registry engine state.

use std::sync::Arc;

use wyrd_client::WyrdClient;
use wyrd_storage_client::WyrdStorageClient;

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
